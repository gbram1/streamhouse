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
    segment::SegmentWriter,
};
use bytes::Bytes;
use object_store::ObjectStore;
use std::sync::Arc;
use streamhouse_core::{record::Record, segment::Compression};
use streamhouse_metadata::{MetadataStore, SegmentInfo};
use tokio::sync::Mutex;

/// Get current timestamp in milliseconds
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Manages writes to a single partition
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

    // Configuration
    config: WriteConfig,
}

impl PartitionWriter {
    /// Create a new partition writer
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

        let current_offset = partition.high_watermark;

        let current_segment = SegmentWriter::new(Compression::Lz4);

        Ok(Self {
            topic,
            partition_id,
            current_segment,
            current_offset,
            segment_created_at: now_ms(),
            segment_size_estimate: 0,
            object_store,
            metadata,
            config,
        })
    }

    /// Append a record and return its offset
    pub async fn append(
        &mut self,
        key: Option<Bytes>,
        value: Bytes,
        timestamp: u64,
    ) -> Result<u64> {
        let offset = self.current_offset;
        self.current_offset += 1;

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

        // Upload to S3 with retries
        self.upload_to_s3(&s3_key, Bytes::from(segment_bytes))
            .await?;

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

    /// Upload segment to S3 with exponential backoff retry
    async fn upload_to_s3(&self, key: &str, data: Bytes) -> Result<()> {
        let path = object_store::path::Path::from(key);

        for attempt in 0..self.config.s3_upload_retries {
            match self.object_store.put(&path, data.clone()).await {
                Ok(_) => {
                    tracing::debug!(
                        key = %key,
                        size = data.len(),
                        attempt = attempt + 1,
                        "Successfully uploaded segment to S3"
                    );
                    return Ok(());
                }
                Err(e) if attempt < self.config.s3_upload_retries - 1 => {
                    let backoff_ms = 100 * 2_u64.pow(attempt);
                    tracing::warn!(
                        key = %key,
                        attempt = attempt + 1,
                        backoff_ms,
                        error = %e,
                        "S3 upload failed, retrying"
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                }
                Err(e) => {
                    tracing::error!(
                        key = %key,
                        error = %e,
                        "S3 upload failed after all retries"
                    );
                    return Err(Error::S3UploadFailed(e.to_string()));
                }
            }
        }

        unreachable!()
    }

    /// Flush any buffered data (called on shutdown)
    pub async fn flush(&mut self) -> Result<()> {
        if self.current_segment.record_count() > 0 {
            self.roll_segment().await?;
        }
        Ok(())
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
