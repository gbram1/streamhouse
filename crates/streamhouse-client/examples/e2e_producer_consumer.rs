//! End-to-End Producer-Consumer Workflow Example
//!
//! This example demonstrates a complete workflow using both Producer and Consumer APIs
//! with offset tracking, showing how they work together in a realistic scenario.
//!
//! Scenario: Order processing pipeline
//! - Producer writes orders to "orders" topic
//! - Consumer reads orders and processes them
//! - Demonstrates offset management, consumer groups, and commit behavior
//!
//! Run with:
//! ```bash
//! cargo run --example e2e_producer_consumer
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_client::{Consumer, OffsetReset, Producer};
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n🎯 StreamHouse E2E Producer-Consumer Example");
    println!("=============================================\n");

    // Setup infrastructure
    println!("📊 Setting up infrastructure...");
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("e2e_workflow.db");
    let data_path = temp_dir.path().join("data");
    std::fs::create_dir_all(&data_path)?;

    let metadata: Arc<dyn MetadataStore> =
        Arc::new(SqliteMetadataStore::new(db_path.to_str().unwrap()).await?);
    let object_store = Arc::new(object_store::local::LocalFileSystem::new_with_prefix(
        &data_path,
    )?) as Arc<dyn object_store::ObjectStore>;

    // Create topics
    metadata
        .create_topic(TopicConfig {
            name: "orders".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            config: HashMap::new(),
        })
        .await?;

    metadata
        .create_topic(TopicConfig {
            name: "processed_orders".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            config: HashMap::new(),
        })
        .await?;

    println!("✅ Topics created: 'orders', 'processed_orders'\n");

    // ============================================================================
    // Phase 1: Producer writes orders
    // ============================================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📤 Phase 1: Producer Writing Orders");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let producer = Producer::builder()
        .metadata_store(Arc::clone(&metadata))
        .agent_group("default")
        .batch_size(10)
        .compression_enabled(true)
        .build()
        .await?;

    // Write 20 orders across multiple partitions
    println!("Writing 20 orders...");
    let mut producer_offsets = Vec::new();

    for i in 0..20 {
        let user_id = format!("user_{}", i % 5); // 5 different users
        let order_data = format!(
            r#"{{"order_id": {}, "user_id": "{}", "amount": {}, "status": "pending"}}"#,
            i,
            user_id,
            (i + 1) * 50
        );

        // Use key-based partitioning (same user goes to same partition)
        let offset = producer
            .send_and_wait(
                "orders",
                Some(user_id.as_bytes()),
                order_data.as_bytes(),
                None, // Let producer choose partition based on key
            )
            .await?;

        producer_offsets.push((user_id.clone(), offset));

        if i % 5 == 4 {
            println!("  ✅ Wrote orders {}-{}", i - 4, i);
        }
    }

    println!("\n✅ All 20 orders written with confirmed offsets");
    println!(
        "  First offset: {}, Last offset: {}",
        producer_offsets[0].1, producer_offsets[19].1
    );

    // ============================================================================
    // Phase 2: Consumer processes orders
    // ============================================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📥 Phase 2: Consumer Processing Orders");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let mut consumer = Consumer::builder()
        .group_id("order_processor")
        .topics(vec!["orders".to_string()])
        .metadata_store(Arc::clone(&metadata))
        .object_store(Arc::clone(&object_store))
        .offset_reset(OffsetReset::Earliest)
        .auto_commit(false) // Manual commit for demonstration
        .build()
        .await?;

    println!("Consumer group 'order_processor' started");
    println!("Processing orders...\n");

    let mut processed_count = 0;
    let mut total_amount = 0;

    // Poll for records
    let records = consumer.poll(Duration::from_secs(2)).await?;
    println!("Retrieved {} records", records.len());

    for record in &records {
        // Parse order data
        let order_data = String::from_utf8_lossy(&record.value);
        let user_key = String::from_utf8_lossy(record.key.as_ref().unwrap());

        // Simulate processing
        processed_count += 1;

        // Extract amount (simple string parsing for demo)
        if let Some(amount_start) = order_data.find(r#""amount": "#) {
            let amount_str = &order_data[amount_start + 10..];
            if let Some(amount_end) = amount_str.find(',') {
                if let Ok(amount) = amount_str[..amount_end].parse::<u32>() {
                    total_amount += amount;
                }
            }
        }

        if processed_count <= 5 {
            println!(
                "  Processing: partition={}, offset={}, user={}",
                record.partition, record.offset, user_key
            );
        }
    }

    println!("  ... (processed {} more)", processed_count - 5);
    println!("\n✅ Processed {} orders", processed_count);
    println!("  Total amount: ${}", total_amount);

    // Commit offsets after processing
    println!("\nCommitting offsets...");
    consumer.commit().await?;
    println!("✅ Offsets committed for consumer group 'order_processor'\n");

    // ============================================================================
    // Phase 3: Write processed results to another topic
    // ============================================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 Phase 3: Writing Processing Results");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Write summary to processed_orders topic
    let summary = format!(
        r#"{{"total_orders": {}, "total_amount": {}, "status": "completed"}}"#,
        processed_count, total_amount
    );

    let summary_offset = producer
        .send_and_wait(
            "processed_orders",
            Some(b"summary"),
            summary.as_bytes(),
            Some(0),
        )
        .await?;

    println!("✅ Processing summary written to 'processed_orders' topic");
    println!("  Offset: {}", summary_offset);

    // ============================================================================
    // Phase 4: Demonstrate consumer position and seeking
    // ============================================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔍 Phase 4: Consumer Position & Seeking");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Check current position
    let position = consumer.position("orders", 0).await?;
    println!("Current position for orders:0 = {}", position);

    // Seek to beginning and re-read
    println!("Seeking to beginning...");
    consumer.seek("orders", 0, 0).await?;

    let reread_records = consumer.poll(Duration::from_secs(1)).await?;
    println!("✅ Re-read {} records from beginning", reread_records.len());

    if !reread_records.is_empty() {
        println!(
            "  First record offset: {}",
            reread_records.first().unwrap().offset
        );
        println!(
            "  Last record offset: {}",
            reread_records.last().unwrap().offset
        );
    }

    // ============================================================================
    // Phase 5: Second consumer with same group (demonstrates offset sharing)
    // ============================================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("👥 Phase 5: Consumer Group Behavior");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Create second consumer with same group_id
    let mut consumer2 = Consumer::builder()
        .group_id("order_processor") // Same group!
        .topics(vec!["orders".to_string()])
        .metadata_store(Arc::clone(&metadata))
        .object_store(Arc::clone(&object_store))
        .offset_reset(OffsetReset::Earliest)
        .build()
        .await?;

    println!("Second consumer joined group 'order_processor'");

    // This consumer should start from the committed offset
    let records2 = consumer2.poll(Duration::from_secs(1)).await?;

    if records2.is_empty() {
        println!("✅ No new records (consumer group offset already at end)");
    } else {
        println!("📦 Consumer2 read {} records", records2.len());
    }

    // ============================================================================
    // Cleanup
    // ============================================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🧹 Cleanup");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    consumer.close().await?;
    consumer2.close().await?;
    producer.close().await?;

    println!("✅ All clients closed\n");

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ E2E Workflow Complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("What we demonstrated:");
    println!("  • Producer writing with offset tracking");
    println!("  • Key-based partitioning (same user → same partition)");
    println!("  • Consumer reading and processing records");
    println!("  • Manual offset commits");
    println!("  • Writing processing results to another topic");
    println!("  • Consumer position tracking and seeking");
    println!("  • Consumer group behavior (offset sharing)");
    println!();

    Ok(())
}
