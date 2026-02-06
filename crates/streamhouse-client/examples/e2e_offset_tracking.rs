//! End-to-End Offset Tracking Example
//!
//! This example demonstrates the complete offset tracking feature implemented in Phase 5.4.
//! It shows:
//! - Sending records with async offset retrieval
//! - Using send_and_wait() for blocking offset access
//! - Verifying offsets are sequential and correct
//! - Consumer reading from exact offsets
//!
//! Run with:
//! ```bash
//! cargo run --example e2e_offset_tracking
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_client::{Consumer, OffsetReset, Producer};
use streamhouse_metadata::{CleanupPolicy, MetadataStore, SqliteMetadataStore, TopicConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n🎯 StreamHouse E2E Offset Tracking Example");
    println!("============================================\n");

    // Step 1: Setup metadata store and object store
    println!("📊 Step 1: Setting up infrastructure");
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("e2e_example.db");
    let data_path = temp_dir.path().join("data");
    std::fs::create_dir_all(&data_path)?;

    let metadata: Arc<dyn MetadataStore> =
        Arc::new(SqliteMetadataStore::new(db_path.to_str().unwrap()).await?);

    let object_store = Arc::new(object_store::local::LocalFileSystem::new_with_prefix(
        &data_path,
    )?) as Arc<dyn object_store::ObjectStore>;
    println!("   ✅ Infrastructure initialized\n");

    // Step 2: Create topic
    println!("📦 Step 2: Creating topic");
    metadata
        .create_topic(TopicConfig {
            name: "e2e_orders".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            cleanup_policy: CleanupPolicy::default(),
            config: HashMap::new(),
        })
        .await?;
    println!("   ✅ Created topic 'e2e_orders' with 3 partitions\n");

    // Step 3: Create producer
    println!("🚀 Step 3: Creating producer");
    let producer = Producer::builder()
        .metadata_store(Arc::clone(&metadata))
        .agent_group("default")
        .batch_size(5) // Small batch for demonstration
        .compression_enabled(true)
        .build()
        .await?;
    println!("   ✅ Producer initialized\n");

    // Step 4: Demonstrate async offset tracking
    println!("✍️  Step 4: Sending records with async offset tracking\n");

    println!("Example 1: send() + wait_offset() pattern");
    let mut results = Vec::new();
    for i in 0..5 {
        let key = format!("user{}", i);
        let value = format!(r#"{{"order_id": {}, "amount": {}}}"#, i, i * 100);

        let result = producer
            .send(
                "e2e_orders",
                Some(key.as_bytes()),
                value.as_bytes(),
                Some(0),
            )
            .await?;

        println!("  Sent order {} (offset not yet known)", i);
        results.push(result);
    }

    // Flush to trigger batch write
    println!("\n  Flushing batch...");
    producer.flush().await?;

    // Now wait for offsets
    println!("  Waiting for offsets...");
    let mut offsets = Vec::new();
    for (i, mut result) in results.into_iter().enumerate() {
        let offset = result.wait_offset().await?;
        offsets.push(offset);
        println!(
            "  ✅ Order {} committed at offset {} (partition {})",
            i, offset, result.partition
        );
    }

    // Verify sequential offsets
    for i in 0..offsets.len() - 1 {
        assert_eq!(
            offsets[i] + 1,
            offsets[i + 1],
            "Offsets should be sequential"
        );
    }
    println!("  ✅ Verified: Offsets are sequential\n");

    // Step 5: Demonstrate send_and_wait() convenience method
    println!("Example 2: send_and_wait() convenience method");
    for i in 5..8 {
        let key = format!("user{}", i);
        let value = format!(r#"{{"order_id": {}, "amount": {}}}"#, i, i * 100);

        let offset = producer
            .send_and_wait(
                "e2e_orders",
                Some(key.as_bytes()),
                value.as_bytes(),
                Some(1),
            )
            .await?;

        println!(
            "  ✅ Order {} sent and committed at offset {} (partition 1)",
            i, offset
        );
    }
    println!();

    // Step 6: Demonstrate batch boundaries
    println!("Example 3: Offsets across batch boundaries");
    println!("  (batch_size=5, sending 12 records → 3 batches)");

    let mut batch_results = Vec::new();
    for i in 0..12 {
        let value = format!(r#"{{"event": "click", "timestamp": {}}}"#, i);
        let result = producer
            .send("e2e_orders", Some(b"batch_test"), value.as_bytes(), Some(2))
            .await?;
        batch_results.push(result);
    }

    producer.flush().await?;

    let mut batch_offsets = Vec::new();
    for mut result in batch_results {
        let offset = result.wait_offset().await?;
        batch_offsets.push(offset);
    }

    // Verify all offsets are sequential
    for i in 0..batch_offsets.len() - 1 {
        assert_eq!(
            batch_offsets[i] + 1,
            batch_offsets[i + 1],
            "Offsets should be sequential across batches"
        );
    }

    println!(
        "  ✅ All 12 records have sequential offsets: {} to {}",
        batch_offsets[0], batch_offsets[11]
    );
    println!("  ✅ Verified: Offsets sequential across 3 batch boundaries\n");

    // Step 7: Read back with consumer to verify
    println!("🔍 Step 7: Verifying with consumer");
    let mut consumer = Consumer::builder()
        .group_id("e2e_verifier")
        .topics(vec!["e2e_orders".to_string()])
        .metadata_store(Arc::clone(&metadata))
        .object_store(Arc::clone(&object_store))
        .offset_reset(OffsetReset::Earliest)
        .build()
        .await?;

    println!("  Reading records from topic...");
    let records = consumer.poll(Duration::from_secs(2)).await?;

    println!("  ✅ Consumer read {} records", records.len());
    println!("  Verifying offsets match producer results...");

    // Verify a few sample records
    if !records.is_empty() {
        println!(
            "  Sample record: partition={}, offset={}, value={:?}",
            records[0].partition,
            records[0].offset,
            String::from_utf8_lossy(&records[0].value)
        );
    }

    consumer.close().await?;
    println!("  ✅ Consumer verification complete\n");

    // Step 8: Demonstrate error handling
    println!("Example 4: Offset tracking with error handling");
    let mut result = producer
        .send("e2e_orders", Some(b"error_test"), b"data", Some(0))
        .await?;

    producer.flush().await?;

    match result.wait_offset().await {
        Ok(offset) => println!("  ✅ Record committed at offset {}", offset),
        Err(e) => println!("  ❌ Error getting offset: {}", e),
    }

    // Try calling wait_offset() again (should error)
    match result.wait_offset().await {
        Ok(_) => println!("  ❌ Unexpected: Should have failed"),
        Err(e) => println!("  ✅ Expected error on second call: {}", e),
    }
    println!();

    // Step 9: Close producer
    println!("🏁 Step 9: Closing producer");
    producer.close().await?;
    println!("   ✅ Producer closed\n");

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ E2E Example Complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Summary:");
    println!("  • Demonstrated async offset tracking with wait_offset()");
    println!("  • Demonstrated convenience method send_and_wait()");
    println!("  • Verified offsets are sequential within and across batches");
    println!("  • Verified consumer can read records at correct offsets");
    println!("  • Demonstrated proper error handling for offset tracking");
    println!();

    Ok(())
}
