# How to Inspect StreamHouse Data

This guide shows you how to see what's actually stored in your StreamHouse system.

---

## Quick Start

```bash
# 1. Generate fresh data through real pipeline
./scripts/real_pipeline_fresh.sh

# 2. Read records from any topic
cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 10

# 3. Inspect binary segment file
./scripts/inspect_segment.sh orders 0 0

# 4. Query metadata
./scripts/interactive_query.sh orders 0 50
```

---

## 1. Reading Records (High-Level)

Use the `simple_reader` example to read actual records:

```bash
# Syntax
cargo run --package streamhouse-storage --example simple_reader -- <topic> <partition> <start_offset> <count>

# Examples
cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 10
cargo run --package streamhouse-storage --example simple_reader -- user-events 0 100 20
cargo run --package streamhouse-storage --example simple_reader -- metrics 2 150 5
```

**Output:**
```
📖 Reading from orders/partition-0
   Starting offset: 0
   Count: 5

✅ Read 5 records

─────────────────────────────────────────
Record #0 (offset=0)
─────────────────────────────────────────
  Key:       order-0-0
  Timestamp: 1769183811987
  Value:     {
  "amount": 50.99,
  "customer": "user-0",
  "order_id": 0,
  "partition": 0
}
```

**What this does:**
1. Connects to metadata store (SQLite/PostgreSQL)
2. Uses Phase 3.4 BTreeMap index to find segment
3. Downloads segment from MinIO (or loads from cache)
4. Decompresses LZ4 blocks
5. Decodes binary record format
6. Returns structured data

---

## 2. Inspecting Binary Segments (Low-Level)

Use `inspect_segment.sh` to see the raw binary format:

```bash
# Syntax
./scripts/inspect_segment.sh <topic> <partition> [base_offset]

# Examples
./scripts/inspect_segment.sh orders 0 0
./scripts/inspect_segment.sh user-events 0
```

**Output:**
```
🔍 Inspecting Segment: data/orders/0/00000000000000000000.seg
==========================================

📦 File Information:
Name      : 00000000000000000000.seg
Date      : 2026-01-23 15:56:52 UTC
Size      : 1.8 KiB
ETag      : da416faafda93240ca20e6c0f16f7e94

📋 Binary Structure:

1. Header (32 bytes):
00000000  53 54 52 4d 00 01 00 01  00 00 00 00 00 00 00 00  |STRM............|
00000010  00 00 00 00 00 00 00 63  00 00 00 64 00 00 00 01  |.......c...d....|
...

   ✅ Magic: STRM (valid StreamHouse segment)
   Version: 0x0001
   Compression: None (0x00)
```

**What you see:**
- **Header**: Magic bytes, version, compression type
- **Hex dump**: Raw binary content
- **Strings**: Readable parts (JSON records if uncompressed)
- **Metadata**: What the segment should contain

---

## 3. Querying Metadata

### PostgreSQL

Check what's in the metadata store:

```bash
# Show all topics
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
  "SELECT name, partition_count FROM topics;"

# Show partitions and watermarks
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
  "SELECT topic, partition_id, high_watermark FROM partitions ORDER BY topic, partition_id;"

# Show segments
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
  "SELECT topic, partition_id, base_offset, end_offset, record_count, s3_key
   FROM segments
   ORDER BY topic, partition_id, base_offset;"
```

### Interactive Query Tool

Find which segment contains a specific offset:

```bash
./scripts/interactive_query.sh orders 0 50
```

**Output:**
```
🔍 Querying: topic=orders, partition=0, offset=50
================================================================

✅ Found segment:
   Offset range: 0 - 99
   Record count: 100
   Size: 1.77 MB
   S3 location: s3://streamhouse/data/orders/0/00000000000000000000.seg

📊 Phase 3.4 BTreeMap Lookup Simulation:
   Query: index.range(..=50).next_back()
   Result: Found segment with base_offset=0
   Complexity: O(log n) < 1µs

🪣 Verifying MinIO storage...
   ✅ File exists: 1.8 KiB, created 2026-01-23 15:56:52 UTC

📖 Read Flow:
   1. PartitionReader.read(offset=50, limit=100)
   2. SegmentIndex.find_segment_for_offset(50) → ...seg
   3. SegmentCache.get_segment(...) → Load from cache or MinIO
   4. SegmentReader.read_range(50, 150)
   5. Return 100 records (offsets 50-149)
```

---

## 4. Browsing MinIO

### Command Line

```bash
# List all buckets
docker exec streamhouse-minio mc ls local/

# List all segments
docker exec streamhouse-minio mc ls local/streamhouse/ --recursive

# Show directory tree
docker exec streamhouse-minio mc tree local/streamhouse/

# Get file details
docker exec streamhouse-minio mc stat local/streamhouse/data/orders/0/00000000000000000000.seg

# Download a segment
docker exec streamhouse-minio mc cp local/streamhouse/data/orders/0/00000000000000000000.seg /tmp/

# Check storage usage
docker exec streamhouse-minio mc du local/streamhouse/
```

### Web UI

```bash
# Open in browser
open http://localhost:9001

# Login credentials
Username: minioadmin
Password: minioadmin
```

Navigate to: **Buckets** → **streamhouse** → **data** → browse topics/partitions

---

## 5. Understanding the Data Layout

### Directory Structure

```
streamhouse/  (bucket)
└── data/
    ├── orders/
    │   ├── 0/
    │   │   └── 00000000000000000000.seg  ← 100 records (offsets 0-99)
    │   ├── 1/
    │   │   └── 00000000000000000000.seg  ← 150 records (offsets 0-149)
    │   └── 2/
    │       └── 00000000000000000000.seg  ← 80 records (offsets 0-79)
    │
    ├── user-events/
    │   ├── 0/
    │   │   └── 00000000000000000000.seg  ← 200 records (offsets 0-199)
    │   └── 1/
    │       └── 00000000000000000000.seg  ← 120 records (offsets 0-119)
    │
    └── metrics/
        ├── 0/
        │   └── 00000000000000000000.seg  ← 250 records (offsets 0-249)
        ├── 1/
        │   └── 00000000000000000000.seg  ← 250 records (offsets 0-249)
        ├── 2/
        │   └── 00000000000000000000.seg  ← 250 records (offsets 0-249)
        └── 3/
            └── 00000000000000000000.seg  ← 250 records (offsets 0-249)
```

### Segment File Format

```
┌─────────────────────────────────────────┐
│ HEADER (32 bytes)                       │
│ - Magic: "STRM"                         │
│ - Version: 1                            │
│ - Compression: 0=None, 1=LZ4            │
├─────────────────────────────────────────┤
│ BLOCK 1 (compressed if LZ4 enabled)     │
│ - Block header (12 bytes)               │
│ - Records (varint encoded, delta coded) │
├─────────────────────────────────────────┤
│ BLOCK 2                                 │
│ ...                                     │
├─────────────────────────────────────────┤
│ OFFSET INDEX                            │
│ - Entry: offset=0, position=32          │
│ - Entry: offset=100, position=1056      │
│ ...                                     │
├─────────────────────────────────────────┤
│ FOOTER                                  │
│ - Index position                        │
│ - CRC32 checksum                        │
│ - Magic bytes (verification)            │
└─────────────────────────────────────────┘
```

### Record Format

Inside blocks, records are encoded as:

```
Record:
  offset:     varint(u64)   ← Delta from previous
  timestamp:  varint(i64)   ← Delta from previous
  key_len:    varint(u32)
  key:        [u8; key_len]
  value_len:  varint(u32)
  value:      [u8; value_len]
```

**Space efficiency:**
- Offset deltas: 1-2 bytes each
- Timestamp deltas: 1-2 bytes each
- Varint encoding: 1-5 bytes depending on value
- LZ4 compression: 2-5x reduction

---

## 6. Data Examples

### Orders Topic (with keys)

```json
{
  "key": "order-0-42",
  "value": {
    "order_id": 42,
    "partition": 0,
    "customer": "user-2",
    "amount": 92.99
  }
}
```

### User Events (with keys)

```json
{
  "key": "event-0-100",
  "value": {
    "event_type": "click",
    "user_id": 0,
    "timestamp": 1769183812037
  }
}
```

### Metrics (no keys)

```json
{
  "key": null,
  "value": {
    "metric": "cpu_usage",
    "host": "host-2",
    "value": 90,
    "timestamp": 1769183812071
  }
}
```

---

## 7. SQL Queries

### Find segment for specific offset

```sql
SELECT base_offset, end_offset, record_count, s3_key
FROM segments
WHERE topic = 'orders' AND partition_id = 0
  AND base_offset <= 50 AND end_offset >= 50;
```

### Show BTreeMap index structure

```sql
SELECT
  base_offset as btree_key,
  jsonb_build_object(
    'base', base_offset,
    'end', end_offset,
    'records', record_count,
    's3_key', s3_key
  ) as segment_info
FROM segments
WHERE topic = 'orders' AND partition_id = 0
ORDER BY base_offset;
```

### List all offsets in range

```sql
WITH RECURSIVE offsets AS (
  SELECT s.base_offset as offset, s.end_offset
  FROM segments s
  WHERE topic = 'orders' AND partition_id = 0

  UNION ALL

  SELECT offset + 1, end_offset
  FROM offsets
  WHERE offset < end_offset
)
SELECT offset FROM offsets ORDER BY offset LIMIT 100;
```

---

## 8. Performance Testing

### Measure read latency

```bash
time cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 100
```

### Check cache effectiveness

```bash
# First read (cache miss)
time cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 10

# Second read (cache hit) - should be faster
time cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 10
```

### Test BTreeMap index

```bash
# Read from different offsets - all use same index
time cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 1
time cargo run --package streamhouse-storage --example simple_reader -- orders 0 50 1
time cargo run --package streamhouse-storage --example simple_reader -- orders 0 99 1
```

---

## Summary

**High-level (recommended):**
```bash
# Read records with full decoding
cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 10
```

**Low-level:**
```bash
# Inspect binary format
./scripts/inspect_segment.sh orders 0 0

# Query metadata
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
  "SELECT * FROM segments WHERE topic = 'orders';"

# Browse MinIO
open http://localhost:9001
```

**All data is REAL** - written through actual StreamHouse APIs with proper compression, indexing, and checksums!
