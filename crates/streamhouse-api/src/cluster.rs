//! Cluster Coordination
//!
//! Manages cluster topology, node membership, controller election, and failure detection
//! for distributed StreamHouse deployments.
//!
//! ## Controller Election
//!
//! Uses a simple deterministic algorithm: the node with the smallest `node_id` becomes
//! the controller. This avoids the complexity of consensus protocols while providing
//! predictable leader selection.
//!
//! ## Failure Detection
//!
//! Nodes that have not sent a heartbeat within the configured `failure_timeout` are
//! automatically marked as inactive by `detect_failed_nodes()`.
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::cluster::ClusterCoordinator;
//! use std::time::Duration;
//!
//! let coordinator = ClusterCoordinator::new("node-1", Duration::from_secs(5), Duration::from_secs(30));
//! coordinator.add_node("node-1", "10.0.1.1:9090").await;
//! coordinator.add_node("node-2", "10.0.1.2:9090").await;
//!
//! let controller = coordinator.get_controller().await;
//! assert_eq!(controller, Some("node-1".to_string()));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Status of a node in the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is active and serving requests.
    Active,
    /// Node is inactive (failed or shut down).
    Inactive,
    /// Node is draining partitions before shutdown.
    Draining,
}

/// A node participating in the cluster.
#[derive(Debug, Clone)]
pub struct ClusterNode {
    /// Unique identifier for this node.
    pub node_id: String,
    /// Network address (host:port) for this node.
    pub address: String,
    /// Current status of the node.
    pub status: NodeStatus,
    /// When this node joined the cluster.
    pub joined_at: Instant,
    /// Last time a heartbeat was received from this node.
    pub last_heartbeat: Instant,
    /// Number of partitions assigned to this node.
    pub partition_count: u32,
}

/// The topology of the cluster: all known nodes and the current controller.
#[derive(Debug, Clone)]
pub struct ClusterTopology {
    /// Map of node_id -> ClusterNode for all known nodes.
    pub nodes: HashMap<String, ClusterNode>,
    /// The node_id of the current controller (smallest node_id among active nodes).
    pub controller_id: Option<String>,
}

impl ClusterTopology {
    /// Create an empty topology.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            controller_id: None,
        }
    }

    /// Recompute the controller: the active node with the smallest node_id.
    fn elect_controller(&mut self) {
        self.controller_id = self
            .nodes
            .values()
            .filter(|n| n.status == NodeStatus::Active)
            .map(|n| n.node_id.clone())
            .min();
    }
}

impl Default for ClusterTopology {
    fn default() -> Self {
        Self::new()
    }
}

/// Coordinates cluster membership, controller election, and failure detection.
#[derive(Debug, Clone)]
pub struct ClusterCoordinator {
    /// The shared cluster topology.
    topology: Arc<RwLock<ClusterTopology>>,
    /// The node_id of the local node running this coordinator.
    pub local_node_id: String,
    /// How often heartbeats are expected.
    pub heartbeat_interval: Duration,
    /// How long before a missing heartbeat marks a node as failed.
    pub failure_timeout: Duration,
}

impl ClusterCoordinator {
    /// Create a new ClusterCoordinator.
    pub fn new(
        local_node_id: &str,
        heartbeat_interval: Duration,
        failure_timeout: Duration,
    ) -> Self {
        info!(
            node_id = local_node_id,
            "Creating cluster coordinator"
        );
        Self {
            topology: Arc::new(RwLock::new(ClusterTopology::new())),
            local_node_id: local_node_id.to_string(),
            heartbeat_interval,
            failure_timeout,
        }
    }

    /// Add a node to the cluster topology.
    pub async fn add_node(&self, node_id: &str, address: &str) {
        let mut topo = self.topology.write().await;
        let now = Instant::now();
        let node = ClusterNode {
            node_id: node_id.to_string(),
            address: address.to_string(),
            status: NodeStatus::Active,
            joined_at: now,
            last_heartbeat: now,
            partition_count: 0,
        };
        info!(node_id = node_id, address = address, "Adding node to cluster");
        topo.nodes.insert(node_id.to_string(), node);
        topo.elect_controller();
    }

    /// Remove a node from the cluster topology.
    pub async fn remove_node(&self, node_id: &str) {
        let mut topo = self.topology.write().await;
        info!(node_id = node_id, "Removing node from cluster");
        topo.nodes.remove(node_id);
        topo.elect_controller();
    }

    /// Get a snapshot of the current cluster topology.
    pub async fn get_topology(&self) -> ClusterTopology {
        self.topology.read().await.clone()
    }

    /// Get the current controller node_id.
    pub async fn get_controller(&self) -> Option<String> {
        self.topology.read().await.controller_id.clone()
    }

    /// Mark a node as active and update its heartbeat timestamp.
    pub async fn mark_node_active(&self, node_id: &str) {
        let mut topo = self.topology.write().await;
        if let Some(node) = topo.nodes.get_mut(node_id) {
            debug!(node_id = node_id, "Marking node active");
            node.status = NodeStatus::Active;
            node.last_heartbeat = Instant::now();
            topo.elect_controller();
        }
    }

    /// Mark a node as inactive.
    pub async fn mark_node_inactive(&self, node_id: &str) {
        let mut topo = self.topology.write().await;
        if let Some(node) = topo.nodes.get_mut(node_id) {
            info!(node_id = node_id, "Marking node inactive");
            node.status = NodeStatus::Inactive;
            topo.elect_controller();
        }
    }

    /// Mark a node as draining.
    pub async fn mark_node_draining(&self, node_id: &str) {
        let mut topo = self.topology.write().await;
        if let Some(node) = topo.nodes.get_mut(node_id) {
            info!(node_id = node_id, "Marking node draining");
            node.status = NodeStatus::Draining;
            topo.elect_controller();
        }
    }

    /// Detect nodes that have not heartbeated within the failure_timeout and mark them inactive.
    /// Returns the list of node_ids that were newly marked as failed.
    pub async fn detect_failed_nodes(&self) -> Vec<String> {
        let mut topo = self.topology.write().await;
        let timeout = self.failure_timeout;
        let mut failed = Vec::new();

        for node in topo.nodes.values_mut() {
            if node.status == NodeStatus::Active && node.last_heartbeat.elapsed() > timeout {
                info!(node_id = %node.node_id, "Node failed heartbeat timeout, marking inactive");
                node.status = NodeStatus::Inactive;
                failed.push(node.node_id.clone());
            }
        }

        if !failed.is_empty() {
            topo.elect_controller();
        }

        failed
    }

    /// Record a heartbeat for a node, updating its last_heartbeat timestamp.
    pub async fn record_heartbeat(&self, node_id: &str) {
        let mut topo = self.topology.write().await;
        if let Some(node) = topo.nodes.get_mut(node_id) {
            debug!(node_id = node_id, "Recording heartbeat");
            node.last_heartbeat = Instant::now();
            if node.status == NodeStatus::Inactive {
                node.status = NodeStatus::Active;
                topo.elect_controller();
            }
        }
    }

    /// Get the number of nodes in the cluster.
    pub async fn node_count(&self) -> usize {
        self.topology.read().await.nodes.len()
    }

    /// Get the number of active nodes.
    pub async fn active_node_count(&self) -> usize {
        self.topology
            .read()
            .await
            .nodes
            .values()
            .filter(|n| n.status == NodeStatus::Active)
            .count()
    }

    /// Update the partition count for a node.
    pub async fn set_partition_count(&self, node_id: &str, count: u32) {
        let mut topo = self.topology.write().await;
        if let Some(node) = topo.nodes.get_mut(node_id) {
            node.partition_count = count;
        }
    }

    /// Get a specific node by ID.
    pub async fn get_node(&self, node_id: &str) -> Option<ClusterNode> {
        self.topology.read().await.nodes.get(node_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_coordinator() -> ClusterCoordinator {
        ClusterCoordinator::new(
            "node-1",
            Duration::from_secs(5),
            Duration::from_secs(30),
        )
    }

    #[tokio::test]
    async fn test_new_coordinator() {
        let coord = make_coordinator();
        assert_eq!(coord.local_node_id, "node-1");
        assert_eq!(coord.node_count().await, 0);
    }

    #[tokio::test]
    async fn test_add_node() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        assert_eq!(coord.node_count().await, 1);
    }

    #[tokio::test]
    async fn test_remove_node() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.add_node("node-2", "10.0.1.2:9090").await;
        assert_eq!(coord.node_count().await, 2);
        coord.remove_node("node-1").await;
        assert_eq!(coord.node_count().await, 1);
    }

    #[tokio::test]
    async fn test_controller_election_smallest_id() {
        let coord = make_coordinator();
        coord.add_node("node-3", "10.0.1.3:9090").await;
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.add_node("node-2", "10.0.1.2:9090").await;
        assert_eq!(coord.get_controller().await, Some("node-1".to_string()));
    }

    #[tokio::test]
    async fn test_controller_changes_on_remove() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.add_node("node-2", "10.0.1.2:9090").await;
        assert_eq!(coord.get_controller().await, Some("node-1".to_string()));

        coord.remove_node("node-1").await;
        assert_eq!(coord.get_controller().await, Some("node-2".to_string()));
    }

    #[tokio::test]
    async fn test_controller_changes_on_inactive() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.add_node("node-2", "10.0.1.2:9090").await;

        coord.mark_node_inactive("node-1").await;
        assert_eq!(coord.get_controller().await, Some("node-2".to_string()));
    }

    #[tokio::test]
    async fn test_no_controller_when_all_inactive() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.mark_node_inactive("node-1").await;
        assert_eq!(coord.get_controller().await, None);
    }

    #[tokio::test]
    async fn test_mark_node_active() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.mark_node_inactive("node-1").await;

        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.status, NodeStatus::Inactive);

        coord.mark_node_active("node-1").await;
        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.status, NodeStatus::Active);
    }

    #[tokio::test]
    async fn test_mark_node_draining() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.mark_node_draining("node-1").await;

        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.status, NodeStatus::Draining);
        // Draining nodes should not be controller
        assert_eq!(coord.get_controller().await, None);
    }

    #[tokio::test]
    async fn test_get_topology() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.add_node("node-2", "10.0.1.2:9090").await;

        let topo = coord.get_topology().await;
        assert_eq!(topo.nodes.len(), 2);
        assert_eq!(topo.controller_id, Some("node-1".to_string()));
    }

    #[tokio::test]
    async fn test_record_heartbeat() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;

        // Record heartbeat should update last_heartbeat
        coord.record_heartbeat("node-1").await;
        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.status, NodeStatus::Active);
    }

    #[tokio::test]
    async fn test_heartbeat_revives_inactive_node() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.mark_node_inactive("node-1").await;
        assert_eq!(coord.get_controller().await, None);

        coord.record_heartbeat("node-1").await;
        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.status, NodeStatus::Active);
        assert_eq!(coord.get_controller().await, Some("node-1".to_string()));
    }

    #[tokio::test]
    async fn test_detect_failed_nodes_no_failures() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        // Node was just added, so heartbeat is fresh
        let failed = coord.detect_failed_nodes().await;
        assert!(failed.is_empty());
    }

    #[tokio::test]
    async fn test_detect_failed_nodes_with_timeout() {
        // Use a very short timeout
        let coord = ClusterCoordinator::new(
            "node-1",
            Duration::from_millis(1),
            Duration::from_millis(1),
        );
        coord.add_node("node-1", "10.0.1.1:9090").await;

        // Wait longer than timeout
        tokio::time::sleep(Duration::from_millis(10)).await;

        let failed = coord.detect_failed_nodes().await;
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0], "node-1");

        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.status, NodeStatus::Inactive);
    }

    #[tokio::test]
    async fn test_active_node_count() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.add_node("node-2", "10.0.1.2:9090").await;
        coord.add_node("node-3", "10.0.1.3:9090").await;

        assert_eq!(coord.active_node_count().await, 3);

        coord.mark_node_inactive("node-2").await;
        assert_eq!(coord.active_node_count().await, 2);
    }

    #[tokio::test]
    async fn test_set_partition_count() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        coord.set_partition_count("node-1", 10).await;

        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.partition_count, 10);
    }

    #[tokio::test]
    async fn test_get_nonexistent_node() {
        let coord = make_coordinator();
        assert!(coord.get_node("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_empty_cluster_no_controller() {
        let coord = make_coordinator();
        assert_eq!(coord.get_controller().await, None);
    }

    #[tokio::test]
    async fn test_node_address() {
        let coord = make_coordinator();
        coord.add_node("node-1", "10.0.1.1:9090").await;
        let node = coord.get_node("node-1").await.unwrap();
        assert_eq!(node.address, "10.0.1.1:9090");
    }

    #[tokio::test]
    async fn test_multiple_controller_elections() {
        let coord = make_coordinator();
        coord.add_node("node-5", "10.0.1.5:9090").await;
        assert_eq!(coord.get_controller().await, Some("node-5".to_string()));

        coord.add_node("node-3", "10.0.1.3:9090").await;
        assert_eq!(coord.get_controller().await, Some("node-3".to_string()));

        coord.add_node("node-1", "10.0.1.1:9090").await;
        assert_eq!(coord.get_controller().await, Some("node-1".to_string()));

        coord.mark_node_inactive("node-1").await;
        assert_eq!(coord.get_controller().await, Some("node-3".to_string()));

        coord.mark_node_inactive("node-3").await;
        assert_eq!(coord.get_controller().await, Some("node-5".to_string()));
    }

    #[tokio::test]
    async fn test_topology_default() {
        let topo = ClusterTopology::default();
        assert!(topo.nodes.is_empty());
        assert_eq!(topo.controller_id, None);
    }

    #[tokio::test]
    async fn test_node_status_serialization() {
        let status = NodeStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: NodeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }
}
