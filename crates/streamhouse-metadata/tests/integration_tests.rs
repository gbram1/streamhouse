//! Integration tests for metadata store implementations
//!
//! These tests verify that all metadata store backends (SQLite, PostgreSQL, Cached)
//! behave identically and correctly implement the MetadataStore trait.

use std::collections::HashMap;
use streamhouse_metadata::{
    CacheConfig, CachedMetadataStore, MetadataStore, SegmentInfo, SqliteMetadataStore, TopicConfig,
};

#[cfg(feature = "postgres")]
use streamhouse_metadata::PostgresMetadataStore;

/// Helper to create a test topic configuration
fn create_test_topic_config(name: &str, partition_count: u32) -> TopicConfig {
    TopicConfig {
        name: name.to_string(),
        partition_count,
        retention_ms: Some(86400000), // 1 day
        config: HashMap::new(),
    }
}

/// Helper to create a test segment
fn create_test_segment(topic: &str, partition_id: u32, base_offset: u64) -> SegmentInfo {
    SegmentInfo {
        id: format!("{}-{}-{}", topic, partition_id, base_offset),
        topic: topic.to_string(),
        partition_id,
        base_offset,
        end_offset: base_offset + 999,
        record_count: 1000,
        size_bytes: 1024 * 1024, // 1 MB
        s3_bucket: "test-bucket".to_string(),
        s3_key: format!("{}/{}/seg_{}.bin", topic, partition_id, base_offset),
        created_at: chrono::Utc::now().timestamp_millis(),
    }
}

// ============================================================================
// SQLite Tests
// ============================================================================

#[tokio::test]
async fn test_sqlite_full_workflow() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_full_metadata_workflow(&store).await;
}

#[tokio::test]
async fn test_sqlite_concurrent_writes() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_concurrent_partition_updates(&store).await;
}

#[tokio::test]
async fn test_sqlite_consumer_groups() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_consumer_group_operations(&store).await;
}

#[tokio::test]
async fn test_sqlite_segment_cleanup() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_segment_retention_cleanup(&store).await;
}

// ============================================================================
// PostgreSQL Tests (if feature enabled)
// ============================================================================

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore] // Requires running PostgreSQL
async fn test_postgres_full_workflow() {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata".to_string()
    });

    let store = PostgresMetadataStore::new(&database_url).await.unwrap();

    // Clean up any existing test data
    let _ = store.delete_topic("integration_test").await;

    test_full_metadata_workflow(&store).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore] // Requires running PostgreSQL
async fn test_postgres_concurrent_writes() {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata".to_string()
    });

    let store = PostgresMetadataStore::new(&database_url).await.unwrap();
    let _ = store.delete_topic("concurrent_test").await;

    test_concurrent_partition_updates(&store).await;
}

// ============================================================================
// Cached Store Tests
// ============================================================================

#[tokio::test]
async fn test_cached_sqlite_full_workflow() {
    let inner = SqliteMetadataStore::new(":memory:").await.unwrap();
    let store = CachedMetadataStore::new(inner);
    test_full_metadata_workflow(&store).await;
}

#[tokio::test]
async fn test_cache_performance() {
    let inner = SqliteMetadataStore::new(":memory:").await.unwrap();
    let store = CachedMetadataStore::new(inner);

    // Create topic
    let config = create_test_topic_config("perf_test", 10);
    store.create_topic(config).await.unwrap();

    // Warm up cache
    let _ = store.get_topic("perf_test").await.unwrap();

    // Reset metrics
    store.metrics().reset();

    // Perform 100 reads - should all be cache hits
    for _ in 0..100 {
        let _ = store.get_topic("perf_test").await.unwrap();
    }

    // Verify high hit rate
    let hit_rate = store.metrics().topic_hit_rate();
    assert!(hit_rate > 0.99, "Expected >99% hit rate, got {}", hit_rate);
}

#[tokio::test]
async fn test_cache_invalidation_correctness() {
    let inner = SqliteMetadataStore::new(":memory:").await.unwrap();
    let store = CachedMetadataStore::new(inner);

    // Create topic and cache it
    let config = create_test_topic_config("cache_test", 5);
    store.create_topic(config).await.unwrap();
    let topic1 = store.get_topic("cache_test").await.unwrap().unwrap();

    // Update partition watermark
    store
        .update_high_watermark("cache_test", 0, 1000)
        .await
        .unwrap();

    // Read partition - should see updated watermark (not cached stale data)
    let partition = store.get_partition("cache_test", 0).await.unwrap().unwrap();
    assert_eq!(partition.high_watermark, 1000);

    // Delete and recreate topic
    store.delete_topic("cache_test").await.unwrap();
    let config2 = create_test_topic_config("cache_test", 10); // Different partition count
    store.create_topic(config2).await.unwrap();

    // Should see new topic with different partition count
    let topic2 = store.get_topic("cache_test").await.unwrap().unwrap();
    assert_eq!(topic2.partition_count, 10);
    assert_ne!(topic1.created_at, topic2.created_at);
}

#[tokio::test]
async fn test_cache_with_custom_config() {
    let inner = SqliteMetadataStore::new(":memory:").await.unwrap();

    // Very small cache for testing eviction
    let config = CacheConfig {
        topic_capacity: 2,
        topic_ttl_ms: 60000,
        partition_capacity: 5,
        partition_ttl_ms: 30000,
        topic_list_capacity: 1,
        topic_list_ttl_ms: 30000,
    };

    let store = CachedMetadataStore::with_config(inner, config);

    // Create 3 topics (exceeds capacity of 2)
    for i in 0..3 {
        let cfg = create_test_topic_config(&format!("topic_{}", i), 1);
        store.create_topic(cfg).await.unwrap();
    }

    // Access topic_0 and topic_1 (should be cached)
    let _ = store.get_topic("topic_0").await.unwrap();
    let _ = store.get_topic("topic_1").await.unwrap();

    // Reset metrics
    store.metrics().reset();

    // Access topic_2 - will evict topic_0 (LRU)
    let _ = store.get_topic("topic_2").await.unwrap();

    // Access topic_0 again - should be cache miss (was evicted)
    let _ = store.get_topic("topic_0").await.unwrap();

    // We should have at least 1 miss (topic_0 after eviction)
    assert!(
        store
            .metrics()
            .topic_misses
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1
    );
}

// ============================================================================
// Shared Test Implementations
// ============================================================================

/// Comprehensive workflow test that exercises all major operations
async fn test_full_metadata_workflow<S: MetadataStore>(store: &S) {
    let topic_name = "integration_test";

    // 1. Create topic
    let config = create_test_topic_config(topic_name, 3);
    store.create_topic(config.clone()).await.unwrap();

    // 2. Verify topic exists
    let topic = store.get_topic(topic_name).await.unwrap().unwrap();
    assert_eq!(topic.name, topic_name);
    assert_eq!(topic.partition_count, 3);
    assert_eq!(topic.retention_ms, Some(86400000));

    // 3. Verify partitions were created
    let partitions = store.list_partitions(topic_name).await.unwrap();
    assert_eq!(partitions.len(), 3);
    for (i, partition) in partitions.iter().enumerate() {
        assert_eq!(partition.partition_id, i as u32);
        assert_eq!(partition.topic, topic_name);
        assert_eq!(partition.high_watermark, 0);
    }

    // 4. Add segments
    for partition_id in 0..3 {
        let segment = create_test_segment(topic_name, partition_id, 0);
        store.add_segment(segment).await.unwrap();
    }

    // 5. Update high watermarks
    for partition_id in 0..3 {
        store
            .update_high_watermark(topic_name, partition_id, 1000)
            .await
            .unwrap();
    }

    // 6. Verify watermarks updated
    for partition_id in 0..3 {
        let partition = store
            .get_partition(topic_name, partition_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(partition.high_watermark, 1000);
    }

    // 7. Find segment by offset
    let segment = store
        .find_segment_for_offset(topic_name, 0, 500)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(segment.base_offset, 0);
    assert_eq!(segment.end_offset, 999);

    // 8. Consumer group operations
    store.ensure_consumer_group("test_group").await.unwrap();
    store
        .commit_offset(
            "test_group",
            topic_name,
            0,
            500,
            Some("metadata".to_string()),
        )
        .await
        .unwrap();

    let offset = store
        .get_committed_offset("test_group", topic_name, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offset, 500);

    // 9. List all offsets for consumer group
    let offsets = store.get_consumer_offsets("test_group").await.unwrap();
    assert_eq!(offsets.len(), 1);
    assert_eq!(offsets[0].committed_offset, 500);

    // 10. Delete consumer group
    store.delete_consumer_group("test_group").await.unwrap();
    let offsets = store.get_consumer_offsets("test_group").await.unwrap();
    assert_eq!(offsets.len(), 0);

    // 11. Clean up
    store.delete_topic(topic_name).await.unwrap();
    let topic = store.get_topic(topic_name).await.unwrap();
    assert!(topic.is_none());
}

/// Test concurrent updates to partition watermarks (simplified - sequential)
async fn test_concurrent_partition_updates<S: MetadataStore>(store: &S) {
    let topic_name = "concurrent_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 1);
    store.create_topic(config).await.unwrap();

    // Perform multiple watermark updates (sequential for test simplicity)
    // In production, these would come from different writers
    for i in 0..10 {
        store
            .update_high_watermark(topic_name, 0, (i + 1) * 100)
            .await
            .unwrap();
    }

    // Verify final watermark
    let partition = store.get_partition(topic_name, 0).await.unwrap().unwrap();
    assert_eq!(partition.high_watermark, 1000);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test consumer group operations thoroughly
async fn test_consumer_group_operations<S: MetadataStore>(store: &S) {
    let topic_name = "consumer_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 5);
    store.create_topic(config).await.unwrap();

    // Create multiple consumer groups
    for group_id in &["group_a", "group_b", "group_c"] {
        store.ensure_consumer_group(group_id).await.unwrap();

        // Commit offsets for multiple partitions
        for partition_id in 0..5 {
            store
                .commit_offset(
                    group_id,
                    topic_name,
                    partition_id,
                    partition_id as u64 * 100,
                    None,
                )
                .await
                .unwrap();
        }
    }

    // Verify each group has correct offsets
    for group_id in &["group_a", "group_b", "group_c"] {
        let offsets = store.get_consumer_offsets(group_id).await.unwrap();
        assert_eq!(offsets.len(), 5);

        for partition_id in 0..5 {
            let offset = store
                .get_committed_offset(group_id, topic_name, partition_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(offset, partition_id as u64 * 100);
        }
    }

    // Update offset (should replace existing)
    store
        .commit_offset("group_a", topic_name, 0, 999, Some("updated".to_string()))
        .await
        .unwrap();

    let offset = store
        .get_committed_offset("group_a", topic_name, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offset, 999);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test segment retention and cleanup
async fn test_segment_retention_cleanup<S: MetadataStore>(store: &S) {
    let topic_name = "retention_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 1);
    store.create_topic(config).await.unwrap();

    // Add multiple segments
    for i in 0..10 {
        let segment = create_test_segment(topic_name, 0, i * 1000);
        store.add_segment(segment).await.unwrap();
    }

    // Verify all segments exist
    let segments = store.get_segments(topic_name, 0).await.unwrap();
    assert_eq!(segments.len(), 10);

    // Delete segments before offset 5000
    let deleted = store
        .delete_segments_before(topic_name, 0, 5000)
        .await
        .unwrap();
    assert_eq!(deleted, 5); // Should delete segments 0-4999

    // Verify remaining segments
    let segments = store.get_segments(topic_name, 0).await.unwrap();
    assert_eq!(segments.len(), 5);
    assert!(segments.iter().all(|s| s.base_offset >= 5000));

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

// ============================================================================
// Cross-Backend Consistency Tests
// ============================================================================

#[tokio::test]
async fn test_sqlite_and_cached_consistency() {
    let sqlite_store = SqliteMetadataStore::new(":memory:").await.unwrap();
    let cached_store =
        CachedMetadataStore::new(SqliteMetadataStore::new(":memory:").await.unwrap());

    // Run same operations on both
    let config = create_test_topic_config("consistency_test", 3);

    sqlite_store.create_topic(config.clone()).await.unwrap();
    cached_store.create_topic(config.clone()).await.unwrap();

    // Verify both return same data
    let sqlite_topic = sqlite_store
        .get_topic("consistency_test")
        .await
        .unwrap()
        .unwrap();
    let cached_topic = cached_store
        .get_topic("consistency_test")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(sqlite_topic.name, cached_topic.name);
    assert_eq!(sqlite_topic.partition_count, cached_topic.partition_count);
    assert_eq!(sqlite_topic.retention_ms, cached_topic.retention_ms);
}

#[tokio::test]
async fn test_topic_list_ordering() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();

    // Create topics
    for i in 0..5 {
        let config = create_test_topic_config(&format!("topic_{}", i), 1);
        store.create_topic(config).await.unwrap();
    }

    let topics = store.list_topics().await.unwrap();
    assert_eq!(topics.len(), 5);

    // Verify all topics are present (ordering may vary by backend)
    let topic_names: Vec<_> = topics.iter().map(|t| t.name.as_str()).collect();
    for i in 0..5 {
        assert!(topic_names.contains(&format!("topic_{}", i).as_str()));
    }
}

#[tokio::test]
async fn test_partition_ordering() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();

    let config = create_test_topic_config("order_test", 10);
    store.create_topic(config).await.unwrap();

    let partitions = store.list_partitions("order_test").await.unwrap();
    assert_eq!(partitions.len(), 10);

    // Should be ordered by partition_id
    for (i, partition) in partitions.iter().enumerate() {
        assert_eq!(partition.partition_id, i as u32);
    }
}

#[tokio::test]
async fn test_segment_ordering() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();

    let config = create_test_topic_config("segment_order_test", 1);
    store.create_topic(config).await.unwrap();

    // Add segments in reverse order
    for i in (0..5).rev() {
        let segment = create_test_segment("segment_order_test", 0, i * 1000);
        store.add_segment(segment).await.unwrap();
    }

    let segments = store.get_segments("segment_order_test", 0).await.unwrap();
    assert_eq!(segments.len(), 5);

    // Should be ordered by base_offset
    for (i, segment) in segments.iter().enumerate() {
        assert_eq!(segment.base_offset, i as u64 * 1000);
    }
}
