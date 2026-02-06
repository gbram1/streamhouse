//! WAL Integration Tests
//!
//! These tests validate end-to-end crash recovery scenarios with real S3 uploads
//! and actual metadata store interactions.

use bytes::Bytes;
use object_store::memory::InMemory;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};
use streamhouse_storage::wal::{SyncPolicy, WALConfig};
use streamhouse_storage::writer::PartitionWriter;
use streamhouse_storage::WriteConfig;
use tempfile::TempDir;

/// Helper to get current time in milliseconds
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Helper to create test metadata store
async fn create_test_metadata() -> Arc<dyn MetadataStore> {
    let store = SqliteMetadataStore::new_in_memory().await.unwrap();

    // Create test topic
    store
        .create_topic(TopicConfig {
            name: "test-topic".to_string(),
            partition_count: 1,
            retention_ms: Some(86400000),
            config: Default::default(),
            cleanup_policy: Default::default(),
        })
        .await
        .unwrap();

    Arc::new(store)
}

/// Helper to create write config with WAL
fn create_write_config_with_wal(wal_dir: std::path::PathBuf) -> WriteConfig {
    WriteConfig {
        segment_max_size: 1024 * 1024, // 1MB
        segment_max_age_ms: 60000,
        s3_bucket: "test-bucket".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_endpoint: None,
        block_size_target: 1024,
        s3_upload_retries: 3,
        wal_config: Some(WALConfig {
            directory: wal_dir,
            sync_policy: SyncPolicy::Always, // Force sync for test reliability
            max_size_bytes: 1024 * 1024,
            batch_enabled: false, // Disable batching for crash recovery tests
            ..Default::default()
        }),
        throttle_config: None,
        ..Default::default()
    }
}

/// Proof test: WAL recovery restores offset continuity without any flush.
/// This isolates the recovery mechanism from the flush behavior.
#[tokio::test]
async fn test_wal_recovery_offset_continuity() {
    let temp_dir = TempDir::new().unwrap();
    let metadata = create_test_metadata().await;
    let object_store = Arc::new(InMemory::new());
    let write_config = create_write_config_with_wal(temp_dir.path().to_path_buf());

    // Write 50 records, then crash (drop without flush)
    {
        let mut writer = PartitionWriter::new(
            "test-topic".to_string(),
            0,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        for i in 0..50 {
            let offset = writer
                .append(
                    Some(Bytes::from(format!("key-{}", i))),
                    Bytes::from(format!("value-{}", i)),
                    now_ms() as u64,
                )
                .await
                .unwrap();
            assert_eq!(offset, i as u64);
        }
        // Crash: drop without flush
    }

    // Metadata still shows HW=0 (nothing flushed to S3)
    let partition = metadata
        .get_partition("test-topic", 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(partition.high_watermark, 0, "Nothing was flushed to S3");

    // Restart: WAL recovery should replay 50 records into the segment buffer
    {
        let mut writer = PartitionWriter::new(
            "test-topic".to_string(),
            0,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        // The next offset should be 50, proving recovery replayed all 50 records
        let offset = writer
            .append(None, Bytes::from("post-recovery"), now_ms() as u64)
            .await
            .unwrap();
        assert_eq!(
            offset, 50,
            "WAL recovery restored 50 records, next offset is 50"
        );
    }
}

#[tokio::test]
async fn test_crash_recovery_no_data_loss() {
    // This test simulates a crash before S3 flush and verifies all data is recovered

    let temp_dir = TempDir::new().unwrap();
    let metadata = create_test_metadata().await;
    let object_store = Arc::new(InMemory::new());
    let write_config = create_write_config_with_wal(temp_dir.path().to_path_buf());

    let topic = "test-topic";
    let partition_id = 0;

    // Step 1: Write 100 records
    let offsets = {
        let mut writer = PartitionWriter::new(
            topic.to_string(),
            partition_id,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        let mut offsets = Vec::new();
        for i in 0..100 {
            let offset = writer
                .append(
                    Some(Bytes::from(format!("key-{}", i))),
                    Bytes::from(format!("value-{}", i)),
                    now_ms() as u64,
                )
                .await
                .unwrap();
            offsets.push(offset);
        }

        // Simulate crash: drop writer WITHOUT calling flush()
        // In production, this would be an actual process kill
        drop(writer);
        offsets
    };

    // Step 2: Restart and verify recovery
    {
        let mut writer = PartitionWriter::new(
            topic.to_string(),
            partition_id,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        // Write one more record after recovery to verify offset continuity
        let offset = writer
            .append(None, Bytes::from("post-recovery"), now_ms() as u64)
            .await
            .unwrap();
        assert_eq!(offset, 100, "Next offset should be 100 (recovery worked!)");

        // Use flush_durable() to force S3 upload regardless of size/age thresholds
        // (flush() is a no-op for small segments that haven't reached thresholds)
        writer.flush_durable().await.unwrap();
    }

    // Step 3: Verify segment in metadata has 101 records (100 recovered + 1 new)
    let segments = metadata.get_segments(topic, partition_id).await.unwrap();
    assert_eq!(segments.len(), 1, "Should have 1 segment after flush");
    assert_eq!(
        segments[0].record_count, 101,
        "Segment should contain 101 records (100 recovered from WAL + 1 new)"
    );

    // Verify high watermark in metadata
    let partition = metadata
        .get_partition(topic, partition_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        partition.high_watermark, 101,
        "High watermark should be 101"
    );
}

#[tokio::test]
async fn test_wal_truncated_after_flush() {
    // This test verifies WAL is truncated after successful S3 upload

    let temp_dir = TempDir::new().unwrap();
    let metadata = create_test_metadata().await;
    let object_store = Arc::new(InMemory::new());
    let write_config = create_write_config_with_wal(temp_dir.path().to_path_buf());

    let mut writer = PartitionWriter::new(
        "test-topic".to_string(),
        0,
        object_store.clone(),
        metadata.clone(),
        write_config,
    )
    .await
    .unwrap();

    // Write 50 records
    for i in 0..50 {
        writer
            .append(
                Some(Bytes::from(format!("key-{}", i))),
                Bytes::from("value"),
                now_ms() as u64,
            )
            .await
            .unwrap();
    }

    // flush_durable() forces S3 upload and truncates WAL
    writer.flush_durable().await.unwrap();

    // Verify metadata shows 50 records
    let partition = metadata
        .get_partition("test-topic", 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        partition.high_watermark, 50,
        "Should have 50 records after flush"
    );

    // Drop writer and restart
    drop(writer);

    let write_config2 = create_write_config_with_wal(temp_dir.path().to_path_buf());
    let mut writer_after_flush = PartitionWriter::new(
        "test-topic".to_string(),
        0,
        object_store.clone(),
        metadata.clone(),
        write_config2,
    )
    .await
    .unwrap();

    // Write one more - if WAL was properly truncated, next offset should be 50
    let offset = writer_after_flush
        .append(None, Bytes::from("after-restart"), now_ms() as u64)
        .await
        .unwrap();
    assert_eq!(
        offset, 50,
        "WAL was truncated, offset continues from metadata"
    );

    writer_after_flush.flush_durable().await.unwrap();

    // Verify final state
    let partition = metadata
        .get_partition("test-topic", 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(partition.high_watermark, 51, "Should have 51 records total");
}

#[tokio::test]
async fn test_multiple_crash_recovery_cycles() {
    // This test simulates multiple crash-recovery cycles

    let temp_dir = TempDir::new().unwrap();
    let metadata = create_test_metadata().await;
    let object_store = Arc::new(InMemory::new());
    let write_config = create_write_config_with_wal(temp_dir.path().to_path_buf());

    // Cycle 1: Write 20, crash
    {
        let mut writer = PartitionWriter::new(
            "test-topic".to_string(),
            0,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        for i in 0..20 {
            writer
                .append(
                    Some(Bytes::from(format!("key-{}", i))),
                    Bytes::from("value"),
                    now_ms() as u64,
                )
                .await
                .unwrap();
        }
        // Crash (no flush)
    }

    // Cycle 2: Recover 20, write 1 more, flush, write 30 more, crash
    {
        let mut writer = PartitionWriter::new(
            "test-topic".to_string(),
            0,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        // Write one more and flush_durable to persist (20 recovered + 1 new = 21)
        writer
            .append(None, Bytes::from("test"), now_ms() as u64)
            .await
            .unwrap();
        writer.flush_durable().await.unwrap();

        let partition = metadata
            .get_partition("test-topic", 0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            partition.high_watermark, 21,
            "Should have 20 recovered + 1 new"
        );

        // Write 30 more records (offsets 21-50)
        for i in 0..30 {
            writer
                .append(
                    Some(Bytes::from(format!("key-{}", 20 + i))),
                    Bytes::from("value"),
                    now_ms() as u64,
                )
                .await
                .unwrap();
        }
        // Crash (no flush) — WAL has 30 unflushed records
    }

    // Cycle 3: Recover 30 from WAL, flush
    {
        let mut writer = PartitionWriter::new(
            "test-topic".to_string(),
            0,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        // Flush to persist all recovered records
        writer.flush_durable().await.unwrap();

        let partition = metadata
            .get_partition("test-topic", 0)
            .await
            .unwrap()
            .unwrap();
        // 21 from cycle 2 flush + 30 recovered from WAL = 51
        assert_eq!(
            partition.high_watermark, 51,
            "Should have all 51 records (20+1+30)"
        );
    }

    // Verify final state: 2 segments from 2 flush_durable calls
    let segments = metadata.get_segments("test-topic", 0).await.unwrap();
    assert_eq!(
        segments.len(),
        2,
        "Two segments: one from cycle 2, one from cycle 3"
    );
    let total_records: u32 = segments.iter().map(|s| s.record_count).sum();
    assert_eq!(
        total_records, 51,
        "Total: 20 recovered + 1 new + 30 recovered = 51"
    );
}

#[tokio::test]
async fn test_concurrent_partition_wals() {
    // This test verifies multiple partitions can have independent WALs

    let temp_dir = TempDir::new().unwrap();
    let metadata = Arc::new(SqliteMetadataStore::new_in_memory().await.unwrap());

    // Create topic with 3 partitions
    metadata
        .create_topic(TopicConfig {
            name: "test-topic".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            config: Default::default(),
            cleanup_policy: Default::default(),
        })
        .await
        .unwrap();

    let object_store = Arc::new(InMemory::new());
    let write_config = create_write_config_with_wal(temp_dir.path().to_path_buf());

    // Write to 3 partitions concurrently
    let mut handles = vec![];
    for partition_id in 0..3 {
        let metadata = metadata.clone();
        let object_store = object_store.clone();
        let write_config = write_config.clone();

        let handle = tokio::spawn(async move {
            let mut writer = PartitionWriter::new(
                "test-topic".to_string(),
                partition_id,
                object_store,
                metadata,
                write_config,
            )
            .await
            .unwrap();

            for i in 0..10 {
                writer
                    .append(
                        Some(Bytes::from(format!("p{}-key-{}", partition_id, i))),
                        Bytes::from("value"),
                        now_ms() as u64,
                    )
                    .await
                    .unwrap();
            }
            // Simulate crash (no flush)
        });

        handles.push(handle);
    }

    // Wait for all writes
    for handle in handles {
        handle.await.unwrap();
    }

    // Recover all partitions
    for partition_id in 0..3 {
        let mut writer = PartitionWriter::new(
            "test-topic".to_string(),
            partition_id,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await
        .unwrap();

        // Write one more and flush_durable to persist
        let offset = writer
            .append(None, Bytes::from("test"), now_ms() as u64)
            .await
            .unwrap();
        assert_eq!(
            offset, 10,
            "Partition {} recovered 10 records",
            partition_id
        );

        writer.flush_durable().await.unwrap();

        let partition = metadata
            .get_partition("test-topic", partition_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            partition.high_watermark, 11,
            "Partition {} should have 10 recovered + 1 new",
            partition_id
        );
    }
}

#[tokio::test]
async fn test_wal_disabled_mode() {
    // This test verifies system works correctly with WAL disabled

    let temp_dir = TempDir::new().unwrap();
    let metadata = create_test_metadata().await;
    let object_store = Arc::new(InMemory::new());

    let write_config_no_wal = WriteConfig {
        segment_max_size: 1024 * 1024,
        segment_max_age_ms: 60000,
        s3_bucket: "test-bucket".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_endpoint: None,
        block_size_target: 1024,
        s3_upload_retries: 3,
        wal_config: None, // WAL disabled
        throttle_config: None,
        ..Default::default()
    };

    let mut writer = PartitionWriter::new(
        "test-topic".to_string(),
        0,
        object_store.clone(),
        metadata.clone(),
        write_config_no_wal.clone(),
    )
    .await
    .unwrap();

    // Write records
    for i in 0..50 {
        writer
            .append(
                Some(Bytes::from(format!("key-{}", i))),
                Bytes::from("value"),
                now_ms() as u64,
            )
            .await
            .unwrap();
    }

    // Simulate crash (no flush)
    drop(writer);

    // Restart - should NOT recover (WAL disabled)
    let writer_after_crash = PartitionWriter::new(
        "test-topic".to_string(),
        0,
        object_store.clone(),
        metadata.clone(),
        write_config_no_wal,
    )
    .await
    .unwrap();

    // Offset should be 0 (no recovery without WAL)
    let partition = metadata
        .get_partition("test-topic", 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        partition.high_watermark, 0,
        "Without WAL, unflushed data is lost"
    );
}
