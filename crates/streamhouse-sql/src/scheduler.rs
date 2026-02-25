//! Continuous query scheduler
//!
//! Manages the lifecycle of continuously running SQL queries: submission,
//! execution, pausing, resuming, and stopping. Each query runs as its own
//! Tokio task and is controlled via a [`tokio::sync::watch`] channel.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info};

use crate::error::SqlError;
use crate::Result;

// ---------------------------------------------------------------------------
// Query status
// ---------------------------------------------------------------------------

/// Lifecycle status of a continuous query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryStatus {
    /// Query has been submitted but not yet started.
    Submitted,
    /// Query is actively processing records.
    Running,
    /// Query has been paused (can be resumed).
    Paused,
    /// Query has been stopped (terminal state).
    Stopped,
    /// Query encountered an unrecoverable error.
    Failed,
}

// ---------------------------------------------------------------------------
// Continuous query definition
// ---------------------------------------------------------------------------

/// A continuously running SQL query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousQuery {
    /// Unique identifier assigned by the scheduler.
    pub query_id: String,
    /// The SQL text of the query.
    pub sql: String,
    /// Current lifecycle status.
    pub status: QueryStatus,
    /// When the query was submitted (ms since epoch).
    pub created_at: i64,
    /// When the query status was last updated (ms since epoch).
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// Query statistics
// ---------------------------------------------------------------------------

/// Runtime statistics for a continuous query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    /// Total number of records processed.
    pub records_processed: u64,
    /// Total bytes processed.
    pub bytes_processed: u64,
    /// Number of processing errors encountered.
    pub errors: u64,
    /// When the query started running (ms since epoch).
    pub started_at: i64,
    /// Timestamp of the most recent activity (ms since epoch).
    pub last_active: i64,
}

impl Default for QueryStats {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            records_processed: 0,
            bytes_processed: 0,
            errors: 0,
            started_at: now,
            last_active: now,
        }
    }
}

// ---------------------------------------------------------------------------
// Control signal
// ---------------------------------------------------------------------------

/// Signals sent to a running query task via a watch channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuerySignal {
    /// Keep running (or start running).
    Run,
    /// Pause processing.
    Pause,
    /// Stop the query permanently.
    Stop,
}

// ---------------------------------------------------------------------------
// QueryHandle (returned to the caller)
// ---------------------------------------------------------------------------

/// Handle for controlling a submitted query.
#[derive(Debug)]
pub struct QueryHandle {
    /// Unique query identifier.
    pub query_id: String,
    signal_tx: watch::Sender<QuerySignal>,
    shared_stats: Arc<Mutex<QueryStats>>,
}

impl QueryHandle {
    /// Pause the query.
    pub fn pause(&self) {
        let _ = self.signal_tx.send(QuerySignal::Pause);
    }

    /// Resume a paused query.
    pub fn resume(&self) {
        let _ = self.signal_tx.send(QuerySignal::Run);
    }

    /// Stop the query (terminal).
    pub fn stop(&self) {
        let _ = self.signal_tx.send(QuerySignal::Stop);
    }

    /// Retrieve a snapshot of the query statistics.
    pub async fn stats(&self) -> QueryStats {
        self.shared_stats.lock().await.clone()
    }
}

// ---------------------------------------------------------------------------
// Internal bookkeeping
// ---------------------------------------------------------------------------

/// Internal entry stored by the scheduler.
struct QueryEntry {
    query: ContinuousQuery,
    signal_tx: watch::Sender<QuerySignal>,
    _task_handle: Option<JoinHandle<()>>,
    shared_stats: Arc<Mutex<QueryStats>>,
}

// ---------------------------------------------------------------------------
// Resource limits
// ---------------------------------------------------------------------------

/// Resource limits for the scheduler.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum number of queries that can run concurrently.
    pub max_concurrent_queries: usize,
    /// Maximum bytes of memory allowed per query (advisory).
    pub per_query_memory_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_concurrent_queries: 64,
            per_query_memory_bytes: 256 * 1024 * 1024, // 256 MB
        }
    }
}

// ---------------------------------------------------------------------------
// QueryScheduler
// ---------------------------------------------------------------------------

/// Schedules and manages the lifecycle of continuous queries.
pub struct QueryScheduler {
    queries: Arc<RwLock<HashMap<String, QueryEntry>>>,
    resource_limits: ResourceLimits,
    next_id: Arc<Mutex<u64>>,
}

impl QueryScheduler {
    /// Create a new scheduler with the given resource limits.
    pub fn new(resource_limits: ResourceLimits) -> Self {
        Self {
            queries: Arc::new(RwLock::new(HashMap::new())),
            resource_limits,
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Submit a new continuous query.
    ///
    /// Returns a [`QueryHandle`] that can be used to control the query.
    pub async fn submit_query(&self, sql: String) -> Result<QueryHandle> {
        // Check capacity
        {
            let queries = self.queries.read().await;
            let active_count = queries
                .values()
                .filter(|e| {
                    matches!(
                        e.query.status,
                        QueryStatus::Running | QueryStatus::Paused | QueryStatus::Submitted
                    )
                })
                .count();
            if active_count >= self.resource_limits.max_concurrent_queries {
                return Err(SqlError::StreamProcessingError(format!(
                    "maximum concurrent queries ({}) reached",
                    self.resource_limits.max_concurrent_queries
                )));
            }
        }

        // Generate query ID
        let query_id = {
            let mut next = self.next_id.lock().await;
            let id = format!("cq-{}", *next);
            *next += 1;
            id
        };

        let now = chrono::Utc::now().timestamp_millis();
        let query = ContinuousQuery {
            query_id: query_id.clone(),
            sql: sql.clone(),
            status: QueryStatus::Running,
            created_at: now,
            updated_at: now,
        };

        let (signal_tx, signal_rx) = watch::channel(QuerySignal::Run);
        let shared_stats = Arc::new(Mutex::new(QueryStats::default()));

        // Spawn a task that simulates query execution
        let task_stats = shared_stats.clone();
        let task_query_id = query_id.clone();
        let task_handle = tokio::spawn(async move {
            debug!(query_id = %task_query_id, "continuous query task started");
            let mut rx = signal_rx;
            let start = Instant::now();
            loop {
                // Check for signal changes
                let signal = rx.borrow().clone();
                match signal {
                    QuerySignal::Stop => {
                        info!(query_id = %task_query_id, "query stopped");
                        break;
                    }
                    QuerySignal::Pause => {
                        // Wait for signal to change
                        if rx.changed().await.is_err() {
                            break;
                        }
                        continue;
                    }
                    QuerySignal::Run => {
                        // Process records (simulated)
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        let mut stats = task_stats.lock().await;
                        stats.records_processed += 1;
                        stats.last_active = chrono::Utc::now().timestamp_millis();
                    }
                }

                // Safety: don't run forever in tests
                if start.elapsed() > std::time::Duration::from_secs(300) {
                    break;
                }

                // Yield to check for new signals
                tokio::task::yield_now().await;
            }
        });

        let handle = QueryHandle {
            query_id: query_id.clone(),
            signal_tx: signal_tx.clone(),
            shared_stats: shared_stats.clone(),
        };

        let entry = QueryEntry {
            query,
            signal_tx,
            _task_handle: Some(task_handle),
            shared_stats,
        };

        {
            let mut queries = self.queries.write().await;
            queries.insert(query_id.clone(), entry);
        }

        info!(query_id = %query_id, sql = %sql, "continuous query submitted");

        Ok(handle)
    }

    /// Stop a query by ID.
    pub async fn stop_query(&self, query_id: &str) -> Result<()> {
        let mut queries = self.queries.write().await;
        let entry = queries.get_mut(query_id).ok_or_else(|| {
            SqlError::StreamProcessingError(format!("query not found: {query_id}"))
        })?;
        let _ = entry.signal_tx.send(QuerySignal::Stop);
        entry.query.status = QueryStatus::Stopped;
        entry.query.updated_at = chrono::Utc::now().timestamp_millis();
        info!(query_id = %query_id, "query stopped");
        Ok(())
    }

    /// Pause a query by ID.
    pub async fn pause_query(&self, query_id: &str) -> Result<()> {
        let mut queries = self.queries.write().await;
        let entry = queries.get_mut(query_id).ok_or_else(|| {
            SqlError::StreamProcessingError(format!("query not found: {query_id}"))
        })?;
        if entry.query.status != QueryStatus::Running {
            return Err(SqlError::StreamProcessingError(format!(
                "query {query_id} is not running (status: {:?})",
                entry.query.status
            )));
        }
        let _ = entry.signal_tx.send(QuerySignal::Pause);
        entry.query.status = QueryStatus::Paused;
        entry.query.updated_at = chrono::Utc::now().timestamp_millis();
        debug!(query_id = %query_id, "query paused");
        Ok(())
    }

    /// Resume a paused query.
    pub async fn resume_query(&self, query_id: &str) -> Result<()> {
        let mut queries = self.queries.write().await;
        let entry = queries.get_mut(query_id).ok_or_else(|| {
            SqlError::StreamProcessingError(format!("query not found: {query_id}"))
        })?;
        if entry.query.status != QueryStatus::Paused {
            return Err(SqlError::StreamProcessingError(format!(
                "query {query_id} is not paused (status: {:?})",
                entry.query.status
            )));
        }
        let _ = entry.signal_tx.send(QuerySignal::Run);
        entry.query.status = QueryStatus::Running;
        entry.query.updated_at = chrono::Utc::now().timestamp_millis();
        debug!(query_id = %query_id, "query resumed");
        Ok(())
    }

    /// List all queries and their statuses.
    pub async fn list_queries(&self) -> Vec<ContinuousQuery> {
        let queries = self.queries.read().await;
        queries.values().map(|e| e.query.clone()).collect()
    }

    /// Retrieve stats for a specific query.
    pub async fn get_stats(&self, query_id: &str) -> Result<QueryStats> {
        let queries = self.queries.read().await;
        let entry = queries.get(query_id).ok_or_else(|| {
            SqlError::StreamProcessingError(format!("query not found: {query_id}"))
        })?;
        let stats = entry.shared_stats.lock().await.clone();
        Ok(stats)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn default_scheduler() -> QueryScheduler {
        QueryScheduler::new(ResourceLimits::default())
    }

    #[tokio::test]
    async fn test_submit_query() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT * FROM orders".to_string())
            .await
            .unwrap();
        assert!(handle.query_id.starts_with("cq-"));
        handle.stop();
    }

    #[tokio::test]
    async fn test_submit_multiple_queries() {
        let scheduler = default_scheduler();
        let h1 = scheduler
            .submit_query("SELECT * FROM orders".to_string())
            .await
            .unwrap();
        let h2 = scheduler
            .submit_query("SELECT * FROM users".to_string())
            .await
            .unwrap();
        assert_ne!(h1.query_id, h2.query_id);
        h1.stop();
        h2.stop();
    }

    #[tokio::test]
    async fn test_stop_query() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT * FROM events".to_string())
            .await
            .unwrap();
        let qid = handle.query_id.clone();
        scheduler.stop_query(&qid).await.unwrap();

        let queries = scheduler.list_queries().await;
        let q = queries.iter().find(|q| q.query_id == qid).unwrap();
        assert_eq!(q.status, QueryStatus::Stopped);
    }

    #[tokio::test]
    async fn test_pause_and_resume_query() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT * FROM metrics".to_string())
            .await
            .unwrap();
        let qid = handle.query_id.clone();

        scheduler.pause_query(&qid).await.unwrap();
        {
            let queries = scheduler.list_queries().await;
            let q = queries.iter().find(|q| q.query_id == qid).unwrap();
            assert_eq!(q.status, QueryStatus::Paused);
        }

        scheduler.resume_query(&qid).await.unwrap();
        {
            let queries = scheduler.list_queries().await;
            let q = queries.iter().find(|q| q.query_id == qid).unwrap();
            assert_eq!(q.status, QueryStatus::Running);
        }

        handle.stop();
    }

    #[tokio::test]
    async fn test_list_queries() {
        let scheduler = default_scheduler();
        let h1 = scheduler
            .submit_query("SELECT 1".to_string())
            .await
            .unwrap();
        let h2 = scheduler
            .submit_query("SELECT 2".to_string())
            .await
            .unwrap();

        let queries = scheduler.list_queries().await;
        assert_eq!(queries.len(), 2);

        h1.stop();
        h2.stop();
    }

    #[tokio::test]
    async fn test_get_stats() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT * FROM logs".to_string())
            .await
            .unwrap();

        // Allow some processing time
        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = scheduler.get_stats(&handle.query_id).await.unwrap();
        assert!(stats.started_at > 0);

        handle.stop();
    }

    #[tokio::test]
    async fn test_get_stats_via_handle() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT * FROM data".to_string())
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
        let stats = handle.stats().await;
        assert!(stats.started_at > 0);

        handle.stop();
    }

    #[tokio::test]
    async fn test_stop_nonexistent_query() {
        let scheduler = default_scheduler();
        let result = scheduler.stop_query("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pause_nonexistent_query() {
        let scheduler = default_scheduler();
        let result = scheduler.pause_query("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resume_nonexistent_query() {
        let scheduler = default_scheduler();
        let result = scheduler.resume_query("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pause_stopped_query_fails() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT 1".to_string())
            .await
            .unwrap();
        let qid = handle.query_id.clone();
        scheduler.stop_query(&qid).await.unwrap();

        let result = scheduler.pause_query(&qid).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resume_running_query_fails() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT 1".to_string())
            .await
            .unwrap();
        let qid = handle.query_id.clone();

        let result = scheduler.resume_query(&qid).await;
        assert!(result.is_err());

        handle.stop();
    }

    #[tokio::test]
    async fn test_max_concurrent_queries_exceeded() {
        let scheduler = QueryScheduler::new(ResourceLimits {
            max_concurrent_queries: 2,
            per_query_memory_bytes: 1024,
        });

        let h1 = scheduler
            .submit_query("SELECT 1".to_string())
            .await
            .unwrap();
        let h2 = scheduler
            .submit_query("SELECT 2".to_string())
            .await
            .unwrap();
        let result = scheduler.submit_query("SELECT 3".to_string()).await;
        assert!(result.is_err());

        h1.stop();
        h2.stop();
    }

    #[tokio::test]
    async fn test_query_handle_pause_resume_stop() {
        let scheduler = default_scheduler();
        let handle = scheduler
            .submit_query("SELECT * FROM t".to_string())
            .await
            .unwrap();

        handle.pause();
        tokio::time::sleep(Duration::from_millis(20)).await;

        handle.resume();
        tokio::time::sleep(Duration::from_millis(20)).await;

        handle.stop();
    }

    #[tokio::test]
    async fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_concurrent_queries, 64);
        assert_eq!(limits.per_query_memory_bytes, 256 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_query_status_values() {
        let statuses = vec![
            QueryStatus::Submitted,
            QueryStatus::Running,
            QueryStatus::Paused,
            QueryStatus::Stopped,
            QueryStatus::Failed,
        ];
        // Ensure all variants are distinct
        for (i, a) in statuses.iter().enumerate() {
            for (j, b) in statuses.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
