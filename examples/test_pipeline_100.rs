use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_metadata::{MetadataStore, PostgresMetadataStore, TopicConfig};
use streamhouse_storage::{PartitionWriter, WriteConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 StreamHouse Pipeline Test - Sending 100 Messages");
    println!("================================================\n");

    // Connect to PostgreSQL
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata".to_string());

    println!("📊 Connecting to PostgreSQL...");
    let metadata = Arc::new(PostgresMetadataStore::new(&database_url).await?);
    println!("   ✓ Connected to metadata store\n");

    // Setup MinIO (S3)
    println!("🗄️  Configuring MinIO (S3)...");
    std::env::set_var("S3_ENDPOINT", "http://localhost:9000");
    std::env::set_var("AWS_ACCESS_KEY_ID", "minioadmin");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "minioadmin");
    std::env::set_var("AWS_REGION", "us-east-1");
    std::env::set_var("STREAMHOUSE_BUCKET", "streamhouse");

    let s3_builder = streamhouse_storage::build_s3_store(
        "http://localhost:9000",
        "streamhouse",
        Some("minioadmin".to_string()),
        Some("minioadmin".to_string()),
        "us-east-1",
    ).await?;
    let object_store = Arc::new(s3_builder);
    println!("   ✓ Connected to MinIO\n");

    // Get or create topic
    let topic_name = "test-topic";
    println!("📝 Checking topic '{}'...", topic_name);

    let topic = match metadata.get_topic(topic_name).await? {
        Some(t) => {
            println!("   ✓ Topic exists with {} partitions\n", t.partition_count);
            t
        },
        None => {
            return Err("Topic 'test-topic' does not exist. Create it first via REST API.".into());
        }
    };

    // Configure write settings with throttling enabled
    let write_config = WriteConfig {
        segment_size_bytes: 1024 * 1024, // 1MB segments for testing
        compression_codec: None,
        throttle_config: Some(streamhouse_storage::ThrottleConfig::default()),
        ..Default::default()
    };

    println!("📤 Sending 100 messages across {} partitions...\n", topic.partition_count);

    // Write messages to partitions (round-robin)
    let messages_per_partition = 100 / topic.partition_count as usize;
    let mut total_sent = 0;

    for partition_id in 0..topic.partition_count {
        println!("   Partition {}: Writing {} messages...", partition_id, messages_per_partition);

        let mut writer = PartitionWriter::new(
            topic_name.to_string(),
            partition_id,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        ).await?;

        for i in 0..messages_per_partition {
            let msg_id = partition_id as usize * messages_per_partition + i;
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64;

            let key = format!("key_{}", msg_id);
            let value = format!(
                r#"{{"message_id": {}, "partition": {}, "content": "Test message {}", "timestamp": {}}}"#,
                msg_id, partition_id, msg_id, timestamp
            );

            writer.append(
                Some(key.into()),
                value.into(),
                timestamp,
            ).await?;

            total_sent += 1;

            if (i + 1) % 10 == 0 {
                print!(".");
                std::io::Write::flush(&mut std::io::stdout())?;
            }
        }

        // Flush to MinIO
        writer.flush().await?;
        println!(" ✓ Flushed to MinIO");
    }

    println!("\n✅ Successfully sent {} messages!", total_sent);
    println!("\n📦 Verifying MinIO storage...");

    // Check MinIO to see if segments were created
    let bucket_url = format!("http://localhost:9001/browser/streamhouse/{}/", topic_name);
    println!("   View segments in MinIO Console: {}", bucket_url);
    println!("   Credentials: minioadmin / minioadmin");

    Ok(())
}
