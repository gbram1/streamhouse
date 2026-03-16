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
//! 1. List all `.seg` files across all org prefixes in S3
//!    (`data/` for the default org, `org-{uuid}/data/` for each non-default org)
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

/// Statistics returned by `reconcile_from_s3()`.
#[derive(Debug, Clone, Default)]
pub struct ReconcileFromS3Stats {
    /// Total `.seg` files found in S3
    pub segments_found: u64,
    /// Segments successfully registered in metadata
    pub segments_registered: u64,
    /// Segments that failed to parse or register
    pub segments_failed: u64,
}

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

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64;
        let grace_ms = self.grace_period.as_millis() as i64;

        // Build list of S3 prefixes to scan — default org + all non-default orgs
        let mut prefixes = vec!["data/".to_string()];
        if let Ok(orgs) = self.metadata.list_organizations().await {
            for org in orgs {
                if org.id != streamhouse_metadata::DEFAULT_ORGANIZATION_ID {
                    prefixes.push(format!("org-{}/data/", org.id));
                }
            }
        }

        // List all .seg files across all org prefixes
        let mut s3_keys: Vec<(String, i64)> = Vec::new();
        for prefix_str in &prefixes {
            let prefix = Path::from(prefix_str.as_str());
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
        }

        if s3_keys.is_empty() {
            tracing::debug!("Reconciler: no .seg files found in S3");
            return Ok(0);
        }

        // Collect all known s3_keys from metadata (across all orgs/topics/partitions)
        let mut known_keys: HashSet<String> = HashSet::new();

        // Build list of org_ids to check
        let mut org_ids: Vec<String> =
            vec![streamhouse_metadata::DEFAULT_ORGANIZATION_ID.to_string()];
        if let Ok(orgs) = self.metadata.list_organizations().await {
            for org in orgs {
                if org.id != streamhouse_metadata::DEFAULT_ORGANIZATION_ID {
                    org_ids.push(org.id);
                }
            }
        }

        for org_id in &org_ids {
            let topics = self.metadata.list_topics_for_org(org_id).await?;
            for topic in &topics {
                let partitions = self.metadata.list_partitions(org_id, &topic.name).await?;
                for partition in &partitions {
                    let segments = self
                        .metadata
                        .get_segments(org_id, &topic.name, partition.partition_id)
                        .await?;
                    for seg in segments {
                        known_keys.insert(seg.s3_key);
                    }
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

    /// Run reverse reconciliation: re-register S3 segments missing from metadata.
    ///
    /// This is the inverse of `reconcile()`. For each `.seg` file in S3 that has
    /// no corresponding metadata entry, it:
    /// 1. Fetches the 64-byte header to extract offset range and record count
    /// 2. Constructs a `SegmentInfo` from the header + S3 listing metadata
    /// 3. Registers the segment via `metadata.add_segment()`
    /// 4. Updates the high watermark if the new segment extends it
    ///
    /// Used during disaster recovery after restoring a metadata snapshot that
    /// may be slightly behind S3.
    pub async fn reconcile_from_s3(
        &self,
    ) -> std::result::Result<ReconcileFromS3Stats, Box<dyn std::error::Error + Send + Sync>> {
        use futures::TryStreamExt;

        let mut stats = ReconcileFromS3Stats::default();

        // Build list of S3 prefixes to scan — default org + all non-default orgs
        let mut prefixes: Vec<(String, String)> = vec![(
            "data/".to_string(),
            streamhouse_metadata::DEFAULT_ORGANIZATION_ID.to_string(),
        )];
        if let Ok(orgs) = self.metadata.list_organizations().await {
            for org in orgs {
                if org.id != streamhouse_metadata::DEFAULT_ORGANIZATION_ID {
                    prefixes.push((format!("org-{}/data/", org.id), org.id.clone()));
                }
            }
        }

        // Collect all known s3_keys from metadata
        let mut known_keys: HashSet<String> = HashSet::new();
        let mut org_ids: Vec<String> =
            vec![streamhouse_metadata::DEFAULT_ORGANIZATION_ID.to_string()];
        if let Ok(orgs) = self.metadata.list_organizations().await {
            for org in orgs {
                if org.id != streamhouse_metadata::DEFAULT_ORGANIZATION_ID {
                    org_ids.push(org.id);
                }
            }
        }
        for org_id in &org_ids {
            if let Ok(topics) = self.metadata.list_topics_for_org(org_id).await {
                for topic in &topics {
                    if let Ok(partitions) = self.metadata.list_partitions(org_id, &topic.name).await
                    {
                        for partition in &partitions {
                            if let Ok(segments) = self
                                .metadata
                                .get_segments(org_id, &topic.name, partition.partition_id)
                                .await
                            {
                                for seg in segments {
                                    known_keys.insert(seg.s3_key);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Scan each prefix for .seg files not in metadata
        for (prefix_str, org_id) in &prefixes {
            let prefix = Path::from(prefix_str.as_str());
            let mut list_stream = self.object_store.list(Some(&prefix));

            while let Some(meta) = list_stream.try_next().await? {
                let key = meta.location.to_string();
                if !key.ends_with(".seg") {
                    continue;
                }

                stats.segments_found += 1;

                if known_keys.contains(&key) {
                    continue; // Already registered
                }

                // Parse S3 key to extract topic, partition, base_offset
                // Expected format: {prefix}{topic}/{partition}/{offset:020}.seg
                let relative = key.strip_prefix(prefix_str).unwrap_or(&key);
                let parts: Vec<&str> = relative.split('/').collect();
                if parts.len() < 3 {
                    tracing::warn!(s3_key = %key, "Cannot parse S3 key, skipping");
                    stats.segments_failed += 1;
                    continue;
                }

                let topic = parts[0].to_string();
                let partition_id: u32 = match parts[1].parse() {
                    Ok(p) => p,
                    Err(_) => {
                        tracing::warn!(s3_key = %key, "Cannot parse partition ID, skipping");
                        stats.segments_failed += 1;
                        continue;
                    }
                };
                // offset filename is like "00000000000000001000.seg"
                let offset_str = parts[2].trim_end_matches(".seg");
                let base_offset: u64 = match offset_str.parse() {
                    Ok(o) => o,
                    Err(_) => {
                        tracing::warn!(s3_key = %key, "Cannot parse base offset, skipping");
                        stats.segments_failed += 1;
                        continue;
                    }
                };

                // Fetch just the header (first HEADER_SIZE bytes)
                let header_range = 0..crate::segment::HEADER_SIZE;
                let header_bytes = match self
                    .object_store
                    .get_range(&meta.location, header_range)
                    .await
                {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!(s3_key = %key, error = %e, "Failed to fetch segment header");
                        stats.segments_failed += 1;
                        continue;
                    }
                };

                // Parse header
                let header_data = bytes::Bytes::from(header_bytes.to_vec());
                let header = match crate::segment::SegmentReader::read_header(&header_data) {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::warn!(s3_key = %key, error = %e, "Failed to parse segment header");
                        stats.segments_failed += 1;
                        continue;
                    }
                };

                let size_bytes = meta.size as u64;
                let created_at = meta
                    .last_modified
                    .signed_duration_since(chrono::DateTime::UNIX_EPOCH)
                    .num_milliseconds();

                let segment_info = streamhouse_metadata::SegmentInfo {
                    id: format!("{}-{}-{}", topic, partition_id, base_offset),
                    topic: topic.clone(),
                    partition_id,
                    base_offset: header.base_offset,
                    end_offset: header.end_offset,
                    record_count: header.record_count,
                    size_bytes,
                    s3_bucket: String::new(), // Not needed for object_store path-based access
                    s3_key: key.clone(),
                    created_at,
                    min_timestamp: 0, // Not available from header alone
                    max_timestamp: 0,
                };

                // Register segment
                if let Err(e) = self.metadata.add_segment(org_id, segment_info).await {
                    tracing::warn!(
                        s3_key = %key,
                        error = %e,
                        "Failed to register segment from S3"
                    );
                    stats.segments_failed += 1;
                    continue;
                }

                // Update high watermark if this segment extends it
                let new_hwm = header.end_offset + 1;
                if let Ok(Some(partition)) = self
                    .metadata
                    .get_partition(org_id, &topic, partition_id)
                    .await
                {
                    if new_hwm > partition.high_watermark {
                        if let Err(e) = self
                            .metadata
                            .update_high_watermark(org_id, &topic, partition_id, new_hwm)
                            .await
                        {
                            tracing::warn!(
                                s3_key = %key,
                                error = %e,
                                "Failed to update high watermark"
                            );
                        }
                    }
                }

                stats.segments_registered += 1;
                tracing::info!(
                    s3_key = %key,
                    topic = %topic,
                    partition = partition_id,
                    base_offset = header.base_offset,
                    end_offset = header.end_offset,
                    "Registered segment from S3"
                );
            }
        }

        tracing::info!(
            segments_found = stats.segments_found,
            segments_registered = stats.segments_registered,
            segments_failed = stats.segments_failed,
            "Reconcile-from-S3 complete"
        );

        Ok(stats)
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

        #[allow(clippy::useless_conversion)]
        store
            .put(&registered_key, Bytes::from("segment-data").into())
            .await
            .unwrap();
        #[allow(clippy::useless_conversion)]
        store
            .put(&orphan_key, Bytes::from("orphan-data").into())
            .await
            .unwrap();

        // Register only the first segment in metadata
        metadata
            .add_segment(
                streamhouse_metadata::DEFAULT_ORGANIZATION_ID,
                streamhouse_metadata::SegmentInfo {
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
                },
            )
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
        #[allow(clippy::useless_conversion)]
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
