# Phase 5: Producer/Consumer Integration - Implementation Plan

**Created**: 2026-01-25
**Status**: 📋 Ready to Start
**Goal**: Build production-ready Producer and Consumer APIs that leverage the multi-agent infrastructure
**Duration**: 4-6 weeks (Phases 5.1-5.4)

---

## Executive Summary

Phase 5 completes the foundational StreamHouse platform by adding high-level APIs for producers and consumers. This enables real applications to use StreamHouse without dealing with low-level partition/segment details.

### What We're Building

```
┌──────────────────────────────────────────────────────────────┐
│                    Phase 5 Architecture                       │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  Application Layer (NEW!)                                     │
│  ┌──────────────┐                    ┌──────────────┐        │
│  │  Producer    │                    │  Consumer    │        │
│  │              │                    │              │        │
│  │ • send()     │                    │ • poll()     │        │
│  │ • flush()    │                    │ • commit()   │        │
│  │ • close()    │                    │ • seek()     │        │
│  └──────┬───────┘                    └──────┬───────┘        │
│         │                                   │                │
│         ▼                                   ▼                │
│  ┌──────────────────────────────────────────────────┐        │
│  │         Agent Layer (Phase 4)                    │        │
│  │  - Multi-agent coordination                      │        │
│  │  - Lease-based partition ownership               │        │
│  │  - Automatic failover                            │        │
│  └──────────────────────────────────────────────────┘        │
│         │                                   │                │
│         ▼                                   ▼                │
│  ┌──────────────┐                    ┌──────────────┐        │
│  │ PartitionWriter                   │ PartitionReader       │
│  │ (Phase 3)    │                    │ (Phase 3)    │        │
│  └──────┬───────┘                    └──────┬───────┘        │
│         │                                   │                │
│         ▼                                   ▼                │
│  ┌──────────────────────────────────────────────────┐        │
│  │              MinIO / S3                          │        │
│  └──────────────────────────────────────────────────┘        │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

### What Problem Does This Solve?

**Before Phase 5**:
- ✅ Storage layer works (Phase 3)
- ✅ Multi-agent coordination works (Phase 4)
- ❌ **No high-level APIs** - applications must manage partitions manually
- ❌ No consumer groups or offset management
- ❌ No batching or automatic retries

**After Phase 5**:
- ✅ **Producer API** - Simple `send(topic, key, value)` interface
- ✅ **Consumer API** - Kafka-like `poll()` and `commit()` semantics
- ✅ **Consumer Groups** - Multiple consumers share partition load
- ✅ **Automatic Failover** - Leverages Phase 4 multi-agent coordination
- ✅ **Production Ready** - Applications can use StreamHouse like Kafka

---

## Foundation Already Built

### Phase 3: Storage Layer
- ✅ `PartitionWriter` - Low-level partition writes
- ✅ `PartitionReader` - Low-level partition reads
- ✅ Segment compression (LZ4)
- ✅ S3 uploads/downloads
- ✅ Metadata store integration

### Phase 4: Multi-Agent Coordination
- ✅ `Agent` - Multi-agent lifecycle
- ✅ `LeaseManager` - Partition ownership
- ✅ `PartitionAssigner` - Automatic partition distribution
- ✅ Failover and recovery

**Phase 5 builds the application layer on top of this solid foundation.**

---

## Phase 5 Breakdown

### Phase 5.1: Producer API (Weeks 1-2)

**Goal**: High-level API for producing records to topics

#### Deliverables

1. **Producer Struct** (`crates/streamhouse-client/src/producer.rs`)
   ```rust
   pub struct Producer {
       agent_client: Arc<AgentClient>,
       config: ProducerConfig,
       metadata_cache: Arc<MetadataCache>,
       partition_router: Arc<PartitionRouter>,
   }

   impl Producer {
       pub async fn send(
           &self,
           topic: &str,
           key: Option<&[u8]>,
           value: &[u8],
       ) -> Result<RecordMetadata>;

       pub async fn flush(&self) -> Result<()>;
       pub async fn close(self) -> Result<()>;
   }
   ```

2. **Key Features**:
   - **Automatic Partitioning**: Hash key to select partition
   - **Batching**: Buffer records and send in batches
   - **Compression**: Automatic LZ4 compression
   - **Retries**: Automatic retry on transient failures
   - **Metadata Caching**: Cache topic metadata to avoid repeated queries

3. **Agent Discovery**:
   ```rust
   pub struct AgentClient {
       metadata_store: Arc<dyn MetadataStore>,
       agent_connections: Arc<RwLock<HashMap<String, AgentConnection>>>,
   }

   impl AgentClient {
       // Find which agent leads a partition
       pub async fn get_partition_leader(
           &self,
           topic: &str,
           partition_id: u32,
       ) -> Result<AgentConnection>;

       // Send write request to agent
       pub async fn write_records(
           &self,
           agent: &AgentConnection,
           topic: &str,
           partition_id: u32,
           records: Vec<Record>,
       ) -> Result<WriteResponse>;
   }
   ```

4. **Configuration**:
   ```rust
   pub struct ProducerConfig {
       pub batch_size: usize,           // Default: 1000 records
       pub linger_ms: u64,               // Default: 100ms
       pub max_in_flight: usize,         // Default: 5
       pub request_timeout_ms: u64,      // Default: 30s
       pub retry_backoff_ms: u64,        // Default: 100ms
       pub max_retries: usize,           // Default: 3
       pub compression: CompressionType, // Default: LZ4
   }
   ```

#### Example Usage

```rust
use streamhouse_client::Producer;

// Create producer
let producer = Producer::builder()
    .metadata_store(metadata_store)
    .batch_size(1000)
    .linger_ms(100)
    .build()
    .await?;

// Send records
for i in 0..10000 {
    let key = format!("user-{}", i % 100);
    let value = format!("{{\"event\":\"click\",\"user\":{}}}", i);

    producer.send("events", Some(key.as_bytes()), value.as_bytes()).await?;
}

// Flush and close
producer.flush().await?;
producer.close().await?;
```

#### Tests

```rust
#[tokio::test]
async fn test_producer_sends_to_partition() { }

#[tokio::test]
async fn test_producer_batching() { }

#[tokio::test]
async fn test_producer_retries_on_failure() { }

#[tokio::test]
async fn test_producer_discovers_agent() { }
```

---

### Phase 5.2: Consumer API (Weeks 2-3)

**Goal**: High-level API for consuming records from topics

#### Deliverables

1. **Consumer Struct** (`crates/streamhouse-client/src/consumer.rs`)
   ```rust
   pub struct Consumer {
       consumer_group: String,
       agent_client: Arc<AgentClient>,
       config: ConsumerConfig,
       subscriptions: Vec<String>,
       assigned_partitions: Vec<TopicPartition>,
       offsets: HashMap<TopicPartition, i64>,
   }

   impl Consumer {
       pub async fn subscribe(&mut self, topics: Vec<String>) -> Result<()>;

       pub async fn poll(&mut self, timeout: Duration) -> Result<Vec<Record>>;

       pub async fn commit(&mut self) -> Result<()>;

       pub async fn seek(&mut self, partition: TopicPartition, offset: i64) -> Result<()>;

       pub async fn close(self) -> Result<()>;
   }
   ```

2. **Key Features**:
   - **Consumer Groups**: Multiple consumers share partition load
   - **Offset Management**: Automatic or manual offset commits
   - **Rebalancing**: Handle partition reassignment
   - **At-least-once Delivery**: Guarantee records are processed
   - **Seek Support**: Start from specific offset

3. **Consumer Group Coordinator**:
   ```rust
   pub struct ConsumerGroupCoordinator {
       metadata_store: Arc<dyn MetadataStore>,
       group_id: String,
   }

   impl ConsumerGroupCoordinator {
       // Join consumer group
       pub async fn join_group(&self, consumer_id: &str) -> Result<JoinResponse>;

       // Get assigned partitions for this consumer
       pub async fn get_assignment(&self) -> Result<Vec<TopicPartition>>;

       // Commit offsets for consumer group
       pub async fn commit_offsets(
           &self,
           offsets: HashMap<TopicPartition, i64>,
       ) -> Result<()>;

       // Leave consumer group
       pub async fn leave_group(&self) -> Result<()>;
   }
   ```

4. **Configuration**:
   ```rust
   pub struct ConsumerConfig {
       pub group_id: String,
       pub enable_auto_commit: bool,     // Default: true
       pub auto_commit_interval_ms: u64, // Default: 5000ms
       pub max_poll_records: usize,      // Default: 500
       pub session_timeout_ms: u64,      // Default: 10000ms
       pub fetch_min_bytes: usize,       // Default: 1 byte
       pub fetch_max_wait_ms: u64,       // Default: 500ms
   }
   ```

#### Example Usage

```rust
use streamhouse_client::Consumer;

// Create consumer
let mut consumer = Consumer::builder()
    .group_id("analytics-consumers")
    .metadata_store(metadata_store)
    .enable_auto_commit(true)
    .build()
    .await?;

// Subscribe to topics
consumer.subscribe(vec!["events".to_string()]).await?;

// Poll for records
loop {
    let records = consumer.poll(Duration::from_millis(100)).await?;

    for record in records {
        println!("Received: {:?}", record);
        // Process record...
    }

    // Commit offsets (if auto_commit disabled)
    if !config.enable_auto_commit {
        consumer.commit().await?;
    }
}
```

#### Tests

```rust
#[tokio::test]
async fn test_consumer_polls_records() { }

#[tokio::test]
async fn test_consumer_group_coordination() { }

#[tokio::test]
async fn test_consumer_offset_commit() { }

#[tokio::test]
async fn test_consumer_rebalancing() { }
```

---

### Phase 5.3: Integration & Examples (Week 4)

**Goal**: End-to-end examples and integration tests

#### Deliverables

1. **Complete Producer Example** (`examples/producer_demo.rs`)
   - Connect to cluster
   - Produce 100K records
   - Show throughput metrics
   - Demonstrate error handling

2. **Complete Consumer Example** (`examples/consumer_demo.rs`)
   - Join consumer group
   - Consume from topic
   - Process records
   - Commit offsets

3. **Multi-Consumer Example** (`examples/consumer_group_demo.rs`)
   - Start 3 consumers in same group
   - Show automatic partition assignment
   - Demonstrate rebalancing when consumer joins/leaves

4. **Integration Tests** (`tests/producer_consumer_test.rs`)
   ```rust
   #[tokio::test]
   async fn test_end_to_end_produce_consume() {
       // Produce 10K records
       // Consume all records
       // Verify count matches
   }

   #[tokio::test]
   async fn test_consumer_group_load_balancing() {
       // Start 2 consumers in same group
       // Produce to 4 partitions
       // Verify each consumer gets ~2 partitions
   }

   #[tokio::test]
   async fn test_failover_scenario() {
       // Producer sends to agent-1
       // Kill agent-1
       // Verify producer switches to agent-2
       // No data loss
   }
   ```

5. **Performance Benchmarks**:
   ```rust
   // Measure throughput
   cargo run --release --example producer_benchmark
   // Output: 50,000 records/sec

   // Measure latency
   cargo run --release --example latency_benchmark
   // Output: p50=10ms, p99=50ms, p99.9=200ms
   ```

---

### Phase 5.4: Documentation & Polish (Weeks 5-6)

**Goal**: Production-ready documentation and hardening

#### Deliverables

1. **API Documentation**:
   - Complete rustdoc for all public APIs
   - Usage examples in doc comments
   - Migration guide from Kafka

2. **User Guide** (`docs/USER_GUIDE.md`):
   - Getting started tutorial
   - Producer best practices
   - Consumer group patterns
   - Error handling guide
   - Performance tuning

3. **Architecture Deep Dive** (`docs/ARCHITECTURE.md`):
   - How Producer finds partition leader
   - How Consumer Group coordination works
   - Offset management internals
   - Failure scenarios and recovery

4. **Production Checklist**:
   - Monitoring setup
   - Alerting rules
   - Capacity planning
   - Disaster recovery

5. **Code Polish**:
   - Comprehensive error messages
   - Logging at appropriate levels
   - Metrics instrumentation
   - Configuration validation

---

## Key Design Decisions

### 1. gRPC vs HTTP

**Decision**: Use gRPC for agent communication

**Rationale**:
- Efficient binary protocol
- Built-in streaming support
- Type-safe schema (protobuf)
- Better performance than HTTP/REST

**Implementation**:
```protobuf
service StreamHouseAgent {
  rpc Write(WriteRequest) returns (WriteResponse);
  rpc Read(ReadRequest) returns (stream ReadResponse);
  rpc GetMetadata(MetadataRequest) returns (MetadataResponse);
}
```

### 2. Partition Assignment Strategy

**Decision**: Hash-based partitioning (consistent with Phase 4)

**Rationale**:
- Deterministic (same key → same partition)
- Load balancing (uniform distribution)
- Compatible with Kafka semantics

**Algorithm**:
```rust
fn partition_for_key(key: &[u8], partition_count: u32) -> u32 {
    let hash = murmur3_hash(key);
    (hash % partition_count as u64) as u32
}
```

### 3. Offset Storage

**Decision**: Store offsets in PostgreSQL (metadata store)

**Rationale**:
- Already using PostgreSQL for metadata
- Strong consistency guarantees
- No additional infrastructure needed

**Schema**:
```sql
CREATE TABLE consumer_offsets (
    consumer_group VARCHAR(255),
    topic VARCHAR(255),
    partition_id INT,
    offset BIGINT,
    metadata TEXT,
    commit_timestamp BIGINT,
    PRIMARY KEY (consumer_group, topic, partition_id)
);
```

### 4. Consumer Group Rebalancing

**Decision**: Simple static assignment (Phase 5), range-based later

**Algorithm (Phase 5)**:
1. Sort partitions alphabetically
2. Sort consumer IDs alphabetically
3. Assign partitions round-robin

**Future (Phase 6+)**:
- Sticky assignment (minimize movement)
- Custom assignors (user-defined)

---

## Testing Strategy

### Unit Tests
- Producer batching logic
- Consumer offset management
- Partition routing
- Metadata caching

### Integration Tests
- End-to-end produce → consume
- Consumer group coordination
- Failover scenarios
- Concurrent producers

### Performance Tests
- Throughput benchmarks
- Latency percentiles
- Memory usage under load
- Connection pooling efficiency

### Chaos Tests (Optional)
- Kill agent mid-write
- Network partition
- Slow consumer
- Out-of-order delivery

---

## Success Criteria

**Phase 5 Complete When**:
- ✅ Producer API can send 50K records/sec
- ✅ Consumer API can read 50K records/sec
- ✅ Consumer groups distribute load evenly
- ✅ Failover works seamlessly (< 5s downtime)
- ✅ All integration tests pass
- ✅ Documentation complete
- ✅ Example applications work end-to-end

---

## Migration Path (from current code)

**Step 1**: Create `streamhouse-client` crate
```bash
cargo new --lib crates/streamhouse-client
```

**Step 2**: Extract reusable logic from Phase 3 examples
- `write_with_real_api.rs` → `Producer` API
- `simple_reader.rs` → `Consumer` API

**Step 3**: Add agent communication layer (gRPC)

**Step 4**: Build consumer group coordination on top of Phase 4

**Step 5**: Write integration tests and examples

---

## Dependencies

**New Crates Needed**:
```toml
[dependencies]
tonic = "0.10"           # gRPC framework
prost = "0.12"           # Protobuf
tokio = "1.35"           # Async runtime
tracing = "0.1"          # Logging
serde = "1.0"            # Serialization
lz4 = "1.24"             # Compression
```

**Phase Dependencies**:
- ✅ Phase 3 (Storage) - Complete
- ✅ Phase 4 (Multi-Agent) - Complete
- ⏳ Phase 5 (Producer/Consumer) - Starting Now

---

## Timeline

### Week 1-2: Producer API
- Day 1-3: Producer struct + agent discovery
- Day 4-6: Batching and compression
- Day 7-10: Retries and error handling
- Day 11-14: Tests and examples

### Week 3-4: Consumer API
- Day 15-17: Consumer struct + polling
- Day 18-20: Consumer group coordinator
- Day 21-24: Offset management
- Day 25-28: Rebalancing logic

### Week 5: Integration
- Day 29-31: End-to-end examples
- Day 32-34: Performance benchmarks
- Day 35: Integration tests

### Week 6: Documentation & Polish
- Day 36-38: API documentation
- Day 39-41: User guide
- Day 42: Final testing and release

---

## Next Steps

**Ready to start Phase 5.1 (Producer API)?**

1. Create `streamhouse-client` crate
2. Define gRPC protobuf schema
3. Implement `Producer` struct
4. Build agent discovery logic
5. Add batching and compression
6. Write tests

Let's build production-ready APIs! 🚀

---

**Last Updated**: 2026-01-25
**Contributors**: Claude Sonnet 4.5
