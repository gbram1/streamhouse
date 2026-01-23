use object_store::{aws::AmazonS3Builder, ObjectStore};
use std::sync::Arc;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};
use streamhouse_storage::{PartitionReader, SegmentCache};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <topic> <partition> [start_offset] [count]", args[0]);
        std::process::exit(1);
    }

    let topic = &args[1];
    let partition: u32 = args[2].parse()?;
    let start_offset: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let count: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(10);

    let metadata: Arc<dyn MetadataStore> = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse_pipeline.db").await?
    );

    let object_store: Arc<dyn ObjectStore> = Arc::new(
        AmazonS3Builder::new()
            .with_bucket_name("streamhouse")
            .with_region("us-east-1")
            .with_endpoint("http://localhost:9000")
            .with_access_key_id("minioadmin")
            .with_secret_access_key("minioadmin")
            .with_allow_http(true)
            .build()?,
    );

    let cache = Arc::new(SegmentCache::new("/tmp/streamhouse-read-cache", 1024 * 1024 * 100)?);

    let reader = PartitionReader::new(
        topic.clone(),
        partition,
        metadata.clone(),
        object_store.clone(),
        cache.clone(),
    );

    println!("\n📖 Reading from {}/partition-{}", topic, partition);
    println!("   Starting offset: {}", start_offset);
    println!("   Count: {}\n", count);

    let result = reader.read(start_offset, count).await?;

    println!("✅ Read {} records\n", result.records.len());

    for (i, record) in result.records.iter().enumerate() {
        println!("─────────────────────────────────────────");
        println!("Record #{} (offset={})", i, record.offset);
        println!("─────────────────────────────────────────");

        if let Some(key) = &record.key {
            println!("  Key:       {}", String::from_utf8_lossy(key));
        } else {
            println!("  Key:       (null)");
        }

        println!("  Timestamp: {}", record.timestamp);

        let value_str = String::from_utf8_lossy(&record.value);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&value_str) {
            println!("  Value:     {}", serde_json::to_string_pretty(&json)?);
        } else {
            println!("  Value:     {}", value_str);
        }
        println!();
    }

    Ok(())
}
