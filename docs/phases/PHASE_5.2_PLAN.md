# Phase 5.2: gRPC Integration - Implementation Plan

**Status**: PLANNING
**Started**: 2026-01-25
**Target Completion**: TBD

## Overview

Phase 5.2 transforms the Producer from direct storage writes (Phase 5.1) to gRPC-based communication with StreamHouse agents. This change enables:

- **50K+ records/sec throughput** (vs ~5 rec/sec in Phase 5.1)
- **Connection pooling** for efficient resource usage
- **Batching** to amortize network overhead
- **Production-ready reliability** with retries and backoff

## Goals

### Primary Goals
1. ✅ Define protobuf schema for Producer↔Agent communication
2. ✅ Implement gRPC service in Agent (server-side)
3. ✅ Implement gRPC client in Producer (client-side)
4. ✅ Add connection pooling (reuse connections per agent)
5. ✅ Implement batching (accumulate records before send)
6. ✅ Add retry logic with exponential backoff
7. ✅ Achieve 50K+ records/sec throughput

### Stretch Goals
- [ ] Metrics and monitoring (latency, throughput, error rates)
- [ ] Circuit breaker pattern for failing agents
- [ ] Health check endpoints

## Architecture

### Before (Phase 5.1)
```text
┌──────────┐
│ Producer │
└────┬─────┘
     │ send() - creates PartitionWriter
     ▼
┌─────────────────┐
│ PartitionWriter │ (new instance per send)
└────┬────────────┘
     │ append() + flush()
     ▼
┌─────────────┐
│   MinIO     │ (S3-compatible storage)
└─────────────┘

Performance: ~5 records/sec
Latency: ~200ms per send
```

### After (Phase 5.2)
```text
┌──────────┐
│ Producer │
└────┬─────┘
     │ send() - batches records
     ▼
┌─────────────────┐
│  Batch Buffer   │ (100 records or 100ms)
└────┬────────────┘
     │ flush batch via gRPC
     ▼
┌─────────────────┐
│ Connection Pool │ (reuse connections)
└────┬────────────┘
     │ ProduceRequest (gRPC)
     ▼
┌─────────────────┐
│ Agent (Server)  │ (holds partition lease)
└────┬────────────┘
     │ PartitionWriter.append()
     ▼
┌─────────────────┐
│ MinIO + Metadata│
└─────────────────┘

Performance: 50K+ records/sec
Latency: <10ms p99
```

## Implementation Plan

### Step 1: Define Protobuf Schema

**File**: `crates/streamhouse-proto/proto/producer.proto`

**Messages**:
```protobuf
message ProduceRequest {
  string topic = 1;
  uint32 partition = 2;
  repeated Record records = 3;

  message Record {
    optional bytes key = 1;
    bytes value = 2;
    uint64 timestamp = 3;
  }
}

message ProduceResponse {
  uint64 base_offset = 1;
  uint32 record_count = 2;
}

service ProducerService {
  rpc Produce(ProduceRequest) returns (ProduceResponse);
}
```

**Why this design?**
- Batching built-in (repeated Record)
- Simple API (one RPC for produce)
- Minimal overhead (no unnecessary fields)
- Timestamp included for accurate record metadata

---

### Step 2: Implement gRPC Service in Agent

**File**: `crates/streamhouse-agent/src/grpc_service.rs` (new)

**Key responsibilities**:
1. Accept `ProduceRequest` from clients
2. Validate partition lease (ensure agent is leader)
3. Append records to `PartitionWriter`
4. Return `ProduceResponse` with assigned offsets

**Error handling**:
- `NOT_FOUND`: Partition lease not held by this agent
- `FAILED_PRECONDITION`: Lease expired
- `UNAVAILABLE`: Agent shutting down
- `INTERNAL`: Storage write failed

**Pseudo-code**:
```rust
async fn produce(&self, request: ProduceRequest) -> Result<ProduceResponse> {
    // 1. Validate lease
    let lease = self.lease_manager.get_lease(&request.topic, request.partition)?;
    if lease.is_expired() {
        return Err(Status::failed_precondition("Lease expired"));
    }

    // 2. Get writer for partition
    let writer = self.writer_pool.get(&request.topic, request.partition)?;

    // 3. Append records
    let base_offset = writer.lock().await.current_offset();
    for record in request.records {
        writer.lock().await.append(record.key, record.value, record.timestamp).await?;
    }

    // 4. Return response
    Ok(ProduceResponse {
        base_offset,
        record_count: request.records.len() as u32,
    })
}
```

---

### Step 3: Implement Connection Pool

**File**: `crates/streamhouse-client/src/connection_pool.rs` (new)

**Design**:
- Pool connections per agent address
- Max connections per agent: 5 (configurable)
- Idle connection timeout: 60s
- Health check on checkout: ping before use

**Key methods**:
```rust
impl ConnectionPool {
    /// Get or create a connection to an agent
    async fn get_connection(&self, address: &str) -> Result<ProducerServiceClient>;

    /// Return connection to pool
    async fn return_connection(&self, address: &str, client: ProducerServiceClient);

    /// Close all connections
    async fn close_all(&self);
}
```

---

### Step 4: Implement Batching in Producer

**File**: `crates/streamhouse-client/src/producer.rs` (modify)

**Changes**:
1. Add batch buffer per partition
2. Accumulate records until:
   - Batch size reached (default: 100 records)
   - Batch timeout reached (default: 100ms)
3. Flush batch to agent via gRPC

**New fields in Producer**:
```rust
struct Producer {
    // ... existing fields ...

    // NEW: Batching
    batch_buffers: Arc<RwLock<HashMap<(String, u32), BatchBuffer>>>,
    batch_flush_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
}

struct BatchBuffer {
    records: Vec<ProducerRecord>,
    first_record_time: Instant,
    sender: mpsc::Sender<ProducerRecord>,
}
```

**Batching flow**:
```rust
pub async fn send(&self, topic: &str, key: Option<&[u8]>, value: &[u8], partition: Option<u32>) -> Result<SendResult> {
    // 1. Route to partition
    let partition = self.route_partition(topic, key, partition).await?;

    // 2. Create record
    let record = ProducerRecord { topic, partition, key, value, timestamp: now_ms() };

    // 3. Add to batch buffer
    let buffer = self.get_or_create_buffer(topic, partition);
    buffer.add(record).await?;

    // 4. Check if should flush
    if buffer.should_flush() {
        self.flush_buffer(topic, partition).await?;
    }

    Ok(SendResult { ... })
}
```

---

### Step 5: Implement Retry Logic

**File**: `crates/streamhouse-client/src/retry.rs` (new)

**Strategy**: Exponential backoff with jitter
- Max retries: 3 (configurable)
- Base delay: 100ms
- Max delay: 5s
- Jitter: ±25% to avoid thundering herd

**Retry conditions**:
- `UNAVAILABLE`: Agent down → retry with different agent
- `DEADLINE_EXCEEDED`: Timeout → retry with same agent
- `INTERNAL`: Storage error → retry with different agent
- `NOT_FOUND`: Lease not held → refresh metadata, retry with correct agent

**Implementation**:
```rust
async fn send_with_retry<T, F>(&self, mut operation: F) -> Result<T>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Result<T>>>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if should_retry(&e) && attempt < self.max_retries => {
                attempt += 1;
                let delay = calculate_backoff(attempt, self.base_delay, self.max_delay);
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

---

### Step 6: Update Agent to Use Writer Pool

**File**: `crates/streamhouse-agent/src/agent.rs` (modify)

**Changes**:
1. Initialize `WriterPool` on agent start
2. Pre-create writers for partitions with leases
3. Reuse writers across requests (no more per-request creation)

**Benefits**:
- **No writer creation overhead** on each request
- **Connection reuse** to S3/MinIO
- **Segment batching** across multiple client requests

---

## Performance Targets

| Metric | Phase 5.1 | Phase 5.2 Target | Stretch Goal |
|--------|-----------|------------------|--------------|
| Throughput | ~5 rec/s | 50K rec/s | 100K rec/s |
| Latency (p50) | ~200ms | <5ms | <2ms |
| Latency (p99) | ~500ms | <10ms | <5ms |
| CPU Usage | Low | Moderate | Moderate |
| Memory | ~10MB | ~50MB | ~100MB |

## Testing Plan

### Unit Tests
1. Batching logic (record accumulation, flush triggers)
2. Retry logic (backoff calculation, retry conditions)
3. Connection pool (get, return, health check)

### Integration Tests
1. Producer→Agent communication (successful produce)
2. Retry on agent failure (agent down, lease lost)
3. Batching end-to-end (100 records batched)
4. Connection pooling (reuse verification)

### Performance Tests
1. **Throughput test**: Send 1M records, measure rec/sec
2. **Latency test**: Measure p50, p95, p99 latencies
3. **Concurrent producers**: 10 producers, 100K records each
4. **Failover test**: Kill agent mid-send, verify retry works

### Acceptance Criteria
- ✅ All unit tests pass
- ✅ All integration tests pass
- ✅ Throughput ≥ 50K rec/s (single producer)
- ✅ Latency p99 ≤ 10ms
- ✅ No data loss on agent failure (retries work)

## Migration Path

### Phase 5.1 → 5.2 Migration
1. **Backward compatibility**: Keep Phase 5.1 direct-write mode as fallback
2. **Feature flag**: `use_grpc` (default: true)
3. **Gradual rollout**: Test with small percentage of traffic first

### Breaking Changes
- **None**: Producer API remains unchanged
- Internal implementation switches from direct writes to gRPC

## Dependencies

### New Dependencies
```toml
[dependencies]
tonic = "0.11"           # gRPC framework
prost = "0.12"           # Protobuf support
tower = "0.4"            # Middleware (connection pooling)
```

### Proto Compilation
```toml
[build-dependencies]
tonic-build = "0.11"
```

## Files to Create/Modify

### New Files
1. `crates/streamhouse-proto/proto/producer.proto` - Protobuf definitions
2. `crates/streamhouse-proto/build.rs` - Proto compilation
3. `crates/streamhouse-proto/src/lib.rs` - Generated code export
4. `crates/streamhouse-agent/src/grpc_service.rs` - Agent gRPC server
5. `crates/streamhouse-client/src/connection_pool.rs` - Connection pooling
6. `crates/streamhouse-client/src/retry.rs` - Retry logic
7. `crates/streamhouse-client/src/batch.rs` - Batching logic

### Modified Files
1. `crates/streamhouse-client/src/producer.rs` - Use gRPC instead of direct writes
2. `crates/streamhouse-agent/src/agent.rs` - Start gRPC server, use writer pool
3. `crates/streamhouse-client/Cargo.toml` - Add gRPC dependencies
4. `crates/streamhouse-agent/Cargo.toml` - Add gRPC dependencies
5. `Cargo.toml` (workspace) - Add streamhouse-proto member

## Implementation Order

### Week 1: Foundation
1. Create streamhouse-proto crate
2. Define protobuf schema
3. Set up proto compilation
4. Verify generated code compiles

### Week 2: Agent gRPC Server
1. Implement ProducerService in agent
2. Add lease validation
3. Integrate with WriterPool
4. Unit tests for service

### Week 3: Client gRPC Client
1. Implement connection pool
2. Add batching logic
3. Integrate gRPC client in Producer
4. Unit tests for batching/pooling

### Week 4: Retry & Testing
1. Implement retry logic
2. Integration tests (end-to-end)
3. Performance benchmarks
4. Documentation updates

## Risks & Mitigations

### Risk 1: gRPC overhead worse than direct writes
**Mitigation**: Benchmark early (Week 2), optimize if needed (batching, compression)

### Risk 2: Connection pool leaks or exhaustion
**Mitigation**: Add connection limits, health checks, timeout handling

### Risk 3: Batch timeout causes latency spikes
**Mitigation**: Tune batch timeout (start at 100ms), make configurable

### Risk 4: Retry logic causes duplicate writes
**Mitigation**: Ensure retries go to correct agent (refresh metadata), add idempotency (Phase 5.4)

## Success Metrics

### Phase 5.2 is complete when:
1. ✅ All tests pass (unit + integration + performance)
2. ✅ Throughput ≥ 50K rec/s
3. ✅ Latency p99 ≤ 10ms
4. ✅ No data loss on agent failures
5. ✅ Documentation updated
6. ✅ Example demonstrates gRPC-based Producer

## Next Phase

After Phase 5.2, we'll move to:
- **Phase 5.3**: Consumer API (poll-based consumption)
- **Phase 5.4**: Advanced features (exactly-once, transactions)
