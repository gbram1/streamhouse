use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};
use streamhouse_storage::{PartitionWriter, WriteConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("StreamHouse Producer Demo (Direct Storage Write)");
    println!("");

    // Setup
    let metadata = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse-demo/metadata.db").await?
    );

    let object_store = Arc::new(
        object_store::local::LocalFileSystem::new_with_prefix("/tmp/streamhouse-demo/storage")?
    );

    // Create topic
    if metadata.get_topic("demo_orders").await?.is_none() {
        metadata.create_topic(TopicConfig {
            name: "demo_orders".to_string(),
            partition_count: 2,
            retention_ms: None,
            config: HashMap::new(),
        }).await?;
        println!("✓ Created topic 'demo_orders' with 2 partitions");
    }

    // Write to both partitions
    for partition_id in 0..2 {
        println!("Writing to partition {}...", partition_id);

        let mut writer = PartitionWriter::new(
            "demo_orders".to_string(),
            partition_id,
            object_store.clone(),
            metadata.clone(),
            WriteConfig::default(),
        ).await?;

        for i in 0..25 {
            let order_id = partition_id * 25 + i;
            let user_id = order_id % 10;
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64;

            let key = format!("user_{}", user_id);
            let value = format!(
                r#"{{"order_id": {}, "user_id": {}, "amount": {}, "timestamp": {}}}"#,
                order_id, user_id, (order_id * 10) + 99, timestamp
            );

            writer.append(
                Some(key.into()),
                value.into(),
                timestamp,
            ).await?;
        }

        writer.flush().await?;
        println!("  ✓ Wrote 25 records to partition {}", partition_id);
    }

    println!("");
    println!("✓ Producer finished - wrote 50 total records");
    Ok(())
}
