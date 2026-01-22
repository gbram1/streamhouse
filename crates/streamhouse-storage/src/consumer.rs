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
