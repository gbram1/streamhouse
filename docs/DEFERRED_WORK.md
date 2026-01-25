# StreamHouse - Deferred Work & Future Tasks

**Last Updated**: 2026-01-25
**Purpose**: Track features/improvements that are valuable but not critical path

---

## Phase 4 Deferred Items

### Phase 4.3 (Original Plan): AZ-Aware Routing

**Status**: ⏳ Deferred
**Priority**: Medium
**When**: After Phase 5 (Producer/Consumer APIs) complete

**What**: Optimize partition assignment to prefer local availability zone

**What we have**:
- ✅ `availability_zone` field in Agent struct
- ✅ Agents can be deployed across multiple AZs
- ✅ Consistent hashing distributes partitions

**What's missing**:
- ⏳ AZ-aware partition assignment (prefer local AZ for partition leadership)
- ⏳ Client routing logic (connect to same-AZ agents)
- ⏳ Cross-AZ cost tracking and optimization

**Why defer**:
- Current consistent hashing works fine
- No customers yet to require cross-AZ cost optimization
- Producer/Consumer APIs more important (Phase 5)
- Can add AZ awareness later without breaking changes

**When to revisit**:
- High cross-AZ data transfer costs observed (>$100/month)
- Multi-AZ deployment becomes common
- Customer requires AZ affinity for compliance

**Implementation checklist**:
```
[ ] Modify consistent hashing to include AZ weight
[ ] Add AZ-preference config to PartitionAssigner
[ ] Implement client-side agent discovery with AZ filtering
[ ] Add metrics for cross-AZ traffic
[ ] Integration tests with multi-AZ setup
[ ] Documentation for AZ-aware deployment
```

**Estimated effort**: 1 week

---

### Phase 4.4: Advanced Agent Group Features

**Status**: ⏳ Deferred
**Priority**: Low
**When**: When multi-tenancy becomes a requirement

**What**: Enhanced agent group isolation and management

**What we have**:
- ✅ `agent_group` field in Agent struct
- ✅ Agents can filter by group
- ✅ Group-based agent discovery

**What's missing**:
- ⏳ Topic→Group affinity (topics pinned to specific agent groups)
- ⏳ VPC/network isolation configuration
- ⏳ Per-group resource quotas and limits
- ⏳ Cross-group partition isolation enforcement

**Why defer**:
- Current group filtering sufficient for basic multi-tenancy
- No customers requiring strict network isolation
- More complex than needed for MVP
- Can add incrementally as needed

**When to revisit**:
- Customer requires prod/staging isolation in same cluster
- Multi-tenant SaaS deployment needed
- Compliance requires network-level isolation

**Implementation checklist**:
```
[ ] Add topic.agent_group field to topics table
[ ] Enforce group affinity in lease acquisition
[ ] Add VPC/subnet configuration to agent groups
[ ] Implement per-group quotas (CPU, memory, bandwidth)
[ ] Add cross-group metrics and monitoring
[ ] Documentation for multi-tenant deployments
```

**Estimated effort**: 2 weeks

---

### Phase 4.5: Network Partition Testing

**Status**: ⏳ Deferred
**Priority**: Low
**When**: For production hardening

**What**: Simulate network partitions and byzantine failures

**What we have**:
- ✅ Agent failure tests (clean shutdown)
- ✅ Lease failover tests
- ✅ Epoch fencing tests

**What's missing**:
- ⏳ Network partition simulation (agent can't reach PostgreSQL)
- ⏳ Split-brain scenario tests (multiple agents think they're leader)
- ⏳ Byzantine failure tests (agent sends corrupt data)
- ⏳ Clock skew tests (system time differences)

**Why defer**:
- Current tests cover 90% of failure scenarios
- Network partitions rare in practice (cloud VPCs are reliable)
- Epoch fencing already prevents split-brain
- Need production workload data first

**When to revisit**:
- Before claiming "production-ready"
- After observing real failures in production
- Customer requires chaos engineering validation

**Test scenarios to add**:
```rust
// Scenario 1: Network partition (agent isolated from PostgreSQL)
test_agent_isolated_from_postgres()
test_lease_expires_during_partition()
test_writes_rejected_after_partition()

// Scenario 2: Split-brain (rare, but theoretically possible)
test_two_agents_think_they_lead_partition()
test_epoch_fencing_prevents_corruption()

// Scenario 3: Byzantine failures
test_agent_sends_corrupt_segment()
test_agent_claims_wrong_offset()
test_agent_lies_about_lease()

// Scenario 4: Clock skew
test_lease_with_clock_skew()
test_heartbeat_with_clock_drift()
```

**Estimated effort**: 1 week

---

## Phase 3 Optional Items (Deferred Until After Phase 4)

### Phase 3.5: CockroachDB Backend

**Status**: ⏳ Deferred
**Priority**: Medium
**When**: After Phase 4 complete, if we hit PostgreSQL limits

**What**: Implement `CockroachMetadataStore` for distributed metadata

**Why defer**:
- PostgreSQL + caching handles 10,000 partitions fine
- Not needed until we hit 100,000+ partitions
- Can add later without breaking changes (trait already defined)

**When to revisit**:
- Single PostgreSQL instance becomes bottleneck
- Need multi-region active-active metadata
- Customer requires 100K+ partition workload

**Implementation checklist**:
```
[ ] Create CockroachMetadataStore struct
[ ] Implement all 21 MetadataStore trait methods
[ ] Write CockroachDB-specific SQL migrations
[ ] Add distributed transaction handling
[ ] Integration tests
[ ] Performance benchmarks (compare to PostgreSQL)
[ ] Documentation
```

**Estimated effort**: 2 weeks

---

### Phase 3.6: DynamoDB Backend

**Status**: ⏳ Deferred
**Priority**: Low
**When**: For AWS-native deployments

**What**: Implement `DynamoDbMetadataStore` for serverless metadata

**Why defer**:
- PostgreSQL works everywhere (not AWS-specific)
- DynamoDB requires different query patterns (no JOINs)
- Cost/benefit unclear (need customer demand)

**When to revisit**:
- Customer requires AWS-native solution (no RDS)
- Want pay-per-request pricing model
- Need unlimited scale (>1M partitions)

**Implementation checklist**:
```
[ ] Design DynamoDB table schema (single-table design)
[ ] Create DynamoDbMetadataStore struct
[ ] Implement all 21 MetadataStore trait methods
[ ] Handle eventual consistency
[ ] Add GSI (Global Secondary Indexes) for queries
[ ] Integration tests with LocalStack
[ ] Cost modeling (vs RDS)
[ ] Documentation
```

**Estimated effort**: 3 weeks

**Challenges**:
- DynamoDB is eventually consistent (partition leases need strong consistency)
- No JOINs (need to denormalize data)
- Different query patterns (need careful GSI design)

---

### Phase 3.7: Pathological Workload Testing

**Status**: ⏳ Deferred
**Priority**: Medium
**When**: After Phase 4 multi-agent is stable

**What**: Test extreme scenarios (1000 partitions × 1000 tiny batches)

**Why defer**:
- Current demo (1,650 records) proves correctness
- Need multi-agent first to test scalability properly
- Can add stress tests anytime

**When to revisit**:
- Phase 4 complete (multi-agent running)
- Before marketing "handles pathological workloads"
- Customer reports performance issues

**Test scenarios**:
```rust
// Scenario 1: Many partitions, tiny batches
for partition in 0..1000 {
    for batch in 0..1000 {
        produce_record(topic, partition, 10_bytes);
    }
}
// Goal: Metadata cache hit rate > 90%

// Scenario 2: Bursty traffic
for _ in 0..10 {
    produce_burst(10_000 records);
    sleep(60 seconds);
}
// Goal: No bufferbloat, consistent latency

// Scenario 3: Hot partition
for _ in 0..1_000_000 {
    produce_record(topic, partition=0, 1KB);
}
// Goal: Single partition handles high throughput
```

**Success criteria**:
- Metadata query latency < 10ms p99
- Cache hit rate > 90%
- End-to-end write latency < 500ms p99
- No database overload

**Estimated effort**: 1 week

---

### Phase 3.8: Cross-Partition Batching

**Status**: ⏳ Deferred
**Priority**: Low
**When**: For cost optimization at scale

**What**: Batch records from multiple partitions into single S3 object

**Current state**:
```
orders/0/segment.bin  (100 records)
orders/1/segment.bin  (150 records)
orders/2/segment.bin  (80 records)
Total: 3 S3 PUT requests
```

**Optimized state**:
```
orders/batch-00001.bin  (330 records across 3 partitions)
Total: 1 S3 PUT request
```

**Why defer**:
- Current approach works fine
- Adds complexity (need to track partition boundaries within batches)
- Only matters at high scale (>10,000 partitions)

**When to revisit**:
- S3 PUT costs become significant (>$50/month)
- Pathological workload testing reveals bottleneck
- Customer requires ultra-low-cost deployment

**Benefits**:
- Fewer S3 PUT requests = lower cost
- Better compression (larger batches)
- Amortized latency

**Challenges**:
- Need to track partition boundaries in batch
- Read path more complex (find partition within batch)
- Lease coordination trickier (who batches what?)

**Estimated effort**: 2 weeks

---

## Phase 2 Deferred Items

### Phase 2.2: Kafka Protocol Compatibility

**Status**: ⏳ Deferred (indefinitely)
**Priority**: Low
**When**: If customer demand requires drop-in Kafka replacement

**What**: Implement Kafka wire protocol for producer/consumer compatibility

**Why defer**:
- gRPC API sufficient for now
- Kafka protocol extremely complex (100+ message types)
- Can wrap StreamHouse with Kafka proxy if needed

**When to revisit**:
- Customer requires existing Kafka clients (no code changes)
- Marketing wants "Kafka-compatible" positioning
- Integration with Kafka ecosystem tools

**Alternatives**:
1. **Kafka Proxy**: Run separate proxy that translates Kafka ↔ StreamHouse gRPC
2. **Client Libraries**: Provide idiomatic clients (Python, Java, Go, Rust)
3. **Connectors**: Build specific connectors (Flink, Spark, etc.)

**Estimated effort**: 8 weeks (very large undertaking)

**Recommendation**: Build client libraries instead of full Kafka protocol

---

## Phase 6+ Future Features

### SQL Stream Processing (Phase 6)

**Status**: 📋 Planned for later
**Priority**: High (our differentiation!)
**When**: After Phase 4 + Phase 5 complete

**What**: Built-in SQL engine for streaming queries

Already documented in [COMPLETE-ROADMAP.md](COMPLETE-ROADMAP.md)

Not deferred - just scheduled for after multi-agent is production-ready.

---

### Schema Registry (Phase 7+)

**Status**: 📋 Planned
**Priority**: Medium
**When**: After SQL processing

**What**: Stateless schema registry for Avro/Protobuf/JSON

**Why later**:
- SQL processing more important (our differentiation)
- Can integrate with Confluent Schema Registry for now
- Need solid foundation first

**Alternatives** (interim):
- Use Confluent Schema Registry (compatible)
- Store schemas in topic configs (metadata store)
- Client-side schema validation

---

### Tableflow - Iceberg Integration (Phase 7+)

**Status**: 📋 Planned
**Priority**: Medium
**When**: After Schema Registry

**What**: Automatic Iceberg table generation from topics

**Use case**: Query streaming data with SQL engines (Trino, Athena, Spark)

**Why later**: Need stable format first

---

### S3 Express One Zone Support (Phase 7+)

**Status**: 📋 Planned
**Priority**: Low
**When**: For ultra-low latency use cases

**What**: Support S3 Express One Zone storage class

**Benefits**:
- 4x lower latency than S3 Standard
- 10x higher throughput
- Single-AZ deployment (no cross-AZ costs)

**Cost**:
- $0.16/GB vs $0.023/GB (7x more expensive)
- Only makes sense for hot data (< 30 days)

**When to revisit**:
- Customer requires < 10ms write latency
- Cost justifies benefit (high-value use case)

---

## Infrastructure & Operations

### Kubernetes Operator

**Status**: 📋 Future
**Priority**: Medium
**When**: For easier K8s deployments

**What**: Custom K8s operator for StreamHouse

```yaml
apiVersion: streamhouse.io/v1
kind: StreamHouseCluster
metadata:
  name: prod-cluster
spec:
  agents:
    replicas: 10
    instanceType: c6i.xlarge
  metadata:
    type: rds-postgresql
  storage:
    type: s3
    bucket: streamhouse-prod
```

**Why later**: Manual K8s manifests work fine for now

---

### Web UI Dashboard

**Status**: 📋 Future
**Priority**: High (for managed service)
**When**: Before commercial launch

**What**: Customer-facing dashboard

**Features**:
- Topic management (create, configure, delete)
- Consumer group monitoring
- Usage analytics
- Query builder (SQL)
- Billing

**Not needed**: Until we have customers

---

### Multi-Region Replication (Orbit)

**Status**: 📋 Future
**Priority**: Low
**When**: For disaster recovery or multi-region active-active

**What**: Cross-region topic replication

**Use cases**:
- Disaster recovery (backup region)
- Multi-region active-active (low latency everywhere)
- Data sovereignty (EU data in EU region)

**Complexity**: High (conflict resolution, offset translation)

---

## Documentation Gaps

### Missing Docs (To Write Later)

1. **Architecture Deep Dive** (after Phase 4)
   - How leases work
   - Failure scenarios
   - Performance tuning

2. **Operations Guide** (after Phase 4)
   - Deployment patterns
   - Monitoring setup
   - Troubleshooting

3. **API Reference** (after Phase 6)
   - Complete gRPC API docs
   - SQL query reference
   - Configuration options

4. **Tutorials** (after Phase 6)
   - Getting started
   - Common patterns
   - Integration guides

---

## Performance Optimizations

### Potential Future Optimizations

1. **Zero-Copy Reads** (Low priority)
   - Use io_uring for S3 downloads
   - Avoid buffer copies in hot path
   - Estimated gain: 10-20% read throughput

2. **Custom Compression** (Low priority)
   - Evaluate Zstandard vs LZ4
   - Train dictionaries for better compression
   - Estimated gain: 20-30% storage savings

3. **Segment Index Optimization** (Medium priority)
   - Use bloom filters for key lookups
   - Cache hot segments in memory
   - Estimated gain: 50% reduction in S3 API calls

4. **Connection Pooling** (Medium priority)
   - Reuse gRPC connections between agents
   - Reduce connection overhead
   - Estimated gain: 5-10% latency reduction

---

## Monitoring & Alerting

### Enhanced Observability (Future)

**Current state**: Basic metrics (CPU, memory, throughput)

**Future enhancements**:
1. Distributed tracing (OpenTelemetry)
2. Request flamegraphs (performance profiling)
3. Query cost tracking (per customer)
4. Anomaly detection (ML-based alerting)
5. Capacity planning dashboard

**When**: After Phase 5 (production hardening)

---

## How to Use This Document

**When adding deferred work**:
1. Add section with clear title
2. Mark priority (High/Medium/Low)
3. Explain why deferred
4. Define "when to revisit" criteria
5. Estimate effort
6. Link to related docs

**When revisiting**:
1. Check "when to revisit" conditions
2. Evaluate current priority
3. Create implementation plan
4. Move to active roadmap
5. Update status here

---

## Summary

**Deferred ≠ Canceled**

These are all valuable features that we'll build eventually. We're just being strategic about sequencing:

1. **Phase 4** (now): Multi-agent architecture
2. **Phase 5**: Production hardening
3. **Phase 6**: SQL processing (our differentiation!)
4. **Phase 7+**: Everything else

The deferred items mostly fall into:
- **Nice-to-haves** (DynamoDB backend, Kafka protocol)
- **Premature optimizations** (cross-partition batching)
- **Ecosystem features** (schema registry, Tableflow)

We'll circle back when:
- Customer demand justifies it
- Core platform is stable
- We have bandwidth

**Focus now**: Ship Phase 4, achieve WarpStream parity, then differentiate with SQL processing! 🚀

---

**Review cadence**: Every 3 months, revisit priorities based on:
- Customer feedback
- Performance bottlenecks discovered
- Competitive landscape changes
- Engineering bandwidth available
