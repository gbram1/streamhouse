use std::sync::Arc;
use std::time::Duration;
use streamhouse_client::{Consumer, OffsetReset};
use streamhouse_metadata::SqliteMetadataStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup metadata store
    let metadata = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse-demo/metadata/metadata.db")
            .await?
    );

    // Setup object store (local filesystem)
    let object_store = Arc::new(
        object_store::local::LocalFileSystem::new_with_prefix("/tmp/streamhouse-demo/storage")?
    );

    // Create consumer
    let mut consumer = Consumer::builder()
        .group_id("demo_analytics")
        .topics(vec!["demo_orders".to_string()])
        .metadata_store(metadata)
        .object_store(object_store)
        .offset_reset(OffsetReset::Earliest)
        .auto_commit(true)
        .auto_commit_interval(Duration::from_secs(2))
        .cache_dir("/tmp/streamhouse-demo/cache")
        .cache_size_bytes(50 * 1024 * 1024)
        .build()
        .await?;

    println!("✓ Consumer connected");
    println!("  Group ID: demo_analytics");
    println!("  Subscribed to: demo_orders");
    println!("");

    // Poll for messages
    println!("Reading orders...");
    let mut total_read = 0;
    let mut poll_count = 0;
    let max_polls = 10;

    while poll_count < max_polls {
        let records = consumer.poll(Duration::from_secs(1)).await?;

        if records.is_empty() {
            if total_read > 0 {
                // No more records, we're done
                break;
            }
            poll_count += 1;
            continue;
        }

        for record in &records {
            total_read += 1;
            let value_str = String::from_utf8_lossy(&record.value);
            println!(
                "  [{}:{}:{}] {}",
                record.topic,
                record.partition,
                record.offset,
                value_str
            );
        }

        println!("  Polled {} records (total: {})", records.len(), total_read);
        poll_count += 1;
    }

    // Final commit and close
    consumer.commit().await?;
    consumer.close().await?;

    println!("");
    println!("✓ Consumer finished");
    println!("  Total records read: {}", total_read);

    Ok(())
}
