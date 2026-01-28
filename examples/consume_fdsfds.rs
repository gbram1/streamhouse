//! Example: Consume messages from fdsfds topic
//!
//! This example shows how to read all messages from a topic in order.
//!
//! Run with:
//! ```bash
//! AWS_ACCESS_KEY_ID=minioadmin AWS_SECRET_ACCESS_KEY=minioadmin AWS_ENDPOINT_URL=http://localhost:9000 cargo run --release --example consume_fdsfds
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

    // Poll for messages (loop until no more messages)
    loop {
        // Poll for up to 1 second
        let records = consumer.poll(Duration::from_secs(1)).await?;

        if records.is_empty() {
            println!("\n✓ No more messages available (reached end of topic)");
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

            // Also decode key if present
            let key_str = record
                .key
                .as_ref()
                .map(|k| String::from_utf8_lossy(k).to_string())
                .unwrap_or_else(|| "(none)".to_string());

            println!(
                "  [Partition {}] Offset {}: Key={}, Value={}",
                record.partition, record.offset, key_str, preview
            );

            last_partition = Some(record.partition);
            last_offset = Some(record.offset);
        }

        // Commit offsets after processing batch
        consumer.commit().await?;
        println!("  ✓ Committed offsets");
    }

    println!("\n📊 Summary:");
    println!("  Total messages consumed: {}", total_consumed);
    if let (Some(partition), Some(offset)) = (last_partition, last_offset) {
        println!(
            "  Last position: Partition {}, Offset {}",
            partition, offset
        );
    }

    println!("\n💡 Tips:");
    println!("  • Messages are always read in order within each partition");
    println!("  • Run this again and it will resume from where it left off");
    println!("  • Change group_id to start from beginning again");

    Ok(())
}
