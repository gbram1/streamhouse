//! Example: Consume messages from fdsfds topic
//!
//! This example shows how to read all messages from a topic in order.
//!
//! Run with:
//! ```bash
//! cargo run --release --example consume_fdsfds
//! ```

use object_store::aws::AmazonS3Builder;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_client::{Consumer, OffsetReset};
use streamhouse_metadata::SqliteMetadataStore;
use streamhouse_storage::SegmentCache;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    tracing_subscriber::fmt::init();

    println!("🚀 StreamHouse Consumer Example");
    println!("Reading messages from 'fdsfds' topic...\n");

    // Connect to metadata store
    let metadata = Arc::new(SqliteMetadataStore::new("./data/metadata.db").await?);

    // Connect to MinIO
    let object_store: Arc<dyn object_store::ObjectStore> = Arc::new(
        AmazonS3Builder::from_env()
            .with_bucket_name("streamhouse-data")
            .with_allow_http(true) // Required for MinIO
            .build()?,
    );

    // Setup segment cache
    let cache = Arc::new(SegmentCache::new("./data/cache", 100 * 1024 * 1024)?);

    // Create consumer
    let consumer = Consumer::builder()
        .group_id("example-consumer-group")
        .topics(vec!["fdsfds".to_string()])
        .metadata_store(metadata)
        .object_store(object_store)
        .segment_cache(cache)
        .offset_reset(OffsetReset::Earliest) // Start from beginning
        .build()
        .await?;

    println!("✓ Consumer connected");
    println!("  Group ID: example-consumer-group");
    println!("  Topic: fdsfds");
    println!("  Starting from: earliest offset");
    println!("\n📖 Reading messages in order:\n");

    let mut total_consumed = 0;
    let mut last_partition = None;
    let mut last_offset = None;

    // Poll for messages
    for _ in 0..10 {
        // Poll for up to 1 second
        let records = consumer.poll(Duration::from_secs(1)).await?;

        if records.is_empty() {
            println!("No more messages available");
            break;
        }

        for record in records {
            total_consumed += 1;

            // Show first 100 chars of value
            let value_preview = String::from_utf8_lossy(&record.value);
            let preview = if value_preview.len() > 100 {
                format!("{}...", &value_preview[..100])
            } else {
                value_preview.to_string()
            };

            println!(
                "  [Partition {}] Offset {}: {}",
                record.partition, record.offset, preview
            );

            last_partition = Some(record.partition);
            last_offset = Some(record.offset);
        }

        // Commit offsets after processing
        consumer.commit().await?;
    }

    println!("\n✓ Consumed {} messages", total_consumed);
    if let (Some(partition), Some(offset)) = (last_partition, last_offset) {
        println!(
            "  Last position: Partition {}, Offset {}",
            partition, offset
        );
    }

    println!("\n💡 Next time you run this, it will resume from where it left off!");
    println!("   (Because we committed the offsets)");

    Ok(())
}
