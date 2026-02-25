//! Enhanced Log Compaction with Tombstone Handling
//!
//! Implements log compaction for StreamHouse topics with `Compact` cleanup policy.
//! Log compaction ensures that the log always retains at least the latest value
//! for each key within a partition.
//!
//! ## Compaction Strategy
//!
//! - Segments are scanned for duplicate keys
//! - When the dirty ratio (duplicate keys / total keys) exceeds the configured threshold,
//!   compaction is triggered
//! - Tombstones (records with null value) are retained for a configurable period
//!   before being expired
//!
//! ## Tombstone Handling
//!
//! When a producer writes a null value for a key, it creates a "tombstone" marker.
//! Tombstones are kept for `tombstone_retention` to ensure downstream consumers
//! have time to observe the deletion. After the retention period, they are removed
//! during compaction.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Errors specific to the compaction system.
#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("Partition not found: {topic}/{partition}")]
    PartitionNotFound { topic: String, partition: u32 },

    #[error("Compaction in progress for {topic}/{partition}")]
    CompactionInProgress { topic: String, partition: u32 },

    #[error("Compaction not needed: dirty ratio {ratio:.2} below threshold {threshold:.2}")]
    CompactionNotNeeded { ratio: f64, threshold: f64 },

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, CompactionError>;

/// Configuration for the compaction scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Minimum interval between compactions for the same partition.
    #[serde(with = "duration_serde")]
    pub min_compaction_interval: Duration,
    /// How long to keep tombstone (delete) markers before expiring them.
    #[serde(with = "duration_serde")]
    pub tombstone_retention: Duration,
    /// Minimum ratio of duplicate keys before compaction is triggered (0.0 to 1.0).
    /// A value of 0.5 means compaction triggers when 50% of keys are duplicates.
    pub min_dirty_ratio: f64,
    /// Maximum segment size in bytes after compaction.
    pub max_segment_size_bytes: u64,
    /// Number of threads to use for compaction operations.
    pub compaction_threads: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_compaction_interval: Duration::from_secs(600), // 10 minutes
            tombstone_retention: Duration::from_secs(86400),   // 24 hours
            min_dirty_ratio: 0.5,
            max_segment_size_bytes: 1_073_741_824, // 1 GB
            compaction_threads: 2,
        }
    }
}

/// Serde helpers for Duration serialization.
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

/// State of compaction for a specific partition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionState {
    pub topic: String,
    pub partition: u32,
    /// The highest offset that has been compacted.
    pub last_compacted_offset: u64,
    /// When compaction was last performed.
    #[serde(skip)]
    pub last_compaction_at: Option<Instant>,
    /// Total number of unique keys before compaction.
    pub total_keys_before: u64,
    /// Total number of unique keys after compaction.
    pub total_keys_after: u64,
    /// Number of tombstones waiting to be expired.
    pub tombstones_pending: u64,
    /// Current dirty ratio (duplicate keys / total keys).
    pub dirty_ratio: f64,
}

/// Result of a compaction operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Number of segments that were compacted.
    pub segments_compacted: usize,
    /// Number of duplicate keys removed.
    pub keys_removed: u64,
    /// Number of tombstones that were expired (permanently removed).
    pub tombstones_expired: u64,
    /// Bytes saved by compaction.
    pub bytes_saved: u64,
    /// How long the compaction took.
    #[serde(with = "duration_serde")]
    pub duration: Duration,
}

/// Aggregate statistics for the compaction scheduler.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactionStats {
    /// Total number of partitions being tracked.
    pub tracked_partitions: usize,
    /// Number of partitions needing compaction.
    pub partitions_needing_compaction: usize,
    /// Total tombstones pending expiration across all partitions.
    pub total_tombstones_pending: u64,
    /// Average dirty ratio across all partitions.
    pub average_dirty_ratio: f64,
    /// Total keys removed in all compaction runs.
    pub total_keys_removed: u64,
}

/// A simulated record for compaction processing.
#[derive(Debug, Clone)]
pub struct CompactionRecord {
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
    pub offset: u64,
    pub timestamp: i64,
    pub size_bytes: u64,
}

/// The compaction scheduler.
///
/// Manages compaction state for all partitions and decides when
/// compaction should be triggered based on the dirty ratio.
pub struct CompactionScheduler {
    config: CompactionConfig,
    state: RwLock<HashMap<(String, u32), CompactionState>>,
}

impl CompactionScheduler {
    /// Create a new CompactionScheduler with the given configuration.
    pub fn new(config: CompactionConfig) -> Self {
        Self {
            config,
            state: RwLock::new(HashMap::new()),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(CompactionConfig::default())
    }

    /// Get the current configuration.
    pub fn config(&self) -> &CompactionConfig {
        &self.config
    }

    /// Check if a partition should be compacted based on its dirty ratio
    /// and the time since last compaction.
    pub async fn should_compact(&self, topic: &str, partition: u32) -> Result<bool> {
        let state = self.state.read().await;
        let key = (topic.to_string(), partition);

        let cs = state.get(&key).ok_or_else(|| CompactionError::PartitionNotFound {
            topic: topic.to_string(),
            partition,
        })?;

        // Check minimum interval
        if let Some(last) = cs.last_compaction_at {
            if last.elapsed() < self.config.min_compaction_interval {
                return Ok(false);
            }
        }

        // Check dirty ratio threshold
        Ok(cs.dirty_ratio >= self.config.min_dirty_ratio)
    }

    /// Perform compaction on a partition using the provided records.
    ///
    /// This method simulates the compaction process:
    /// 1. Scans records for duplicate keys
    /// 2. Keeps only the latest value for each key
    /// 3. Handles tombstone expiration
    ///
    /// Returns the compaction result with statistics.
    pub async fn compact_partition(
        &self,
        topic: &str,
        partition: u32,
        records: &[CompactionRecord],
        current_time_ms: i64,
    ) -> Result<CompactionResult> {
        let start = Instant::now();

        // Track the latest record for each key
        let mut latest_by_key: HashMap<Vec<u8>, &CompactionRecord> = HashMap::new();
        let total_before = records.len() as u64;
        let mut total_bytes_before: u64 = 0;

        for record in records {
            total_bytes_before += record.size_bytes;
            latest_by_key.insert(record.key.clone(), record);
        }

        let total_after = latest_by_key.len() as u64;
        let keys_removed = total_before.saturating_sub(total_after);

        // Expire tombstones
        let tombstone_threshold_ms =
            current_time_ms - self.config.tombstone_retention.as_millis() as i64;
        let mut tombstones_expired: u64 = 0;

        latest_by_key.retain(|_, record| {
            if record.value.is_none() && record.timestamp < tombstone_threshold_ms {
                tombstones_expired += 1;
                false
            } else {
                true
            }
        });

        let final_count = latest_by_key.len() as u64;
        let mut total_bytes_after: u64 = 0;
        let mut tombstones_pending: u64 = 0;

        for record in latest_by_key.values() {
            total_bytes_after += record.size_bytes;
            if record.value.is_none() {
                tombstones_pending += 1;
            }
        }

        let bytes_saved = total_bytes_before.saturating_sub(total_bytes_after);
        let duration = start.elapsed();

        // Update compaction state
        let mut state = self.state.write().await;
        let key = (topic.to_string(), partition);
        let cs = state.get_mut(&key).ok_or_else(|| CompactionError::PartitionNotFound {
            topic: topic.to_string(),
            partition,
        })?;

        let max_offset = records.iter().map(|r| r.offset).max().unwrap_or(0);
        cs.last_compacted_offset = max_offset;
        cs.last_compaction_at = Some(Instant::now());
        cs.total_keys_before = total_before;
        cs.total_keys_after = final_count;
        cs.tombstones_pending = tombstones_pending;
        cs.dirty_ratio = if total_before > 0 {
            1.0 - (final_count as f64 / total_before as f64)
        } else {
            0.0
        };

        let result = CompactionResult {
            segments_compacted: 1,
            keys_removed,
            tombstones_expired,
            bytes_saved,
            duration,
        };

        info!(
            "Compacted {}/{}: removed {} keys, expired {} tombstones, saved {} bytes in {:?}",
            topic, partition, keys_removed, tombstones_expired, bytes_saved, duration
        );

        Ok(result)
    }

    /// Expire tombstones for a partition without performing full compaction.
    ///
    /// Useful for periodic tombstone cleanup independent of the compaction schedule.
    pub async fn expire_tombstones(
        &self,
        topic: &str,
        partition: u32,
        records: &[CompactionRecord],
        current_time_ms: i64,
    ) -> Result<u64> {
        let tombstone_threshold_ms =
            current_time_ms - self.config.tombstone_retention.as_millis() as i64;

        let expired = records
            .iter()
            .filter(|r| r.value.is_none() && r.timestamp < tombstone_threshold_ms)
            .count() as u64;

        // Update state
        let mut state = self.state.write().await;
        let key = (topic.to_string(), partition);
        if let Some(cs) = state.get_mut(&key) {
            cs.tombstones_pending = cs.tombstones_pending.saturating_sub(expired);
        }

        debug!(
            "Expired {} tombstones for {}/{}",
            expired, topic, partition
        );

        Ok(expired)
    }

    /// Get compaction statistics across all tracked partitions.
    pub async fn get_compaction_stats(&self) -> CompactionStats {
        let state = self.state.read().await;
        let mut stats = CompactionStats::default();

        stats.tracked_partitions = state.len();
        let mut total_dirty_ratio = 0.0;

        for cs in state.values() {
            if cs.dirty_ratio >= self.config.min_dirty_ratio {
                stats.partitions_needing_compaction += 1;
            }
            stats.total_tombstones_pending += cs.tombstones_pending;
            total_dirty_ratio += cs.dirty_ratio;
            stats.total_keys_removed += cs.total_keys_before.saturating_sub(cs.total_keys_after);
        }

        if stats.tracked_partitions > 0 {
            stats.average_dirty_ratio = total_dirty_ratio / stats.tracked_partitions as f64;
        }

        stats
    }

    /// Schedule a partition for compaction tracking.
    ///
    /// Must be called before `should_compact` or `compact_partition`.
    pub async fn schedule_compaction(
        &self,
        topic: String,
        partition: u32,
        dirty_ratio: f64,
        tombstones_pending: u64,
    ) -> Result<()> {
        let mut state = self.state.write().await;
        let key = (topic.clone(), partition);

        let cs = CompactionState {
            topic,
            partition,
            last_compacted_offset: 0,
            last_compaction_at: None,
            total_keys_before: 0,
            total_keys_after: 0,
            tombstones_pending,
            dirty_ratio,
        };

        state.insert(key, cs);
        Ok(())
    }

    /// Update the dirty ratio for a partition.
    pub async fn update_dirty_ratio(
        &self,
        topic: &str,
        partition: u32,
        dirty_ratio: f64,
    ) -> Result<()> {
        let mut state = self.state.write().await;
        let key = (topic.to_string(), partition);

        let cs = state.get_mut(&key).ok_or_else(|| CompactionError::PartitionNotFound {
            topic: topic.to_string(),
            partition,
        })?;

        cs.dirty_ratio = dirty_ratio;
        Ok(())
    }

    /// Get the compaction state for a specific partition.
    pub async fn get_partition_state(
        &self,
        topic: &str,
        partition: u32,
    ) -> Result<CompactionState> {
        let state = self.state.read().await;
        let key = (topic.to_string(), partition);

        state
            .get(&key)
            .cloned()
            .ok_or_else(|| CompactionError::PartitionNotFound {
                topic: topic.to_string(),
                partition,
            })
    }

    /// Remove a partition from compaction tracking.
    pub async fn remove_partition(&self, topic: &str, partition: u32) {
        let mut state = self.state.write().await;
        let key = (topic.to_string(), partition);
        state.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_scheduler() -> CompactionScheduler {
        CompactionScheduler::with_defaults()
    }

    fn make_records(keys: &[(&[u8], Option<&[u8]>, u64, i64)]) -> Vec<CompactionRecord> {
        keys.iter()
            .map(|(key, value, offset, timestamp)| CompactionRecord {
                key: key.to_vec(),
                value: value.map(|v| v.to_vec()),
                offset: *offset,
                timestamp: *timestamp,
                size_bytes: 100,
            })
            .collect()
    }

    // Test 1: should_compact returns true when dirty ratio exceeds threshold
    #[tokio::test]
    async fn test_should_compact_above_threshold() {
        let scheduler = default_scheduler();

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.6, 0)
            .await
            .unwrap();

        let should = scheduler.should_compact("topic", 0).await.unwrap();
        assert!(should);
    }

    // Test 2: should_compact returns false when dirty ratio is below threshold
    #[tokio::test]
    async fn test_should_compact_below_threshold() {
        let scheduler = default_scheduler();

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.3, 0)
            .await
            .unwrap();

        let should = scheduler.should_compact("topic", 0).await.unwrap();
        assert!(!should);
    }

    // Test 3: Compact partition removes duplicate keys
    #[tokio::test]
    async fn test_compact_removes_duplicates() {
        let scheduler = default_scheduler();
        let now_ms = 1700000000000_i64;

        scheduler
            .schedule_compaction("orders".to_string(), 0, 0.5, 0)
            .await
            .unwrap();

        let records = make_records(&[
            (b"key-1", Some(b"value-1-old"), 0, now_ms),
            (b"key-2", Some(b"value-2"), 1, now_ms),
            (b"key-1", Some(b"value-1-new"), 2, now_ms + 1000), // duplicate key-1
            (b"key-3", Some(b"value-3"), 3, now_ms),
            (b"key-2", Some(b"value-2-new"), 4, now_ms + 2000), // duplicate key-2
        ]);

        let result = scheduler
            .compact_partition("orders", 0, &records, now_ms + 3000)
            .await
            .unwrap();

        assert_eq!(result.keys_removed, 2); // 5 total - 3 unique = 2 removed
        assert_eq!(result.segments_compacted, 1);
        assert!(result.bytes_saved > 0);
    }

    // Test 4: Tombstone handling - recent tombstones are kept
    #[tokio::test]
    async fn test_tombstones_kept_when_recent() {
        let scheduler = default_scheduler();
        let now_ms = 1700000000000_i64;

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.5, 0)
            .await
            .unwrap();

        let records = make_records(&[
            (b"key-1", Some(b"value-1"), 0, now_ms),
            (b"key-2", None, 1, now_ms), // tombstone - recent
        ]);

        let result = scheduler
            .compact_partition("topic", 0, &records, now_ms + 1000)
            .await
            .unwrap();

        // Tombstone should not be expired (it's recent)
        assert_eq!(result.tombstones_expired, 0);
    }

    // Test 5: Tombstone handling - old tombstones are expired
    #[tokio::test]
    async fn test_tombstones_expired_when_old() {
        let scheduler = default_scheduler();
        let now_ms = 1700000000000_i64;

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.5, 0)
            .await
            .unwrap();

        let records = make_records(&[
            (b"key-1", Some(b"value-1"), 0, now_ms),
            (b"key-2", None, 1, now_ms), // tombstone
        ]);

        // Compact after tombstone_retention (24 hours default)
        let future = now_ms + 86400 * 1000 + 1000; // 24h + 1s
        let result = scheduler
            .compact_partition("topic", 0, &records, future)
            .await
            .unwrap();

        assert_eq!(result.tombstones_expired, 1);
    }

    // Test 6: Compaction state is updated after compaction
    #[tokio::test]
    async fn test_compaction_state_updated() {
        let scheduler = default_scheduler();
        let now_ms = 1700000000000_i64;

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.6, 0)
            .await
            .unwrap();

        let records = make_records(&[
            (b"key-1", Some(b"v1"), 0, now_ms),
            (b"key-1", Some(b"v2"), 1, now_ms + 100),
            (b"key-2", Some(b"v3"), 2, now_ms + 200),
        ]);

        scheduler
            .compact_partition("topic", 0, &records, now_ms + 1000)
            .await
            .unwrap();

        let state = scheduler.get_partition_state("topic", 0).await.unwrap();
        assert_eq!(state.last_compacted_offset, 2);
        assert_eq!(state.total_keys_after, 2); // key-1 and key-2
        assert!(state.last_compaction_at.is_some());
    }

    // Test 7: Partition not found error
    #[tokio::test]
    async fn test_partition_not_found() {
        let scheduler = default_scheduler();

        let result = scheduler.should_compact("nonexistent", 0).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CompactionError::PartitionNotFound { .. } => {}
            e => panic!("Expected PartitionNotFound, got {:?}", e),
        }
    }

    // Test 8: Compaction stats across multiple partitions
    #[tokio::test]
    async fn test_compaction_stats() {
        let scheduler = default_scheduler();

        scheduler
            .schedule_compaction("t1".to_string(), 0, 0.6, 5)
            .await
            .unwrap();
        scheduler
            .schedule_compaction("t1".to_string(), 1, 0.3, 2)
            .await
            .unwrap();
        scheduler
            .schedule_compaction("t2".to_string(), 0, 0.8, 10)
            .await
            .unwrap();

        let stats = scheduler.get_compaction_stats().await;
        assert_eq!(stats.tracked_partitions, 3);
        assert_eq!(stats.partitions_needing_compaction, 2); // t1/0 and t2/0
        assert_eq!(stats.total_tombstones_pending, 17); // 5 + 2 + 10
    }

    // Test 9: Update dirty ratio
    #[tokio::test]
    async fn test_update_dirty_ratio() {
        let scheduler = default_scheduler();

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.3, 0)
            .await
            .unwrap();

        // Initially below threshold
        assert!(!scheduler.should_compact("topic", 0).await.unwrap());

        // Update dirty ratio above threshold
        scheduler
            .update_dirty_ratio("topic", 0, 0.7)
            .await
            .unwrap();

        assert!(scheduler.should_compact("topic", 0).await.unwrap());
    }

    // Test 10: Remove partition from tracking
    #[tokio::test]
    async fn test_remove_partition() {
        let scheduler = default_scheduler();

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.5, 0)
            .await
            .unwrap();

        // Should exist
        assert!(scheduler.get_partition_state("topic", 0).await.is_ok());

        scheduler.remove_partition("topic", 0).await;

        // Should be gone
        assert!(scheduler.get_partition_state("topic", 0).await.is_err());
    }

    // Test 11: Expire tombstones independently
    #[tokio::test]
    async fn test_expire_tombstones_standalone() {
        let scheduler = default_scheduler();
        let now_ms = 1700000000000_i64;

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.5, 3)
            .await
            .unwrap();

        let records = make_records(&[
            (b"k1", None, 0, now_ms - 100_000_000), // very old tombstone
            (b"k2", None, 1, now_ms - 100_000_000), // very old tombstone
            (b"k3", None, 2, now_ms),                // recent tombstone
            (b"k4", Some(b"val"), 3, now_ms),        // not a tombstone
        ]);

        let expired = scheduler
            .expire_tombstones("topic", 0, &records, now_ms + 1000)
            .await
            .unwrap();

        assert_eq!(expired, 2); // k1 and k2 are old tombstones
    }

    // Test 12: should_compact respects minimum interval
    #[tokio::test]
    async fn test_should_compact_respects_interval() {
        let config = CompactionConfig {
            min_compaction_interval: Duration::from_secs(3600), // 1 hour
            min_dirty_ratio: 0.5,
            ..CompactionConfig::default()
        };
        let scheduler = CompactionScheduler::new(config);
        let now_ms = 1700000000000_i64;

        scheduler
            .schedule_compaction("topic".to_string(), 0, 0.8, 0)
            .await
            .unwrap();

        // First compaction should be allowed (no previous compaction)
        assert!(scheduler.should_compact("topic", 0).await.unwrap());

        // Run compaction
        let records = make_records(&[(b"k1", Some(b"v1"), 0, now_ms)]);
        scheduler
            .compact_partition("topic", 0, &records, now_ms)
            .await
            .unwrap();

        // Update dirty ratio back to high
        scheduler
            .update_dirty_ratio("topic", 0, 0.9)
            .await
            .unwrap();

        // Should not compact because minimum interval hasn't passed
        assert!(!scheduler.should_compact("topic", 0).await.unwrap());
    }
}
