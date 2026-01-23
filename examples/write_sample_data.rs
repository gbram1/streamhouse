//! Write Sample Data to StreamHouse
//!
//! This example writes sample data to demonstrate how data is stored in PostgreSQL and MinIO.
//!
//! Run with:
//! ```bash
//! export DATABASE_URL=postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata
//! cargo run --example write_sample_data --features postgres
//! ```

use bytes::Bytes;
use object_store::{aws::AmazonS3Builder, ObjectStore};
use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_core::record::Record;
use streamhouse_metadata::{MetadataStore, PostgresMetadataStore, TopicConfig};
use streamhouse_storage::{PartitionWriter, WriteConfig};

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Writing sample data to StreamHouse\n");

    // 1. Connect to PostgreSQL
    println!("📊 Connecting to PostgreSQL...");
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata".to_string()
    });
    let metadata: Arc<dyn MetadataStore> = Arc::new(
        PostgresMetadataStore::new(&database_url)
            .await
            .expect("Failed to connect to PostgreSQL"),
    );
    println!("✅ Connected to PostgreSQL\n");

    // 2. Connect to MinIO
    println!("🪣 Connecting to MinIO...");
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
    println!("✅ Connected to MinIO\n");

    // 3. Create topic
    let topic_name = "demo-orders";
    println!("📝 Creating topic: {}", topic_name);

    // Clean up if exists
    let _ = metadata.delete_topic(topic_name).await;

    metadata
        .create_topic(TopicConfig {
            name: topic_name.to_string(),
            partition_count: 3,
            retention_ms: Some(7 * 24 * 60 * 60 * 1000), // 7 days
            config: {
                let mut config = HashMap::new();
                config.insert("compression".to_string(), "lz4".to_string());
                config.insert("created_by".to_string(), "example".to_string());
                config
            },
        })
        .await?;
    println!("✅ Topic created: {} (3 partitions)\n", topic_name);

    // 4. Write records to partition 0
    println!("✍️  Writing records to partition 0...");
    let write_config = WriteConfig {
        max_segment_size: 1024 * 64, // 64KB segments (small for demo)
        ..Default::default()
    };

    let mut writer = PartitionWriter::new(
        topic_name.to_string(),
        0,
        metadata.clone(),
        object_store.clone(),
        write_config,
    );

    // Write sample orders
    let orders = vec![
        ("order-1", r#"{"item":"laptop","price":1299.99,"qty":1}"#),
        ("order-2", r#"{"item":"mouse","price":29.99,"qty":2}"#),
        ("order-3", r#"{"item":"keyboard","price":89.99,"qty":1}"#),
        ("order-4", r#"{"item":"monitor","price":399.99,"qty":2}"#),
        ("order-5", r#"{"item":"headphones","price":149.99,"qty":1}"#),
    ];

    for (i, (key, value)) in orders.iter().enumerate() {
        let record = Record::new(
            i as u64,
            current_timestamp(),
            Some(Bytes::from(key.to_string())),
            Bytes::from(value.to_string()),
        );
        writer.append(record).await?;
        println!("  ✓ Record {}: {} = {}", i, key, value);
    }

    // Flush to S3
    println!("\n💾 Flushing to S3...");
    writer.flush().await?;
    println!("✅ Records flushed to MinIO\n");

    // 5. Write more records to partition 1
    println!("✍️  Writing records to partition 1...");
    let mut writer = PartitionWriter::new(
        topic_name.to_string(),
        1,
        metadata.clone(),
        object_store.clone(),
        write_config,
    );

    for i in 0..3 {
        let record = Record::new(
            i as u64,
            current_timestamp(),
            Some(Bytes::from(format!("user-{}", i + 1))),
            Bytes::from(format!(r#"{{"action":"login","user_id":{}}}"#, i + 1)),
        );
        writer.append(record).await?;
        println!("  ✓ Record {}: user-{}", i, i + 1);
    }

    writer.flush().await?;
    println!("✅ Records flushed to MinIO\n");

    // 6. Commit consumer offset
    println!("👥 Creating consumer group and committing offset...");
    metadata.ensure_consumer_group("demo-consumer").await?;
    metadata
        .commit_offset("demo-consumer", topic_name, 0, 3, Some("processed 3 orders".to_string()))
        .await?;
    println!("✅ Consumer offset committed\n");

    // 7. Display summary
    println!("✨ Sample data written successfully!\n");
    println!("Summary:");
    println!("  📊 PostgreSQL:");
    println!("     - 1 topic (demo-orders)");
    println!("     - 3 partitions");
    println!("     - 2 segments (partition 0 & 1)");
    println!("     - 1 consumer group (demo-consumer)");
    println!();
    println!("  🪣 MinIO:");
    println!("     - Segment files stored in: streamhouse/demo-orders/");
    println!("     - Partition 0: 5 records");
    println!("     - Partition 1: 3 records");
    println!();
    println!("Run './scripts/inspect_data.sh' to view the data!");

    Ok(())
}
