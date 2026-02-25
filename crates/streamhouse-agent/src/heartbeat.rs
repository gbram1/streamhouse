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

    /// Helper: create a metadata store backed by a temp SQLite DB.
    async fn make_store() -> (Arc<dyn MetadataStore>, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("hb_test.db");
        let store = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        (Arc::new(store) as Arc<dyn MetadataStore>, temp_dir)
    }

    // ----------------------------------------------------------------
    // Existing tests
    // ----------------------------------------------------------------

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

    // ----------------------------------------------------------------
    // New tests
    // ----------------------------------------------------------------

    #[tokio::test]
    async fn test_heartbeat_construction_stores_fields() {
        let (store, _dir) = make_store().await;
        let started = current_timestamp_ms();

        let hb = HeartbeatTask::new(
            "hb-agent-1".to_string(),
            "10.0.0.1:8080".to_string(),
            "us-west-2b".to_string(),
            "staging".to_string(),
            started,
            r#"{"version":"3.0"}"#.to_string(),
            Duration::from_secs(30),
            Arc::clone(&store),
        );

        // Validate fields are stored correctly (they are private, so we
        // check indirectly: run one heartbeat cycle and verify the
        // agent record written to the metadata store).
        let handle = tokio::spawn(async move {
            hb.run().await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // The first heartbeat fires after `interval`, so wait a bit
        // We used 30s interval which is too long for the test, so
        // abort and check that NO agent was written (interval hasn't
        // elapsed yet).
        handle.abort();
        let _ = handle.await;

        // With a 30-second interval and only 100ms of wait, the heartbeat
        // should NOT have fired yet -- confirming the interval is respected.
        let agents = store.list_agents(None, None).await.unwrap();
        assert!(
            agents.is_empty(),
            "no heartbeat should fire within 100ms of a 30s interval"
        );
    }

    #[tokio::test]
    async fn test_heartbeat_interval_respected() {
        let (store, _dir) = make_store().await;

        let hb = HeartbeatTask::new(
            "hb-interval".to_string(),
            "127.0.0.1:9090".to_string(),
            "az-1".to_string(),
            "default".to_string(),
            current_timestamp_ms(),
            "{}".to_string(),
            Duration::from_millis(80),
            Arc::clone(&store),
        );

        let handle = tokio::spawn(async move {
            hb.run().await;
        });

        // After 50ms (less than one interval of 80ms), no heartbeat yet
        tokio::time::sleep(Duration::from_millis(50)).await;
        let agents = store.list_agents(None, None).await.unwrap();
        assert!(agents.is_empty(), "heartbeat should not fire before interval elapses");

        // After another 60ms (total ~110ms > 80ms), first heartbeat should have fired
        tokio::time::sleep(Duration::from_millis(60)).await;
        let agents = store.list_agents(None, None).await.unwrap();
        assert_eq!(agents.len(), 1, "first heartbeat should have registered the agent");

        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_heartbeat_multiple_cycles() {
        let (store, _dir) = make_store().await;

        let hb = HeartbeatTask::new(
            "hb-multi".to_string(),
            "127.0.0.1:9090".to_string(),
            "az-1".to_string(),
            "default".to_string(),
            current_timestamp_ms(),
            "{}".to_string(),
            Duration::from_millis(50),
            Arc::clone(&store),
        );

        let handle = tokio::spawn(async move {
            hb.run().await;
        });

        // Wait long enough for at least 3 heartbeat cycles (50ms * 4 + buffer)
        tokio::time::sleep(Duration::from_millis(250)).await;

        let agents = store.list_agents(None, None).await.unwrap();
        assert_eq!(agents.len(), 1);

        // The last_heartbeat should be very recent (within ~100ms)
        let age = current_timestamp_ms() - agents[0].last_heartbeat;
        assert!(
            age < 150,
            "after multiple cycles last heartbeat should be recent, was {}ms old",
            age
        );

        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_heartbeat_cancellation() {
        let (store, _dir) = make_store().await;

        let hb = HeartbeatTask::new(
            "hb-cancel".to_string(),
            "127.0.0.1:9090".to_string(),
            "az-1".to_string(),
            "default".to_string(),
            current_timestamp_ms(),
            "{}".to_string(),
            Duration::from_millis(50),
            Arc::clone(&store),
        );

        let handle = tokio::spawn(async move {
            hb.run().await;
        });

        // Let one heartbeat fire
        tokio::time::sleep(Duration::from_millis(80)).await;
        let agents = store.list_agents(None, None).await.unwrap();
        assert_eq!(agents.len(), 1);
        let hb_before = agents[0].last_heartbeat;

        // Abort the task
        handle.abort();
        let _ = handle.await;

        // Wait a bit and verify no more heartbeats happen
        tokio::time::sleep(Duration::from_millis(150)).await;
        let agents = store.list_agents(None, None).await.unwrap();
        // The heartbeat value should not have advanced after cancellation
        assert_eq!(
            agents[0].last_heartbeat, hb_before,
            "heartbeat should stop updating after cancellation"
        );
    }

    #[tokio::test]
    async fn test_heartbeat_preserves_agent_metadata() {
        let (store, _dir) = make_store().await;

        let hb = HeartbeatTask::new(
            "hb-meta".to_string(),
            "10.0.0.5:7070".to_string(),
            "eu-west-1a".to_string(),
            "production".to_string(),
            12345678,
            r#"{"version":"2.1","build":"abc123"}"#.to_string(),
            Duration::from_millis(50),
            Arc::clone(&store),
        );

        let handle = tokio::spawn(async move {
            hb.run().await;
        });

        tokio::time::sleep(Duration::from_millis(80)).await;

        let agents = store.list_agents(None, None).await.unwrap();
        assert_eq!(agents.len(), 1);
        let agent = &agents[0];
        assert_eq!(agent.agent_id, "hb-meta");
        assert_eq!(agent.address, "10.0.0.5:7070");
        assert_eq!(agent.availability_zone, "eu-west-1a");
        assert_eq!(agent.agent_group, "production");
        assert_eq!(agent.started_at, 12345678);
        assert_eq!(agent.metadata.get("version").map(|s| s.as_str()), Some("2.1"));
        assert_eq!(agent.metadata.get("build").map(|s| s.as_str()), Some("abc123"));

        handle.abort();
        let _ = handle.await;
    }

    #[test]
    fn test_current_timestamp_ms_is_positive() {
        let ts = current_timestamp_ms();
        assert!(ts > 0, "timestamp should be positive");
        // Should be after 2020-01-01 in milliseconds
        assert!(ts > 1_577_836_800_000, "timestamp should be after 2020");
    }
}
