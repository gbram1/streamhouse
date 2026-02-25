//! Agent - Core Lifecycle and Configuration
//!
//! The Agent is the main coordinator for a StreamHouse instance. It manages:
//! - Agent registration and heartbeat
//! - Partition lease management
//! - Graceful start/stop
//!
//! ## Lifecycle
//!
//! 1. **Build**: Configure agent (id, address, AZ, group)
//! 2. **Start**: Register with metadata store, start heartbeat
//! 3. **Run**: Serve traffic, renew leases
//! 4. **Stop**: Flush data, release leases, deregister
//!
//! ## Example
//!
//! ```rust,no_run
//! use streamhouse_agent::Agent;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let agent = Agent::builder()
//!     .agent_id("agent-us-east-1a-001")
//!     .address("10.0.1.5:9090")
//!     .availability_zone("us-east-1a")
//!     .agent_group("prod")
//!     .build()
//!     .await?;
//!
//! agent.start().await?;
//! // ... serve traffic ...
//! agent.stop().await?;
//! # Ok(())
//! # }
//! ```

use crate::assigner::PartitionAssigner;
use crate::error::{AgentError, Result};
use crate::heartbeat::HeartbeatTask;
use crate::lease_manager::LeaseManager;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::{AgentInfo, MetadataStore};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Agent configuration.
///
/// Defines all configuration parameters for a StreamHouse agent.
/// Use `AgentBuilder` for ergonomic construction with defaults.
///
/// # Fields
///
/// * `agent_id` - Unique agent identifier (e.g., "agent-us-east-1a-001")
/// * `address` - Agent's gRPC address (e.g., "10.0.1.5:9090")
/// * `availability_zone` - Cloud availability zone (e.g., "us-east-1a")
/// * `agent_group` - Agent group for network isolation (e.g., "prod", "staging")
/// * `heartbeat_interval` - How often to send heartbeats (default: 20s)
/// * `heartbeat_timeout` - When to consider agent dead (default: 60s)
/// * `metadata` - Optional metadata JSON string for monitoring/debugging
/// * `managed_topics` - Topics to auto-assign partitions for (optional)
///
/// # Examples
///
/// ```ignore
/// use streamhouse_agent::AgentConfig;
/// use std::time::Duration;
///
/// let config = AgentConfig {
///     agent_id: "agent-us-east-1a-001".to_string(),
///     address: "10.0.1.5:9090".to_string(),
///     availability_zone: "us-east-1a".to_string(),
///     agent_group: "prod".to_string(),
///     heartbeat_interval: Duration::from_secs(20),
///     heartbeat_timeout: Duration::from_secs(60),
///     metadata: Some(r#"{"version":"1.0.0"}"#.to_string()),
///     managed_topics: vec!["orders".to_string()],
/// };
/// ```
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Unique agent ID (e.g., "agent-us-east-1a-001")
    pub agent_id: String,

    /// Agent address for gRPC (e.g., "10.0.1.5:9090")
    pub address: String,

    /// Availability zone (e.g., "us-east-1a")
    pub availability_zone: String,

    /// Agent group for multi-tenancy (e.g., "prod", "staging")
    pub agent_group: String,

    /// Heartbeat interval (default: 20s)
    pub heartbeat_interval: Duration,

    /// Heartbeat timeout - agents dead if no heartbeat in this time (default: 60s)
    pub heartbeat_timeout: Duration,

    /// Optional metadata (JSON string for monitoring/debugging)
    pub metadata: Option<String>,

    /// Topics to auto-assign partitions for (optional)
    pub managed_topics: Vec<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: String::new(),
            address: String::new(),
            availability_zone: "default".to_string(),
            agent_group: "default".to_string(),
            heartbeat_interval: Duration::from_secs(20),
            heartbeat_timeout: Duration::from_secs(60),
            metadata: None,
            managed_topics: Vec::new(),
        }
    }
}

/// Main agent struct - manages lifecycle and coordination.
///
/// The Agent is the primary coordinator for a StreamHouse instance. It manages:
/// - Agent registration and heartbeat with the metadata store
/// - Partition lease acquisition and renewal
/// - Graceful start/stop with proper cleanup
/// - Optional automatic partition assignment
///
/// # Architecture
///
/// Agents are **stateless** - all state lives in the metadata store and S3.
/// This allows:
/// - Instant failover (another agent takes over expired leases)
/// - Easy horizontal scaling (just start more agents)
/// - No data loss on crashes (committed data is in S3)
///
/// # Thread Safety
///
/// Agent is Send + Sync and can be safely shared via Arc<Agent>.
/// Internal state is protected by RwLock for concurrent access.
///
/// # Lifecycle
///
/// 1. **Build**: Configure agent via `AgentBuilder`
/// 2. **Start**: Register with metadata, start heartbeat, acquire leases
/// 3. **Run**: Serve produce/consume requests, renew leases
/// 4. **Stop**: Flush data, release leases, deregister
///
/// # Examples
///
/// ```ignore
/// use streamhouse_agent::Agent;
///
/// // Create and start agent
/// let agent = Agent::builder()
///     .agent_id("agent-us-east-1a-001")
///     .address("10.0.1.5:9090")
///     .availability_zone("us-east-1a")
///     .agent_group("prod")
///     .build()
///     .await?;
///
/// agent.start().await?;
///
/// // ... serve traffic ...
///
/// // Graceful shutdown
/// agent.stop().await?;
/// ```
pub struct Agent {
    /// Agent configuration
    config: AgentConfig,

    /// Metadata store for coordination
    metadata_store: Arc<dyn MetadataStore>,

    /// Agent start timestamp (milliseconds since Unix epoch)
    started_at: i64,

    /// Agent state (Created, Started, or Stopped)
    state: Arc<RwLock<AgentState>>,

    /// Background heartbeat task handle
    heartbeat_handle: Arc<RwLock<Option<JoinHandle<()>>>>,

    /// Lease manager for partition leadership coordination
    lease_manager: Arc<LeaseManager>,

    /// Partition assigner for automatic partition assignment (optional)
    partition_assigner: Arc<RwLock<Option<PartitionAssigner>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentState {
    Created,
    Started,
    Stopped,
}

impl Agent {
    /// Create a new agent builder.
    ///
    /// This is the recommended way to construct an Agent. The builder provides
    /// a fluent API with sensible defaults for all optional parameters.
    ///
    /// # Returns
    ///
    /// An `AgentBuilder` for configuring and building the agent.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let agent = Agent::builder()
    ///     .agent_id("agent-001")
    ///     .address("localhost:9090")
    ///     .build()
    ///     .await?;
    /// ```
    pub fn builder() -> AgentBuilder {
        AgentBuilder::new()
    }

    /// Start the agent (register, begin heartbeat, acquire leases).
    ///
    /// This method performs the following operations in order:
    /// 1. Registers agent with metadata store (creates or updates agent record)
    /// 2. Starts background heartbeat task (sends heartbeat every 20s)
    /// 3. Starts lease renewal task (renews partition leases before expiration)
    /// 4. Optionally starts partition assignment (if managed_topics configured)
    ///
    /// # Returns
    ///
    /// `Ok(())` on successful start.
    ///
    /// # Errors
    ///
    /// - `AlreadyStarted`: Agent is already running
    /// - `Metadata`: Failed to register with metadata store
    ///
    /// # Important
    ///
    /// After calling `start()`, the agent is ready to serve traffic. The agent
    /// will automatically:
    /// - Maintain liveness via heartbeats
    /// - Acquire and renew partition leases
    /// - Detect lease expiration and gracefully release partitions
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let agent = Agent::builder()
    ///     .agent_id("agent-001")
    ///     .address("localhost:9090")
    ///     .build()
    ///     .await?;
    ///
    /// // Start agent (non-blocking)
    /// agent.start().await?;
    ///
    /// println!("Agent started and ready to serve traffic");
    /// ```
    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;

        if *state == AgentState::Started {
            return Err(AgentError::AlreadyStarted);
        }

        info!(
            agent_id = %self.config.agent_id,
            address = %self.config.address,
            availability_zone = %self.config.availability_zone,
            agent_group = %self.config.agent_group,
            "Starting StreamHouse agent"
        );

        // Register agent with metadata store
        self.register().await?;

        // Start heartbeat task
        self.start_heartbeat().await?;

        // Start lease renewal task
        self.lease_manager.start_renewal_task().await?;

        // Start partition assigner if topics configured
        if !self.config.managed_topics.is_empty() {
            let assigner = PartitionAssigner::new(
                self.config.agent_id.clone(),
                self.config.agent_group.clone(),
                Arc::clone(&self.metadata_store),
                Arc::clone(&self.lease_manager),
                self.config.managed_topics.clone(),
            );

            assigner.start().await?;

            *self.partition_assigner.write().await = Some(assigner);

            info!(
                agent_id = %self.config.agent_id,
                topics = ?self.config.managed_topics,
                "Partition assigner started"
            );
        }

        *state = AgentState::Started;

        info!(
            agent_id = %self.config.agent_id,
            "Agent started successfully"
        );

        Ok(())
    }

    /// Stop the agent (flush + release leases + deregister)
    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.write().await;

        if *state != AgentState::Started {
            warn!(
                agent_id = %self.config.agent_id,
                "Agent not started, skipping stop"
            );
            return Ok(());
        }

        info!(
            agent_id = %self.config.agent_id,
            "Stopping agent gracefully"
        );

        // Stop partition assigner (releases leases automatically)
        if let Some(assigner) = self.partition_assigner.write().await.take() {
            assigner.stop().await?;
            info!(
                agent_id = %self.config.agent_id,
                "Partition assigner stopped"
            );
        }

        // Stop lease renewal task
        self.lease_manager.stop_renewal_task().await?;

        // Release all partition leases (for manual leases)
        self.lease_manager.release_all_leases().await?;

        // Stop heartbeat task
        self.stop_heartbeat().await?;

        // Deregister agent
        self.deregister().await?;

        *state = AgentState::Stopped;

        info!(
            agent_id = %self.config.agent_id,
            "Agent stopped successfully"
        );

        Ok(())
    }

    /// Get agent ID
    pub fn agent_id(&self) -> &str {
        &self.config.agent_id
    }

    /// Get agent address
    pub fn address(&self) -> &str {
        &self.config.address
    }

    /// Get availability zone
    pub fn availability_zone(&self) -> &str {
        &self.config.availability_zone
    }

    /// Get agent group
    pub fn agent_group(&self) -> &str {
        &self.config.agent_group
    }

    /// Check if agent is started
    pub async fn is_started(&self) -> bool {
        *self.state.read().await == AgentState::Started
    }

    /// Get the lease manager (for acquiring partition leases)
    pub fn lease_manager(&self) -> &LeaseManager {
        &self.lease_manager
    }

    /// Get the partition assigner (if enabled)
    pub async fn partition_assigner(&self) -> Option<Vec<(String, u32)>> {
        if let Some(assigner) = self.partition_assigner.read().await.as_ref() {
            Some(assigner.get_assigned_partitions().await)
        } else {
            None
        }
    }

    /// Register agent with metadata store
    async fn register(&self) -> Result<()> {
        let metadata = if let Some(json_str) = &self.config.metadata {
            serde_json::from_str(json_str).unwrap_or_else(|_| HashMap::new())
        } else {
            HashMap::new()
        };

        let agent_info = AgentInfo {
            agent_id: self.config.agent_id.clone(),
            address: self.config.address.clone(),
            availability_zone: self.config.availability_zone.clone(),
            agent_group: self.config.agent_group.clone(),
            last_heartbeat: current_timestamp_ms(),
            started_at: self.started_at,
            metadata,
        };

        self.metadata_store.register_agent(agent_info).await?;

        info!(
            agent_id = %self.config.agent_id,
            "Agent registered with metadata store"
        );

        Ok(())
    }

    /// Deregister agent from metadata store
    async fn deregister(&self) -> Result<()> {
        self.metadata_store
            .deregister_agent(&self.config.agent_id)
            .await?;

        info!(
            agent_id = %self.config.agent_id,
            "Agent deregistered from metadata store"
        );

        Ok(())
    }

    /// Start heartbeat background task
    async fn start_heartbeat(&self) -> Result<()> {
        let agent_id = self.config.agent_id.clone();
        let address = self.config.address.clone();
        let availability_zone = self.config.availability_zone.clone();
        let agent_group = self.config.agent_group.clone();
        let metadata = self
            .config
            .metadata
            .clone()
            .unwrap_or_else(|| "{}".to_string());
        let started_at = self.started_at;
        let interval = self.config.heartbeat_interval;
        let metadata_store = Arc::clone(&self.metadata_store);

        let heartbeat_task = HeartbeatTask::new(
            agent_id,
            address,
            availability_zone,
            agent_group,
            started_at,
            metadata,
            interval,
            metadata_store,
        );

        let handle = tokio::spawn(async move {
            heartbeat_task.run().await;
        });

        *self.heartbeat_handle.write().await = Some(handle);

        info!(
            agent_id = %self.config.agent_id,
            interval_seconds = self.config.heartbeat_interval.as_secs(),
            "Heartbeat task started"
        );

        Ok(())
    }

    /// Stop heartbeat background task
    async fn stop_heartbeat(&self) -> Result<()> {
        let mut handle_guard = self.heartbeat_handle.write().await;

        if let Some(handle) = handle_guard.take() {
            handle.abort();

            // Wait for task to finish (ignore abort error)
            let _ = handle.await;

            info!(
                agent_id = %self.config.agent_id,
                "Heartbeat task stopped"
            );
        }

        Ok(())
    }
}

/// Builder for Agent
pub struct AgentBuilder {
    config: AgentConfig,
    metadata_store: Option<Arc<dyn MetadataStore>>,
}

impl AgentBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: AgentConfig::default(),
            metadata_store: None,
        }
    }

    /// Set agent ID
    pub fn agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.config.agent_id = agent_id.into();
        self
    }

    /// Set agent address
    pub fn address(mut self, address: impl Into<String>) -> Self {
        self.config.address = address.into();
        self
    }

    /// Set availability zone
    pub fn availability_zone(mut self, az: impl Into<String>) -> Self {
        self.config.availability_zone = az.into();
        self
    }

    /// Set agent group
    pub fn agent_group(mut self, group: impl Into<String>) -> Self {
        self.config.agent_group = group.into();
        self
    }

    /// Set heartbeat interval
    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.config.heartbeat_interval = interval;
        self
    }

    /// Set heartbeat timeout
    pub fn heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.config.heartbeat_timeout = timeout;
        self
    }

    /// Set metadata (JSON string)
    pub fn metadata(mut self, metadata: impl Into<String>) -> Self {
        self.config.metadata = Some(metadata.into());
        self
    }

    /// Set metadata store
    pub fn metadata_store(mut self, store: Arc<dyn MetadataStore>) -> Self {
        self.metadata_store = Some(store);
        self
    }

    /// Set topics for automatic partition assignment
    pub fn managed_topics(mut self, topics: Vec<String>) -> Self {
        self.config.managed_topics = topics;
        self
    }

    /// Build the agent
    pub async fn build(self) -> Result<Agent> {
        // Validate required fields
        if self.config.agent_id.is_empty() {
            return Err(AgentError::Storage("agent_id is required".to_string()));
        }

        if self.config.address.is_empty() {
            return Err(AgentError::Storage("address is required".to_string()));
        }

        let metadata_store = self
            .metadata_store
            .ok_or_else(|| AgentError::Storage("metadata_store is required".to_string()))?;

        let started_at = current_timestamp_ms();

        let lease_manager = Arc::new(LeaseManager::new(
            self.config.agent_id.clone(),
            Arc::clone(&metadata_store),
        ));

        Ok(Agent {
            config: self.config,
            metadata_store,
            started_at,
            state: Arc::new(RwLock::new(AgentState::Created)),
            heartbeat_handle: Arc::new(RwLock::new(None)),
            lease_manager,
            partition_assigner: Arc::new(RwLock::new(None)),
        })
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
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
    use streamhouse_metadata::SqliteMetadataStore;

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_agent.db");

        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let agent = Agent::builder()
            .agent_id("test-agent-1")
            .address("127.0.0.1:9090")
            .availability_zone("test-az")
            .agent_group("test")
            .metadata_store(Arc::clone(&metadata))
            .build()
            .await
            .unwrap();

        // Agent should not be started initially
        assert!(!agent.is_started().await);

        // Start agent
        agent.start().await.unwrap();
        assert!(agent.is_started().await);

        // Verify agent is registered
        let agents = metadata.list_agents(None, None).await.unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_id, "test-agent-1");

        // Stop agent
        agent.stop().await.unwrap();
        assert!(!agent.is_started().await);

        // Agent should be deregistered
        let agents = metadata.list_agents(None, None).await.unwrap();
        assert_eq!(agents.len(), 0);
    }

    #[test]
    fn test_agent_config_defaults() {
        let config = AgentConfig::default();
        assert_eq!(config.agent_id, "");
        assert_eq!(config.address, "");
        assert_eq!(config.availability_zone, "default");
        assert_eq!(config.agent_group, "default");
        assert_eq!(config.heartbeat_interval, Duration::from_secs(20));
        assert_eq!(config.heartbeat_timeout, Duration::from_secs(60));
        assert!(config.metadata.is_none());
        assert!(config.managed_topics.is_empty());
    }

    #[tokio::test]
    async fn test_builder_sets_fields() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_builder.db");
        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let agent = Agent::builder()
            .agent_id("my-agent")
            .address("10.0.0.1:9090")
            .availability_zone("us-west-2a")
            .agent_group("staging")
            .heartbeat_interval(Duration::from_secs(5))
            .heartbeat_timeout(Duration::from_secs(15))
            .metadata(r#"{"version":"2.0"}"#)
            .metadata_store(metadata)
            .build()
            .await
            .unwrap();

        assert_eq!(agent.agent_id(), "my-agent");
        assert_eq!(agent.address(), "10.0.0.1:9090");
        assert_eq!(agent.availability_zone(), "us-west-2a");
        assert_eq!(agent.agent_group(), "staging");
    }

    #[tokio::test]
    async fn test_builder_requires_agent_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_no_id.db");
        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let result = Agent::builder()
            .address("10.0.0.1:9090")
            .metadata_store(metadata)
            .build()
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_builder_requires_address() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_no_addr.db");
        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let result = Agent::builder()
            .agent_id("agent-1")
            .metadata_store(metadata)
            .build()
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_builder_requires_metadata_store() {
        let result = Agent::builder()
            .agent_id("agent-1")
            .address("10.0.0.1:9090")
            .build()
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_not_started_initially() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_not_started.db");
        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let agent = Agent::builder()
            .agent_id("agent-1")
            .address("127.0.0.1:9090")
            .metadata_store(metadata)
            .build()
            .await
            .unwrap();

        assert!(!agent.is_started().await);
    }

    #[tokio::test]
    async fn test_stop_when_not_started_is_ok() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_stop_not_started.db");
        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let agent = Agent::builder()
            .agent_id("agent-1")
            .address("127.0.0.1:9090")
            .metadata_store(metadata)
            .build()
            .await
            .unwrap();

        // Should not error when stopping a non-started agent
        agent.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_agent_managed_topics() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_managed.db");
        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let agent = Agent::builder()
            .agent_id("agent-topics")
            .address("127.0.0.1:9090")
            .metadata_store(metadata)
            .managed_topics(vec!["orders".to_string(), "events".to_string()])
            .build()
            .await
            .unwrap();

        // Before start, no assigned partitions
        assert!(agent.partition_assigner().await.is_none());
    }

    #[tokio::test]
    async fn test_agent_cannot_start_twice() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_agent_double_start.db");

        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let agent = Agent::builder()
            .agent_id("test-agent-2")
            .address("127.0.0.1:9090")
            .availability_zone("test-az")
            .agent_group("test")
            .metadata_store(metadata)
            .build()
            .await
            .unwrap();

        agent.start().await.unwrap();

        // Second start should fail
        let result = agent.start().await;
        assert!(matches!(result, Err(AgentError::AlreadyStarted)));

        agent.stop().await.unwrap();
    }
}
