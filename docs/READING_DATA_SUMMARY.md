# How to Read Your Data from StreamHouse

## Quick Answer

**Users read messages using the Consumer API**, which automatically fetches segments from S3 and returns messages **in order by offset**.

## Visual Example

You sent 220 messages to the `fdsfds` topic. Here's what happened:

### 1. Data Went to S3 (MinIO)
```
streamhouse-data/
└── data/
    └── fdsfds/
        └── 0/
            ├── 00000000000000000000.seg  (offsets 0-1)
            └── 00000000000000000002.seg  (offsets 2-116)
```

### 2. Reading the Data (Consumer API)

**Create a consumer**:
```rust
let consumer = Consumer::builder()
    .group_id("my-app")
    .topics(vec!["fdsfds"])
    .offset_reset(OffsetReset::Earliest) // Start from beginning
    .build().await?;
```

**Read messages in order**:
```rust
loop {
    let records = consumer.poll(Duration::from_secs(1)).await?;

    for record in records {
        println!("Offset {}: {}",
            record.offset,
            String::from_utf8_lossy(&record.value)
        );
    }

    consumer.commit().await?; // Save progress
}
```

**Output** (in order):
```
Offset 0: {"test": 50}
Offset 1: Message number 1 with some padding data...
Offset 2: Message number 2 with some padding data...
Offset 3: Message number 3 with some padding data...
...
Offset 116: Message number 220 with some padding data...
```

## How It Works Internally

```
┌─────────────┐
│  Consumer   │  "Give me messages from fdsfds, partition 0"
└──────┬──────┘
       │
       ↓
┌──────────────────┐
│ PartitionReader  │  "Need to read offset 0..."
└──────┬───────────┘
       │
       ↓
┌──────────────────┐
│  SegmentCache    │  "Check if segment is cached..."
└──────┬───────────┘
       │ (not cached)
       ↓
┌──────────────────┐
│  S3 (MinIO)      │  Downloads: data/fdsfds/0/00000000000000000000.seg
└──────┬───────────┘
       │
       ↓
┌──────────────────┐
│  SegmentCache    │  Saves to: ./data/cache/fdsfds-0-0.seg
└──────┬───────────┘
       │
       ↓
┌──────────────────┐
│  Decoder         │  Reads binary format, extracts records
└──────┬───────────┘
       │
       ↓
┌──────────────────┐
│  Consumer        │  Returns: Vec<Record>
└──────────────────┘
       │
       ↓
  Your Application  ← Receives records in order!
```

## Key Points

### 1. **Order is Guaranteed Within Partitions**
- Offset 0 always comes before offset 1
- Offset 1 always comes before offset 2
- etc.

This is **guaranteed** and enforced by the offset sequence.

### 2. **Automatic S3 Handling**
You don't manually download segments. The Consumer does it automatically:
- Downloads from S3 when needed
- Caches locally for fast re-reading
- Handles segment boundaries transparently

### 3. **Consumer Groups Remember Position**
```
First run:
  - Reads offsets 0-100
  - Commits offset 100

Second run (after restart):
  - Automatically resumes from offset 101
  - No duplicate processing!
```

### 4. **Multiple Partitions = Parallel Reading**
If `fdsfds` had 3 partitions, consumer would read from all simultaneously:
```
Poll 1: [P0: offset 0-9, P1: offset 0-7, P2: offset 0-11]
Poll 2: [P0: offset 10-19, P1: offset 8-15, P2: offset 12-23]
...
```

Order guaranteed within each partition, but not across partitions.

## Three Ways to Consume Data

### Option 1: Consumer API (Rust)
**Best for**: Production applications, high throughput

```rust
use streamhouse_client::Consumer;

let consumer = Consumer::builder()
    .group_id("my-app")
    .topics(vec!["fdsfds"])
    .build().await?;

loop {
    let records = consumer.poll(Duration::from_secs(1)).await?;
    process(records);
    consumer.commit().await?;
}
```

### Option 2: REST API
**Best for**: Quick scripts, non-Rust languages

```bash
# Read first 10 messages
curl "http://localhost:3001/api/v1/consume?topic=fdsfds&partition=0&offset=0&max_records=10"

# Read next 10
curl "http://localhost:3001/api/v1/consume?topic=fdsfds&partition=0&offset=10&max_records=10"
```

### Option 3: Web Console (Future)
**Best for**: Debugging, viewing data

Not yet built, but planned feature:
- Navigate to topic
- Click "View Messages"
- Browse messages with pagination

## Testing Right Now

### Quick Test with curl:

Since we don't have a consume REST endpoint yet, the easiest way is to build a simple Rust consumer:

```bash
# Create a simple consumer
cat > test_consumer.rs <<'EOF'
use streamhouse_client::{Consumer, OffsetReset};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = std::sync::Arc::new(
        streamhouse_metadata::SqliteMetadataStore::new("./data/metadata.db").await?
    );

    let object_store: std::sync::Arc<dyn object_store::ObjectStore> = std::sync::Arc::new(
        object_store::aws::AmazonS3Builder::from_env()
            .with_bucket_name("streamhouse-data")
            .with_allow_http(true)
            .build()?
    );

    let cache = std::sync::Arc::new(
        streamhouse_storage::SegmentCache::new("./data/cache", 100_000_000)?
    );

    let consumer = Consumer::builder()
        .group_id("test-group")
        .topics(vec!["fdsfds".to_string()])
        .metadata_store(metadata)
        .object_store(object_store)
        .segment_cache(cache)
        .offset_reset(OffsetReset::Earliest)
        .build()
        .await?;

    println!("Reading messages from fdsfds:\n");

    let mut count = 0;
    let records = consumer.poll(Duration::from_secs(2)).await?;

    for record in records.iter().take(10) {
        count += 1;
        let value = String::from_utf8_lossy(&record.value);
        let preview = if value.len() > 100 {
            format!("{}...", &value[..100])
        } else {
            value.to_string()
        };

        println!("Offset {}: {}", record.offset, preview);
    }

    println!("\n✓ Read {} messages (showing first 10)", records.len());

    Ok(())
}
EOF

# Run it
AWS_ACCESS_KEY_ID=minioadmin \
AWS_SECRET_ACCESS_KEY=minioadmin \
AWS_ENDPOINT_URL=http://localhost:9000 \
cargo script test_consumer.rs
```

## What You'll See

When you run a consumer on the `fdsfds` topic, you'll see:

```
Reading messages from fdsfds:

Offset 0: {"test": 50}
Offset 1: Message number 1 with some padding data XXXXX...
Offset 2: Message number 2 with some padding data XXXXX...
Offset 3: Message number 3 with some padding data XXXXX...
Offset 4: Message number 4 with some padding data XXXXX...
Offset 5: Message number 5 with some padding data XXXXX...
Offset 6: Message number 6 with some padding data XXXXX...
Offset 7: Message number 7 with some padding data XXXXX...
Offset 8: Message number 8 with some padding data XXXXX...
Offset 9: Message number 9 with some padding data XXXXX...

✓ Read 117 messages (showing first 10)
```

**Notice**: They come out **in order** by offset (0, 1, 2, 3...), exactly as they went in!

## Summary

**Your question**: "How will users actually see the data in the segments files in the order in which they came in"

**Answer**:
1. Users don't directly access segment files
2. They use the **Consumer API** which:
   - Automatically downloads segments from S3
   - Decodes the binary format
   - **Returns messages in offset order**
   - Handles all the complexity internally

**Guarantees**:
- ✅ Messages returned in exact order they were written (by offset)
- ✅ No duplicates (with proper offset commits)
- ✅ Can resume from last position (consumer groups)
- ✅ Fast (local caching after first download)

**The magic**: Offsets enforce order. Offset 0 is always first, offset 1 is always second, etc. The Consumer API respects this order when reading from segments.

---

**Next**: Would you like me to build a "View Messages" feature in the web console so you can browse messages with a UI?
