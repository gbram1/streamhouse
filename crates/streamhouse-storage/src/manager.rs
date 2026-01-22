//! Storage Manager
//!
//! This module implements the top-level storage coordinator that manages all topic writers.
//!
//! ## What Does StorageManager Do?
//!
//! StorageManager is the main entry point for writing data to StreamHouse. It:
//! - Creates and caches TopicWriter instances for each topic
//! - Routes append requests to the appropriate topic writer
//! - Provides global flush operations
//! - Handles topic discovery and validation
//!
//! ## Architecture
//!
//! ```text
//! StorageManager
//!     │
//!     ├─ TopicWriter("orders")
//!     │    ├─ PartitionWriter(0)
//!     │    ├─ PartitionWriter(1)
//!     │    └─ PartitionWriter(2)
//!     │
//!     └─ TopicWriter("events")
//!          ├─ PartitionWriter(0)
//!          └─ PartitionWriter(1)
//! ```
//!
//! ## Thread Safety
//!
//! - StorageManager is Send + Sync
//! - Can be safely shared via Arc<StorageManager>
//! - Uses RwLock for topic writer cache (fast reads, rare writes)
//! - Each partition writer has its own Mutex (fine-grained locking)
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::{StorageManager, WriteConfig};
//! use object_store::aws::AmazonS3Builder;
//!
//! // Setup
//! let object_store = Arc::new(AmazonS3Builder::new()...build()?);
//! let metadata = Arc::new(SqliteMetadataStore::new("metadata.db").await?);
//! let config = WriteConfig::default();
//!
//! let storage = StorageManager::new(object_store, metadata, config);
//!
//! // Append records
//! let result = storage.append(
//!     "orders",
//!     Some(Bytes::from("user-123")),
//!     Bytes::from(r#"{"amount": 99.99}"#),
//!     None,
//! ).await?;
//!
//! println!("Written to partition {} at offset {}", result.partition, result.offset);
//!
//! // Flush all pending writes
//! storage.flush_all().await?;
//! ```

use crate::{
    config::WriteConfig,
    error::{Error, Result},
    writer::TopicWriter,
};
use bytes::Bytes;
use object_store::ObjectStore;
use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_metadata::MetadataStore;
use tokio::sync::RwLock;

/// Get current timestamp in milliseconds
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Manages all topic writers
pub struct StorageManager {
    topic_writers: Arc<RwLock<HashMap<String, Arc<TopicWriter>>>>,
    object_store: Arc<dyn ObjectStore>,
    metadata: Arc<dyn MetadataStore>,
    config: WriteConfig,
}

impl StorageManager {
    /// Create a new storage manager
    pub fn new(
        object_store: Arc<dyn ObjectStore>,
        metadata: Arc<dyn MetadataStore>,
        config: WriteConfig,
    ) -> Self {
        Self {
            topic_writers: Arc::new(RwLock::new(HashMap::new())),
            object_store,
            metadata,
            config,
        }
    }

    /// Get or create a topic writer
    async fn get_or_create_writer(&self, topic: &str) -> Result<Arc<TopicWriter>> {
        // Fast path: read lock
        {
            let writers = self.topic_writers.read().await;
            if let Some(writer) = writers.get(topic) {
                return Ok(writer.clone());
            }
        }

        // Slow path: write lock
        let mut writers = self.topic_writers.write().await;

        // Double-check (another thread may have created it)
        if let Some(writer) = writers.get(topic) {
            return Ok(writer.clone());
        }

        // Get topic metadata
        let topic_meta = self
            .metadata
            .get_topic(topic)
            .await?
            .ok_or_else(|| Error::TopicNotFound(topic.to_string()))?;

        // Create writer
        let writer = TopicWriter::new(
            topic.to_string(),
            topic_meta.partition_count,
            self.object_store.clone(),
            self.metadata.clone(),
            self.config.clone(),
        )
        .await?;

        let writer = Arc::new(writer);
        writers.insert(topic.to_string(), writer.clone());

        tracing::info!(
            topic = %topic,
            partition_count = topic_meta.partition_count,
            "Created topic writer"
        );

        Ok(writer)
    }

    /// Append a record
    pub async fn append(
        &self,
        topic: &str,
        key: Option<Bytes>,
        value: Bytes,
        timestamp: Option<u64>,
    ) -> Result<AppendResult> {
        let writer = self.get_or_create_writer(topic).await?;
        let (partition, offset) = writer.append(key, value, timestamp).await?;

        Ok(AppendResult {
            topic: topic.to_string(),
            partition,
            offset,
            timestamp: timestamp.unwrap_or_else(|| now_ms() as u64),
        })
    }

    /// Flush all writers
    pub async fn flush_all(&self) -> Result<()> {
        let writers = self.topic_writers.read().await;
        for writer in writers.values() {
            writer.flush().await?;
        }
        Ok(())
    }
}

/// Result of appending a record
#[derive(Debug, Clone)]
pub struct AppendResult {
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub timestamp: u64,
}
