//! Heartbeat Task - Agent Liveness Detection
//!
//! The heartbeat task runs in the background and periodically updates the agent's
//! `last_heartbeat` timestamp in the metadata store. This allows other agents and
//! monitoring systems to detect when an agent has died.
//!
//! ## How It Works
//!
//! 1. Every 20 seconds (configurable), call `register_agent()` with current timestamp
//! 2. If update fails, log error but continue (transient failures are OK)
//! 3. Run until the task is cancelled (when agent stops)
//!
//! ## Failure Detection
//!
//! - Agents with `last_heartbeat > 60s old` are considered dead
//! - `list_agents()` automatically filters out stale agents
//! - Monitoring dashboards show agent heartbeat age
//!
//! ## Example
//!
//! ```rust,no_run
//! use streamhouse_agent::HeartbeatTask;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! # async fn example(metadata_store: Arc<dyn streamhouse_metadata::MetadataStore>) {
//! let heartbeat = HeartbeatTask::new(
//!     "agent-1".to_string(),
//!     "10.0.1.5:9090".to_string(),
//!     "us-east-1a".to_string(),
//!     "prod".to_string(),
//!     1234567890,  // started_at
//!     "{}".to_string(),  // metadata
//!     Duration::from_secs(20),
//!     metadata_store,
//! );
//!
//! // Run in background (cancellable)
//! let handle = tokio::spawn(async move {
//!     heartbeat.run().await;
//! });
//!
//! // Later: cancel the task
//! handle.abort();
//! # }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::{AgentInfo, MetadataStore};
use tracing::{debug, error, info, warn};

/// Background task that sends periodic heartbeats
pub struct HeartbeatTask {
    agent_id: String,
    address: String,
    availability_zone: String,
    agent_group: String,
    started_at: i64,
    metadata: String,
    interval: Duration,
    metadata_store: Arc<dyn MetadataStore>,
}

impl HeartbeatTask {
    /// Create a new heartbeat task
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent_id: String,
        address: String,
        availability_zone: String,
        agent_group: String,
        started_at: i64,
        metadata: String,
        interval: Duration,
        metadata_store: Arc<dyn MetadataStore>,
    ) -> Self {
        Self {
            agent_id,
            address,
            availability_zone,
            agent_group,
            started_at,
            metadata,
            interval,
            metadata_store,
        }
    }

    /// Run the heartbeat loop (blocks until cancelled)
    pub async fn run(self) {
        info!(
            agent_id = %self.agent_id,
            interval_seconds = self.interval.as_secs(),
            "Heartbeat task started"
        );

        let mut heartbeat_count: u64 = 0;
        let mut failure_count: u64 = 0;

        loop {
            // Wait for interval (or cancellation)
            tokio::select! {
                _ = tokio::time::sleep(self.interval) => {}
                _ = tokio::signal::ctrl_c() => {
                    info!(
                        agent_id = %self.agent_id,
                        "Heartbeat task received shutdown signal"
                    );
                    break;
                }
            }

            // Send heartbeat
            match self.send_heartbeat().await {
                Ok(_) => {
                    heartbeat_count += 1;
                    debug!(
                        agent_id = %self.agent_id,
                        count = heartbeat_count,
                        "Heartbeat sent successfully"
                    );
                }
                Err(e) => {
                    failure_count += 1;
                    error!(
                        agent_id = %self.agent_id,
                        error = %e,
                        failure_count,
                        "Heartbeat failed"
                    );

                    // Log warning if failures are frequent
                    if failure_count >= 3 {
                        warn!(
                            agent_id = %self.agent_id,
                            failure_count,
                            "Multiple consecutive heartbeat failures - agent may be considered dead"
                        );
                    }
                }
            }
        }

        info!(
            agent_id = %self.agent_id,
            total_heartbeats = heartbeat_count,
            total_failures = failure_count,
            "Heartbeat task stopped"
        );
    }

    /// Send a single heartbeat
    async fn send_heartbeat(&self) -> Result<(), streamhouse_metadata::MetadataError> {
        let metadata: HashMap<String, String> =
            serde_json::from_str(&self.metadata).unwrap_or_else(|_| HashMap::new());

        let agent_info = AgentInfo {
            agent_id: self.agent_id.clone(),
            address: self.address.clone(),
            availability_zone: self.availability_zone.clone(),
            agent_group: self.agent_group.clone(),
            last_heartbeat: current_timestamp_ms(),
            started_at: self.started_at,
            metadata,
        };

        self.metadata_store.register_agent(agent_info).await
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
    async fn test_heartbeat_task() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_heartbeat.db");

        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let heartbeat = HeartbeatTask::new(
            "test-agent-heartbeat".to_string(),
            "127.0.0.1:9090".to_string(),
            "test-az".to_string(),
            "test".to_string(),
            current_timestamp_ms(),
            "{}".to_string(),
            Duration::from_millis(100), // Fast interval for testing
            Arc::clone(&metadata),
        );

        // Run heartbeat task for 350ms
        let handle = tokio::spawn(async move {
            heartbeat.run().await;
        });

        // Wait for a few heartbeats
        tokio::time::sleep(Duration::from_millis(350)).await;

        // Check that agent is registered
        let agents = metadata.list_agents(None, None).await.unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_id, "test-agent-heartbeat");

        // Heartbeat should be recent (< 200ms old)
        let age_ms = current_timestamp_ms() - agents[0].last_heartbeat;
        assert!(age_ms < 200, "Heartbeat too old: {}ms", age_ms);

        // Stop task
        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_heartbeat_updates_timestamp() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_heartbeat_updates.db");

        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let heartbeat = HeartbeatTask::new(
            "test-agent-updates".to_string(),
            "127.0.0.1:9090".to_string(),
            "test-az".to_string(),
            "test".to_string(),
            current_timestamp_ms(),
            "{}".to_string(),
            Duration::from_millis(50),
            Arc::clone(&metadata),
        );

        let handle = tokio::spawn(async move {
            heartbeat.run().await;
        });

        // Get first heartbeat
        tokio::time::sleep(Duration::from_millis(75)).await;
        let agents = metadata.list_agents(None, None).await.unwrap();
        let first_heartbeat = agents[0].last_heartbeat;

        // Wait for another heartbeat
        tokio::time::sleep(Duration::from_millis(100)).await;
        let agents = metadata.list_agents(None, None).await.unwrap();
        let second_heartbeat = agents[0].last_heartbeat;

        // Timestamp should have advanced
        assert!(
            second_heartbeat > first_heartbeat,
            "Heartbeat not updating: {} vs {}",
            first_heartbeat,
            second_heartbeat
        );

        handle.abort();
        let _ = handle.await;
    }
}
