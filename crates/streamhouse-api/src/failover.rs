//! Automatic Failover for High Availability
//!
//! Provides automatic failover capabilities for StreamHouse clusters.
//! Works in conjunction with leader election to ensure continuous availability.
//!
//! ## Features
//!
//! - Health monitoring of cluster nodes
//! - Automatic leader promotion on failure
//! - Configurable failover policies
//! - Fencing to prevent split-brain
//! - Failover event notifications
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::failover::{FailoverManager, FailoverConfig, HealthChecker};
//!
//! // Create failover manager with leader election
//! let config = FailoverConfig::default()
//!     .health_check_interval(Duration::from_secs(5))
//!     .failover_timeout(Duration::from_secs(30));
//!
//! let manager = FailoverManager::new(config, leader_election).await?;
//!
//! // Start monitoring
//! manager.start().await?;
//!
//! // Subscribe to failover events
//! let mut events = manager.subscribe();
//! while let Ok(event) = events.recv().await {
//!     match event {
//!         FailoverEvent::LeaderChanged { new_leader, old_leader } => {
//!             println!("Leader changed: {} -> {}", old_leader, new_leader);
//!         }
//!         FailoverEvent::NodeDown { node_id } => {
//!             println!("Node {} is down", node_id);
//!         }
//!     }
//! }
//! ```
//!
//! ## Failover Policies
//!
//! - **Immediate**: Failover as soon as leader is detected unhealthy
//! - **Quorum**: Require majority of nodes to agree before failover
//! - **Manual**: Only failover with explicit human intervention

use crate::leader::{LeaderElection, LeaderEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, RwLock};
use tokio::time::interval;

/// Failover errors
#[derive(Debug, Error)]
pub enum FailoverError {
    #[error("Leader election error: {0}")]
    LeaderElection(#[from] crate::leader::LeaderError),

    #[error("Health check failed: {0}")]
    HealthCheck(String),

    #[error("Failover failed: {0}")]
    FailoverFailed(String),

    #[error("Already running")]
    AlreadyRunning,

    #[error("Not running")]
    NotRunning,

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, FailoverError>;

/// Failover policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FailoverPolicy {
    /// Failover immediately when leader is unhealthy
    #[default]
    Immediate,
    /// Require manual intervention for failover
    Manual,
    /// Failover after grace period
    Graceful { grace_period: Duration },
}

/// Health status of a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Node is healthy
    Healthy,
    /// Node is degraded but functional
    Degraded,
    /// Node is unhealthy
    Unhealthy,
    /// Node status is unknown
    Unknown,
}

impl HealthStatus {
    /// Check if the node is considered healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }
}

/// Node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node identifier
    pub node_id: String,
    /// Node address (for health checks)
    pub address: Option<String>,
    /// Current health status
    pub health: HealthStatus,
    /// Last health check time
    pub last_check: Option<i64>,
    /// Last healthy time
    pub last_healthy: Option<i64>,
    /// Consecutive failed health checks
    pub failed_checks: u32,
    /// Whether this node is the leader
    pub is_leader: bool,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl NodeInfo {
    /// Create a new node info
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            address: None,
            health: HealthStatus::Unknown,
            last_check: None,
            last_healthy: None,
            failed_checks: 0,
            is_leader: false,
            metadata: HashMap::new(),
        }
    }

    /// Set node address
    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }
}

/// Failover configuration
#[derive(Debug, Clone)]
pub struct FailoverConfig {
    /// How often to check node health
    pub health_check_interval: Duration,
    /// Timeout for health checks
    pub health_check_timeout: Duration,
    /// Number of failed checks before marking unhealthy
    pub unhealthy_threshold: u32,
    /// Number of successful checks before marking healthy
    pub healthy_threshold: u32,
    /// Failover policy
    pub policy: FailoverPolicy,
    /// Maximum time to wait for failover to complete
    pub failover_timeout: Duration,
    /// Whether to enable automatic failover
    pub auto_failover: bool,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(5),
            health_check_timeout: Duration::from_secs(3),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            policy: FailoverPolicy::Immediate,
            failover_timeout: Duration::from_secs(30),
            auto_failover: true,
        }
    }
}

impl FailoverConfig {
    /// Set health check interval
    pub fn health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    /// Set health check timeout
    pub fn health_check_timeout(mut self, timeout: Duration) -> Self {
        self.health_check_timeout = timeout;
        self
    }

    /// Set unhealthy threshold
    pub fn unhealthy_threshold(mut self, threshold: u32) -> Self {
        self.unhealthy_threshold = threshold;
        self
    }

    /// Set failover policy
    pub fn policy(mut self, policy: FailoverPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Set failover timeout
    pub fn failover_timeout(mut self, timeout: Duration) -> Self {
        self.failover_timeout = timeout;
        self
    }

    /// Enable or disable automatic failover
    pub fn auto_failover(mut self, enabled: bool) -> Self {
        self.auto_failover = enabled;
        self
    }
}

/// Failover event
#[derive(Debug, Clone)]
pub enum FailoverEvent {
    /// A node's health status changed
    HealthChanged {
        node_id: String,
        old_status: HealthStatus,
        new_status: HealthStatus,
    },
    /// A node was detected as down
    NodeDown { node_id: String },
    /// A node came back up
    NodeUp { node_id: String },
    /// Leadership changed
    LeaderChanged {
        old_leader: Option<String>,
        new_leader: String,
        fencing_token: u64,
    },
    /// Failover initiated
    FailoverStarted {
        reason: String,
        old_leader: Option<String>,
    },
    /// Failover completed
    FailoverCompleted {
        new_leader: String,
        duration_ms: u64,
    },
    /// Failover failed
    FailoverFailed { reason: String },
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Node ID
    pub node_id: String,
    /// Health status
    pub status: HealthStatus,
    /// Response time (ms)
    pub response_time_ms: Option<u64>,
    /// Error message (if unhealthy)
    pub error: Option<String>,
    /// Timestamp
    pub timestamp: i64,
}

/// Trait for health checking
#[async_trait::async_trait]
pub trait HealthChecker: Send + Sync {
    /// Check health of a node
    async fn check(&self, node: &NodeInfo) -> HealthCheckResult;
}

/// Default health checker (HTTP-based)
pub struct HttpHealthChecker {
    client: reqwest::Client,
    path: String,
    timeout: Duration,
}

impl HttpHealthChecker {
    /// Create a new HTTP health checker
    pub fn new(path: impl Into<String>, timeout: Duration) -> Self {
        Self {
            client: reqwest::Client::new(),
            path: path.into(),
            timeout,
        }
    }
}

#[async_trait::async_trait]
impl HealthChecker for HttpHealthChecker {
    async fn check(&self, node: &NodeInfo) -> HealthCheckResult {
        let address = match &node.address {
            Some(addr) => addr,
            None => {
                return HealthCheckResult {
                    node_id: node.node_id.clone(),
                    status: HealthStatus::Unknown,
                    response_time_ms: None,
                    error: Some("No address configured".to_string()),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
            }
        };

        let url = format!("{}{}", address, self.path);
        let start = Instant::now();

        let result = tokio::time::timeout(self.timeout, self.client.get(&url).send()).await;

        let elapsed = start.elapsed().as_millis() as u64;
        let timestamp = chrono::Utc::now().timestamp_millis();

        match result {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    HealthCheckResult {
                        node_id: node.node_id.clone(),
                        status: HealthStatus::Healthy,
                        response_time_ms: Some(elapsed),
                        error: None,
                        timestamp,
                    }
                } else {
                    HealthCheckResult {
                        node_id: node.node_id.clone(),
                        status: HealthStatus::Unhealthy,
                        response_time_ms: Some(elapsed),
                        error: Some(format!("HTTP status: {}", response.status())),
                        timestamp,
                    }
                }
            }
            Ok(Err(e)) => HealthCheckResult {
                node_id: node.node_id.clone(),
                status: HealthStatus::Unhealthy,
                response_time_ms: Some(elapsed),
                error: Some(e.to_string()),
                timestamp,
            },
            Err(_) => HealthCheckResult {
                node_id: node.node_id.clone(),
                status: HealthStatus::Unhealthy,
                response_time_ms: Some(elapsed),
                error: Some("Health check timeout".to_string()),
                timestamp,
            },
        }
    }
}

/// Simple in-memory health checker for testing
pub struct InMemoryHealthChecker {
    status: RwLock<HashMap<String, HealthStatus>>,
}

impl InMemoryHealthChecker {
    /// Create a new in-memory health checker
    pub fn new() -> Self {
        Self {
            status: RwLock::new(HashMap::new()),
        }
    }

    /// Set health status for a node
    pub async fn set_status(&self, node_id: &str, status: HealthStatus) {
        self.status
            .write()
            .await
            .insert(node_id.to_string(), status);
    }
}

impl Default for InMemoryHealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HealthChecker for InMemoryHealthChecker {
    async fn check(&self, node: &NodeInfo) -> HealthCheckResult {
        let status = self
            .status
            .read()
            .await
            .get(&node.node_id)
            .copied()
            .unwrap_or(HealthStatus::Healthy);

        HealthCheckResult {
            node_id: node.node_id.clone(),
            status,
            response_time_ms: Some(1),
            error: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Failover manager
pub struct FailoverManager {
    config: FailoverConfig,
    leader_election: Arc<LeaderElection>,
    health_checker: Arc<dyn HealthChecker>,
    /// Known nodes in the cluster
    nodes: RwLock<HashMap<String, NodeInfo>>,
    /// Current leader
    current_leader: RwLock<Option<String>>,
    /// Whether the manager is running
    running: AtomicBool,
    /// Event broadcaster
    event_tx: broadcast::Sender<FailoverEvent>,
    /// Shutdown signal
    shutdown_tx: RwLock<Option<oneshot::Sender<()>>>,
}

impl FailoverManager {
    /// Create a new failover manager
    pub async fn new(
        config: FailoverConfig,
        leader_election: Arc<LeaderElection>,
        health_checker: Arc<dyn HealthChecker>,
    ) -> Result<Arc<Self>> {
        let (event_tx, _) = broadcast::channel(64);

        let manager = Arc::new(Self {
            config,
            leader_election,
            health_checker,
            nodes: RwLock::new(HashMap::new()),
            current_leader: RwLock::new(None),
            running: AtomicBool::new(false),
            event_tx,
            shutdown_tx: RwLock::new(None),
        });

        Ok(manager)
    }

    /// Register a node
    pub async fn register_node(&self, node: NodeInfo) {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node.node_id.clone(), node);
    }

    /// Unregister a node
    pub async fn unregister_node(&self, node_id: &str) {
        let mut nodes = self.nodes.write().await;
        nodes.remove(node_id);
    }

    /// Get all registered nodes
    pub async fn get_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values().cloned().collect()
    }

    /// Get a specific node
    pub async fn get_node(&self, node_id: &str) -> Option<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.get(node_id).cloned()
    }

    /// Get the current leader
    pub async fn current_leader(&self) -> Option<String> {
        self.current_leader.read().await.clone()
    }

    /// Subscribe to failover events
    pub fn subscribe(&self) -> broadcast::Receiver<FailoverEvent> {
        self.event_tx.subscribe()
    }

    /// Start the failover manager
    pub async fn start(self: Arc<Self>) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(FailoverError::AlreadyRunning);
        }

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        // Start health check loop
        let manager = self.clone();
        tokio::spawn(async move {
            let mut health_interval = interval(manager.config.health_check_interval);
            health_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            // Subscribe to leader events
            let mut leader_events = manager.leader_election.subscribe();

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        tracing::info!("Failover manager shutting down");
                        break;
                    }
                    _ = health_interval.tick() => {
                        if let Err(e) = manager.run_health_checks().await {
                            tracing::error!("Health check error: {}", e);
                        }
                    }
                    Ok(event) = leader_events.recv() => {
                        manager.handle_leader_event(event).await;
                    }
                }
            }

            manager.running.store(false, Ordering::SeqCst);
        });

        tracing::info!("Failover manager started");
        Ok(())
    }

    /// Stop the failover manager
    pub async fn stop(&self) -> Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(FailoverError::NotRunning);
        }

        let mut shutdown_tx = self.shutdown_tx.write().await;
        if let Some(tx) = shutdown_tx.take() {
            let _ = tx.send(());
        }

        Ok(())
    }

    /// Run health checks on all nodes
    async fn run_health_checks(&self) -> Result<()> {
        let nodes: Vec<NodeInfo> = self.nodes.read().await.values().cloned().collect();

        for node in nodes {
            let result = self.health_checker.check(&node).await;
            self.update_node_health(&node.node_id, result).await?;
        }

        // Check if failover is needed
        if self.config.auto_failover {
            self.check_failover_needed().await?;
        }

        Ok(())
    }

    /// Update node health based on check result
    async fn update_node_health(&self, node_id: &str, result: HealthCheckResult) -> Result<()> {
        let mut nodes = self.nodes.write().await;

        if let Some(node) = nodes.get_mut(node_id) {
            let old_status = node.health;
            let now = chrono::Utc::now().timestamp_millis();

            node.last_check = Some(now);

            if result.status.is_healthy() {
                node.failed_checks = 0;
                node.last_healthy = Some(now);

                // Check if recovered
                if !old_status.is_healthy() && node.health != HealthStatus::Unknown {
                    node.health = HealthStatus::Healthy;

                    let _ = self.event_tx.send(FailoverEvent::HealthChanged {
                        node_id: node_id.to_string(),
                        old_status,
                        new_status: HealthStatus::Healthy,
                    });

                    let _ = self.event_tx.send(FailoverEvent::NodeUp {
                        node_id: node_id.to_string(),
                    });
                }

                node.health = HealthStatus::Healthy;
            } else {
                node.failed_checks += 1;

                // Check if threshold exceeded
                if node.failed_checks >= self.config.unhealthy_threshold
                    && old_status.is_healthy()
                {
                    node.health = HealthStatus::Unhealthy;

                    let _ = self.event_tx.send(FailoverEvent::HealthChanged {
                        node_id: node_id.to_string(),
                        old_status,
                        new_status: HealthStatus::Unhealthy,
                    });

                    let _ = self.event_tx.send(FailoverEvent::NodeDown {
                        node_id: node_id.to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Check if failover is needed
    async fn check_failover_needed(&self) -> Result<()> {
        // Get current leader ID first
        let leader_id = self.current_leader.read().await.clone();

        // Check if current leader is unhealthy
        if let Some(leader_id) = leader_id {
            let is_unhealthy = {
                let nodes = self.nodes.read().await;
                nodes
                    .get(&leader_id)
                    .map(|leader| !leader.health.is_healthy())
                    .unwrap_or(false)
            };

            if is_unhealthy {
                // Leader is unhealthy, trigger failover
                self.trigger_failover(format!("Leader {} is unhealthy", leader_id))
                    .await?;
            }
        }

        Ok(())
    }

    /// Trigger a failover
    async fn trigger_failover(&self, reason: String) -> Result<()> {
        let old_leader = self.current_leader.read().await.clone();

        tracing::warn!(
            "Triggering failover: {} (old leader: {:?})",
            reason,
            old_leader
        );

        let _ = self.event_tx.send(FailoverEvent::FailoverStarted {
            reason: reason.clone(),
            old_leader: old_leader.clone(),
        });

        let start = Instant::now();

        // Apply failover policy
        match self.config.policy {
            FailoverPolicy::Immediate => {
                // Try to acquire leadership immediately
                self.attempt_leadership().await?;
            }
            FailoverPolicy::Manual => {
                tracing::info!("Manual failover required - waiting for intervention");
                // Don't do anything automatic
            }
            FailoverPolicy::Graceful { grace_period } => {
                // Wait for grace period before failover
                tokio::time::sleep(grace_period).await;
                self.attempt_leadership().await?;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        if let Some(new_leader) = self.current_leader.read().await.clone() {
            let _ = self.event_tx.send(FailoverEvent::FailoverCompleted {
                new_leader,
                duration_ms,
            });
        }

        Ok(())
    }

    /// Attempt to acquire leadership
    async fn attempt_leadership(&self) -> Result<()> {
        match self.leader_election.try_acquire().await {
            Ok(true) => {
                let new_leader = self.leader_election.node_id().to_string();
                let fencing_token = self.leader_election.fencing_token().unwrap_or(0);

                let old_leader = {
                    let mut current = self.current_leader.write().await;
                    let old = current.clone();
                    *current = Some(new_leader.clone());
                    old
                };

                let _ = self.event_tx.send(FailoverEvent::LeaderChanged {
                    old_leader,
                    new_leader,
                    fencing_token,
                });

                Ok(())
            }
            Ok(false) => {
                tracing::debug!("Failed to acquire leadership - another node may be leader");
                Ok(())
            }
            Err(e) => Err(FailoverError::FailoverFailed(e.to_string())),
        }
    }

    /// Handle leader election events
    async fn handle_leader_event(&self, event: LeaderEvent) {
        match event {
            LeaderEvent::Acquired { fencing_token } => {
                let new_leader = self.leader_election.node_id().to_string();
                let old_leader = {
                    let mut current = self.current_leader.write().await;
                    let old = current.clone();
                    *current = Some(new_leader.clone());
                    old
                };

                let _ = self.event_tx.send(FailoverEvent::LeaderChanged {
                    old_leader,
                    new_leader,
                    fencing_token,
                });
            }
            LeaderEvent::Lost { reason } => {
                tracing::warn!("Lost leadership: {:?}", reason);

                {
                    let mut current = self.current_leader.write().await;
                    *current = None;
                }

                // Try to trigger failover if auto failover is enabled
                if self.config.auto_failover {
                    if let Err(e) = self
                        .trigger_failover(format!("Lost leadership: {:?}", reason))
                        .await
                    {
                        tracing::error!("Failed to trigger failover: {}", e);
                    }
                }
            }
            LeaderEvent::Changed {
                new_leader,
                fencing_token,
            } => {
                let old_leader = {
                    let mut current = self.current_leader.write().await;
                    let old = current.clone();
                    *current = Some(new_leader.clone());
                    old
                };

                let _ = self.event_tx.send(FailoverEvent::LeaderChanged {
                    old_leader,
                    new_leader,
                    fencing_token,
                });
            }
        }
    }

    /// Force a failover (for manual intervention)
    pub async fn force_failover(&self, reason: String) -> Result<()> {
        self.trigger_failover(reason).await
    }

    /// Get failover statistics
    pub async fn stats(&self) -> FailoverStats {
        let nodes = self.nodes.read().await;

        let total_nodes = nodes.len();
        let healthy_nodes = nodes.values().filter(|n| n.health.is_healthy()).count();
        let current_leader = self.current_leader.read().await.clone();

        FailoverStats {
            total_nodes,
            healthy_nodes,
            unhealthy_nodes: total_nodes - healthy_nodes,
            current_leader,
            is_running: self.running.load(Ordering::SeqCst),
        }
    }
}

/// Failover statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverStats {
    pub total_nodes: usize,
    pub healthy_nodes: usize,
    pub unhealthy_nodes: usize,
    pub current_leader: Option<String>,
    pub is_running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leader::LeaderConfig;

    #[tokio::test]
    async fn test_failover_manager_creation() {
        let election = LeaderElection::new(
            LeaderConfig::memory()
                .namespace("test")
                .node_id("node-1")
                .lease_duration(Duration::from_secs(30))
                .renew_interval(Duration::from_secs(10)),
        )
        .await
        .unwrap();

        let health_checker = Arc::new(InMemoryHealthChecker::new());
        let config = FailoverConfig::default();

        let manager = FailoverManager::new(config, election, health_checker)
            .await
            .unwrap();

        assert!(!manager.running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_node_registration() {
        let election = LeaderElection::new(
            LeaderConfig::memory()
                .namespace("test")
                .node_id("node-1")
                .lease_duration(Duration::from_secs(30))
                .renew_interval(Duration::from_secs(10)),
        )
        .await
        .unwrap();

        let health_checker = Arc::new(InMemoryHealthChecker::new());
        let manager = FailoverManager::new(FailoverConfig::default(), election, health_checker)
            .await
            .unwrap();

        // Register nodes
        manager
            .register_node(NodeInfo::new("node-1").with_address("http://node1:8080"))
            .await;
        manager
            .register_node(NodeInfo::new("node-2").with_address("http://node2:8080"))
            .await;

        let nodes = manager.get_nodes().await;
        assert_eq!(nodes.len(), 2);

        // Unregister a node
        manager.unregister_node("node-1").await;
        let nodes = manager.get_nodes().await;
        assert_eq!(nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_health_check_update() {
        let election = LeaderElection::new(
            LeaderConfig::memory()
                .namespace("test")
                .node_id("node-1")
                .lease_duration(Duration::from_secs(30))
                .renew_interval(Duration::from_secs(10)),
        )
        .await
        .unwrap();

        let health_checker = Arc::new(InMemoryHealthChecker::new());
        let manager = FailoverManager::new(
            FailoverConfig::default().unhealthy_threshold(2),
            election,
            health_checker.clone(),
        )
        .await
        .unwrap();

        // Register a node
        manager.register_node(NodeInfo::new("node-1")).await;

        // Initially healthy
        health_checker
            .set_status("node-1", HealthStatus::Healthy)
            .await;
        manager.run_health_checks().await.unwrap();

        let node = manager.get_node("node-1").await.unwrap();
        assert_eq!(node.health, HealthStatus::Healthy);

        // Simulate failures
        health_checker
            .set_status("node-1", HealthStatus::Unhealthy)
            .await;
        manager.run_health_checks().await.unwrap();
        manager.run_health_checks().await.unwrap();

        let node = manager.get_node("node-1").await.unwrap();
        assert_eq!(node.health, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_failover_events() {
        let election = LeaderElection::new(
            LeaderConfig::memory()
                .namespace("test")
                .node_id("node-1")
                .lease_duration(Duration::from_secs(30))
                .renew_interval(Duration::from_secs(10)),
        )
        .await
        .unwrap();

        let health_checker = Arc::new(InMemoryHealthChecker::new());
        let manager = FailoverManager::new(FailoverConfig::default(), election, health_checker)
            .await
            .unwrap();

        let mut events = manager.subscribe();

        // Register a node and set as unhealthy
        manager.register_node(NodeInfo::new("node-1")).await;

        // Trigger failover
        manager
            .force_failover("Test failover".to_string())
            .await
            .unwrap();

        // Should receive events
        let event = events.try_recv().unwrap();
        assert!(matches!(event, FailoverEvent::FailoverStarted { .. }));
    }

    #[test]
    fn test_config_builder() {
        let config = FailoverConfig::default()
            .health_check_interval(Duration::from_secs(10))
            .unhealthy_threshold(5)
            .policy(FailoverPolicy::Manual)
            .auto_failover(false);

        assert_eq!(config.health_check_interval, Duration::from_secs(10));
        assert_eq!(config.unhealthy_threshold, 5);
        assert_eq!(config.policy, FailoverPolicy::Manual);
        assert!(!config.auto_failover);
    }

    #[test]
    fn test_health_status() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(HealthStatus::Degraded.is_healthy());
        assert!(!HealthStatus::Unhealthy.is_healthy());
        assert!(!HealthStatus::Unknown.is_healthy());
    }
}
