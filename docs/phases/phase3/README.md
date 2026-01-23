# Phase 3: Scalable Metadata (Weeks 17-24)

**Status**: 🚧 In Progress (Phase 3.1 Complete ✅)
**Timeline**: 8 weeks
**Goal**: WarpStream-style pluggable metadata service for 10K+ partitions

---

## Why This Phase Is Critical

This is **the most important phase** for StreamHouse's success. WarpStream's breakthrough came from their hyper-scalable metadata service, not just S3 storage.

> "The ability to handle pathological workloads (really high volumes of tiny batches spread across 10s or 100s of thousands of partitions) is really tough for completely stateless architectures so we spent a lot of time making sure the control plane could handle that."
> — WarpStream Reddit AMA

Without scalable metadata, StreamHouse would fail at:
- **10,000+ partitions**: SQLite becomes a bottleneck
- **Cross-partition batching**: Need fast metadata lookups across many partitions
- **Multi-agent coordination**: Agents need to share state via metadata store
- **Production workloads**: Real-world usage involves thousands of topics/partitions

---

## Phase Overview

### What We're Building

A **pluggable metadata architecture** that supports multiple backends:

```
┌─────────────────────────────────────────┐
│         Application Layer               │
│    (Server, Agents, API Handlers)       │
└─────────────────┬───────────────────────┘
                  │
         ┌────────▼────────┐
         │ MetadataStore   │  ◄── Common Trait Interface
         │     Trait       │
         └────────┬────────┘
                  │
    ┌─────────────┼─────────────┬──────────────┐
    │             │             │              │
┌───▼────┐  ┌────▼─────┐  ┌────▼─────┐  ┌────▼─────┐
│ SQLite │  │PostgreSQL│  │CockroachDB│  │ DynamoDB │
│ (Dev)  │  │ (10K)    │  │  (100K)   │  │(Unlimited)│
└────────┘  └──────────┘  └───────────┘  └──────────┘
```

### Key Features

1. **Backend Agnostic**: Switch databases without changing application code
2. **Caching Layer**: 90%+ cache hit rate for metadata queries
3. **BTreeMap Indexes**: O(log n) partition→segment lookups
4. **Agent Coordination**: Support for Phase 4 multi-agent architecture

---

## Phase 3 Breakdown

### ✅ Phase 3.1: Abstract Metadata Interface (Week 17) - COMPLETE

**Goal**: Extend MetadataStore trait to support all future operations

**Deliverables**:
- ✅ Agent coordination operations (8 new methods)
- ✅ `AgentInfo` and `PartitionLease` types
- ✅ Database migration for agent tables
- ✅ SQLite implementation of agent operations
- ✅ All tests passing (29 tests)

**Documentation**: [3.1-METADATA-INTERFACE.md](3.1-METADATA-INTERFACE.md)

---

### 📋 Phase 3.2: PostgreSQL Backend (Week 18)

**Goal**: Production-ready metadata backend for 10K partitions

**Deliverables**:
- [ ] `PostgresMetadataStore` implementation
- [ ] Connection pooling with sqlx
- [ ] All MetadataStore methods implemented
- [ ] Migration scripts for PostgreSQL
- [ ] Performance testing (< 10ms p99)

**Target**: 10,000 partitions, < 10ms metadata queries

---

### 📋 Phase 3.3: Metadata Caching Layer (Week 19)

**Goal**: 90%+ cache hit rate for metadata queries

**Deliverables**:
- [ ] `CachedMetadataStore` wrapper (decorator pattern)
- [ ] LRU cache for topics (5 min TTL)
- [ ] LRU cache for partitions (1 min TTL)
- [ ] Cache invalidation on writes
- [ ] Cache metrics and monitoring

**Target**: 90% cache hit rate, < 1ms cached queries

---

### 📋 Phase 3.4: Partition→Segment Index Optimization (Week 20)

**Goal**: O(log n) offset lookups instead of linear scan

**Deliverables**:
- [ ] In-memory BTreeMap index per partition
- [ ] Efficient range queries for offset→segment mapping
- [ ] Background index refresh thread
- [ ] Memory-bounded index (configurable size)

**Performance**: < 1ms p99 for offset lookups

---

### 📋 Phase 3.5: CockroachDB Backend (Week 21)

**Goal**: Distributed metadata backend for 100K partitions

**Deliverables**:
- [ ] `CockroachMetadataStore` implementation
- [ ] Distributed transactions support
- [ ] Multi-region configuration
- [ ] Automatic replication

**Target**: 100,000 partitions, multi-region deployment

---

### 📋 Phase 3.6: DynamoDB Backend (Week 22 - Optional)

**Goal**: AWS-native metadata backend

**Deliverables**:
- [ ] `DynamoDbMetadataStore` implementation
- [ ] Pay-per-request pricing support
- [ ] Global secondary indexes
- [ ] Auto-scaling configuration

**Target**: Unlimited partitions, AWS-native integration

---

### 📋 Phase 3.7: Pathological Workload Testing (Week 23)

**Goal**: Validate 10K+ partition handling

**Test Scenario**:
```rust
// 1000 partitions × 1000 tiny batches = 1M writes
for partition in 0..1000 {
    for batch in 0..1000 {
        produce_record(
            topic = "test",
            partition = partition,
            value = 10 bytes,
        );
    }
}
```

**Success Criteria**:
- ✅ Metadata query latency < 10ms p99
- ✅ Cache hit rate > 90%
- ✅ End-to-end write latency < 500ms p99
- ✅ No database overload

---

### 📋 Phase 3.8: Cross-Partition Batching (Week 24)

**Goal**: Reduce S3 costs by batching writes across partitions

**Deliverables**:
- [ ] `CrossPartitionWriter` struct
- [ ] Batch records from multiple partitions into single S3 object
- [ ] Metadata tracking for cross-partition segments
- [ ] Configurable batching window (100ms default)

**Benefits**:
- Lower S3 costs (fewer PUT requests)
- Amortized latency across records
- Better resource utilization

---

## Architecture Decisions

### Why Pluggable Backends?

Different deployment scenarios need different metadata stores:

| Scenario | Backend | Why |
|----------|---------|-----|
| Local development | SQLite | Zero config, embedded |
| Single-region prod | PostgreSQL | Reliable, 10K partitions |
| Multi-region prod | CockroachDB | Global consistency, 100K partitions |
| AWS deployment | DynamoDB | Managed service, unlimited scale |

### Why Caching?

Metadata access patterns are highly cacheable:
- Topics are read frequently, created rarely
- Partition metadata changes only on writes
- Segment metadata is append-only (never modified)

With 90% cache hit rate:
- 10ms database query → 0.1ms cache lookup
- 100x improvement in p99 latency
- Reduced database load

### Why BTreeMap Index?

Linear scan for offset→segment mapping is O(n):
```rust
// Bad: Linear scan through all segments
for segment in segments {
    if offset >= segment.base_offset && offset <= segment.end_offset {
        return segment;
    }
}
```

BTreeMap provides O(log n) lookup:
```rust
// Good: Binary search in BTreeMap
let segment = index.range(..=offset).next_back();
```

For 1000 segments: 1000 comparisons → 10 comparisons

---

## Success Criteria

Phase 3 is complete when:

- ✅ Multiple metadata backends implemented (SQLite, PostgreSQL, CockroachDB)
- ✅ Cache hit rate > 90% in production workloads
- ✅ Metadata query latency < 10ms p99
- ✅ Support for 10,000+ partitions
- ✅ Pathological workload test passes
- ✅ Cross-partition batching reduces S3 costs by 50%+
- ✅ All integration tests pass with each backend
- ✅ Documentation for backend selection and configuration

---

## Testing Strategy

### Unit Tests
- Each backend implements full MetadataStore trait
- Mock backends for fast testing
- Isolation between test runs

### Integration Tests
- Test against real database instances
- Performance benchmarks for each backend
- Failure scenarios (connection loss, timeouts)

### Load Tests
- 10,000 partition creation
- 1M+ segment metadata writes
- Concurrent reads across all partitions
- Cache effectiveness measurement

### Chaos Tests
- Database failures during operations
- Network partitions
- Cache invalidation edge cases

---

## Performance Targets

| Metric | Phase 1 (SQLite) | Phase 3 (PostgreSQL + Cache) |
|--------|------------------|------------------------------|
| Max partitions | ~1,000 | 10,000+ |
| Metadata query latency (p50) | < 1ms | < 0.5ms (cached) |
| Metadata query latency (p99) | < 5ms | < 10ms |
| Write throughput | 50K writes/sec | 100K writes/sec |
| Cache hit rate | N/A | > 90% |
| Offset lookup time | O(n) linear | O(log n) binary |

---

## Migration Path

### From Phase 2 → Phase 3

1. No application code changes required (trait abstraction)
2. Update config to specify backend
3. Run migration scripts
4. Deploy with new backend
5. Monitor cache hit rates and adjust TTLs

### Rollback Strategy

- Metadata backends are stateless (all state in database)
- Can switch backends by changing config
- SQLite remains available for rollback

---

## Next Steps

After Phase 3:
- **Phase 4**: Multi-Agent Architecture (use agent coordination operations)
- **Phase 5**: Transactions & Exactly-Once Semantics
- **Phase 6**: SQL Stream Processing

---

## Quick Reference

```bash
# Run all Phase 3 tests
cargo test -p streamhouse-metadata

# Test specific backend
cargo test -p streamhouse-metadata test_postgres
cargo test -p streamhouse-metadata test_cockroach

# Benchmark metadata performance
cargo bench -p streamhouse-metadata

# Run pathological workload test
cargo test -p streamhouse-server test_pathological_workload -- --nocapture
```

---

**Status**: Phase 3.1 complete! Moving to PostgreSQL backend implementation.
