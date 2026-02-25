//! Cluster Health Monitor
//!
//! Tracks the health of all nodes in a StreamHouse cluster, recording heartbeats,
//! latency measurements, and error counts. Provides an aggregate cluster health
//! status: Healthy, Degraded, or Critical.
//!
//! ## Health Levels
//!
//! - **Healthy**: All nodes are responsive with acceptable error rates
//! - **Degraded**: Some nodes have elevated error counts above the threshold
//! - **Critical**: More than 50% of nodes are unhealthy or there are leadership gaps
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::health_monitor::{ClusterHealthMonitor, HealthThresholds};
//!
//! let monitor = ClusterHealthMonitor::new(HealthThresholds::default());
//! monitor.record_heartbeat("node-1").await;
//! monitor.record_latency("node-1", 5.0).await;
//!
//! let status = monitor.cluster_status().await;
//! println!("Cluster health: {:?}", status);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Health status of a single node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeHealthStatus {
    /// Node is healthy and responsive.
    Healthy,
    /// Node is unhealthy (too many errors or unresponsive).
    Unhealthy,
    /// Node health is unknown (no data yet).
    Unknown,
}

/// Health information for a single node.
#[derive(Debug, Clone)]
pub struct NodeHealth {
    /// Node identifier.
    pub node_id: String,
    /// Current health status.
    pub status: NodeHealthStatus,
    /// When the last health check was performed.
    pub last_check: Instant,
    /// Average latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Number of partitions this node is leader for.
    pub partition_leadership_count: u32,
    /// Number of errors recorded for this node.
    pub error_count: u64,
}

/// Aggregate health status of the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterHealth {
    /// All nodes are healthy.
    Healthy,
    /// Some nodes have issues but the cluster is functional.
    Degraded,
    /// The cluster is in a critical state (>50% unhealthy or leadership gaps).
    Critical,
}

/// Thresholds for determining cluster health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthThresholds {
    /// Error count above which a node is considered unhealthy.
    pub error_count_threshold: u64,
    /// Latency (ms) above which a node is considered unhealthy.
    pub latency_threshold_ms: f64,
    /// Expected total partition leadership count across the cluster.
    pub expected_leadership_count: u32,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            error_count_threshold: 10,
            latency_threshold_ms: 1000.0,
            expected_leadership_count: 0,
        }
    }
}

/// Monitors the health of all nodes in the cluster.
#[derive(Debug, Clone)]
pub struct ClusterHealthMonitor {
    /// Per-node health data.
    node_health: Arc<RwLock<HashMap<String, NodeHealth>>>,
    /// Thresholds for health determination.
    pub thresholds: HealthThresholds,
}

impl ClusterHealthMonitor {
    /// Create a new health monitor with the given thresholds.
    pub fn new(thresholds: HealthThresholds) -> Self {
        info!("Creating cluster health monitor");
        Self {
            node_health: Arc::new(RwLock::new(HashMap::new())),
            thresholds,
        }
    }

    /// Record a heartbeat from a node. Creates the node entry if it doesn't exist.
    pub async fn record_heartbeat(&self, node_id: &str) {
        let mut health = self.node_health.write().await;
        let entry = health.entry(node_id.to_string()).or_insert_with(|| {
            debug!(node_id = node_id, "Creating new node health entry");
            NodeHealth {
                node_id: node_id.to_string(),
                status: NodeHealthStatus::Healthy,
                last_check: Instant::now(),
                avg_latency_ms: 0.0,
                partition_leadership_count: 0,
                error_count: 0,
            }
        });
        entry.last_check = Instant::now();
        entry.status = if entry.error_count > self.thresholds.error_count_threshold {
            NodeHealthStatus::Unhealthy
        } else {
            NodeHealthStatus::Healthy
        };
    }

    /// Record a latency measurement for a node.
    pub async fn record_latency(&self, node_id: &str, latency_ms: f64) {
        let mut health = self.node_health.write().await;
        if let Some(entry) = health.get_mut(node_id) {
            // Exponential moving average
            entry.avg_latency_ms = entry.avg_latency_ms * 0.7 + latency_ms * 0.3;
            debug!(
                node_id = node_id,
                latency_ms = latency_ms,
                avg = entry.avg_latency_ms,
                "Recorded latency"
            );
        }
    }

    /// Record an error for a node. Increments the error counter.
    pub async fn record_error(&self, node_id: &str) {
        let mut health = self.node_health.write().await;
        if let Some(entry) = health.get_mut(node_id) {
            entry.error_count += 1;
            if entry.error_count > self.thresholds.error_count_threshold {
                entry.status = NodeHealthStatus::Unhealthy;
            }
            debug!(
                node_id = node_id,
                error_count = entry.error_count,
                "Recorded error"
            );
        }
    }

    /// Set the partition leadership count for a node.
    pub async fn set_leadership_count(&self, node_id: &str, count: u32) {
        let mut health = self.node_health.write().await;
        if let Some(entry) = health.get_mut(node_id) {
            entry.partition_leadership_count = count;
        }
    }

    /// Check the health of a specific node.
    pub async fn check_health(&self, node_id: &str) -> Option<NodeHealthStatus> {
        let health = self.node_health.read().await;
        health.get(node_id).map(|entry| entry.status.clone())
    }

    /// Get full health info for a specific node.
    pub async fn get_node_health(&self, node_id: &str) -> Option<NodeHealth> {
        let health = self.node_health.read().await;
        health.get(node_id).cloned()
    }

    /// Compute the aggregate cluster health status.
    pub async fn cluster_status(&self) -> ClusterHealth {
        let health = self.node_health.read().await;

        if health.is_empty() {
            return ClusterHealth::Healthy;
        }

        let total_nodes = health.len();
        let unhealthy_count = health
            .values()
            .filter(|n| n.status == NodeHealthStatus::Unhealthy)
            .count();

        // Critical: >50% nodes unhealthy
        if unhealthy_count * 2 > total_nodes {
            info!(
                unhealthy = unhealthy_count,
                total = total_nodes,
                "Cluster is CRITICAL"
            );
            return ClusterHealth::Critical;
        }

        // Check for leadership gaps if expected count is configured
        if self.thresholds.expected_leadership_count > 0 {
            let actual_leadership: u32 = health
                .values()
                .map(|n| n.partition_leadership_count)
                .sum();
            if actual_leadership < self.thresholds.expected_leadership_count {
                info!(
                    actual = actual_leadership,
                    expected = self.thresholds.expected_leadership_count,
                    "Cluster is CRITICAL: leadership gap detected"
                );
                return ClusterHealth::Critical;
            }
        }

        // Degraded: any node has error_count above threshold
        let any_degraded = health
            .values()
            .any(|n| n.error_count > self.thresholds.error_count_threshold);

        if any_degraded {
            info!("Cluster is DEGRADED");
            return ClusterHealth::Degraded;
        }

        ClusterHealth::Healthy
    }

    /// Get the number of monitored nodes.
    pub async fn node_count(&self) -> usize {
        self.node_health.read().await.len()
    }

    /// Get all node health entries.
    pub async fn all_node_health(&self) -> Vec<NodeHealth> {
        self.node_health.read().await.values().cloned().collect()
    }

    /// Reset error count for a node.
    pub async fn reset_errors(&self, node_id: &str) {
        let mut health = self.node_health.write().await;
        if let Some(entry) = health.get_mut(node_id) {
            entry.error_count = 0;
            entry.status = NodeHealthStatus::Healthy;
        }
    }

    /// Remove a node from monitoring.
    pub async fn remove_node(&self, node_id: &str) {
        let mut health = self.node_health.write().await;
        health.remove(node_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_monitor() -> ClusterHealthMonitor {
        ClusterHealthMonitor::new(HealthThresholds::default())
    }

    #[tokio::test]
    async fn test_new_monitor() {
        let monitor = make_monitor();
        assert_eq!(monitor.node_count().await, 0);
    }

    #[tokio::test]
    async fn test_record_heartbeat_creates_entry() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        assert_eq!(monitor.node_count().await, 1);
    }

    #[tokio::test]
    async fn test_record_heartbeat_updates_status() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        let status = monitor.check_health("node-1").await;
        assert_eq!(status, Some(NodeHealthStatus::Healthy));
    }

    #[tokio::test]
    async fn test_record_latency() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        monitor.record_latency("node-1", 10.0).await;

        let health = monitor.get_node_health("node-1").await.unwrap();
        assert!(health.avg_latency_ms > 0.0);
    }

    #[tokio::test]
    async fn test_record_error() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        monitor.record_error("node-1").await;

        let health = monitor.get_node_health("node-1").await.unwrap();
        assert_eq!(health.error_count, 1);
    }

    #[tokio::test]
    async fn test_error_threshold_marks_unhealthy() {
        let monitor = ClusterHealthMonitor::new(HealthThresholds {
            error_count_threshold: 2,
            ..Default::default()
        });
        monitor.record_heartbeat("node-1").await;

        // Record errors up to threshold
        monitor.record_error("node-1").await;
        monitor.record_error("node-1").await;
        let status = monitor.check_health("node-1").await;
        assert_eq!(status, Some(NodeHealthStatus::Healthy));

        // Exceed threshold
        monitor.record_error("node-1").await;
        let status = monitor.check_health("node-1").await;
        assert_eq!(status, Some(NodeHealthStatus::Unhealthy));
    }

    #[tokio::test]
    async fn test_cluster_status_healthy() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        monitor.record_heartbeat("node-2").await;

        let status = monitor.cluster_status().await;
        assert_eq!(status, ClusterHealth::Healthy);
    }

    #[tokio::test]
    async fn test_cluster_status_degraded() {
        let monitor = ClusterHealthMonitor::new(HealthThresholds {
            error_count_threshold: 1,
            ..Default::default()
        });
        monitor.record_heartbeat("node-1").await;
        monitor.record_heartbeat("node-2").await;
        monitor.record_heartbeat("node-3").await;

        // Exceed threshold on one node
        monitor.record_error("node-1").await;
        monitor.record_error("node-1").await;

        let status = monitor.cluster_status().await;
        assert_eq!(status, ClusterHealth::Degraded);
    }

    #[tokio::test]
    async fn test_cluster_status_critical_majority_unhealthy() {
        let monitor = ClusterHealthMonitor::new(HealthThresholds {
            error_count_threshold: 0,
            ..Default::default()
        });
        monitor.record_heartbeat("node-1").await;
        monitor.record_heartbeat("node-2").await;
        monitor.record_heartbeat("node-3").await;

        // Make majority unhealthy (>50%)
        monitor.record_error("node-1").await;
        monitor.record_error("node-2").await;

        let status = monitor.cluster_status().await;
        assert_eq!(status, ClusterHealth::Critical);
    }

    #[tokio::test]
    async fn test_cluster_status_critical_leadership_gap() {
        let monitor = ClusterHealthMonitor::new(HealthThresholds {
            expected_leadership_count: 10,
            ..Default::default()
        });
        monitor.record_heartbeat("node-1").await;
        monitor.set_leadership_count("node-1", 3).await;

        let status = monitor.cluster_status().await;
        assert_eq!(status, ClusterHealth::Critical);
    }

    #[tokio::test]
    async fn test_empty_cluster_is_healthy() {
        let monitor = make_monitor();
        let status = monitor.cluster_status().await;
        assert_eq!(status, ClusterHealth::Healthy);
    }

    #[tokio::test]
    async fn test_get_nonexistent_node() {
        let monitor = make_monitor();
        assert!(monitor.check_health("nonexistent").await.is_none());
        assert!(monitor.get_node_health("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_reset_errors() {
        let monitor = ClusterHealthMonitor::new(HealthThresholds {
            error_count_threshold: 0,
            ..Default::default()
        });
        monitor.record_heartbeat("node-1").await;
        monitor.record_error("node-1").await;

        let health = monitor.get_node_health("node-1").await.unwrap();
        assert_eq!(health.error_count, 1);

        monitor.reset_errors("node-1").await;
        let health = monitor.get_node_health("node-1").await.unwrap();
        assert_eq!(health.error_count, 0);
        assert_eq!(health.status, NodeHealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_remove_node() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        assert_eq!(monitor.node_count().await, 1);

        monitor.remove_node("node-1").await;
        assert_eq!(monitor.node_count().await, 0);
    }

    #[tokio::test]
    async fn test_all_node_health() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        monitor.record_heartbeat("node-2").await;

        let all = monitor.all_node_health().await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_set_leadership_count() {
        let monitor = make_monitor();
        monitor.record_heartbeat("node-1").await;
        monitor.set_leadership_count("node-1", 5).await;

        let health = monitor.get_node_health("node-1").await.unwrap();
        assert_eq!(health.partition_leadership_count, 5);
    }
}
