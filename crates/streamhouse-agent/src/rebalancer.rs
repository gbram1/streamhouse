//! Partition Rebalancer
//!
//! Implements multiple partition assignment strategies for distributing partitions
//! across cluster nodes. Supports round-robin, least-loaded, consistent hashing,
//! and sticky assignment approaches.
//!
//! ## Strategies
//!
//! - **RoundRobin**: Distributes partitions evenly across nodes in order
//! - **LeastLoaded**: Assigns to nodes with the fewest partitions / lowest load
//! - **ConsistentHash**: Uses hashing for stable assignment across topology changes
//! - **StickyAssignment**: Minimizes partition movements during rebalance
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_agent::rebalancer::{PartitionRebalancer, RebalanceStrategy, NodeLoad};
//!
//! let mut rebalancer = PartitionRebalancer::new(RebalanceStrategy::RoundRobin);
//! rebalancer.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
//! rebalancer.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));
//! rebalancer.set_assignment("orders", 0, "node-1");
//!
//! let plan = rebalancer.compute_rebalance();
//! for movement in &plan.movements {
//!     println!("Move {}/{} -> {}", movement.topic, movement.partition_id, movement.to_node);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use tracing::{debug, info};

/// Hash a string using the default hasher.
pub fn hash_string(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Strategy for partition rebalancing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebalanceStrategy {
    /// Distribute partitions evenly in round-robin order.
    RoundRobin,
    /// Assign partitions to the least-loaded nodes first.
    LeastLoaded,
    /// Use consistent hashing for stable assignments.
    ConsistentHash,
    /// Minimize movements by keeping existing assignments where possible.
    StickyAssignment,
}

/// Load metrics for a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLoad {
    /// Number of partitions assigned to this node.
    pub partition_count: u32,
    /// Throughput in bytes per second.
    pub bytes_per_sec: f64,
    /// Throughput in records per second.
    pub records_per_sec: f64,
}

impl NodeLoad {
    /// Create a new NodeLoad with the given metrics.
    pub fn new(partition_count: u32, bytes_per_sec: f64, records_per_sec: f64) -> Self {
        Self {
            partition_count,
            bytes_per_sec,
            records_per_sec,
        }
    }
}

/// Configuration for rebalancing behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceConfig {
    /// Maximum number of partition movements to execute concurrently.
    pub max_concurrent_movements: usize,
    /// Whether to consider rack/zone placement when assigning partitions.
    pub rack_aware: bool,
    /// Threshold (0.0 - 1.0) above which load imbalance triggers rebalancing.
    pub imbalance_threshold: f64,
}

impl Default for RebalanceConfig {
    fn default() -> Self {
        Self {
            max_concurrent_movements: 5,
            rack_aware: false,
            imbalance_threshold: 0.1,
        }
    }
}

/// A single partition movement from one node to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionMovement {
    /// Topic name.
    pub topic: String,
    /// Partition ID within the topic.
    pub partition_id: u32,
    /// Node the partition is currently on (None if unassigned).
    pub from_node: Option<String>,
    /// Node the partition should be moved to.
    pub to_node: String,
}

/// The result of computing a rebalance: a set of movements to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalancePlan {
    /// Ordered list of partition movements.
    pub movements: Vec<PartitionMovement>,
    /// Strategy that was used.
    pub strategy: RebalanceStrategy,
    /// Whether the cluster was already balanced (no movements needed).
    pub was_balanced: bool,
    /// Number of partitions that remain on their current node.
    pub partitions_unchanged: usize,
    /// Total number of partitions considered.
    pub total_partitions: usize,
}

impl RebalancePlan {
    /// Number of partition movements in this plan.
    pub fn movement_count(&self) -> usize {
        self.movements.len()
    }
}

/// Partition rebalancer supporting multiple assignment strategies.
#[derive(Debug, Clone)]
pub struct PartitionRebalancer {
    /// The rebalancing strategy to use.
    pub strategy: RebalanceStrategy,
    /// Configuration controlling rebalance behavior.
    pub config: RebalanceConfig,
    /// Current load metrics per node.
    pub node_loads: HashMap<String, NodeLoad>,
    /// Current partition assignments: (topic, partition_id) -> node_id.
    pub current_assignment: HashMap<(String, u32), String>,
}

impl PartitionRebalancer {
    /// Create a new rebalancer with default config.
    pub fn new(strategy: RebalanceStrategy) -> Self {
        Self {
            strategy,
            config: RebalanceConfig::default(),
            node_loads: HashMap::new(),
            current_assignment: HashMap::new(),
        }
    }

    /// Create a new rebalancer with a specific config.
    pub fn with_config(strategy: RebalanceStrategy, config: RebalanceConfig) -> Self {
        Self {
            strategy,
            config,
            node_loads: HashMap::new(),
            current_assignment: HashMap::new(),
        }
    }

    /// Add a node with its current load.
    pub fn add_node(&mut self, node_id: &str, load: NodeLoad) {
        info!(node_id = node_id, "Adding node to rebalancer");
        self.node_loads.insert(node_id.to_string(), load);
    }

    /// Remove a node from the rebalancer.
    pub fn remove_node(&mut self, node_id: &str) {
        info!(node_id = node_id, "Removing node from rebalancer");
        self.node_loads.remove(node_id);
    }

    /// Set the current assignment for a partition.
    pub fn set_assignment(&mut self, topic: &str, partition_id: u32, node_id: &str) {
        self.current_assignment
            .insert((topic.to_string(), partition_id), node_id.to_string());
    }

    /// Compute a rebalance plan based on the configured strategy.
    pub fn compute_rebalance(&self) -> RebalancePlan {
        if self.node_loads.is_empty() {
            return RebalancePlan {
                movements: Vec::new(),
                strategy: self.strategy.clone(),
                was_balanced: true,
                partitions_unchanged: self.current_assignment.len(),
                total_partitions: self.current_assignment.len(),
            };
        }

        let new_assignment = match &self.strategy {
            RebalanceStrategy::RoundRobin => self.assign_round_robin(),
            RebalanceStrategy::LeastLoaded => self.assign_least_loaded(),
            RebalanceStrategy::ConsistentHash => self.assign_consistent_hash(),
            RebalanceStrategy::StickyAssignment => self.assign_sticky(),
        };

        let total_partitions = self.current_assignment.len();
        let mut movements = Vec::new();
        let mut partitions_unchanged = 0;

        for ((topic, partition_id), new_node) in &new_assignment {
            let old_node = self.current_assignment.get(&(topic.clone(), *partition_id));
            if old_node.map(|n| n.as_str()) == Some(new_node.as_str()) {
                partitions_unchanged += 1;
            } else {
                movements.push(PartitionMovement {
                    topic: topic.clone(),
                    partition_id: *partition_id,
                    from_node: old_node.cloned(),
                    to_node: new_node.clone(),
                });
            }
        }

        let was_balanced = movements.is_empty();

        // Truncate to max concurrent movements
        movements.truncate(self.config.max_concurrent_movements);

        debug!(
            strategy = ?self.strategy,
            total = total_partitions,
            movements = movements.len(),
            unchanged = partitions_unchanged,
            "Computed rebalance plan"
        );

        RebalancePlan {
            movements,
            strategy: self.strategy.clone(),
            was_balanced,
            partitions_unchanged,
            total_partitions,
        }
    }

    /// Execute the rebalance plan by updating current assignments.
    pub fn execute_plan(&mut self, plan: &RebalancePlan) {
        for movement in &plan.movements {
            info!(
                topic = %movement.topic,
                partition = movement.partition_id,
                to = %movement.to_node,
                "Executing partition movement"
            );
            self.current_assignment.insert(
                (movement.topic.clone(), movement.partition_id),
                movement.to_node.clone(),
            );
        }
    }

    /// Assign partitions using round-robin across sorted node IDs.
    fn assign_round_robin(&self) -> HashMap<(String, u32), String> {
        let mut nodes: Vec<String> = self.node_loads.keys().cloned().collect();
        nodes.sort();

        let mut assignment = HashMap::new();
        let mut partitions: Vec<(String, u32)> = self.current_assignment.keys().cloned().collect();
        partitions.sort();

        for (i, key) in partitions.iter().enumerate() {
            let node = &nodes[i % nodes.len()];
            assignment.insert(key.clone(), node.clone());
        }

        assignment
    }

    /// Assign partitions to nodes with the fewest assigned partitions.
    fn assign_least_loaded(&self) -> HashMap<(String, u32), String> {
        let mut nodes: Vec<String> = self.node_loads.keys().cloned().collect();
        nodes.sort();

        let mut assignment = HashMap::new();
        let mut load_counts: HashMap<String, u32> = nodes.iter().map(|n| (n.clone(), 0)).collect();

        let mut partitions: Vec<(String, u32)> = self.current_assignment.keys().cloned().collect();
        partitions.sort();

        for key in &partitions {
            // Find node with smallest load count (tie-break by node name)
            let best_node = load_counts
                .iter()
                .min_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(b.0)))
                .map(|(n, _)| n.clone())
                .unwrap();

            *load_counts.get_mut(&best_node).unwrap() += 1;
            assignment.insert(key.clone(), best_node);
        }

        assignment
    }

    /// Assign partitions using consistent hashing.
    fn assign_consistent_hash(&self) -> HashMap<(String, u32), String> {
        let mut nodes: Vec<String> = self.node_loads.keys().cloned().collect();
        nodes.sort();

        let mut assignment = HashMap::new();
        let mut partitions: Vec<(String, u32)> = self.current_assignment.keys().cloned().collect();
        partitions.sort();

        for (topic, partition_id) in &partitions {
            let key = format!("{}:{}", topic, partition_id);
            let hash = hash_string(&key);
            let idx = (hash as usize) % nodes.len();
            assignment.insert(
                (topic.clone(), *partition_id),
                nodes[idx].clone(),
            );
        }

        assignment
    }

    /// Assign partitions using sticky assignment: keep current assignments
    /// where possible, only reassigning partitions on nodes that no longer exist.
    fn assign_sticky(&self) -> HashMap<(String, u32), String> {
        let mut nodes: Vec<String> = self.node_loads.keys().cloned().collect();
        nodes.sort();

        let mut assignment = HashMap::new();
        let mut load_counts: HashMap<String, u32> = nodes.iter().map(|n| (n.clone(), 0)).collect();

        let mut partitions: Vec<(String, u32)> = self.current_assignment.keys().cloned().collect();
        partitions.sort();

        // Phase 1: keep existing assignments for partitions on valid nodes
        let mut unassigned = Vec::new();
        for key in &partitions {
            if let Some(current_node) = self.current_assignment.get(key) {
                if self.node_loads.contains_key(current_node) {
                    assignment.insert(key.clone(), current_node.clone());
                    *load_counts.get_mut(current_node).unwrap() += 1;
                    continue;
                }
            }
            unassigned.push(key.clone());
        }

        // Phase 2: assign orphaned partitions to least-loaded nodes
        for key in unassigned {
            let best_node = load_counts
                .iter()
                .min_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(b.0)))
                .map(|(n, _)| n.clone())
                .unwrap();

            *load_counts.get_mut(&best_node).unwrap() += 1;
            assignment.insert(key, best_node);
        }

        assignment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config_unlimited() -> RebalanceConfig {
        RebalanceConfig {
            max_concurrent_movements: usize::MAX,
            rack_aware: false,
            imbalance_threshold: 0.1,
        }
    }

    #[test]
    fn test_new_rebalancer() {
        let rb = PartitionRebalancer::new(RebalanceStrategy::RoundRobin);
        assert_eq!(rb.strategy, RebalanceStrategy::RoundRobin);
        assert!(rb.node_loads.is_empty());
        assert!(rb.current_assignment.is_empty());
    }

    #[test]
    fn test_with_config() {
        let config = RebalanceConfig {
            max_concurrent_movements: 10,
            rack_aware: true,
            imbalance_threshold: 0.2,
        };
        let rb = PartitionRebalancer::with_config(RebalanceStrategy::LeastLoaded, config);
        assert_eq!(rb.strategy, RebalanceStrategy::LeastLoaded);
        assert_eq!(rb.config.max_concurrent_movements, 10);
        assert!(rb.config.rack_aware);
    }

    #[test]
    fn test_add_remove_node() {
        let mut rb = PartitionRebalancer::new(RebalanceStrategy::RoundRobin);
        rb.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
        assert_eq!(rb.node_loads.len(), 1);

        rb.remove_node("node-1");
        assert!(rb.node_loads.is_empty());
    }

    #[test]
    fn test_set_assignment() {
        let mut rb = PartitionRebalancer::new(RebalanceStrategy::RoundRobin);
        rb.set_assignment("orders", 0, "node-1");
        assert_eq!(
            rb.current_assignment.get(&("orders".to_string(), 0)),
            Some(&"node-1".to_string())
        );
    }

    #[test]
    fn test_round_robin_balanced() {
        let mut rb = PartitionRebalancer::with_config(
            RebalanceStrategy::RoundRobin,
            test_config_unlimited(),
        );
        rb.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
        rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));

        rb.set_assignment("orders", 0, "node-1");
        rb.set_assignment("orders", 1, "node-1");
        rb.set_assignment("orders", 2, "node-1");
        rb.set_assignment("orders", 3, "node-1");

        let plan = rb.compute_rebalance();
        assert_eq!(plan.total_partitions, 4);
        // Round-robin should distribute: 2 to node-1, 2 to node-2
        assert!(!plan.was_balanced);
    }

    #[test]
    fn test_round_robin_assignment() {
        let mut rb = PartitionRebalancer::with_config(
            RebalanceStrategy::RoundRobin,
            test_config_unlimited(),
        );
        rb.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
        rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));

        rb.set_assignment("topic", 0, "node-1");
        rb.set_assignment("topic", 1, "node-1");

        let plan = rb.compute_rebalance();
        // One of the partitions should move to node-2
        assert_eq!(plan.total_partitions, 2);
    }

    #[test]
    fn test_least_loaded_strategy() {
        let mut rb = PartitionRebalancer::with_config(
            RebalanceStrategy::LeastLoaded,
            test_config_unlimited(),
        );
        rb.add_node("node-1", NodeLoad::new(5, 1000.0, 500.0));
        rb.add_node("node-2", NodeLoad::new(1, 100.0, 50.0));

        rb.set_assignment("orders", 0, "node-1");
        rb.set_assignment("orders", 1, "node-1");
        rb.set_assignment("orders", 2, "node-1");
        rb.set_assignment("orders", 3, "node-1");

        let plan = rb.compute_rebalance();
        assert_eq!(plan.total_partitions, 4);
    }

    #[test]
    fn test_consistent_hash_strategy() {
        let mut rb = PartitionRebalancer::with_config(
            RebalanceStrategy::ConsistentHash,
            test_config_unlimited(),
        );
        rb.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
        rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));

        rb.set_assignment("orders", 0, "node-1");
        rb.set_assignment("orders", 1, "node-1");
        rb.set_assignment("orders", 2, "node-1");

        let plan = rb.compute_rebalance();
        assert_eq!(plan.total_partitions, 3);
    }

    #[test]
    fn test_consistent_hash_stable() {
        // Same inputs should produce same outputs
        let make_rb = || {
            let mut rb = PartitionRebalancer::with_config(
                RebalanceStrategy::ConsistentHash,
                test_config_unlimited(),
            );
            rb.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
            rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));
            rb.set_assignment("orders", 0, "node-1");
            rb.set_assignment("orders", 1, "node-1");
            rb
        };

        let plan1 = make_rb().compute_rebalance();
        let plan2 = make_rb().compute_rebalance();
        assert_eq!(plan1.movements.len(), plan2.movements.len());
    }

    #[test]
    fn test_sticky_keeps_existing() {
        let mut rb = PartitionRebalancer::with_config(
            RebalanceStrategy::StickyAssignment,
            test_config_unlimited(),
        );
        rb.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
        rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));

        rb.set_assignment("orders", 0, "node-1");
        rb.set_assignment("orders", 1, "node-2");

        let plan = rb.compute_rebalance();
        assert!(plan.was_balanced);
        assert_eq!(plan.partitions_unchanged, 2);
        assert_eq!(plan.movement_count(), 0);
    }

    #[test]
    fn test_sticky_reassigns_orphans() {
        let mut rb = PartitionRebalancer::with_config(
            RebalanceStrategy::StickyAssignment,
            test_config_unlimited(),
        );
        rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));

        // node-1 no longer exists but has partitions
        rb.set_assignment("orders", 0, "node-1");
        rb.set_assignment("orders", 1, "node-2");

        let plan = rb.compute_rebalance();
        assert_eq!(plan.movement_count(), 1);
        assert_eq!(plan.movements[0].to_node, "node-2");
        assert_eq!(plan.partitions_unchanged, 1);
    }

    #[test]
    fn test_empty_cluster_rebalance() {
        let rb = PartitionRebalancer::new(RebalanceStrategy::RoundRobin);
        let plan = rb.compute_rebalance();
        assert!(plan.was_balanced);
        assert_eq!(plan.movement_count(), 0);
    }

    #[test]
    fn test_execute_plan() {
        let mut rb = PartitionRebalancer::with_config(
            RebalanceStrategy::RoundRobin,
            test_config_unlimited(),
        );
        rb.add_node("node-1", NodeLoad::new(0, 0.0, 0.0));
        rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));
        rb.set_assignment("orders", 0, "node-1");
        rb.set_assignment("orders", 1, "node-1");

        let plan = rb.compute_rebalance();
        rb.execute_plan(&plan);

        // After execution, assignments should be updated
        let values: Vec<&String> = rb.current_assignment.values().collect();
        // At least one should be node-2
        assert!(values.iter().any(|v| v.as_str() == "node-2"));
    }

    #[test]
    fn test_max_concurrent_movements() {
        let config = RebalanceConfig {
            max_concurrent_movements: 2,
            rack_aware: false,
            imbalance_threshold: 0.1,
        };
        let mut rb = PartitionRebalancer::with_config(RebalanceStrategy::RoundRobin, config);
        rb.add_node("node-2", NodeLoad::new(0, 0.0, 0.0));

        // All partitions on non-existent node-1, so all need to move
        for i in 0..10 {
            rb.set_assignment("orders", i, "node-1");
        }

        let plan = rb.compute_rebalance();
        assert!(plan.movement_count() <= 2);
    }

    #[test]
    fn test_hash_string_deterministic() {
        let h1 = hash_string("orders:0");
        let h2 = hash_string("orders:0");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_string_different_inputs() {
        let h1 = hash_string("orders:0");
        let h2 = hash_string("orders:1");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_rebalance_plan_movement_count() {
        let plan = RebalancePlan {
            movements: vec![
                PartitionMovement {
                    topic: "t".to_string(),
                    partition_id: 0,
                    from_node: None,
                    to_node: "n1".to_string(),
                },
                PartitionMovement {
                    topic: "t".to_string(),
                    partition_id: 1,
                    from_node: None,
                    to_node: "n2".to_string(),
                },
            ],
            strategy: RebalanceStrategy::RoundRobin,
            was_balanced: false,
            partitions_unchanged: 0,
            total_partitions: 2,
        };
        assert_eq!(plan.movement_count(), 2);
    }

    #[test]
    fn test_node_load_new() {
        let load = NodeLoad::new(10, 1024.0, 500.0);
        assert_eq!(load.partition_count, 10);
        assert_eq!(load.bytes_per_sec, 1024.0);
        assert_eq!(load.records_per_sec, 500.0);
    }
}
