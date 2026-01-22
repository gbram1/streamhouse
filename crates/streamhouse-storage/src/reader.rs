//! Partition Reader with Caching and Prefetching
//!
//! This module implements efficient reading from S3 with aggressive caching.
//!
//! ## The Problem
//!
//! S3 has high latency (~50-200ms per GET). If we download on every read:
//! - Consumer waits 50-200ms per segment
//! - Poor user experience
//! - Wasted network bandwidth
//! - Higher S3 costs
//!
//! ## The Solution
//!
//! PartitionReader uses three techniques to achieve <10ms p99 latency:
//!
//! 1. **Caching**: Keep recently accessed segments in local disk cache
//! 2. **Prefetching**: Download next segment in background during sequential reads
//! 3. **Metadata lookups**: Use index to find segments instantly
//!
//! ## Read Flow
//!
//! ```text
//! read(offset=50000)
//!     ↓
//! Query metadata → Which segment?
//!     ↓
//! Check cache → orders-0-00000000000000000000
//!     ↓
//! Cache HIT (80% of time) → Return in <1ms ✅
//!     ↓
//! Cache MISS (20% of time)
//!     ↓
//! Download from S3 (~50ms)
//!     ↓
//! Cache for next time
//!     ↓
//! Background: Prefetch next segment
//!     ↓
//! Return records
//! ```
//!
//! ## Prefetching Strategy
//!
//! When consumer reads entire batch (max_records), we assume sequential access:
//! - Spawn background task to download next segment
//! - By the time consumer finishes current batch, next is ready
//! - Hides S3 latency completely for sequential reads
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::{PartitionReader, SegmentCache};
//!
//! let cache = Arc::new(SegmentCache::new("/tmp/cache", 1024*1024*1024)?);
//!
//! let reader = PartitionReader::new(
//!     "orders".to_string(),
//!     0,  // partition_id
//!     metadata,
//!     object_store,
//!     cache,
//! );
//!
//! // Read from offset
//! let result = reader.read(50000, 1000).await?;
//! for record in result.records {
//!     println!("Offset {}: {:?}", record.offset, record.value);
//! }
//! ```

use crate::{cache::SegmentCache, error::{Error, Result}, segment::SegmentReader};
use bytes::Bytes;
use std::sync::Arc;
use streamhouse_core::record::Record;
use streamhouse_metadata::{MetadataStore, SegmentInfo};
use object_store::ObjectStore;

/// Reads records from a single partition with caching
pub struct PartitionReader {
    topic: String,
    partition_id: u32,
    metadata: Arc<dyn MetadataStore>,
    object_store: Arc<dyn ObjectStore>,
    cache: Arc<SegmentCache>,
}

impl PartitionReader {
    /// Create a new partition reader
    pub fn new(
        topic: String,
        partition_id: u32,
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn ObjectStore>,
        cache: Arc<SegmentCache>,
    ) -> Self {
        Self {
            topic,
            partition_id,
            metadata,
            object_store,
            cache,
        }
    }

    /// Read records starting from offset
    ///
    /// Returns up to `max_records` records, or fewer if end of segment reached.
    /// Automatically caches segments and prefetches next segment for sequential reads.
    pub async fn read(&self, start_offset: u64, max_records: usize) -> Result<ReadResult> {
        // Find segment containing start_offset
        let segment_info = self
            .metadata
            .find_segment_for_offset(&self.topic, self.partition_id, start_offset)
            .await?
            .ok_or_else(|| Error::OffsetNotFound(start_offset))?;

        // Get segment data (from cache or S3)
        let segment_data = self.get_segment(&segment_info).await?;

        // Open segment reader
        let reader = SegmentReader::new(segment_data)
            .map_err(|e| Error::SegmentError(e.to_string()))?;

        // Read records from offset
        let mut records = reader
            .read_from_offset(start_offset)
            .map_err(|e| Error::SegmentError(e.to_string()))?;

        // Limit to max_records
        if records.len() > max_records {
            records.truncate(max_records);
        }

        // If we read a full batch, prefetch next segment
        if records.len() == max_records {
            self.prefetch_next_segment(&segment_info).await;
        }

        // Get high watermark from metadata
        let partition = self
            .metadata
            .get_partition(&self.topic, self.partition_id)
            .await?
            .ok_or_else(|| Error::PartitionNotFound {
                topic: self.topic.clone(),
                partition: self.partition_id,
            })?;

        Ok(ReadResult {
            records,
            high_watermark: partition.high_watermark,
        })
    }

    /// Get segment data (cache-aware)
    async fn get_segment(&self, info: &SegmentInfo) -> Result<Bytes> {
        let cache_key = self.cache_key(info);

        // Try cache first
        if let Some(data) = self.cache.get(&cache_key).await? {
            tracing::debug!(
                topic = %self.topic,
                partition = self.partition_id,
                base_offset = info.base_offset,
                "Segment cache hit"
            );
            return Ok(data);
        }

        // Cache miss - download from S3
        tracing::debug!(
            topic = %self.topic,
            partition = self.partition_id,
            base_offset = info.base_offset,
            s3_key = %info.s3_key,
            "Segment cache miss, downloading from S3"
        );

        let path = object_store::path::Path::from(info.s3_key.as_str());
        let result = self.object_store.get(&path).await?;
        let data = result.bytes().await?;

        // Cache for future reads
        self.cache.put(&cache_key, data.clone()).await?;

        Ok(data)
    }

    /// Prefetch next segment in background (for sequential reads)
    async fn prefetch_next_segment(&self, current: &SegmentInfo) {
        let next_offset = current.end_offset + 1;
        let topic = self.topic.clone();
        let partition_id = self.partition_id;
        let metadata = self.metadata.clone();
        let object_store = self.object_store.clone();
        let cache = self.cache.clone();

        tokio::spawn(async move {
            // Find next segment
            if let Ok(Some(next_segment)) = metadata
                .find_segment_for_offset(&topic, partition_id, next_offset)
                .await
            {
                let cache_key = format!("{}-{}-{}", topic, partition_id, next_segment.base_offset);

                // Check if already cached
                if let Ok(Some(_)) = cache.get(&cache_key).await {
                    return; // Already cached
                }

                // Download and cache
                tracing::debug!(
                    topic = %topic,
                    partition = partition_id,
                    base_offset = next_segment.base_offset,
                    "Prefetching next segment"
                );

                let path = object_store::path::Path::from(next_segment.s3_key.as_str());
                if let Ok(result) = object_store.get(&path).await {
                    if let Ok(data) = result.bytes().await {
                        let _ = cache.put(&cache_key, data).await;
                    }
                }
            }
        });
    }

    fn cache_key(&self, info: &SegmentInfo) -> String {
        format!("{}-{}-{}", self.topic, self.partition_id, info.base_offset)
    }
}

/// Result of a read operation
#[derive(Debug, Clone)]
pub struct ReadResult {
    /// Records read from the partition
    pub records: Vec<Record>,

    /// Current high watermark of the partition (next offset to be written)
    pub high_watermark: u64,
}
