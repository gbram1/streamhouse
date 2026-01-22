# Week-by-Week Breakdown: Months 1-4

## Overview

This document breaks down every week for the first 4 months (16 weeks) in detail. At the end of this period, you'll have a working Kafka replacement.

---

## Month 1: Core Storage (Weeks 1-4)

### Week 1: Project Setup + Segment Format

**Monday**
- [ ] Create GitHub repository
- [ ] Initialize Rust workspace
- [ ] Create crate structure:
  ```
  mkdir -p crates/{core,storage,metadata,server,cli}
  ```
- [ ] Set up Cargo.toml with workspace dependencies
- [ ] Get "hello world" compiling

**Tuesday**
- [ ] Set up GitHub Actions CI:
  - cargo build
  - cargo test
  - cargo clippy
  - cargo fmt --check
- [ ] Add pre-commit hooks locally
- [ ] Write initial README.md with project vision

**Wednesday**
- [ ] Set up local development environment:
  ```bash
  docker run -d -p 9000:9000 -p 9001:9001 \
    minio/minio server /data --console-address ":9001"
  ```
- [ ] Add object_store crate, test S3 connection to MinIO
- [ ] Create `crates/core/src/lib.rs` with basic types:
  ```rust
  pub struct Record {
      pub key: Option<Vec<u8>>,
      pub value: Vec<u8>,
      pub timestamp: i64,
  }
  
  pub struct TopicPartition {
      pub topic: String,
      pub partition: u32,
  }
  ```

**Thursday**
- [ ] Design segment format (draw diagram, write notes)
- [ ] Create `crates/storage/src/segment.rs`
- [ ] Implement segment header struct:
  ```rust
  pub struct SegmentHeader {
      pub magic: [u8; 4],      // "STRM"
      pub version: u16,
      pub base_offset: u64,
      pub record_count: u32,
      pub created_at: i64,
  }
  ```
- [ ] Implement header serialization/deserialization

**Friday**
- [ ] Implement RecordBatch structure
- [ ] Implement batch serialization (without compression first)
- [ ] Write unit test: create batch, serialize, deserialize, verify
- [ ] End of day: commit and push everything

**Week 1 Deliverables:**
- [x] CI pipeline running
- [x] Local MinIO working
- [x] Segment header format defined and implemented
- [x] RecordBatch serialization working

---

### Week 2: Segment Read/Write + Compression

**Monday**
- [ ] Add LZ4 compression to RecordBatch
- [ ] Implement SegmentWriter:
  ```rust
  impl SegmentWriter {
      pub fn new(base_offset: u64) -> Self;
      pub fn append(&mut self, record: Record) -> u64; // returns offset
      pub fn finish(self) -> Bytes; // returns complete segment
  }
  ```
- [ ] Unit test: write 1000 records, verify offsets

**Tuesday**
- [ ] Implement segment index (offset -> byte position)
- [ ] Add index to segment footer
- [ ] Update SegmentWriter to build index
- [ ] Unit test: verify index correctness

**Wednesday**
- [ ] Implement SegmentReader:
  ```rust
  impl SegmentReader {
      pub fn open(data: Bytes) -> Result<Self>;
      pub fn read_at(&self, offset: u64) -> Option<Record>;
      pub fn iter_from(&self, offset: u64) -> impl Iterator<Item=Record>;
  }
  ```
- [ ] Unit test: write segment, read back all records

**Thursday**
- [ ] Add S3 upload to SegmentWriter:
  ```rust
  impl SegmentWriter {
      pub async fn flush_to_s3(&self, client: &S3Client, path: &str) -> Result<()>;
  }
  ```
- [ ] Add S3 download to SegmentReader:
  ```rust
  impl SegmentReader {
      pub async fn from_s3(client: &S3Client, path: &str) -> Result<Self>;
  }
  ```
- [ ] Integration test: write to MinIO, read back

**Friday**
- [ ] Benchmark segment write performance:
  - 1KB records
  - 10KB records
  - With/without compression
- [ ] Document results
- [ ] Code cleanup, add doc comments
- [ ] Write short blog post draft about segment format

**Week 2 Deliverables:**
- [x] SegmentWriter with compression
- [x] SegmentReader with index lookup
- [x] S3 integration working
- [x] Benchmarks documented

---

### Week 3: Metadata Store

**Monday**
- [ ] Create `crates/metadata/src/lib.rs`
- [ ] Define MetadataStore trait:
  ```rust
  #[async_trait]
  pub trait MetadataStore: Send + Sync {
      async fn create_topic(&self, config: &TopicConfig) -> Result<()>;
      async fn get_topic(&self, name: &str) -> Result<Option<Topic>>;
      async fn list_topics(&self) -> Result<Vec<Topic>>;
      // ... etc
  }
  ```
- [ ] Design SQLite schema (see project plan)

**Tuesday**
- [ ] Add sqlx crate for SQLite
- [ ] Implement SQLite MetadataStore:
  - create_topic
  - get_topic
  - delete_topic
  - list_topics
- [ ] Unit tests for topic operations

**Wednesday**
- [ ] Add partition operations:
  - get_partition
  - update_high_watermark
- [ ] Add segment tracking:
  - add_segment
  - get_segments
  - find_segment_for_offset
- [ ] Unit tests

**Thursday**
- [ ] Add consumer offset operations:
  - commit_offset
  - get_committed_offset
- [ ] Unit tests for offset operations
- [ ] Integration test: full topic lifecycle

**Friday**
- [ ] Add database migrations system
- [ ] Handle schema upgrades gracefully
- [ ] Code review and cleanup
- [ ] Document metadata schema in README

**Week 3 Deliverables:**
- [x] MetadataStore trait defined
- [x] SQLite implementation complete
- [x] All CRUD operations working
- [x] Consumer offset tracking working

---

### Week 4: Write Path

**Monday**
- [ ] Create PartitionWriter:
  ```rust
  pub struct PartitionWriter {
      topic: String,
      partition: u32,
      current_segment: SegmentWriter,
      next_offset: u64,
      config: WriteConfig,
  }
  
  impl PartitionWriter {
      pub async fn append(&mut self, record: Record) -> Result<u64>;
      pub async fn flush(&mut self) -> Result<()>;
  }
  ```
- [ ] Implement buffering logic

**Tuesday**
- [ ] Implement segment rolling:
  - Roll when size > threshold (64MB)
  - Roll when time > threshold (10 min)
- [ ] On roll: upload to S3, update metadata
- [ ] Unit test: verify rolling works

**Wednesday**
- [ ] Create TopicWriter (manages multiple partitions):
  ```rust
  pub struct TopicWriter {
      partitions: Vec<PartitionWriter>,
  }
  
  impl TopicWriter {
      pub async fn append(&mut self, key: Option<&[u8]>, value: &[u8]) -> Result<(u32, u64)>;
  }
  ```
- [ ] Implement partition selection (hash by key or round-robin)

**Thursday**
- [ ] Create WriteService (gRPC interface):
  ```rust
  impl StreamingService for WriteService {
      async fn produce(&self, request: ProduceRequest) -> Result<ProduceResponse>;
  }
  ```
- [ ] Integration test: produce records via gRPC

**Friday**
- [ ] Add flush on shutdown (graceful shutdown)
- [ ] Add flush timeout handling
- [ ] Benchmark write throughput
- [ ] Document write path architecture

**Week 4 Deliverables:**
- [x] PartitionWriter with buffering
- [x] Segment rolling working
- [x] TopicWriter with partition selection
- [x] gRPC produce endpoint

---

## Month 2: Read Path + Basic API (Weeks 5-8)

### Week 5: Read Path

**Monday**
- [ ] Create PartitionReader:
  ```rust
  pub struct PartitionReader {
      topic: String,
      partition: u32,
      metadata: Arc<dyn MetadataStore>,
      s3: Arc<S3Client>,
      cache: SegmentCache,
  }
  
  impl PartitionReader {
      pub async fn read(&self, offset: u64, max_records: usize) -> Result<Vec<Record>>;
  }
  ```

**Tuesday**
- [ ] Implement segment lookup:
  - Query metadata for segment containing offset
  - Handle offset not found
  - Handle reading across segment boundaries

**Wednesday**
- [ ] Implement SegmentCache:
  ```rust
  pub struct SegmentCache {
      cache_dir: PathBuf,
      max_size: u64,
      segments: DashMap<String, CachedSegment>,
  }
  
  impl SegmentCache {
      pub async fn get(&self, segment_id: &str, s3: &S3Client) -> Result<Arc<SegmentReader>>;
      pub fn evict_lru(&self);
  }
  ```
- [ ] LRU eviction when cache full

**Thursday**
- [ ] Add prefetching:
  - When reading segment N, prefetch segment N+1 in background
- [ ] Integration test: sequential read performance

**Friday**
- [ ] Benchmark read latency:
  - Cache hit
  - Cache miss (S3 fetch)
- [ ] Benchmark sequential vs random reads
- [ ] Document read path

**Week 5 Deliverables:**
- [x] PartitionReader working
- [x] Segment cache with LRU eviction
- [x] Prefetching implemented
- [x] Read benchmarks documented

---

### Week 6: gRPC API Server

**Monday**
- [ ] Define complete proto file (see project plan)
- [ ] Generate Rust code with tonic-build
- [ ] Create server skeleton

**Tuesday**
- [ ] Implement Produce RPC
- [ ] Implement ProduceStream RPC (streaming)
- [ ] Unit tests

**Wednesday**
- [ ] Implement Consume RPC:
  - Support start offset
  - Support max_records
  - Support max_wait_ms (long polling)
- [ ] Unit tests

**Thursday**
- [ ] Implement admin RPCs:
  - CreateTopic
  - DeleteTopic
  - ListTopics
  - GetTopicMetadata
- [ ] Implement offset RPCs:
  - CommitOffset
  - GetCommittedOffset

**Friday**
- [ ] Add health check endpoint
- [ ] Add graceful shutdown
- [ ] Integration test: full produce/consume cycle
- [ ] Document API

**Week 6 Deliverables:**
- [x] Complete gRPC API
- [x] Produce and consume working
- [x] Admin operations working
- [x] Health checks

---

### Week 7: CLI Tool

**Monday**
- [ ] Set up CLI with clap:
  ```rust
  #[derive(Parser)]
  struct Cli {
      #[command(subcommand)]
      command: Commands,
  }
  
  enum Commands {
      Topic(TopicCommands),
      Produce(ProduceArgs),
      Consume(ConsumeArgs),
  }
  ```

**Tuesday**
- [ ] Implement topic commands:
  ```bash
  streamctl topic create <name> --partitions 3
  streamctl topic list
  streamctl topic describe <name>
  streamctl topic delete <name>
  ```

**Wednesday**
- [ ] Implement produce command:
  ```bash
  echo "message" | streamctl produce <topic>
  streamctl produce <topic> --file data.jsonl
  streamctl produce <topic> --key "k1" --value "v1"
  ```

**Thursday**
- [ ] Implement consume command:
  ```bash
  streamctl consume <topic> --from-beginning
  streamctl consume <topic> --partition 0 --offset 100
  streamctl consume <topic> --follow
  streamctl consume <topic> --group my-group
  ```

**Friday**
- [ ] Add pretty output formatting (tables, colors)
- [ ] Add --output json flag
- [ ] Test all commands
- [ ] Write CLI documentation

**Week 7 Deliverables:**
- [x] Complete CLI tool
- [x] All commands working
- [x] Documentation

---

### Week 8: Testing + Documentation

**Monday**
- [ ] Write comprehensive integration tests:
  - Basic produce/consume
  - Multiple partitions
  - Consumer offsets
  - Segment rollover
  - Cache behavior

**Tuesday**
- [ ] Write failure tests:
  - S3 unavailable
  - Metadata store unavailable
  - Invalid requests
- [ ] Add retry logic where missing

**Wednesday**
- [ ] Performance benchmarks:
  - Write throughput (records/sec)
  - Read latency (p50, p99)
  - End-to-end latency
- [ ] Compare to Kafka (if possible)

**Thursday**
- [ ] Write getting started guide:
  - Installation
  - Quick start (5 min)
  - Produce/consume example
- [ ] Architecture documentation

**Friday**
- [ ] Write and publish blog post
- [ ] Create demo video/GIF
- [ ] Update README
- [ ] Tag v0.1.0 release

**Week 8 Deliverables:**
- [x] Comprehensive test suite
- [x] Benchmarks documented
- [x] Getting started guide
- [x] First blog post published
- [x] v0.1.0 release

---

## Month 3: Kafka Protocol (Weeks 9-12)

### Week 9: Protocol Foundation

**Monday**
- [ ] Study Kafka protocol specification
- [ ] Create `crates/kafka-protocol/`
- [ ] Implement request frame parsing:
  ```rust
  pub struct RequestHeader {
      pub api_key: i16,
      pub api_version: i16,
      pub correlation_id: i32,
      pub client_id: Option<String>,
  }
  ```

**Tuesday**
- [ ] Implement response frame serialization
- [ ] Create codec for tokio:
  ```rust
  impl Decoder for KafkaCodec { ... }
  impl Encoder<KafkaResponse> for KafkaCodec { ... }
  ```

**Wednesday**
- [ ] Implement ApiVersions request/response
- [ ] This is the first request any client sends
- [ ] Test with kafkacat: `kafkacat -b localhost:9092 -L`

**Thursday**
- [ ] Set up TCP server with tokio
- [ ] Connection handling
- [ ] Request routing based on api_key

**Friday**
- [ ] Test ApiVersions with real clients
- [ ] Debug any protocol issues
- [ ] Document protocol implementation status

**Week 9 Deliverables:**
- [x] Protocol codec working
- [x] TCP server accepting connections
- [x] ApiVersions working
- [x] kafkacat can connect

---

### Week 10: Metadata + Produce APIs

**Monday**
- [ ] Implement Metadata request parsing
- [ ] Implement Metadata response:
  ```rust
  pub struct MetadataResponse {
      pub brokers: Vec<BrokerMetadata>,
      pub cluster_id: Option<String>,
      pub controller_id: i32,
      pub topics: Vec<TopicMetadata>,
  }
  ```

**Tuesday**
- [ ] Wire Metadata API to your metadata store
- [ ] Test: client can discover topics
- [ ] Handle topic auto-creation (optional)

**Wednesday**
- [ ] Implement Produce request parsing
- [ ] Parse RecordBatch format (Kafka's format)
- [ ] This is complex - take your time

**Thursday**
- [ ] Implement Produce response
- [ ] Wire to your write path
- [ ] Test with kafka-console-producer

**Friday**
- [ ] Test with kafka-python producer
- [ ] Test with Java producer (if time)
- [ ] Debug any issues

**Week 10 Deliverables:**
- [x] Metadata API working
- [x] Produce API working
- [x] kafka-console-producer works

---

### Week 11: Fetch API

**Monday**
- [ ] Implement Fetch request parsing
- [ ] Understand fetch session (can ignore for now)
- [ ] Implement basic fetch without sessions

**Tuesday**
- [ ] Implement Fetch response with RecordBatch
- [ ] Wire to your read path
- [ ] Test basic fetch

**Wednesday**
- [ ] Implement long polling:
  - If no data, wait up to max_wait_ms
  - Return immediately when data arrives
- [ ] Test long polling behavior

**Thursday**
- [ ] Implement ListOffsets API:
  - EARLIEST (-2)
  - LATEST (-1)
  - Specific timestamp
- [ ] Needed for consumer --from-beginning

**Friday**
- [ ] Test with kafka-console-consumer
- [ ] Test with kafka-python consumer
- [ ] End-to-end: produce + consume via Kafka protocol

**Week 11 Deliverables:**
- [x] Fetch API working
- [x] Long polling working
- [x] ListOffsets working
- [x] kafka-console-consumer works

---

### Week 12: Consumer Groups

**Monday**
- [ ] Implement FindCoordinator API
- [ ] For now, always return self as coordinator
- [ ] Implement basic group state storage

**Tuesday**
- [ ] Implement JoinGroup API:
  - Accept join requests
  - Wait for all members (timeout)
  - Return member assignment

**Wednesday**
- [ ] Implement SyncGroup API:
  - Leader sends partition assignments
  - All members receive their assignments
- [ ] Implement Heartbeat API

**Thursday**
- [ ] Implement OffsetCommit API
- [ ] Implement OffsetFetch API
- [ ] Wire to your metadata store

**Friday**
- [ ] Test consumer groups:
  - Single consumer
  - Multiple consumers in group
  - Consumer join/leave triggers rebalance
- [ ] Debug issues

**Week 12 Deliverables:**
- [x] Consumer groups working
- [x] Offset commit/fetch working
- [x] Basic rebalancing working

---

## Month 4: Polish + Release (Weeks 13-16)

### Week 13: Additional APIs + Testing

**Monday**
- [ ] Implement CreateTopics API
- [ ] Implement DeleteTopics API
- [ ] Test with kafka-topics.sh

**Tuesday**
- [ ] Implement LeaveGroup API
- [ ] Improve rebalancing logic
- [ ] Handle edge cases

**Wednesday**
- [ ] Set up compatibility test suite
- [ ] Test matrix: kafka-python, confluent-kafka-go, etc.
- [ ] Document results

**Thursday**
- [ ] Fix compatibility issues found
- [ ] Add any missing error codes
- [ ] Improve error messages

**Friday**
- [ ] Performance testing vs Kafka protocol
- [ ] Document known limitations
- [ ] Update compatibility matrix

**Week 13 Deliverables:**
- [x] Topic management APIs
- [x] Improved consumer groups
- [x] Compatibility test suite

---

### Week 14: Docker + Deployment

**Monday**
- [ ] Create Dockerfile:
  ```dockerfile
  FROM rust:1.75 as builder
  WORKDIR /app
  COPY . .
  RUN cargo build --release
  
  FROM debian:bookworm-slim
  COPY --from=builder /app/target/release/streamctl /usr/local/bin/
  ENTRYPOINT ["streamctl", "server"]
  ```

**Tuesday**
- [ ] Create docker-compose.yml:
  ```yaml
  services:
    streaming:
      image: your-platform
      ports:
        - "9092:9092"
      environment:
        - S3_ENDPOINT=http://minio:9000
    minio:
      image: minio/minio
  ```

**Wednesday**
- [ ] Create basic Kubernetes manifests
- [ ] Test deployment on local k8s (kind/minikube)

**Thursday**
- [ ] Create Helm chart (basic)
- [ ] Document deployment options

**Friday**
- [ ] Test Docker deployment end-to-end
- [ ] Test K8s deployment
- [ ] Update getting started guide

**Week 14 Deliverables:**
- [x] Docker image
- [x] docker-compose setup
- [x] Basic Kubernetes manifests
- [x] Helm chart

---

### Week 15: Documentation + Content

**Monday**
- [ ] Write complete getting started guide
- [ ] Write configuration reference
- [ ] Write CLI reference

**Tuesday**
- [ ] Write architecture documentation
- [ ] Write troubleshooting guide
- [ ] Create FAQ

**Wednesday**
- [ ] Write blog post: "Building a Kafka Replacement in Rust"
- [ ] Focus on technical decisions
- [ ] Include benchmarks

**Thursday**
- [ ] Create demo repository with examples:
  - Python producer/consumer
  - Go producer/consumer
  - docker-compose quickstart

**Friday**
- [ ] Record demo video
- [ ] Create social media content
- [ ] Prepare launch materials

**Week 15 Deliverables:**
- [x] Complete documentation
- [x] Blog post ready
- [x] Demo repository
- [x] Launch materials

---

### Week 16: Launch v0.2.0

**Monday**
- [ ] Final testing pass
- [ ] Fix any remaining bugs
- [ ] Update all version numbers

**Tuesday**
- [ ] Tag v0.2.0 release
- [ ] Publish Docker image
- [ ] Update README with new features

**Wednesday**
- [ ] Publish blog post
- [ ] Post on Hacker News
- [ ] Post on Reddit (r/rust, r/programming)

**Thursday**
- [ ] Monitor feedback
- [ ] Respond to comments
- [ ] Fix any critical issues reported

**Friday**
- [ ] Retrospective: what worked, what didn't
- [ ] Plan Phase 3 (Processing) in detail
- [ ] Take a break!

**Week 16 Deliverables:**
- [x] v0.2.0 released
- [x] Blog post published
- [x] Public launch complete

---

## End of Month 4 Status

You now have:

1. **S3-native storage layer**
   - Segments stored in S3
   - 80% cheaper than Kafka storage

2. **Kafka-compatible API**
   - Existing apps work with config change
   - Consumer groups working

3. **CLI tool**
   - Full topic and message management

4. **Documentation**
   - Getting started guide
   - Architecture docs
   - API reference

5. **Deployment options**
   - Docker
   - Kubernetes

**What you can say:**
> "Drop-in Kafka replacement. 80% cheaper. Change one config line."

**Next:** Add SQL processing (Phase 3) to differentiate from WarpStream/Redpanda.
