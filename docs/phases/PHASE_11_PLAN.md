# Phase 11: Distributed Architecture & Horizontal Scaling

**Status**: ⏳ DEFERRED (Future Work)
**Priority**: Medium (for production scale-out)
**Prerequisites**: Phases 1-8.5 complete
**Estimated Effort**: 2-3 weeks

## Overview

Transform the unified server architecture into a fully distributed system with horizontal scaling capabilities. This phase focuses on separating the API layer from the storage agents, enabling multi-server deployments with gRPC-based coordination.

## Current State (Phase 8.5)

**Unified Server Architecture**:
```
┌─────────────────────────────────────────┐
│      Unified Server (single process)    │
│                                          │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐ │
│  │  gRPC   │  │  HTTP   │  │  Web    │ │
│  │ :50051  │  │  :8080  │  │ Console │ │
│  └─────────┘  └─────────┘  └─────────┘ │
│       │            │            │        │
│       └────────────┴────────────┘        │
│                   │                      │
│           ┌───────▼─────────┐            │
│           │  WriterPool     │            │
│           │  (in-process)   │            │
│           └─────────────────┘            │
│                   │                      │
│           ┌───────▼─────────┐            │
│           │ PostgreSQL/S3   │            │
│           └─────────────────┘            │
└─────────────────────────────────────────┘

✅ Simple deployment
✅ Low latency (in-memory)
✅ Good for development/demos
❌ Single point of failure
❌ Limited to one machine's resources
❌ Can't scale horizontally
```

## Target State (Phase 11)

**Distributed Architecture**:
```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│   API Server 1   │  │   API Server 2   │  │   API Server N   │
│   (Stateless)    │  │   (Stateless)    │  │   (Stateless)    │
│                  │  │                  │  │                  │
│  ┌────┐  ┌────┐ │  │  ┌────┐  ┌────┐ │  │  ┌────┐  ┌────┐ │
│  │HTTP│  │gRPC│ │  │  │HTTP│  │gRPC│ │  │  │HTTP│  │gRPC│ │
│  │8080│  │impl│ │  │  │8080│  │impl│ │  │  │8080│  │impl│ │
│  └────┘  └────┘ │  │  └────┘  └────┘ │  │  └────┘  └────┘ │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │                     │                     │
         │    gRPC Producer    │    gRPC Producer    │
         │    Client (fast)    │    Client (fast)    │
         │                     │                     │
         └──────────┬──────────┴──────────┬──────────┘
                    │                     │
         ┌──────────▼─────────┐  ┌────────▼──────────┐
         │    Agent 1         │  │    Agent 2        │
         │  (Partition 0-2)   │  │  (Partition 3-5)  │
         │                    │  │                   │
         │  ┌──────────────┐  │  │  ┌──────────────┐│
         │  │ WriterPool   │  │  │  │ WriterPool   ││
         │  └──────────────┘  │  │  └──────────────┘│
         └────────┬───────────┘  └─────────┬─────────┘
                  │                        │
         ┌────────▼────────────────────────▼─────────┐
         │      PostgreSQL + MinIO (Shared)          │
         │    - Metadata coordination                │
         │    - Segment storage                      │
         └───────────────────────────────────────────┘

✅ Horizontal scaling (add more API servers)
✅ High availability (multiple agents)
✅ Fault tolerance (agent failures isolated)
✅ Better resource utilization (separate concerns)
✅ Production-ready scaling
⚠️  Slightly higher latency (network hops)
⚠️  More complex deployment
```

## Motivation

### When Unified Server is Sufficient
- **Development/testing**: Local development and integration tests
- **Low traffic**: <10K requests/second
- **Demos/POCs**: Portfolio projects and demonstrations
- **Single region**: All users in one geographic area

### When Distributed Architecture is Needed
- **High throughput**: >100K requests/second
- **High availability**: Zero downtime requirements (99.99%+)
- **Horizontal scaling**: Need to add capacity dynamically
- **Geographic distribution**: Multi-region deployments
- **Resource isolation**: Separate API and storage concerns
- **Fault isolation**: Agent failures shouldn't affect API servers

## Goals

1. **Separate API and Storage Layers**
   - API servers become stateless (no WriterPool)
   - Agents own partitions and manage WriterPools
   - Communication via gRPC (fast binary protocol)

2. **Enable Horizontal Scaling**
   - Add/remove API servers dynamically
   - Add/remove agents dynamically
   - Load balancing across servers

3. **Maintain Backward Compatibility**
   - Unified server still works (for dev/test)
   - Easy toggle between modes via config

4. **Production-Grade Reliability**
   - Health checks for all components
   - Graceful degradation
   - Automatic failover
   - Circuit breakers

## Technical Design

### 1. Architecture Components

#### API Server (Stateless)
```rust
pub struct ApiServer {
    metadata: Arc<dyn MetadataStore>,
    producer: Arc<Producer>,  // Uses gRPC to agents
    object_store: Arc<dyn ObjectStore>,
    segment_cache: Arc<SegmentCache>,
}

// AppState for distributed mode
let state = AppState {
    metadata,
    producer: Some(Arc::new(producer)),  // gRPC-based
    writer_pool: None,                   // No local writers
    object_store,
    segment_cache,
};
```

#### Storage Agent
```rust
pub struct StorageAgent {
    metadata: Arc<dyn MetadataStore>,
    writer_pool: Arc<WriterPool>,        // Manages partitions
    object_store: Arc<dyn ObjectStore>,
    lease_manager: Arc<LeaseManager>,    // Claims partitions
}

// Agent registers and claims partitions
async fn start_agent() {
    // Register with metadata store
    let agent_id = register_agent().await?;

    // Claim partitions via leasing
    let partitions = claim_partitions(agent_id).await?;

    // Start gRPC server
    serve_grpc_writes(partitions).await?;
}
```

### 2. Configuration Toggle

**Environment Variable**:
```bash
# Unified mode (current default)
export STREAMHOUSE_MODE=unified

# Distributed mode (Phase 11)
export STREAMHOUSE_MODE=distributed
```

**Code**:
```rust
enum ServerMode {
    Unified,     // Single process, WriterPool in-process
    Distributed, // Separate API and agents, gRPC communication
}

// In unified-server.rs
let mode = match env::var("STREAMHOUSE_MODE").as_deref() {
    Ok("distributed") => ServerMode::Distributed,
    _ => ServerMode::Unified,
};

match mode {
    ServerMode::Unified => {
        // Current implementation
        let state = AppState {
            producer: None,
            writer_pool: Some(writer_pool),
            // ...
        };
    }
    ServerMode::Distributed => {
        // New implementation
        let producer = Producer::builder()
            .metadata_store(metadata.clone())
            .build()
            .await?;

        let state = AppState {
            producer: Some(Arc::new(producer)),
            writer_pool: None,
            // ...
        };
    }
}
```

### 3. Partition Assignment

**Consistent Hashing** (already implemented in Phase 4):
```
Topic: orders, Partition: 0 → Agent: agent-1
Topic: orders, Partition: 1 → Agent: agent-1
Topic: orders, Partition: 2 → Agent: agent-1
Topic: orders, Partition: 3 → Agent: agent-2
Topic: orders, Partition: 4 → Agent: agent-2
Topic: orders, Partition: 5 → Agent: agent-2
```

**Lease Management**:
- Agents claim partitions via partition_leases table
- Leases expire after N seconds (heartbeat-based)
- Other agents can claim expired leases (failover)

### 4. gRPC Communication Flow

**Produce Request**:
```
Client → API Server → Producer.send()
                    ↓
              [Lookup partition owner in metadata]
                    ↓
              [Connect to agent via gRPC pool]
                    ↓
              Agent → WriterPool.append()
                    ↓
              [Write to S3 via WriterPool]
                    ↓
              Agent → Response (offset)
                    ↓
         API Server → Client (offset)
```

**Consume Request** (unchanged):
```
Client → API Server → Direct S3 read
                    ↓
              [No agent involved]
```

## Implementation Plan

### Step 1: Extract API Server Mode
**Files to modify**:
- `crates/streamhouse-server/src/bin/unified-server.rs`
  - Add `ServerMode` enum
  - Conditional AppState initialization
  - Add `STREAMHOUSE_MODE` env var check

**Estimated time**: 2 days

### Step 2: Create Standalone Agent Binary
**New file**:
- `crates/streamhouse-server/src/bin/storage-agent.rs`
  - Register agent with metadata store
  - Claim partitions via leasing
  - Start gRPC server for writes
  - Heartbeat loop

**Estimated time**: 3 days

### Step 3: Add Health Checks
**Files to modify**:
- `crates/streamhouse-api/src/handlers/metrics.rs`
  - Add `/health/live` (liveness probe)
  - Add `/health/ready` (readiness probe)
  - Check database connectivity
  - Check agent availability (for distributed mode)

**Estimated time**: 1 day

### Step 4: Load Balancing
**Deployment configs**:
- Add Kubernetes manifests
  - API Server deployment (replicas: N)
  - Agent StatefulSet (replicas: M)
  - LoadBalancer service for API servers
- Add Docker Compose for local distributed testing

**Estimated time**: 2 days

### Step 5: Integration Testing
**New tests**:
- Multi-server integration tests
- Agent failover scenarios
- Partition rebalancing
- Network partition handling

**Estimated time**: 3 days

### Step 6: Documentation
**New docs**:
- Distributed deployment guide
- Load balancing configuration
- Scaling guide (when to add servers/agents)
- Migration guide (unified → distributed)

**Estimated time**: 2 days

## Success Criteria

| Criterion | Target |
|-----------|--------|
| Multiple API servers work simultaneously | ✅ |
| Multiple agents work simultaneously | ✅ |
| Failover works (agent dies, partition reassigned) | ✅ |
| Health checks detect unhealthy components | ✅ |
| Performance: <5ms p99 latency overhead vs unified | ✅ |
| Scaling: Can handle 10x traffic by adding servers | ✅ |
| Documentation complete | ✅ |

## Performance Expectations

**Latency**:
- Unified mode: ~1ms (in-memory)
- Distributed mode: ~3-5ms (gRPC network hop)
- Trade-off: Acceptable for scalability gain

**Throughput**:
- Unified mode: Limited by single machine (CPU/memory)
- Distributed mode: Linear scaling with agent count
- Example: 10 agents = 10x throughput

## Deployment Examples

### Kubernetes (Distributed)
```yaml
# API Servers (stateless)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: streamhouse-api
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: api
        image: streamhouse:latest
        env:
        - name: STREAMHOUSE_MODE
          value: "distributed"

---
# Storage Agents (stateful)
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: streamhouse-agent
spec:
  replicas: 5
  template:
    spec:
      containers:
      - name: agent
        image: streamhouse-agent:latest
```

### Docker Compose (Distributed)
```yaml
services:
  api-1:
    image: streamhouse:latest
    environment:
      - STREAMHOUSE_MODE=distributed
    ports:
      - "8080:8080"

  api-2:
    image: streamhouse:latest
    environment:
      - STREAMHOUSE_MODE=distributed
    ports:
      - "8081:8080"

  agent-1:
    image: streamhouse-agent:latest
    environment:
      - AGENT_ID=agent-1

  agent-2:
    image: streamhouse-agent:latest
    environment:
      - AGENT_ID=agent-2

  postgres:
    image: postgres:15

  minio:
    image: minio/minio:latest
```

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Network latency increases | Medium | Use gRPC (fast), keep agents in same region |
| Agent failures cause downtime | High | Implement automatic failover with leases |
| Split-brain scenarios | High | Use fencing tokens, generation IDs |
| Complex deployment | Medium | Provide Kubernetes/Docker examples |
| Debugging distributed systems | High | Add distributed tracing (OpenTelemetry) |

## Dependencies

**Existing**:
- ✅ Phase 4: Agent architecture (already built)
- ✅ Phase 5: Producer client with gRPC (already built)
- ✅ Phase 8.5: Unified server (current state)

**New**:
- Health check endpoints (simple)
- Load balancer configuration (standard)
- Distributed tracing (optional, nice-to-have)

## Future Enhancements (Beyond Phase 11)

- **Multi-region**: Cross-region replication
- **Tiered agents**: Hot/cold storage separation
- **Auto-scaling**: Dynamic agent provisioning based on load
- **Rack awareness**: Place replicas in different availability zones

## Conclusion

Phase 11 transforms StreamHouse from a monolithic unified server into a production-grade distributed system capable of horizontal scaling. This is essential for:

1. **Production deployments** requiring high availability
2. **Large-scale workloads** exceeding single-machine capacity
3. **Geographic distribution** across multiple regions

The implementation leverages existing gRPC infrastructure from Phase 4-5, making this a natural evolution rather than a complete rewrite.

**Recommendation**: Defer until production scaling is needed. The unified server is sufficient for development, demos, and moderate workloads.

---

**Status**: ⏳ DEFERRED
**Next Step**: Complete Phases 7-10 first (observability, consumer features, transactions, performance)
**When to implement**: When production traffic exceeds single-server capacity or HA requirements emerge
