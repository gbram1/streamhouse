# Project Plan: Unified S3-Native Streaming Platform (Revised)

## Executive Summary

**Project Name:** [TBD - suggestions: Conduit, Tributary, Flume, Rivulet]

**Vision:** Build a unified data streaming platform that replaces both Kafka (transport) and Flink (processing) with a single S3-native system written in Rust.

**Key Insight:** Existing streaming processors (Materialize, RisingWave, Arroyo) all require Kafka. They reduce complexity in one layer while leaving the expensive, complex layer (Kafka) untouched. The real value is replacing Kafka first, then adding processing as a native capability.

**Core Pitch:**
- Month 4: "Kafka replacement, 80% cheaper"
- Month 10: "Kafka + Flink in one system"
- Month 14: "Drop-in replacement with built-in SQL processing"

**Timeline:** 18-24 months to production-ready with paying customers

**Founder Situation:** Solo, enjoys hard problems, no hard time constraints

---

## Why Storage First

| Approach | What you are | Market outcome |
|----------|--------------|----------------|
| Processing only (needs Kafka) | Another Materialize | Struggling market |
| Storage only (Kafka replacement) | Another WarpStream | Acquired for $220M |
| Storage + Processing integrated | Something new | Differentiated |

**The lesson from Materialize:** They built impressive technology but left the expensive piece (Kafka) in place. Customers paid two bills, managed two systems. "Nice to have" not "must have."

**The lesson from WarpStream:** They replaced the expensive piece. Clear ROI: "80% cheaper." Acquired in 18 months for $220M.

**Your approach:** Start with storage (clear value), add processing (differentiation), end up with something nobody else has.

---

## Roadmap Overview

```
         STORAGE LAYER              PROCESSING             PRODUCT
    ┌─────────────────────┐   ┌─────────────────────┐   ┌──────────────┐
    │  Months 1-4         │   │  Months 5-10        │   │ Months 11-14 │
    │                     │   │                     │   │              │
    │  S3-native log      │   │  SQL processing     │   │ Kafka proto  │
    │  Basic produce/     │──▶│  Windows, aggs      │──▶│ Cloud deploy │
    │  consume            │   │  State management   │   │ UI           │
    │  Kafka protocol     │   │  Checkpointing      │   │              │
    │  (basic)            │   │                     │   │              │
    └─────────────────────┘   └─────────────────────┘   └──────────────┘
           │                          │                        │
           ▼                          ▼                        ▼
    "Kafka replacement"      "Integrated platform"      "Production ready"
     Ship & get users         Differentiated             Paying customers


    GROWTH & SCALE
    ┌─────────────────────┐
    │  Months 15-18+      │
    │                     │
    │  Advanced features  │
    │  Enterprise         │
    │  Scale & optimize   │
    │                     │
    └─────────────────────┘
           │
           ▼
    "Mature product"
```

---

## Phase Breakdown

### Phase 1: Core Storage Layer (Months 1-2)
**Goal:** S3-native append-only log that can store and retrieve events

### Phase 2: Basic Kafka Compatibility (Months 3-4)
**Goal:** Kafka protocol support so existing apps can use your system

### Phase 3: Processing Foundation (Months 5-7)
**Goal:** SQL engine that processes streams from your storage layer

### Phase 4: Distributed Processing (Months 8-10)
**Goal:** Multi-node processing with fault tolerance

### Phase 5: Product Polish (Months 11-14)
**Goal:** Full Kafka compatibility, UI, cloud deployment

### Phase 6: Launch & Growth (Months 15-18+)
**Goal:** Paying customers, enterprise features

---

## Phase 1: Core Storage Layer (Months 1-2)

**Goal:** Build an S3-native append-only log. Events go in, events come out.

### What You're Building

```
┌─────────────────────────────────────────────────────────────────┐
│                         YOUR SYSTEM                             │
│                                                                 │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │   Ingest    │────▶│   Segment   │────▶│    Read     │       │
│  │   Service   │     │   Manager   │     │   Service   │       │
│  └─────────────┘     └─────────────┘     └─────────────┘       │
│         │                   │                   │               │
│         │                   ▼                   │               │
│         │           ┌─────────────┐             │               │
│         │           │  Metadata   │             │               │
│         │           │   Store     │             │               │
│         │           │  (SQLite)   │             │               │
│         │           └─────────────┘             │               │
│         │                   │                   │               │
│         └───────────────────┼───────────────────┘               │
│                             ▼                                   │
│                      ┌─────────────┐                            │
│                      │     S3      │                            │
│                      │  (segments) │                            │
│                      └─────────────┘                            │
└─────────────────────────────────────────────────────────────────┘
```

### Initiative 1.1: Project Setup (Week 1)

**Tasks:**
- [ ] Create GitHub repository
- [ ] Initialize Rust workspace with cargo workspaces
- [ ] Set up CI/CD (GitHub Actions: build, test, clippy, fmt)
- [ ] Create initial crate structure
- [ ] Write README with vision and goals
- [ ] Set up development environment (Docker for S3/MinIO)

**Crate structure:**
```
streaming-platform/
├── Cargo.toml                 # Workspace root
├── crates/
│   ├── core/                  # Shared types, errors, config
│   ├── storage/               # S3 storage layer
│   ├── metadata/              # Topic/partition metadata
│   ├── server/                # API server (gRPC/HTTP)
│   └── cli/                   # Command-line tool
```

**Deliverables:**
- [ ] Running CI pipeline
- [ ] Basic project compiles
- [ ] MinIO running locally in Docker
- [ ] README explaining what you're building

---

### Initiative 1.2: Segment Format Design (Week 1-2)

**Tasks:**
- [ ] Design segment file format
- [ ] Implement segment writer
- [ ] Implement segment reader
- [ ] Write unit tests
- [ ] Benchmark write performance

**Segment format:**
```
┌────────────────────────────────────────────────────┐
│                  SEGMENT FILE                       │
├────────────────────────────────────────────────────┤
│  Header (32 bytes)                                 │
│  ├── Magic number (4 bytes): "STRM"               │
│  ├── Version (2 bytes): 1                         │
│  ├── Flags (2 bytes)                              │
│  ├── Base offset (8 bytes): first offset in file  │
│  ├── Record count (4 bytes)                       │
│  ├── Created timestamp (8 bytes)                  │
│  └── Reserved (4 bytes)                           │
├────────────────────────────────────────────────────┤
│  Record Batch 1                                    │
│  ├── Batch length (4 bytes)                       │
│  ├── Record count (2 bytes)                       │
│  ├── First offset delta (4 bytes)                 │
│  ├── Timestamps (compressed)                       │
│  ├── Keys (length-prefixed, compressed)           │
│  └── Values (length-prefixed, compressed)         │
├────────────────────────────────────────────────────┤
│  Record Batch 2                                    │
│  └── ...                                          │
├────────────────────────────────────────────────────┤
│  Index (at end of file)                           │
│  ├── Offset -> byte position mapping              │
│  └── Timestamp -> offset mapping                  │
├────────────────────────────────────────────────────┤
│  Footer (32 bytes)                                │
│  ├── Index offset (8 bytes)                       │
│  ├── CRC32 checksum (4 bytes)                     │
│  └── Magic number (4 bytes): "MRTS"               │
└────────────────────────────────────────────────────┘
```

**Key decisions:**
- Use LZ4 compression (fast)
- Batch records together (better compression, fewer S3 calls)
- Index at end (single sequential write)
- Target segment size: 64-256MB

**Code interface:**
```rust
// crates/storage/src/segment.rs

pub struct SegmentWriter {
    buffer: Vec<RecordBatch>,
    base_offset: u64,
    config: SegmentConfig,
}

impl SegmentWriter {
    pub fn new(base_offset: u64, config: SegmentConfig) -> Self;
    pub fn append(&mut self, record: Record) -> Result<u64>; // returns offset
    pub fn should_roll(&self) -> bool; // size/time threshold
    pub async fn flush(&mut self, s3: &S3Client) -> Result<SegmentInfo>;
}

pub struct SegmentReader {
    data: Bytes,  // memory-mapped or downloaded
    index: SegmentIndex,
}

impl SegmentReader {
    pub async fn open(s3: &S3Client, path: &str) -> Result<Self>;
    pub fn read_at(&self, offset: u64) -> Result<Record>;
    pub fn read_range(&self, start: u64, end: u64) -> Result<Vec<Record>>;
    pub fn iter_from(&self, offset: u64) -> impl Iterator<Item = Record>;
}
```

**Deliverables:**
- [ ] SegmentWriter that creates valid segment files
- [ ] SegmentReader that reads them back
- [ ] Unit tests proving correctness
- [ ] Benchmark: writes/sec, read latency

---

### Initiative 1.3: Metadata Store (Week 2-3)

**Tasks:**
- [ ] Design metadata schema
- [ ] Implement metadata store (SQLite first)
- [ ] Topic CRUD operations
- [ ] Partition management
- [ ] Segment tracking

**Metadata schema:**
```sql
-- Topics
CREATE TABLE topics (
    name TEXT PRIMARY KEY,
    partition_count INTEGER NOT NULL,
    retention_ms BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    config JSON
);

-- Partitions
CREATE TABLE partitions (
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    high_watermark BIGINT DEFAULT 0,  -- latest offset
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (topic) REFERENCES topics(name)
);

-- Segments
CREATE TABLE segments (
    id TEXT PRIMARY KEY,  -- UUID
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    base_offset BIGINT NOT NULL,
    end_offset BIGINT NOT NULL,
    size_bytes BIGINT NOT NULL,
    s3_path TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id)
);

-- Consumer offsets
CREATE TABLE consumer_offsets (
    group_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    committed_offset BIGINT NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (group_id, topic, partition_id)
);
```

**Code interface:**
```rust
// crates/metadata/src/lib.rs

#[async_trait]
pub trait MetadataStore: Send + Sync {
    // Topics
    async fn create_topic(&self, topic: &TopicConfig) -> Result<()>;
    async fn delete_topic(&self, name: &str) -> Result<()>;
    async fn get_topic(&self, name: &str) -> Result<Option<Topic>>;
    async fn list_topics(&self) -> Result<Vec<Topic>>;
    
    // Partitions
    async fn get_partition(&self, topic: &str, partition: u32) -> Result<Partition>;
    async fn update_high_watermark(&self, topic: &str, partition: u32, offset: u64) -> Result<()>;
    
    // Segments
    async fn add_segment(&self, segment: &SegmentInfo) -> Result<()>;
    async fn get_segments(&self, topic: &str, partition: u32) -> Result<Vec<SegmentInfo>>;
    async fn find_segment_for_offset(&self, topic: &str, partition: u32, offset: u64) -> Result<Option<SegmentInfo>>;
    
    // Consumer offsets
    async fn commit_offset(&self, group: &str, topic: &str, partition: u32, offset: u64) -> Result<()>;
    async fn get_committed_offset(&self, group: &str, topic: &str, partition: u32) -> Result<Option<u64>>;
}
```

**Deliverables:**
- [ ] SQLite implementation of MetadataStore
- [ ] All CRUD operations working
- [ ] Unit tests
- [ ] Later: can swap to Postgres/FoundationDB for distributed deployment

---

### Initiative 1.4: Write Path (Week 3-4)

**Tasks:**
- [ ] Implement producer/ingest service
- [ ] Batching buffer (collect records, write periodically)
- [ ] Segment rolling (by size and time)
- [ ] Offset assignment
- [ ] S3 upload with retries

**Write flow:**
```
Record arrives
     │
     ▼
┌─────────────┐
│  Validate   │ (topic exists, record size OK)
└─────────────┘
     │
     ▼
┌─────────────┐
│  Assign     │ (partition based on key hash or round-robin)
│  Partition  │
└─────────────┘
     │
     ▼
┌─────────────┐
│  Buffer     │ (in-memory, per-partition)
│  Record     │
└─────────────┘
     │
     ├── Buffer full OR time elapsed
     │
     ▼
┌─────────────┐
│  Write      │ (flush buffer to segment)
│  Segment    │
└─────────────┘
     │
     ├── Segment size threshold reached
     │
     ▼
┌─────────────┐
│  Upload     │ (segment to S3)
│  to S3      │
└─────────────┘
     │
     ▼
┌─────────────┐
│  Update     │ (record segment in metadata)
│  Metadata   │
└─────────────┘
     │
     ▼
┌─────────────┐
│  Ack to     │ (return offset to producer)
│  Producer   │
└─────────────┘
```

**Configuration:**
```rust
pub struct WriteConfig {
    pub buffer_size: usize,           // 1MB default
    pub buffer_flush_ms: u64,         // 100ms default
    pub segment_max_size: usize,      // 64MB default
    pub segment_max_age_ms: u64,      // 10 minutes default
    pub s3_upload_timeout_ms: u64,    // 30 seconds
    pub s3_upload_retries: u32,       // 3
}
```

**Code interface:**
```rust
// crates/storage/src/writer.rs

pub struct PartitionWriter {
    topic: String,
    partition: u32,
    buffer: SegmentWriter,
    metadata: Arc<dyn MetadataStore>,
    s3: Arc<S3Client>,
    config: WriteConfig,
}

impl PartitionWriter {
    pub async fn append(&mut self, key: Option<&[u8]>, value: &[u8]) -> Result<u64>;
    pub async fn flush(&mut self) -> Result<()>;
}

pub struct TopicWriter {
    partitions: Vec<PartitionWriter>,
}

impl TopicWriter {
    pub async fn append(&mut self, key: Option<&[u8]>, value: &[u8]) -> Result<(u32, u64)>;
}
```

**Deliverables:**
- [ ] Records can be written and end up in S3
- [ ] Offsets are assigned correctly
- [ ] Segments roll at correct thresholds
- [ ] Metadata is updated
- [ ] Integration test: write 100K records, verify in S3

---

### Initiative 1.5: Read Path (Week 4-5)

**Tasks:**
- [ ] Implement consumer/read service
- [ ] Segment lookup by offset
- [ ] Segment caching (local disk)
- [ ] Sequential read optimization
- [ ] Prefetching

**Read flow:**
```
Consumer requests offset 15000
           │
           ▼
    ┌─────────────┐
    │  Find       │ (query metadata for segment containing offset)
    │  Segment    │
    └─────────────┘
           │
           ▼
    ┌─────────────┐
    │  Check      │ (is segment in local cache?)
    │  Cache      │
    └─────────────┘
         │    │
    cache│    │cache miss
     hit │    │
         │    ▼
         │  ┌─────────────┐
         │  │  Download   │ (fetch from S3)
         │  │  Segment    │
         │  └─────────────┘
         │         │
         │         ▼
         │  ┌─────────────┐
         │  │  Cache      │ (store locally)
         │  │  Segment    │
         │  └─────────────┘
         │         │
         └────┬────┘
              │
              ▼
       ┌─────────────┐
       │  Read from  │ (use segment index)
       │  Offset     │
       └─────────────┘
              │
              ▼
       ┌─────────────┐
       │  Return     │
       │  Records    │
       └─────────────┘
              │
              ▼
       ┌─────────────┐
       │  Prefetch   │ (background: next segment)
       │  Next       │
       └─────────────┘
```

**Cache design:**
```rust
pub struct SegmentCache {
    cache_dir: PathBuf,
    max_size_bytes: u64,
    current_size: AtomicU64,
    segments: DashMap<String, CachedSegment>,
}

impl SegmentCache {
    pub async fn get(&self, segment_id: &str, s3: &S3Client) -> Result<Arc<SegmentReader>>;
    pub fn evict_lru(&self);
}
```

**Deliverables:**
- [ ] Consumer can read from any offset
- [ ] Segment caching working
- [ ] Prefetching improves sequential read performance
- [ ] Benchmark: read latency (cached vs uncached)

---

### Initiative 1.6: Basic API Server (Week 5-6)

**Tasks:**
- [ ] gRPC service definition
- [ ] Implement produce RPC
- [ ] Implement consume RPC
- [ ] Implement admin RPCs (create topic, etc.)
- [ ] Health checks

**Proto definition:**
```protobuf
syntax = "proto3";
package streaming.v1;

service StreamingService {
    // Produce
    rpc Produce(ProduceRequest) returns (ProduceResponse);
    rpc ProduceStream(stream ProduceRequest) returns (stream ProduceResponse);
    
    // Consume
    rpc Consume(ConsumeRequest) returns (stream ConsumeResponse);
    
    // Admin
    rpc CreateTopic(CreateTopicRequest) returns (CreateTopicResponse);
    rpc DeleteTopic(DeleteTopicRequest) returns (DeleteTopicResponse);
    rpc ListTopics(ListTopicsRequest) returns (ListTopicsResponse);
    rpc GetTopicMetadata(GetTopicMetadataRequest) returns (GetTopicMetadataResponse);
    
    // Consumer groups
    rpc CommitOffset(CommitOffsetRequest) returns (CommitOffsetResponse);
    rpc GetCommittedOffset(GetCommittedOffsetRequest) returns (GetCommittedOffsetResponse);
}

message ProduceRequest {
    string topic = 1;
    optional bytes key = 2;
    bytes value = 3;
    optional int32 partition = 4;  // if not set, partition by key or round-robin
}

message ProduceResponse {
    int32 partition = 1;
    int64 offset = 2;
}

message ConsumeRequest {
    string topic = 1;
    int32 partition = 2;
    int64 start_offset = 3;  // -1 for earliest, -2 for latest
    int32 max_records = 4;
    int32 max_wait_ms = 5;
}

message ConsumeResponse {
    repeated Record records = 1;
    int64 high_watermark = 2;
}

message Record {
    int64 offset = 1;
    int64 timestamp = 2;
    optional bytes key = 3;
    bytes value = 4;
}
```

**Deliverables:**
- [ ] gRPC server running
- [ ] Can produce via gRPC
- [ ] Can consume via gRPC
- [ ] Can manage topics via gRPC

---

### Initiative 1.7: CLI Tool (Week 6)

**Tasks:**
- [ ] Build CLI using clap
- [ ] Produce command
- [ ] Consume command
- [ ] Topic management commands
- [ ] Pretty output formatting

**CLI design:**
```bash
# Topic management
streamctl topic create orders --partitions 3 --retention 7d
streamctl topic list
streamctl topic describe orders
streamctl topic delete orders

# Produce
echo '{"order_id": 123}' | streamctl produce orders
streamctl produce orders --file events.jsonl
streamctl produce orders --key "user-123" --value '{"event": "click"}'

# Consume
streamctl consume orders --from-beginning
streamctl consume orders --partition 0 --offset 1000
streamctl consume orders --group my-consumer-group
streamctl consume orders --follow  # like tail -f

# Debug
streamctl segment list orders/0
streamctl segment dump s3://bucket/segments/segment-0001.seg
```

**Deliverables:**
- [ ] Fully functional CLI
- [ ] Can demo the system end-to-end
- [ ] Useful for testing and debugging

---

### Initiative 1.8: Testing & Documentation (Week 6-7)

**Tasks:**
- [ ] Integration test suite
- [ ] Performance benchmarks
- [ ] Getting started documentation
- [ ] Architecture documentation
- [ ] First blog post

**Integration tests:**
```rust
#[tokio::test]
async fn test_produce_consume_basic() {
    let server = TestServer::start().await;
    
    // Create topic
    server.create_topic("test", 1).await.unwrap();
    
    // Produce records
    for i in 0..100 {
        server.produce("test", format!("message-{}", i)).await.unwrap();
    }
    
    // Consume and verify
    let records = server.consume("test", 0, 0, 100).await.unwrap();
    assert_eq!(records.len(), 100);
    assert_eq!(records[0].value, b"message-0");
    assert_eq!(records[99].value, b"message-99");
}

#[tokio::test]
async fn test_segment_rollover() { ... }

#[tokio::test]
async fn test_consumer_offset_commit() { ... }

#[tokio::test]  
async fn test_cache_eviction() { ... }

#[tokio::test]
async fn test_restart_recovery() { ... }
```

**Benchmarks:**
```
Benchmark: Write throughput
- 1KB records, 1 partition: X records/sec
- 1KB records, 10 partitions: X records/sec
- 10KB records, 1 partition: X records/sec

Benchmark: Read latency
- Cached segment: Xms p50, Xms p99
- Uncached segment: Xms p50, Xms p99

Benchmark: End-to-end latency
- Produce to consume: Xms p50, Xms p99
```

**Deliverables:**
- [ ] Integration test suite passing
- [ ] Benchmark results documented
- [ ] README with getting started guide
- [ ] Blog post: "Building an S3-Native Streaming Platform in Rust"

---

### Phase 1 Milestones

| Week | Milestone | Deliverable |
|------|-----------|-------------|
| 1 | Project setup | CI running, MinIO local |
| 2 | Segment format | Can write/read segment files |
| 3 | Metadata store | Topics and partitions working |
| 4 | Write path | Records flow to S3 |
| 5 | Read path | Can consume from any offset |
| 6 | API + CLI | gRPC server, CLI tool |
| 7 | Testing | Integration tests, benchmarks |
| 8 | Polish | Docs, blog post, bug fixes |

**End of Phase 1:** Working S3-native log. Not Kafka-compatible yet, but functional via gRPC/CLI.

---

## Phase 2: Basic Kafka Compatibility (Months 3-4)

**Goal:** Implement enough of the Kafka protocol that existing Kafka clients can produce and consume.

### Why Kafka Protocol Matters

Without Kafka protocol:
- Users must rewrite applications to use your gRPC API
- Can't use existing Kafka ecosystem (connectors, tools)
- Adoption friction is high

With Kafka protocol:
- Existing apps work with a config change
- kafka-console-producer, kafkacat, etc. all work
- Migration is low-risk

### What You're Building

```
┌─────────────────────────────────────────────────────────────────┐
│                         YOUR SYSTEM                             │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                  Kafka Protocol Layer                    │   │
│  │                                                          │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │   │
│  │  │ Produce  │ │  Fetch   │ │ Metadata │ │ Offsets  │   │   │
│  │  │  API     │ │   API    │ │   API    │ │   API    │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   Storage Layer                          │   │
│  │                   (from Phase 1)                         │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
        ▲
        │ Kafka Protocol (TCP)
        │
┌───────┴───────┐
│  Kafka        │
│  Clients      │
│  (unmodified) │
└───────────────┘
```

### Initiative 2.1: Kafka Protocol Foundation (Week 1-2)

**Tasks:**
- [ ] Implement Kafka binary protocol codec
- [ ] Request/response framing
- [ ] API versioning support
- [ ] TCP server with connection handling
- [ ] Error code mapping

**Kafka protocol basics:**
```
Request Frame:
┌────────────────────────────────────────┐
│ Size (4 bytes) - total request size    │
├────────────────────────────────────────┤
│ API Key (2 bytes) - which API          │
│ API Version (2 bytes)                  │
│ Correlation ID (4 bytes) - request ID  │
│ Client ID (string)                     │
├────────────────────────────────────────┤
│ Request Body (varies by API)           │
└────────────────────────────────────────┘

Response Frame:
┌────────────────────────────────────────┐
│ Size (4 bytes) - total response size   │
├────────────────────────────────────────┤
│ Correlation ID (4 bytes) - matches req │
├────────────────────────────────────────┤
│ Response Body (varies by API)          │
└────────────────────────────────────────┘
```

**Code structure:**
```rust
// crates/kafka-protocol/src/codec.rs

pub struct KafkaCodec;

impl Decoder for KafkaCodec {
    type Item = KafkaRequest;
    type Error = ProtocolError;
    
    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error>;
}

impl Encoder<KafkaResponse> for KafkaCodec {
    type Error = ProtocolError;
    
    fn encode(&mut self, response: KafkaResponse, buf: &mut BytesMut) -> Result<(), Self::Error>;
}

// crates/kafka-protocol/src/messages.rs

pub enum KafkaRequest {
    Produce(ProduceRequest),
    Fetch(FetchRequest),
    Metadata(MetadataRequest),
    OffsetFetch(OffsetFetchRequest),
    OffsetCommit(OffsetCommitRequest),
    FindCoordinator(FindCoordinatorRequest),
    JoinGroup(JoinGroupRequest),
    SyncGroup(SyncGroupRequest),
    Heartbeat(HeartbeatRequest),
    LeaveGroup(LeaveGroupRequest),
    ListOffsets(ListOffsetsRequest),
    ApiVersions(ApiVersionsRequest),
    // ... more as needed
}
```

**Deliverables:**
- [ ] Protocol codec (encode/decode)
- [ ] TCP server accepting connections
- [ ] ApiVersions request working (first request any client sends)

---

### Initiative 2.2: Metadata API (Week 2-3)

**Tasks:**
- [ ] Implement Metadata request/response
- [ ] Broker information
- [ ] Topic metadata
- [ ] Partition leaders

**What clients expect:**
```
Client sends: MetadataRequest { topics: ["orders"] }

Server responds: MetadataResponse {
    brokers: [
        { node_id: 1, host: "localhost", port: 9092 }
    ],
    topics: [
        {
            name: "orders",
            partitions: [
                { partition: 0, leader: 1, replicas: [1], isr: [1] },
                { partition: 1, leader: 1, replicas: [1], isr: [1] },
                { partition: 2, leader: 1, replicas: [1], isr: [1] },
            ]
        }
    ]
}
```

**Simplification:** For single-node deployment, you're the only broker, you're the leader for everything. Multi-node comes later.

**Deliverables:**
- [ ] Metadata API working
- [ ] Clients can discover topics and partitions

---

### Initiative 2.3: Produce API (Week 3-4)

**Tasks:**
- [ ] Implement Produce request/response
- [ ] Record batch parsing
- [ ] Multiple produce protocol versions
- [ ] Acks handling (0, 1, all)

**Produce flow:**
```
Client sends:
ProduceRequest {
    acks: 1,
    timeout_ms: 30000,
    topics: [
        {
            name: "orders",
            partitions: [
                {
                    partition: 0,
                    records: RecordBatch { ... }
                }
            ]
        }
    ]
}

Server:
1. Parse record batch
2. Write to storage layer (Phase 1)
3. Wait for ack level
4. Return offsets

Server responds:
ProduceResponse {
    topics: [
        {
            name: "orders",
            partitions: [
                { partition: 0, error: NONE, offset: 1000 }
            ]
        }
    ]
}
```

**Deliverables:**
- [ ] kafka-console-producer works
- [ ] Python kafka-python producer works
- [ ] Java Kafka producer works

---

### Initiative 2.4: Fetch API (Week 4-5)

**Tasks:**
- [ ] Implement Fetch request/response
- [ ] Offset-based fetching
- [ ] Max bytes handling
- [ ] Wait for new data (long polling)

**Fetch flow:**
```
Client sends:
FetchRequest {
    max_wait_ms: 500,
    min_bytes: 1,
    max_bytes: 1048576,
    topics: [
        {
            name: "orders",
            partitions: [
                { partition: 0, fetch_offset: 1000, max_bytes: 1048576 }
            ]
        }
    ]
}

Server:
1. Find segment containing offset 1000
2. Read records from offset 1000
3. If no data and max_wait_ms > 0, wait for new data
4. Return records

Server responds:
FetchResponse {
    topics: [
        {
            name: "orders",
            partitions: [
                {
                    partition: 0,
                    error: NONE,
                    high_watermark: 1500,
                    records: RecordBatch { ... }
                }
            ]
        }
    ]
}
```

**Deliverables:**
- [ ] kafka-console-consumer works
- [ ] Long polling works (waits for new data)
- [ ] Python consumer works
- [ ] Java consumer works

---

### Initiative 2.5: Consumer Groups (Week 5-6)

**Tasks:**
- [ ] FindCoordinator API
- [ ] JoinGroup API
- [ ] SyncGroup API
- [ ] Heartbeat API
- [ ] LeaveGroup API
- [ ] OffsetFetch/OffsetCommit APIs
- [ ] Partition assignment (range or round-robin)

**Consumer group flow:**
```
Consumer 1                    Server                    Consumer 2
    │                           │                           │
    │──FindCoordinator─────────▶│                           │
    │◀─────────(coordinator)────│                           │
    │                           │                           │
    │──JoinGroup───────────────▶│◀──────────JoinGroup──────│
    │                           │                           │
    │                     (wait for all members)            │
    │                           │                           │
    │                     (assign partitions)               │
    │                           │                           │
    │◀─────SyncGroup────────────│───────SyncGroup─────────▶│
    │  (partitions 0,1)         │         (partition 2)     │
    │                           │                           │
    │──Heartbeat───────────────▶│◀──────────Heartbeat──────│
    │◀─────────(ok)─────────────│───────────(ok)───────────▶│
```

**This is complex.** Consumer groups are one of Kafka's trickier features. Aim for basic functionality first:
- Single consumer group works
- Multiple consumers in group works
- Rebalancing on join/leave works
- Offset commit/fetch works

**Deliverables:**
- [ ] Consumer groups working
- [ ] Can run multiple consumers sharing partitions
- [ ] Offsets are committed and restored

---

### Initiative 2.6: Additional APIs (Week 6-7)

**Tasks:**
- [ ] ListOffsets API (find earliest/latest offset)
- [ ] CreateTopics API
- [ ] DeleteTopics API
- [ ] DescribeConfigs API (basic)

**Deliverables:**
- [ ] kafka-topics.sh commands work
- [ ] Can create/delete topics via Kafka protocol

---

### Initiative 2.7: Testing & Compatibility (Week 7-8)

**Tasks:**
- [ ] Test with kafka-python
- [ ] Test with confluent-kafka-go
- [ ] Test with Java Kafka client
- [ ] Test with kafkacat/kcat
- [ ] Test with common frameworks (if applicable)
- [ ] Document known limitations
- [ ] Performance comparison vs real Kafka

**Compatibility test matrix:**

| Client | Produce | Consume | Consumer Groups | Status |
|--------|---------|---------|-----------------|--------|
| kafka-console-* | | | | |
| kafkacat/kcat | | | | |
| kafka-python | | | | |
| confluent-kafka-python | | | | |
| franz-go | | | | |
| sarama (Go) | | | | |
| Java Kafka client | | | | |

**Deliverables:**
- [ ] Compatibility matrix filled out
- [ ] Known issues documented
- [ ] Blog post: "Kafka-Compatible in Rust"

---

### Phase 2 Milestones

| Week | Milestone | Deliverable |
|------|-----------|-------------|
| 9 | Protocol foundation | TCP server, ApiVersions working |
| 10 | Metadata API | Clients can discover cluster |
| 11 | Produce API | Can produce via Kafka protocol |
| 12 | Fetch API | Can consume via Kafka protocol |
| 13-14 | Consumer groups | Groups and offsets working |
| 15 | Additional APIs | Topic management via protocol |
| 16 | Testing | Compatibility verified |

**End of Phase 2:** Drop-in Kafka replacement for basic use cases. Existing apps work with config change only.

**What you can now say:** "Replace Kafka, save 80% on infrastructure costs."

---

## Phase 3: Processing Foundation (Months 5-7)

**Goal:** Add SQL processing that runs natively on your storage layer.

### Why This Matters Now

At end of Phase 2, you have a Kafka replacement. That's valuable but not unique (WarpStream, Redpanda exist).

Phase 3 is where you become differentiated:
- "Kafka + Flink in one system"
- "Process your streams with SQL, no separate system needed"

### What You're Building

```
┌─────────────────────────────────────────────────────────────────┐
│                         YOUR SYSTEM                             │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   SQL Processing Layer                   │   │
│  │                                                          │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │   │
│  │  │  SQL     │ │ Logical  │ │ Physical │ │ Execution│   │   │
│  │  │  Parser  │▶│ Planner  │▶│ Planner  │▶│  Engine  │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │   │
│  │                                               │          │   │
│  │                                               ▼          │   │
│  │                                        ┌──────────┐      │   │
│  │                                        │  State   │      │   │
│  │                                        │  Store   │      │   │
│  │                                        └──────────┘      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   Storage Layer                          │   │
│  │                  (Phase 1 + Kafka Protocol)              │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Initiative 3.1: SQL Parser Extension (Week 1-2)

**Tasks:**
- [ ] Integrate DataFusion
- [ ] Extend SQL parser for streaming syntax
- [ ] CREATE STREAM statement
- [ ] CREATE MATERIALIZED VIEW statement
- [ ] WINDOW clauses
- [ ] EMIT clause (when to output)

**SQL syntax:**
```sql
-- Create a stream from a topic
CREATE STREAM orders (
    order_id VARCHAR,
    customer_id VARCHAR,
    amount DECIMAL,
    order_time TIMESTAMP
) WITH (
    topic = 'orders',
    format = 'json',
    timestamp_field = 'order_time'
);

-- Simple transformation
CREATE STREAM high_value_orders AS
SELECT * FROM orders WHERE amount > 1000;

-- Tumbling window aggregation
CREATE MATERIALIZED VIEW hourly_sales AS
SELECT
    TUMBLE_START(order_time, INTERVAL '1 hour') as window_start,
    SUM(amount) as total_sales,
    COUNT(*) as order_count
FROM orders
GROUP BY TUMBLE(order_time, INTERVAL '1 hour');

-- Sliding window
CREATE MATERIALIZED VIEW rolling_avg AS
SELECT
    customer_id,
    AVG(amount) as avg_amount
FROM orders
GROUP BY 
    customer_id,
    HOP(order_time, INTERVAL '5 minutes', INTERVAL '1 hour');

-- Output to topic
CREATE SINK alerts TO TOPIC 'fraud-alerts' AS
SELECT * FROM orders WHERE amount > 10000;
```

**Deliverables:**
- [ ] Parser handles all streaming SQL syntax
- [ ] Logical plan generation for streaming queries

---

### Initiative 3.2: Streaming Operators (Week 2-4)

**Tasks:**
- [ ] Source operator (reads from storage layer)
- [ ] Filter operator
- [ ] Project operator
- [ ] Aggregate operator (stateful)
- [ ] Window operator (tumbling first)
- [ ] Sink operator (writes to storage layer)

**Operator interface:**
```rust
// crates/execution/src/operator.rs

#[async_trait]
pub trait Operator: Send + Sync {
    /// Process a batch of records
    async fn execute(&mut self, input: RecordBatch) -> Result<Option<RecordBatch>>;
    
    /// Called when a watermark advances
    async fn on_watermark(&mut self, watermark: Timestamp) -> Result<Vec<RecordBatch>>;
    
    /// Get operator state for checkpointing
    fn checkpoint(&self) -> Result<Bytes>;
    
    /// Restore operator state
    fn restore(&mut self, state: Bytes) -> Result<()>;
    
    /// Output schema
    fn schema(&self) -> &Schema;
}

// Example: Filter operator
pub struct FilterOperator {
    predicate: Expr,
    schema: Schema,
}

impl Operator for FilterOperator {
    async fn execute(&mut self, input: RecordBatch) -> Result<Option<RecordBatch>> {
        let mask = evaluate_predicate(&self.predicate, &input)?;
        let filtered = filter_record_batch(&input, &mask)?;
        Ok(Some(filtered))
    }
    
    async fn on_watermark(&mut self, _: Timestamp) -> Result<Vec<RecordBatch>> {
        Ok(vec![])  // Stateless, nothing to emit
    }
    
    fn checkpoint(&self) -> Result<Bytes> {
        Ok(Bytes::new())  // Stateless
    }
    
    fn restore(&mut self, _: Bytes) -> Result<()> {
        Ok(())
    }
    
    fn schema(&self) -> &Schema {
        &self.schema
    }
}
```

**Deliverables:**
- [ ] All basic operators implemented
- [ ] Can chain operators into a pipeline
- [ ] Unit tests for each operator

---

### Initiative 3.3: Windowing (Week 4-5)

**Tasks:**
- [ ] Tumbling windows
- [ ] Sliding/hopping windows
- [ ] Session windows (stretch)
- [ ] Watermark handling
- [ ] Late data handling

**Window concepts:**
```
Tumbling Window (non-overlapping):
Time:  |----1----|----2----|----3----|
Data:  * *  *    * * *     *   * *

Sliding/Hopping Window (overlapping):
Time:  |----1----|
         |----2----|
              |----3----|
Data:  * *  *  * * *   *   * *
Window 1 sees: * * *
Window 2 sees: * * * * *
Window 3 sees: * * * * * * * *

Session Window (gap-based):
Time:  *  * *      *  * * *          * *
       |-----|     |-------|         |--|
       Session1    Session2          Session3
```

**Watermarks:**
```rust
// Watermark = "I've seen all events up to this timestamp"
// When watermark passes window end, window can close and emit

pub struct WatermarkTracker {
    current_watermark: Timestamp,
    max_out_of_order: Duration,
}

impl WatermarkTracker {
    pub fn observe(&mut self, event_time: Timestamp) {
        // Watermark = max observed event time - max_out_of_order
        let potential = event_time - self.max_out_of_order;
        if potential > self.current_watermark {
            self.current_watermark = potential;
        }
    }
    
    pub fn watermark(&self) -> Timestamp {
        self.current_watermark
    }
}
```

**Deliverables:**
- [ ] Tumbling windows working
- [ ] Sliding windows working
- [ ] Watermarks tracked correctly
- [ ] Windows emit when watermark passes

---

### Initiative 3.4: State Management (Week 5-6)

**Tasks:**
- [ ] State store interface
- [ ] RocksDB implementation
- [ ] Key-value state for aggregations
- [ ] Window state
- [ ] State cleanup (TTL)

**State interface:**
```rust
// crates/execution/src/state.rs

#[async_trait]
pub trait StateStore: Send + Sync {
    // Key-value operations
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    async fn delete(&self, key: &[u8]) -> Result<()>;
    
    // Prefix scan (for window state)
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    
    // Checkpointing
    async fn checkpoint(&self) -> Result<StateSnapshot>;
    async fn restore(&self, snapshot: StateSnapshot) -> Result<()>;
}

// State key design for windows:
// [operator_id][window_key][group_key] -> aggregate_state
// 
// Example: SUM(amount) GROUP BY customer_id, TUMBLE(ts, 1 hour)
// Key: [op1][2024-01-01T10:00:00][customer-123] -> 1500.00
```

**Deliverables:**
- [ ] RocksDB state store working
- [ ] Aggregations maintain state correctly
- [ ] State can be checkpointed and restored

---

### Initiative 3.5: Execution Runtime (Week 6-7)

**Tasks:**
- [ ] Pipeline builder (operators → execution graph)
- [ ] Single-threaded runtime
- [ ] Batch processing loop
- [ ] Watermark propagation
- [ ] Error handling and recovery

**Runtime flow:**
```
┌─────────────────────────────────────────────────────────────────┐
│                      Execution Runtime                          │
│                                                                 │
│   ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌─────────┐ │
│   │  Source  │───▶│  Filter  │───▶│ Aggregate│───▶│  Sink   │ │
│   │ (topic)  │    │          │    │ (window) │    │ (topic) │ │
│   └──────────┘    └──────────┘    └──────────┘    └─────────┘ │
│        │                               │                        │
│        │         Watermarks            │                        │
│        └──────────────────────────────▶│                        │
│                                        │                        │
│                                        ▼                        │
│                                 ┌──────────┐                    │
│                                 │  State   │                    │
│                                 │  Store   │                    │
│                                 └──────────┘                    │
└─────────────────────────────────────────────────────────────────┘
```

**Runtime loop:**
```rust
// crates/execution/src/runtime.rs

pub struct StreamingRuntime {
    pipeline: Pipeline,
    state: Arc<dyn StateStore>,
}

impl StreamingRuntime {
    pub async fn run(&mut self) -> Result<()> {
        loop {
            // 1. Read batch from source
            let batch = self.pipeline.source.next_batch().await?;
            
            // 2. Update watermark
            let watermark = self.pipeline.source.watermark();
            
            // 3. Process through operators
            let mut current = Some(batch);
            for operator in &mut self.pipeline.operators {
                if let Some(input) = current {
                    current = operator.execute(input).await?;
                }
                
                // Emit any watermark-triggered output
                let watermark_output = operator.on_watermark(watermark).await?;
                for output in watermark_output {
                    self.pipeline.sink.write(output).await?;
                }
            }
            
            // 4. Write final output
            if let Some(output) = current {
                self.pipeline.sink.write(output).await?;
            }
        }
    }
}
```

**Deliverables:**
- [ ] Runtime executes pipelines
- [ ] End-to-end: SQL query → running pipeline → output

---

### Initiative 3.6: Query API (Week 7-8)

**Tasks:**
- [ ] REST/gRPC API for submitting queries
- [ ] Query management (start, stop, status)
- [ ] Query result querying (for materialized views)
- [ ] CLI commands for queries

**API:**
```protobuf
service QueryService {
    rpc CreateQuery(CreateQueryRequest) returns (CreateQueryResponse);
    rpc StopQuery(StopQueryRequest) returns (StopQueryResponse);
    rpc GetQueryStatus(GetQueryStatusRequest) returns (GetQueryStatusResponse);
    rpc ListQueries(ListQueriesRequest) returns (ListQueriesResponse);
    
    // Query materialized views
    rpc QueryView(QueryViewRequest) returns (QueryViewResponse);
}

message CreateQueryRequest {
    string sql = 1;
    string name = 2;
}
```

**CLI:**
```bash
# Submit a query
streamctl query create --name hourly_sales --sql "
    CREATE MATERIALIZED VIEW hourly_sales AS
    SELECT TUMBLE_START(ts, INTERVAL '1 hour') as hour, SUM(amount)
    FROM orders
    GROUP BY TUMBLE(ts, INTERVAL '1 hour')
"

# Check status
streamctl query status hourly_sales

# Query the materialized view
streamctl query select hourly_sales "SELECT * WHERE hour > '2024-01-01'"

# Stop query
streamctl query stop hourly_sales
```

**Deliverables:**
- [ ] Query API working
- [ ] CLI can manage queries
- [ ] Can query materialized views

---

### Phase 3 Milestones

| Week | Milestone | Deliverable |
|------|-----------|-------------|
| 17-18 | SQL parser | Streaming SQL syntax parsing |
| 19-20 | Operators | Filter, project, aggregate working |
| 21 | Windows | Tumbling and sliding windows |
| 22 | State | RocksDB state management |
| 23 | Runtime | Single-node execution working |
| 24 | API | Query submission and management |

**End of Phase 3:** Single-node stream processing with SQL. Integrated with storage layer. No external Kafka or Flink needed.

---

## Phase 4: Distributed Processing (Months 8-10)

**Goal:** Scale processing across multiple nodes with fault tolerance.

### Initiative 4.1: Distributed Architecture (Week 1-3)
- [ ] Coordinator service design
- [ ] Worker service design
- [ ] Task scheduling
- [ ] Communication protocol (gRPC)

### Initiative 4.2: Partitioned Execution (Week 3-6)
- [ ] Partition-parallel source reading
- [ ] Shuffle/exchange operators
- [ ] Hash partitioning by key
- [ ] Data exchange between workers

### Initiative 4.3: Checkpointing (Week 6-9)
- [ ] Checkpoint coordinator
- [ ] Barrier injection
- [ ] State snapshot to S3
- [ ] Coordinated checkpoint completion

### Initiative 4.4: Fault Tolerance (Week 9-12)
- [ ] Worker failure detection
- [ ] Task restart on failure
- [ ] State restoration from checkpoint
- [ ] Exactly-once guarantees

### Phase 4 Milestones

| Week | Milestone |
|------|-----------|
| 25-27 | Distributed architecture |
| 28-30 | Partitioned execution |
| 31-33 | Checkpointing |
| 34-36 | Fault tolerance |

**End of Phase 4:** Distributed, fault-tolerant stream processing.

---

## Phase 5: Product Polish (Months 11-14)

**Goal:** Production-ready product with UI and cloud deployment.

### Initiative 5.1: Full Kafka Protocol (Week 1-4)
- [ ] Remaining API coverage
- [ ] Transactions (basic)
- [ ] ACLs
- [ ] Compatibility testing

### Initiative 5.2: Web UI (Week 4-8)
- [ ] Dashboard (health, throughput)
- [ ] Topic browser
- [ ] Query editor (SQL)
- [ ] Pipeline monitoring

### Initiative 5.3: Cloud Deployment (Week 8-12)
- [ ] Terraform modules (AWS)
- [ ] Kubernetes Helm chart
- [ ] BYOC deployment model
- [ ] Auto-scaling

### Initiative 5.4: Observability (Week 10-14)
- [ ] Prometheus metrics
- [ ] Distributed tracing
- [ ] Log aggregation
- [ ] Alerting

### Phase 5 Milestones

| Week | Milestone |
|------|-----------|
| 37-40 | Full Kafka protocol |
| 41-44 | Web UI |
| 45-48 | Cloud deployment |
| 49-52 | Observability |

**End of Phase 5:** Production-ready platform.

---

## Phase 6: Launch & Growth (Months 15-18+)

### Initiative 6.1: Documentation (Week 1-4)
### Initiative 6.2: Beta Program (Week 1-8)
### Initiative 6.3: Pricing & Billing (Week 4-8)
### Initiative 6.4: Public Launch (Week 8-12)
### Initiative 6.5: Sales & Growth (Week 12+)

---

## Technical Architecture

### Crate Structure (Final)

```
streaming-platform/
├── Cargo.toml
├── crates/
│   ├── core/                  # Shared types, errors
│   ├── storage/               # S3 segment read/write
│   ├── metadata/              # Topic/partition metadata
│   ├── kafka-protocol/        # Kafka wire protocol
│   ├── sql/                   # SQL parsing, planning
│   ├── execution/             # Streaming operators, runtime
│   ├── state/                 # State store (RocksDB)
│   ├── coordinator/           # Distributed coordinator
│   ├── worker/                # Worker node
│   ├── server/                # Main server binary
│   └── cli/                   # CLI tool
├── web/                       # React UI
├── deploy/                    # Terraform, K8s
└── docs/                      # Documentation
```

### Key Dependencies

```toml
[workspace.dependencies]
# Async runtime
tokio = { version = "1.0", features = ["full"] }

# Data processing
arrow = "50.0"
datafusion = "35.0"
parquet = "50.0"

# Storage
object_store = "0.9"
rocksdb = "0.21"

# Networking
tonic = "0.11"
tokio-util = { version = "0.7", features = ["codec"] }
bytes = "1.0"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
prost = "0.12"

# Observability
tracing = "0.1"
tracing-subscriber = "0.3"
metrics = "0.22"

# Utilities
thiserror = "1.0"
anyhow = "1.0"
clap = { version = "4.0", features = ["derive"] }
```

---

## Success Metrics

| Phase | Metric | Target |
|-------|--------|--------|
| 1 | Write throughput | 50K records/sec |
| 2 | Kafka compatibility | 80% API coverage |
| 3 | Query latency | < 100ms p99 |
| 4 | Recovery time | < 60 seconds |
| 5 | Deploy time | < 15 minutes |
| 6 | Paying customers | 10+ |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Kafka protocol complexity | Start with 20% of APIs that cover 80% of use cases |
| Motivation decay | Ship something public by month 4, blog regularly |
| Technical dead-ends | Prototype risky parts early |
| No users | Start talking to potential users in month 3 |
| Competitor moves | Focus on integration as differentiator |

---

## Weekly Template

```
Week of: ___________
Phase: ___  Initiative: ___

Goals this week:
1. [ ] 
2. [ ] 
3. [ ] 

Completed:
- 

Blockers:
- 

Next week:
- 

Public update: [ ] Blog  [ ] Twitter  [ ] Discord
```

---

*Document Version: 2.0 (Storage-First)*
*Last Updated: January 2025*
