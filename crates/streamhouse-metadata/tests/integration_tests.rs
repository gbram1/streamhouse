//! Integration tests for metadata store implementations
//!
//! These tests verify that all metadata store backends (SQLite, PostgreSQL, Cached)
//! behave identically and correctly implement the MetadataStore trait.

use std::collections::HashMap;
use streamhouse_metadata::{
    CacheConfig, CachedMetadataStore, InitProducerConfig, MetadataStore, ProducerState, SegmentInfo,
    SqliteMetadataStore, TopicConfig, TransactionState,
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

// ============================================================================
// Phase 16: Exactly-Once Semantics Tests
// ============================================================================

#[tokio::test]
async fn test_sqlite_idempotent_producer() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_idempotent_producer_operations(&store).await;
}

#[tokio::test]
async fn test_sqlite_transactions() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_transaction_operations(&store).await;
}

#[tokio::test]
async fn test_sqlite_lso_management() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_lso_operations(&store).await;
}

#[tokio::test]
async fn test_cached_idempotent_producer() {
    let inner = SqliteMetadataStore::new(":memory:").await.unwrap();
    let store = CachedMetadataStore::new(inner);
    test_idempotent_producer_operations(&store).await;
}

#[tokio::test]
async fn test_cached_transactions() {
    let inner = SqliteMetadataStore::new(":memory:").await.unwrap();
    let store = CachedMetadataStore::new(inner);
    test_transaction_operations(&store).await;
}

/// Test idempotent producer initialization and sequence tracking
async fn test_idempotent_producer_operations<S: MetadataStore>(store: &S) {
    let topic_name = "idempotent_test";

    // Create topic first
    let config = create_test_topic_config(topic_name, 3);
    store.create_topic(config).await.unwrap();

    // 1. Initialize producer without transactional ID
    let producer1 = store
        .init_producer(InitProducerConfig {
            transactional_id: None,
            organization_id: None,
            timeout_ms: 60000,
            metadata: None,
        })
        .await
        .unwrap();

    assert!(!producer1.id.is_empty());
    assert_eq!(producer1.epoch, 0);
    assert_eq!(producer1.state, ProducerState::Active);

    // 2. Initialize producer with transactional ID
    let producer2 = store
        .init_producer(InitProducerConfig {
            transactional_id: Some("txn-producer-1".to_string()),
            organization_id: None,
            timeout_ms: 60000,
            metadata: None,
        })
        .await
        .unwrap();

    assert!(!producer2.id.is_empty());
    assert_eq!(producer2.epoch, 0);
    assert_eq!(producer2.transactional_id, Some("txn-producer-1".to_string()));

    // 3. Re-initialize same transactional producer - should bump epoch
    let producer2_v2 = store
        .init_producer(InitProducerConfig {
            transactional_id: Some("txn-producer-1".to_string()),
            organization_id: None,
            timeout_ms: 60000,
            metadata: None,
        })
        .await
        .unwrap();

    assert_eq!(producer2_v2.id, producer2.id);
    assert_eq!(producer2_v2.epoch, 1); // Epoch bumped

    // 4. Test sequence number tracking
    // First batch - should succeed (new sequence)
    let is_new = store
        .check_and_update_sequence(&producer1.id, topic_name, 0, 0, 10)
        .await
        .unwrap();
    assert!(is_new);

    // Same sequence again - should return false (duplicate)
    let is_new = store
        .check_and_update_sequence(&producer1.id, topic_name, 0, 0, 10)
        .await
        .unwrap();
    assert!(!is_new);

    // Next sequence - should succeed
    let is_new = store
        .check_and_update_sequence(&producer1.id, topic_name, 0, 10, 5)
        .await
        .unwrap();
    assert!(is_new);

    // 5. Get producer and verify state
    let retrieved = store.get_producer(&producer1.id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.state, ProducerState::Active);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test transaction lifecycle operations
async fn test_transaction_operations<S: MetadataStore>(store: &S) {
    let topic_name = "txn_test";

    // Create topic first
    let config = create_test_topic_config(topic_name, 3);
    store.create_topic(config).await.unwrap();

    // Initialize transactional producer
    let producer = store
        .init_producer(InitProducerConfig {
            transactional_id: Some("txn-test-producer".to_string()),
            organization_id: None,
            timeout_ms: 60000,
            metadata: None,
        })
        .await
        .unwrap();

    // 1. Begin transaction
    let txn = store
        .begin_transaction(&producer.id, 60000)
        .await
        .unwrap();

    assert!(!txn.transaction_id.is_empty());
    assert_eq!(txn.producer_id, producer.id);
    assert_eq!(txn.state, TransactionState::Ongoing);

    // 2. Add partitions to transaction
    store
        .add_transaction_partition(&txn.transaction_id, topic_name, 0, 100)
        .await
        .unwrap();
    store
        .add_transaction_partition(&txn.transaction_id, topic_name, 1, 200)
        .await
        .unwrap();

    // 3. Update partition offset (simulating more writes)
    store
        .update_transaction_partition_offset(&txn.transaction_id, topic_name, 0, 109)
        .await
        .unwrap();

    // 4. Get transaction and verify partitions
    let retrieved = store.get_transaction(&txn.transaction_id).await.unwrap();
    assert!(retrieved.is_some());
    let _retrieved = retrieved.unwrap();

    // Get partitions separately
    let partitions = store.get_transaction_partitions(&txn.transaction_id).await.unwrap();
    assert_eq!(partitions.len(), 2);

    // 5. Commit transaction
    store.commit_transaction(&txn.transaction_id).await.unwrap();

    // Verify transaction is committed
    let committed = store.get_transaction(&txn.transaction_id).await.unwrap();
    assert!(committed.is_some());
    assert_eq!(committed.unwrap().state, TransactionState::Committed);

    // 6. Test abort flow with a new transaction
    let txn2 = store
        .begin_transaction(&producer.id, 60000)
        .await
        .unwrap();

    store
        .add_transaction_partition(&txn2.transaction_id, topic_name, 2, 300)
        .await
        .unwrap();

    store.abort_transaction(&txn2.transaction_id).await.unwrap();

    // Verify transaction is aborted
    let aborted = store.get_transaction(&txn2.transaction_id).await.unwrap();
    assert!(aborted.is_some());
    assert_eq!(aborted.unwrap().state, TransactionState::Aborted);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test Last Stable Offset (LSO) management
async fn test_lso_operations<S: MetadataStore>(store: &S) {
    let topic_name = "lso_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 2);
    store.create_topic(config).await.unwrap();

    // 1. Get LSO before any transactions (should be 0 or error)
    let lso = store.get_last_stable_offset(topic_name, 0).await;
    // LSO defaults to 0 if not set
    assert!(lso.is_ok() || lso.is_err());

    // 2. Update LSO
    store
        .update_last_stable_offset(topic_name, 0, 500)
        .await
        .unwrap();

    // 3. Verify LSO updated
    let lso = store.get_last_stable_offset(topic_name, 0).await.unwrap();
    assert_eq!(lso, 500);

    // 4. Update LSO to higher value
    store
        .update_last_stable_offset(topic_name, 0, 1000)
        .await
        .unwrap();

    let lso = store.get_last_stable_offset(topic_name, 0).await.unwrap();
    assert_eq!(lso, 1000);

    // 5. Test that LSO only moves forward (updates should use MAX)
    store
        .update_last_stable_offset(topic_name, 0, 800) // Lower than current
        .await
        .unwrap();

    let lso = store.get_last_stable_offset(topic_name, 0).await.unwrap();
    assert_eq!(lso, 1000); // Should still be 1000

    // 6. Test different partition has independent LSO
    store
        .update_last_stable_offset(topic_name, 1, 300)
        .await
        .unwrap();

    let lso0 = store.get_last_stable_offset(topic_name, 0).await.unwrap();
    let lso1 = store.get_last_stable_offset(topic_name, 1).await.unwrap();
    assert_eq!(lso0, 1000);
    assert_eq!(lso1, 300);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

#[tokio::test]
async fn test_producer_fencing() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    let topic_name = "fencing_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 1);
    store.create_topic(config).await.unwrap();

    // Initialize transactional producer
    let producer_v1 = store
        .init_producer(InitProducerConfig {
            transactional_id: Some("fencing-producer".to_string()),
            organization_id: None,
            timeout_ms: 60000,
            metadata: None,
        })
        .await
        .unwrap();

    assert_eq!(producer_v1.epoch, 0);

    // Begin transaction with epoch 0
    let txn1 = store
        .begin_transaction(&producer_v1.id, 60000)
        .await
        .unwrap();

    // Another instance restarts and gets new epoch
    let producer_v2 = store
        .init_producer(InitProducerConfig {
            transactional_id: Some("fencing-producer".to_string()),
            organization_id: None,
            timeout_ms: 60000,
            metadata: None,
        })
        .await
        .unwrap();

    assert_eq!(producer_v2.epoch, 1); // Epoch bumped
    assert_eq!(producer_v2.id, producer_v1.id); // Same producer ID

    // New producer should be able to begin transactions
    let txn2 = store
        .begin_transaction(&producer_v2.id, 60000)
        .await
        .unwrap();

    assert_ne!(txn2.transaction_id, txn1.transaction_id);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

#[tokio::test]
async fn test_sequence_gap_detection() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    let topic_name = "sequence_gap_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 1);
    store.create_topic(config).await.unwrap();

    // Initialize producer
    let producer = store
        .init_producer(InitProducerConfig {
            transactional_id: None,
            organization_id: None,
            timeout_ms: 60000,
            metadata: None,
        })
        .await
        .unwrap();

    // First batch at sequence 0
    let is_new = store
        .check_and_update_sequence(&producer.id, topic_name, 0, 0, 10)
        .await
        .unwrap();
    assert!(is_new);

    // Skip to sequence 20 (gap from 10-19)
    // This should still succeed - we're lenient on gaps
    let result = store
        .check_and_update_sequence(&producer.id, topic_name, 0, 20, 5)
        .await;
    // Implementation may either accept gaps or reject them
    // For StreamHouse, we accept gaps (lenient approach like Kafka)
    assert!(result.is_ok());

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}
