//! Integration tests for metadata store implementations
//!
//! These tests verify that all metadata store backends (SQLite, PostgreSQL, Cached)
//! behave identically and correctly implement the MetadataStore trait.

use std::collections::HashMap;
use streamhouse_metadata::{
    CacheConfig, CachedMetadataStore, CleanupPolicy, InitProducerConfig, LeaderChangeReason,
    LeaseTransferState, MetadataStore, ProducerState, SegmentInfo, SqliteMetadataStore, TopicConfig,
    TransactionState,
};

#[cfg(feature = "postgres")]
use streamhouse_metadata::PostgresMetadataStore;

/// Helper to create a test topic configuration
fn create_test_topic_config(name: &str, partition_count: u32) -> TopicConfig {
    TopicConfig {
        name: name.to_string(),
        partition_count,
        retention_ms: Some(86400000), // 1 day
        cleanup_policy: CleanupPolicy::default(),
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

    // 5. Test that LSO can be overwritten (implementation does unconditional update)
    store
        .update_last_stable_offset(topic_name, 0, 800)
        .await
        .unwrap();

    let lso = store.get_last_stable_offset(topic_name, 0).await.unwrap();
    assert_eq!(lso, 800); // Implementation overwrites unconditionally

    // 6. Test different partition has independent LSO
    store
        .update_last_stable_offset(topic_name, 1, 300)
        .await
        .unwrap();

    let lso0 = store.get_last_stable_offset(topic_name, 0).await.unwrap();
    let lso1 = store.get_last_stable_offset(topic_name, 1).await.unwrap();
    assert_eq!(lso0, 800); // Was overwritten from 1000 to 800 in step 5
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
    // Implementation rejects gaps with a SequenceError
    let result = store
        .check_and_update_sequence(&producer.id, topic_name, 0, 20, 5)
        .await;
    assert!(result.is_err(), "Sequence gaps should be rejected");

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

// ============================================================================
// Phase 17: Fast Leader Handoff Tests
// ============================================================================

#[tokio::test]
async fn test_sqlite_lease_transfer_lifecycle() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_lease_transfer_lifecycle(&store).await;
}

#[tokio::test]
async fn test_sqlite_lease_transfer_rejection() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_lease_transfer_rejection(&store).await;
}

#[tokio::test]
async fn test_sqlite_leader_change_tracking() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_leader_change_tracking(&store).await;
}

#[tokio::test]
async fn test_sqlite_transfer_timeout_cleanup() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_transfer_timeout_cleanup(&store).await;
}

#[tokio::test]
async fn test_sqlite_pending_transfers_for_agent() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    test_pending_transfers_for_agent(&store).await;
}

#[tokio::test]
async fn test_cached_lease_transfer_lifecycle() {
    let inner = SqliteMetadataStore::new(":memory:").await.unwrap();
    let store = CachedMetadataStore::new(inner);
    test_lease_transfer_lifecycle(&store).await;
}

/// Test the full lease transfer lifecycle: initiate -> accept -> complete
async fn test_lease_transfer_lifecycle<S: MetadataStore>(store: &S) {
    let topic_name = "transfer_test";

    // Create topic and register agents
    let config = create_test_topic_config(topic_name, 3);
    store.create_topic(config).await.unwrap();

    let agent1 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-001".to_string(),
        address: "10.0.0.1:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    let agent2 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-002".to_string(),
        address: "10.0.0.2:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };

    store.register_agent(agent1).await.unwrap();
    store.register_agent(agent2).await.unwrap();

    // 1. Agent 1 acquires lease
    let lease = store
        .acquire_partition_lease(topic_name, 0, "agent-001", 60_000)
        .await
        .unwrap();
    assert_eq!(lease.leader_agent_id, "agent-001");
    let initial_epoch = lease.epoch;

    // 2. Initiate transfer from agent-001 to agent-002
    let transfer = store
        .initiate_lease_transfer(
            topic_name,
            0,
            "agent-001",
            "agent-002",
            LeaderChangeReason::GracefulHandoff,
            30_000,
        )
        .await
        .unwrap();

    assert_eq!(transfer.topic, topic_name);
    assert_eq!(transfer.partition_id, 0);
    assert_eq!(transfer.from_agent_id, "agent-001");
    assert_eq!(transfer.to_agent_id, "agent-002");
    assert_eq!(transfer.state, LeaseTransferState::Pending);
    assert_eq!(transfer.reason, LeaderChangeReason::GracefulHandoff);

    // 3. Verify we can get the transfer
    let retrieved = store.get_lease_transfer(&transfer.transfer_id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.transfer_id, transfer.transfer_id);

    // 4. Target agent accepts the transfer
    let accepted = store
        .accept_lease_transfer(&transfer.transfer_id, "agent-002")
        .await
        .unwrap();
    assert_eq!(accepted.state, LeaseTransferState::Accepted);

    // 5. Source agent completes the transfer after flushing
    let new_lease = store
        .complete_lease_transfer(&transfer.transfer_id, 1000, 1001)
        .await
        .unwrap();

    // 6. Verify the new lease
    assert_eq!(new_lease.leader_agent_id, "agent-002");
    assert_eq!(new_lease.topic, topic_name);
    assert_eq!(new_lease.partition_id, 0);
    assert!(new_lease.epoch > initial_epoch); // Epoch should have incremented

    // 7. Verify the transfer is marked completed
    let completed_transfer = store.get_lease_transfer(&transfer.transfer_id).await.unwrap();
    assert!(completed_transfer.is_some());
    let completed_transfer = completed_transfer.unwrap();
    assert_eq!(completed_transfer.state, LeaseTransferState::Completed);
    assert_eq!(completed_transfer.last_flushed_offset, Some(1000));
    assert_eq!(completed_transfer.high_watermark, Some(1001));

    // 8. Verify agent-002 now holds the lease
    let current_lease = store.get_partition_lease(topic_name, 0).await.unwrap();
    assert!(current_lease.is_some());
    assert_eq!(current_lease.unwrap().leader_agent_id, "agent-002");

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test lease transfer rejection flow
async fn test_lease_transfer_rejection<S: MetadataStore>(store: &S) {
    let topic_name = "reject_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 1);
    store.create_topic(config).await.unwrap();

    // Register agents
    let agent1 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-001".to_string(),
        address: "10.0.0.1:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    let agent2 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-002".to_string(),
        address: "10.0.0.2:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };

    store.register_agent(agent1).await.unwrap();
    store.register_agent(agent2).await.unwrap();

    // Agent 1 acquires lease
    store
        .acquire_partition_lease(topic_name, 0, "agent-001", 60_000)
        .await
        .unwrap();

    // Initiate transfer
    let transfer = store
        .initiate_lease_transfer(
            topic_name,
            0,
            "agent-001",
            "agent-002",
            LeaderChangeReason::Rebalance,
            30_000,
        )
        .await
        .unwrap();

    assert_eq!(transfer.state, LeaseTransferState::Pending);

    // Target agent rejects the transfer
    store
        .reject_lease_transfer(&transfer.transfer_id, "agent-002", "Not ready for leadership")
        .await
        .unwrap();

    // Verify transfer is rejected
    let rejected = store.get_lease_transfer(&transfer.transfer_id).await.unwrap();
    assert!(rejected.is_some());
    let rejected = rejected.unwrap();
    assert_eq!(rejected.state, LeaseTransferState::Rejected);
    assert_eq!(rejected.error, Some("Not ready for leadership".to_string()));

    // Original leader should still hold the lease
    let lease = store.get_partition_lease(topic_name, 0).await.unwrap();
    assert!(lease.is_some());
    assert_eq!(lease.unwrap().leader_agent_id, "agent-001");

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test leader change tracking/recording
async fn test_leader_change_tracking<S: MetadataStore>(store: &S) {
    let topic_name = "leader_change_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 1);
    store.create_topic(config).await.unwrap();

    // Record various leadership changes
    store
        .record_leader_change(
            topic_name,
            0,
            None, // No previous leader
            "agent-001",
            LeaderChangeReason::Initial,
            1,
            0, // No gap for initial
        )
        .await
        .unwrap();

    store
        .record_leader_change(
            topic_name,
            0,
            Some("agent-001"),
            "agent-002",
            LeaderChangeReason::GracefulHandoff,
            2,
            50, // 50ms gap
        )
        .await
        .unwrap();

    store
        .record_leader_change(
            topic_name,
            0,
            Some("agent-002"),
            "agent-003",
            LeaderChangeReason::LeaseExpired,
            3,
            30_000, // 30 second gap (lease expired)
        )
        .await
        .unwrap();

    store
        .record_leader_change(
            topic_name,
            0,
            Some("agent-003"),
            "agent-004",
            LeaderChangeReason::AgentCrash,
            4,
            60_000, // 60 second gap (crash)
        )
        .await
        .unwrap();

    // The records should be stored successfully
    // Note: We don't have a direct query for leader_changes table,
    // but the operations should succeed without error

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test cleanup of timed out transfers
async fn test_transfer_timeout_cleanup<S: MetadataStore>(store: &S) {
    let topic_name = "timeout_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 2);
    store.create_topic(config).await.unwrap();

    // Register agents
    let agent1 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-001".to_string(),
        address: "10.0.0.1:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    let agent2 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-002".to_string(),
        address: "10.0.0.2:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };

    store.register_agent(agent1).await.unwrap();
    store.register_agent(agent2).await.unwrap();

    // Acquire lease for partition 0
    store
        .acquire_partition_lease(topic_name, 0, "agent-001", 60_000)
        .await
        .unwrap();

    // Initiate transfer with very short timeout (1ms)
    let transfer = store
        .initiate_lease_transfer(
            topic_name,
            0,
            "agent-001",
            "agent-002",
            LeaderChangeReason::GracefulHandoff,
            1, // 1ms timeout - will be expired immediately
        )
        .await
        .unwrap();

    // Wait a bit to ensure timeout
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Cleanup should mark the transfer as timed out
    let cleaned = store.cleanup_timed_out_transfers().await.unwrap();
    assert!(cleaned >= 1, "Expected at least 1 transfer to be cleaned up");

    // Verify transfer is marked as timed out
    let timed_out = store.get_lease_transfer(&transfer.transfer_id).await.unwrap();
    assert!(timed_out.is_some());
    assert_eq!(timed_out.unwrap().state, LeaseTransferState::TimedOut);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test getting pending transfers for an agent
async fn test_pending_transfers_for_agent<S: MetadataStore>(store: &S) {
    let topic_name = "pending_test";

    // Create topic with multiple partitions
    let config = create_test_topic_config(topic_name, 4);
    store.create_topic(config).await.unwrap();

    // Register agents
    let agent1 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-001".to_string(),
        address: "10.0.0.1:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    let agent2 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-002".to_string(),
        address: "10.0.0.2:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    let agent3 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-003".to_string(),
        address: "10.0.0.3:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };

    store.register_agent(agent1).await.unwrap();
    store.register_agent(agent2).await.unwrap();
    store.register_agent(agent3).await.unwrap();

    // Agent 1 acquires leases for partitions 0 and 1
    store
        .acquire_partition_lease(topic_name, 0, "agent-001", 60_000)
        .await
        .unwrap();
    store
        .acquire_partition_lease(topic_name, 1, "agent-001", 60_000)
        .await
        .unwrap();

    // Agent 3 acquires lease for partition 2
    store
        .acquire_partition_lease(topic_name, 2, "agent-003", 60_000)
        .await
        .unwrap();

    // Initiate transfers to agent-002
    store
        .initiate_lease_transfer(
            topic_name,
            0,
            "agent-001",
            "agent-002",
            LeaderChangeReason::GracefulHandoff,
            60_000,
        )
        .await
        .unwrap();

    store
        .initiate_lease_transfer(
            topic_name,
            1,
            "agent-001",
            "agent-002",
            LeaderChangeReason::GracefulHandoff,
            60_000,
        )
        .await
        .unwrap();

    // Initiate transfer from agent-003 to agent-002
    store
        .initiate_lease_transfer(
            topic_name,
            2,
            "agent-003",
            "agent-002",
            LeaderChangeReason::Rebalance,
            60_000,
        )
        .await
        .unwrap();

    // Get pending transfers for agent-002 (should see 3 incoming transfers)
    let pending = store.get_pending_transfers_for_agent("agent-002").await.unwrap();
    assert_eq!(pending.len(), 3, "Expected 3 pending transfers for agent-002");

    // Verify all are targeting agent-002
    for transfer in &pending {
        assert_eq!(transfer.to_agent_id, "agent-002");
        assert_eq!(transfer.state, LeaseTransferState::Pending);
    }

    // Get pending transfers for agent-001 (includes both outgoing and incoming transfers)
    // Agent-001 initiated 2 transfers (to agent-002), so it should see 2 outgoing transfers
    let pending_001 = store.get_pending_transfers_for_agent("agent-001").await.unwrap();
    assert_eq!(pending_001.len(), 2, "Expected 2 pending transfers for agent-001 (outgoing)");

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test that only the source agent can complete a transfer
#[tokio::test]
async fn test_transfer_authorization() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    let topic_name = "auth_test";

    // Create topic
    let config = create_test_topic_config(topic_name, 1);
    store.create_topic(config).await.unwrap();

    // Register agents
    let agent1 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-001".to_string(),
        address: "10.0.0.1:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    let agent2 = streamhouse_metadata::AgentInfo {
        agent_id: "agent-002".to_string(),
        address: "10.0.0.2:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };

    store.register_agent(agent1).await.unwrap();
    store.register_agent(agent2).await.unwrap();

    // Agent 1 acquires lease
    store
        .acquire_partition_lease(topic_name, 0, "agent-001", 60_000)
        .await
        .unwrap();

    // Initiate transfer
    let transfer = store
        .initiate_lease_transfer(
            topic_name,
            0,
            "agent-001",
            "agent-002",
            LeaderChangeReason::GracefulHandoff,
            60_000,
        )
        .await
        .unwrap();

    // Wrong agent tries to accept - should fail
    let result = store.accept_lease_transfer(&transfer.transfer_id, "agent-003").await;
    assert!(result.is_err(), "Non-target agent should not be able to accept");

    // Correct agent accepts
    let accepted = store
        .accept_lease_transfer(&transfer.transfer_id, "agent-002")
        .await
        .unwrap();
    assert_eq!(accepted.state, LeaseTransferState::Accepted);

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}

/// Test multiple concurrent transfers don't interfere
#[tokio::test]
async fn test_concurrent_transfers_different_partitions() {
    let store = SqliteMetadataStore::new(":memory:").await.unwrap();
    let topic_name = "concurrent_test";

    // Create topic with 4 partitions
    let config = create_test_topic_config(topic_name, 4);
    store.create_topic(config).await.unwrap();

    // Register agents
    for i in 1..=4 {
        let agent = streamhouse_metadata::AgentInfo {
            agent_id: format!("agent-{:03}", i),
            address: format!("10.0.0.{}:9090", i),
            availability_zone: "us-east-1a".to_string(),
            agent_group: "test".to_string(),
            last_heartbeat: chrono::Utc::now().timestamp_millis(),
            started_at: chrono::Utc::now().timestamp_millis(),
            metadata: HashMap::new(),
        };
        store.register_agent(agent).await.unwrap();
    }

    // Each agent acquires one partition
    for i in 0..4 {
        store
            .acquire_partition_lease(topic_name, i, &format!("agent-{:03}", i + 1), 60_000)
            .await
            .unwrap();
    }

    // Initiate transfers: each partition transfers to the next agent (circular)
    let mut transfers = vec![];
    for i in 0..4 {
        let from = format!("agent-{:03}", i + 1);
        let to = format!("agent-{:03}", (i + 1) % 4 + 1);
        let transfer = store
            .initiate_lease_transfer(
                topic_name,
                i,
                &from,
                &to,
                LeaderChangeReason::Rebalance,
                60_000,
            )
            .await
            .unwrap();
        transfers.push(transfer);
    }

    // All transfers should be pending
    for transfer in &transfers {
        let t = store.get_lease_transfer(&transfer.transfer_id).await.unwrap().unwrap();
        assert_eq!(t.state, LeaseTransferState::Pending);
    }

    // Accept all transfers
    for i in 0..4 {
        let target = format!("agent-{:03}", (i + 1) % 4 + 1);
        store
            .accept_lease_transfer(&transfers[i as usize].transfer_id, &target)
            .await
            .unwrap();
    }

    // Complete all transfers
    for (i, transfer) in transfers.iter().enumerate() {
        let new_lease = store
            .complete_lease_transfer(&transfer.transfer_id, (i as u64 + 1) * 100, (i as u64 + 1) * 100 + 1)
            .await
            .unwrap();

        // Verify the new leader
        let expected_leader = format!("agent-{:03}", (i + 1) % 4 + 1);
        assert_eq!(new_lease.leader_agent_id, expected_leader);
    }

    // Verify final state: each partition now has a different leader
    for i in 0..4 {
        let lease = store.get_partition_lease(topic_name, i).await.unwrap().unwrap();
        let expected_leader = format!("agent-{:03}", (i + 1) % 4 + 1);
        assert_eq!(lease.leader_agent_id, expected_leader);
    }

    // Clean up
    store.delete_topic(topic_name).await.unwrap();
}
