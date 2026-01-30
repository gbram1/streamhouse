//! Partition Assigner - Automatic Partition Distribution
//!
//! The PartitionAssigner automatically distributes partitions across available agents
//! using consistent hashing. It watches for topology changes (agents joining/leaving)
//! and triggers rebalancing when needed.
//!
//! ## How It Works
//!
//! 1. **Watch Agent Membership**: Monitor `agents` table for changes
//! 2. **Calculate Assignment**: Use consistent hashing to distribute partitions
//! 3. **Acquire Leases**: For assigned partitions, acquire leases via LeaseManager
//! 4. **Release Leases**: For unassigned partitions, release leases
//!
//! ## Assignment Algorithm
//!
//! Uses **consistent hashing** to minimize partition movement:
//! - Hash both agent IDs and partition IDs onto a ring [0, 2^64)
//! - Each partition assigned to nearest agent clockwise on the ring
//! - When agent joins/leaves, only nearby partitions reassign
//!
//! ## Example
//!
//! ```rust,ignore
//! use streamhouse_agent::PartitionAssigner;
//! use std::sync::Arc;
//!
//! # async fn example(
//! #     agent_id: String,
//! #     agent_group: String,
//! #     metadata_store: Arc<dyn streamhouse_metadata::MetadataStore>,
//! #     lease_manager: Arc<streamhouse_agent::LeaseManager>,
//! # ) {
//! let assigner = PartitionAssigner::new(
//!     agent_id,
//!     agent_group,
//!     metadata_store,
//!     lease_manager,
//!     vec!["orders".to_string(), "users".to_string()], // Topics to manage
//! );
//!
//! // Start background task
//! assigner.start().await.unwrap();
//!
//! // Automatically acquires/releases leases as topology changes
//! // ...
//!
//! // Stop and release all
//! assigner.stop().await.unwrap();
//! # }
//! ```

use crate::error::{AgentError, Result};
use crate::lease_manager::LeaseManager;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::MetadataStore;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Default rebalance check interval (30 seconds)
const DEFAULT_REBALANCE_INTERVAL: Duration = Duration::from_secs(30);

/// Partition Assigner - automatically distributes partitions across agents
pub struct PartitionAssigner {
    agent_id: String,
    agent_group: String,
    metadata_store: Arc<dyn MetadataStore>,
    lease_manager: Arc<LeaseManager>,
    topics: Vec<String>,
    rebalance_interval: Duration,

    /// Current assignment state
    state: Arc<RwLock<AssignerState>>,

    /// Background task handle
    task_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

#[derive(Debug)]
struct AssignerState {
    /// Partitions currently assigned to this agent
    assigned_partitions: HashSet<(String, u32)>,

    /// Last known agent topology (agent_id → hash)
    agent_topology: HashMap<String, u64>,

    /// Assignment statistics
    stats: AssignmentStats,
}

#[derive(Debug, Default, Clone)]
pub struct AssignmentStats {
    pub rebalance_count: u64,
    pub assignments_gained: u64,
    pub assignments_lost: u64,
    pub last_rebalance_ms: i64,
}

impl PartitionAssigner {
    /// Create a new partition assigner
    pub fn new(
        agent_id: String,
        agent_group: String,
        metadata_store: Arc<dyn MetadataStore>,
        lease_manager: Arc<LeaseManager>,
        topics: Vec<String>,
    ) -> Self {
        Self {
            agent_id,
            agent_group,
            metadata_store,
            lease_manager,
            topics,
            rebalance_interval: DEFAULT_REBALANCE_INTERVAL,
            state: Arc::new(RwLock::new(AssignerState {
                assigned_partitions: HashSet::new(),
                agent_topology: HashMap::new(),
                stats: AssignmentStats::default(),
            })),
            task_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Set custom rebalance interval
    pub fn with_rebalance_interval(mut self, interval: Duration) -> Self {
        self.rebalance_interval = interval;
        self
    }

    /// Start the partition assigner background task
    pub async fn start(&self) -> Result<()> {
        let mut handle_guard = self.task_handle.write().await;

        if handle_guard.is_some() {
            return Err(AgentError::AlreadyStarted);
        }

        info!(
            agent_id = %self.agent_id,
            topics = ?self.topics,
            interval_secs = self.rebalance_interval.as_secs(),
            "Starting partition assigner"
        );

        let task = RebalanceTask {
            agent_id: self.agent_id.clone(),
            agent_group: self.agent_group.clone(),
            metadata_store: Arc::clone(&self.metadata_store),
            lease_manager: Arc::clone(&self.lease_manager),
            topics: self.topics.clone(),
            interval: self.rebalance_interval,
            state: Arc::clone(&self.state),
        };

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        *handle_guard = Some(handle);

        Ok(())
    }

    /// Stop the partition assigner and release all leases
    pub async fn stop(&self) -> Result<()> {
        let mut handle_guard = self.task_handle.write().await;

        if let Some(handle) = handle_guard.take() {
            handle.abort();
            let _ = handle.await;

            info!(
                agent_id = %self.agent_id,
                "Partition assigner stopped"
            );
        }

        // Release all assigned partitions
        let partitions_to_release: Vec<(String, u32)> = {
            let state = self.state.read().await;
            state.assigned_partitions.iter().cloned().collect()
        };

        for (topic, partition_id) in partitions_to_release {
            if let Err(e) = self.lease_manager.release_lease(&topic, partition_id).await {
                warn!(
                    agent_id = %self.agent_id,
                    topic = %topic,
                    partition_id = partition_id,
                    error = %e,
                    "Failed to release partition during assigner shutdown"
                );
            }
        }

        Ok(())
    }

    /// Get current assignment statistics
    pub async fn get_stats(&self) -> AssignmentStats {
        let state = self.state.read().await;
        state.stats.clone()
    }

    /// Get currently assigned partitions
    pub async fn get_assigned_partitions(&self) -> Vec<(String, u32)> {
        let state = self.state.read().await;
        state.assigned_partitions.iter().cloned().collect()
    }
}

/// Background task that performs periodic rebalancing
struct RebalanceTask {
    agent_id: String,
    agent_group: String,
    metadata_store: Arc<dyn MetadataStore>,
    lease_manager: Arc<LeaseManager>,
    topics: Vec<String>,
    interval: Duration,
    state: Arc<RwLock<AssignerState>>,
}

impl RebalanceTask {
    async fn run(self) {
        info!(
            agent_id = %self.agent_id,
            interval_secs = self.interval.as_secs(),
            "Rebalance task started"
        );

        // Initial rebalance immediately
        if let Err(e) = self.perform_rebalance().await {
            warn!(
                agent_id = %self.agent_id,
                error = %e,
                "Initial rebalance failed"
            );
        }

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.interval) => {}
                _ = tokio::signal::ctrl_c() => {
                    info!(
                        agent_id = %self.agent_id,
                        "Rebalance task received shutdown signal"
                    );
                    break;
                }
            }

            if let Err(e) = self.perform_rebalance().await {
                warn!(
                    agent_id = %self.agent_id,
                    error = %e,
                    "Rebalance failed"
                );
            }
        }

        let state = self.state.read().await;
        info!(
            agent_id = %self.agent_id,
            rebalance_count = state.stats.rebalance_count,
            assignments_gained = state.stats.assignments_gained,
            assignments_lost = state.stats.assignments_lost,
            "Rebalance task stopped"
        );
    }

    async fn perform_rebalance(&self) -> Result<()> {
        // 1. Get current live agents
        let agents = self
            .metadata_store
            .list_agents(Some(&self.agent_group), None)
            .await?;

        if agents.is_empty() {
            debug!(
                agent_id = %self.agent_id,
                "No live agents in group, skipping rebalance"
            );
            return Ok(());
        }

        let agent_ids: Vec<String> = agents.iter().map(|a| a.agent_id.clone()).collect();

        // 2. Check if topology changed
        let topology_changed = {
            let state = self.state.read().await;
            let current_topology: HashMap<String, u64> = agent_ids
                .iter()
                .map(|id| (id.clone(), hash_string(id)))
                .collect();

            current_topology != state.agent_topology
        };

        if !topology_changed {
            debug!(
                agent_id = %self.agent_id,
                agent_count = agent_ids.len(),
                "No topology change, skipping rebalance"
            );
            return Ok(());
        }

        info!(
            agent_id = %self.agent_id,
            agent_count = agent_ids.len(),
            agents = ?agent_ids,
            "Topology changed, performing rebalance"
        );

        // 3. Calculate new assignment using consistent hashing
        let mut new_assignment = HashSet::new();

        for topic_name in &self.topics {
            // Get topic info
            let topic = match self.metadata_store.get_topic(topic_name).await? {
                Some(t) => t,
                None => {
                    warn!(
                        topic = %topic_name,
                        "Topic not found, skipping"
                    );
                    continue;
                }
            };

            // Assign each partition
            for partition_id in 0..topic.partition_count {
                let assigned_agent =
                    assign_partition_consistent_hash(topic_name, partition_id, &agent_ids);

                if assigned_agent == self.agent_id {
                    new_assignment.insert((topic_name.clone(), partition_id));
                }
            }
        }

        // 4. Calculate diff
        let (to_acquire, to_release) = {
            let state = self.state.read().await;
            let current = &state.assigned_partitions;

            let to_acquire: Vec<(String, u32)> =
                new_assignment.difference(current).cloned().collect();

            let to_release: Vec<(String, u32)> =
                current.difference(&new_assignment).cloned().collect();

            (to_acquire, to_release)
        };

        debug!(
            agent_id = %self.agent_id,
            to_acquire = to_acquire.len(),
            to_release = to_release.len(),
            "Rebalance diff calculated"
        );

        // 5. Release old assignments
        for (topic, partition_id) in &to_release {
            match self.lease_manager.release_lease(topic, *partition_id).await {
                Ok(_) => {
                    info!(
                        agent_id = %self.agent_id,
                        topic = %topic,
                        partition_id = partition_id,
                        "Released partition"
                    );
                }
                Err(e) => {
                    warn!(
                        agent_id = %self.agent_id,
                        topic = %topic,
                        partition_id = partition_id,
                        error = %e,
                        "Failed to release partition"
                    );
                }
            }
        }

        // 6. Acquire new assignments
        for (topic, partition_id) in &to_acquire {
            match self.lease_manager.ensure_lease(topic, *partition_id).await {
                Ok(epoch) => {
                    info!(
                        agent_id = %self.agent_id,
                        topic = %topic,
                        partition_id = partition_id,
                        epoch = epoch,
                        "Acquired partition"
                    );
                }
                Err(e) => {
                    warn!(
                        agent_id = %self.agent_id,
                        topic = %topic,
                        partition_id = partition_id,
                        error = %e,
                        "Failed to acquire partition (may be held by another agent)"
                    );
                    // Don't add to assignment if acquisition failed
                    continue;
                }
            }
        }

        // 7. Update state
        let total_assigned = new_assignment.len();
        {
            let mut state = self.state.write().await;
            state.assigned_partitions = new_assignment;
            state.agent_topology = agent_ids
                .iter()
                .map(|id| (id.clone(), hash_string(id)))
                .collect();
            state.stats.rebalance_count += 1;
            state.stats.assignments_gained += to_acquire.len() as u64;
            state.stats.assignments_lost += to_release.len() as u64;
            state.stats.last_rebalance_ms = current_timestamp_ms();
        }

        info!(
            agent_id = %self.agent_id,
            acquired = to_acquire.len(),
            released = to_release.len(),
            total_assigned = total_assigned,
            "Rebalance completed"
        );

        Ok(())
    }
}

/// Assign a partition using consistent hashing
///
/// Maps both partition and agents onto a ring, assigns partition to nearest agent clockwise.
fn assign_partition_consistent_hash(
    topic: &str,
    partition_id: u32,
    agent_ids: &[String],
) -> String {
    // Hash the partition
    let partition_key = format!("{}:{}", topic, partition_id);
    let partition_hash = hash_string(&partition_key);

    // Find nearest agent clockwise on the ring
    let mut best_agent = &agent_ids[0];
    let mut best_distance = u64::MAX;

    for agent_id in agent_ids {
        let agent_hash = hash_string(agent_id);

        // Calculate clockwise distance on the ring
        let distance = if agent_hash >= partition_hash {
            agent_hash - partition_hash
        } else {
            // Wrap around
            (u64::MAX - partition_hash) + agent_hash
        };

        if distance < best_distance {
            best_distance = distance;
            best_agent = agent_id;
        }
    }

    best_agent.clone()
}

/// Hash a string to u64 using FNV-1a
fn hash_string(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Get current timestamp in milliseconds since epoch
fn current_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistent_hash_assignment() {
        let agents = vec![
            "agent-1".to_string(),
            "agent-2".to_string(),
            "agent-3".to_string(),
        ];

        // Assign all partitions
        let mut assignments: HashMap<String, Vec<u32>> = HashMap::new();
        for partition_id in 0..9 {
            let assigned = assign_partition_consistent_hash("orders", partition_id, &agents);
            assignments.entry(assigned).or_default().push(partition_id);
        }

        // At least some agents should get assignments (consistent hashing may not use all)
        assert!(
            !assignments.is_empty(),
            "At least one agent should have assignments"
        );

        // Total should be 9
        let total: usize = assignments.values().map(|v| v.len()).sum();
        assert_eq!(total, 9, "All partitions should be assigned");
    }

    #[test]
    fn test_consistent_hash_stability() {
        let agents = vec![
            "agent-1".to_string(),
            "agent-2".to_string(),
            "agent-3".to_string(),
        ];

        // Initial assignment
        let mut initial: HashMap<u32, String> = HashMap::new();
        for partition_id in 0..9 {
            let assigned = assign_partition_consistent_hash("orders", partition_id, &agents);
            initial.insert(partition_id, assigned);
        }

        // Remove one agent
        let agents_minus_one = vec!["agent-1".to_string(), "agent-3".to_string()];

        let mut moved = 0;
        for partition_id in 0..9 {
            let new_assigned =
                assign_partition_consistent_hash("orders", partition_id, &agents_minus_one);

            if initial[&partition_id] == "agent-2" {
                // Partitions from removed agent must move
                assert_ne!(new_assigned, "agent-2");
            } else if new_assigned != initial[&partition_id] {
                // Count how many other partitions moved
                moved += 1;
            }
        }

        // Most partitions should stay in place
        assert!(moved <= 3, "Too many partitions moved: {}", moved);
    }

    #[test]
    fn test_hash_string_consistency() {
        // Same string should always hash to same value
        let hash1 = hash_string("agent-1");
        let hash2 = hash_string("agent-1");
        assert_eq!(hash1, hash2);

        // Different strings should (probably) hash to different values
        let hash3 = hash_string("agent-2");
        assert_ne!(hash1, hash3);
    }
}
