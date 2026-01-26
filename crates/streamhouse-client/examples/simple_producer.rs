//! Simple Producer Example
//!
//! This example demonstrates how to use the StreamHouse Producer API
//! to send records to topics.
//!
//! Run with:
//! ```bash
//! cargo run --package streamhouse-client --example simple_producer
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_client::Producer;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n🎯 StreamHouse Producer Example");
    println!("================================\n");

    // Step 1: Setup metadata store
    println!("📊 Step 1: Setting up metadata store (SQLite)");
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("producer_example.db");

    let metadata: Arc<dyn MetadataStore> =
        Arc::new(SqliteMetadataStore::new(db_path.to_str().unwrap()).await?);
    println!("   ✅ Metadata store initialized\n");

    // Step 2: Create topics
    println!("📦 Step 2: Creating topics");

    metadata
        .create_topic(TopicConfig {
            name: "orders".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000), // 1 day
            config: HashMap::new(),
        })
        .await?;
    println!("   ✅ Created topic 'orders' with 3 partitions");

    metadata
        .create_topic(TopicConfig {
            name: "user-events".to_string(),
            partition_count: 6,
            retention_ms: Some(86400000),
            config: HashMap::new(),
        })
        .await?;
    println!("   ✅ Created topic 'user-events' with 6 partitions\n");

    // Step 3: Create producer
    println!("🚀 Step 3: Creating producer");
    let producer = Producer::builder()
        .metadata_store(Arc::clone(&metadata))
        .agent_group("default")
        .compression_enabled(true)
        .build()
        .await?;
    println!("   ✅ Producer initialized\n");

    // Step 4: Send records
    println!("✍️  Step 4: Sending records\n");

    // Example 1: Send with key-based partitioning
    println!("Example 1: Key-based partitioning");
    for i in 0..10 {
        let user_id = format!("user{}", i % 3); // 3 different users
        let order_data = format!("{{\"order_id\": {}, \"amount\": {}}}", i, i * 100);

        let result = producer
            .send(
                "orders",
                Some(user_id.as_bytes()),
                order_data.as_bytes(),
                None,
            )
            .await?;

        println!(
            "  Sent order {} → partition {}, offset {}",
            i,
            result.partition,
            result.offset.unwrap_or(0)
        );
    }
    println!();

    // Example 2: Send to explicit partition
    println!("Example 2: Explicit partition selection");
    for partition in 0..3 {
        let event_data = format!("{{\"event\": \"login\", \"partition\": {}}}", partition);

        let result = producer
            .send("user-events", None, event_data.as_bytes(), Some(partition))
            .await?;

        println!(
            "  Sent event → partition {} (explicit), offset {}",
            result.partition,
            result.offset.unwrap_or(0)
        );
    }
    println!();

    // Example 3: Batch sends (same key)
    println!("Example 3: Batch sends to same partition");
    let user_key = b"user123";
    for i in 0..5 {
        let event = format!("{{\"action\": \"click\", \"timestamp\": {}}}", i);

        let result = producer
            .send("user-events", Some(user_key), event.as_bytes(), None)
            .await?;

        println!(
            "  Event {} → partition {}, offset {}",
            i,
            result.partition,
            result.offset.unwrap_or(0)
        );
    }
    println!();

    // Step 5: Flush and close
    println!("🏁 Step 5: Flushing and closing");
    producer.flush().await?;
    println!("   ✅ All records flushed");

    producer.close().await?;
    println!("   ✅ Producer closed\n");

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Example Complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Summary:");
    println!("  • Created 2 topics (orders: 3 partitions, user-events: 6 partitions)");
    println!("  • Sent 18 total records");
    println!("  • Demonstrated key-based partitioning");
    println!("  • Demonstrated explicit partition selection");
    println!("  • Demonstrated batch sends to same partition");
    println!();

    Ok(())
}
