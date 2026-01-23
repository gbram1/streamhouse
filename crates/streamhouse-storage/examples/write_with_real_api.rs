//! Write real data through actual StreamHouse pipeline to PostgreSQL + MinIO
//!
//! This uses PostgreSQL if available, falls back to SQLite for metadata.
//! Segments always go to MinIO via real S3 API.

use bytes::Bytes;
use object_store::{aws::AmazonS3Builder, ObjectStore};
use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_storage::{PartitionReader, PartitionWriter, SegmentCache, WriteConfig};

#[cfg(feature = "postgres")]
use streamhouse_metadata::{MetadataStore, PostgresMetadataStore, TopicConfig};

#[cfg(not(feature = "postgres"))]
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🎯 Writing Real Data via StreamHouse APIs");
    println!("==========================================\n");

    // Connect to metadata store
    #[cfg(feature = "postgres")]
    let metadata: Arc<dyn MetadataStore> = {
        println!("📊 Connecting to PostgreSQL...");
        let database_url =
            "postgresql://streamhouse:streamhouse@localhost:5432/streamhouse_metadata";
        Arc::new(PostgresMetadataStore::new(database_url).await?)
    };

    #[cfg(not(feature = "postgres"))]
    let metadata: Arc<dyn MetadataStore> = {
        println!("📊 Using SQLite (same MetadataStore trait as PostgreSQL)...");
        Arc::new(SqliteMetadataStore::new("/tmp/streamhouse_pipeline.db").await?)
    };
    println!("   ✅ Connected\n");

    // Connect to MinIO
    println!("🪣 Connecting to MinIO...");
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
    println!("   ✅ Connected\n");

    let cache = Arc::new(SegmentCache::new(
        "/tmp/streamhouse-cache",
        1024 * 1024 * 100,
    )?);

    // Create topics
    println!("📝 Creating topics...");

    let topics = vec![
        ("orders", 3, 604800000),      // 7 days retention
        ("user-events", 2, 259200000), // 3 days retention
        ("metrics", 4, 86400000),      // 1 day retention
    ];

    for (name, partitions, retention) in &topics {
        metadata
            .create_topic(TopicConfig {
                name: name.to_string(),
                partition_count: *partitions,
                retention_ms: Some(*retention),
                config: HashMap::new(),
            })
            .await?;
        println!("   ✅ Created '{}' ({} partitions)", name, partitions);
    }
    println!();

    let write_config = WriteConfig {
        segment_max_size: 1024 * 100, // 100KB for demo
        segment_max_age_ms: 60000,
        s3_bucket: "streamhouse".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_endpoint: Some("http://localhost:9000".to_string()),
        block_size_target: 1024 * 10,
        s3_upload_retries: 3,
    };

    // Write to orders topic
    println!("✍️  Writing to 'orders' topic...");
    for partition in 0..3 {
        let mut writer = PartitionWriter::new(
            "orders".to_string(),
            partition,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await?;

        let count = match partition {
            0 => 100,
            1 => 150,
            _ => 80,
        };

        for i in 0..count {
            let key = format!("order-{}-{}", partition, i);
            let value = format!(
                r#"{{"order_id":{},"partition":{},"customer":"user-{}","amount":{}.99}}"#,
                i,
                partition,
                i % 20,
                50 + (i % 50)
            );
            writer
                .append(
                    Some(Bytes::from(key)),
                    Bytes::from(value),
                    current_timestamp(),
                )
                .await?;
        }

        writer.flush().await?;
        println!("   ✅ Partition {}: {} records written", partition, count);
    }
    println!();

    // Write to user-events topic
    println!("✍️  Writing to 'user-events' topic...");
    for partition in 0..2 {
        let mut writer = PartitionWriter::new(
            "user-events".to_string(),
            partition,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await?;

        let count = if partition == 0 { 200 } else { 120 };

        for i in 0..count {
            let key = format!("event-{}-{}", partition, i);
            let value = format!(
                r#"{{"event_type":"{}","user_id":{},"timestamp":{}}}"#,
                if i % 3 == 0 {
                    "click"
                } else if i % 3 == 1 {
                    "view"
                } else {
                    "purchase"
                },
                i % 50,
                current_timestamp()
            );
            writer
                .append(
                    Some(Bytes::from(key)),
                    Bytes::from(value),
                    current_timestamp(),
                )
                .await?;
        }

        writer.flush().await?;
        println!("   ✅ Partition {}: {} records written", partition, count);
    }
    println!();

    // Write to metrics topic
    println!("✍️  Writing to 'metrics' topic...");
    for partition in 0..4 {
        let mut writer = PartitionWriter::new(
            "metrics".to_string(),
            partition,
            object_store.clone(),
            metadata.clone(),
            write_config.clone(),
        )
        .await?;

        let count = 250;

        for i in 0..count {
            let value = format!(
                r#"{{"metric":"cpu_usage","host":"host-{}","value":{},"timestamp":{}}}"#,
                partition,
                20 + (i % 80),
                current_timestamp()
            );
            writer
                .append(None, Bytes::from(value), current_timestamp())
                .await?;
        }

        writer.flush().await?;
        println!("   ✅ Partition {}: {} records written", partition, count);
    }
    println!();

    // Verify by reading back
    println!("📖 Verifying reads with Phase 3.4 segment index...");
    let reader = PartitionReader::new(
        "orders".to_string(),
        0,
        metadata.clone(),
        object_store.clone(),
        cache.clone(),
    );

    let result = reader.read(0, 5).await?;
    println!(
        "   ✅ Read {} records from orders/partition-0",
        result.records.len()
    );
    for (i, record) in result.records.iter().take(3).enumerate() {
        let key = record
            .key
            .as_ref()
            .map(|k| String::from_utf8_lossy(k).to_string())
            .unwrap_or("null".to_string());
        let value = String::from_utf8_lossy(&record.value);
        println!("      [{}] {}: {}", i, key, value);
    }
    println!();

    println!("════════════════════════════════════════");
    println!("✅ Real Pipeline Complete!");
    println!("════════════════════════════════════════");
    println!();
    println!("Total records written: 1,650");
    println!("  • orders:      330 records (3 partitions)");
    println!("  • user-events: 320 records (2 partitions)");
    println!("  • metrics:     1,000 records (4 partitions)");
    println!();

    Ok(())
}
