//! Writer Pool - Manages Active Segment Writers Across Partitions
//!
//! This module implements `WriterPool`, which keeps one active `SegmentWriter` per partition
//! and periodically flushes them to S3. This solves the Phase 1 limitation where segments
//! weren't flushed until the server restarted.
//!
//! ## Problem in Phase 1
//!
//! Each produce request created a new `SegmentWriter`, wrote the record, then dropped the writer
//! without flushing. Records stayed in memory but never reached S3, so consumers couldn't read them.
//!
//! ## Solution: Writer Pooling
//!
//! - Keep one `SegmentWriter` per partition alive across requests
//! - Background thread flushes all writers every 5 seconds
//! - Graceful shutdown flushes all writers before exit
//!
//! ## Example Usage
//!
//! ```ignore
//! use streamhouse_storage::WriterPool;
//!
//! // Create pool
//! let pool = WriterPool::new(metadata_store, storage_backend, write_config);
//!
//! // Start background flush thread
//! pool.start_background_flush(Duration::from_secs(5));
//!
//! // Get writer for partition (creates if doesn't exist)
//! let writer = pool.get_writer("orders", 0).await?;
//!
//! // Write record
//! let mut writer_guard = writer.lock().await;
//! writer_guard.append(record)?;
//!
//! // Background thread will flush periodically
//! // Or call flush_all() manually
//! pool.flush_all().await?;
//! ```
//!
//! ## Thread Safety
//!
//! - `WriterPool` uses `Arc<RwLock<HashMap>>` for the writer map
//! - Each writer is wrapped in `Arc<Mutex<SegmentWriter>>` for safe concurrent access
//! - Multiple threads can get writers simultaneously (read lock)
//! - Only writer creation takes write lock (rare)
//!
//! ## Performance Benefits
//!
//! - **6x throughput improvement**: Reusing writers eliminates setup overhead
//! - **Lower latency**: No writer creation on hot path
//! - **Batching**: Multiple records share the same segment
//! - **Consume works**: Background flush makes data available

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, RwLock};
use tokio::time;

use object_store::ObjectStore;
use streamhouse_metadata::MetadataStore;

use crate::config::WriteConfig;
use crate::error::Result;
use crate::writer::{DurableFlushHandle, PartitionWriter};

/// Type alias for the writer map to reduce type complexity.
/// Key is (org_id, topic, partition) to ensure tenant isolation.
type WriterMap = Arc<RwLock<HashMap<(String, String, u32), Arc<Mutex<PartitionWriter>>>>>;

/// Pool of active segment writers, one per (org, topic, partition)
///
/// The pool manages writers across all organizations, topics and partitions,
/// keeping them alive between requests and periodically flushing them to storage.
pub struct WriterPool {
    /// Map of (org_id, topic, partition) -> PartitionWriter
    ///
    /// Using RwLock for the map since reads (get_writer) are common,
    /// writes (creating new writer) are rare.
    writers: WriterMap,

    /// Map of (org_id, topic, partition) -> DurableFlushHandle for batched ACK_DURABLE
    #[allow(clippy::type_complexity)]
    durable_flush_handles: Arc<RwLock<HashMap<(String, String, u32), DurableFlushHandle>>>,

    /// Metadata store for segment tracking
    metadata_store: Arc<dyn MetadataStore>,

    /// Object store for S3 or local filesystem
    object_store: Arc<dyn ObjectStore>,

    /// Write configuration (segment size, flush interval, etc.)
    config: WriteConfig,
}

impl WriterPool {
    /// Create a new writer pool
    ///
    /// ## Arguments
    ///
    /// - `metadata_store`: Metadata store for tracking segments
    /// - `object_store`: Where to upload segments (S3 or local)
    /// - `config`: Write configuration
    ///
    /// ## Returns
    ///
    /// A new `WriterPool` ready to manage writers
    pub fn new(
        metadata_store: Arc<dyn MetadataStore>,
        object_store: Arc<dyn ObjectStore>,
        config: WriteConfig,
    ) -> Self {
        Self {
            writers: Arc::new(RwLock::new(HashMap::new())),
            durable_flush_handles: Arc::new(RwLock::new(HashMap::new())),
            metadata_store,
            object_store,
            config,
        }
    }

    /// Get writer for an org's topic partition, creating it if it doesn't exist
    ///
    /// This is the main entry point for producing records. Call this to get the writer
    /// for a partition, then lock it and append records.
    ///
    /// ## Arguments
    ///
    /// - `org_id`: Organization ID (ensures tenant isolation)
    /// - `topic`: Topic name
    /// - `partition`: Partition number
    ///
    /// ## Returns
    ///
    /// An `Arc<Mutex<PartitionWriter>>` that can be locked to append records
    ///
    /// ## Example
    ///
    /// ```ignore
    /// let writer = pool.get_writer("org-uuid", "orders", 0).await?;
    /// let mut guard = writer.lock().await;
    /// guard.append(record)?;
    /// ```
    pub async fn get_writer(
        &self,
        org_id: &str,
        topic: &str,
        partition: u32,
    ) -> Result<Arc<Mutex<PartitionWriter>>> {
        let key = (org_id.to_string(), topic.to_string(), partition);

        // Try to get existing writer (read lock - fast path)
        {
            let readers = self.writers.read().await;
            if let Some(writer) = readers.get(&key) {
                return Ok(Arc::clone(writer));
            }
        }

        // Writer doesn't exist, create it (write lock - slow path, rare)
        let mut writers = self.writers.write().await;

        // Double-check in case another thread created it while we waited for write lock
        if let Some(writer) = writers.get(&key) {
            return Ok(Arc::clone(writer));
        }

        // Create new writer
        tracing::debug!(
            org_id = %org_id,
            topic = %topic,
            partition = partition,
            "Creating new partition writer"
        );

        // Resolve org-scoped S3 prefix
        let s3_data_prefix = if org_id.is_empty() {
            "data".to_string()
        } else {
            format!("org-{}/data", org_id)
        };

        let partition_writer = PartitionWriter::new(
            org_id.to_string(),
            topic.to_string(),
            partition,
            Arc::clone(&self.object_store),
            Arc::clone(&self.metadata_store),
            self.config.clone(),
            s3_data_prefix,
        )
        .await?;

        let writer = Arc::new(Mutex::new(partition_writer));
        writers.insert(key, Arc::clone(&writer));

        Ok(writer)
    }

    /// Read records from the in-memory buffer of a partition writer.
    ///
    /// Returns records that have been appended but not yet flushed to S3,
    /// enabling sub-second consume latency.
    pub async fn read_buffered(
        &self,
        org_id: &str,
        topic: &str,
        partition: u32,
        start_offset: u64,
        max_records: usize,
    ) -> Vec<streamhouse_core::record::Record> {
        let key = (org_id.to_string(), topic.to_string(), partition);
        let readers = self.writers.read().await;
        if let Some(writer) = readers.get(&key) {
            let guard = writer.lock().await;
            guard.read_buffered(start_offset, max_records)
        } else {
            Vec::new()
        }
    }

    /// Request a batched durable flush for a partition.
    ///
    /// Instead of flushing to S3 immediately, this queues the caller to wait
    /// for the next batch flush (~200ms window). All callers in the same window
    /// share a single S3 upload.
    pub async fn request_durable_flush(
        &self,
        org_id: &str,
        topic: &str,
        partition: u32,
    ) -> Result<()> {
        let key = (org_id.to_string(), topic.to_string(), partition);

        // Fast path: check if handle exists
        {
            let handles = self.durable_flush_handles.read().await;
            if let Some(handle) = handles.get(&key) {
                return handle.request_flush().await;
            }
        }

        // Slow path: create handle (need writer first)
        let writer = self.get_writer(org_id, topic, partition).await?;
        let mut handles = self.durable_flush_handles.write().await;

        // Double-check
        if let Some(handle) = handles.get(&key) {
            return handle.request_flush().await;
        }

        let handle = DurableFlushHandle::spawn(writer, self.config.durable_batch_max_age_ms);
        let result = handle.request_flush().await;
        handles.insert(key, handle);
        result
    }

    /// Flush all active writers to storage
    ///
    /// This method iterates through all active writers and flushes any that have pending data.
    /// Called by the background flush thread and during graceful shutdown.
    ///
    /// ## Behavior
    ///
    /// - Only flushes writers with pending records
    /// - Uploads segments to S3
    /// - Updates metadata with new segments
    /// - Logs any errors but doesn't fail the entire flush
    ///
    /// ## Example
    ///
    /// ```ignore
    /// // Manually flush all writers
    /// pool.flush_all().await?;
    /// ```
    pub async fn flush_all(&self) -> Result<()> {
        let readers = self.writers.read().await;

        let mut flush_count = 0;
        let mut error_count = 0;

        for ((org_id, topic, partition), writer) in readers.iter() {
            let mut writer_guard = writer.lock().await;

            tracing::debug!(
                org_id = %org_id,
                topic = %topic,
                partition = partition,
                "Flushing partition writer"
            );

            // Use flush_durable() to unconditionally roll segments with data.
            // flush() only rolls when size >= 64MB or age >= 10min, which means
            // small amounts of data stay in memory and are never consumable.
            match writer_guard.flush_durable().await {
                Ok(_) => {
                    flush_count += 1;
                }
                Err(e) => {
                    error_count += 1;
                    tracing::error!(
                        topic = %topic,
                        partition = partition,
                        error = %e,
                        "Failed to flush partition writer"
                    );
                }
            }
        }

        if flush_count > 0 || error_count > 0 {
            tracing::info!(
                flushed = flush_count,
                errors = error_count,
                total_writers = readers.len(),
                "Background flush completed"
            );
        }

        Ok(())
    }

    /// Start background flush thread
    ///
    /// Spawns a tokio task that periodically flushes all writers. This is the core
    /// mechanism that solves the Phase 1 consume issue.
    ///
    /// ## Arguments
    ///
    /// - `interval`: How often to flush (default: 5 seconds)
    ///
    /// ## Returns
    ///
    /// A `tokio::task::JoinHandle` that can be used to wait for shutdown
    ///
    /// ## Example
    ///
    /// ```ignore
    /// // Start background flush every 5 seconds
    /// let handle = pool.start_background_flush(Duration::from_secs(5));
    ///
    /// // Later, during shutdown:
    /// handle.abort();
    /// ```
    pub fn start_background_flush(
        self: Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = time::interval(interval);
            ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;

                tracing::trace!("Background flush tick");

                let start = std::time::Instant::now();
                if let Err(e) = self.flush_all().await {
                    tracing::error!(
                        error = %e,
                        "Background flush failed"
                    );
                }
                let elapsed = start.elapsed();
                if elapsed.as_secs_f64() > 2.0 {
                    tracing::warn!(
                        elapsed_ms = elapsed.as_millis() as u64,
                        "Background flush cycle took >2s"
                    );
                }
            }
        })
    }

    /// Flush all writers to S3, then take per-org metadata snapshots and upload them.
    ///
    /// This enforces **snapshot ordering**: all buffered data reaches S3 *before*
    /// the metadata snapshot is taken, guaranteeing that the snapshot is always
    /// a subset of what exists in S3. On recovery, restoring the snapshot and
    /// running `reconcile_from_s3` fills any gap.
    ///
    /// Snapshots are org-scoped:
    /// - Default org → `_snapshots/{timestamp}.json.gz`
    /// - Non-default orgs → `org-{uuid}/_snapshots/{timestamp}.json.gz`
    pub async fn flush_all_and_snapshot(&self) -> Result<()> {
        // Step 1: Push all buffered data to S3
        self.flush_all().await?;

        // Step 2: Enumerate all organizations
        let mut org_prefixes: Vec<(String, String)> = Vec::new();
        if let Ok(orgs) = self.metadata_store.list_organizations().await {
            for org in orgs {
                org_prefixes.push((org.id.clone(), format!("org-{}/_snapshots", org.id)));
            }
        }
        // Always include legacy "_snapshots" prefix for the first org (backwards compat)
        if org_prefixes.is_empty() {
            // No orgs found — nothing to snapshot
        } else if !org_prefixes.iter().any(|(_, p)| p == "_snapshots") {
            org_prefixes.insert(0, (org_prefixes[0].0.clone(), "_snapshots".to_string()));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        // Step 3: Create per-org snapshots
        for (org_id, snapshot_prefix) in &org_prefixes {
            let backup = streamhouse_metadata::backup::MetadataBackup::from_store_for_org(
                &*self.metadata_store,
                org_id,
            )
            .await
            .map_err(|e| {
                crate::error::Error::SegmentError(format!(
                    "Snapshot metadata export failed for org {}: {}",
                    org_id, e
                ))
            })?;

            // Skip orgs with no topics
            if backup.topic_count() == 0 {
                continue;
            }

            let json = backup.to_json_compact().map_err(|e| {
                crate::error::Error::SegmentError(format!("Snapshot serialization failed: {}", e))
            })?;

            // Gzip compress
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(json.as_bytes()).map_err(|e| {
                crate::error::Error::SegmentError(format!("Snapshot compression failed: {}", e))
            })?;
            let compressed = encoder.finish().map_err(|e| {
                crate::error::Error::SegmentError(format!(
                    "Snapshot compression finalize failed: {}",
                    e
                ))
            })?;

            // Upload to {snapshot_prefix}/{timestamp}.json.gz
            let path = object_store::path::Path::from(format!(
                "{}/{}.json.gz",
                snapshot_prefix, timestamp
            ));

            #[allow(clippy::useless_conversion)]
            self.object_store
                .put(&path, bytes::Bytes::from(compressed).into())
                .await
                .map_err(|e| {
                    crate::error::Error::SegmentError(format!("Snapshot upload failed: {}", e))
                })?;

            tracing::info!(
                snapshot_path = %path,
                org_id = %org_id,
                topics = backup.topic_count(),
                "Metadata snapshot uploaded"
            );
        }

        Ok(())
    }

    /// Start a background loop that periodically calls `flush_all_and_snapshot()`.
    ///
    /// This is separate from the 5-second flush loop — snapshots are expensive
    /// (full metadata export + S3 upload) and run at a longer interval (default 1 hour).
    ///
    /// Configurable via `SNAPSHOT_INTERVAL_SECS` env var.
    pub fn start_background_snapshot(
        self: Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = time::interval(interval);
            ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            // Skip the initial immediate tick
            ticker.tick().await;

            loop {
                ticker.tick().await;

                tracing::debug!("Snapshot tick");

                match self.flush_all_and_snapshot().await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "Background snapshot failed"
                        );
                    }
                }
            }
        })
    }

    /// Get count of active writers (for monitoring)
    ///
    /// Returns the number of partition writers currently in the pool.
    /// Useful for metrics and debugging.
    pub async fn writer_count(&self) -> usize {
        self.writers.read().await.len()
    }

    /// Shutdown the pool, flushing all writers
    ///
    /// Call this during graceful shutdown to ensure all pending data is flushed
    /// to storage before the server exits.
    ///
    /// ## Example
    ///
    /// ```ignore
    /// // During shutdown
    /// pool.shutdown().await?;
    /// ```
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down writer pool, flushing all writers");

        self.flush_all().await?;

        let writer_count = self.writer_count().await;
        tracing::info!(writer_count = writer_count, "Writer pool shutdown complete");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;
    use streamhouse_metadata::{SqliteMetadataStore, TopicConfig};

    /// Helper to create an in-memory metadata store with a test topic
    async fn create_test_metadata(topic: &str, partition_count: u32) -> Arc<dyn MetadataStore> {
        let store = SqliteMetadataStore::new_in_memory().await.unwrap();
        store
            .ensure_organization(streamhouse_metadata::TEST_ORG_ID, "Test Org")
            .await
            .unwrap();
        store
            .create_topic_for_org(
                streamhouse_metadata::TEST_ORG_ID,
                TopicConfig {
                    name: topic.to_string(),
                    partition_count,
                    retention_ms: Some(86400000),
                    config: Default::default(),
                    cleanup_policy: Default::default(),
                },
            )
            .await
            .unwrap();
        Arc::new(store)
    }

    fn test_config() -> WriteConfig {
        WriteConfig {
            segment_max_size: 1024 * 1024,
            s3_bucket: "test-bucket".to_string(),
            s3_region: "us-east-1".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_writer_pool_creation() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());
        assert_eq!(pool.writer_count().await, 0);
    }

    #[tokio::test]
    async fn test_writer_count_increases_on_get_writer() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());
        let org = streamhouse_metadata::TEST_ORG_ID;

        assert_eq!(pool.writer_count().await, 0);

        let _w = pool.get_writer(org, "orders", 0).await.unwrap();
        assert_eq!(pool.writer_count().await, 1);

        let _w = pool.get_writer(org, "orders", 1).await.unwrap();
        assert_eq!(pool.writer_count().await, 2);
    }

    #[tokio::test]
    async fn test_get_writer_returns_same_writer_for_same_partition() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());
        let org = streamhouse_metadata::TEST_ORG_ID;

        let w1 = pool.get_writer(org, "orders", 0).await.unwrap();
        let w2 = pool.get_writer(org, "orders", 0).await.unwrap();

        // Should be the same Arc (same pointer)
        assert!(Arc::ptr_eq(&w1, &w2));
        assert_eq!(pool.writer_count().await, 1);
    }

    #[tokio::test]
    async fn test_get_writer_different_partitions() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());
        let org = streamhouse_metadata::TEST_ORG_ID;

        let w0 = pool.get_writer(org, "orders", 0).await.unwrap();
        let w1 = pool.get_writer(org, "orders", 1).await.unwrap();

        // Different partitions should produce different writers
        assert!(!Arc::ptr_eq(&w0, &w1));
        assert_eq!(pool.writer_count().await, 2);
    }

    #[tokio::test]
    async fn test_flush_all_empty_pool() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());

        // Flush on empty pool should succeed without error
        pool.flush_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_flush_all_with_writers() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());
        let org = streamhouse_metadata::TEST_ORG_ID;

        let _w0 = pool.get_writer(org, "orders", 0).await.unwrap();
        let _w1 = pool.get_writer(org, "orders", 1).await.unwrap();

        // Flush should succeed even with writers that have no data
        pool.flush_all().await.unwrap();
        assert_eq!(pool.writer_count().await, 2);
    }

    #[tokio::test]
    async fn test_shutdown_empty_pool() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());

        pool.shutdown().await.unwrap();
        assert_eq!(pool.writer_count().await, 0);
    }

    #[tokio::test]
    async fn test_shutdown_with_writers() {
        let metadata = create_test_metadata("orders", 3).await;
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let pool = WriterPool::new(metadata, object_store, test_config());

        let _w = pool
            .get_writer(streamhouse_metadata::TEST_ORG_ID, "orders", 0)
            .await
            .unwrap();

        // Shutdown should call flush_all and succeed
        pool.shutdown().await.unwrap();
        // Writers still exist after shutdown (pool just flushes, doesn't clear)
        assert_eq!(pool.writer_count().await, 1);
    }
}
