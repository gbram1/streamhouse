//! S3 Orphan Reconciler
//!
//! Background job that detects and cleans up orphaned S3 objects —
//! segment files that exist in S3 but have no corresponding metadata entry.
//!
//! ## Why Orphans Occur
//!
//! If the process crashes after uploading a segment to S3 (writer.rs upload_to_s3)
//! but before registering it in the metadata store (add_segment_and_update_watermark),
//! an S3 object exists with no metadata reference. These "orphans" waste storage
//! and could cause confusion during debugging.
//!
//! ## How It Works
//!
//! 1. List all `.seg` files under the `data/` prefix in S3
//! 2. For each file, check if the metadata store has a segment with that `s3_key`
//! 3. If no metadata entry exists AND the file is older than the grace period,
//!    delete it from S3
//!
//! The grace period (default 1 hour) prevents deleting files that are still being
//! registered by an in-flight write operation.

use object_store::{path::Path, ObjectStore};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::MetadataStore;

/// Reconciles S3 objects against metadata to find and delete orphans.
pub struct S3Reconciler {
    object_store: Arc<dyn ObjectStore>,
    metadata: Arc<dyn MetadataStore>,
    /// Minimum age before an unregistered object is considered orphaned
    grace_period: Duration,
}

impl S3Reconciler {
    pub fn new(
        object_store: Arc<dyn ObjectStore>,
        metadata: Arc<dyn MetadataStore>,
        grace_period: Duration,
    ) -> Self {
        Self {
            object_store,
            metadata,
            grace_period,
        }
    }

    /// Run one reconciliation pass.
    ///
    /// Returns the number of orphaned objects deleted.
    pub async fn reconcile(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        use futures::TryStreamExt;

        let prefix = Path::from("data/");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64;
        let grace_ms = self.grace_period.as_millis() as i64;

        // List all .seg files in S3
        let mut s3_keys: Vec<(String, i64)> = Vec::new();
        let mut list_stream = self.object_store.list(Some(&prefix));
        while let Some(meta) = list_stream.try_next().await? {
            let key = meta.location.to_string();
            if key.ends_with(".seg") {
                let last_modified_ms = meta
                    .last_modified
                    .signed_duration_since(chrono::DateTime::UNIX_EPOCH)
                    .num_milliseconds();
                s3_keys.push((key, last_modified_ms));
            }
        }

        if s3_keys.is_empty() {
            tracing::debug!("Reconciler: no .seg files found in S3");
            return Ok(0);
        }

        // Collect all known s3_keys from metadata (across all topics/partitions)
        let topics = self.metadata.list_topics().await?;
        let mut known_keys: HashSet<String> = HashSet::new();

        for topic in &topics {
            let partitions = self.metadata.list_partitions(&topic.name).await?;
            for partition in &partitions {
                let segments = self
                    .metadata
                    .get_segments(&topic.name, partition.partition_id)
                    .await?;
                for seg in segments {
                    known_keys.insert(seg.s3_key);
                }
            }
        }

        // Find and delete orphans past grace period
        let mut deleted = 0u64;
        for (s3_key, last_modified_ms) in &s3_keys {
            if known_keys.contains(s3_key) {
                continue;
            }

            let age_ms = now - last_modified_ms;
            if age_ms < grace_ms {
                tracing::debug!(
                    s3_key = %s3_key,
                    age_ms,
                    "Orphan candidate within grace period, skipping"
                );
                continue;
            }

            tracing::info!(
                s3_key = %s3_key,
                age_ms,
                "Deleting orphaned S3 object"
            );
            match self.object_store.delete(&Path::from(s3_key.as_str())).await {
                Ok(()) => deleted += 1,
                Err(e) => {
                    tracing::warn!(
                        s3_key = %s3_key,
                        error = %e,
                        "Failed to delete orphaned S3 object"
                    );
                }
            }
        }

        if deleted > 0 {
            tracing::info!(
                total_s3_objects = s3_keys.len(),
                known_segments = known_keys.len(),
                deleted,
                "Reconciliation complete"
            );
        }

        Ok(deleted)
    }

    /// Start a background reconciliation loop.
    ///
    /// Runs `reconcile()` at the specified interval until the returned handle is aborted.
    pub fn start_background(self: Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // Skip the initial immediate tick
            ticker.tick().await;

            loop {
                ticker.tick().await;
                match self.reconcile().await {
                    Ok(deleted) => {
                        if deleted > 0 {
                            tracing::info!(deleted, "Background reconciliation pass complete");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Background reconciliation failed");
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use object_store::memory::InMemory;
    use streamhouse_metadata::SqliteMetadataStore;
    use streamhouse_metadata::{MetadataStore, TopicConfig};

    #[tokio::test]
    async fn test_reconciler_deletes_orphans() {
        let store = Arc::new(InMemory::new());
        let metadata = Arc::new(SqliteMetadataStore::new_in_memory().await.unwrap());

        // Create a topic so segments can reference it
        metadata
            .create_topic(TopicConfig {
                name: "orders".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: std::collections::HashMap::new(),
                cleanup_policy: Default::default(),
            })
            .await
            .unwrap();

        // Upload two objects: one registered in metadata, one orphaned
        let registered_key = Path::from("data/orders/0/00000000000000000000.seg");
        let orphan_key = Path::from("data/orders/0/00000000000000001000.seg");

        store
            .put(&registered_key, Bytes::from("segment-data").into())
            .await
            .unwrap();
        store
            .put(&orphan_key, Bytes::from("orphan-data").into())
            .await
            .unwrap();

        // Register only the first segment in metadata
        metadata
            .add_segment(streamhouse_metadata::SegmentInfo {
                id: "orders-0-0".to_string(),
                topic: "orders".to_string(),
                partition_id: 0,
                base_offset: 0,
                end_offset: 999,
                record_count: 1000,
                size_bytes: 12,
                s3_bucket: "test".to_string(),
                s3_key: registered_key.to_string(),
                created_at: 0,
                min_timestamp: 0,
                max_timestamp: 0,
            })
            .await
            .unwrap();

        // Run reconciler with zero grace period
        let reconciler = Arc::new(S3Reconciler::new(
            store.clone(),
            metadata,
            Duration::from_secs(0),
        ));

        let deleted = reconciler.reconcile().await.unwrap();
        assert_eq!(deleted, 1, "Should delete exactly one orphan");

        // Verify the registered object still exists
        assert!(store.get(&registered_key).await.is_ok());
        // Verify the orphan was deleted
        assert!(store.get(&orphan_key).await.is_err());
    }

    #[tokio::test]
    async fn test_reconciler_respects_grace_period() {
        let store = Arc::new(InMemory::new());
        let metadata = Arc::new(SqliteMetadataStore::new_in_memory().await.unwrap());

        metadata
            .create_topic(TopicConfig {
                name: "logs".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: std::collections::HashMap::new(),
                cleanup_policy: Default::default(),
            })
            .await
            .unwrap();

        // Upload an orphan (just created, within grace period)
        let orphan_key = Path::from("data/logs/0/00000000000000000000.seg");
        store
            .put(&orphan_key, Bytes::from("recent-orphan").into())
            .await
            .unwrap();

        // Use a very long grace period
        let reconciler = Arc::new(S3Reconciler::new(
            store.clone(),
            metadata,
            Duration::from_secs(86400),
        ));

        let deleted = reconciler.reconcile().await.unwrap();
        assert_eq!(
            deleted, 0,
            "Should not delete recent orphan within grace period"
        );

        // Verify the object still exists
        assert!(store.get(&orphan_key).await.is_ok());
    }
}
