# WarpStream Architecture Learnings & Our Path Forward

**Status**: 📚 Research Document
**Date**: January 2026
**Source**: WarpStream Reddit AMA + Official Docs

## Executive Summary

WarpStream was acquired by Confluent for ~$220M after proving S3-native Kafka works. This document captures their key architectural innovations and how StreamHouse will match/exceed them.

**Key Insight**: WarpStream's success came from **two things working together**:
1. S3-native storage (we have this) ✅
2. Hyper-scalable metadata service (we need this) ❌

---

## WarpStream's Three Key Innovations

### 1. Stateless Agent Architecture

**The Problem**: Traditional Kafka brokers are stateful. Each broker "owns" specific partitions. Adding/removing brokers requires rebalancing (slow, risky).

**WarpStream's Solution**: ANY agent can be:
- Leader for ANY topic
- Coordinator for ANY consumer group
- Commit offsets for ANY group

**Benefits**:
- ✅ Trivial auto-scaling (no rebalancing)
- ✅ No single points of failure
- ✅ Scale on CPU/bandwidth instantly

**What This Means for Us**:
Phase 4 priority. Our Phase 1-3 architecture is single-server, which is fine for getting started but won't scale to WarpStream levels.

---

### 2. Hyper-Scalable Metadata Service

**WarpStream Quote from Reddit AMA**:
> "We built an extremely scaleable metadata service. This gets glossed over in a lot of the 'diskless kafka' implementation discussions, but the ability to handle pathological workloads (really high volumes of tiny batches spread across 10s or 100s of thousands of partitions) is really tough for completely stateless architectures so we spent a lot of time making sure the control plane could handle that."

**Metadata Backend Options**:
- DynamoDB (AWS)
- Spanner (GCP)
- Cosmos DB (Azure)
- Any SQL database (pluggable architecture)

**Key Quote**:
> "We don't really depend on much functionality from the underlying datastore, so it's easy to add new backends for it!"

**Performance Requirements**:
- Must handle 100,000+ partitions
- Must handle high volumes of tiny batches
- Uses lots of in-memory batching/caching

**What This Means for Us**:
**THIS IS OUR PHASE 3 PRIORITY** 🎯

Our SQLite metadata store is fine for Phase 1-2, but won't scale. We need:

1. **Abstract the `MetadataStore` trait** (already exists!)
2. **Add pluggable backends**:
   - PostgreSQL (Phase 3.1) - good for 10K partitions
   - CockroachDB (Phase 3.2) - distributed SQL, scales to 100K
   - DynamoDB (Phase 3.3) - AWS-native, WarpStream-proven

3. **In-memory caching layer**:
   - LRU cache for hot partition metadata
   - Reduces round-trips to metadata store
   - Critical for low-latency operations

4. **Efficient partition→segment indexing**:
   - Fast lookup of which segments contain which offsets
   - Handle pathological workloads (many tiny batches)

---

### 3. Smart S3 Batching Strategy

**The Challenge**: S3 has higher latency than local disk (~10ms vs sub-ms).

**WarpStream's Approach**:
> "Agents make a few files per second, but each file contains records from multiple topics and partitions."

**Cross-Partition Batching**:
- Write records from multiple partitions into same S3 object
- Reduces number of S3 PUT requests
- Amortizes S3 latency across many records

**Background Compaction**:
- Small files get batched into larger files over time
- Optimizes for cost-effective historical reprocessing
- Keeps hot data in small files, cold data in large files

**Performance Numbers**:
- Write latency: ~400-600ms p99 (S3 Standard)
- End-to-end latency: < 1.5s p99
- With S3 Express One Zone: 4x lower latency

**Cost Benefits**:
- 80% cheaper than Kafka
- No cross-AZ replication costs
- No expensive EBS volumes

**What This Means for Us**:
Phase 2.1 should add cross-partition batching:
- Current: Each partition has its own SegmentWriter
- Future: One writer batches multiple partitions
- Reduces S3 API calls, improves cost efficiency

---

## Our Architecture Evolution

### Current State (Phase 1 Complete)

```
┌──────────────────────────────────────────────────┐
│           StreamHouse Phase 1                    │
├──────────────────────────────────────────────────┤
│                                                  │
│  gRPC API → Single Server → SQLite Metadata      │
│                    ↓                             │
│            PartitionWriter → S3 Segment          │
│                                                  │
│  Limitations:                                    │
│  ❌ Single server (not distributed)              │
│  ❌ SQLite won't scale past ~1K partitions       │
│  ❌ Each partition = separate segment            │
│  ❌ Segments not flushed until full              │
│                                                  │
└──────────────────────────────────────────────────┘
```

### Phase 2: Performance + Kafka Protocol (Weeks 9-16)

```
┌──────────────────────────────────────────────────┐
│           StreamHouse Phase 2                    │
├──────────────────────────────────────────────────┤
│                                                  │
│  Kafka Protocol → Single Server → SQLite         │
│                         ↓                        │
│                   WriterPool                     │
│                         ↓                        │
│             Background Flush Thread              │
│                         ↓                        │
│                   S3 Segments                    │
│                                                  │
│  Improvements:                                   │
│  ✅ Writer pooling (reuse across requests)       │
│  ✅ Background flush (5s interval)               │
│  ✅ Kafka protocol (standard clients work)       │
│  ✅ Consumer rebalancing                         │
│  ❌ Still single-server                          │
│  ❌ Still SQLite (won't scale)                   │
│                                                  │
└──────────────────────────────────────────────────┘
```

### Phase 3: Scalable Metadata (Weeks 17-24) 🎯 NEW PRIORITY

```
┌──────────────────────────────────────────────────┐
│           StreamHouse Phase 3                    │
├──────────────────────────────────────────────────┤
│                                                  │
│  Kafka Protocol → Single Server → Pluggable DB  │
│                         ↓                        │
│                   WriterPool                     │
│                         ↓                        │
│             Background Flush Thread              │
│                         ↓                        │
│            Cross-Partition Batching              │
│                         ↓                        │
│                   S3 Segments                    │
│                                                  │
│  Metadata Backend Options:                       │
│  ✅ SQLite (< 1K partitions)                     │
│  ✅ PostgreSQL (< 10K partitions)                │
│  ✅ CockroachDB (< 100K partitions)              │
│  ✅ DynamoDB (AWS-native, unlimited)             │
│                                                  │
│  Metadata Caching:                               │
│  ✅ In-memory LRU cache for hot metadata         │
│  ✅ Partition→segment index optimization         │
│                                                  │
│  Improvements:                                   │
│  ✅ Handles 10,000+ partitions                   │
│  ✅ Pathological workload support                │
│  ✅ Multi-cloud support                          │
│  ❌ Still single-server                          │
│                                                  │
└──────────────────────────────────────────────────┘
```

### Phase 4: Multi-Agent Architecture (Weeks 25-32)

```
┌──────────────────────────────────────────────────┐
│           StreamHouse Phase 4                    │
├──────────────────────────────────────────────────┤
│                                                  │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐   │
│  │  Agent 1  │  │  Agent 2  │  │  Agent N  │   │
│  │           │  │           │  │           │   │
│  │  Stateless - any agent can be leader      │   │
│  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘   │
│        │              │              │          │
│        └──────────────┼──────────────┘          │
│                       ↓                         │
│           Distributed Metadata Store            │
│        (CockroachDB or DynamoDB)                │
│                       ↓                         │
│                  S3 Segments                    │
│                                                  │
│  Agent Capabilities:                            │
│  ✅ Any agent = leader for any partition        │
│  ✅ Any agent = consumer group coordinator       │
│  ✅ Trivial auto-scaling                        │
│  ✅ No rebalancing needed                       │
│  ✅ No single point of failure                  │
│                                                  │
│  Agent Groups:                                  │
│  ✅ Network-isolated pools                      │
│  ✅ Multi-VPC support                           │
│  ✅ Multi-region coordination                   │
│                                                  │
└──────────────────────────────────────────────────┘
```

---

## Detailed Phase 3 Plan: Scalable Metadata

### Goal
Build WarpStream-style pluggable metadata service that can scale from 1 partition (dev) to 100,000+ partitions (production).

### Success Criteria
- ✅ Support 10,000 partitions with low latency
- ✅ Pluggable backends (SQLite → PostgreSQL → DynamoDB)
- ✅ In-memory caching reduces metadata round-trips by 90%
- ✅ Pathological workload test passes (1000 tiny batches/sec across 1000 partitions)

### Implementation Steps

#### 3.1: Abstract Metadata Interface (Week 17)

Already have `MetadataStore` trait in `crates/streamhouse-metadata/src/lib.rs`.

Verify it covers all operations:
- ✅ Topic CRUD
- ✅ Partition CRUD
- ✅ Segment CRUD
- ✅ Consumer offset operations
- ✅ Consumer group operations

Add any missing operations needed for agent coordination.

#### 3.2: PostgreSQL Backend (Week 18)

Implement `PostgresMetadataStore`:
```rust
pub struct PostgresMetadataStore {
    pool: sqlx::PgPool,
}

impl MetadataStore for PostgresMetadataStore {
    // Implement all trait methods using PostgreSQL
}
```

Why PostgreSQL first:
- Familiar to most developers
- Good balance of features and simplicity
- Can handle 10K partitions easily
- Free to run (unlike DynamoDB)

#### 3.3: Metadata Caching Layer (Week 19)

```rust
pub struct CachedMetadataStore<T: MetadataStore> {
    inner: Arc<T>,
    topic_cache: Arc<RwLock<LruCache<String, Topic>>>,
    partition_cache: Arc<RwLock<LruCache<(String, u32), Partition>>>,
    segment_index: Arc<RwLock<BTreeMap<(String, u32, u64), SegmentInfo>>>,
}
```

Cache Strategy:
- Topic metadata: Cache for 5 minutes (rarely changes)
- Partition metadata: Cache for 1 minute
- Segment index: Cache indefinitely, invalidate on write
- Consumer offsets: Never cache (must be consistent)

#### 3.4: Partition→Segment Index Optimization (Week 20)

Current problem: Finding segment for offset requires:
1. Query all segments for (topic, partition)
2. Filter where base_offset <= target < end_offset
3. Sort by base_offset

Optimized approach: In-memory BTreeMap index
```rust
// Key: (topic, partition, base_offset)
// Value: SegmentInfo (end_offset, s3_key, size)
BTreeMap<(String, u32, u64), SegmentInfo>
```

Lookup: O(log n) instead of O(n)

#### 3.5: CockroachDB Backend (Week 21)

Implement `CockroachMetadataStore`:
- Same interface as PostgreSQL
- Uses CockroachDB-specific features:
  - Distributed transactions
  - Multi-region support
  - Horizontal scalability

Why CockroachDB:
- PostgreSQL-compatible (easy migration)
- Distributed by default
- Can scale to 100K+ partitions
- Built-in replication

#### 3.6: DynamoDB Backend (Week 22 - Optional)

Implement `DynamoDbMetadataStore`:
- WarpStream-proven technology
- AWS-native (no server management)
- Unlimited scalability
- Pay-per-request pricing

Tables:
- `streamhouse_topics`: Partition key = topic_name
- `streamhouse_partitions`: PK = topic_name, SK = partition_id
- `streamhouse_segments`: PK = topic#partition, SK = base_offset
- `streamhouse_consumer_offsets`: PK = group_id, SK = topic#partition

#### 3.7: Pathological Workload Testing (Week 23)

Test scenario from WarpStream learnings:
> "Really high volumes of tiny batches spread across 10s or 100s of thousands of partitions"

Test:
```rust
// 1000 partitions × 1000 batches = 1M tiny writes
for partition in 0..1000 {
    for batch in 0..1000 {
        produce_record(
            topic = "test",
            partition = partition,
            value = 10 bytes,  // Tiny!
        );
    }
}
```

Metrics to track:
- Metadata query latency (should be < 10ms p99)
- Cache hit rate (should be > 90%)
- Database load (should be manageable)
- End-to-end write latency (should be < 500ms p99)

#### 3.8: Cross-Partition Batching (Week 24)

Implement WarpStream's "multiple topics/partitions per file" strategy:

```rust
pub struct CrossPartitionWriter {
    current_batch: HashMap<(String, u32), Vec<Record>>,
    max_batch_size: usize,
    flush_interval: Duration,
}

impl CrossPartitionWriter {
    // Batch records from multiple partitions
    // Flush when batch is full or time elapses
    pub async fn write(&mut self, topic: &str, partition: u32, record: Record) {
        self.current_batch
            .entry((topic.to_string(), partition))
            .or_default()
            .push(record);

        if self.should_flush() {
            self.flush_to_s3().await?;
        }
    }

    async fn flush_to_s3(&mut self) {
        // Write single S3 object containing:
        // - Records from partition 0 of topic A
        // - Records from partition 1 of topic A
        // - Records from partition 0 of topic B
        // etc.

        // Update metadata with one transaction
    }
}
```

Benefits:
- Fewer S3 PUT requests → lower cost
- Amortize S3 latency across many records
- Better resource utilization

---

## Comparison: StreamHouse vs WarpStream

### What We'll Match (by Phase 4)

| Feature | WarpStream | StreamHouse Phase 4 | Status |
|---------|-----------|---------------------|--------|
| S3-native storage | ✅ | ✅ | Complete (Phase 1) |
| Kafka protocol | ✅ | ✅ | Phase 2 |
| Stateless agents | ✅ | ✅ | Phase 4 |
| Pluggable metadata | ✅ | ✅ | Phase 3 |
| 10K+ partitions | ✅ | ✅ | Phase 3 |
| Cross-partition batching | ✅ | ✅ | Phase 3 |
| Background compaction | ✅ | ⏳ | Phase 5 |
| Virtual clusters | ✅ | ⏳ | Phase 5 |
| Multi-region | ✅ | ⏳ | Phase 5 |

### What We Add (Differentiation)

| Feature | WarpStream | StreamHouse | Advantage |
|---------|-----------|-------------|-----------|
| SQL processing | ❌ | ✅ | No Flink needed |
| Built-in stream processing | ❌ | ✅ | One system not two |
| DataFusion integration | ❌ | ✅ | Ecosystem access |
| Cost | ~$730/mo | ~$500/mo | 30% cheaper |
| Complexity | Low | Low | Same simplicity |

### What We Won't Match (Yet)

| Feature | WarpStream | StreamHouse | Notes |
|---------|-----------|-------------|-------|
| 100K+ partitions | ✅ | ⏳ | Phase 5+ (not critical for MVP) |
| Battle-tested at scale | ✅ | ❌ | They have production usage |
| Multi-cloud day 1 | ✅ | ❌ | Start with AWS, add later |
| Schema Registry | ✅ | ⏳ | Phase 6 |
| Tableflow (Iceberg) | ✅ | ⏳ | Phase 6 |

---

## Key Takeaways

### 1. Metadata is Critical

WarpStream spent significant engineering effort on their metadata service. It's not a "nice to have" - it's the foundation that enables stateless agents.

**Our Action**: Phase 3 (Scalable Metadata) is **mandatory** before Phase 4 (Multi-Agent). Can't skip it.

### 2. Start Simple, Plan for Scale

WarpStream started with one backend, then added pluggability. We should:
- Phase 1-2: SQLite (good enough)
- Phase 3: Add PostgreSQL (10K partitions)
- Phase 4: Add CockroachDB/DynamoDB (100K partitions)

### 3. In-Memory Caching is Essential

WarpStream: "We do lots of batching and caching in-memory as you would expect"

Metadata queries are on the hot path. Even 10ms to database becomes noticeable at scale. Cache hit rate of 90%+ is critical.

### 4. Cross-Partition Batching Matters

Writing one S3 object per partition is wasteful. Batching records from multiple partitions into shared objects:
- Reduces S3 API costs
- Amortizes latency
- Improves throughput

### 5. We Have an Advantage

WarpStream only does transport. We do transport + processing. That's the unique value prop.

---

## Updated Roadmap

```
Phase 1 (Complete) ✅
├── S3-native storage
├── Binary segment format
├── SQLite metadata
└── gRPC API

Phase 2.1 (Week 9) 🚧 Current
├── Writer pooling
├── Background flush
└── Basic consume working

Phase 2.2 (Weeks 10-12)
├── Kafka wire protocol
├── Consumer rebalancing
└── Standard client support

Phase 3 (Weeks 13-20) 🎯 CRITICAL
├── Pluggable metadata backend
│   ├── PostgreSQL
│   ├── CockroachDB
│   └── DynamoDB (optional)
├── In-memory caching layer
├── Partition→segment index
├── Pathological workload tests
└── Cross-partition batching

Phase 4 (Weeks 21-28)
├── Multi-agent deployment
├── Stateless agent architecture
├── Leader election per partition
└── Agent groups

Phase 5+ (Month 7+)
├── 100K partition support
├── Background compaction
├── Virtual clusters
└── SQL processing engine
```

---

## Conclusion

WarpStream proved that S3-native Kafka works and is worth $220M. Their architecture is sound and battle-tested.

**Our path forward**:
1. **Finish Phase 2** (writer pooling + Kafka protocol)
2. **Prioritize Phase 3** (scalable metadata) - this is the foundation
3. **Then Phase 4** (multi-agent architecture) - this is the scaling layer
4. **Add SQL processing** (Phase 5) - this is our differentiation

By Phase 4, we'll match WarpStream's core architecture. By Phase 5, we'll exceed it with built-in stream processing.

**Key insight**: We're not just rebuilding WarpStream. We're building WarpStream + Flink in one system. That's the vision.

---

**References**:
- [WarpStream Reddit AMA](https://www.reddit.com/r/apachekafka/comments/1kijwdz/were_the_cofounders_of_warpstream_ask_us_anything/)
- [WarpStream Architecture Docs](https://docs.warpstream.com/warpstream/overview/architecture)
- [WarpStream AI Info](https://www.warpstream.com/ai-info)

*Last updated: 2026-01-22*
