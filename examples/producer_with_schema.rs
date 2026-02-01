//! Example: Producer with Schema Registry Integration
//!
//! This example demonstrates how to send messages with automatic schema
//! registration and validation using the StreamHouse Schema Registry.
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
//! # Usage
//!
//! ```bash
//! cargo run --example producer_with_schema
//! ```

use streamhouse_client::{Producer, SchemaFormat};
use streamhouse_metadata::SqliteMetadata;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== StreamHouse Producer with Schema Registry Example ===\n");

    // 1. Create metadata store (using SQLite for simplicity)
    let metadata = Arc::new(SqliteMetadata::new("producer_schema_example.db").await?);

    // 2. Create topic if it doesn't exist
    let topic_name = "orders";
    println!("1. Creating topic '{}'...", topic_name);
    
    match metadata.create_topic(topic_name, 3, None, None).await {
        Ok(_) => println!("   ✓ Topic created successfully"),
        Err(e) if e.to_string().contains("already exists") => {
            println!("   ✓ Topic already exists")
        }
        Err(e) => return Err(e.into()),
    }

    // 3. Build producer with schema registry enabled
    println!("\n2. Building producer with schema registry...");
    let producer = Producer::builder()
        .metadata_store(Arc::clone(&metadata))
        .agent_group("default")
        .schema_registry("http://localhost:8080") // Point to unified server
        .build()
        .await?;

    println!("   ✓ Producer initialized");

    // 4. Define Avro schema
    let avro_schema = r#"{
        "type": "record",
        "name": "Order",
        "namespace": "com.example",
        "fields": [
            {"name": "id", "type": "int"},
            {"name": "customer", "type": "string"},
            {"name": "amount", "type": "double"}
        ]
    }"#;

    println!("\n3. Avro Schema:");
    println!("   {}", avro_schema.replace("\n", "\n   "));

    // 5. Send messages with schema (schema will be auto-registered)
    println!("\n4. Sending messages with schema...");

    for i in 1..=5 {
        // In a real application, you would serialize using an Avro library
        // For this example, we're using JSON as placeholder
        let value = format!(
            r#"{{"id": {}, "customer": "customer_{}", "amount": {}.99}}"#,
            i, i, i * 10
        );

        let result = producer
            .send_with_schema(
                topic_name,
                Some(format!("order-{}", i).as_bytes()),
                value.as_bytes(),
                avro_schema,
                SchemaFormat::Avro,
                None,
            )
            .await?;

        println!(
            "   ✓ Message {} sent to partition {} (schema will be auto-registered on first send)",
            i, result.partition
        );
    }

    println!("\n5. Waiting for batches to flush...");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    println!("\n=== Success! ===");
    println!("\nWhat happened:");
    println!("1. Schema was registered with the Schema Registry (or existing ID was returned)");
    println!("2. Schema ID was cached locally for subsequent sends");
    println!("3. Each message was prepended with:");
    println!("   - Magic byte (0x00)");
    println!("   - Schema ID (4 bytes, big-endian)");
    println!("   - Original payload");
    println!("\nYou can verify the schema registration at:");
    println!("  http://localhost:8080/schemas/subjects");

    // 6. Close producer
    producer.close().await;
    println!("\n✓ Producer closed");

    Ok(())
}
