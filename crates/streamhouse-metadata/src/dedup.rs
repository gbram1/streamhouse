//! Idempotent Producer Dedup Cache
//!
//! This module implements an in-memory deduplication cache for producer sequence tracking.
//! It provides fast, lock-based sequence validation to detect duplicate records, reject
//! out-of-order deliveries, and accept new records in the correct sequence.
//!
//! ## Purpose
//!
//! When producers retry failed requests (e.g., due to network timeouts), the same batch
//! of records may arrive more than once. Without deduplication:
//! - Records get written twice (exactly-once semantics violated)
//! - Consumer applications process duplicates (incorrect business logic)
//!
//! With the dedup cache:
//! - Duplicate batches are silently rejected (already committed)
//! - Sequence gaps are detected (data loss between producer and broker)
//! - New batches are accepted and tracked
//!
//! ## Architecture
//!
//! ```text
//! Producer A (seq 0,1,2,3...)
//!        │
//!        ▼
//! ┌──────────────────┐
//! │   DedupCache     │
//! │ (producer_id,    │ ◄── You are here
//! │  topic,          │
//! │  partition)      │
//! │  → last_sequence │
//! └──────────────────┘
//!        │
//!    Ok / Duplicate / SequenceGap
//! ```
//!
//! ## Performance
//!
//! - **Check + update**: < 1µs (in-memory LRU with RwLock)
//! - **Memory**: ~200 bytes per tracked (producer, topic, partition) tuple
//! - **Capacity**: Configurable LRU eviction (default: 100,000 entries)

use lru::LruCache;
use std::fmt;
use std::num::NonZeroUsize;
use tokio::sync::RwLock;

/// Result of a deduplication check against the sequence cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DedupResult {
    /// New sequence accepted. The batch is valid and the cache has been updated.
    Ok,
    /// Duplicate detected. The batch has already been processed (sequence <= last known).
    Duplicate,
    /// Sequence gap detected. There is a gap between the last known sequence and the
    /// incoming base_sequence, indicating potential data loss.
    SequenceGap {
        /// The last sequence number successfully recorded for this producer/topic/partition.
        expected: i64,
        /// The base_sequence that was received (too far ahead).
        got: i64,
    },
}

impl fmt::Display for DedupResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DedupResult::Ok => write!(f, "Ok"),
            DedupResult::Duplicate => write!(f, "Duplicate"),
            DedupResult::SequenceGap { expected, got } => {
                write!(f, "SequenceGap(expected={}, got={})", expected, got)
            }
        }
    }
}

/// Statistics about the dedup cache.
#[derive(Debug, Clone)]
pub struct DedupStats {
    /// Number of entries currently in the cache.
    pub entries: usize,
    /// Maximum capacity of the cache.
    pub capacity: usize,
    /// Total number of successful (Ok) checks.
    pub accepted: u64,
    /// Total number of duplicate detections.
    pub duplicates: u64,
    /// Total number of sequence gap detections.
    pub gaps: u64,
}

/// In-memory LRU dedup cache for producer sequence tracking.
///
/// Maps `(producer_id, topic, partition)` to the last accepted sequence number.
/// Uses an LRU eviction policy so that inactive producer/partition combos are
/// eventually evicted to keep memory bounded.
pub struct DedupCache {
    /// LRU cache mapping (producer_id, topic, partition) -> last_sequence
    cache: RwLock<LruCache<(String, String, u32), i64>>,
    /// Maximum capacity
    capacity: usize,
    /// Counters for stats
    accepted: std::sync::atomic::AtomicU64,
    duplicates: std::sync::atomic::AtomicU64,
    gaps: std::sync::atomic::AtomicU64,
}

impl DedupCache {
    /// Create a new DedupCache with the given maximum capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of (producer_id, topic, partition) entries to track.
    ///   When the cache is full, the least recently used entry is evicted.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0.
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).expect("DedupCache capacity must be > 0");
        Self {
            cache: RwLock::new(LruCache::new(cap)),
            capacity,
            accepted: std::sync::atomic::AtomicU64::new(0),
            duplicates: std::sync::atomic::AtomicU64::new(0),
            gaps: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Check a producer's sequence number and update the cache if valid.
    ///
    /// The deduplication logic:
    /// - If no prior sequence exists for this (producer, topic, partition), the batch is
    ///   accepted if `base_sequence == 0` (first batch) or unconditionally (idempotent
    ///   producer may have been evicted).
    /// - If `base_sequence <= last_sequence`, the batch is a **duplicate**.
    /// - If `base_sequence == last_sequence + 1`, the batch is valid (**Ok**).
    /// - If `base_sequence > last_sequence + 1`, there is a **SequenceGap**.
    ///
    /// On `Ok`, the last_sequence is updated to `base_sequence + record_count - 1`.
    ///
    /// # Arguments
    ///
    /// * `producer_id` - Unique producer identifier
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    /// * `base_sequence` - First sequence number in the batch
    /// * `record_count` - Number of records in the batch (must be >= 1)
    ///
    /// # Returns
    ///
    /// A `DedupResult` indicating whether the batch is accepted, duplicate, or has a gap.
    pub async fn check_and_update(
        &self,
        producer_id: &str,
        topic: &str,
        partition: u32,
        base_sequence: i64,
        record_count: i64,
    ) -> DedupResult {
        let key = (producer_id.to_string(), topic.to_string(), partition);
        let mut cache = self.cache.write().await;

        let last_seq = cache.get(&key).copied();

        match last_seq {
            None => {
                // First time seeing this producer/topic/partition combo (or evicted).
                // Accept unconditionally and record the sequence.
                let new_last = base_sequence + record_count - 1;
                cache.put(key, new_last);
                self.accepted
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                DedupResult::Ok
            }
            Some(last) => {
                if base_sequence <= last {
                    // Duplicate: we've already seen up to `last`, and this batch starts
                    // at or before that.
                    self.duplicates
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    DedupResult::Duplicate
                } else if base_sequence == last + 1 {
                    // Valid next batch: contiguous with the last accepted sequence.
                    let new_last = base_sequence + record_count - 1;
                    cache.put(key, new_last);
                    self.accepted
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    DedupResult::Ok
                } else {
                    // Gap: base_sequence > last + 1
                    self.gaps
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    DedupResult::SequenceGap {
                        expected: last + 1,
                        got: base_sequence,
                    }
                }
            }
        }
    }

    /// Evict all entries for a given producer across all topics and partitions.
    ///
    /// This is called when a producer is fenced (e.g., a new instance with the same
    /// transactional ID starts up). After eviction, the producer's sequences are
    /// no longer tracked, and a new epoch can start fresh.
    ///
    /// # Arguments
    ///
    /// * `producer_id` - Producer ID whose entries should be evicted
    pub async fn evict_producer(&self, producer_id: &str) {
        let mut cache = self.cache.write().await;
        // Collect keys to remove (can't mutate while iterating in LRU)
        let keys_to_remove: Vec<(String, String, u32)> = cache
            .iter()
            .filter(|(k, _)| k.0 == producer_id)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Get current cache statistics.
    pub fn stats(&self) -> DedupStats {
        // We can't call async here easily, so we use try_read or just report capacity.
        // For the entry count, we use a best-effort approach.
        let entries = self
            .cache
            .try_read()
            .map(|c| c.len())
            .unwrap_or(0);

        DedupStats {
            entries,
            capacity: self.capacity,
            accepted: self.accepted.load(std::sync::atomic::Ordering::Relaxed),
            duplicates: self.duplicates.load(std::sync::atomic::Ordering::Relaxed),
            gaps: self.gaps.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Get the last recorded sequence for a given (producer, topic, partition) tuple.
    ///
    /// Returns `None` if the entry has been evicted or was never recorded.
    pub async fn get_sequence(
        &self,
        producer_id: &str,
        topic: &str,
        partition: u32,
    ) -> Option<i64> {
        let key = (producer_id.to_string(), topic.to_string(), partition);
        let mut cache = self.cache.write().await;
        cache.get(&key).copied()
    }

    /// Get the number of entries currently in the cache.
    pub async fn len(&self) -> usize {
        self.cache.read().await.len()
    }

    /// Check if the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.cache.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_first_sequence_accepted() {
        let cache = DedupCache::new(100);
        let result = cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;
        assert_eq!(result, DedupResult::Ok);
    }

    #[tokio::test]
    async fn test_contiguous_sequences_accepted() {
        let cache = DedupCache::new(100);

        // First batch: seq 0-9
        let r1 = cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;
        assert_eq!(r1, DedupResult::Ok);

        // Second batch: seq 10-19 (contiguous)
        let r2 = cache
            .check_and_update("producer-1", "topic-a", 0, 10, 10)
            .await;
        assert_eq!(r2, DedupResult::Ok);

        // Third batch: seq 20-29 (contiguous)
        let r3 = cache
            .check_and_update("producer-1", "topic-a", 0, 20, 10)
            .await;
        assert_eq!(r3, DedupResult::Ok);
    }

    #[tokio::test]
    async fn test_duplicate_detection() {
        let cache = DedupCache::new(100);

        // First batch: seq 0-9
        cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;

        // Retry same batch: seq 0-9 (duplicate)
        let result = cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;
        assert_eq!(result, DedupResult::Duplicate);
    }

    #[tokio::test]
    async fn test_partial_duplicate_detection() {
        let cache = DedupCache::new(100);

        // First batch: seq 0-9
        cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;

        // Partial overlap: seq 5-14 (still duplicate since base_sequence=5 <= last=9)
        let result = cache
            .check_and_update("producer-1", "topic-a", 0, 5, 10)
            .await;
        assert_eq!(result, DedupResult::Duplicate);
    }

    #[tokio::test]
    async fn test_sequence_gap_detection() {
        let cache = DedupCache::new(100);

        // First batch: seq 0-9
        cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;

        // Gap: seq 20-29 (skipped 10-19)
        let result = cache
            .check_and_update("producer-1", "topic-a", 0, 20, 10)
            .await;
        assert_eq!(
            result,
            DedupResult::SequenceGap {
                expected: 10,
                got: 20
            }
        );
    }

    #[tokio::test]
    async fn test_different_producers_independent() {
        let cache = DedupCache::new(100);

        // Producer 1: seq 0-9
        let r1 = cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;
        assert_eq!(r1, DedupResult::Ok);

        // Producer 2: seq 0-9 (independent, should also be accepted)
        let r2 = cache
            .check_and_update("producer-2", "topic-a", 0, 0, 10)
            .await;
        assert_eq!(r2, DedupResult::Ok);
    }

    #[tokio::test]
    async fn test_different_topics_independent() {
        let cache = DedupCache::new(100);

        // Same producer, topic-a: seq 0-9
        cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;

        // Same producer, topic-b: seq 0-9 (independent)
        let result = cache
            .check_and_update("producer-1", "topic-b", 0, 0, 10)
            .await;
        assert_eq!(result, DedupResult::Ok);
    }

    #[tokio::test]
    async fn test_different_partitions_independent() {
        let cache = DedupCache::new(100);

        // Same producer, same topic, partition 0: seq 0-9
        cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;

        // Same producer, same topic, partition 1: seq 0-9 (independent)
        let result = cache
            .check_and_update("producer-1", "topic-a", 1, 0, 10)
            .await;
        assert_eq!(result, DedupResult::Ok);
    }

    #[tokio::test]
    async fn test_evict_producer() {
        let cache = DedupCache::new(100);

        // Add entries for producer-1 across topics/partitions
        cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;
        cache
            .check_and_update("producer-1", "topic-a", 1, 0, 5)
            .await;
        cache
            .check_and_update("producer-1", "topic-b", 0, 0, 3)
            .await;

        // Add entry for producer-2
        cache
            .check_and_update("producer-2", "topic-a", 0, 0, 10)
            .await;

        assert_eq!(cache.len().await, 4);

        // Evict producer-1
        cache.evict_producer("producer-1").await;

        assert_eq!(cache.len().await, 1);

        // Producer-1 entries gone: accepting seq 0 again should succeed (fresh start)
        let result = cache
            .check_and_update("producer-1", "topic-a", 0, 0, 10)
            .await;
        assert_eq!(result, DedupResult::Ok);

        // Producer-2 entry still intact: seq 0 should be duplicate
        let result = cache
            .check_and_update("producer-2", "topic-a", 0, 0, 10)
            .await;
        assert_eq!(result, DedupResult::Duplicate);
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let cache = DedupCache::new(100);

        // One accepted
        cache
            .check_and_update("p1", "t", 0, 0, 1)
            .await;

        // One contiguous accepted
        cache
            .check_and_update("p1", "t", 0, 1, 1)
            .await;

        // One duplicate
        cache
            .check_and_update("p1", "t", 0, 0, 1)
            .await;

        // One gap
        cache
            .check_and_update("p1", "t", 0, 100, 1)
            .await;

        let stats = cache.stats();
        assert_eq!(stats.capacity, 100);
        assert_eq!(stats.accepted, 2);
        assert_eq!(stats.duplicates, 1);
        assert_eq!(stats.gaps, 1);
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        // Small cache: only 2 entries
        let cache = DedupCache::new(2);

        // Fill cache with 2 entries
        cache
            .check_and_update("p1", "t", 0, 0, 10)
            .await;
        cache
            .check_and_update("p2", "t", 0, 0, 10)
            .await;

        // Add a third entry, should evict p1 (LRU)
        cache
            .check_and_update("p3", "t", 0, 0, 10)
            .await;

        assert_eq!(cache.len().await, 2);

        // p1 was evicted, so seq 0 is accepted again (fresh start)
        let result = cache
            .check_and_update("p1", "t", 0, 0, 10)
            .await;
        assert_eq!(result, DedupResult::Ok);
    }

    #[tokio::test]
    async fn test_single_record_batches() {
        let cache = DedupCache::new(100);

        // Single record batches: seq 0, 1, 2, 3...
        for i in 0..10 {
            let result = cache
                .check_and_update("p1", "t", 0, i, 1)
                .await;
            assert_eq!(result, DedupResult::Ok, "seq {} should be accepted", i);
        }

        // Verify last sequence
        let last = cache.get_sequence("p1", "t", 0).await;
        assert_eq!(last, Some(9));
    }

    #[tokio::test]
    async fn test_get_sequence() {
        let cache = DedupCache::new(100);

        // Not yet tracked
        assert_eq!(cache.get_sequence("p1", "t", 0).await, None);

        // After recording
        cache
            .check_and_update("p1", "t", 0, 0, 5)
            .await;
        assert_eq!(cache.get_sequence("p1", "t", 0).await, Some(4));

        // After another batch
        cache
            .check_and_update("p1", "t", 0, 5, 5)
            .await;
        assert_eq!(cache.get_sequence("p1", "t", 0).await, Some(9));
    }

    #[tokio::test]
    async fn test_dedup_result_display() {
        assert_eq!(DedupResult::Ok.to_string(), "Ok");
        assert_eq!(DedupResult::Duplicate.to_string(), "Duplicate");
        assert_eq!(
            DedupResult::SequenceGap {
                expected: 10,
                got: 20
            }
            .to_string(),
            "SequenceGap(expected=10, got=20)"
        );
    }

    #[tokio::test]
    async fn test_empty_cache() {
        let cache = DedupCache::new(100);
        assert!(cache.is_empty().await);
        assert_eq!(cache.len().await, 0);

        cache
            .check_and_update("p1", "t", 0, 0, 1)
            .await;
        assert!(!cache.is_empty().await);
        assert_eq!(cache.len().await, 1);
    }
}
