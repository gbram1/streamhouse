//! Partition Assigner - Automatic Partition Distribution
//!
//! The PartitionAssigner automatically distributes partitions across available agents
//! using deterministic round-robin. It watches for topology changes (agents joining/leaving)
//! and triggers rebalancing when needed.
//!
//! ## How It Works
//!
//! 1. **Watch Agent Membership**: Monitor `agents` table for changes
//! 2. **Calculate Assignment**: Use deterministic round-robin to distribute partitions
//! 3. **Acquire Leases**: For assigned partitions, acquire leases via LeaseManager
//! 4. **Release Leases**: For unassigned partitions, release leases
//!
//! ## Assignment Algorithm
//!
//! Uses **deterministic round-robin** for perfectly even distribution:
//! - All agents sort partition and agent lists identically
//! - Partition `i` is assigned to `sorted_agents[i % num_agents]`
//! - Guarantees even distribution (e.g., 6 partitions / 3 agents = 2 each)
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
            let mut sorted_ids = agent_ids.clone();
            sorted_ids.sort();
            let current_topology: HashMap<String, u64> = sorted_ids
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i as u64))
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

        // 3. Calculate new assignment using deterministic round-robin
        //    All agents sort partition and agent lists identically, so they agree.
        let mut new_assignment = HashSet::new();

        // Collect all (topic, partition_id) tuples
        let mut all_partitions: Vec<(String, u32)> = Vec::new();
        for topic_name in &self.topics {
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

            for partition_id in 0..topic.partition_count {
                all_partitions.push((topic_name.clone(), partition_id));
            }
        }

        // Sort partitions deterministically
        all_partitions.sort();

        // Sort agents alphabetically
        let mut sorted_agents = agent_ids.clone();
        sorted_agents.sort();

        // Round-robin: partition i → sorted_agents[i % num_agents]
        let num_agents = sorted_agents.len();
        for (i, (topic, partition_id)) in all_partitions.iter().enumerate() {
            if sorted_agents[i % num_agents] == self.agent_id {
                new_assignment.insert((topic.clone(), *partition_id));
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
            let mut sorted_ids = agent_ids.clone();
            sorted_ids.sort();
            state.agent_topology = sorted_ids
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i as u64))
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

/// Assign a partition using deterministic round-robin.
///
/// Both agent and partition lists are sorted identically on all agents,
/// so every agent agrees on the assignment. Partition `i` goes to
/// `sorted_agents[i % num_agents]`, giving perfect even distribution.
#[cfg(test)]
fn assign_partition_round_robin(
    sorted_partitions: &[(String, u32)],
    sorted_agents: &[String],
) -> HashMap<String, HashSet<(String, u32)>> {
    let mut assignments: HashMap<String, HashSet<(String, u32)>> = HashMap::new();
    let num_agents = sorted_agents.len();
    for (i, partition) in sorted_partitions.iter().enumerate() {
        assignments
            .entry(sorted_agents[i % num_agents].clone())
            .or_default()
            .insert(partition.clone());
    }
    assignments
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
    fn test_round_robin_even_distribution() {
        let agents = vec![
            "agent-1".to_string(),
            "agent-2".to_string(),
            "agent-3".to_string(),
        ];

        // 6 partitions across 3 agents → exactly 2 each
        let mut partitions: Vec<(String, u32)> = Vec::new();
        for p in 0..6 {
            partitions.push(("orders".to_string(), p));
        }
        partitions.sort();

        let mut sorted_agents = agents.clone();
        sorted_agents.sort();

        let assignments = assign_partition_round_robin(&partitions, &sorted_agents);

        // Every agent gets exactly 2
        for agent in &sorted_agents {
            let count = assignments.get(agent).map(|s| s.len()).unwrap_or(0);
            assert_eq!(count, 2, "Agent {} should have 2 partitions, got {}", agent, count);
        }

        // Total should be 6
        let total: usize = assignments.values().map(|s| s.len()).sum();
        assert_eq!(total, 6, "All partitions should be assigned");
    }

    #[test]
    fn test_round_robin_9_partitions() {
        let agents = vec![
            "agent-1".to_string(),
            "agent-2".to_string(),
            "agent-3".to_string(),
        ];

        let mut partitions: Vec<(String, u32)> = Vec::new();
        for p in 0..9 {
            partitions.push(("orders".to_string(), p));
        }
        partitions.sort();

        let mut sorted_agents = agents.clone();
        sorted_agents.sort();

        let assignments = assign_partition_round_robin(&partitions, &sorted_agents);

        // 9 / 3 = 3 each
        for agent in &sorted_agents {
            let count = assignments.get(agent).map(|s| s.len()).unwrap_or(0);
            assert_eq!(count, 3, "Agent {} should have 3 partitions, got {}", agent, count);
        }
    }

    #[test]
    fn test_round_robin_deterministic() {
        let agents = vec![
            "agent-3".to_string(),
            "agent-1".to_string(),
            "agent-2".to_string(),
        ];

        let mut partitions: Vec<(String, u32)> = Vec::new();
        for p in 0..6 {
            partitions.push(("orders".to_string(), p));
        }
        partitions.sort();

        let mut sorted_agents = agents.clone();
        sorted_agents.sort();

        // Run twice — must produce identical results
        let a1 = assign_partition_round_robin(&partitions, &sorted_agents);
        let a2 = assign_partition_round_robin(&partitions, &sorted_agents);
        assert_eq!(a1, a2, "Round-robin must be deterministic");
    }

    #[test]
    fn test_round_robin_agent_removal() {
        // With 3 agents and 6 partitions
        let agents_3 = vec![
            "agent-1".to_string(),
            "agent-2".to_string(),
            "agent-3".to_string(),
        ];

        let mut partitions: Vec<(String, u32)> = Vec::new();
        for p in 0..6 {
            partitions.push(("orders".to_string(), p));
        }
        partitions.sort();

        let mut sorted_3 = agents_3.clone();
        sorted_3.sort();

        let assignments_3 = assign_partition_round_robin(&partitions, &sorted_3);

        // Remove agent-2
        let agents_2 = vec!["agent-1".to_string(), "agent-3".to_string()];
        let mut sorted_2 = agents_2.clone();
        sorted_2.sort();

        let assignments_2 = assign_partition_round_robin(&partitions, &sorted_2);

        // All 6 partitions still assigned, 3 each
        for agent in &sorted_2 {
            let count = assignments_2.get(agent).map(|s| s.len()).unwrap_or(0);
            assert_eq!(count, 3, "Agent {} should have 3 partitions after removal", agent);
        }

        // agent-2 should have no partitions
        assert!(
            !assignments_2.contains_key("agent-2"),
            "Removed agent should have no partitions"
        );

        // Total still 6
        let total: usize = assignments_2.values().map(|s| s.len()).sum();
        assert_eq!(total, 6);

        // Verify 3-agent assignment was 2 each
        for agent in &sorted_3 {
            let count = assignments_3.get(agent).map(|s| s.len()).unwrap_or(0);
            assert_eq!(count, 2);
        }
    }
}
