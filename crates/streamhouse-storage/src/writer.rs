//! Partition and Topic Writers
//!
//! This module implements the core write path components that manage buffering,
//! segment rolling, and S3 uploads.
//!
//! ## Components
//!
//! ### PartitionWriter
//! Manages writes to a single partition. Responsibilities:
//! - Buffers records in memory using SegmentWriter
//! - Rolls segments based on size and time thresholds
//! - Uploads completed segments to S3 with retry logic
//! - Updates metadata store with segment info and high watermark
//!
//! ### TopicWriter
//! Manages writes across all partitions for a topic. Responsibilities:
//! - Routes records to partitions (hash-based or round-robin)
//! - Coordinates concurrent writes across partitions
//! - Provides topic-level flush operations
//!
//! ## Write Flow
//!
//! ```text
//! append(record)
//!     ↓
//! SegmentWriter.append()  ← In-memory buffer
//!     ↓
//! should_roll_segment()?
//!     ↓ YES
//! finish_segment()        ← Compress + index
//!     ↓
//! upload_to_s3()          ← With retries
//!     ↓
//! update_metadata()       ← Register segment
//!     ↓
//! new_segment()           ← Start fresh
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::{PartitionWriter, WriteConfig};
//!
//! let writer = PartitionWriter::new(
//!     "orders".to_string(),
//!     0,  // partition_id
//!     object_store,
//!     metadata,
//!     config,
//! ).await?;
//!
//! // Append records
//! let offset = writer.append(
//!     Some(Bytes::from("user-123")),
//!     Bytes::from(r#"{"amount": 99.99}"#),
//!     timestamp,
//! ).await?;
//!
//! // Flush on shutdown
//! writer.flush().await?;
//! ```

use crate::{
    config::WriteConfig,
    error::{Error, Result},
    rate_limiter::S3Operation,
    segment::SegmentWriter,
    throttle::{ThrottleCoordinator, ThrottleDecision},
    wal::WAL,
};
use bytes::Bytes;
use object_store::ObjectStore;
use std::sync::Arc;
use streamhouse_core::{record::Record, segment::Compression};
use streamhouse_metadata::{MetadataStore, SegmentInfo};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;

/// Get current timestamp in milliseconds
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Manages writes to a single partition.
///
/// PartitionWriter handles all aspects of writing records to a partition:
/// - Buffers records in memory using `SegmentWriter`
/// - Rolls segments when size/time thresholds are exceeded
/// - Uploads completed segments to S3 with retry logic
/// - Updates metadata store with segment info and high watermark
///
/// # Thread Safety
///
/// Not Send/Sync - typically wrapped in Arc<Mutex<>> for concurrent access.
///
/// # Lifecycle
///
/// 1. **Create**: Initialize with current high watermark from metadata
/// 2. **Append**: Write records, automatically roll segments when needed
/// 3. **Flush**: Manually flush current segment (e.g., on shutdown)
/// 4. **Drop**: Segment is NOT auto-flushed - call flush() explicitly
///
/// # Performance
///
/// - In-memory buffering: ~2.26M records/sec write throughput
/// - S3 upload: Batched (64MB segments) with exponential backoff retries
/// - Metadata updates: One update per segment (not per record)
///
/// # Examples
///
/// ```ignore
/// let mut writer = PartitionWriter::new(
///     "orders".to_string(),
///     0,
///     object_store,
///     metadata,
///     config,
/// ).await?;
///
/// // Append records
/// for i in 0..10_000 {
///     writer.append(
///         Some(Bytes::from(format!("key-{}", i))),
///         Bytes::from("value data"),
///         now_ms() as u64,
///     ).await?;
/// }
///
/// // Flush on shutdown
/// writer.flush().await?;
/// ```
pub struct PartitionWriter {
    topic: String,
    partition_id: u32,

    // Current segment being written
    current_segment: SegmentWriter,
    current_offset: u64,
    segment_created_at: i64,
    segment_size_estimate: usize,

    // S3 and metadata
    object_store: Arc<dyn ObjectStore>,
    metadata: Arc<dyn MetadataStore>,

    // Write-Ahead Log for durability (optional)
    wal: Option<WAL>,

    // S3 Throttle Coordinator (optional)
    throttle: Option<ThrottleCoordinator>,

    // Stale WAL files from other agents to clean up after first S3 flush (Phase 12.4.5)
    stale_wal_files: Vec<std::path::PathBuf>,

    // Configuration
    config: WriteConfig,
}

impl PartitionWriter {
    /// Create a new partition writer for a specific partition.
    ///
    /// Initializes the writer with the current high watermark from the metadata store.
    /// The first record appended will be assigned `high_watermark` as its offset.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID (must exist in metadata)
    /// * `object_store` - S3-compatible object store for uploading segments
    /// * `metadata` - Metadata store for tracking segments and offsets
    /// * `config` - Write configuration (segment size, S3 bucket, etc.)
    ///
    /// # Returns
    ///
    /// A new `PartitionWriter` ready to accept records.
    ///
    /// # Errors
    ///
    /// - `PartitionNotFound`: Partition doesn't exist in metadata
    /// - `MetadataError`: Failed to query metadata store
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let writer = PartitionWriter::new(
    ///     "orders".to_string(),
    ///     0,
    ///     Arc::new(S3ObjectStore::new()),
    ///     Arc::new(SqliteMetadataStore::new("metadata.db").await?),
    ///     WriteConfig::default(),
    /// ).await?;
    /// ```
    pub async fn new(
        topic: String,
        partition_id: u32,
        object_store: Arc<dyn ObjectStore>,
        metadata: Arc<dyn MetadataStore>,
        config: WriteConfig,
    ) -> Result<Self> {
        // Get current high watermark from metadata
        let partition = metadata
            .get_partition(&topic, partition_id)
            .await?
            .ok_or_else(|| Error::PartitionNotFound {
                topic: topic.clone(),
                partition: partition_id,
            })?;

        let mut current_offset = partition.high_watermark;

        let mut current_segment = SegmentWriter::new(Compression::Lz4);

        // Initialize WAL if configured
        let mut stale_wal_files = Vec::new();
        let wal = if let Some(ref wal_config) = config.wal_config {
            // Phase 12.4.5: Shared WAL recovery
            // If agent_id is configured, scan for WAL files from other agents
            let other_agent_records = if let Some(ref agent_id) = wal_config.agent_id {
                match WAL::recover_partition(
                    &wal_config.directory,
                    &topic,
                    partition_id,
                    agent_id,
                )
                .await
                {
                    Ok((records, stale_paths)) => {
                        stale_wal_files = stale_paths;
                        records
                    }
                    Err(e) => {
                        tracing::warn!(
                            topic = %topic,
                            partition = partition_id,
                            error = %e,
                            "Failed to scan for shared WAL files, continuing with own WAL only"
                        );
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };

            // Open this agent's WAL (creates new file or opens existing)
            let wal = WAL::open(&topic, partition_id, wal_config.clone()).await?;

            // Recover from this agent's own WAL (same-node restart case)
            let own_records = wal.recover().await?;

            // Merge: other agents' records first, then own records
            let total_other = other_agent_records.len();
            let total_own = own_records.len();
            let all_recovered: Vec<_> = other_agent_records
                .into_iter()
                .chain(own_records)
                .collect();

            if !all_recovered.is_empty() {
                tracing::info!(
                    topic = %topic,
                    partition = partition_id,
                    from_other_agents = total_other,
                    from_own_wal = total_own,
                    total = all_recovered.len(),
                    stale_files = stale_wal_files.len(),
                    "Recovering unflushed records from WAL"
                );

                // Replay records into current segment
                for wal_record in all_recovered {
                    let record = Record::new(
                        current_offset,
                        wal_record.timestamp,
                        wal_record.key,
                        wal_record.value,
                    );
                    current_offset += 1;
                    current_segment
                        .append(&record)
                        .map_err(|e| Error::SegmentError(e.to_string()))?;
                }

                tracing::info!(
                    topic = %topic,
                    partition = partition_id,
                    "WAL recovery complete"
                );
            }

            Some(wal)
        } else {
            None
        };

        // Initialize throttle coordinator if configured
        let throttle = config
            .throttle_config
            .as_ref()
            .map(|throttle_config| ThrottleCoordinator::new(throttle_config.clone()));

        Ok(Self {
            topic,
            partition_id,
            current_segment,
            current_offset,
            segment_created_at: now_ms(),
            segment_size_estimate: 0,
            object_store,
            metadata,
            wal,
            throttle,
            stale_wal_files,
            config,
        })
    }

    /// Append a record to the partition and return its assigned offset.
    ///
    /// Records are buffered in memory until the segment reaches size or age thresholds,
    /// at which point it's automatically flushed to S3.
    ///
    /// # Arguments
    ///
    /// * `key` - Optional record key (used for compaction, routing, etc.)
    /// * `value` - Record payload (the actual data)
    /// * `timestamp` - Record timestamp in milliseconds since Unix epoch
    ///
    /// # Returns
    ///
    /// The offset assigned to this record. Offsets are monotonically increasing
    /// starting from the partition's high watermark.
    ///
    /// # Errors
    ///
    /// - `SegmentError`: Failed to append to current segment
    /// - `S3UploadFailed`: Segment roll triggered upload that failed
    /// - `MetadataError`: Failed to update metadata after segment roll
    ///
    /// # Automatic Segment Rolling
    ///
    /// Segments are automatically rolled (finalized and uploaded) when:
    /// - Size exceeds `config.segment_max_size` (default 64MB)
    /// - Age exceeds `config.segment_max_age_ms` (default 10 minutes)
    ///
    /// # Performance
    ///
    /// - **In-memory append**: ~2.26M records/sec (no I/O)
    /// - **Segment roll**: Triggers S3 upload (~200ms for 64MB segment)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Append with key
    /// let offset = writer.append(
    ///     Some(Bytes::from("user-123")),
    ///     Bytes::from(r#"{"amount": 99.99}"#),
    ///     1234567890000,
    /// ).await?;
    ///
    /// println!("Wrote record at offset {}", offset);
    /// ```
    #[tracing::instrument(skip(self, key, value), fields(topic = %self.topic, partition = %self.partition_id, value_len = value.len()))]
    pub async fn append(
        &mut self,
        key: Option<Bytes>,
        value: Bytes,
        timestamp: u64,
    ) -> Result<u64> {
        let offset = self.current_offset;
        self.current_offset += 1;

        // Write to WAL first for durability (if enabled)
        if let Some(ref wal) = self.wal {
            wal.append(key.as_deref(), value.as_ref()).await?;
        }

        // Create Record
        let record = Record::new(offset, timestamp, key, value);

        // Estimate size
        self.segment_size_estimate += record.estimated_size();

        // Append to current segment
        self.current_segment
            .append(&record)
            .map_err(|e| Error::SegmentError(e.to_string()))?;

        // Check if we should roll the segment
        if self.should_roll_segment() {
            self.roll_segment().await?;
        }

        Ok(offset)
    }

    /// Append a batch of records, using a single WAL write for the entire batch.
    ///
    /// This amortizes WAL channel overhead: instead of N individual channel sends,
    /// all records are serialized into one buffer and sent as a single message.
    /// Segment appends are still per-record to maintain offset assignment semantics.
    pub async fn append_batch(
        &mut self,
        records: &[(Option<Bytes>, Bytes, u64)], // (key, value, timestamp)
    ) -> Result<Vec<u64>> {
        if records.is_empty() {
            return Ok(vec![]);
        }

        // Assign offsets upfront
        let base_offset = self.current_offset;
        let offsets: Vec<u64> = (0..records.len() as u64)
            .map(|i| base_offset + i)
            .collect();
        self.current_offset += records.len() as u64;

        // Single WAL write for the entire batch
        if let Some(ref wal) = self.wal {
            let wal_records: Vec<(Option<&[u8]>, &[u8])> = records
                .iter()
                .map(|(key, value, _ts)| (key.as_deref(), value.as_ref()))
                .collect();
            wal.append_batch(&wal_records).await?;
        }

        // Append each record to the segment (maintains offset/index semantics)
        for (i, (key, value, timestamp)) in records.iter().enumerate() {
            let record = Record::new(offsets[i], *timestamp, key.clone(), value.clone());
            self.segment_size_estimate += record.estimated_size();
            self.current_segment
                .append(&record)
                .map_err(|e| Error::SegmentError(e.to_string()))?;
        }

        // Check if we should roll the segment after the batch
        if self.should_roll_segment() {
            self.roll_segment().await?;
        }

        Ok(offsets)
    }

    /// Check if we should create a new segment
    fn should_roll_segment(&self) -> bool {
        // Check size threshold
        if self.segment_size_estimate >= self.config.segment_max_size {
            return true;
        }

        // Check age threshold
        let age_ms = (now_ms() - self.segment_created_at) as u64;
        if age_ms >= self.config.segment_max_age_ms {
            return true;
        }

        false
    }

    /// Finish current segment and start a new one
    async fn roll_segment(&mut self) -> Result<()> {
        // Take ownership of current segment
        let segment = std::mem::replace(
            &mut self.current_segment,
            SegmentWriter::new(Compression::Lz4),
        );

        let base_offset = segment
            .base_offset()
            .ok_or_else(|| Error::SegmentError("Cannot roll empty segment".to_string()))?;
        let end_offset = segment
            .last_offset()
            .ok_or_else(|| Error::SegmentError("Cannot roll empty segment".to_string()))?;
        let record_count = segment.record_count();

        // Finish the segment (compress, add index, footer)
        let segment_bytes = segment
            .finish()
            .map_err(|e| Error::SegmentError(e.to_string()))?;
        let size_bytes = segment_bytes.len() as u64;

        // Generate S3 path
        let s3_key = format!(
            "data/{}/{}/{:020}.seg",
            self.topic, self.partition_id, base_offset
        );

        // Record segment write metrics (Phase 7.1d)
        streamhouse_observability::metrics::SEGMENT_WRITES_TOTAL
            .with_label_values(&[&self.topic, &self.partition_id.to_string()])
            .inc();

        // Upload to S3 with retries
        self.upload_to_s3(&s3_key, Bytes::from(segment_bytes))
            .await?;

        // Record segment flush metrics (Phase 7.1d)
        streamhouse_observability::metrics::SEGMENT_FLUSHES_TOTAL
            .with_label_values(&[&self.topic, &self.partition_id.to_string()])
            .inc();

        // Record segment in metadata
        let segment_info = SegmentInfo {
            id: format!("{}-{}-{}", self.topic, self.partition_id, base_offset),
            topic: self.topic.clone(),
            partition_id: self.partition_id,
            base_offset,
            end_offset,
            record_count,
            size_bytes,
            s3_bucket: self.config.s3_bucket.clone(),
            s3_key: s3_key.clone(),
            created_at: now_ms(),
        };

        self.metadata.add_segment(segment_info).await?;

        // Update high watermark
        self.metadata
            .update_high_watermark(&self.topic, self.partition_id, end_offset + 1)
            .await?;

        // Truncate WAL now that data is safely in S3
        if let Some(ref wal) = self.wal {
            wal.truncate().await?;
            tracing::debug!(
                topic = %self.topic,
                partition = self.partition_id,
                "WAL truncated after successful S3 upload"
            );
        }

        // Phase 12.4.5: Clean up stale WAL files from other agents
        // Only safe to delete after recovered records are in S3
        if !self.stale_wal_files.is_empty() {
            for stale_path in self.stale_wal_files.drain(..) {
                match tokio::fs::remove_file(&stale_path).await {
                    Ok(()) => {
                        tracing::info!(
                            topic = %self.topic,
                            partition = self.partition_id,
                            path = ?stale_path,
                            "Removed stale WAL file from another agent"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            topic = %self.topic,
                            partition = self.partition_id,
                            path = ?stale_path,
                            error = %e,
                            "Failed to remove stale WAL file"
                        );
                    }
                }
            }
        }

        // Reset state for new segment
        self.segment_created_at = now_ms();
        self.segment_size_estimate = 0;

        tracing::info!(
            topic = %self.topic,
            partition = self.partition_id,
            base_offset,
            end_offset,
            size_bytes,
            s3_key = %s3_key,
            "Segment rolled and uploaded to S3"
        );

        Ok(())
    }

    /// Upload segment to S3 with exponential backoff retry and throttling protection.
    /// Uses multipart upload for large segments (Phase 8.4a optimization).
    async fn upload_to_s3(&self, key: &str, data: Bytes) -> Result<()> {
        let path = object_store::path::Path::from(key);

        // Use multipart upload for large segments (Phase 8.4a)
        if data.len() >= self.config.multipart_threshold {
            return self.upload_multipart(&path, data).await;
        }

        for attempt in 0..self.config.s3_upload_retries {
            // Check throttle before attempting upload (Phase 12.4.2)
            if let Some(ref throttle) = self.throttle {
                match throttle.acquire(S3Operation::Put).await {
                    ThrottleDecision::Allow => {
                        // Allowed to proceed
                    }
                    ThrottleDecision::RateLimited => {
                        tracing::warn!(
                            key = %key,
                            attempt = attempt + 1,
                            "S3 PUT rate limited - backpressure applied"
                        );
                        return Err(Error::S3RateLimited);
                    }
                    ThrottleDecision::CircuitOpen => {
                        tracing::error!(
                            key = %key,
                            attempt = attempt + 1,
                            "S3 circuit breaker open - rejecting request"
                        );
                        return Err(Error::S3CircuitOpen);
                    }
                }
            }

            // Record S3 request metric (Phase 7.1d)
            streamhouse_observability::metrics::S3_REQUESTS_TOTAL
                .with_label_values(&["PUT"])
                .inc();

            // Measure S3 latency
            let start = std::time::Instant::now();

            match self.object_store.put(&path, data.clone()).await {
                Ok(_) => {
                    // Record S3 latency on success (Phase 7.1d)
                    let duration = start.elapsed().as_secs_f64();
                    streamhouse_observability::metrics::S3_LATENCY
                        .with_label_values(&["PUT"])
                        .observe(duration);

                    // Report success to throttle coordinator (Phase 12.4.2)
                    if let Some(ref throttle) = self.throttle {
                        throttle.report_result(S3Operation::Put, true, false).await;
                    }

                    tracing::debug!(
                        key = %key,
                        size = data.len(),
                        attempt = attempt + 1,
                        "Successfully uploaded segment to S3"
                    );
                    return Ok(());
                }
                Err(e) if attempt < self.config.s3_upload_retries - 1 => {
                    // Record S3 error metric (Phase 7.1d)
                    streamhouse_observability::metrics::S3_ERRORS_TOTAL
                        .with_label_values(&["PUT", "retry"])
                        .inc();

                    // Check if this is a throttle error (503 SlowDown) (Phase 12.4.2)
                    let is_throttle_error =
                        e.to_string().contains("SlowDown") || e.to_string().contains("503");

                    // Report failure to throttle coordinator (Phase 12.4.2)
                    if let Some(ref throttle) = self.throttle {
                        throttle
                            .report_result(S3Operation::Put, false, is_throttle_error)
                            .await;
                    }

                    let backoff_ms = 100 * 2_u64.pow(attempt);
                    tracing::warn!(
                        key = %key,
                        attempt = attempt + 1,
                        backoff_ms,
                        error = %e,
                        is_throttle_error,
                        "S3 upload failed, retrying"
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                }
                Err(e) => {
                    // Record S3 error metric (Phase 7.1d)
                    streamhouse_observability::metrics::S3_ERRORS_TOTAL
                        .with_label_values(&["PUT", "failed"])
                        .inc();

                    // Check if this is a throttle error (503 SlowDown) (Phase 12.4.2)
                    let is_throttle_error =
                        e.to_string().contains("SlowDown") || e.to_string().contains("503");

                    // Report final failure to throttle coordinator (Phase 12.4.2)
                    if let Some(ref throttle) = self.throttle {
                        throttle
                            .report_result(S3Operation::Put, false, is_throttle_error)
                            .await;
                    }

                    tracing::error!(
                        key = %key,
                        error = %e,
                        is_throttle_error,
                        "S3 upload failed after all retries"
                    );
                    return Err(Error::S3UploadFailed(e.to_string()));
                }
            }
        }

        unreachable!()
    }

    /// Upload segment using multipart upload for large files (Phase 8.4a).
    ///
    /// Benefits:
    /// - Better throughput for large segments
    /// - Streaming upload (memory efficient)
    ///
    /// Note: In object_store 0.9, put_multipart returns an AsyncWrite stream.
    /// For true parallel uploads, we'd need to chunk and spawn tasks.
    async fn upload_multipart(&self, path: &object_store::path::Path, data: Bytes) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let part_size = self.config.multipart_part_size;
        let num_parts = data.len().div_ceil(part_size);

        tracing::debug!(
            path = %path,
            size = data.len(),
            part_size,
            num_parts,
            "Starting multipart upload (Phase 8.4a)"
        );

        // Check throttle before starting multipart upload
        if let Some(ref throttle) = self.throttle {
            match throttle.acquire(S3Operation::Put).await {
                crate::throttle::ThrottleDecision::Allow => {}
                crate::throttle::ThrottleDecision::RateLimited => {
                    return Err(Error::S3RateLimited);
                }
                crate::throttle::ThrottleDecision::CircuitOpen => {
                    return Err(Error::S3CircuitOpen);
                }
            }
        }

        let start = std::time::Instant::now();

        // Start multipart upload - returns (multipart_id, writer)
        let (_multipart_id, mut writer) = match self.object_store.put_multipart(path).await {
            Ok(upload) => upload,
            Err(e) => {
                if let Some(ref throttle) = self.throttle {
                    throttle.report_result(S3Operation::Put, false, false).await;
                }
                return Err(Error::S3UploadFailed(format!(
                    "Failed to start multipart: {}",
                    e
                )));
            }
        };

        // Write data in chunks (streams efficiently to S3)
        for (i, chunk) in data.chunks(part_size).enumerate() {
            tracing::trace!(part = i, size = chunk.len(), "Writing part");

            if let Err(e) = writer.write_all(chunk).await {
                if let Some(ref throttle) = self.throttle {
                    throttle.report_result(S3Operation::Put, false, false).await;
                }
                return Err(Error::S3UploadFailed(format!("Part write failed: {}", e)));
            }
        }

        // Complete the multipart upload by shutting down the writer
        match writer.shutdown().await {
            Ok(_) => {
                let duration = start.elapsed().as_secs_f64();
                streamhouse_observability::metrics::S3_LATENCY
                    .with_label_values(&["PUT_MULTIPART"])
                    .observe(duration);

                if let Some(ref throttle) = self.throttle {
                    throttle.report_result(S3Operation::Put, true, false).await;
                }

                tracing::info!(
                    path = %path,
                    size = data.len(),
                    num_parts,
                    duration_secs = duration,
                    "Multipart upload completed successfully (Phase 8.4a)"
                );
                Ok(())
            }
            Err(e) => {
                if let Some(ref throttle) = self.throttle {
                    throttle.report_result(S3Operation::Put, false, false).await;
                }
                Err(Error::S3UploadFailed(format!(
                    "Multipart upload failed: {}",
                    e
                )))
            }
        }
    }

    /// Flush any buffered data to S3.
    ///
    /// Manually triggers a segment roll if there are any buffered records.
    /// This should be called during graceful shutdown to ensure no data is lost.
    ///
    /// # Behavior
    ///
    /// - If current segment is empty: No-op, returns immediately
    /// - If current segment has records: Rolls the segment (compress, upload, update metadata)
    ///
    /// # Errors
    ///
    /// - `S3UploadFailed`: Failed to upload segment to S3
    /// - `MetadataError`: Failed to register segment in metadata store
    /// - `SegmentError`: Failed to finalize segment
    ///
    /// # Important
    ///
    /// `PartitionWriter` does NOT auto-flush on drop. You MUST call `flush()` explicitly
    /// before dropping the writer, otherwise buffered records will be lost.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Graceful shutdown
    /// writer.flush().await?;
    /// drop(writer);
    ///
    /// // Or with Arc<Mutex<>>
    /// {
    ///     let mut writer = writer_mutex.lock().await;
    ///     writer.flush().await?;
    /// }
    /// ```
    pub async fn flush(&mut self) -> Result<()> {
        // Only flush if segment should roll (meets size or age threshold)
        if self.current_segment.record_count() > 0 && self.should_roll_segment() {
            self.roll_segment().await?;
        }
        Ok(())
    }

    /// Force flush all buffered data to S3 immediately for durable acknowledgment.
    ///
    /// Unlike `flush()`, this method immediately rolls the segment regardless of
    /// size or age thresholds. Use this for ACK_DURABLE mode where the producer
    /// must wait for S3 persistence before receiving acknowledgment.
    ///
    /// # Latency
    ///
    /// This operation typically takes ~150ms as it performs a synchronous S3 upload.
    /// Only use this when durability guarantees are more important than throughput.
    ///
    /// # Behavior
    ///
    /// - If current segment is empty: No-op, returns immediately
    /// - If current segment has records: Forces immediate segment roll and S3 upload
    ///
    /// # Returns
    ///
    /// The end offset of the flushed segment, or None if segment was empty.
    ///
    /// # Errors
    ///
    /// - `S3UploadFailed`: Failed to upload segment to S3
    /// - `MetadataError`: Failed to register segment in metadata store
    /// - `SegmentError`: Failed to finalize segment
    pub async fn flush_durable(&mut self) -> Result<Option<u64>> {
        if self.current_segment.record_count() > 0 {
            let end_offset = self.current_segment.last_offset();
            self.roll_segment().await?;
            Ok(end_offset)
        } else {
            Ok(None)
        }
    }
}

// ============================================================================
// Batched Durable Flush (ACK_DURABLE optimization)
// ============================================================================
//
// Mirrors the WAL group commit pattern (wal.rs WalWriter::run):
//   Callers ─→ [mpsc channel] ─→ DurableFlushTask ─→ roll_segment ─→ S3
//
// Instead of each ACK_DURABLE produce call doing its own S3 upload,
// writes are batched over a time window (~200ms) and flushed together.

/// Commands sent to the durable flush background task
enum DurableFlushCmd {
    /// A produce call wants to wait for the next S3 flush
    WaitForFlush(oneshot::Sender<std::result::Result<(), String>>),
    /// Shutdown the flush task
    Shutdown,
}

/// Handle to a running DurableFlushTask for one partition
pub struct DurableFlushHandle {
    cmd_tx: mpsc::Sender<DurableFlushCmd>,
    task: JoinHandle<()>,
}

impl DurableFlushHandle {
    /// Spawn a new durable flush task for a partition writer
    pub fn spawn(
        writer: Arc<Mutex<PartitionWriter>>,
        max_age_ms: u64,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(16_384);

        let task = tokio::spawn(async move {
            let mut runner = DurableFlushTask {
                writer,
                cmd_rx,
                max_age_ms,
            };
            runner.run().await;
        });

        Self { cmd_tx, task }
    }

    /// Request a durable flush and wait for S3 confirmation
    pub async fn request_flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(DurableFlushCmd::WaitForFlush(tx))
            .await
            .map_err(|_| Error::SegmentError("Durable flush task closed".to_string()))?;
        rx.await
            .map_err(|_| Error::SegmentError("Durable flush task dropped".to_string()))?
            .map_err(|e| Error::SegmentError(format!("Durable flush failed: {}", e)))
    }

    /// Shutdown the flush task
    pub async fn shutdown(self) {
        let _ = self.cmd_tx.send(DurableFlushCmd::Shutdown).await;
        let _ = self.task.await;
    }
}

/// Background task that batches ACK_DURABLE writes and flushes to S3
struct DurableFlushTask {
    writer: Arc<Mutex<PartitionWriter>>,
    cmd_rx: mpsc::Receiver<DurableFlushCmd>,
    max_age_ms: u64,
}

impl DurableFlushTask {
    /// Main event loop — mirrors WalWriter::run() pattern from wal.rs
    async fn run(&mut self) {
        let batch_timeout = std::time::Duration::from_millis(self.max_age_ms);

        loop {
            // Step 1: Wait for first waiter (block indefinitely if no one waiting)
            let first = match self.cmd_rx.recv().await {
                Some(cmd) => cmd,
                None => break, // channel closed
            };

            let mut waiters: Vec<oneshot::Sender<std::result::Result<(), String>>> = Vec::new();

            match first {
                DurableFlushCmd::WaitForFlush(tx) => waiters.push(tx),
                DurableFlushCmd::Shutdown => break,
            }

            // Step 2: Wait for batch window, collecting more waiters
            let deadline = tokio::time::Instant::now() + batch_timeout;
            loop {
                match tokio::time::timeout_at(deadline, self.cmd_rx.recv()).await {
                    Ok(Some(DurableFlushCmd::WaitForFlush(tx))) => waiters.push(tx),
                    Ok(Some(DurableFlushCmd::Shutdown)) => {
                        // Flush remaining waiters before shutdown
                        self.do_flush(&mut waiters).await;
                        return;
                    }
                    Ok(None) => {
                        // Channel closed
                        self.do_flush(&mut waiters).await;
                        return;
                    }
                    Err(_) => break, // Timeout — time to flush
                }
            }

            // Step 3: Drain any remaining commands (non-blocking)
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                match cmd {
                    DurableFlushCmd::WaitForFlush(tx) => waiters.push(tx),
                    DurableFlushCmd::Shutdown => {
                        self.do_flush(&mut waiters).await;
                        return;
                    }
                }
            }

            // Step 4: Flush to S3 and notify all waiters
            self.do_flush(&mut waiters).await;
        }
    }

    /// Acquire writer lock, call flush_durable(), resolve all waiters
    async fn do_flush(&self, waiters: &mut Vec<oneshot::Sender<std::result::Result<(), String>>>) {
        if waiters.is_empty() {
            return;
        }

        let result = {
            let mut writer_guard = self.writer.lock().await;
            writer_guard.flush_durable().await
        };

        let response = match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e.to_string()),
        };

        let waiter_count = waiters.len();
        for waiter in waiters.drain(..) {
            let _ = waiter.send(response.clone());
        }

        tracing::debug!(
            waiter_count,
            "Batched durable flush completed"
        );
    }
}

/// Manages writes across multiple partitions for a single topic
pub struct TopicWriter {
    #[allow(dead_code)]
    topic: String,
    partitions: Vec<Arc<Mutex<PartitionWriter>>>,
    round_robin_counter: Arc<std::sync::atomic::AtomicU32>,
}

impl TopicWriter {
    /// Create a new topic writer
    pub async fn new(
        topic: String,
        partition_count: u32,
        object_store: Arc<dyn ObjectStore>,
        metadata: Arc<dyn MetadataStore>,
        config: WriteConfig,
    ) -> Result<Self> {
        let mut partitions = Vec::new();

        for partition_id in 0..partition_count {
            let writer = PartitionWriter::new(
                topic.clone(),
                partition_id,
                object_store.clone(),
                metadata.clone(),
                config.clone(),
            )
            .await?;

            partitions.push(Arc::new(Mutex::new(writer)));
        }

        Ok(Self {
            topic,
            partitions,
            round_robin_counter: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        })
    }

    /// Append a record, automatically selecting partition
    #[tracing::instrument(skip(self, key, value), fields(topic = %self.topic, value_len = value.len()))]
    pub async fn append(
        &self,
        key: Option<Bytes>,
        value: Bytes,
        timestamp: Option<u64>,
    ) -> Result<(u32, u64)> {
        // Determine partition
        let partition_id = self.select_partition(&key);

        // Append to partition
        let timestamp = timestamp.unwrap_or_else(|| now_ms() as u64);
        let mut writer = self.partitions[partition_id as usize].lock().await;
        let offset = writer.append(key, value, timestamp).await?;

        Ok((partition_id, offset))
    }

    /// Select partition based on key (or round-robin if no key)
    fn select_partition(&self, key: &Option<Bytes>) -> u32 {
        match key {
            Some(k) => {
                // Hash the key to determine partition
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                k.hash(&mut hasher);
                let hash = hasher.finish();
                (hash % self.partitions.len() as u64) as u32
            }
            None => {
                // Round-robin
                let partition = self
                    .round_robin_counter
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                partition % self.partitions.len() as u32
            }
        }
    }

    /// Flush all partitions
    pub async fn flush(&self) -> Result<()> {
        for partition in &self.partitions {
            let mut writer = partition.lock().await;
            writer.flush().await?;
        }
        Ok(())
    }
}
