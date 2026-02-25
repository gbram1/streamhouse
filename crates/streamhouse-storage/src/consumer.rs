//! Consumer API
//!
//! High-level API for consuming records from partitions with automatic offset management.
//!
//! ## What is a Consumer?
//!
//! A Consumer wraps PartitionReader and adds:
//! - **Offset tracking**: Remembers current position
//! - **Consumer groups**: Multiple consumers coordinate via committed offsets
//! - **Auto-commit**: Optionally commit after each poll
//! - **Seeking**: Jump to specific offsets
//!
//! ## Consumer Groups
//!
//! Consumer groups enable parallel processing:
//! - Multiple consumers in same group process different partitions
//! - Each consumer commits its progress to metadata store
//! - On restart, consumer resumes from last committed offset
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::Consumer;
//!
//! // Create consumer (part of "analytics-group")
//! let mut consumer = Consumer::new(
//!     "orders".to_string(),
//!     0,  // partition
//!     Some("analytics-group".to_string()),
//!     reader,
//!     metadata,
//! ).await?;
//!
//! // Poll for records
//! loop {
//!     let records = consumer.poll(100).await?;
//!
//!     for record in records {
//!         process(record);
//!     }
//!
//!     // Commit progress
//!     consumer.commit().await?;
//! }
//! ```

use crate::{error::Result, reader::PartitionReader};
use std::sync::Arc;
use streamhouse_core::record::Record;
use streamhouse_metadata::MetadataStore;

/// High-level consumer for reading from a partition
pub struct Consumer {
    group_id: Option<String>,
    topic: String,
    partition_id: u32,
    reader: Arc<PartitionReader>,
    metadata: Arc<dyn MetadataStore>,
    current_offset: u64,
}

impl Consumer {
    /// Create a new consumer
    ///
    /// If `group_id` is provided, consumer will resume from last committed offset.
    /// Otherwise, starts from offset 0.
    pub async fn new(
        topic: String,
        partition_id: u32,
        group_id: Option<String>,
        reader: Arc<PartitionReader>,
        metadata: Arc<dyn MetadataStore>,
    ) -> Result<Self> {
        // Determine starting offset
        let current_offset = if let Some(ref group) = group_id {
            // Resume from committed offset
            metadata
                .get_committed_offset(group, &topic, partition_id)
                .await?
                .unwrap_or(0)
        } else {
            // Start from beginning
            0
        };

        Ok(Self {
            group_id,
            topic,
            partition_id,
            reader,
            metadata,
            current_offset,
        })
    }

    /// Poll for next batch of records
    ///
    /// Returns up to `max_records` records. If no records available, returns empty vec.
    /// Automatically updates internal offset position.
    pub async fn poll(&mut self, max_records: usize) -> Result<Vec<Record>> {
        let result = self.reader.read(self.current_offset, max_records).await?;

        if !result.records.is_empty() {
            let last_offset = result.records.last().unwrap().offset;
            self.current_offset = last_offset + 1;
        }

        Ok(result.records)
    }

    /// Commit current offset (for consumer groups)
    ///
    /// Saves progress to metadata store so consumer can resume after restart.
    pub async fn commit(&self) -> Result<()> {
        if let Some(ref group) = self.group_id {
            self.metadata
                .commit_offset(
                    group,
                    &self.topic,
                    self.partition_id,
                    self.current_offset,
                    None,
                )
                .await?;

            tracing::debug!(
                group = %group,
                topic = %self.topic,
                partition = self.partition_id,
                offset = self.current_offset,
                "Committed offset"
            );
        }

        Ok(())
    }

    /// Seek to specific offset
    pub fn seek(&mut self, offset: u64) {
        self.current_offset = offset;
    }

    /// Get current position
    pub fn position(&self) -> u64 {
        self.current_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::SegmentCache;
    use crate::reader::PartitionReader;
    use object_store::memory::InMemory;
    use object_store::ObjectStore;
    use streamhouse_metadata::{SqliteMetadataStore, TopicConfig};

    async fn create_test_metadata(topic: &str, partitions: u32) -> Arc<dyn MetadataStore> {
        let store = SqliteMetadataStore::new_in_memory().await.unwrap();
        store
            .create_topic(TopicConfig {
                name: topic.to_string(),
                partition_count: partitions,
                retention_ms: Some(86400000),
                config: Default::default(),
                cleanup_policy: Default::default(),
            })
            .await
            .unwrap();
        Arc::new(store)
    }

    fn create_test_reader(
        topic: &str,
        partition_id: u32,
        metadata: Arc<dyn MetadataStore>,
        cache: Arc<SegmentCache>,
    ) -> Arc<PartitionReader> {
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        Arc::new(PartitionReader::new(
            topic.to_string(),
            partition_id,
            metadata.clone(),
            object_store,
            cache,
        ))
    }

    #[tokio::test]
    async fn test_consumer_starts_at_offset_zero_without_group() {
        let metadata = create_test_metadata("orders", 1).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap());
        let reader = create_test_reader("orders", 0, metadata.clone(), cache);

        let consumer = Consumer::new("orders".to_string(), 0, None, reader, metadata)
            .await
            .unwrap();

        assert_eq!(consumer.position(), 0);
    }

    #[tokio::test]
    async fn test_consumer_with_group_starts_at_zero_when_no_committed_offset() {
        let metadata = create_test_metadata("orders", 1).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap());
        let reader = create_test_reader("orders", 0, metadata.clone(), cache);

        let consumer = Consumer::new(
            "orders".to_string(),
            0,
            Some("my-group".to_string()),
            reader,
            metadata,
        )
        .await
        .unwrap();

        assert_eq!(consumer.position(), 0);
    }

    #[tokio::test]
    async fn test_consumer_with_group_resumes_from_committed_offset() {
        let metadata = create_test_metadata("orders", 1).await;

        // Commit an offset first
        metadata
            .commit_offset("my-group", "orders", 0, 42, None)
            .await
            .unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap());
        let reader = create_test_reader("orders", 0, metadata.clone(), cache);

        let consumer = Consumer::new(
            "orders".to_string(),
            0,
            Some("my-group".to_string()),
            reader,
            metadata,
        )
        .await
        .unwrap();

        assert_eq!(consumer.position(), 42);
    }

    #[tokio::test]
    async fn test_consumer_seek() {
        let metadata = create_test_metadata("orders", 1).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap());
        let reader = create_test_reader("orders", 0, metadata.clone(), cache);

        let mut consumer = Consumer::new("orders".to_string(), 0, None, reader, metadata)
            .await
            .unwrap();

        assert_eq!(consumer.position(), 0);

        consumer.seek(100);
        assert_eq!(consumer.position(), 100);

        consumer.seek(0);
        assert_eq!(consumer.position(), 0);

        consumer.seek(u64::MAX);
        assert_eq!(consumer.position(), u64::MAX);
    }

    #[tokio::test]
    async fn test_consumer_commit_without_group_is_noop() {
        let metadata = create_test_metadata("orders", 1).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap());
        let reader = create_test_reader("orders", 0, metadata.clone(), cache);

        let consumer = Consumer::new("orders".to_string(), 0, None, reader, metadata.clone())
            .await
            .unwrap();

        // commit without group should be a no-op (not an error)
        consumer.commit().await.unwrap();

        // No offset should have been committed
        let offset = metadata
            .get_committed_offset("no-group", "orders", 0)
            .await
            .unwrap();
        assert!(offset.is_none());
    }

    #[tokio::test]
    async fn test_consumer_commit_with_group_persists_offset() {
        let metadata = create_test_metadata("orders", 1).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap());
        let reader = create_test_reader("orders", 0, metadata.clone(), cache);

        let mut consumer = Consumer::new(
            "orders".to_string(),
            0,
            Some("test-group".to_string()),
            reader,
            metadata.clone(),
        )
        .await
        .unwrap();

        // Move the position forward (simulating poll)
        consumer.seek(50);

        // Commit
        consumer.commit().await.unwrap();

        // Verify offset was persisted
        let offset = metadata
            .get_committed_offset("test-group", "orders", 0)
            .await
            .unwrap();
        assert_eq!(offset, Some(50));
    }

    #[tokio::test]
    async fn test_consumer_position_after_seek() {
        let metadata = create_test_metadata("events", 1).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap());
        let reader = create_test_reader("events", 0, metadata.clone(), cache);

        let mut consumer = Consumer::new("events".to_string(), 0, None, reader, metadata)
            .await
            .unwrap();

        // Multiple seeks should always reflect the latest position
        for target in [10, 20, 5, 1000, 0] {
            consumer.seek(target);
            assert_eq!(consumer.position(), target);
        }
    }
}
