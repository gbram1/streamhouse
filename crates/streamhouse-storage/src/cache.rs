//! Segment Cache with LRU Eviction
//!
//! This module implements a disk-based LRU cache for segment files downloaded from S3.
//!
//! ## Why Caching?
//!
//! S3 has high latency (~50-200ms per GET request). Without caching:
//! - Every read requires an S3 download
//! - P99 latency would be 100-200ms
//! - Costs increase (S3 GET requests cost money)
//! - Network bandwidth wasted
//!
//! With caching:
//! - Cache hits: <1ms (local disk read)
//! - Sequential reads: >80% cache hit rate
//! - Reduced S3 costs
//! - Better user experience
//!
//! ## How It Works
//!
//! ```text
//! Consumer requests offset 50,000
//!         ↓
//! Find segment: orders-0-00000000000000000000
//!         ↓
//! Check cache: /tmp/cache/orders-0-00000000000000000000.seg
//!         ↓
//!     CACHE HIT? ────YES──→ Read from disk (<1ms)
//!         │
//!         NO
//!         ↓
//! Download from S3 (~50ms)
//!         ↓
//! Save to cache (for next time)
//!         ↓
//! Check if cache full?
//!         ↓
//!     YES → Evict LRU segment
//!         ↓
//! Return data to consumer
//! ```
//!
//! ## LRU Eviction
//!
//! When cache is full, we evict the **Least Recently Used** segment:
//! - Keeps frequently accessed segments in cache
//! - Automatically adapts to access patterns
//! - Sequential reads stay cached
//! - Old segments naturally fall out
//!
//! Example:
//! ```ignore
//! Cache (max 3 segments):
//! 1. Access segment A → Cache: [A]
//! 2. Access segment B → Cache: [A, B]
//! 3. Access segment C → Cache: [A, B, C]  (full!)
//! 4. Access segment D → Evict A (LRU) → Cache: [B, C, D]
//! 5. Access segment B → Move to front → Cache: [C, D, B]
//! 6. Access segment E → Evict C (LRU) → Cache: [D, B, E]
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::SegmentCache;
//!
//! // Create cache (1GB max, stored in /tmp/streamhouse-cache)
//! let cache = SegmentCache::new("/tmp/streamhouse-cache", 1024 * 1024 * 1024)?;
//!
//! // Try to get from cache
//! let cache_key = "orders-0-00000000000000000000";
//! match cache.get(cache_key).await? {
//!     Some(data) => {
//!         println!("Cache hit! {} bytes", data.len());
//!     }
//!     None => {
//!         // Download from S3
//!         let data = download_from_s3().await?;
//!
//!         // Save for next time
//!         cache.put(cache_key, data).await?;
//!     }
//! }
//! ```

use crate::error::Result;
use bytes::Bytes;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Disk-based LRU cache for segment files
pub struct SegmentCache {
    /// Directory where cached segments are stored
    cache_dir: PathBuf,

    /// Maximum total size of cache in bytes
    max_size_bytes: u64,

    /// Current total size of all cached segments
    current_size: Arc<Mutex<u64>>,

    /// LRU tracker: maps cache_key → size
    /// Automatically tracks access order
    lru: Arc<Mutex<LruCache<String, u64>>>,
}

impl SegmentCache {
    /// Create a new segment cache
    ///
    /// # Arguments
    /// * `cache_dir` - Directory to store cached segments
    /// * `max_size_bytes` - Maximum cache size (will evict LRU when full)
    ///
    /// # Example
    /// ```ignore
    /// let cache = SegmentCache::new("/tmp/cache", 1024 * 1024 * 1024)?; // 1GB
    /// ```
    pub fn new<P: AsRef<Path>>(cache_dir: P, max_size_bytes: u64) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();

        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&cache_dir)?;

        // LRU can track up to 10,000 segments (should be enough for most workloads)
        let capacity = NonZeroUsize::new(10000).unwrap();
        let lru = Arc::new(Mutex::new(LruCache::new(capacity)));

        Ok(Self {
            cache_dir,
            max_size_bytes,
            current_size: Arc::new(Mutex::new(0)),
            lru,
        })
    }

    /// Get segment from cache
    ///
    /// Returns `Some(data)` if cached, `None` if not found.
    /// Updates LRU order (moves to front).
    pub async fn get(&self, cache_key: &str) -> Result<Option<Bytes>> {
        let path = self.cache_path(cache_key);

        if !path.exists() {
            // Record cache miss (Phase 7.1e)
            streamhouse_observability::metrics::CACHE_MISSES_TOTAL.inc();
            return Ok(None);
        }

        // Read from disk
        let data = tokio::fs::read(&path).await?;

        // Update LRU (move to front - this segment was just accessed)
        {
            let mut lru = self.lru.lock().await;
            lru.get(cache_key);
        }

        // Record cache hit (Phase 7.1e)
        streamhouse_observability::metrics::CACHE_HITS_TOTAL.inc();

        tracing::debug!(
            cache_key = %cache_key,
            size = data.len(),
            "Cache hit"
        );

        Ok(Some(Bytes::from(data)))
    }

    /// Put segment in cache
    ///
    /// Evicts LRU segments if necessary to make room.
    /// Writes segment to disk and updates tracking.
    pub async fn put(&self, cache_key: &str, data: Bytes) -> Result<()> {
        let size = data.len() as u64;

        // Make room if needed (evict LRU segments)
        self.evict_if_needed(size).await?;

        // Write to disk
        let path = self.cache_path(cache_key);
        tokio::fs::write(&path, &data).await?;

        // Update size tracking
        {
            let mut current_size = self.current_size.lock().await;
            *current_size += size;

            // Record cache size metric (Phase 7.1e)
            streamhouse_observability::metrics::CACHE_SIZE_BYTES.set(*current_size as i64);
        }

        // Update LRU (add to front)
        {
            let mut lru = self.lru.lock().await;
            lru.put(cache_key.to_string(), size);
        }

        tracing::debug!(
            cache_key = %cache_key,
            size = data.len(),
            "Cached segment"
        );

        Ok(())
    }

    /// Evict LRU segments until we have enough space
    async fn evict_if_needed(&self, needed: u64) -> Result<()> {
        let mut current_size = self.current_size.lock().await;
        let mut lru = self.lru.lock().await;

        while *current_size + needed > self.max_size_bytes {
            // Pop least recently used segment
            if let Some((cache_key, size)) = lru.pop_lru() {
                // Delete from disk
                let path = self.cache_path(&cache_key);
                if let Err(e) = tokio::fs::remove_file(&path).await {
                    tracing::warn!(
                        cache_key = %cache_key,
                        error = %e,
                        "Failed to delete cached segment"
                    );
                }

                *current_size = current_size.saturating_sub(size);

                // Record cache size metric after eviction (Phase 7.1e)
                streamhouse_observability::metrics::CACHE_SIZE_BYTES.set(*current_size as i64);

                tracing::debug!(
                    cache_key = %cache_key,
                    size,
                    "Evicted from cache"
                );
            } else {
                // Cache is empty but we still need space
                // This means the segment is larger than max cache size
                tracing::warn!(
                    needed,
                    max_size = self.max_size_bytes,
                    "Cannot cache: segment larger than max cache size"
                );
                break;
            }
        }

        Ok(())
    }

    /// Get path for cached segment
    fn cache_path(&self, cache_key: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.seg", cache_key))
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let current_size = *self.current_size.lock().await;
        let entry_count = self.lru.lock().await.len();

        CacheStats {
            current_size,
            max_size: self.max_size_bytes,
            entry_count,
            utilization_pct: (current_size as f64 / self.max_size_bytes as f64 * 100.0),
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Current total size of cached segments
    pub current_size: u64,

    /// Maximum allowed cache size
    pub max_size: u64,

    /// Number of cached segments
    pub entry_count: usize,

    /// Cache utilization percentage (0-100)
    pub utilization_pct: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap();
        let stats = cache.stats().await;
        assert_eq!(stats.current_size, 0);
        assert_eq!(stats.entry_count, 0);
        assert_eq!(stats.max_size, 1024 * 1024);
        assert_eq!(stats.utilization_pct, 0.0);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap();
        let result = cache.get("nonexistent-key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 1024 * 1024).unwrap();

        let data = Bytes::from(vec![1u8, 2, 3, 4, 5]);
        cache.put("segment-1", data.clone()).await.unwrap();

        let retrieved = cache.get("segment-1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[tokio::test]
    async fn test_cache_stats_after_put() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 1024).unwrap();

        cache.put("seg-a", Bytes::from(vec![0u8; 100])).await.unwrap();
        cache.put("seg-b", Bytes::from(vec![0u8; 200])).await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.current_size, 300);
        assert_eq!(stats.entry_count, 2);
    }

    #[tokio::test]
    async fn test_cache_lru_eviction() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Max cache: 250 bytes
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 250).unwrap();

        // Put 100 bytes
        cache.put("seg-a", Bytes::from(vec![1u8; 100])).await.unwrap();
        // Put 100 bytes
        cache.put("seg-b", Bytes::from(vec![2u8; 100])).await.unwrap();
        // Put 100 bytes — should evict seg-a (LRU)
        cache.put("seg-c", Bytes::from(vec![3u8; 100])).await.unwrap();

        // seg-a should have been evicted
        let result = cache.get("seg-a").await.unwrap();
        assert!(result.is_none());

        // seg-b and seg-c should still be cached
        assert!(cache.get("seg-b").await.unwrap().is_some());
        assert!(cache.get("seg-c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_cache_path_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 1024).unwrap();
        let path = cache.cache_path("orders-0-00000000000000000000");
        assert!(path.to_str().unwrap().ends_with("orders-0-00000000000000000000.seg"));
    }

    #[tokio::test]
    async fn test_cache_utilization_percentage() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 1000).unwrap();

        cache.put("seg-a", Bytes::from(vec![0u8; 500])).await.unwrap();
        let stats = cache.stats().await;
        assert!((stats.utilization_pct - 50.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_cache_oversize_segment() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Max cache: 50 bytes
        let cache = SegmentCache::new(temp_dir.path().join("cache"), 50).unwrap();

        // Put 100 bytes — larger than max cache. Should not panic.
        cache.put("big-seg", Bytes::from(vec![0u8; 100])).await.unwrap();

        // Stats may be inconsistent but shouldn't panic
        let _stats = cache.stats().await;
    }
}
