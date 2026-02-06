export interface BlogPost {
  slug: string;
  title: string;
  excerpt: string;
  content: string;
  author: {
    name: string;
    avatar: string;
    role: string;
  };
  publishedAt: string;
  readingTime: string;
  category: string;
  tags: string[];
  featured?: boolean;
}

export const blogPosts: BlogPost[] = [
  {
    slug: "phase-7-release",
    title: "StreamHouse Phase 7: SQL Stream Processing and Beyond",
    excerpt:
      "Announcing Phase 7 with built-in SQL stream processing, enhanced observability, and 40% performance improvements. Transform your streaming data with familiar SQL syntax.",
    content: `
## Introducing Phase 7

We're thrilled to announce StreamHouse Phase 7, our most significant release yet. This update brings SQL stream processing directly into the core platform, eliminating the need for separate processing clusters like Flink or Spark Streaming.

### SQL Stream Processing

Write familiar SQL to transform your streaming data in real-time:

\`\`\`sql
CREATE STREAM user_events_enriched AS
SELECT
  u.user_id,
  u.event_type,
  p.plan_name,
  p.mrr
FROM user_events u
JOIN profiles p ON u.user_id = p.user_id
WHERE u.event_type IN ('purchase', 'upgrade')
\`\`\`

The SQL engine runs directly within StreamHouse agents, processing events with sub-millisecond latency. No external dependencies, no additional infrastructure to manage.

### Performance Improvements

Phase 7 delivers substantial performance gains across the board:

- **40% faster writes** through optimized segment buffering
- **60% reduction in S3 API calls** via intelligent batching
- **25% lower memory usage** with improved LRU cache eviction

These improvements translate directly to cost savings—our benchmark tests show an average 35% reduction in infrastructure costs for high-throughput workloads.

### Enhanced Observability

New Grafana dashboards provide unprecedented visibility into your streaming infrastructure:

- Real-time throughput and latency metrics
- Consumer lag visualization with alerting
- S3 operation breakdowns
- Agent health and resource utilization

### Breaking Changes

Phase 7 maintains full backward compatibility with existing topics and consumers. However, if you're using the experimental SQL features from Phase 6, please review the migration guide.

### Getting Started

Upgrade to Phase 7 today:

\`\`\`bash
cargo install streamhouse --version 0.7.0
\`\`\`

Or pull the latest Docker image:

\`\`\`bash
docker pull streamhouse/agent:0.7.0
\`\`\`

Check out the [documentation](/docs) for detailed upgrade instructions and new feature guides.

---

We're incredibly grateful to our community for their feedback and contributions. Phase 7 wouldn't be possible without you. Join us on [Discord](https://discord.gg/streamhouse) to share your thoughts and get help from the team.
    `,
    author: {
      name: "Alex Chen",
      avatar: "AC",
      role: "Lead Engineer",
    },
    publishedAt: "2024-01-15",
    readingTime: "5 min read",
    category: "Release",
    tags: ["release", "sql", "performance", "observability"],
    featured: true,
  },
  {
    slug: "why-s3-native-streaming",
    title: "Why We Built an S3-Native Streaming Platform",
    excerpt:
      "Traditional message brokers weren't designed for the cloud. Here's why we rethought streaming from first principles with object storage at the core.",
    content: `
## The Problem with Traditional Brokers

When we started building StreamHouse, we asked ourselves a simple question: if you were designing a streaming platform today, with cloud-native infrastructure available, would you still use local disks?

The answer was clearly no.

### The Hidden Costs of Disk-Based Streaming

Traditional brokers like Kafka were designed in an era when local SSDs were the fastest storage option. But this architecture carries significant hidden costs in cloud environments:

**1. Replication Overhead**

Kafka replicates data 3x across brokers for durability. In AWS, this means:
- 3x the EBS storage costs
- Significant inter-AZ network traffic (expensive)
- Complex leader election and ISR management

**2. Capacity Planning Nightmares**

With disk-attached brokers, you must provision for peak load:
- Over-provision to handle traffic spikes
- Under-utilize most of the time
- Manual rebalancing when adding capacity

**3. Operational Complexity**

Disk-based brokers require constant attention:
- Monitor disk usage and IOPS
- Plan partition migrations
- Handle broker failures and recovery

### The S3 Insight

Amazon S3 changed everything. For $0.023/GB/month, you get:
- 11 nines of durability (99.999999999%)
- Unlimited capacity
- No replication management
- Pay only for what you store

The question became: can we build a streaming platform that leverages S3 as the source of truth?

### How StreamHouse Works

StreamHouse uses a disaggregated architecture:

\`\`\`
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Producers  │────▶│   Agents    │────▶│     S3      │
└─────────────┘     └─────────────┘     └─────────────┘
                          │
                          ▼
                    ┌─────────────┐
                    │  Metadata   │
                    │  (Postgres) │
                    └─────────────┘
\`\`\`

**Agents** are stateless compute nodes that:
- Buffer incoming events in memory
- Flush segments to S3 periodically
- Serve reads from cache or S3

**Metadata Store** (PostgreSQL) tracks only:
- Topic and partition configurations
- Consumer group offsets
- Agent leases and watermarks

No event data touches the metadata store. This keeps it lightweight and fast.

### The Results

StreamHouse delivers:
- **80% lower costs** vs. traditional Kafka deployments
- **Zero disk management** - no provisioning, no monitoring
- **Instant scaling** - add agents without data migration
- **Built-in durability** - S3's 11 nines out of the box

### Trade-offs

S3-native streaming isn't without trade-offs:

**Latency**: S3 writes add 50-100ms latency vs. local disk. For most use cases, this is acceptable. For ultra-low-latency requirements, hybrid architectures work well.

**S3 API costs**: High-throughput workloads can incur S3 API charges. StreamHouse mitigates this with intelligent batching and caching.

### Conclusion

The cloud changed the economics of storage. StreamHouse embraces this reality by building on S3 from day one. The result is a simpler, cheaper, and more reliable streaming platform.

Try it yourself—[get started free](/console).
    `,
    author: {
      name: "Sarah Kim",
      avatar: "SK",
      role: "Co-founder & CTO",
    },
    publishedAt: "2024-01-08",
    readingTime: "7 min read",
    category: "Architecture",
    tags: ["architecture", "s3", "cloud-native", "design"],
  },
  {
    slug: "migrating-from-kafka",
    title: "Migrating from Kafka to StreamHouse: A Step-by-Step Guide",
    excerpt:
      "A practical guide to migrating your existing Kafka workloads to StreamHouse with zero downtime and minimal code changes.",
    content: `
## Overview

Migrating from Kafka to StreamHouse is straightforward thanks to our Kafka-compatible protocol. This guide walks through the process step by step.

### Prerequisites

Before starting, ensure you have:
- StreamHouse 0.6+ deployed
- Access to your Kafka cluster
- Producer/consumer applications ready to update

### Step 1: Deploy StreamHouse

Start with a minimal StreamHouse deployment:

\`\`\`bash
# Start infrastructure
docker compose up -d minio postgres

# Run an agent
cargo run --bin agent
\`\`\`

### Step 2: Create Topics

Mirror your Kafka topic configuration:

\`\`\`bash
streamctl topics create orders --partitions 8
streamctl topics create events --partitions 16
\`\`\`

### Step 3: Set Up Dual-Write

Update producers to write to both Kafka and StreamHouse:

\`\`\`rust
// Pseudo-code for dual-write
async fn produce(event: Event) {
    // Write to Kafka (existing)
    kafka_producer.send(event.clone()).await?;

    // Write to StreamHouse (new)
    streamhouse_producer.send(event).await?;
}
\`\`\`

Monitor both systems to ensure data consistency.

### Step 4: Migrate Consumers

Once dual-write is stable, migrate consumers one at a time:

\`\`\`rust
// Update connection string
let consumer = StreamHouseConsumer::new(
    "streamhouse://agent:9090",
    "orders",
    "order-processor"
).await?;
\`\`\`

### Step 5: Disable Kafka Writes

After all consumers are migrated, remove Kafka from producers:

\`\`\`rust
async fn produce(event: Event) {
    streamhouse_producer.send(event).await?;
}
\`\`\`

### Step 6: Decommission Kafka

Once confident in StreamHouse:
1. Stop Kafka consumers
2. Verify no active producers
3. Backup Kafka data if needed
4. Shut down Kafka cluster

### Common Issues

**Consumer Lag After Migration**

If you see consumer lag after migration, it's likely due to offset differences. Reset offsets to earliest:

\`\`\`bash
streamctl consumer-groups reset order-processor --topic orders --to-earliest
\`\`\`

**Missing Messages**

Enable dual-read temporarily to verify data consistency:

\`\`\`rust
let kafka_msg = kafka_consumer.poll().await?;
let sh_msg = streamhouse_consumer.poll().await?;
assert_eq!(kafka_msg.payload, sh_msg.payload);
\`\`\`

### Performance Tuning

StreamHouse defaults work well for most workloads, but you may want to tune:

\`\`\`toml
[agent]
segment_flush_interval = "5s"  # Increase for better batching
cache_size_mb = 512            # Increase for read-heavy workloads
\`\`\`

### Conclusion

Migration from Kafka to StreamHouse can be completed in a few hours for simple deployments, or a few days for complex production systems. The key is the dual-write phase—take your time to verify data consistency before cutting over.

Questions? Join our [Discord](https://discord.gg/streamhouse) for help.
    `,
    author: {
      name: "Marcus Johnson",
      avatar: "MJ",
      role: "Developer Advocate",
    },
    publishedAt: "2024-01-02",
    readingTime: "6 min read",
    category: "Tutorial",
    tags: ["kafka", "migration", "tutorial", "getting-started"],
  },
  {
    slug: "real-time-analytics-pipeline",
    title: "Building a Real-Time Analytics Pipeline with StreamHouse",
    excerpt:
      "Learn how to build an end-to-end real-time analytics pipeline using StreamHouse SQL processing, from ingestion to dashboard.",
    content: `
## Introduction

Real-time analytics pipelines traditionally require multiple systems: Kafka for ingestion, Flink for processing, and a database for serving. StreamHouse consolidates this into a single platform.

### Architecture Overview

\`\`\`
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Events    │────▶│ StreamHouse │────▶│  Dashboard  │
│   (API)     │     │    SQL      │     │  (Grafana)  │
└─────────────┘     └─────────────┘     └─────────────┘
\`\`\`

### Step 1: Set Up Event Ingestion

Create a topic for raw events:

\`\`\`bash
streamctl topics create raw_events --partitions 4
\`\`\`

Produce events from your API:

\`\`\`javascript
// Express middleware
app.use((req, res, next) => {
  producer.send('raw_events', {
    timestamp: Date.now(),
    path: req.path,
    method: req.method,
    user_id: req.user?.id,
    response_time_ms: res.responseTime
  });
  next();
});
\`\`\`

### Step 2: Create Aggregation Streams

Use SQL to compute real-time aggregates:

\`\`\`sql
-- Requests per minute by endpoint
CREATE STREAM rpm_by_endpoint AS
SELECT
  path,
  COUNT(*) as request_count,
  AVG(response_time_ms) as avg_latency,
  TUMBLE_START(timestamp, INTERVAL '1' MINUTE) as window_start
FROM raw_events
GROUP BY
  path,
  TUMBLE(timestamp, INTERVAL '1' MINUTE);
\`\`\`

### Step 3: Connect to Grafana

StreamHouse exposes a Prometheus endpoint for metrics:

\`\`\`yaml
# prometheus.yml
scrape_configs:
  - job_name: 'streamhouse'
    static_configs:
      - targets: ['agent:8080']
\`\`\`

Create Grafana dashboards to visualize your streams.

### Step 4: Add Alerting

Configure alerts for anomalies:

\`\`\`sql
CREATE STREAM high_latency_alerts AS
SELECT *
FROM rpm_by_endpoint
WHERE avg_latency > 500;
\`\`\`

Connect alerts to PagerDuty or Slack via webhooks.

### Performance Considerations

For high-throughput analytics:

1. **Partition wisely**: Use high-cardinality keys for even distribution
2. **Window size**: Larger windows reduce output volume but increase latency
3. **Materialized views**: Cache expensive aggregations

### Conclusion

StreamHouse SQL processing eliminates the need for separate stream processing infrastructure. Build real-time analytics pipelines with familiar SQL syntax and minimal operational overhead.

[Read the full tutorial →](/docs/tutorials/analytics-pipeline)
    `,
    author: {
      name: "Emily Zhang",
      avatar: "EZ",
      role: "Solutions Architect",
    },
    publishedAt: "2023-12-20",
    readingTime: "8 min read",
    category: "Tutorial",
    tags: ["analytics", "sql", "grafana", "tutorial"],
  },
  {
    slug: "inside-the-segment",
    title: "Inside the Segment: How StreamHouse Stores Billions of Events on S3",
    excerpt:
      "A deep dive into StreamHouse's binary segment format — how records become blocks, blocks become segments, and segments land in S3 with LZ4 compression and CRC32 integrity checks.",
    content: `
## Every Event Has a Home

When you produce a message to StreamHouse, it doesn't just vanish into the cloud. It follows a precise, deterministic path: from your producer, through an agent's memory buffer, into a compressed binary segment, and finally into an S3 object where it will live with 11 nines of durability.

This post walks through exactly what happens to your bytes at every step.

## The Storage Hierarchy

StreamHouse organizes data in four levels:

\`\`\`
Records → Blocks → Segments → Partitions
   │          │         │          │
 Single    ~1MB     ~64MB    Ordered
 event    batches   S3 files   log
\`\`\`

**Records** are individual events — a key, value, timestamp, and optional headers. **Blocks** group ~1MB of records together for compression. **Segments** are the unit of storage on S3, typically 64MB containing many blocks. **Partitions** are the ordered sequence of segments that form a topic's log.

## The Segment Binary Format

Every segment is a self-contained, immutable file with four sections:

\`\`\`
┌─────────────────────────────────┐
│ Header (64 bytes)               │
│  - Magic: 0x5348 ("SH")        │
│  - Version: u16                 │
│  - Flags: u32                   │
│  - Compression: u8             │
│  - Created timestamp: i64       │
│  - Record count: u64            │
│  - Start/End offset: u64        │
├─────────────────────────────────┤
│ Block 0                         │
│  - Compressed records (~1MB)    │
│  - CRC32 checksum              │
├─────────────────────────────────┤
│ Block 1                         │
│  - Compressed records (~1MB)    │
│  - CRC32 checksum              │
├─────────────────────────────────┤
│ ...more blocks...               │
├─────────────────────────────────┤
│ Sparse Index                    │
│  - Offset → byte position      │
│  - One entry per block          │
├─────────────────────────────────┤
│ Footer (16 bytes)               │
│  - Index offset: u64            │
│  - File CRC32: u32             │
│  - Magic: 0x454E ("EN")        │
└─────────────────────────────────┘
\`\`\`

The header is read first to verify the file and understand its contents. The footer is read to locate the index. The index maps offsets to byte positions so consumers can jump directly to the block containing a target offset without scanning the entire file.

## Record Encoding

Inside each block, records use varint encoding and delta compression for maximum space efficiency:

\`\`\`
┌─────────────────────────────────┐
│ Record                          │
│  - Offset delta (varint)        │
│  - Timestamp delta (varint)     │
│  - Key length (varint)          │
│  - Key bytes                    │
│  - Value length (varint)        │
│  - Value bytes                  │
│  - Header count (varint)        │
│  - Header entries               │
└─────────────────────────────────┘
\`\`\`

Instead of storing absolute offsets and timestamps, we store deltas from the previous record. A sequence of offsets like \`[1000, 1001, 1002, 1003]\` becomes \`[1000, 1, 1, 1]\` — varints that encode in a single byte each.

## Why LZ4?

We chose LZ4 as the default compression algorithm after extensive benchmarking:

- **JSON payloads**: 4.3x compression ratio, 3.2 GB/s decompression
- **Protobuf payloads**: 1.4x ratio, 3.8 GB/s decompression
- **Text logs**: 8x ratio, 2.9 GB/s decompression

LZ4 decompresses at nearly memory bandwidth speed, which matters because consumers need to decompress segments on every read. We also support Zstd for workloads where storage cost matters more than latency — it achieves roughly 2x better compression at 5-10x slower decompression.

\`\`\`bash
# Choose compression per topic
streamctl topic create --name logs --partitions 6 --compression lz4
streamctl topic create --name archive --partitions 3 --compression zstd
\`\`\`

## Why 64MB Segments?

The 64MB target isn't arbitrary. It's the result of balancing three forces:

**Smaller segments** (e.g. 4MB):
- More S3 PUT operations = higher cost ($0.005 per 1000 PUTs)
- More metadata entries in PostgreSQL
- Better read granularity for small range queries

**Larger segments** (e.g. 256MB):
- Fewer S3 operations = lower cost
- Less metadata overhead
- Higher read amplification — consumers must download more unused data

At 64MB with LZ4, a typical segment holds 100K-500K records and costs ~$0.000005 to PUT. The segment flushes every 10 seconds or when the buffer hits 64MB, whichever comes first.

## CRC32 at Every Level

Data integrity is non-negotiable. StreamHouse computes CRC32 checksums at three levels:

1. **Per-block**: Each compressed block has a CRC32 of its compressed bytes. If a block fails validation during read, the agent retries the S3 fetch.
2. **Per-file**: The footer contains a CRC32 of the entire segment. Corrupted segments are detected during any read operation.
3. **Per-record (WAL)**: The Write-Ahead Log checksums every individual record before it enters the buffer.

If any checksum fails, StreamHouse rejects the data and logs a corruption event rather than serving bad records.

## The Lifecycle of a Segment

1. **Buffering**: Records arrive via gRPC/HTTP and enter the agent's in-memory SegmentBuffer, organized by partition
2. **Flushing**: When the buffer reaches 64MB or 10 seconds elapse, the agent compresses blocks with LZ4, builds the index, and computes checksums
3. **Upload**: The segment is uploaded to S3 as a single PUT operation
4. **Registration**: The agent records the segment's S3 path, offset range, and size in the PostgreSQL metadata store
5. **Sealing**: The segment is now immutable — it will never be modified, only eventually deleted by retention policies

## S3 Path Layout

Segments are organized in S3 with a predictable path structure:

\`\`\`
s3://streamhouse-data/
  topics/
    user-events/
      partitions/
        0/
          segments/
            00000000-00000999.seg
            00001000-00001999.seg
        1/
          segments/
            00000000-00000499.seg
\`\`\`

This structure enables efficient prefix listing when an agent needs to discover segments for a partition, and makes it easy to configure S3 lifecycle rules for cost optimization.

## What This Means for You

The segment format is entirely transparent to users — you never interact with it directly. But understanding it explains several StreamHouse behaviors:

- **Why produce latency is 50-100ms**: Records are buffered until a segment is ready to flush
- **Why tail reads are fast**: Recent data is still in the agent's memory buffer, no S3 fetch needed
- **Why storage is cheap**: LZ4 compression + S3 pricing = pennies per GB per month
- **Why data is durable**: CRC32 checksums + S3's 11 nines = you won't lose events

The segment format is open and documented. Build tools on top of it, inspect your data directly, or just rest easy knowing your events are stored with care.
    `,
    author: {
      name: "Alex Chen",
      avatar: "AC",
      role: "Lead Engineer",
    },
    publishedAt: "2024-02-12",
    readingTime: "10 min read",
    category: "Deep Dive",
    tags: ["storage", "s3", "compression", "internals"],
  },
  {
    slug: "topics-partitions-offsets",
    title: "Topics, Partitions, and Offsets: The Building Blocks of StreamHouse",
    excerpt:
      "How does StreamHouse organize billions of events into ordered, replayable streams? A deep dive into topics, partition assignment, offset tracking, and consumer groups.",
    content: `
## The Log Abstraction

At its core, StreamHouse is a distributed log. Every event you produce is appended to an ordered, immutable sequence. Understanding the three building blocks — topics, partitions, and offsets — is key to using StreamHouse effectively.

## Topics: Named Event Streams

A topic is a named category for events. Think of it like a database table, but append-only. When you create a topic, you're declaring: "this is where events of type X go."

\`\`\`bash
streamctl topic create --name user-events --partitions 6 --retention 30d --compression lz4
\`\`\`

Topics have configuration that controls their behavior:

- **Partition count**: How many parallel lanes the topic has (more partitions = more parallelism)
- **Retention**: How long events are kept (time-based, size-based, or both)
- **Compression**: LZ4 (fast) or Zstd (compact)
- **Schema**: Optional schema validation via the Schema Registry

Unlike traditional message queues, events in a topic are **persistent and replayable**. A consumer can read from the beginning, the end, or any point in between. Multiple consumer groups can independently process the same topic at different speeds.

## Partitions: Parallel Ordered Lanes

Each topic is split into one or more partitions. A partition is a single ordered sequence of events — within a partition, events have a strict total order.

Why partitions? Two reasons:

**1. Parallelism**: Each partition can be processed by a different consumer. A topic with 6 partitions can have up to 6 consumers processing simultaneously.

**2. Ordering guarantees**: Events with the same key always go to the same partition, so they're always processed in order.

### How Keys Map to Partitions

When a producer sends a message with a key, StreamHouse uses murmur2 hashing (consistent with Kafka) to determine the target partition:

\`\`\`
partition = murmur2(key) % num_partitions
\`\`\`

This means all events for \`user-123\` always land in the same partition, preserving order for that user. Events without a key are distributed round-robin.

\`\`\`bash
# These always go to the same partition
streamctl produce --topic user-events --key "user-123" --message '{"event": "login"}'
streamctl produce --topic user-events --key "user-123" --message '{"event": "purchase"}'

# These are spread across partitions
streamctl produce --topic user-events --message '{"event": "heartbeat"}'
\`\`\`

### Choosing the Right Partition Count

This is one of the most common questions. Here's our guidance:

- **Low throughput** (<10K events/sec): 3-6 partitions
- **Medium throughput** (10K-100K events/sec): 6-12 partitions
- **High throughput** (100K+ events/sec): 12-64 partitions

The upper limit is practical, not technical. More partitions mean more S3 segments, more metadata entries, and more consumer coordination. Start small and increase if you hit throughput limits.

## Offsets: Your Place in the Stream

Every event within a partition has an **offset** — a monotonically increasing 64-bit integer that uniquely identifies it. Offsets start at 0 and never go backwards.

\`\`\`
Partition 0:
  Offset 0: {"user": "alice", "event": "signup"}
  Offset 1: {"user": "alice", "event": "login"}
  Offset 2: {"user": "bob", "event": "signup"}
  Offset 3: {"user": "alice", "event": "purchase"}
  ...
\`\`\`

Offsets serve two critical purposes:

**1. Position tracking**: A consumer's offset tells StreamHouse exactly where to resume reading after a restart.

**2. Exactly-once semantics**: By committing offsets only after successful processing, consumers achieve exactly-once delivery guarantees.

### The High Watermark

Each partition tracks a **high watermark** — the offset of the most recently committed (flushed to S3) event. Consumers can read up to the high watermark. Events still buffering in agent memory are not yet visible to consumers using committed reads.

\`\`\`
Partition 0 state:
  First offset:     0
  High watermark:   15,234
  Log end offset:   15,289  (includes unflushed buffer)
\`\`\`

The gap between the high watermark and log end offset represents events that are in the agent's write buffer but not yet flushed to S3.

## Consumer Groups: Coordinated Processing

A consumer group is a set of consumers that cooperatively process a topic. StreamHouse ensures each partition is assigned to exactly one consumer in the group:

\`\`\`
Topic: user-events (6 partitions)
Consumer Group: analytics-pipeline

  Consumer A → Partition 0, 1
  Consumer B → Partition 2, 3
  Consumer C → Partition 4, 5
\`\`\`

If Consumer B crashes, its partitions are redistributed:

\`\`\`
  Consumer A → Partition 0, 1, 2
  Consumer C → Partition 3, 4, 5
\`\`\`

### Offset Commits

Consumer groups track their progress by committing offsets to the metadata store. Two strategies:

**Auto-commit**: Offsets are committed every 5 seconds automatically. Simple but may cause duplicate processing on crash.

\`\`\`bash
streamctl consume --topic user-events --group analytics --auto-commit
\`\`\`

**Manual commit**: Your application commits offsets explicitly after processing. More complex but enables exactly-once processing when combined with idempotent writes.

## How It All Fits Together in S3

Here's the physical reality of a topic with 3 partitions:

\`\`\`
PostgreSQL (metadata):
  topics: {name: "user-events", partitions: 3}
  partitions: [{id: 0, hwm: 15234}, {id: 1, hwm: 12001}, {id: 2, hwm: 18442}]
  segments: [{partition: 0, start: 0, end: 999, path: "s3://..."}, ...]

S3 (data):
  topics/user-events/partitions/0/segments/00000000-00000999.seg (64MB)
  topics/user-events/partitions/0/segments/00001000-00001999.seg (64MB)
  topics/user-events/partitions/1/segments/00000000-00000799.seg (64MB)
  topics/user-events/partitions/2/segments/00000000-00001199.seg (64MB)
\`\`\`

Metadata stays small (kilobytes). Data stays in S3 (terabytes). Agents are stateless in between.

## Key Takeaways

- **Topics** are named, persistent event streams with configurable retention and compression
- **Partitions** enable parallel processing while preserving per-key ordering
- **Offsets** are your consumer's bookmark — commit them wisely
- **Consumer groups** coordinate multiple consumers to share the load
- The entire model is Kafka-compatible, so existing mental models and patterns transfer directly

Understanding these primitives is the foundation for everything else in StreamHouse — from building real-time pipelines to debugging consumer lag.
    `,
    author: {
      name: "Marcus Johnson",
      avatar: "MJ",
      role: "Developer Advocate",
    },
    publishedAt: "2024-02-05",
    readingTime: "9 min read",
    category: "Deep Dive",
    tags: ["topics", "partitions", "offsets", "consumer-groups", "fundamentals"],
  },
  {
    slug: "zero-data-loss",
    title: "Zero Data Loss: How StreamHouse's Write-Ahead Log Prevents Lost Events",
    excerpt:
      "What happens when an agent crashes mid-write? A deep dive into StreamHouse's WAL, CRC32 checksums, sync policies, and the recovery process that ensures your events survive any failure.",
    content: `
## The Durability Problem

Here's the worst-case scenario: your producer sends 10,000 events to a StreamHouse agent. The agent buffers them in memory, preparing a segment for S3 upload. Then the process crashes. The segment never reaches S3. Are those 10,000 events gone?

Without the Write-Ahead Log (WAL), yes. With it, no.

## The Data Path

To understand the WAL, you need to understand the data path:

\`\`\`
Producer → Agent (gRPC) → WAL (disk) → SegmentBuffer (RAM) → S3
\`\`\`

Every record that enters an agent is written to the WAL **before** it enters the in-memory segment buffer. The WAL is a sequential, append-only file on the agent's local disk. If the agent crashes, the WAL file survives, and on restart, records are replayed from the WAL back into memory.

## WAL Record Format

Each entry in the WAL contains everything needed to reconstruct the record:

\`\`\`
┌─────────────────────────────────┐
│ WAL Entry                       │
│  - Length: u32                  │
│  - CRC32: u32                  │
│  - Topic: string               │
│  - Partition: u32              │
│  - Key: bytes                  │
│  - Value: bytes                │
│  - Timestamp: i64              │
│  - Headers: [(string, bytes)]  │
└─────────────────────────────────┘
\`\`\`

The CRC32 checksum covers the entire entry, including the length field. This catches partial writes, bit flips, and filesystem corruption. During recovery, any entry with a bad CRC is discarded — it represents an incomplete write that was interrupted by the crash.

## Three Sync Policies

The critical question is: when do we \`fsync\` the WAL to disk? StreamHouse offers three policies to match different durability-performance tradeoffs:

### Always Sync

\`\`\`bash
export WAL_SYNC_POLICY=always
\`\`\`

Every record is \`fsync\`'d to disk before the produce request is acknowledged. **Zero data loss** even on power failure. Throughput: 50,000-100,000 records/sec.

This is the safest option. The latency cost is 100-500 microseconds per \`fsync\` on SSD, which is acceptable for most workloads.

### Interval Sync (Recommended)

\`\`\`bash
export WAL_SYNC_POLICY=interval
export WAL_SYNC_INTERVAL_MS=100
\`\`\`

The WAL is \`fsync\`'d every 100ms. Records written between syncs may be lost on a power failure (not a process crash — the OS buffer cache survives process crashes on Linux). At risk: 100-1000 records in the worst case.

This is the recommended default. It achieves 1-2 million records/sec while losing at most 100ms of data on a catastrophic hardware failure.

### Never Sync

\`\`\`bash
export WAL_SYNC_POLICY=never
\`\`\`

The WAL relies entirely on the OS buffer cache for durability. Data is durable against process crashes but may be lost on power failure. Throughput: 2+ million records/sec.

Use this for development or workloads where occasional data loss is acceptable (metrics, debug logs).

## The Recovery Process

When a StreamHouse agent starts, it checks for an existing WAL file. If one exists, recovery runs automatically:

1. **Open the WAL file** and read from the beginning
2. **Validate each entry** by computing the CRC32 and comparing it to the stored checksum
3. **Replay valid entries** into the in-memory SegmentBuffer, partitioned by topic and partition
4. **Skip invalid entries** — these represent partially written records from the crash point
5. **Resume normal operation** — the next produce request appends to the existing WAL
6. **Flush recovered segments to S3** following the normal segment lifecycle

\`\`\`
Agent startup:
  [INFO] WAL file found: /data/wal/streamhouse.wal (24MB)
  [INFO] Replaying WAL entries...
  [INFO] Recovered 48,231 records across 12 partitions
  [INFO] Skipped 3 entries with invalid CRC (partial writes)
  [INFO] Recovery complete in 340ms
  [INFO] Agent ready to accept connections
\`\`\`

The recovery process is fast — it reads sequentially from disk at SSD speed, typically recovering millions of records per second.

## Failure Scenarios

### Scenario 1: Agent Process Crash

The agent receives a SIGSEGV, OOM kill, or unhandled panic.

- **WAL entries already synced**: Recovered on restart. Zero loss.
- **WAL entries in OS buffer cache**: Recovered on restart (process crash doesn't clear the page cache). Zero loss.
- **Segment buffer (RAM)**: Anything already in the WAL is safe. The segment that was building in memory is reconstructed from WAL replay.

### Scenario 2: Agent Crash During S3 Upload

The agent crashes while uploading a segment to S3.

- **S3 upload is atomic** — either the full object lands or it doesn't. Partial uploads don't create visible objects.
- **On restart**, the WAL replay reconstructs the segment buffer. The agent re-uploads the segment.
- **Duplicate segments** are prevented by checking the metadata store for existing segment registrations before uploading.

### Scenario 3: Power Failure

The physical machine loses power, wiping both RAM and the OS buffer cache.

- With **always sync**: Zero loss. Every acknowledged record is on disk.
- With **interval sync**: Loss of up to one sync interval (default 100ms) of data.
- With **never sync**: Loss of all unflushed WAL data.

## The Full Durability Stack

The WAL is one layer in a multi-layer durability strategy:

\`\`\`
Layer 1: WAL (local disk)        → survives process crashes
Layer 2: S3 (object storage)     → survives hardware failure (11 nines)
Layer 3: PostgreSQL (metadata)   → survives with automated backups
Layer 4: CRC32 checksums         → detects corruption at every level
\`\`\`

Once a segment is flushed to S3 and registered in the metadata store, the WAL entries for those records are no longer needed. The WAL is periodically truncated to reclaim disk space.

## Monitoring the WAL

Keep an eye on these metrics:

\`\`\`
streamhouse_wal_size_bytes           # Current WAL file size
streamhouse_wal_entries_total        # Total entries written
streamhouse_wal_recovery_records     # Records recovered on last startup
streamhouse_wal_sync_duration_ms     # Time spent in fsync
streamhouse_wal_corruption_detected  # CRC failures (should be 0)
\`\`\`

Alert on \`wal_corruption_detected > 0\` — it indicates a disk issue that needs investigation.

## The Bottom Line

StreamHouse's WAL guarantees that acknowledged events survive agent crashes with configurable durability. Combined with S3's 11-nines durability and CRC32 checksums at every level, the system provides end-to-end data integrity from producer to consumer.

Choose your sync policy based on your workload:

- **Financial transactions**: Always sync
- **General production**: Interval sync (100ms)
- **Dev and metrics**: Never sync

Your data is safe.
    `,
    author: {
      name: "Sarah Kim",
      avatar: "SK",
      role: "Co-founder & CTO",
    },
    publishedAt: "2024-01-29",
    readingTime: "11 min read",
    category: "Deep Dive",
    tags: ["durability", "wal", "data-safety", "internals", "reliability"],
  },
  {
    slug: "stateless-agents",
    title: "The Stateless Agent: How StreamHouse Scales Without Disks",
    excerpt:
      "StreamHouse agents hold no persistent state — no disks, no replication, no rebalancing. Here's how lease coordination, failure detection, and S3 make this possible.",
    content: `
## Why Stateless?

The single biggest operational burden in running Apache Kafka is managing broker state. Each broker owns partitions, stores replicas on local disk, and must coordinate leader election with other brokers. Adding or removing a broker triggers a complex rebalancing process that can take hours.

StreamHouse agents are stateless. You can start one, stop one, or replace one without any data migration. This post explains how.

## What an Agent Actually Does

A StreamHouse agent is a Rust process that handles three things:

1. **Accept produce requests**: Buffer records in memory, flush segments to S3
2. **Serve consume requests**: Find segments in S3 (or cache), decompress, return records
3. **Coordinate with peers**: Acquire leases to avoid duplicate work

That's it. The agent doesn't own partitions. It doesn't replicate data. It doesn't store anything permanently. When it restarts, it starts fresh.

\`\`\`
┌─────────────────────────────────────────┐
│ StreamHouse Agent                       │
│                                         │
│  ┌──────────────────────────────────┐   │
│  │ Writer Pool                      │   │
│  │  Partition 0: [records...] 23MB  │   │
│  │  Partition 3: [records...] 45MB  │   │
│  │  Partition 7: [records...] 12MB  │   │
│  └──────────────────────────────────┘   │
│                                         │
│  ┌──────────────────────────────────┐   │
│  │ Segment Cache (LRU)             │   │
│  │  512MB of recently read segments │   │
│  └──────────────────────────────────┘   │
│                                         │
│  ┌──────────────────────────────────┐   │
│  │ Metadata Cache (LRU)            │   │
│  │  Topics: 5 min TTL              │   │
│  │  Partitions: 30s TTL            │   │
│  └──────────────────────────────────┘   │
│                                         │
│  State: NONE on disk                    │
└─────────────────────────────────────────┘
\`\`\`

## Lease-Based Coordination

The challenge with stateless agents is coordination: if any agent can write to any partition, how do you prevent two agents from creating conflicting segments for the same partition?

The answer is **leases**. Before an agent flushes a segment for a partition, it acquires a lease from PostgreSQL:

\`\`\`sql
-- Simplified lease acquisition
SELECT * FROM partition_leases
WHERE partition_id = $1 AND topic_id = $2
FOR UPDATE SKIP LOCKED;

-- If no active lease, acquire one
INSERT INTO partition_leases (partition_id, topic_id, agent_id, expires_at)
VALUES ($1, $2, $3, NOW() + INTERVAL '30 seconds')
ON CONFLICT (partition_id, topic_id)
DO UPDATE SET agent_id = $3, expires_at = NOW() + INTERVAL '30 seconds'
WHERE partition_leases.expires_at < NOW();
\`\`\`

Leases have a 30-second TTL and are renewed every 10 seconds via heartbeat. If an agent crashes, its leases expire and other agents can take over.

## Failure Detection

StreamHouse uses multiple mechanisms to detect agent failures:

**1. Lease expiration**: If an agent stops heartbeating, its leases expire in 30 seconds. Other agents detect this on their next coordination cycle.

**2. Health endpoints**: Kubernetes liveness and readiness probes check agent health every 10 seconds.

\`\`\`yaml
livenessProbe:
  httpGet:
    path: /health/live
    port: 8080
  periodSeconds: 10
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /health/ready
    port: 8080
  periodSeconds: 5
\`\`\`

**3. Connection monitoring**: The agent monitors its PostgreSQL connection. If metadata queries fail for 3 consecutive attempts, the agent marks itself as unhealthy.

The maximum detection latency is bounded by the lease TTL: 30 seconds in the worst case. In practice, Kubernetes restarts unhealthy agents in under 30 seconds.

## Scaling: Just Add Agents

Because agents are stateless, scaling is trivially simple:

\`\`\`yaml
# Scale from 3 to 10 agents
kubectl scale deployment streamhouse --replicas=10
\`\`\`

There is no:
- Partition reassignment
- Data rebalancing
- Leader election
- Replica synchronization

Every new agent immediately starts accepting requests. The load balancer distributes traffic evenly. Each agent independently acquires leases for the partitions it writes to.

### Auto-Scaling

For production deployments, use Kubernetes HPA based on CPU or custom metrics:

\`\`\`yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: streamhouse-agents
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: streamhouse
  minReplicas: 3
  maxReplicas: 50
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
\`\`\`

## The Metadata Cache

Agents cache frequently accessed metadata to minimize PostgreSQL load:

- **Topic metadata**: Cached for 5 minutes (topic config rarely changes)
- **Partition info**: Cached for 30 seconds (high watermarks change frequently)
- **Segment index**: In-memory BTreeMap for offset lookups (100x faster than DB queries)

This caching layer achieves 90%+ hit rates, reducing PostgreSQL load by 10-20x. Cache entries are invalidated on TTL expiry — write-through ensures consistency.

## Performance Characteristics

A single StreamHouse agent (4 CPU, 4GB RAM):

- **Produce throughput**: 50,000+ records/sec
- **Consume throughput**: 200,000+ records/sec
- **Produce latency**: <1ms P50, <5ms P99 (buffered, not including S3 flush)
- **Consume latency**: 5ms P50, 10ms P99 (cached segments)
- **Memory usage**: ~2GB (write buffers + segment cache)

Three agents behind a load balancer comfortably handle 150,000 produces/sec and 600,000 consumes/sec.

## What This Means for Operations

Running StreamHouse agents is fundamentally different from running Kafka brokers:

| | Kafka Brokers | StreamHouse Agents |
|---|---|---|
| **Scaling** | Partition rebalancing (hours) | kubectl scale (seconds) |
| **Failure recovery** | ISR catch-up (minutes) | Lease expiry (30 seconds) |
| **Disk management** | Monitor IOPS, plan capacity | No disks needed |
| **Upgrades** | Rolling restart with care | Rolling restart, any order |
| **Cost** | Provisioned for peak | Scale to actual load |

The stateless agent architecture is the reason StreamHouse is operationally simpler than Kafka. No state means no state to lose, no state to replicate, and no state to rebalance.
    `,
    author: {
      name: "Alex Chen",
      avatar: "AC",
      role: "Lead Engineer",
    },
    publishedAt: "2024-01-22",
    readingTime: "9 min read",
    category: "Deep Dive",
    tags: ["agents", "architecture", "scaling", "kubernetes", "operations"],
  },
  {
    slug: "taming-s3-at-scale",
    title: "Taming S3 at Scale: Circuit Breakers and Intelligent Rate Limiting",
    excerpt:
      "At high throughput, S3 will throttle you. Here's how StreamHouse uses token buckets, circuit breakers, and adaptive backoff to handle 10,000+ S3 operations per second without dropping events.",
    content: `
## The S3 Throttling Problem

S3 is not infinitely fast. AWS documents specific request rate limits:

- **PUT/POST/DELETE**: 3,500 requests/sec per prefix
- **GET/HEAD**: 5,500 requests/sec per prefix

When you exceed these limits, S3 returns HTTP 503 (Service Unavailable) or 429 (Too Many Requests). In a high-throughput streaming platform, hitting these limits is not a theoretical concern — it's a Tuesday.

A single StreamHouse agent flushing 64MB segments every 10 seconds generates modest S3 traffic. But 20 agents across hundreds of partitions? That's thousands of PUT and GET operations per second.

## Our Three-Layer Defense

StreamHouse implements three complementary mechanisms to handle S3 throttling:

### Layer 1: Token Bucket Rate Limiter

Before any S3 operation, the agent must acquire a token from a rate limiter. Tokens replenish at a configured rate, spreading operations evenly over time.

\`\`\`
Default limits:
  PUT operations:    3,000/sec (below S3's 3,500 limit)
  GET operations:    5,000/sec (below S3's 5,500 limit)
  DELETE operations: 3,000/sec
\`\`\`

We set defaults below S3's documented limits to leave headroom for burst absorption. The token bucket allows short bursts (up to 2x the rate for 1 second) while maintaining the average rate.

### Layer 2: Circuit Breaker

If S3 starts returning throttle responses despite rate limiting, the circuit breaker activates:

\`\`\`
States:
  CLOSED  → Normal operation. All requests pass through.
  OPEN    → S3 is throttling. All requests are queued/rejected.
  HALF_OPEN → Testing recovery. Limited requests allowed.

Transitions:
  CLOSED → OPEN:      5 throttle responses in 10 seconds
  OPEN → HALF_OPEN:   After 30 second cooldown
  HALF_OPEN → CLOSED: 3 consecutive successes
  HALF_OPEN → OPEN:   Any throttle response
\`\`\`

When the circuit breaker opens, segment flushes are paused and records accumulate in the agent's memory buffer. This is safe because the WAL ensures durability. When S3 recovers, the circuit breaker closes and buffered segments flush.

### Layer 3: Adaptive Backoff

Individual retries use exponential backoff with jitter:

\`\`\`
retry_delay = min(base_delay * 2^attempt + random_jitter, max_delay)

Attempt 1: 100ms + jitter
Attempt 2: 200ms + jitter
Attempt 3: 400ms + jitter
Attempt 4: 800ms + jitter
...
Max delay: 30 seconds
Max attempts: 10
\`\`\`

The jitter prevents thundering herd — without it, all agents would retry simultaneously after a throttle event, causing another wave of throttling.

## S3 Prefix Optimization

S3 rate limits are per-prefix. StreamHouse's path layout naturally distributes load:

\`\`\`
s3://bucket/topics/user-events/partitions/0/segments/...
s3://bucket/topics/user-events/partitions/1/segments/...
s3://bucket/topics/order-events/partitions/0/segments/...
\`\`\`

Each partition is a separate prefix, so a topic with 6 partitions gets 6x the rate limit. This is why increasing partition count can improve throughput — it's not just about consumer parallelism.

## Monitoring Throttle Health

StreamHouse exposes detailed metrics for S3 operations:

\`\`\`
streamhouse_s3_requests_total{operation="put"}     # Total S3 PUT requests
streamhouse_s3_throttled_total{operation="put"}     # Throttled responses
streamhouse_s3_circuit_breaker_state                # 0=closed, 1=open, 2=half_open
streamhouse_s3_rate_limiter_queued                  # Requests waiting for tokens
streamhouse_s3_retry_total                          # Total retries
\`\`\`

Alert if the throttle rate exceeds 1%:

\`\`\`yaml
- alert: S3ThrottlingHigh
  expr: rate(streamhouse_s3_throttled_total[5m]) / rate(streamhouse_s3_requests_total[5m]) > 0.01
  for: 5m
  annotations:
    summary: "S3 throttle rate above 1%"
    runbook: "Consider increasing partition count or reducing flush frequency"
\`\`\`

## Configuration

All throttle parameters are tunable:

\`\`\`toml
[storage.throttle]
put_rate_limit = 3000
get_rate_limit = 5000
delete_rate_limit = 3000
circuit_breaker_threshold = 5
circuit_breaker_cooldown_secs = 30
max_retry_attempts = 10
base_retry_delay_ms = 100
\`\`\`

For workloads with dedicated S3 prefixes (where you won't share rate limits with other applications), you can increase these limits.

## Real-World Impact

In our load tests with 20 agents and 200 partitions:

- **Without throttle protection**: 12% of segment uploads failed during peak hours, requiring manual intervention
- **With throttle protection**: 0% upload failures, automatic recovery from all S3 throttle events, maximum 30-second delay during circuit breaker open events

The buffering provided by the WAL and in-memory segment buffers means S3 throttling never results in data loss — only temporary increased latency.

S3 is an incredible storage backend. But at scale, you need to respect its limits. StreamHouse does this automatically so you don't have to.
    `,
    author: {
      name: "Alex Chen",
      avatar: "AC",
      role: "Lead Engineer",
    },
    publishedAt: "2024-02-19",
    readingTime: "8 min read",
    category: "Deep Dive",
    tags: ["s3", "reliability", "circuit-breaker", "performance", "operations"],
  },
  {
    slug: "sql-on-streams-datafusion",
    title: "SQL on Streams: How Apache DataFusion Powers Real-Time Queries",
    excerpt:
      "StreamHouse embeds Apache DataFusion for SQL stream processing. Here's how we turned a batch query engine into a streaming powerhouse — with tumbling windows, stream joins, and continuous queries.",
    content: `
## Why SQL?

Every data engineer knows SQL. But stream processing has traditionally required learning specialized frameworks: Flink's DataStream API, Spark Structured Streaming's DataFrame API, or ksqlDB's custom dialect.

We wanted StreamHouse users to write plain SQL and have it work on streaming data. No new APIs. No new deployment infrastructure. Just SQL.

## Why DataFusion?

[Apache DataFusion](https://datafusion.apache.org/) is a query engine written in Rust, built on Apache Arrow for columnar in-memory processing. It gives us:

- **SQL parser and planner**: Full ANSI SQL support
- **Query optimizer**: Predicate pushdown, projection pruning, join reordering
- **Columnar execution**: Arrow RecordBatches for vectorized processing
- **Extensibility**: Custom table providers, functions, and execution plans

DataFusion was designed for batch queries, but its architecture is flexible enough for streaming. Here's how we adapted it.

## The Streaming Table Provider

In standard DataFusion, a table is a static set of files or partitions. In StreamHouse, a table is a live stream of events. We implemented a custom \`StreamTableProvider\` that:

1. **Registers topic schema**: Maps topic fields to Arrow data types
2. **Provides a streaming execution plan**: Instead of scanning files, it continuously polls for new records
3. **Tracks watermarks**: Knows how far into the stream we've processed

\`\`\`sql
-- Register a topic as a SQL table
CREATE STREAM clickstream (
  user_id VARCHAR,
  page VARCHAR,
  referrer VARCHAR,
  event_time TIMESTAMP
) WITH (
  topic = 'clicks',
  format = 'json',
  timestamp_field = 'event_time'
);

-- Query it like any table
SELECT page, count(*) as views
FROM clickstream
WHERE event_time > NOW() - INTERVAL '1 hour'
GROUP BY page
ORDER BY views DESC;
\`\`\`

## Windowed Aggregations

Aggregations over unbounded streams need windows — finite time boundaries that define "group this set of events together." StreamHouse supports three window types:

### Tumbling Windows

Fixed-size, non-overlapping. Every event belongs to exactly one window.

\`\`\`sql
SELECT
  page,
  count(*) as views,
  window_start,
  window_end
FROM TUMBLE(clickstream, event_time, INTERVAL '5 minutes')
GROUP BY page, window_start, window_end;
\`\`\`

Under the hood, this creates a custom physical plan node that:
1. Reads batches from the stream
2. Assigns each record to a window based on its timestamp
3. Maintains per-window accumulators (count, sum, etc.)
4. Emits completed windows when the watermark advances past the window end

### Hopping Windows

Fixed-size with a smaller advance interval. Events belong to multiple overlapping windows.

\`\`\`sql
-- 10-minute windows that advance every 2 minutes
SELECT
  page,
  count(*) as views,
  avg(load_time_ms) as avg_load
FROM HOP(clickstream, event_time, INTERVAL '2 minutes', INTERVAL '10 minutes')
GROUP BY page, window_start, window_end;
\`\`\`

### Session Windows

Dynamic windows that group events close together in time, separated by gaps of inactivity. Perfect for user sessions.

\`\`\`sql
SELECT
  user_id,
  count(*) as clicks,
  min(event_time) as session_start,
  max(event_time) as session_end
FROM SESSION(clickstream, event_time, INTERVAL '30 minutes')
GROUP BY user_id, session_start, session_end;
\`\`\`

## Continuous Queries

One-off queries are useful for exploration. Continuous queries are useful for production. A continuous query runs indefinitely, processing each new event and writing results to an output topic.

\`\`\`sql
CREATE CONTINUOUS QUERY page_view_counts AS
  SELECT
    page,
    count(*) as views,
    window_start
  FROM TUMBLE(clickstream, event_time, INTERVAL '1 minute')
  GROUP BY page, window_start, window_end
  OUTPUT TO 'page-views-per-minute';
\`\`\`

The output topic (\`page-views-per-minute\`) is a regular StreamHouse topic. Other consumers, SQL queries, or external systems can read from it.

## Stream-Table Joins

The most powerful pattern: joining a real-time stream with a "table" (a compacted topic that represents the latest state per key).

\`\`\`sql
-- Compacted topic as a table
CREATE TABLE customers (
  customer_id VARCHAR PRIMARY KEY,
  name VARCHAR,
  plan VARCHAR,
  mrr DECIMAL
) WITH (topic = 'customers', format = 'json');

-- Enrich orders with customer data
SELECT
  o.order_id,
  o.amount,
  c.name,
  c.plan,
  c.mrr
FROM orders o
JOIN customers c ON o.customer_id = c.customer_id;
\`\`\`

The table is backed by a compacted topic — StreamHouse maintains the latest value for each key in memory. When an order arrives, the join lookups are O(1) hash map accesses.

## Performance

SQL processing runs directly within the StreamHouse agent process. No external clusters, no network hops, no serialization overhead.

Benchmarks on a single agent (4 CPU, 4GB RAM):

- **Simple filter + project**: 500,000 events/sec
- **Tumbling window aggregation**: 200,000 events/sec
- **Stream-table join**: 150,000 events/sec
- **Complex multi-join**: 50,000 events/sec

Processing latency adds <1ms for simple queries and <5ms for windowed aggregations.

## When to Use SQL vs. External Processing

StreamHouse SQL is ideal for:
- Filtering and transforming events
- Windowed aggregations (counts, sums, averages)
- Enrichment joins with lookup tables
- Real-time dashboards and alerting

For complex event processing (CEP), machine learning inference, or stateful computations that don't map to SQL, external processors like Flink remain the better choice. StreamHouse integrates cleanly as the transport layer in those architectures.

## Getting Started

\`\`\`bash
# Interactive SQL shell
streamctl sql --interactive

# Run a query from the CLI
streamctl sql "SELECT count(*) FROM clickstream WHERE page = '/pricing'"

# Create a continuous query via the REST API
curl -X POST http://localhost:8080/api/sql \\
  -H "Content-Type: application/json" \\
  -d '{"query": "CREATE CONTINUOUS QUERY ..."}'
\`\`\`

Or use the web console's SQL Workbench for a visual query editor with auto-complete and result visualization.

SQL on streams isn't the future — it's available now in StreamHouse.
    `,
    author: {
      name: "Emily Zhang",
      avatar: "EZ",
      role: "Solutions Architect",
    },
    publishedAt: "2024-02-26",
    readingTime: "10 min read",
    category: "Deep Dive",
    tags: ["sql", "datafusion", "stream-processing", "windows", "joins"],
  },
  {
    slug: "log-aggregation-use-case",
    title: "Replacing Your Log Pipeline: StreamHouse for Centralized Log Aggregation",
    excerpt:
      "Elasticsearch is expensive. Kafka + Fluentd is complex. Here's how StreamHouse replaces your entire log pipeline with a single system — ingest, store, query, and alert on logs with SQL.",
    content: `
## The Log Pipeline Problem

Most companies run a log pipeline that looks like this:

\`\`\`
Apps → Fluentd/Vector → Kafka → Logstash → Elasticsearch → Kibana
\`\`\`

That's five systems to manage, each with its own configuration, scaling concerns, and failure modes. Elasticsearch alone can cost $10,000+/month for moderate log volumes, and operating it requires dedicated expertise.

What if you could replace this entire stack with one system?

## StreamHouse as a Log Platform

StreamHouse's combination of cheap S3 storage, built-in SQL, and high-throughput ingestion makes it a natural fit for log aggregation:

\`\`\`
Apps → StreamHouse → SQL Queries + Grafana
\`\`\`

### Why It Works

**Cost**: Logs stored in S3 at $0.023/GB/month with LZ4 compression (typically 8x for text logs). 1TB of raw logs becomes ~125GB on S3, costing ~$3/month. Compare that to Elasticsearch at $50-100/GB/month.

**Throughput**: A single agent ingests 50,000+ log lines/sec. Three agents handle 150,000+ logs/sec — enough for most organizations.

**Retention**: Store months or years of logs cheaply. Set different retention for different log topics:

\`\`\`bash
# Debug logs: keep 7 days
streamctl topic create --name logs-debug --partitions 6 --retention 7d --compression zstd

# Application logs: keep 30 days
streamctl topic create --name logs-app --partitions 12 --retention 30d --compression lz4

# Audit logs: keep forever
streamctl topic create --name logs-audit --partitions 6 --retention infinite --compression zstd
\`\`\`

**Queryability**: SQL. No query DSL to learn.

## Setting It Up

### Step 1: Ship Logs to StreamHouse

Use Vector (our recommended log collector) or any HTTP/gRPC client:

\`\`\`toml
# vector.toml
[sources.app_logs]
type = "file"
include = ["/var/log/app/*.log"]

[transforms.parse]
type = "remap"
inputs = ["app_logs"]
source = '''
. = parse_json!(.message)
.timestamp = now()
.hostname = get_hostname!()
'''

[sinks.streamhouse]
type = "http"
inputs = ["parse"]
uri = "http://streamhouse:8080/api/topics/logs-app/produce"
encoding.codec = "json"
\`\`\`

### Step 2: Create a SQL Stream

\`\`\`sql
CREATE STREAM app_logs (
  timestamp TIMESTAMP,
  hostname VARCHAR,
  service VARCHAR,
  level VARCHAR,
  message VARCHAR,
  trace_id VARCHAR,
  duration_ms DOUBLE
) WITH (topic = 'logs-app', format = 'json');
\`\`\`

### Step 3: Query Your Logs

\`\`\`sql
-- Find errors in the last hour
SELECT timestamp, service, message
FROM app_logs
WHERE level = 'error'
  AND timestamp > NOW() - INTERVAL '1 hour'
ORDER BY timestamp DESC
LIMIT 100;

-- Error rate per service per 5 minutes
SELECT
  service,
  count(*) FILTER (WHERE level = 'error') as errors,
  count(*) as total,
  round(100.0 * count(*) FILTER (WHERE level = 'error') / count(*), 2) as error_rate_pct,
  window_start
FROM TUMBLE(app_logs, timestamp, INTERVAL '5 minutes')
GROUP BY service, window_start, window_end
ORDER BY error_rate_pct DESC;

-- Slow requests by endpoint
SELECT
  message,
  avg(duration_ms) as avg_duration,
  max(duration_ms) as max_duration,
  count(*) as request_count
FROM app_logs
WHERE duration_ms > 1000
GROUP BY message
ORDER BY avg_duration DESC;
\`\`\`

### Step 4: Set Up Alerts

Create continuous queries that detect anomalies and write to an alerts topic:

\`\`\`sql
CREATE CONTINUOUS QUERY error_spike_detector AS
  SELECT
    service,
    count(*) as error_count,
    window_start
  FROM TUMBLE(app_logs, timestamp, INTERVAL '1 minute')
  WHERE level = 'error'
  GROUP BY service, window_start, window_end
  HAVING count(*) > 100
  OUTPUT TO 'alerts-error-spikes';
\`\`\`

Connect the alerts topic to PagerDuty, Slack, or any webhook endpoint.

## Cost Comparison

For 500GB/day of raw logs (a mid-size company):

| Component | ELK Stack | StreamHouse |
|---|---|---|
| Ingestion | Kafka + Logstash: $800/mo | Included |
| Storage (30d) | Elasticsearch: $7,500/mo | S3: $45/mo |
| Compute | ES nodes: $2,000/mo | 3 agents: $300/mo |
| **Total** | **$10,300/mo** | **$345/mo** |

That's a 97% cost reduction. Even accounting for S3 API costs (PUT/GET), StreamHouse comes in at under $500/month for this workload.

## Trade-offs

StreamHouse isn't a full-text search engine. It won't give you Elasticsearch's relevance ranking or fuzzy matching. For most log use cases — finding errors, tracking requests, computing metrics — SQL is more than sufficient.

If you need full-text search on a subset of logs, consider routing high-value logs to a smaller Elasticsearch cluster while sending everything to StreamHouse for cheap, long-term retention.

## Getting Started

\`\`\`bash
# 1. Deploy StreamHouse
docker compose up -d

# 2. Create log topics
streamctl topic create --name logs-app --partitions 12 --retention 30d --compression lz4

# 3. Point your log shipper at StreamHouse
# 4. Query with SQL
streamctl sql --interactive
\`\`\`

Your logs deserve better than a $10,000/month Elasticsearch bill.
    `,
    author: {
      name: "Marcus Johnson",
      avatar: "MJ",
      role: "Developer Advocate",
    },
    publishedAt: "2024-03-05",
    readingTime: "8 min read",
    category: "Use Case",
    tags: ["logs", "observability", "use-case", "cost-savings", "sql"],
  },
  {
    slug: "schema-evolution-without-pain",
    title: "Schema Evolution Without the Pain: StreamHouse's Built-In Schema Registry",
    excerpt:
      "Schemas change. Producers and consumers deploy at different times. Here's how StreamHouse's Schema Registry handles Avro, Protobuf, and JSON Schema evolution with automatic compatibility checking.",
    content: `
## The Schema Problem

You have a topic called \`user-events\`. A producer writes events with fields \`user_id\`, \`event\`, and \`timestamp\`. Six months later, you add a \`country\` field. A year later, you rename \`event\` to \`event_type\`.

What happens to consumers still reading old data? What happens when a producer accidentally drops a required field?

Without schema management, the answer is: silent data corruption, broken pipelines, and 3am pages.

## StreamHouse Schema Registry

StreamHouse includes a built-in Schema Registry that provides:

- **Schema versioning**: Every schema change gets a version number
- **Compatibility checking**: Changes are validated against configurable rules before registration
- **Producer validation**: Messages are validated against the schema at write time
- **Multi-format support**: Avro, Protobuf, and JSON Schema

The registry is API-compatible with the Confluent Schema Registry, so existing tools and client libraries work out of the box.

## Registering a Schema

\`\`\`bash
# Register a JSON Schema for user-events
curl -X POST http://localhost:8081/subjects/user-events-value/versions \\
  -H "Content-Type: application/json" \\
  -d '{
    "schemaType": "JSON",
    "schema": "{
      \\"type\\": \\"object\\",
      \\"required\\": [\\"user_id\\", \\"event\\", \\"timestamp\\"],
      \\"properties\\": {
        \\"user_id\\": {\\"type\\": \\"string\\"},
        \\"event\\": {\\"type\\": \\"string\\"},
        \\"timestamp\\": {\\"type\\": \\"integer\\"}
      }
    }"
  }'

# Response: {"id": 1}
\`\`\`

The schema gets an ID (1) that's embedded in every message produced with this schema. Consumers use the ID to fetch the schema for deserialization.

## Compatibility Modes

The key feature of any schema registry is compatibility checking. When you register a new version of a schema, the registry validates it against the previous version(s):

### Backward Compatible (Default)

New schema can read data written with the old schema. You can:
- Add optional fields (with defaults)
- Remove fields

You cannot:
- Add required fields
- Change field types

\`\`\`json
// Version 1
{"user_id": "string", "event": "string", "timestamp": "integer"}

// Version 2 (backward compatible - added optional field)
{"user_id": "string", "event": "string", "timestamp": "integer", "country": "string (optional)"}
\`\`\`

### Forward Compatible

Old schema can read data written with the new schema. You can:
- Remove optional fields
- Add fields

You cannot:
- Remove required fields
- Change field types

### Full Compatible

Both backward and forward compatible. The safest mode but the most restrictive.

\`\`\`bash
# Set compatibility mode for a subject
curl -X PUT http://localhost:8081/config/user-events-value \\
  -H "Content-Type: application/json" \\
  -d '{"compatibility": "FULL"}'
\`\`\`

## Producer-Side Validation

When a producer sends a message with a schema ID, the StreamHouse agent validates the payload before accepting it:

\`\`\`bash
# Produce with schema validation
streamctl produce --topic user-events \\
  --schema-id 1 \\
  --key "user-123" \\
  --message '{"user_id": "123", "event": "login", "timestamp": 1706000000}'
# Success

# This will be rejected — missing required field
streamctl produce --topic user-events \\
  --schema-id 1 \\
  --key "user-123" \\
  --message '{"user_id": "123"}'
# Error: Schema validation failed: missing required field "event"
\`\`\`

Catching bad data at the producer is far cheaper than debugging it downstream.

## Schema Caching

The Schema Registry caches schemas in memory for fast lookups. With a 10,000-schema cache and 1-hour TTL, the cache achieves ~99% hit rates in production. A cache miss costs a single PostgreSQL query (~2ms).

\`\`\`
Schema lookup path:
  1. Check in-memory cache → ~100ns (99% of requests)
  2. Query PostgreSQL → ~2ms (1% of requests)
  3. Return schema + update cache
\`\`\`

## Avro and Protobuf Support

While JSON Schema is the simplest to get started with, Avro and Protobuf offer better performance for production workloads:

**Avro**: Binary format with schema embedded in the file header. Excellent for evolving schemas because readers can use the writer's schema for deserialization.

**Protobuf**: Google's binary format. Best for gRPC-heavy environments and when you need language-neutral schema definitions.

\`\`\`bash
# Register an Avro schema
curl -X POST http://localhost:8081/subjects/orders-value/versions \\
  -H "Content-Type: application/json" \\
  -d '{
    "schemaType": "AVRO",
    "schema": "{
      \\"type\\": \\"record\\",
      \\"name\\": \\"Order\\",
      \\"fields\\": [
        {\\"name\\": \\"order_id\\", \\"type\\": \\"string\\"},
        {\\"name\\": \\"amount\\", \\"type\\": \\"double\\"},
        {\\"name\\": \\"currency\\", \\"type\\": \\"string\\", \\"default\\": \\"USD\\"}
      ]
    }"
  }'
\`\`\`

## Best Practices

1. **Always use backward compatibility** for consumer-first deployments (most common)
2. **Use full compatibility** for topics shared across many teams
3. **Register schemas before producing** — don't rely on auto-registration in production
4. **Version your schemas in source control** alongside your application code
5. **Monitor compatibility check failures** — they indicate coordination problems between teams

## The Bottom Line

Schema management isn't optional at scale. StreamHouse's built-in Schema Registry makes it free to adopt — no additional infrastructure, no additional service to operate. Register schemas, enforce compatibility, and catch data quality issues before they reach your consumers.

\`\`\`bash
# Get started in 30 seconds
curl http://localhost:8081/subjects  # List all registered schemas
\`\`\`
    `,
    author: {
      name: "Sarah Kim",
      avatar: "SK",
      role: "Co-founder & CTO",
    },
    publishedAt: "2024-03-12",
    readingTime: "9 min read",
    category: "Deep Dive",
    tags: ["schema-registry", "avro", "data-quality", "compatibility", "evolution"],
  },
  {
    slug: "cdc-pipelines-streamhouse",
    title: "Real-Time CDC Pipelines: Streaming Database Changes with StreamHouse",
    excerpt:
      "Every INSERT, UPDATE, and DELETE in your database can be a stream event. Here's how to build Change Data Capture pipelines with StreamHouse for real-time data synchronization, search indexing, and cache invalidation.",
    content: `
## What is CDC?

Change Data Capture (CDC) turns database changes into a stream of events. Every time a row is inserted, updated, or deleted, a corresponding event is emitted. This enables real-time data synchronization between systems without polling or batch ETL.

\`\`\`
PostgreSQL → CDC → StreamHouse → Downstream Systems
                                    ├── Elasticsearch (search index)
                                    ├── Redis (cache invalidation)
                                    ├── Data warehouse (analytics)
                                    └── Other microservices
\`\`\`

## Why StreamHouse for CDC?

CDC generates high-volume, ordered event streams — exactly what StreamHouse is built for. Key advantages:

- **Ordered delivery**: CDC events must be processed in order per key. StreamHouse's partition-key ordering guarantees this.
- **Replay**: If a downstream system needs to rebuild its state, consumers can replay from the beginning of the topic.
- **Fan-out**: Multiple downstream systems can independently consume the same CDC stream.
- **Cost**: CDC data is often high-volume but rarely queried. S3 storage at $0.023/GB is perfect.
- **SQL processing**: Filter, transform, and aggregate CDC events using SQL before they reach downstream systems.

## Setting Up CDC with Debezium

[Debezium](https://debezium.io/) is the standard open-source CDC tool. It captures changes from PostgreSQL's WAL (Write-Ahead Log) and produces them to a streaming platform.

### Step 1: Configure PostgreSQL for Logical Replication

\`\`\`sql
-- In postgresql.conf
wal_level = logical
max_replication_slots = 4
max_wal_senders = 4

-- Create a replication user
CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD 'secret';
GRANT SELECT ON ALL TABLES IN SCHEMA public TO replicator;
\`\`\`

### Step 2: Create StreamHouse Topics

Create topics that mirror your database tables:

\`\`\`bash
# One topic per table, keyed by primary key
streamctl topic create --name cdc.public.users --partitions 6 --retention 30d
streamctl topic create --name cdc.public.orders --partitions 12 --retention 30d
streamctl topic create --name cdc.public.products --partitions 6 --retention 30d
\`\`\`

### Step 3: Configure Debezium

\`\`\`json
{
  "name": "pg-source",
  "config": {
    "connector.class": "io.debezium.connector.postgresql.PostgresConnector",
    "database.hostname": "postgres",
    "database.port": "5432",
    "database.user": "replicator",
    "database.password": "secret",
    "database.dbname": "myapp",
    "topic.prefix": "cdc",
    "plugin.name": "pgoutput",
    "publication.autocreate.mode": "filtered",
    "table.include.list": "public.users,public.orders,public.products"
  }
}
\`\`\`

### Step 4: Consume CDC Events

CDC events include the full row data plus metadata:

\`\`\`json
{
  "op": "u",
  "before": {"id": 1, "name": "Alice", "email": "alice@old.com"},
  "after": {"id": 1, "name": "Alice", "email": "alice@new.com"},
  "source": {
    "table": "users",
    "lsn": 123456789,
    "txId": 42
  },
  "ts_ms": 1706000000000
}
\`\`\`

The \`op\` field indicates the operation: \`c\` (create), \`u\` (update), \`d\` (delete), \`r\` (snapshot read).

## Use Case 1: Search Index Sync

Keep Elasticsearch in sync with your database:

\`\`\`sql
-- Create a continuous query that transforms CDC events for Elasticsearch
CREATE CONTINUOUS QUERY user_search_updates AS
  SELECT
    after->>'id' as user_id,
    after->>'name' as name,
    after->>'email' as email,
    after->>'bio' as bio,
    CASE op
      WHEN 'd' THEN 'delete'
      ELSE 'upsert'
    END as action
  FROM cdc_users
  WHERE op IN ('c', 'u', 'd')
  OUTPUT TO 'search-user-updates';
\`\`\`

A downstream consumer reads from \`search-user-updates\` and applies changes to Elasticsearch.

## Use Case 2: Cache Invalidation

Invalidate Redis cache entries when database rows change:

\`\`\`sql
CREATE CONTINUOUS QUERY cache_invalidations AS
  SELECT
    'product:' || (after->>'id') as cache_key,
    op as operation
  FROM cdc_products
  OUTPUT TO 'cache-invalidations';
\`\`\`

A consumer reads from \`cache-invalidations\` and calls \`DEL\` on the corresponding Redis keys. This is faster and more reliable than TTL-based cache expiry.

## Use Case 3: Real-Time Analytics

Stream database changes to your data warehouse without batch ETL:

\`\`\`sql
-- Aggregate order metrics in real-time
CREATE CONTINUOUS QUERY order_metrics AS
  SELECT
    count(*) FILTER (WHERE op = 'c') as new_orders,
    sum(CAST(after->>'amount' AS DOUBLE)) FILTER (WHERE op = 'c') as total_revenue,
    window_start
  FROM TUMBLE(cdc_orders, ts_ms, INTERVAL '1 minute')
  GROUP BY window_start, window_end
  OUTPUT TO 'order-metrics-per-minute';
\`\`\`

## Ordering Guarantees

CDC events must be processed in order for each row (primary key). StreamHouse guarantees this because:

1. Debezium produces events with the primary key as the message key
2. StreamHouse routes all events with the same key to the same partition
3. Within a partition, events are strictly ordered
4. Consumer groups assign entire partitions (not individual keys) to consumers

This means all changes to \`user_id=123\` are processed by the same consumer in the exact order they occurred in the database.

## Handling Schema Changes

When your database schema evolves (ALTER TABLE), Debezium emits events with the new schema. StreamHouse's Schema Registry can validate these changes:

\`\`\`bash
# Set backward compatibility on CDC topics
curl -X PUT http://localhost:8081/config/cdc.public.users-value \\
  -H "Content-Type: application/json" \\
  -d '{"compatibility": "BACKWARD"}'
\`\`\`

Adding nullable columns is always backward compatible. Removing columns or changing types will be caught by the compatibility check before any bad data reaches consumers.

## Getting Started

\`\`\`bash
# 1. Deploy StreamHouse + Debezium
docker compose up -d

# 2. Register the Debezium connector
curl -X POST http://debezium:8083/connectors -d @connector.json

# 3. Watch CDC events flow
streamctl consume --topic cdc.public.users --from-beginning --format json
\`\`\`

CDC with StreamHouse gives you a real-time, replayable, SQL-queryable mirror of every change in your database. Build search indexes, invalidate caches, feed analytics, and sync microservices — all from a single, ordered event stream.
    `,
    author: {
      name: "Emily Zhang",
      avatar: "EZ",
      role: "Solutions Architect",
    },
    publishedAt: "2024-03-19",
    readingTime: "10 min read",
    category: "Use Case",
    tags: ["cdc", "postgresql", "debezium", "use-case", "data-sync"],
  },
];

export function getBlogPost(slug: string): BlogPost | undefined {
  return blogPosts.find((post) => post.slug === slug);
}

export function getAllBlogPosts(): BlogPost[] {
  return blogPosts.sort(
    (a, b) => new Date(b.publishedAt).getTime() - new Date(a.publishedAt).getTime()
  );
}

export function getFeaturedPost(): BlogPost | undefined {
  return blogPosts.find((post) => post.featured);
}

export function getPostsByCategory(category: string): BlogPost[] {
  return blogPosts.filter((post) => post.category === category);
}
