//! Example: Consumer with Schema Registry Integration
//!
//! This example demonstrates how to consume messages with automatic schema
//! resolution using the StreamHouse Schema Registry.
//!
//! # Prerequisites
//!
//! 1. Run PostgreSQL:
//!    ```bash
//!    docker run -d -p 5432:5432 \
//!      -e POSTGRES_USER=postgres \
//!      -e POSTGRES_PASSWORD=password \
//!      -e POSTGRES_DB=streamhouse \
//!      postgres:15
//!    ```
//!
//! 2. Run StreamHouse server with schema registry enabled:
//!    ```bash
//!    cargo run --bin unified-server --features postgres
//!    ```
//!
//! 3. Run the producer example to send messages with schemas:
//!    ```bash
//!    cargo run --example producer_with_schema
//!    ```
//!
//! # Usage
//!
//! ```bash
//! cargo run --example consumer_with_schema
//! ```

use streamhouse_client::Consumer;
use streamhouse_metadata::SqliteMetadata;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== StreamHouse Consumer with Schema Registry Example ===\n");

    // 1. Create metadata store (using SQLite for simplicity)
    let metadata = Arc::new(SqliteMetadata::new("consumer_schema_example.db").await?);

    // 2. Create object store (using local filesystem)
    let object_store: Arc<dyn object_store::ObjectStore> = Arc::new(
        object_store::local::LocalFileSystem::new_with_prefix("/tmp/streamhouse-data")?
    );

    // 3. Build consumer with schema registry enabled
    println!("1. Building consumer with schema registry...");
    let mut consumer = Consumer::builder()
        .group_id("schema-test-group")
        .topics(vec!["orders".to_string()])
        .metadata_store(Arc::clone(&metadata))
        .object_store(object_store)
        .schema_registry("http://localhost:8080") // Point to unified server
        .build()
        .await?;

    println!("   ✓ Consumer initialized");
    println!("\n2. Polling for messages (Ctrl+C to stop)...\n");

    // 4. Poll for messages (will auto-resolve schemas)
    let mut total_messages = 0;
    let mut schemas_resolved = 0;

    for _ in 0..10 {
        let records = consumer.poll(Duration::from_secs(1)).await?;

        if records.is_empty() {
            println!("   No messages available, waiting...");
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }

        for record in records {
            total_messages += 1;

            print!("   ✓ Message {}: ", total_messages);
            print!("topic={}, partition={}, offset={}", 
                record.topic, record.partition, record.offset);

            // Check if schema was resolved
            if let Some(schema_id) = record.schema_id {
                schemas_resolved += 1;
                print!(", schema_id={}", schema_id);

                if let Some(ref schema) = record.schema {
                    print!(", schema={}/v{}", schema.subject, schema.version);
                }

                println!(" [SCHEMA RESOLVED]");

                // Show the actual payload (without magic byte + schema ID)
                println!("      Value: {}", String::from_utf8_lossy(&record.value));
            } else {
                println!(" [NO SCHEMA]");
                println!("      Value: {}", String::from_utf8_lossy(&record.value));
            }
        }

        // Commit offsets
        consumer.commit().await?;
    }

    println!("\n=== Summary ===");
    println!("Total messages: {}", total_messages);
    println!("Schemas resolved: {}", schemas_resolved);

    if schemas_resolved > 0 {
        println!("\nWhat happened:");
        println!("1. Consumer extracted schema IDs from message payloads");
        println!("2. Schemas were resolved from the registry (or cache)");
        println!("3. ConsumedRecord.schema_id and .schema fields were populated");
        println!("4. Message payloads were stripped of schema ID header");
        println!("\nYou can now deserialize messages using the resolved schemas!");
    } else {
        println!("\nNo schemas were found in messages.");
        println!("Run the producer_with_schema example first to send messages with schemas.");
    }

    // 5. Close consumer
    consumer.close().await?;
    println!("\n✓ Consumer closed");

    Ok(())
}
