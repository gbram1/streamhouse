use std::sync::Arc;
use std::time::Duration;
use streamhouse_client::{Consumer, OffsetReset};
use streamhouse_metadata::SqliteMetadataStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("StreamHouse Consumer Demo");
    println!("");

    // Setup
    let metadata = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse-demo/metadata.db").await?
    );

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
    println!("  Topic: demo_orders (2 partitions)");
    println!("");

    // Read all messages
    println!("Reading orders...");
    let mut total_read = 0;
    let mut polls = 0;
    let start = std::time::Instant::now();

    loop {
        let records = consumer.poll(Duration::from_secs(1)).await?;

        if records.is_empty() {
            if total_read > 0 {
                break; // Done reading
            }
            polls += 1;
            if polls > 5 {
                break; // No data found
            }
            continue;
        }

        for record in &records {
            total_read += 1;
            let value_str = String::from_utf8_lossy(&record.value);
            println!(
                "  [{:02}] partition {} offset {} - {}",
                total_read,
                record.partition,
                record.offset,
                value_str
            );
        }
    }

    let elapsed = start.elapsed();
    consumer.close().await?;

    println!("");
    println!("✓ Consumer finished");
    println!("  Total records: {}", total_read);
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.0} rec/s", total_read as f64 / elapsed.as_secs_f64());

    Ok(())
}
