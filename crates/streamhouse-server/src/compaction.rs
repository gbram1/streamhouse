//! Log Compaction Background Task
//!
//! Implements key-based log compaction for topics with `cleanup_policy: compact`.
//! Compaction retains only the latest value for each key, removing older duplicates.
//!
//! ## How Compaction Works
//!
//! 1. Scan topics with `cleanup_policy: compact` or `compact,delete`
//! 2. For each partition, read segments from last compacted offset
//! 3. Build key -> (offset, value) map, keeping latest value per key
//! 4. Create new compacted segment with deduplicated records
//! 5. Update compaction state and remove old segments
//!
//! ## Tombstones
//!
//! Records with null values are tombstones - they mark a key as deleted.
//! During compaction:
//! - Tombstones override previous values for the same key
//! - Tombstones older than `delete.retention.ms` are removed

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use object_store::ObjectStore;
use streamhouse_core::segment::Compression;
use streamhouse_core::Record;
use streamhouse_metadata::{MetadataStore, SegmentInfo};
use streamhouse_storage::{SegmentCache, SegmentReader, SegmentWriter};

/// Compaction configuration
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// How often to check for compaction work (default: 5 minutes)
    pub check_interval: Duration,
    /// Maximum records to process in one compaction run (default: 1,000,000)
    pub max_records_per_run: usize,
    /// Minimum segment age before compaction (default: 1 hour)
    pub min_segment_age: Duration,
    /// Tombstone retention period (default: 24 hours)
    pub tombstone_retention: Duration,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(300), // 5 minutes
            max_records_per_run: 1_000_000,
            min_segment_age: Duration::from_secs(3600), // 1 hour
            tombstone_retention: Duration::from_secs(86400), // 24 hours
        }
    }
}

/// Compaction statistics
#[derive(Debug, Default, Clone)]
pub struct CompactionStats {
    /// Number of compaction runs completed
    pub runs_completed: u64,
    /// Total records processed
    pub records_processed: u64,
    /// Total keys deduplicated (removed)
    pub keys_removed: u64,
    /// Total segments compacted
    pub segments_compacted: u64,
    /// Bytes saved through compaction
    pub bytes_saved: u64,
}

/// Log compaction background task
pub struct CompactionTask {
    metadata: Arc<dyn MetadataStore>,
    #[allow(dead_code)]
    object_store: Arc<dyn ObjectStore>,
    #[allow(dead_code)]
    segment_cache: Arc<SegmentCache>,
    config: CompactionConfig,
    stats: CompactionStats,
}

impl CompactionTask {
    /// Create a new compaction task
    pub fn new(
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn ObjectStore>,
        segment_cache: Arc<SegmentCache>,
        config: CompactionConfig,
    ) -> Self {
        Self {
            metadata,
            object_store,
            segment_cache,
            config,
            stats: CompactionStats::default(),
        }
    }

    /// Start the compaction background task
    pub fn start(self: Arc<Self>, shutdown_rx: oneshot::Receiver<()>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(self.config.check_interval);
            let mut shutdown_rx = shutdown_rx;

            info!(
                "🗜️ Log compaction started (interval: {:?})",
                self.config.check_interval
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = self.run_compaction_cycle().await {
                            error!("Compaction cycle failed: {}", e);
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("🗜️ Log compaction shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Run one compaction cycle
    async fn run_compaction_cycle(&self) -> Result<(), CompactionError> {
        // Get all topics
        let topics = self
            .metadata
            .list_topics()
            .await
            .map_err(|e| CompactionError::Metadata(e.to_string()))?;

        // Filter to compacted topics
        let compacted_topics: Vec<_> = topics
            .into_iter()
            .filter(|t| t.cleanup_policy.is_compacted())
            .collect();

        if compacted_topics.is_empty() {
            debug!("No compacted topics found");
            return Ok(());
        }

        debug!(
            "Found {} compacted topics to process",
            compacted_topics.len()
        );

        for topic in compacted_topics {
            if let Err(e) = self.compact_topic(&topic.name, topic.partition_count).await {
                warn!("Failed to compact topic {}: {}", topic.name, e);
            }
        }

        Ok(())
    }

    /// Compact a single topic
    async fn compact_topic(
        &self,
        topic: &str,
        partition_count: u32,
    ) -> Result<(), CompactionError> {
        debug!(
            "Compacting topic {} ({} partitions)",
            topic, partition_count
        );

        for partition_id in 0..partition_count {
            if let Err(e) = self.compact_partition(topic, partition_id).await {
                warn!(
                    "Failed to compact partition {}/{}: {}",
                    topic, partition_id, e
                );
            }
        }

        Ok(())
    }

    /// Compact a single partition.
    ///
    /// Reads all eligible segments, builds a key-map keeping only the latest
    /// value per key, writes a new compacted segment, and replaces the old
    /// segments in both the object store and metadata.
    async fn compact_partition(
        &self,
        topic: &str,
        partition_id: u32,
    ) -> Result<CompactionResult, CompactionError> {
        // Get segments for this partition (ordered by base_offset)
        let segments = self
            .metadata
            .get_segments(topic, partition_id)
            .await
            .map_err(|e| CompactionError::Metadata(e.to_string()))?;

        if segments.is_empty() {
            return Ok(CompactionResult::default());
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Phase 1: Determine which segments are eligible for compaction.
        // Need at least 2 eligible segments for compaction to be worthwhile.
        let mut segments_to_compact: Vec<&SegmentInfo> = Vec::new();
        for segment in &segments {
            let segment_age_ms = now_ms - segment.created_at;
            if segment_age_ms < self.config.min_segment_age.as_millis() as i64 {
                continue;
            }
            segments_to_compact.push(segment);
        }

        if segments_to_compact.len() < 2 {
            debug!(
                "Partition {}/{}: only {} eligible segment(s), skipping compaction",
                topic,
                partition_id,
                segments_to_compact.len()
            );
            return Ok(CompactionResult::default());
        }

        // Phase 2: Read all eligible segments and build key-map.
        //
        // For keyed records: key -> (offset, timestamp, value)
        //   - Latest offset wins (segments are ordered by base_offset)
        //   - Empty value = tombstone
        //
        // For unkeyed records: collected in order (cannot be deduplicated)
        let mut key_map: HashMap<Bytes, (u64, u64, Bytes)> = HashMap::new();
        // Tombstones tracked separately: key -> (offset, timestamp)
        let mut tombstones: HashMap<Bytes, (u64, u64)> = HashMap::new();
        let mut unkeyed_records: Vec<Record> = Vec::new();
        let mut records_processed: u64 = 0;
        let mut original_bytes: u64 = 0;

        for segment in &segments_to_compact {
            // Enforce max records per run
            if records_processed >= self.config.max_records_per_run as u64 {
                debug!(
                    "Partition {}/{}: reached max_records_per_run ({}), stopping scan",
                    topic, partition_id, self.config.max_records_per_run
                );
                break;
            }

            original_bytes += segment.size_bytes;

            // Read segment data from object store
            let path = object_store::path::Path::from(segment.s3_key.clone());
            let get_result = self
                .object_store
                .get(&path)
                .await
                .map_err(|e| CompactionError::Storage(format!("Failed to GET segment {}: {}", segment.s3_key, e)))?;

            let data = get_result
                .bytes()
                .await
                .map_err(|e| CompactionError::Storage(format!("Failed to read bytes for {}: {}", segment.s3_key, e)))?;

            // Parse segment using SegmentReader
            let reader = SegmentReader::new(data).map_err(|e| {
                CompactionError::SegmentRead(format!(
                    "Failed to parse segment {}: {}",
                    segment.s3_key, e
                ))
            })?;

            let records = reader.read_all().map_err(|e| {
                CompactionError::SegmentRead(format!(
                    "Failed to read records from {}: {}",
                    segment.s3_key, e
                ))
            })?;

            for record in records {
                records_processed += 1;

                match &record.key {
                    Some(key) if !key.is_empty() => {
                        if record.value.is_empty() {
                            // Tombstone: key exists but value is empty
                            tombstones.insert(key.clone(), (record.offset, record.timestamp));
                            // Remove from key_map since tombstone supersedes
                            key_map.remove(key);
                        } else {
                            // Normal keyed record: update key_map if this offset is newer
                            let should_update = match key_map.get(key) {
                                Some((existing_offset, _, _)) => record.offset > *existing_offset,
                                None => true,
                            };
                            if should_update {
                                key_map.insert(
                                    key.clone(),
                                    (record.offset, record.timestamp, record.value.clone()),
                                );
                                // If there was a tombstone for this key, remove it
                                // since a newer record supersedes it
                                tombstones.remove(key);
                            }
                        }
                    }
                    _ => {
                        // Unkeyed records: cannot be deduplicated, keep all
                        unkeyed_records.push(record);
                    }
                }
            }
        }

        // Phase 3: Remove expired tombstones
        let tombstone_retention_ms = self.config.tombstone_retention.as_millis() as i64;
        tombstones.retain(|_, (_, ts)| now_ms - (*ts as i64) < tombstone_retention_ms);

        // Phase 4: Build the compacted record set, sorted by offset
        let mut compacted_records: Vec<Record> = Vec::new();

        // Add surviving keyed records
        for (key, (offset, timestamp, value)) in &key_map {
            compacted_records.push(Record::new(
                *offset,
                *timestamp,
                Some(key.clone()),
                value.clone(),
            ));
        }

        // Add surviving tombstones (as records with empty value)
        for (key, (offset, timestamp)) in &tombstones {
            compacted_records.push(Record::new(
                *offset,
                *timestamp,
                Some(key.clone()),
                Bytes::new(),
            ));
        }

        // Add unkeyed records
        compacted_records.extend(unkeyed_records);

        // Sort by offset to maintain ordering invariant
        compacted_records.sort_by_key(|r| r.offset);

        // Calculate how many keys were removed
        let output_count = compacted_records.len() as u64;
        let keys_removed = records_processed.saturating_sub(output_count);

        // Phase 5: Write compacted segment
        if !compacted_records.is_empty() {
            let mut writer = SegmentWriter::new(Compression::Lz4);
            for record in &compacted_records {
                writer.append(record).map_err(|e| {
                    CompactionError::Storage(format!("Failed to write compacted record: {}", e))
                })?;
            }

            let compacted_bytes = writer.finish().map_err(|e| {
                CompactionError::Storage(format!("Failed to finish compacted segment: {}", e))
            })?;

            let compacted_size = compacted_bytes.len() as u64;
            let bytes_saved = original_bytes.saturating_sub(compacted_size);

            // Upload compacted segment
            let base_offset = compacted_records.first().unwrap().offset;
            let end_offset = compacted_records.last().unwrap().offset;
            let compacted_key = format!(
                "{}/{}/seg_{}_compacted.bin",
                topic, partition_id, base_offset
            );
            let compacted_path = object_store::path::Path::from(compacted_key.clone());

            self.object_store
                .put(&compacted_path, Bytes::from(compacted_bytes).into())
                .await
                .map_err(|e| {
                    CompactionError::Storage(format!("Failed to upload compacted segment: {}", e))
                })?;

            // Register new compacted segment in metadata
            let compacted_segment = SegmentInfo {
                id: format!("{}-{}-{}-compacted", topic, partition_id, base_offset),
                topic: topic.to_string(),
                partition_id,
                base_offset,
                end_offset,
                record_count: compacted_records.len() as u32,
                size_bytes: compacted_size,
                s3_bucket: segments_to_compact
                    .first()
                    .map(|s| s.s3_bucket.clone())
                    .unwrap_or_default(),
                s3_key: compacted_key,
                created_at: now_ms,
            };

            self.metadata
                .add_segment(compacted_segment)
                .await
                .map_err(|e| CompactionError::Metadata(format!("Failed to add compacted segment: {}", e)))?;

            // Delete old segments from object store and metadata
            for segment in &segments_to_compact {
                let old_path = object_store::path::Path::from(segment.s3_key.clone());
                if let Err(e) = self.object_store.delete(&old_path).await {
                    warn!(
                        "Failed to delete old segment {} from object store: {}",
                        segment.s3_key, e
                    );
                }
            }

            // Remove old segment metadata (delete all segments before the new end offset + 1)
            if let Some(first_segment) = segments_to_compact.first() {
                self.metadata
                    .delete_segments_before(topic, partition_id, first_segment.base_offset)
                    .await
                    .map_err(|e| {
                        CompactionError::Metadata(format!(
                            "Failed to delete old segment metadata: {}",
                            e
                        ))
                    })?;
            }

            debug!(
                "Partition {}/{}: compacted {} records into {} ({} removed, {} bytes saved)",
                topic, partition_id, records_processed, output_count, keys_removed, bytes_saved
            );

            Ok(CompactionResult {
                records_processed,
                keys_removed,
                segments_compacted: segments_to_compact.len() as u64,
                bytes_saved,
            })
        } else {
            // All records were removed (all expired tombstones, empty partition)
            // Delete old segments
            for segment in &segments_to_compact {
                let old_path = object_store::path::Path::from(segment.s3_key.clone());
                if let Err(e) = self.object_store.delete(&old_path).await {
                    warn!(
                        "Failed to delete old segment {} from object store: {}",
                        segment.s3_key, e
                    );
                }
            }

            debug!(
                "Partition {}/{}: all {} records removed during compaction",
                topic, partition_id, records_processed
            );

            Ok(CompactionResult {
                records_processed,
                keys_removed,
                segments_compacted: segments_to_compact.len() as u64,
                bytes_saved: original_bytes,
            })
        }
    }

    /// Get current compaction statistics
    pub fn stats(&self) -> &CompactionStats {
        &self.stats
    }
}

/// Result of compacting a partition
#[derive(Debug, Default)]
#[allow(dead_code)]
struct CompactionResult {
    records_processed: u64,
    keys_removed: u64,
    segments_compacted: u64,
    bytes_saved: u64,
}

/// Compaction errors
#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("Metadata error: {0}")]
    Metadata(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Segment read error: {0}")]
    SegmentRead(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use object_store::memory::InMemory;
    use std::sync::Mutex;
    use streamhouse_metadata::*;

    // ---------------------------------------------------------------
    // Mock MetadataStore for compaction tests
    // ---------------------------------------------------------------

    /// A minimal mock that stores segments in memory and implements
    /// only the methods used by the compaction logic.
    struct MockMetadataStore {
        segments: Mutex<Vec<SegmentInfo>>,
        topics: Mutex<Vec<Topic>>,
    }

    impl MockMetadataStore {
        fn new() -> Self {
            Self {
                segments: Mutex::new(Vec::new()),
                topics: Mutex::new(Vec::new()),
            }
        }

        fn with_segments(segments: Vec<SegmentInfo>) -> Self {
            Self {
                segments: Mutex::new(segments),
                topics: Mutex::new(Vec::new()),
            }
        }

        #[allow(dead_code)]
        fn get_stored_segments(&self) -> Vec<SegmentInfo> {
            self.segments.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl MetadataStore for MockMetadataStore {
        async fn create_topic(&self, _config: TopicConfig) -> Result<()> {
            unimplemented!()
        }
        async fn delete_topic(&self, _name: &str) -> Result<()> {
            unimplemented!()
        }
        async fn get_topic(&self, _name: &str) -> Result<Option<Topic>> {
            unimplemented!()
        }
        async fn list_topics(&self) -> Result<Vec<Topic>> {
            Ok(self.topics.lock().unwrap().clone())
        }
        async fn get_partition(&self, _topic: &str, _partition_id: u32) -> Result<Option<Partition>> {
            unimplemented!()
        }
        async fn update_high_watermark(&self, _topic: &str, _partition_id: u32, _offset: u64) -> Result<()> {
            unimplemented!()
        }
        async fn list_partitions(&self, _topic: &str) -> Result<Vec<Partition>> {
            unimplemented!()
        }
        async fn add_segment(&self, segment: SegmentInfo) -> Result<()> {
            self.segments.lock().unwrap().push(segment);
            Ok(())
        }
        async fn get_segments(&self, topic: &str, partition_id: u32) -> Result<Vec<SegmentInfo>> {
            let segments = self.segments.lock().unwrap();
            let mut matching: Vec<_> = segments
                .iter()
                .filter(|s| s.topic == topic && s.partition_id == partition_id)
                .cloned()
                .collect();
            matching.sort_by_key(|s| s.base_offset);
            Ok(matching)
        }
        async fn find_segment_for_offset(&self, _topic: &str, _partition_id: u32, _offset: u64) -> Result<Option<SegmentInfo>> {
            unimplemented!()
        }
        async fn delete_segments_before(&self, topic: &str, partition_id: u32, before_offset: u64) -> Result<u64> {
            let mut segments = self.segments.lock().unwrap();
            let before_count = segments.len();
            segments.retain(|s| !(s.topic == topic && s.partition_id == partition_id && s.end_offset < before_offset));
            Ok((before_count - segments.len()) as u64)
        }
        async fn ensure_consumer_group(&self, _group_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn commit_offset(&self, _group_id: &str, _topic: &str, _partition_id: u32, _offset: u64, _metadata: Option<String>) -> Result<()> {
            unimplemented!()
        }
        async fn get_committed_offset(&self, _group_id: &str, _topic: &str, _partition_id: u32) -> Result<Option<u64>> {
            unimplemented!()
        }
        async fn get_consumer_offsets(&self, _group_id: &str) -> Result<Vec<ConsumerOffset>> {
            unimplemented!()
        }
        async fn list_consumer_groups(&self) -> Result<Vec<String>> {
            unimplemented!()
        }
        async fn delete_consumer_group(&self, _group_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn register_agent(&self, _agent: AgentInfo) -> Result<()> {
            unimplemented!()
        }
        async fn get_agent(&self, _agent_id: &str) -> Result<Option<AgentInfo>> {
            unimplemented!()
        }
        async fn list_agents(&self, _agent_group: Option<&str>, _availability_zone: Option<&str>) -> Result<Vec<AgentInfo>> {
            unimplemented!()
        }
        async fn deregister_agent(&self, _agent_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn acquire_partition_lease(&self, _topic: &str, _partition_id: u32, _agent_id: &str, _lease_duration_ms: i64) -> Result<PartitionLease> {
            unimplemented!()
        }
        async fn get_partition_lease(&self, _topic: &str, _partition_id: u32) -> Result<Option<PartitionLease>> {
            unimplemented!()
        }
        async fn release_partition_lease(&self, _topic: &str, _partition_id: u32, _agent_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn list_partition_leases(&self, _topic: Option<&str>, _agent_id: Option<&str>) -> Result<Vec<PartitionLease>> {
            unimplemented!()
        }
        async fn create_organization(&self, _config: CreateOrganization) -> Result<Organization> {
            unimplemented!()
        }
        async fn get_organization(&self, _id: &str) -> Result<Option<Organization>> {
            unimplemented!()
        }
        async fn get_organization_by_slug(&self, _slug: &str) -> Result<Option<Organization>> {
            unimplemented!()
        }
        async fn list_organizations(&self) -> Result<Vec<Organization>> {
            unimplemented!()
        }
        async fn update_organization_status(&self, _id: &str, _status: OrganizationStatus) -> Result<()> {
            unimplemented!()
        }
        async fn update_organization_plan(&self, _id: &str, _plan: OrganizationPlan) -> Result<()> {
            unimplemented!()
        }
        async fn delete_organization(&self, _id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn create_api_key(&self, _org_id: &str, _config: CreateApiKey, _key_hash: &str, _key_prefix: &str) -> Result<ApiKey> {
            unimplemented!()
        }
        async fn get_api_key(&self, _id: &str) -> Result<Option<ApiKey>> {
            unimplemented!()
        }
        async fn validate_api_key(&self, _key_hash: &str) -> Result<Option<ApiKey>> {
            unimplemented!()
        }
        async fn list_api_keys(&self, _organization_id: &str) -> Result<Vec<ApiKey>> {
            unimplemented!()
        }
        async fn touch_api_key(&self, _id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn revoke_api_key(&self, _id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn get_organization_quota(&self, _organization_id: &str) -> Result<OrganizationQuota> {
            unimplemented!()
        }
        async fn set_organization_quota(&self, _quota: OrganizationQuota) -> Result<()> {
            unimplemented!()
        }
        async fn get_organization_usage(&self, _organization_id: &str) -> Result<Vec<OrganizationUsage>> {
            unimplemented!()
        }
        async fn update_organization_usage(&self, _organization_id: &str, _metric: &str, _value: i64) -> Result<()> {
            unimplemented!()
        }
        async fn increment_organization_usage(&self, _organization_id: &str, _metric: &str, _delta: i64) -> Result<()> {
            unimplemented!()
        }
        async fn init_producer(&self, _config: InitProducerConfig) -> Result<Producer> {
            unimplemented!()
        }
        async fn get_producer(&self, _producer_id: &str) -> Result<Option<Producer>> {
            unimplemented!()
        }
        async fn get_producer_by_transactional_id(&self, _transactional_id: &str, _organization_id: Option<&str>) -> Result<Option<Producer>> {
            unimplemented!()
        }
        async fn update_producer_heartbeat(&self, _producer_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn fence_producer(&self, _producer_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn cleanup_expired_producers(&self, _timeout_ms: i64) -> Result<u64> {
            unimplemented!()
        }
        async fn allocate_numeric_producer_id(&self, _producer_id: &str) -> Result<i64> {
            unimplemented!()
        }
        async fn get_producer_by_numeric_id(&self, _numeric_id: i64) -> Result<Option<Producer>> {
            unimplemented!()
        }
        async fn get_producer_sequence(&self, _producer_id: &str, _topic: &str, _partition_id: u32) -> Result<Option<i64>> {
            unimplemented!()
        }
        async fn update_producer_sequence(&self, _producer_id: &str, _topic: &str, _partition_id: u32, _sequence: i64) -> Result<()> {
            unimplemented!()
        }
        async fn check_and_update_sequence(&self, _producer_id: &str, _topic: &str, _partition_id: u32, _base_sequence: i64, _record_count: u32) -> Result<bool> {
            unimplemented!()
        }
        async fn begin_transaction(&self, _producer_id: &str, _timeout_ms: u32) -> Result<Transaction> {
            unimplemented!()
        }
        async fn get_transaction(&self, _transaction_id: &str) -> Result<Option<Transaction>> {
            unimplemented!()
        }
        async fn add_transaction_partition(&self, _transaction_id: &str, _topic: &str, _partition_id: u32, _first_offset: u64) -> Result<()> {
            unimplemented!()
        }
        async fn update_transaction_partition_offset(&self, _transaction_id: &str, _topic: &str, _partition_id: u32, _last_offset: u64) -> Result<()> {
            unimplemented!()
        }
        async fn get_transaction_partitions(&self, _transaction_id: &str) -> Result<Vec<TransactionPartition>> {
            unimplemented!()
        }
        async fn prepare_transaction(&self, _transaction_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn commit_transaction(&self, _transaction_id: &str) -> Result<i64> {
            unimplemented!()
        }
        async fn abort_transaction(&self, _transaction_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn cleanup_completed_transactions(&self, _max_age_ms: i64) -> Result<u64> {
            unimplemented!()
        }
        async fn add_transaction_marker(&self, _marker: TransactionMarker) -> Result<()> {
            unimplemented!()
        }
        async fn get_transaction_markers(&self, _topic: &str, _partition_id: u32, _min_offset: u64) -> Result<Vec<TransactionMarker>> {
            unimplemented!()
        }
        async fn get_last_stable_offset(&self, _topic: &str, _partition_id: u32) -> Result<u64> {
            unimplemented!()
        }
        async fn update_last_stable_offset(&self, _topic: &str, _partition_id: u32, _lso: u64) -> Result<()> {
            unimplemented!()
        }
        async fn initiate_lease_transfer(&self, _topic: &str, _partition_id: u32, _from: &str, _to: &str, _reason: LeaderChangeReason, _timeout_ms: u32) -> Result<LeaseTransfer> {
            unimplemented!()
        }
        async fn accept_lease_transfer(&self, _transfer_id: &str, _agent_id: &str) -> Result<LeaseTransfer> {
            unimplemented!()
        }
        async fn complete_lease_transfer(&self, _transfer_id: &str, _last_flushed_offset: u64, _high_watermark: u64) -> Result<PartitionLease> {
            unimplemented!()
        }
        async fn reject_lease_transfer(&self, _transfer_id: &str, _agent_id: &str, _reason: &str) -> Result<()> {
            unimplemented!()
        }
        async fn get_lease_transfer(&self, _transfer_id: &str) -> Result<Option<LeaseTransfer>> {
            unimplemented!()
        }
        async fn get_pending_transfers_for_agent(&self, _agent_id: &str) -> Result<Vec<LeaseTransfer>> {
            unimplemented!()
        }
        async fn cleanup_timed_out_transfers(&self) -> Result<u64> {
            unimplemented!()
        }
        async fn record_leader_change(&self, _topic: &str, _partition_id: u32, _from_agent_id: Option<&str>, _to_agent_id: &str, _reason: LeaderChangeReason, _epoch: u64, _gap_ms: i64) -> Result<()> {
            unimplemented!()
        }
        async fn create_materialized_view(&self, _config: CreateMaterializedView) -> Result<MaterializedView> {
            unimplemented!()
        }
        async fn get_materialized_view(&self, _name: &str) -> Result<Option<MaterializedView>> {
            unimplemented!()
        }
        async fn get_materialized_view_by_id(&self, _id: &str) -> Result<Option<MaterializedView>> {
            unimplemented!()
        }
        async fn list_materialized_views(&self) -> Result<Vec<MaterializedView>> {
            unimplemented!()
        }
        async fn update_materialized_view_status(&self, _id: &str, _status: MaterializedViewStatus, _error_message: Option<&str>) -> Result<()> {
            unimplemented!()
        }
        async fn update_materialized_view_stats(&self, _id: &str, _row_count: u64, _last_refresh_at: i64) -> Result<()> {
            unimplemented!()
        }
        async fn delete_materialized_view(&self, _name: &str) -> Result<()> {
            unimplemented!()
        }
        async fn get_materialized_view_offsets(&self, _view_id: &str) -> Result<Vec<MaterializedViewOffset>> {
            unimplemented!()
        }
        async fn update_materialized_view_offset(&self, _view_id: &str, _partition_id: u32, _last_offset: u64) -> Result<()> {
            unimplemented!()
        }
        async fn get_materialized_view_data(&self, _view_id: &str, _limit: Option<usize>) -> Result<Vec<MaterializedViewData>> {
            unimplemented!()
        }
        async fn upsert_materialized_view_data(&self, _data: MaterializedViewData) -> Result<()> {
            unimplemented!()
        }
        async fn create_connector(&self, _connector: ConnectorInfo) -> Result<()> {
            unimplemented!()
        }
        async fn get_connector(&self, _name: &str) -> Result<Option<ConnectorInfo>> {
            unimplemented!()
        }
        async fn list_connectors(&self) -> Result<Vec<ConnectorInfo>> {
            unimplemented!()
        }
        async fn delete_connector(&self, _name: &str) -> Result<()> {
            unimplemented!()
        }
        async fn update_connector_state(&self, _name: &str, _state: &str, _error_message: Option<&str>) -> Result<()> {
            unimplemented!()
        }
        async fn update_connector_records_processed(&self, _name: &str, _records_processed: i64) -> Result<()> {
            unimplemented!()
        }
    }

    // ---------------------------------------------------------------
    // Test helpers
    // ---------------------------------------------------------------

    /// Build a segment from records, upload it to the object store, and return metadata.
    async fn build_and_upload_segment(
        object_store: &Arc<dyn ObjectStore>,
        topic: &str,
        partition_id: u32,
        records: &[Record],
        created_at: i64,
    ) -> SegmentInfo {
        let mut writer = SegmentWriter::new(Compression::Lz4);
        for record in records {
            writer.append(record).unwrap();
        }
        let segment_bytes = writer.finish().unwrap();
        let size_bytes = segment_bytes.len() as u64;
        let base_offset = records.first().unwrap().offset;
        let end_offset = records.last().unwrap().offset;

        let s3_key = format!("{}/{}/seg_{}.bin", topic, partition_id, base_offset);
        let path = object_store::path::Path::from(s3_key.clone());
        object_store
            .put(&path, Bytes::from(segment_bytes).into())
            .await
            .unwrap();

        SegmentInfo {
            id: format!("{}-{}-{}", topic, partition_id, base_offset),
            topic: topic.to_string(),
            partition_id,
            base_offset,
            end_offset,
            record_count: records.len() as u32,
            size_bytes,
            s3_bucket: "test-bucket".to_string(),
            s3_key,
            created_at,
        }
    }

    /// Helper to create a CompactionTask with test-friendly config.
    fn make_task(
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn ObjectStore>,
    ) -> CompactionTask {
        let tmp = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(tmp.path(), 100_000_000).unwrap());
        let config = CompactionConfig {
            check_interval: Duration::from_secs(1),
            max_records_per_run: 1_000_000,
            min_segment_age: Duration::from_secs(0), // no minimum age for tests
            tombstone_retention: Duration::from_secs(86400),
        };
        CompactionTask::new(metadata, object_store, cache, config)
    }

    // ---------------------------------------------------------------
    // Existing config/policy tests
    // ---------------------------------------------------------------

    #[test]
    fn test_compaction_config_default() {
        let config = CompactionConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(300));
        assert_eq!(config.max_records_per_run, 1_000_000);
    }

    #[test]
    fn test_cleanup_policy_is_compacted() {
        assert!(!CleanupPolicy::Delete.is_compacted());
        assert!(CleanupPolicy::Compact.is_compacted());
        assert!(CleanupPolicy::CompactAndDelete.is_compacted());
    }

    #[test]
    fn test_cleanup_policy_from_str() {
        assert_eq!(CleanupPolicy::from_str("delete"), CleanupPolicy::Delete);
        assert_eq!(CleanupPolicy::from_str("compact"), CleanupPolicy::Compact);
        assert_eq!(
            CleanupPolicy::from_str("compact,delete"),
            CleanupPolicy::CompactAndDelete
        );
        assert_eq!(
            CleanupPolicy::from_str("delete,compact"),
            CleanupPolicy::CompactAndDelete
        );
        assert_eq!(CleanupPolicy::from_str("unknown"), CleanupPolicy::Delete);
    }

    // ---------------------------------------------------------------
    // compact_partition: empty segments list
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_empty_segments() {
        let metadata = Arc::new(MockMetadataStore::new());
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let task = make_task(metadata, store);

        let result = task.compact_partition("test-topic", 0).await.unwrap();
        assert_eq!(result.records_processed, 0);
        assert_eq!(result.keys_removed, 0);
        assert_eq!(result.segments_compacted, 0);
        assert_eq!(result.bytes_saved, 0);
    }

    // ---------------------------------------------------------------
    // compact_partition: single segment (too few to compact)
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_single_segment_skipped() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let old_timestamp = 1_000_000; // old enough

        let records = vec![
            Record::new(0, 1000, Some(Bytes::from("key1")), Bytes::from("val1")),
            Record::new(1, 1001, Some(Bytes::from("key2")), Bytes::from("val2")),
        ];
        let seg = build_and_upload_segment(&store, "topic", 0, &records, old_timestamp).await;
        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg]));
        let task = make_task(metadata, store);

        // Only 1 eligible segment -> should skip compaction
        let result = task.compact_partition("topic", 0).await.unwrap();
        assert_eq!(result.records_processed, 0);
        assert_eq!(result.segments_compacted, 0);
    }

    // ---------------------------------------------------------------
    // compact_partition: segments too new (skipped due to min_segment_age)
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_segments_too_new() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let future_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + 3_600_000; // 1 hour in the future

        let records1 = vec![
            Record::new(0, 1000, Some(Bytes::from("key1")), Bytes::from("val1")),
        ];
        let records2 = vec![
            Record::new(1, 1001, Some(Bytes::from("key1")), Bytes::from("val2")),
        ];
        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, future_timestamp).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, future_timestamp).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let tmp = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(tmp.path(), 100_000_000).unwrap());
        let config = CompactionConfig {
            check_interval: Duration::from_secs(1),
            max_records_per_run: 1_000_000,
            min_segment_age: Duration::from_secs(7200), // 2 hours
            tombstone_retention: Duration::from_secs(86400),
        };
        let task = CompactionTask::new(metadata, store, cache, config);

        let result = task.compact_partition("topic", 0).await.unwrap();
        // Both segments are too new -> 0 eligible -> skipped
        assert_eq!(result.records_processed, 0);
        assert_eq!(result.segments_compacted, 0);
    }

    // ---------------------------------------------------------------
    // compact_partition: key-map deduplication (latest value wins)
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_deduplication() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let old_ts = 1_000_000;

        // Segment 1: key1=val1, key2=val2
        let records1 = vec![
            Record::new(0, 1000, Some(Bytes::from("key1")), Bytes::from("val1_old")),
            Record::new(1, 1001, Some(Bytes::from("key2")), Bytes::from("val2_old")),
        ];
        // Segment 2: key1=val1_new (overwrites), key3=val3 (new key)
        let records2 = vec![
            Record::new(2, 1002, Some(Bytes::from("key1")), Bytes::from("val1_new")),
            Record::new(3, 1003, Some(Bytes::from("key3")), Bytes::from("val3")),
        ];

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let task = make_task(metadata.clone(), store.clone());

        let result = task.compact_partition("topic", 0).await.unwrap();

        // 4 records processed, 3 unique keys -> 1 removed
        assert_eq!(result.records_processed, 4);
        assert_eq!(result.keys_removed, 1);
        assert_eq!(result.segments_compacted, 2);

        // Verify the compacted segment was uploaded and is readable
        let compacted_path = object_store::path::Path::from("topic/0/seg_1_compacted.bin");
        let compacted_data = store.get(&compacted_path).await.unwrap().bytes().await.unwrap();
        let reader = SegmentReader::new(compacted_data).unwrap();
        let compacted_records = reader.read_all().unwrap();

        assert_eq!(compacted_records.len(), 3);

        // Records should be sorted by offset: key2(offset=1), key1_new(offset=2), key3(offset=3)
        assert_eq!(compacted_records[0].offset, 1);
        assert_eq!(compacted_records[0].key.as_ref().unwrap().as_ref(), b"key2");
        assert_eq!(compacted_records[0].value.as_ref(), b"val2_old");

        assert_eq!(compacted_records[1].offset, 2);
        assert_eq!(compacted_records[1].key.as_ref().unwrap().as_ref(), b"key1");
        assert_eq!(compacted_records[1].value.as_ref(), b"val1_new");

        assert_eq!(compacted_records[2].offset, 3);
        assert_eq!(compacted_records[2].key.as_ref().unwrap().as_ref(), b"key3");
        assert_eq!(compacted_records[2].value.as_ref(), b"val3");
    }

    // ---------------------------------------------------------------
    // compact_partition: tombstone handling
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_tombstones() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let old_ts = now_ms - 3_600_000; // 1 hour ago (segment created_at)

        // Segment 1: key1=val1, key2=val2
        // Use recent timestamps for the records so tombstones aren't expired
        let records1 = vec![
            Record::new(0, now_ms as u64, Some(Bytes::from("key1")), Bytes::from("val1")),
            Record::new(1, now_ms as u64, Some(Bytes::from("key2")), Bytes::from("val2")),
        ];
        // Segment 2: key1 tombstone (empty value), key3=val3
        let records2 = vec![
            Record::new(2, now_ms as u64, Some(Bytes::from("key1")), Bytes::new()), // tombstone
            Record::new(3, now_ms as u64, Some(Bytes::from("key3")), Bytes::from("val3")),
        ];

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let task = make_task(metadata, store.clone());

        let result = task.compact_partition("topic", 0).await.unwrap();

        // 4 records processed; after compaction: key2(1) + key1_tombstone(2) + key3(3) = 3
        assert_eq!(result.records_processed, 4);
        assert_eq!(result.keys_removed, 1); // key1's original value removed

        // Verify compacted segment content
        let compacted_path = object_store::path::Path::from("topic/0/seg_1_compacted.bin");
        let compacted_data = store.get(&compacted_path).await.unwrap().bytes().await.unwrap();
        let reader = SegmentReader::new(compacted_data).unwrap();
        let compacted_records = reader.read_all().unwrap();

        assert_eq!(compacted_records.len(), 3);

        // key2 survives as normal record
        assert_eq!(compacted_records[0].key.as_ref().unwrap().as_ref(), b"key2");
        assert!(!compacted_records[0].value.is_empty());

        // key1 tombstone survives (empty value, recent timestamp)
        assert_eq!(compacted_records[1].key.as_ref().unwrap().as_ref(), b"key1");
        assert!(compacted_records[1].value.is_empty());

        // key3 survives as normal record
        assert_eq!(compacted_records[2].key.as_ref().unwrap().as_ref(), b"key3");
    }

    // ---------------------------------------------------------------
    // compact_partition: expired tombstones are removed
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_expired_tombstones_removed() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let old_ts = 1_000_000; // segment created_at

        // Segment 1: key1=val1
        let records1 = vec![
            Record::new(0, 1000, Some(Bytes::from("key1")), Bytes::from("val1")),
        ];
        // Segment 2: key1 tombstone with very old timestamp
        let records2 = vec![
            Record::new(1, 1001, Some(Bytes::from("key1")), Bytes::new()), // tombstone
        ];

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let tmp = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(tmp.path(), 100_000_000).unwrap());
        let config = CompactionConfig {
            check_interval: Duration::from_secs(1),
            max_records_per_run: 1_000_000,
            min_segment_age: Duration::from_secs(0),
            tombstone_retention: Duration::from_secs(1), // 1 second retention
        };
        let task = CompactionTask::new(metadata, store.clone(), cache, config);

        // Tombstone timestamp is 1001ms since epoch, which is definitely expired
        // given tombstone_retention of 1 second relative to now
        let result = task.compact_partition("topic", 0).await.unwrap();

        // 2 records processed, tombstone expired, key1 removed by tombstone
        // -> 0 surviving records... but wait, the tombstone removed key1 from key_map,
        // then the tombstone itself expires. So nothing survives.
        assert_eq!(result.records_processed, 2);
        assert_eq!(result.keys_removed, 2); // both removed: key1 overwritten by tombstone, tombstone expired
    }

    // ---------------------------------------------------------------
    // compact_partition: unkeyed records are preserved
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_unkeyed_records_preserved() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let old_ts = 1_000_000;

        // Segment 1: unkeyed records + keyed record
        let records1 = vec![
            Record::new(0, 1000, None, Bytes::from("unkeyed1")),
            Record::new(1, 1001, Some(Bytes::from("key1")), Bytes::from("val1_old")),
        ];
        // Segment 2: unkeyed + duplicate key
        let records2 = vec![
            Record::new(2, 1002, None, Bytes::from("unkeyed2")),
            Record::new(3, 1003, Some(Bytes::from("key1")), Bytes::from("val1_new")),
        ];

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let task = make_task(metadata, store.clone());

        let result = task.compact_partition("topic", 0).await.unwrap();

        // 4 records processed: 2 unkeyed (kept), 1 keyed (deduped to latest) -> 3 output
        assert_eq!(result.records_processed, 4);
        assert_eq!(result.keys_removed, 1);

        // Verify compacted content
        let compacted_path = object_store::path::Path::from("topic/0/seg_0_compacted.bin");
        let compacted_data = store.get(&compacted_path).await.unwrap().bytes().await.unwrap();
        let reader = SegmentReader::new(compacted_data).unwrap();
        let compacted_records = reader.read_all().unwrap();

        assert_eq!(compacted_records.len(), 3);
        // Sorted by offset: unkeyed1(0), unkeyed2(2), key1_new(3)
        assert!(compacted_records[0].key.is_none());
        assert_eq!(compacted_records[0].value.as_ref(), b"unkeyed1");

        assert!(compacted_records[1].key.is_none());
        assert_eq!(compacted_records[1].value.as_ref(), b"unkeyed2");

        assert_eq!(compacted_records[2].key.as_ref().unwrap().as_ref(), b"key1");
        assert_eq!(compacted_records[2].value.as_ref(), b"val1_new");
    }

    // ---------------------------------------------------------------
    // compact_partition: many duplicate keys across multiple segments
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_many_duplicates() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let old_ts = 1_000_000;

        // Segment 1: 10 records for same key
        let records1: Vec<Record> = (0..10)
            .map(|i| {
                Record::new(
                    i,
                    1000 + i,
                    Some(Bytes::from("same-key")),
                    Bytes::from(format!("val-seg1-{}", i)),
                )
            })
            .collect();

        // Segment 2: 10 more records for same key (offsets 10-19)
        let records2: Vec<Record> = (10..20)
            .map(|i| {
                Record::new(
                    i,
                    1000 + i,
                    Some(Bytes::from("same-key")),
                    Bytes::from(format!("val-seg2-{}", i)),
                )
            })
            .collect();

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let task = make_task(metadata, store.clone());

        let result = task.compact_partition("topic", 0).await.unwrap();

        // 20 records processed, 1 unique key -> 19 removed
        assert_eq!(result.records_processed, 20);
        assert_eq!(result.keys_removed, 19);
        assert_eq!(result.segments_compacted, 2);

        // Verify only the latest value survives
        let compacted_path = object_store::path::Path::from("topic/0/seg_19_compacted.bin");
        let compacted_data = store.get(&compacted_path).await.unwrap().bytes().await.unwrap();
        let reader = SegmentReader::new(compacted_data).unwrap();
        let compacted_records = reader.read_all().unwrap();

        assert_eq!(compacted_records.len(), 1);
        assert_eq!(compacted_records[0].offset, 19);
        assert_eq!(compacted_records[0].value.as_ref(), b"val-seg2-19");
    }

    // ---------------------------------------------------------------
    // compact_partition: tombstone then new value for same key
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_tombstone_then_new_value() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let old_ts = now_ms - 3_600_000;

        // Segment 1: key1=val1
        let records1 = vec![
            Record::new(0, now_ms as u64, Some(Bytes::from("key1")), Bytes::from("val1")),
        ];
        // Segment 2: key1 tombstone
        let records2 = vec![
            Record::new(1, now_ms as u64, Some(Bytes::from("key1")), Bytes::new()),
        ];
        // Segment 3: key1=val1_resurrected (new value after tombstone)
        let records3 = vec![
            Record::new(2, now_ms as u64, Some(Bytes::from("key1")), Bytes::from("val1_resurrected")),
        ];

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;
        let seg3 = build_and_upload_segment(&store, "topic", 0, &records3, old_ts).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2, seg3]));
        let task = make_task(metadata, store.clone());

        let result = task.compact_partition("topic", 0).await.unwrap();

        // 3 records processed, 1 unique key output -> 2 removed
        assert_eq!(result.records_processed, 3);
        assert_eq!(result.keys_removed, 2);

        // Verify the resurrected value survives (tombstone was superseded)
        let compacted_path = object_store::path::Path::from("topic/0/seg_2_compacted.bin");
        let compacted_data = store.get(&compacted_path).await.unwrap().bytes().await.unwrap();
        let reader = SegmentReader::new(compacted_data).unwrap();
        let compacted_records = reader.read_all().unwrap();

        assert_eq!(compacted_records.len(), 1);
        assert_eq!(compacted_records[0].key.as_ref().unwrap().as_ref(), b"key1");
        assert_eq!(compacted_records[0].value.as_ref(), b"val1_resurrected");
    }

    // ---------------------------------------------------------------
    // compact_partition: max_records_per_run limit
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_max_records_limit() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let old_ts = 1_000_000;

        // Segment 1: 5 records
        let records1: Vec<Record> = (0..5)
            .map(|i| {
                Record::new(
                    i,
                    1000 + i,
                    Some(Bytes::from(format!("key{}", i))),
                    Bytes::from(format!("val{}", i)),
                )
            })
            .collect();

        // Segment 2: 5 more records
        let records2: Vec<Record> = (5..10)
            .map(|i| {
                Record::new(
                    i,
                    1000 + i,
                    Some(Bytes::from(format!("key{}", i))),
                    Bytes::from(format!("val{}", i)),
                )
            })
            .collect();

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let tmp = tempfile::tempdir().unwrap();
        let cache = Arc::new(SegmentCache::new(tmp.path(), 100_000_000).unwrap());
        let config = CompactionConfig {
            check_interval: Duration::from_secs(1),
            max_records_per_run: 5, // Only process 5 records
            min_segment_age: Duration::from_secs(0),
            tombstone_retention: Duration::from_secs(86400),
        };
        let task = CompactionTask::new(metadata, store.clone(), cache, config);

        let result = task.compact_partition("topic", 0).await.unwrap();

        // Should process only the first segment's records (5), then stop
        assert_eq!(result.records_processed, 5);
    }

    // ---------------------------------------------------------------
    // compact_partition: old segment data is cleaned up from object store
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_compact_partition_old_segments_deleted() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let old_ts = 1_000_000;

        let records1 = vec![
            Record::new(0, 1000, Some(Bytes::from("key1")), Bytes::from("val1")),
        ];
        let records2 = vec![
            Record::new(1, 1001, Some(Bytes::from("key1")), Bytes::from("val2")),
        ];

        let seg1 = build_and_upload_segment(&store, "topic", 0, &records1, old_ts).await;
        let seg2 = build_and_upload_segment(&store, "topic", 0, &records2, old_ts).await;
        let old_key1 = seg1.s3_key.clone();
        let old_key2 = seg2.s3_key.clone();

        let metadata = Arc::new(MockMetadataStore::with_segments(vec![seg1, seg2]));
        let task = make_task(metadata, store.clone());

        let _result = task.compact_partition("topic", 0).await.unwrap();

        // Old segments should have been deleted from object store
        let old_path1 = object_store::path::Path::from(old_key1);
        let old_path2 = object_store::path::Path::from(old_key2);
        assert!(store.get(&old_path1).await.is_err());
        assert!(store.get(&old_path2).await.is_err());

        // New compacted segment should exist
        let compacted_path = object_store::path::Path::from("topic/0/seg_1_compacted.bin");
        assert!(store.get(&compacted_path).await.is_ok());
    }

    // ---------------------------------------------------------------
    // CompactionConfig: defaults and custom values
    // ---------------------------------------------------------------

    #[test]
    fn test_compaction_config_default_values_detailed() {
        let config = CompactionConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(300));
        assert_eq!(config.max_records_per_run, 1_000_000);
        assert_eq!(config.min_segment_age, Duration::from_secs(3600));
        assert_eq!(config.tombstone_retention, Duration::from_secs(86400));
    }

    #[test]
    fn test_compaction_config_custom_values() {
        let config = CompactionConfig {
            check_interval: Duration::from_secs(60),
            max_records_per_run: 500,
            min_segment_age: Duration::from_secs(120),
            tombstone_retention: Duration::from_secs(3600),
        };
        assert_eq!(config.check_interval, Duration::from_secs(60));
        assert_eq!(config.max_records_per_run, 500);
        assert_eq!(config.min_segment_age, Duration::from_secs(120));
        assert_eq!(config.tombstone_retention, Duration::from_secs(3600));
    }

    #[test]
    fn test_compaction_config_clone() {
        let config = CompactionConfig {
            check_interval: Duration::from_millis(100),
            max_records_per_run: 42,
            min_segment_age: Duration::from_secs(10),
            tombstone_retention: Duration::from_secs(20),
        };
        let cloned = config.clone();
        assert_eq!(cloned.check_interval, config.check_interval);
        assert_eq!(cloned.max_records_per_run, config.max_records_per_run);
        assert_eq!(cloned.min_segment_age, config.min_segment_age);
        assert_eq!(cloned.tombstone_retention, config.tombstone_retention);
    }

    // ---------------------------------------------------------------
    // CompactionStats: defaults and tracking
    // ---------------------------------------------------------------

    #[test]
    fn test_compaction_stats_default() {
        let stats = CompactionStats::default();
        assert_eq!(stats.runs_completed, 0);
        assert_eq!(stats.records_processed, 0);
        assert_eq!(stats.keys_removed, 0);
        assert_eq!(stats.segments_compacted, 0);
        assert_eq!(stats.bytes_saved, 0);
    }

    #[test]
    fn test_compaction_stats_clone() {
        let mut stats = CompactionStats::default();
        stats.runs_completed = 5;
        stats.records_processed = 1000;
        stats.keys_removed = 200;
        stats.segments_compacted = 10;
        stats.bytes_saved = 50_000;

        let cloned = stats.clone();
        assert_eq!(cloned.runs_completed, 5);
        assert_eq!(cloned.records_processed, 1000);
        assert_eq!(cloned.keys_removed, 200);
        assert_eq!(cloned.segments_compacted, 10);
        assert_eq!(cloned.bytes_saved, 50_000);
    }

    // ---------------------------------------------------------------
    // CompactionError: Display and variant tests
    // ---------------------------------------------------------------

    #[test]
    fn test_compaction_error_metadata_display() {
        let err = CompactionError::Metadata("connection lost".to_string());
        assert_eq!(err.to_string(), "Metadata error: connection lost");
    }

    #[test]
    fn test_compaction_error_storage_display() {
        let err = CompactionError::Storage("disk full".to_string());
        assert_eq!(err.to_string(), "Storage error: disk full");
    }

    #[test]
    fn test_compaction_error_segment_read_display() {
        let err = CompactionError::SegmentRead("corrupt header".to_string());
        assert_eq!(err.to_string(), "Segment read error: corrupt header");
    }

    // ---------------------------------------------------------------
    // CompactionResult: default values
    // ---------------------------------------------------------------

    #[test]
    fn test_compaction_result_default() {
        let result = CompactionResult::default();
        assert_eq!(result.records_processed, 0);
        assert_eq!(result.keys_removed, 0);
        assert_eq!(result.segments_compacted, 0);
        assert_eq!(result.bytes_saved, 0);
    }

    // ---------------------------------------------------------------
    // CompactionTask: stats accessor
    // ---------------------------------------------------------------

    #[test]
    fn test_compaction_task_stats_accessor() {
        let metadata: Arc<dyn MetadataStore> = Arc::new(MockMetadataStore::new());
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let task = make_task(metadata, store);

        let stats = task.stats();
        assert_eq!(stats.runs_completed, 0);
        assert_eq!(stats.records_processed, 0);
    }
}
