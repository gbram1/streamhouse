//! Integration tests for the segment cache LRU system

use bytes::Bytes;
use streamhouse_storage::SegmentCache;
use tempfile::TempDir;

async fn create_cache(max_size: u64) -> (SegmentCache, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let cache = SegmentCache::new(temp_dir.path().join("cache"), max_size).unwrap();
    (cache, temp_dir)
}

#[tokio::test]
async fn test_cache_miss_returns_none() {
    let (cache, _dir) = create_cache(1024 * 1024).await;

    let result = cache.get("nonexistent-segment").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_put_then_get() {
    let (cache, _dir) = create_cache(1024 * 1024).await;

    let data = Bytes::from(b"hello segment data".to_vec());
    cache.put("orders-0-00000", data.clone()).await.unwrap();

    let result = cache.get("orders-0-00000").await.unwrap();
    assert_eq!(result.unwrap(), data);
}

#[tokio::test]
async fn test_cache_stats_accurate() {
    let (cache, _dir) = create_cache(10_000).await;

    // Empty cache
    let stats = cache.stats().await;
    assert_eq!(stats.current_size, 0);
    assert_eq!(stats.entry_count, 0);
    assert_eq!(stats.utilization_pct, 0.0);

    // After one put
    cache
        .put("seg-1", Bytes::from(vec![0u8; 1000]))
        .await
        .unwrap();
    let stats = cache.stats().await;
    assert_eq!(stats.current_size, 1000);
    assert_eq!(stats.entry_count, 1);
    assert!((stats.utilization_pct - 10.0).abs() < 0.01);

    // After second put
    cache
        .put("seg-2", Bytes::from(vec![0u8; 4000]))
        .await
        .unwrap();
    let stats = cache.stats().await;
    assert_eq!(stats.current_size, 5000);
    assert_eq!(stats.entry_count, 2);
    assert!((stats.utilization_pct - 50.0).abs() < 0.01);
}

#[tokio::test]
async fn test_lru_eviction_order() {
    // Cache holds max 500 bytes
    let (cache, _dir) = create_cache(500).await;

    // Put A (200 bytes), B (200 bytes)
    cache
        .put("seg-a", Bytes::from(vec![1u8; 200]))
        .await
        .unwrap();
    cache
        .put("seg-b", Bytes::from(vec![2u8; 200]))
        .await
        .unwrap();

    // Access A to make it recently used
    cache.get("seg-a").await.unwrap();

    // Put C (200 bytes) — should evict B (LRU), not A
    cache
        .put("seg-c", Bytes::from(vec![3u8; 200]))
        .await
        .unwrap();

    // B should be evicted
    assert!(cache.get("seg-b").await.unwrap().is_none());
    // A should still be present
    assert!(cache.get("seg-a").await.unwrap().is_some());
    // C should be present
    assert!(cache.get("seg-c").await.unwrap().is_some());
}

#[tokio::test]
async fn test_multiple_evictions() {
    // Cache holds max 300 bytes
    let (cache, _dir) = create_cache(300).await;

    // Fill cache
    cache
        .put("seg-1", Bytes::from(vec![0u8; 100]))
        .await
        .unwrap();
    cache
        .put("seg-2", Bytes::from(vec![0u8; 100]))
        .await
        .unwrap();
    cache
        .put("seg-3", Bytes::from(vec![0u8; 100]))
        .await
        .unwrap();

    // Put 250 bytes — should evict multiple segments
    cache
        .put("seg-big", Bytes::from(vec![0u8; 250]))
        .await
        .unwrap();

    let stats = cache.stats().await;
    // Only the big segment should remain
    assert_eq!(stats.entry_count, 1);
}

#[tokio::test]
async fn test_cache_persists_to_disk() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");

    let cache = SegmentCache::new(&cache_dir, 1024 * 1024).unwrap();
    cache
        .put("persistent-seg", Bytes::from(b"data on disk".to_vec()))
        .await
        .unwrap();

    // Verify file exists on disk
    let seg_path = cache_dir.join("persistent-seg.seg");
    assert!(seg_path.exists());
    let contents = tokio::fs::read(&seg_path).await.unwrap();
    assert_eq!(contents, b"data on disk");
}

#[tokio::test]
async fn test_cache_creation_creates_directory() {
    let temp_dir = TempDir::new().unwrap();
    let nested_cache_dir = temp_dir.path().join("deep").join("nested").join("cache");

    // Directory should not exist yet
    assert!(!nested_cache_dir.exists());

    let _cache = SegmentCache::new(&nested_cache_dir, 1024 * 1024).unwrap();

    // Directory should now be created
    assert!(nested_cache_dir.exists());
    assert!(nested_cache_dir.is_dir());
}

#[tokio::test]
async fn test_cache_put_overwrites_same_key() {
    let (cache, _dir) = create_cache(1024 * 1024).await;

    // Put initial data
    cache.put("seg-x", Bytes::from(b"version-1".to_vec())).await.unwrap();

    // Overwrite with new data
    cache.put("seg-x", Bytes::from(b"version-2".to_vec())).await.unwrap();

    // Should get the latest value
    let result = cache.get("seg-x").await.unwrap().unwrap();
    assert_eq!(result.as_ref(), b"version-2");
}

#[tokio::test]
async fn test_cache_stats_max_size_matches_config() {
    let max_size = 5_000_000u64;
    let (cache, _dir) = create_cache(max_size).await;

    let stats = cache.stats().await;
    assert_eq!(stats.max_size, max_size);
    assert_eq!(stats.current_size, 0);
    assert_eq!(stats.entry_count, 0);
}

#[tokio::test]
async fn test_cache_eviction_frees_disk_space() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    // Max cache: 150 bytes
    let cache = SegmentCache::new(&cache_dir, 150).unwrap();

    // Put segment that fills most of cache
    cache.put("seg-old", Bytes::from(vec![0u8; 100])).await.unwrap();

    // Verify file exists on disk
    let old_file = cache_dir.join("seg-old.seg");
    assert!(old_file.exists());

    // Put another segment that causes eviction
    cache.put("seg-new", Bytes::from(vec![0u8; 100])).await.unwrap();

    // Old file should have been removed from disk during eviction
    assert!(!old_file.exists());

    // New file should exist
    let new_file = cache_dir.join("seg-new.seg");
    assert!(new_file.exists());
}
