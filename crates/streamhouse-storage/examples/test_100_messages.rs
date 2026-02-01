//! Test Pipeline: Send 100 messages and verify flush to MinIO
//!
//! This test demonstrates the full write path with our running server's infrastructure:
//! 1. Connect to PostgreSQL metadata store
//! 2. Connect to MinIO (S3)
//! 3. Write 100 messages through PartitionWriter
//! 4. Flush to MinIO and verify
//!
//! Run with:
//! ```bash
//! cargo run -p streamhouse-storage --example test_100_messages --features postgres
//! ```

use bytes::Bytes;
use object_store::{aws::AmazonS3Builder, ObjectStore};
use std::sync::Arc;
use streamhouse_metadata::{MetadataStore, PostgresMetadataStore};
use streamhouse_storage::{PartitionWriter, WriteConfig};

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🚀 StreamHouse Pipeline Test - 100 Messages");
    println!("=============================================");
    println!();

    // Connect to PostgreSQL
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata".to_string());

    println!("📊 Step 1: Connect to PostgreSQL metadata store");
    let metadata: Arc<dyn MetadataStore> = Arc::new(
        PostgresMetadataStore::new(&database_url).await?
    );
    println!("   ✅ Connected to PostgreSQL");
    println!();

    // Connect to MinIO
    println!("🪣 Step 2: Connect to MinIO (S3)");
    let object_store: Arc<dyn ObjectStore> = Arc::new(
        AmazonS3Builder::new()
            .with_bucket_name("streamhouse")
            .with_region("us-east-1")
            .with_endpoint("http://localhost:9000")
            .with_access_key_id("minioadmin")
            .with_secret_access_key("minioadmin")
            .with_allow_http(true)
            .build()?,
    );
    println!("   ✅ Connected to MinIO at localhost:9000");
    println!();

    // Get topic
    let topic = "test-topic";
    println!("📝 Step 3: Check topic '{}'", topic);

    let topic_info = metadata.get_topic(topic).await?
        .ok_or("Topic 'test-topic' does not exist")?;

    println!("   ✅ Topic exists with {} partitions", topic_info.partition_count);
    println!();

    // Configure writes with throttling
    let write_config = WriteConfig {
        segment_max_size: 1024 * 50, // 50KB for quick flush
        segment_max_age_ms: 60000,
        s3_bucket: "streamhouse".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_endpoint: Some("http://localhost:9000".to_string()),
        block_size_target: 1024 * 10,
        s3_upload_retries: 3,
        wal_config: None,
    };

    println!("📤 Step 4: Writing 100 messages across {} partitions", topic_info.partition_count);
    println!();

    let messages_per_partition = 100 / topic_info.partition_count as usize;
    let mut total_sent = 0;

    for partition_id in 0..topic_info.partition_count {
        println!("   Partition {}: Writing {} messages...", partition_id, messages_per_partition);

        let mut writer = PartitionWriter::new(
            topic.to_string(),
            partition_id,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        ).await?;

        for i in 0..messages_per_partition {
            let msg_id = partition_id as usize * messages_per_partition + i;
            let timestamp = current_timestamp();

            let key = format!("key_{}", msg_id);
            let value = format!(
                r#"{{"message_id": {}, "partition": {}, "content": "Test message {}", "timestamp": {}}}"#,
                msg_id, partition_id, msg_id, timestamp
            );

            writer.append(
                Some(Bytes::from(key)),
                Bytes::from(value),
                timestamp,
            ).await?;

            total_sent += 1;

            if (i + 1) % 10 == 0 {
                print!(".");
                use std::io::Write;
                std::io::stdout().flush()?;
            }
        }

        println!();
        println!("   💾 Flushing to MinIO...");
        writer.flush().await?;
        println!("   ✅ Flushed {} messages to MinIO", messages_per_partition);
        println!();
    }

    println!("✅ Step 5: Verify data in metadata store");
    for partition_id in 0..topic_info.partition_count {
        let partition = metadata.get_partition(topic, partition_id).await?.unwrap();
        let segments = metadata.get_segments(topic, partition_id).await?;

        println!("   Partition {} watermark: {} ({} segments)",
            partition_id, partition.high_watermark, segments.len());

        for seg in &segments {
            println!("     • offsets {}-{}, {} records, {} bytes",
                seg.base_offset, seg.end_offset, seg.record_count, seg.size_bytes);
            println!("       s3://{}/{}", seg.s3_bucket, seg.s3_key);
        }
    }
    println!();

    println!("════════════════════════════════════════");
    println!("✅ Pipeline Test Complete!");
    println!("════════════════════════════════════════");
    println!();
    println!("Summary:");
    println!("  ✅ Sent {} messages total", total_sent);
    println!("  ✅ Flushed to MinIO successfully");
    println!("  ✅ Metadata updated in PostgreSQL");
    println!();
    println!("View in MinIO Console:");
    println!("  • URL: http://localhost:9001");
    println!("  • Credentials: minioadmin / minioadmin");
    println!("  • Browse to: streamhouse/{}/", topic);
    println!();
    println!("Check metrics:");
    println!("  curl http://localhost:8080/metrics | grep streamhouse");
    println!();

    Ok(())
}
