//! End-to-End Integration Test
//!
//! Tests the complete StreamHouse stack with PostgreSQL + MinIO
//!
//! Run with:
//! ```bash
//! export DATABASE_URL=postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata
//! export AWS_ACCESS_KEY_ID=minioadmin
//! export AWS_SECRET_ACCESS_KEY=minioadmin
//! export AWS_REGION=us-east-1
//! export AWS_ENDPOINT=http://localhost:9000
//! cargo test --test e2e_test --features postgres -- --nocapture
//! ```

#[cfg(all(test, feature = "postgres"))]
mod e2e_tests {
    use bytes::Bytes;
    use object_store::{aws::AmazonS3Builder, ObjectStore};
    use std::collections::HashMap;
    use std::sync::Arc;
    use streamhouse_core::record::Record;
    use streamhouse_metadata::{
        CachedMetadataStore, MetadataStore, PostgresMetadataStore, TopicConfig,
    };
    use streamhouse_storage::{
        PartitionReader, PartitionWriter, SegmentCache, SegmentIndexConfig, WriteConfig,
    };

    fn current_timestamp() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL + MinIO running
    async fn test_complete_write_read_workflow() {
        // Setup PostgreSQL metadata store
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
                .to_string()
        });

        let postgres_store = PostgresMetadataStore::new(&database_url)
            .await
            .expect("Failed to connect to PostgreSQL");

        // Wrap with caching layer
        let metadata: Arc<dyn MetadataStore> = Arc::new(CachedMetadataStore::new(postgres_store));

        // Setup MinIO object store
        let object_store: Arc<dyn ObjectStore> = Arc::new(
            AmazonS3Builder::new()
                .with_bucket_name("streamhouse")
                .with_region("us-east-1")
                .with_endpoint("http://localhost:9000")
                .with_access_key_id("minioadmin")
                .with_secret_access_key("minioadmin")
                .with_allow_http(true)
                .build()
                .expect("Failed to create S3 client"),
        );

        // Setup segment cache
        let cache = Arc::new(
            SegmentCache::new("/tmp/streamhouse-cache", 1024 * 1024 * 100)
                .expect("Failed to create cache"),
        );

        let topic_name = "e2e_test_topic";
        let partition_id = 0u32;

        // Clean up any existing test data
        let _ = metadata.delete_topic(topic_name).await;

        // 1. Create topic
        println!("✓ Creating topic: {}", topic_name);
        metadata
            .create_topic(TopicConfig {
                name: topic_name.to_string(),
                partition_count: 3,
                retention_ms: Some(7 * 24 * 60 * 60 * 1000), // 7 days
                config: HashMap::new(),
            })
            .await
            .expect("Failed to create topic");

        // Verify topic exists
        let topic = metadata
            .get_topic(topic_name)
            .await
            .expect("Failed to get topic")
            .expect("Topic not found");
        assert_eq!(topic.name, topic_name);
        assert_eq!(topic.partition_count, 3);
        println!("✓ Topic created successfully");

        // 2. Write records using PartitionWriter
        println!("✓ Writing 1000 records to partition {}", partition_id);
        let write_config = WriteConfig::default();
        let mut writer = PartitionWriter::new(
            topic_name.to_string(),
            partition_id,
            metadata.clone(),
            object_store.clone(),
            write_config,
        );

        for i in 0..1000 {
            let record = Record::new(
                i,
                current_timestamp(),
                Some(Bytes::from(format!("key-{}", i))),
                Bytes::from(format!("value-{}", i)),
            );
            writer
                .append(record)
                .await
                .expect("Failed to append record");
        }

        // Flush to S3
        writer.flush().await.expect("Failed to flush");
        println!("✓ Records written and flushed to S3");

        // Verify segment was registered in metadata
        let partition = metadata
            .get_partition(topic_name, partition_id)
            .await
            .expect("Failed to get partition")
            .expect("Partition not found");
        assert_eq!(partition.high_watermark, 1000);
        println!("✓ Partition watermark: {}", partition.high_watermark);

        // 3. Read records using PartitionReader
        println!("✓ Reading records from partition {}", partition_id);
        let reader = PartitionReader::with_index_config(
            topic_name.to_string(),
            partition_id,
            metadata.clone(),
            object_store.clone(),
            cache.clone(),
            SegmentIndexConfig::default(),
        );

        // Read first 100 records
        let result = reader
            .read(0, 100)
            .await
            .expect("Failed to read records");
        assert_eq!(result.records.len(), 100);
        assert_eq!(result.high_watermark, 1000);
        println!("✓ Read {} records", result.records.len());

        // Verify record content
        let first_record = &result.records[0];
        assert_eq!(first_record.offset, 0);
        assert_eq!(first_record.key, Some(Bytes::from("key-0")));
        assert_eq!(first_record.value, Bytes::from("value-0"));
        println!("✓ Record content verified");

        // Read from middle of partition
        let result = reader
            .read(500, 100)
            .await
            .expect("Failed to read from offset 500");
        assert_eq!(result.records.len(), 100);
        assert_eq!(result.records[0].offset, 500);
        println!("✓ Read from offset 500 successful");

        // 4. Test segment index performance
        println!("✓ Testing segment index performance");
        let start = std::time::Instant::now();
        for i in (0..1000).step_by(100) {
            let _ = reader.read(i, 10).await.expect("Failed to read");
        }
        let duration = start.elapsed();
        println!(
            "✓ 10 reads across partition took {:?} (~{:?} per read)",
            duration,
            duration / 10
        );

        // 5. Test consumer offset tracking
        println!("✓ Testing consumer offset tracking");
        let group_id = "test_consumer_group";
        metadata
            .ensure_consumer_group(group_id)
            .await
            .expect("Failed to create consumer group");

        metadata
            .commit_offset(group_id, topic_name, partition_id, 500, None)
            .await
            .expect("Failed to commit offset");

        let committed = metadata
            .get_committed_offset(group_id, topic_name, partition_id)
            .await
            .expect("Failed to get committed offset")
            .expect("No committed offset found");
        assert_eq!(committed, 500);
        println!("✓ Consumer offset committed: {}", committed);

        // 6. Test cache metrics
        let cache_stats = cache.stats().await;
        println!("✓ Cache stats: {:?}", cache_stats);

        // 7. Clean up
        println!("✓ Cleaning up test data");
        metadata
            .delete_consumer_group(group_id)
            .await
            .expect("Failed to delete consumer group");
        metadata
            .delete_topic(topic_name)
            .await
            .expect("Failed to delete topic");

        println!("\n🎉 End-to-end test passed!");
        println!("   - PostgreSQL metadata: ✓");
        println!("   - MinIO storage: ✓");
        println!("   - Segment caching: ✓");
        println!("   - Segment indexing: ✓");
        println!("   - Consumer tracking: ✓");
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL + MinIO running
    async fn test_phase_3_4_segment_index() {
        println!("🧪 Testing Phase 3.4 Segment Index with real backend");

        // Setup
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
                .to_string()
        });

        let postgres_store = PostgresMetadataStore::new(&database_url)
            .await
            .expect("Failed to connect to PostgreSQL");

        let metadata: Arc<dyn MetadataStore> = Arc::new(CachedMetadataStore::new(postgres_store));

        let object_store: Arc<dyn ObjectStore> = Arc::new(
            AmazonS3Builder::new()
                .with_bucket_name("streamhouse")
                .with_region("us-east-1")
                .with_endpoint("http://localhost:9000")
                .with_access_key_id("minioadmin")
                .with_secret_access_key("minioadmin")
                .with_allow_http(true)
                .build()
                .expect("Failed to create S3 client"),
        );

        let cache = Arc::new(
            SegmentCache::new("/tmp/streamhouse-cache", 1024 * 1024 * 100)
                .expect("Failed to create cache"),
        );

        let topic_name = "index_test";
        let partition_id = 0u32;

        // Clean up
        let _ = metadata.delete_topic(topic_name).await;

        // Create topic
        metadata
            .create_topic(TopicConfig {
                name: topic_name.to_string(),
                partition_count: 1,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .expect("Failed to create topic");

        // Write multiple small segments
        let write_config = WriteConfig {
            max_segment_size: 1024 * 10, // 10KB segments
            ..Default::default()
        };

        let mut writer = PartitionWriter::new(
            topic_name.to_string(),
            partition_id,
            metadata.clone(),
            object_store.clone(),
            write_config,
        );

        // Write 5000 records (will create multiple segments)
        println!("✓ Writing 5000 records (multiple segments)");
        for i in 0..5000 {
            let record = Record::new(
                i,
                current_timestamp(),
                Some(Bytes::from(format!("k{}", i))),
                Bytes::from(vec![0u8; 100]), // 100 byte value
            );
            writer.append(record).await.expect("Failed to append");
        }
        writer.flush().await.expect("Failed to flush");

        // Create reader with segment index
        let reader = PartitionReader::with_index_config(
            topic_name.to_string(),
            partition_id,
            metadata.clone(),
            object_store.clone(),
            cache.clone(),
            SegmentIndexConfig::default(),
        );

        // First read - will populate index
        println!("✓ First read (populates index)");
        let start = std::time::Instant::now();
        let _ = reader.read(0, 10).await.expect("Failed to read");
        let first_read = start.elapsed();
        println!("   First read took: {:?}", first_read);

        // Subsequent reads - should use index (< 1µs lookup)
        println!("✓ Subsequent reads (uses index)");
        let start = std::time::Instant::now();
        for offset in (0..5000).step_by(500) {
            let _ = reader.read(offset, 10).await.expect("Failed to read");
        }
        let indexed_reads = start.elapsed();
        let per_read = indexed_reads / 10;
        println!("   10 indexed reads took: {:?}", indexed_reads);
        println!("   Average per read: {:?}", per_read);

        // Verify segment count
        let segment_count = reader.segment_index.segment_count().await;
        println!("✓ Segments in index: {}", segment_count);
        assert!(segment_count >= 5, "Should have at least 5 segments");

        // Clean up
        metadata
            .delete_topic(topic_name)
            .await
            .expect("Failed to delete topic");

        println!("\n🎉 Phase 3.4 segment index test passed!");
        println!("   Index lookup performance: {:?} per read", per_read);
    }
}
