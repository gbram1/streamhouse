use std::sync::Arc;
use streamhouse_client::{Producer, ProducerConfig};
use streamhouse_metadata::SqliteMetadataStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup metadata store
    let metadata = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse-demo/metadata/metadata.db")
            .await?
    );

    // Create topic if it doesn't exist
    use streamhouse_metadata::{MetadataStore, TopicConfig};
    use std::collections::HashMap;

    if metadata.get_topic("demo_orders").await?.is_none() {
        metadata.create_topic(TopicConfig {
            name: "demo_orders".to_string(),
            partition_count: 2,
            retention_ms: None,
            config: HashMap::new(),
        }).await?;
        println!("✓ Created topic 'demo_orders' with 2 partitions");
    }

    // Create producer
    let mut producer = Producer::builder()
        .metadata_store(metadata)
        .agent_group("default")
        .batch_size(100)
        .batch_linger_ms(50)
        .compression_enabled(true)
        .build()
        .await?;

    println!("✓ Producer connected");
    println!("");

    // Send messages
    println!("Sending 50 orders...");
    for i in 0..50 {
        let key = format!("user_{}", i % 10);
        let value = format!(
            r#"{{"order_id": {}, "user_id": {}, "amount": {}, "timestamp": {}}}"#,
            i,
            i % 10,
            (i * 10) + 99,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
        );

        producer.send(
            "demo_orders",
            Some(key.as_bytes()),
            value.as_bytes(),
            None,
        ).await?;

        if (i + 1) % 10 == 0 {
            println!("  Sent {} orders", i + 1);
        }
    }

    // Flush to ensure all messages are sent
    producer.flush().await?;
    println!("✓ All messages sent and flushed");

    Ok(())
}
