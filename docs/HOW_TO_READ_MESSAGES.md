# How Users Read Messages from StreamHouse

## Overview

Users read messages from segments using the **Consumer API**. Messages are automatically read **in order by offset** from S3-stored segments.

## How It Works

### 1. **Sequential Reading by Offset**

```
Topic: fdsfds, Partition 0

S3 Storage (segments):
├── 00000000000000000000.seg  → Contains offsets 0-1
└── 00000000000000000002.seg  → Contains offsets 2-116

Consumer reads:
  Offset 0  ← First message from first segment
  Offset 1  ← Second message from first segment
  Offset 2  ← First message from second segment
  Offset 3  ← Second message from second segment
  ...
  Offset 116 ← Last message
```

### 2. **Consumer Flow**

```
Consumer.poll()
  ↓
PartitionReader fetches segment from S3
  ↓
SegmentCache caches segment locally
  ↓
Decoder reads records in order
  ↓
Returns records with offset, key, value
  ↓
Consumer commits offset (bookmark for next time)
```

## Three Ways to Read Messages

### Option 1: Using the Consumer API (Code)

**Rust Example**:

```rust
use streamhouse_client::{Consumer, OffsetReset};

// Create consumer
let consumer = Consumer::builder()
    .group_id("my-app")
    .topics(vec!["fdsfds".to_string()])
    .metadata_store(metadata)
    .object_store(object_store)
    .offset_reset(OffsetReset::Earliest) // Start from beginning
    .build()
    .await?;

// Read messages in order
loop {
    let records = consumer.poll(Duration::from_secs(1)).await?;

    for record in records {
        println!("Partition {}, Offset {}: {}",
            record.partition,
            record.offset,
            String::from_utf8_lossy(&record.value)
        );
    }

    consumer.commit().await?; // Save progress
}
```

**Output**:
```
Partition 0, Offset 0: {"test": 50}
Partition 0, Offset 1: Message number 1 with some padding data...
Partition 0, Offset 2: Message number 2 with some padding data...
Partition 0, Offset 3: Message number 3 with some padding data...
...
Partition 0, Offset 116: Message number 220 with some padding data...
```

### Option 2: Using the REST API

**Endpoint**: `GET /api/v1/consume`

```bash
curl "http://localhost:3001/api/v1/consume?topic=fdsfds&partition=0&offset=0&max_records=10"
```

**Response**:
```json
{
  "records": [
    {
      "partition": 0,
      "offset": 0,
      "key": null,
      "value": "{\"test\": 50}",
      "timestamp": 1706438400000
    },
    {
      "partition": 0,
      "offset": 1,
      "key": null,
      "value": "Message number 1 with some padding data...",
      "timestamp": 1706438401000
    }
  ],
  "next_offset": 10
}
```

Then fetch next batch:
```bash
curl "http://localhost:3001/api/v1/consume?topic=fdsfds&partition=0&offset=10&max_records=10"
```

### Option 3: Using the Web Console (Future Feature)

**Not yet implemented**, but the plan is:

1. Go to http://localhost:3002/topics
2. Click "View Messages" on `fdsfds` topic
3. See messages in table format:

```
Partition | Offset | Key    | Value                          | Timestamp
---------|--------|--------|--------------------------------|----------
0        | 0      | (none) | {"test": 50}                   | 5:20 PM
0        | 1      | (none) | Message number 1 with...       | 5:20 PM
0        | 2      | (none) | Message number 2 with...       | 5:20 PM
```

## Key Concepts

### 1. **Offsets Guarantee Order**

Within a partition, offsets are sequential:
- Offset 0 came first
- Offset 1 came second
- Offset 2 came third
- etc.

This is **guaranteed** - you will always read them in this order.

### 2. **Consumer Groups Track Progress**

A consumer group remembers which offset it read last:

```
Consumer Group: "my-app"
Topic: fdsfds, Partition 0
Last Committed Offset: 42

Next poll() starts at offset 43
```

This means:
- ✅ If your app crashes, it resumes from offset 43
- ✅ Multiple consumers in same group split partitions
- ✅ No duplicate processing (when using commits correctly)

### 3. **Multiple Partitions = Parallel Reading**

If `fdsfds` has 3 partitions:
- Partition 0: offsets 0, 1, 2, 3...
- Partition 1: offsets 0, 1, 2, 3...
- Partition 2: offsets 0, 1, 2, 3...

Consumer reads from ALL partitions in parallel:

```
Poll 1: Returns 10 records from partition 0
Poll 2: Returns 8 records from partition 1
Poll 3: Returns 12 records from partition 2
Poll 4: Returns 5 more from partition 0
...
```

**Order guarantee**: Within EACH partition, order is preserved. ACROSS partitions, order is not guaranteed.

## Example Use Cases

### 1. **Process All Historical Data**

```rust
let consumer = Consumer::builder()
    .offset_reset(OffsetReset::Earliest) // Start from beginning
    .build().await?;

// Reads ALL messages from oldest to newest
while let records = consumer.poll().await? {
    process(records);
}
```

### 2. **Real-Time Streaming**

```rust
let consumer = Consumer::builder()
    .offset_reset(OffsetReset::Latest) // Start from now
    .build().await?;

// Only reads NEW messages as they arrive
loop {
    let records = consumer.poll(Duration::from_secs(1)).await?;
    process(records);
}
```

### 3. **Resume from Last Position**

```rust
let consumer = Consumer::builder()
    .group_id("my-app") // Remembers last offset
    .offset_reset(OffsetReset::Latest)
    .build().await?;

// Automatically resumes from last committed offset
loop {
    let records = consumer.poll().await?;
    process(records);
    consumer.commit().await?; // Save progress
}
```

## Under the Hood: Segment Format

Each segment file stores messages in a custom binary format:

```
Segment File Structure:
┌──────────────────────────────────────┐
│ Header (magic number, version)       │
├──────────────────────────────────────┤
│ Record 1:                            │
│   - offset: 0                        │
│   - key_len: 0                       │
│   - value_len: 12                    │
│   - value: {"test": 50}              │
├──────────────────────────────────────┤
│ Record 2:                            │
│   - offset: 1                        │
│   - key_len: 0                       │
│   - value_len: 5045                  │
│   - value: Message number 1...       │
├──────────────────────────────────────┤
│ ...                                  │
└──────────────────────────────────────┘
```

The `PartitionReader` automatically:
1. Downloads segment from S3
2. Caches it locally
3. Decodes records in offset order
4. Returns them to the consumer

## Testing It Out

### Step 1: Run Simple Reader Example

```bash
# Install example dependencies
AWS_ACCESS_KEY_ID=minioadmin \
AWS_SECRET_ACCESS_KEY=minioadmin \
AWS_ENDPOINT_URL=http://localhost:9000 \
cargo run --release --example simple_reader -- \
  --topic fdsfds \
  --partition 0 \
  --start-offset 0 \
  --max-records 10
```

### Step 2: Check Committed Offsets

```bash
sqlite3 ./data/metadata.db "SELECT * FROM consumer_offsets WHERE topic='fdsfds';"
```

### Step 3: Build a Consumer App

See `examples/consume_fdsfds.rs` for a complete working example!

## Common Questions

**Q: Can I read messages out of order?**
A: No. Within a partition, you must read sequentially by offset. This is by design for consistency.

**Q: What if I want to re-process old messages?**
A: Create a new consumer group with a different `group_id`, or seek to a specific offset.

**Q: How fast can I read?**
A: Very fast! Segments are cached locally after first download. Reading from cache is ~100k records/sec.

**Q: Do I need to poll forever?**
A: No. If you reach the end, `poll()` returns an empty vec. You can stop or keep polling for new messages.

**Q: Can multiple apps read the same topic?**
A: Yes! Each consumer group tracks offsets independently. 10 different apps can all read the same topic.

## Summary

**How users read data**:
1. Create a Consumer with a consumer group ID
2. Call `poll()` to fetch records
3. Records are returned **in offset order**
4. Call `commit()` to save progress
5. Repeat

**Guarantees**:
- ✅ Messages read in order within each partition
- ✅ No duplicates (when committing correctly)
- ✅ Can resume from last position
- ✅ Multiple consumers can share work (partition assignment)

**The data flows**:
```
Producer → Agent → S3 Segment
                      ↓
                  Consumer reads in order
                      ↓
                  Your application processes
```

---

**Next Steps**: Check out the `simple_reader` example or build your own consumer!
