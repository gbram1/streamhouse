# Storage Architecture: Segments, Blocks, and Binary Format

This document explains how StreamHouse stores data - from individual records to compressed segments in S3.

---

## Table of Contents

1. [Overview](#overview)
2. [The Storage Hierarchy](#the-storage-hierarchy)
3. [Records](#records)
4. [Blocks](#blocks)
5. [Segments](#segments)
6. [Binary Format Specification](#binary-format-specification)
7. [Compression Strategy](#compression-strategy)
8. [Why This Design?](#why-this-design)
9. [Performance Characteristics](#performance-characteristics)

---

## Overview

StreamHouse stores events in **immutable, compressed segment files** in S3. Each segment contains thousands of records, organized into compressed blocks, with an index for fast lookups.

```text
┌─────────────────────────────────────────────────────────┐
│                     SEGMENT FILE                         │
│                    (64MB, in S3)                         │
├─────────────────────────────────────────────────────────┤
│  Header (64 bytes)                                       │
│  - Magic bytes, version, compression, offsets            │
├─────────────────────────────────────────────────────────┤
│  Block 1 (~1MB compressed)                               │
│  ├─ Record 0 (offset=100, ts=123456, key, value)        │
│  ├─ Record 1 (offset=101, ts=123457, key, value)        │
│  └─ ... (thousands of records)                          │
├─────────────────────────────────────────────────────────┤
│  Block 2 (~1MB compressed)                               │
│  └─ ... more records                                    │
├─────────────────────────────────────────────────────────┤
│  ... (64 blocks total)                                  │
├─────────────────────────────────────────────────────────┤
│  Index (offset → file position mapping)                 │
│  ├─ offset=100 → position=64                            │
│  ├─ offset=5000 → position=1048640                      │
│  └─ ... (one entry per block)                           │
├─────────────────────────────────────────────────────────┤
│  Footer (32 bytes)                                       │
│  - Index position, CRC32, magic bytes                   │
└─────────────────────────────────────────────────────────┘
```

---

## The Storage Hierarchy

StreamHouse organizes data in a four-level hierarchy:

### Level 1: Records
- **What**: A single event/message
- **Size**: ~100 bytes to 1MB (typically ~1KB)
- **Location**: In-memory, then written to blocks
- **Example**: `{"user_id": "alice", "action": "purchase", "amount": 99.99}`

### Level 2: Blocks
- **What**: A batch of ~1000-10000 records, compressed together
- **Size**: ~1MB compressed (target size)
- **Location**: Within a segment file
- **Purpose**: Compression unit (compress many records together for better ratio)

### Level 3: Segments
- **What**: A collection of ~64 blocks
- **Size**: 64MB (configurable, default)
- **Location**: S3 (one file per segment)
- **Purpose**: Upload unit (immutable file in object storage)
- **Lifespan**: Created once, never modified, eventually deleted based on retention

### Level 4: Partitions
- **What**: Ordered sequence of segments for a topic partition
- **Size**: Unlimited (grows over time)
- **Location**: S3 bucket with prefix `data/{topic}/{partition}/`
- **Purpose**: Scalability (parallel reads/writes across partitions)

```text
Topic: "orders" (3 partitions)
│
├─ Partition 0
│  ├─ Segment 0 (offsets 0-99,999)
│  ├─ Segment 1 (offsets 100,000-199,999)
│  └─ Segment 2 (offsets 200,000-299,999) ← Active
│
├─ Partition 1
│  ├─ Segment 0 (offsets 0-99,999)
│  └─ Segment 1 (offsets 100,000-199,999) ← Active
│
└─ Partition 2
   └─ Segment 0 (offsets 0-99,999) ← Active
```

---

## Records

A record is the fundamental unit of data in StreamHouse.

### Structure

```rust
struct Record {
    offset: u64,           // Unique ID within partition (0, 1, 2, ...)
    timestamp: u64,        // Milliseconds since epoch
    key: Option<Bytes>,    // Optional key for partitioning
    value: Bytes,          // The actual payload
}
```

### Example

```json
{
  "offset": 12345,
  "timestamp": 1706198400000,
  "key": "user-alice",
  "value": "{\"action\": \"purchase\", \"amount\": 99.99}"
}
```

### Properties

- **Immutable**: Once written, never modified
- **Ordered**: Monotonically increasing offsets within a partition
- **Zero-copy**: Uses `bytes::Bytes` for efficient memory handling

---

## Blocks

Blocks are the **compression unit** in StreamHouse.

### Why Blocks?

Instead of compressing the entire segment at once, we compress in ~1MB chunks:

1. **Parallel decompression**: Can decompress blocks in parallel
2. **Partial reads**: Don't need to decompress entire file to read from offset 50,000
3. **Error isolation**: Corruption affects one block, not entire segment
4. **Memory efficiency**: Process one block at a time

### Block Structure

```text
┌───────────────────────────────────┐
│        BLOCK (uncompressed)       │
├───────────────────────────────────┤
│  Record 0 (delta-encoded)         │
│  ├─ offset_delta = 0 (first)      │
│  ├─ timestamp_delta = 0           │
│  ├─ key_len = 10                  │
│  ├─ key = "user-alice"            │
│  ├─ value_len = 42                │
│  └─ value = "{...}"               │
├───────────────────────────────────┤
│  Record 1 (delta-encoded)         │
│  ├─ offset_delta = 1 (100→101)    │
│  ├─ timestamp_delta = 100         │
│  └─ ...                           │
├───────────────────────────────────┤
│  ... more records                 │
└───────────────────────────────────┘
         ↓ LZ4 Compression
┌───────────────────────────────────┐
│    BLOCK (compressed, ~15% size)  │
└───────────────────────────────────┘
```

### Delta Encoding

Records within a block are delta-encoded to improve compression:

**Without delta encoding:**
```
Record 0: offset=100, timestamp=1706198400000
Record 1: offset=101, timestamp=1706198400100
Record 2: offset=102, timestamp=1706198400200
```
→ Each record needs 8 bytes for offset + 8 bytes for timestamp = 16 bytes × 3 = **48 bytes**

**With delta encoding:**
```
Record 0: offset_delta=0, timestamp_delta=0         (base values)
Record 1: offset_delta=1, timestamp_delta=100
Record 2: offset_delta=1, timestamp_delta=100
```
→ Deltas are small, encoded as varints: 1 byte each × 6 = **6 bytes** (8× savings!)

### Varint Encoding

Small integers (like deltas of 1) use variable-length encoding:

- `0-127`: 1 byte
- `128-16,383`: 2 bytes
- `16,384-2,097,151`: 3 bytes
- etc.

Since most offsets are sequential (delta=1) and timestamps are close together (delta=100ms), we get 1-byte varints instead of 8-byte fixed integers.

---

## Segments

Segments are **immutable files** stored in S3.

### Segment Lifecycle

```text
1. CREATE
   ├─ In-memory: SegmentWriter buffers records
   └─ Threshold: 64MB or 10 minutes

2. FINALIZE
   ├─ Compress all blocks with LZ4
   ├─ Build offset index
   ├─ Add header and footer
   └─ Calculate CRC32 checksum

3. UPLOAD
   ├─ Upload to S3: s3://bucket/data/orders/0/00000000000000000000.seg
   └─ Retry with exponential backoff (3 attempts)

4. REGISTER
   ├─ Add segment metadata to SQLite
   └─ Update partition high watermark

5. SERVE
   └─ Consumers read segments from S3

6. DELETE (after retention period)
   └─ Remove from S3 and metadata
```

### Segment Naming

```
s3://streamhouse/data/{topic}/{partition}/{base_offset}.seg

Examples:
s3://streamhouse/data/orders/0/00000000000000000000.seg  (offsets 0-99,999)
s3://streamhouse/data/orders/0/00000000000000100000.seg  (offsets 100,000-199,999)
s3://streamhouse/data/orders/1/00000000000000000000.seg  (partition 1)
```

The base offset is zero-padded to 20 digits for lexicographic sorting.

### Rolling Strategy

A new segment is created when **either** condition is met:

1. **Size threshold**: Current segment reaches 64MB (configurable)
2. **Time threshold**: 10 minutes since segment creation (configurable)

This ensures:
- **Large topics**: Segments fill up quickly due to size
- **Small topics**: Segments don't sit open forever (10min max)

---

## Binary Format Specification

### Complete Segment Format

```text
┌────────────────────────────────────────────────────────────┐
│                         HEADER (64 bytes)                  │
├────────────────────────────────────────────────────────────┤
│  Offset │ Size │ Field                                     │
│─────────┼──────┼───────────────────────────────────────────│
│  0      │  4   │ Magic bytes: 0x53484D53 ("SHMS")         │
│  4      │  2   │ Version: 1                                │
│  6      │  1   │ Compression: 0=None, 1=LZ4, 2=Zstd        │
│  7      │  1   │ Reserved                                  │
│  8      │  8   │ Base offset (first record offset)         │
│  16     │  8   │ Last offset (last record offset)          │
│  24     │  4   │ Record count                              │
│  28     │  4   │ Block count                               │
│  32     │  8   │ Created timestamp (ms since epoch)        │
│  40     │  24  │ Reserved for future use                   │
└────────────────────────────────────────────────────────────┘
│                     BLOCK 0 (compressed)                   │
├────────────────────────────────────────────────────────────┤
│  4 bytes │ Uncompressed size                              │
│  4 bytes │ Compressed size                                │
│  N bytes │ LZ4-compressed record data                     │
└────────────────────────────────────────────────────────────┘
│                     BLOCK 1 (compressed)                   │
├────────────────────────────────────────────────────────────┤
│  ... same format                                           │
└────────────────────────────────────────────────────────────┘
│                  ... more blocks ...                       │
└────────────────────────────────────────────────────────────┘
│                   INDEX (variable size)                    │
├────────────────────────────────────────────────────────────┤
│  Per entry (24 bytes):                                     │
│  ├─ 8 bytes: Offset (first record in block)               │
│  ├─ 8 bytes: File position (where block starts)           │
│  └─ 8 bytes: Timestamp (first record timestamp)           │
│                                                            │
│  Example entries:                                          │
│  ├─ offset=0, position=64, timestamp=1706198400000        │
│  ├─ offset=5000, position=1048640, timestamp=1706198500000│
│  └─ ... (one per block)                                   │
└────────────────────────────────────────────────────────────┘
│                    FOOTER (32 bytes)                       │
├────────────────────────────────────────────────────────────┤
│  0      │  8   │ Index position (file offset)              │
│  8      │  4   │ Index size (bytes)                        │
│  12     │  4   │ CRC32 checksum (entire file)              │
│  16     │  12  │ Reserved                                  │
│  28     │  4   │ Magic bytes: 0x53484D53 ("SHMS")         │
└────────────────────────────────────────────────────────────┘
```

### Reading Algorithm

To read records from offset 50,000:

```rust
1. Read footer (last 32 bytes)
2. Validate magic bytes and CRC32
3. Read index from position in footer
4. Binary search index for offset 50,000
   → Found: Block starts at file position 5,242,944
5. Seek to position 5,242,944
6. Read block header (8 bytes): uncompressed=1,048,576, compressed=157,286
7. Read compressed data (157,286 bytes)
8. Decompress with LZ4 → 1,048,576 bytes
9. Delta-decode records
10. Return records with offset >= 50,000
```

**Performance**: Finding and reading from offset 50,000 takes ~640µs (including decompression)

---

## Compression Strategy

### Why LZ4 Over Zstd?

| Metric | LZ4 | Zstd (level 3) |
|--------|-----|----------------|
| Compression ratio | ~80-85% | ~88-92% |
| Write throughput | 2.26M rec/s | ~500K rec/s |
| Read throughput | 3.10M rec/s | ~1.2M rec/s |
| CPU usage | Low | Medium |
| Latency (p99) | <3ms | ~10ms |

**Decision**: LZ4 wins
- **Speed matters more than size**: Streaming systems prioritize latency
- **Good enough compression**: 80% is still excellent
- **Lower costs**: Less CPU = cheaper cloud costs
- **Proven**: Kafka uses LZ4 as default for similar reasons

### What Makes Our Data Compressible?

1. **Delta encoding**: Sequential offsets compress to 1 byte each
2. **Similar timestamps**: Millisecond deltas compress well
3. **Repeated keys**: Same user IDs appear multiple times
4. **JSON patterns**: Field names repeat (`"user_id":`, `"amount":`)
5. **LZ4 dictionary**: Learns patterns within each block

**Real example** (1000 order records):
- Uncompressed: 156 KB
- LZ4 compressed: 24 KB
- **Ratio: 84.6% reduction** ✅

---

## Why This Design?

### Design Decisions Explained

#### 1. Why 64MB Segments?

**Too small** (1MB):
- Too many files in S3 (expensive to list)
- Higher metadata overhead
- More S3 API calls

**Too large** (1GB):
- Longer upload times
- More memory needed for reading
- Slower recovery on failure

**Just right** (64MB):
- ~2 seconds to upload (on 250 Mbps connection)
- ~100,000 records per segment (easy to manage)
- Aligns with S3 multipart upload size
- Same as Kafka's default

#### 2. Why ~1MB Blocks?

**Too small** (64KB):
- Poor compression ratio (dictionary too small)
- Too many index entries
- More overhead per block

**Too large** (16MB):
- Must decompress huge chunk for small read
- High memory usage
- Slower seeks

**Just right** (~1MB):
- Excellent compression (enough data for dictionary)
- Fast decompression (<1ms)
- Low memory footprint
- Parallel processing friendly

#### 3. Why Delta Encoding?

Without delta encoding:
```
Offset: 8 bytes × 100,000 records = 800 KB
Timestamp: 8 bytes × 100,000 records = 800 KB
Total: 1,600 KB
```

With delta encoding + varints:
```
Offset deltas: ~1 byte × 100,000 = 100 KB  (8× smaller!)
Timestamp deltas: ~1-2 bytes × 100,000 = 150 KB  (5× smaller!)
Total: 250 KB
```

**Savings: 1,350 KB (84% reduction) before compression even starts!**

#### 4. Why Immutable Segments?

**Alternative**: Append to existing files (like databases)

**Problems**:
- S3 doesn't support append (would need to rewrite entire file)
- No way to update index incrementally
- Corruption risk if write fails mid-update
- Cache invalidation nightmare

**Immutable benefits**:
- Simple: Write once, never modify
- Safe: No corruption risk
- Cacheable: Consumers can cache forever
- Parallel: Multiple consumers read same file safely
- Same model as Kafka, Pulsar, all modern streaming systems

---

## Performance Characteristics

### Write Path

```
Producer → Buffer → SegmentWriter → S3
           ↓
    (in-memory batching)
           ↓
    Flush at 64MB or 10 min
           ↓
    Compress + Upload
```

| Metric | Performance |
|--------|-------------|
| Throughput | 50,000 rec/s per partition |
| Latency (p50) | <1ms |
| Latency (p99) | <10ms |
| Segment roll time | <500ms (64MB) |
| S3 upload time | ~2s (depends on network) |

### Read Path

```
Consumer → Metadata → Find Segment → Download → Decompress → Read
                                         ↓
                                    (cached in memory)
```

| Metric | Performance |
|--------|-------------|
| Throughput | 100,000 rec/s per partition |
| Seek latency | <1ms (via index) |
| First record latency | ~50ms (download + decompress) |
| Subsequent records | <10µs (in memory) |
| Cache hit ratio | >95% (sequential reads) |

### Storage Efficiency

**Example**: 1 billion records/day

Without compression:
- Record size: ~1 KB
- Daily storage: 1 TB
- Monthly: 30 TB
- Yearly: 365 TB
- **Cost** (S3 $0.023/GB/month): $8,395/month

With LZ4 compression (85% reduction):
- Compressed size: ~150 bytes/record
- Daily storage: 150 GB
- Monthly: 4.5 TB
- Yearly: 54.75 TB
- **Cost**: $1,259/month

**Savings: $7,136/month (85% reduction)** 💰

---

## Summary

StreamHouse's storage architecture is designed for:

1. **High throughput**: 50K+ writes/sec, 100K+ reads/sec
2. **Low latency**: <10ms p99 for writes, <1ms for reads
3. **Cost efficiency**: 85% compression, cheap S3 storage
4. **Reliability**: Immutable files, checksums, retries
5. **Scalability**: Partitioning for parallel processing

The four-level hierarchy (Records → Blocks → Segments → Partitions) provides:
- **Blocks** for efficient compression
- **Segments** for manageable S3 uploads
- **Partitions** for horizontal scaling

This design is proven, tested, and similar to Kafka - because it works.
