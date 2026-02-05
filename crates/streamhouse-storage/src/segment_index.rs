//! In-Memory Segment Index for O(log n) Offset Lookups
//!
//! ## The Problem
//!
//! Without an in-memory index, every `read()` call requires:
//! 1. Query metadata store (even with caching, ~100µs)
//! 2. Parse response
//! 3. Find segment
//!
//! For high-throughput consumers (10K+ QPS), this adds significant overhead.
//!
//! ## The Solution
//!
//! `SegmentIndex` maintains an in-memory BTreeMap of segments per partition:
//! - **Key**: `base_offset` (u64)
//! - **Value**: `SegmentInfo`
//! - **Lookup**: O(log n) via range query
//!
//! ```text
//! BTreeMap<u64, SegmentInfo>
//!   0      -> SegmentInfo { end_offset: 9999, ... }
//!   10000  -> SegmentInfo { end_offset: 19999, ... }
//!   20000  -> SegmentInfo { end_offset: 29999, ... }
//!
//! Query offset 15000:
//!   range(..=15000).next_back() -> Some((10000, SegmentInfo))
//! ```
//!
//! ## Performance
//!
//! - **Cold lookup** (index miss): ~100µs + metadata query
//! - **Warm lookup** (index hit): **< 1µs** (BTreeMap range query)
//! - **Memory**: ~500 bytes per segment (10K segments = 5 MB)
//!
//! ## Refresh Strategy
//!
//! - **Initial load**: On first access
//! - **On miss**: If offset not found in index, refresh and retry
//! - **TTL-based**: Refresh every 30 seconds (configurable)
//!
//! This handles new segments being written while keeping refresh overhead minimal.
//!
//! ## Thread Safety
//!
//! - Uses `Arc<RwLock<BTreeMap>>` for concurrent reads
//! - Reads are lock-free (RwLock allows multiple readers)
//! - Writes (refresh) take exclusive lock briefly (~1ms)

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use streamhouse_metadata::{MetadataStore, SegmentInfo};
use tokio::sync::RwLock;

/// Configuration for segment index
#[derive(Debug, Clone)]
pub struct SegmentIndexConfig {
    /// How often to refresh the index from metadata store
    pub refresh_interval: Duration,

    /// Maximum number of segments to cache per partition
    pub max_segments: usize,
}

impl Default for SegmentIndexConfig {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_secs(30),
            max_segments: 10_000,
        }
    }
}

/// In-memory index of segments for a single partition
///
/// Provides O(log n) lookup for offset -> segment mapping.
#[derive(Clone)]
pub struct SegmentIndex {
    topic: String,
    partition_id: u32,
    metadata: Arc<dyn MetadataStore>,
    config: SegmentIndexConfig,

    /// BTreeMap indexed by base_offset for efficient range queries
    index: Arc<RwLock<BTreeMap<u64, SegmentInfo>>>,

    /// Last time index was refreshed
    last_refresh: Arc<RwLock<Option<Instant>>>,
}

impl SegmentIndex {
    /// Create a new segment index
    pub fn new(
        topic: String,
        partition_id: u32,
        metadata: Arc<dyn MetadataStore>,
        config: SegmentIndexConfig,
    ) -> Self {
        Self {
            topic,
            partition_id,
            metadata,
            config,
            index: Arc::new(RwLock::new(BTreeMap::new())),
            last_refresh: Arc::new(RwLock::new(None)),
        }
    }

    /// Find segment containing the given offset
    ///
    /// Returns `None` if no segment contains the offset.
    /// Automatically refreshes index if needed or on miss.
    pub async fn find_segment_for_offset(
        &self,
        offset: u64,
    ) -> Result<Option<SegmentInfo>, streamhouse_metadata::MetadataError> {
        // Check if refresh is needed
        if self.should_refresh().await {
            self.refresh().await?;
        }

        // Try to find in index
        let result = self.lookup_in_index(offset).await;

        if result.is_some() {
            return Ok(result);
        }

        // Index miss - refresh and retry once
        tracing::debug!(
            topic = %self.topic,
            partition = self.partition_id,
            offset,
            "Segment index miss, refreshing"
        );

        self.refresh().await?;
        Ok(self.lookup_in_index(offset).await)
    }

    /// Lookup offset in the in-memory index (no metadata query)
    async fn lookup_in_index(&self, offset: u64) -> Option<SegmentInfo> {
        let index = self.index.read().await;

        // Find largest base_offset <= target offset
        let (_, segment) = index.range(..=offset).next_back()?;

        // Verify offset is within segment range
        if offset >= segment.base_offset && offset <= segment.end_offset {
            Some(segment.clone())
        } else {
            None
        }
    }

    /// Check if index should be refreshed based on TTL
    async fn should_refresh(&self) -> bool {
        let last_refresh = self.last_refresh.read().await;

        match *last_refresh {
            None => true, // Never refreshed
            Some(instant) => instant.elapsed() >= self.config.refresh_interval,
        }
    }

    /// Refresh index from metadata store
    async fn refresh(&self) -> Result<(), streamhouse_metadata::MetadataError> {
        tracing::debug!(
            topic = %self.topic,
            partition = self.partition_id,
            "Refreshing segment index"
        );

        // Query all segments for this partition
        let segments = self
            .metadata
            .get_segments(&self.topic, self.partition_id)
            .await?;

        // Build new index
        let mut new_index = BTreeMap::new();
        for segment in segments {
            new_index.insert(segment.base_offset, segment);
        }

        // Apply max_segments limit (keep most recent)
        if new_index.len() > self.config.max_segments {
            let excess = new_index.len() - self.config.max_segments;
            let keys_to_remove: Vec<_> = new_index.keys().take(excess).cloned().collect();
            for key in keys_to_remove {
                new_index.remove(&key);
            }
        }

        // Update index
        let mut index = self.index.write().await;
        *index = new_index;

        // Update refresh timestamp
        let mut last_refresh = self.last_refresh.write().await;
        *last_refresh = Some(Instant::now());

        tracing::debug!(
            topic = %self.topic,
            partition = self.partition_id,
            segment_count = index.len(),
            "Segment index refreshed"
        );

        Ok(())
    }

    /// Get current number of segments in index
    pub async fn segment_count(&self) -> usize {
        self.index.read().await.len()
    }

    /// Force a refresh of the index (useful after writing new segments)
    pub async fn force_refresh(&self) -> Result<(), streamhouse_metadata::MetadataError> {
        self.refresh().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use streamhouse_metadata::{MetadataStore, SegmentInfo, SqliteMetadataStore, TopicConfig};

    async fn create_test_store() -> Arc<SqliteMetadataStore> {
        let store = SqliteMetadataStore::new(":memory:").await.unwrap();
        Arc::new(store)
    }

    async fn setup_test_partition(store: &Arc<SqliteMetadataStore>) {
        store
            .create_topic(TopicConfig {
                name: "test".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: HashMap::new(),
                cleanup_policy: Default::default(),
            })
            .await
            .unwrap();

        // Add some segments
        for i in 0..5 {
            let base_offset = i * 10000;
            store
                .add_segment(SegmentInfo {
                    id: format!("seg-{}", i),
                    topic: "test".to_string(),
                    partition_id: 0,
                    base_offset,
                    end_offset: base_offset + 9999,
                    record_count: 10000,
                    size_bytes: 1024 * 1024,
                    s3_bucket: "test".to_string(),
                    s3_key: format!("test/0/seg_{}.bin", i),
                    created_at: 0,
                })
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_segment_index_lookup() {
        let store = create_test_store().await;
        setup_test_partition(&store).await;

        let index = SegmentIndex::new(
            "test".to_string(),
            0,
            store.clone(),
            SegmentIndexConfig::default(),
        );

        // First lookup should trigger refresh
        let segment = index.find_segment_for_offset(15000).await.unwrap().unwrap();
        assert_eq!(segment.base_offset, 10000);
        assert_eq!(segment.end_offset, 19999);

        // Second lookup should hit index
        let segment = index.find_segment_for_offset(25000).await.unwrap().unwrap();
        assert_eq!(segment.base_offset, 20000);
        assert_eq!(segment.end_offset, 29999);

        // Verify all 5 segments loaded
        assert_eq!(index.segment_count().await, 5);
    }

    #[tokio::test]
    async fn test_segment_index_boundaries() {
        let store = create_test_store().await;
        setup_test_partition(&store).await;

        let index = SegmentIndex::new(
            "test".to_string(),
            0,
            store.clone(),
            SegmentIndexConfig::default(),
        );

        // Test exact base_offset
        let segment = index.find_segment_for_offset(0).await.unwrap().unwrap();
        assert_eq!(segment.base_offset, 0);

        // Test exact end_offset
        let segment = index.find_segment_for_offset(9999).await.unwrap().unwrap();
        assert_eq!(segment.base_offset, 0);
        assert_eq!(segment.end_offset, 9999);

        // Test first offset of next segment
        let segment = index.find_segment_for_offset(10000).await.unwrap().unwrap();
        assert_eq!(segment.base_offset, 10000);
    }

    #[tokio::test]
    async fn test_segment_index_miss() {
        let store = create_test_store().await;
        setup_test_partition(&store).await;

        let index = SegmentIndex::new(
            "test".to_string(),
            0,
            store.clone(),
            SegmentIndexConfig::default(),
        );

        // Try to find offset beyond all segments
        let result = index.find_segment_for_offset(100000).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_segment_index_refresh_on_new_segment() {
        let store = create_test_store().await;
        setup_test_partition(&store).await;

        let index = SegmentIndex::new(
            "test".to_string(),
            0,
            store.clone(),
            SegmentIndexConfig::default(),
        );

        // Initial lookup
        assert_eq!(index.segment_count().await, 0);
        index.find_segment_for_offset(0).await.unwrap();
        assert_eq!(index.segment_count().await, 5);

        // Add a new segment
        store
            .add_segment(SegmentInfo {
                id: "seg-5".to_string(),
                topic: "test".to_string(),
                partition_id: 0,
                base_offset: 50000,
                end_offset: 59999,
                record_count: 10000,
                size_bytes: 1024 * 1024,
                s3_bucket: "test".to_string(),
                s3_key: "test/0/seg_5.bin".to_string(),
                created_at: 0,
            })
            .await
            .unwrap();

        // Lookup in new segment should trigger refresh
        let segment = index.find_segment_for_offset(55000).await.unwrap().unwrap();
        assert_eq!(segment.base_offset, 50000);
        assert_eq!(index.segment_count().await, 6);
    }

    #[tokio::test]
    async fn test_segment_index_max_segments() {
        let store = create_test_store().await;
        store
            .create_topic(TopicConfig {
                name: "test".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: HashMap::new(),
                cleanup_policy: Default::default(),
            })
            .await
            .unwrap();

        // Add 20 segments
        for i in 0..20 {
            let base_offset = i * 1000;
            store
                .add_segment(SegmentInfo {
                    id: format!("seg-{}", i),
                    topic: "test".to_string(),
                    partition_id: 0,
                    base_offset,
                    end_offset: base_offset + 999,
                    record_count: 1000,
                    size_bytes: 1024,
                    s3_bucket: "test".to_string(),
                    s3_key: format!("test/0/seg_{}.bin", i),
                    created_at: 0,
                })
                .await
                .unwrap();
        }

        let index = SegmentIndex::new(
            "test".to_string(),
            0,
            store.clone(),
            SegmentIndexConfig {
                refresh_interval: Duration::from_secs(30),
                max_segments: 10,
            },
        );

        // Trigger refresh
        index.find_segment_for_offset(0).await.unwrap();

        // Should only keep 10 most recent segments
        assert_eq!(index.segment_count().await, 10);
    }
}
