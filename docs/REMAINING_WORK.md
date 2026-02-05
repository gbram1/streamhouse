# StreamHouse: Remaining Work & Roadmap

**Date**: February 4, 2026
**Current Status**: v1.0 Production Ready - Client SDKs Complete
**See Also**: [REMAINING_PHASES.md](./REMAINING_PHASES.md) for latest status

---

## Executive Summary

**Completed:**
- ✅ Phases 1-6: Core streaming platform (storage, agents, Producer/Consumer APIs)
- ✅ Phase 8: Schema Registry REST API & Avro compatibility
- ✅ Phase 8.5: Unified Server (single binary)
- ✅ **Phase 9: Schema Registry PostgreSQL storage + Producer integration**
- ✅ **Phase 12.4.1: Write-Ahead Log (core functionality)**
- ✅ **Phase 24: Stream JOINs (INNER, LEFT, RIGHT, FULL)**
- ✅ **Phase 25: Materialized Views (background maintenance, PostgreSQL storage)**
- ✅ **Phase 12.1: Client SDKs (Python, TypeScript, Java, Go + test suites)**
- ✅ **Phase 9.3-9.4: Advanced Consumer (coordinator, compaction, wildcard subscriptions)**

**Remaining Critical Work:** ~8 weeks to feature-complete
**Most Critical:** Phase 8.2-8.5 (Performance)

---

## Immediate Critical Path: Phase 12.4 Completion

These are **blocking issues** for production use identified in the roadmap.

### Phase 12.4.2: S3 Throttling Protection ⚠️ **CRITICAL**
**Priority**: HIGHEST
**Effort**: 40 hours
**Status**: NOT STARTED

**Problem:** Cascading failures during S3 rate limit events can cause data loss and service outages.

**Implementation Tasks:**
1. **Rate Limiter** (~200 LOC)
   - Per-operation rate limiting (GET/PUT/DELETE)
   - Token bucket algorithm with configurable rates
   - Separate limits for different S3 operations
   - Files: `crates/streamhouse-storage/src/rate_limiter.rs` (new)

2. **Circuit Breaker** (~200 LOC)
   - Fail fast after N consecutive failures
   - Half-open state for testing recovery
   - Exponential backoff with jitter
   - Files: `crates/streamhouse-storage/src/circuit_breaker.rs` (new)

3. **Backpressure** (~100 LOC)
   - Reject writes when over S3 rate limit
   - Return 429 (Too Many Requests) to producers
   - Gradual recovery when throttling stops
   - Files: Modify `crates/streamhouse-agent/src/grpc_server.rs`

4. **Prefix Sharding** (~100 LOC)
   - Distribute load across multiple S3 prefixes
   - Avoid hot partition issues
   - Configuration: `S3_PREFIX_COUNT` env var
   - Files: Modify `crates/streamhouse-storage/src/writer.rs`

**Testing:**
- Chaos test: Inject S3 throttling (simulate 503 responses)
- Verify graceful degradation (no data loss)
- Measure recovery time after throttling stops
- Integration test with actual S3 rate limits

**Success Criteria:**
- Zero data loss under throttling
- Graceful degradation (backpressure to producers)
- Auto-recovery when throttling stops
- < 10% overhead in normal conditions

---

### Phase 12.4.3: Operational Runbooks
**Priority**: HIGH
**Effort**: 30 hours
**Status**: NOT STARTED

**Problem:** Customers can't debug/fix issues without operational guidance.

**Deliverables:**
1. **Runbook: Consumer Lag Troubleshooting** (`docs/runbooks/consumer-lag.md`)
   - Symptoms: Lag increasing over time
   - Root causes: Slow processing, partition skew, network issues
   - Debugging steps: Check metrics, inspect segments, verify offsets
   - Solutions: Add consumers, optimize processing, rebalance

2. **Runbook: S3 Throttling Response** (`docs/runbooks/s3-throttling.md`)
   - Symptoms: 503 errors, write failures, increased latency
   - Root causes: Rate limits exceeded, hot partitions
   - Debugging steps: Check CloudWatch metrics, S3 request rates
   - Solutions: Enable prefix sharding, reduce write rate, increase limits

3. **Runbook: PostgreSQL Failover** (`docs/runbooks/postgres-failover.md`)
   - Symptoms: Metadata queries failing, connection timeouts
   - Root causes: Primary down, network partition, replica lag
   - Debugging steps: Check PostgreSQL logs, verify connectivity
   - Solutions: Promote replica, update connection string, restart agents

4. **Runbook: Segment Corruption Recovery** (`docs/runbooks/segment-corruption.md`)
   - Symptoms: CRC mismatch, deserialization errors
   - Root causes: Partial S3 uploads, bit rot, software bugs
   - Debugging steps: Download segment, verify checksum, inspect bytes
   - Solutions: Re-upload from WAL, restore from backup, skip segment

5. **Runbook: Producer Backpressure** (`docs/runbooks/producer-backpressure.md`)
   - Symptoms: Producer send() timeouts, 429 errors
   - Root causes: Agent overload, S3 throttling, network congestion
   - Debugging steps: Check agent CPU/memory, S3 metrics, network stats
   - Solutions: Scale agents, reduce write rate, optimize batching

6. **Runbook: Partition Rebalancing Issues** (`docs/runbooks/partition-rebalancing.md`)
   - Symptoms: Uneven partition distribution, lease conflicts
   - Root causes: Agent failures, network partitions, metadata inconsistency
   - Debugging steps: Check partition leases, agent heartbeats, logs
   - Solutions: Release stuck leases, restart agents, force rebalance

7. **FAQ: Common Errors** (`docs/runbooks/FAQ.md`)
   - Topic not found
   - Partition not found
   - Offset out of range
   - Schema validation failed
   - Connection refused

**Testing:**
- Manually trigger each failure scenario
- Verify runbook instructions work
- Update based on real production incidents

---

### Phase 12.4.4: Monitoring & Dashboards
**Priority**: HIGH
**Effort**: 40 hours
**Status**: PARTIAL (observability crate exists, dashboards missing)

**Current State:**
- ✅ `streamhouse-observability` crate exists with basic metrics
- ❌ No Prometheus exporter
- ❌ No Grafana dashboards
- ❌ No alerting rules

**Implementation Tasks:**
1. **Prometheus Exporter** (~400 LOC)
   - HTTP endpoint: `/metrics`
   - Metrics to expose:
     - Producer: throughput, latency, errors, batch size
     - Consumer: lag, throughput, errors, rebalances
     - S3: request rate, throttling events, upload latency
     - PostgreSQL: query latency, connection pool size
     - Segments: flush rate, size distribution
     - WAL: append rate, recovery count, size
   - Files: `crates/streamhouse-observability/src/prometheus_exporter.rs` (new)

2. **Grafana Dashboards** (5 dashboards)
   - **Overview Dashboard** (`grafana/dashboards/overview.json`)
     - System health: uptime, error rate, request rate
     - Resource usage: CPU, memory, disk, network
     - Key metrics: producer throughput, consumer lag

   - **Producer Metrics** (`grafana/dashboards/producer.json`)
     - Throughput (rec/s, MB/s)
     - Latency (p50, p95, p99)
     - Error rate by type
     - Batch size distribution
     - Schema registry cache hit rate

   - **Consumer Lag Monitoring** (`grafana/dashboards/consumer-lag.json`)
     - Lag by consumer group and partition
     - Consumption rate (rec/s)
     - Time to catch up (estimated)
     - Rebalance events
     - Error rate

   - **Storage Metrics** (`grafana/dashboards/storage.json`)
     - S3 request rate (GET/PUT/DELETE)
     - S3 throttling events (503 count)
     - Segment flush rate and latency
     - WAL size and append rate
     - Disk usage by partition

   - **Error Tracking** (`grafana/dashboards/errors.json`)
     - Error rate by type and component
     - Top errors (ranked by frequency)
     - Error trends over time
     - Alert history

3. **Alerting Rules** (`prometheus/alerts.yml`)
   - Consumer lag > 1M messages
   - S3 throttling sustained > 1 minute
   - Error rate > 1%
   - Uptime < 99.9%
   - Disk usage > 80%
   - PostgreSQL query latency > 1s
   - WAL size > 90% of max
   - Producer send failures > 5% of requests
   - Partition lease conflicts detected
   - Agent heartbeat missed (> 2 minutes)

**Testing:**
- Deploy to test environment
- Generate load and verify metrics appear
- Trigger alerts manually
- Verify alert notifications work

---

## Phase 7: Observability (Remaining 60%)

**Status**: PARTIAL (Phase 12.4.4 covers monitoring, this covers tracing/logging)
**Priority**: MEDIUM
**Effort**: 40 hours
**Prerequisite**: Phase 12.4.4 complete

**What's Missing:**
1. **Distributed Tracing** (OpenTelemetry)
   - Trace requests across Producer → Agent → S3 → Consumer
   - Visualize latency breakdown by component
   - Identify slow queries and bottlenecks
   - Integration with Jaeger or Tempo

2. **Log Aggregation** (Loki/ELK)
   - Centralized logging for all components
   - Structured JSON logs with context
   - Log correlation with trace IDs
   - Searchable log archive

3. **Performance Profiling Dashboards**
   - Flame graphs for CPU profiling
   - Memory allocation tracking
   - Slow query analysis
   - Bottleneck identification

4. **Cost Attribution**
   - Track costs by topic/partition
   - S3 storage costs
   - S3 request costs
   - Compute costs (agent hours)
   - Generate cost reports

**Effort Breakdown:**
- OpenTelemetry integration: 20 hours
- Log aggregation setup: 10 hours
- Profiling dashboards: 5 hours
- Cost attribution: 5 hours

---

## Phase 10: Transactions and Exactly-Once ⏳ CRITICAL GAP

**Status**: NOT STARTED
**Priority**: HIGH (blocks financial use cases)
**Effort**: 200+ hours (4-6 weeks)
**Prerequisite**: Phase 12.4 complete, $100K+ ARR

**Problem:** Without exactly-once semantics, StreamHouse can't be used for financial transactions or critical event processing.

**Features Needed:**
1. **Idempotent Producer** (~80 hours)
   - Producer ID + sequence number per partition
   - Deduplication based on (producer_id, sequence)
   - Persist producer state in metadata store
   - Reject duplicate writes

2. **Transaction Log** (~40 hours)
   - Track in-flight transactions
   - Transaction coordinator service
   - BEGIN/COMMIT/ABORT primitives
   - Transaction timeout handling

3. **Two-Phase Commit** (~60 hours)
   - Coordinator-based atomic commit
   - Prepare phase: validate all partitions
   - Commit phase: mark transaction complete
   - Rollback on any partition failure

4. **Consumer Transaction Isolation** (~20 hours)
   - Read committed only (default)
   - Skip uncommitted messages
   - Transaction markers in segments
   - Consumer offset management with transactions

**Complexity:** HIGH - Distributed systems problem with edge cases
**Risk:** Implementation bugs could cause data loss

**When to Implement:**
- After Phase 12.4 complete (production hardening)
- When customers request exactly-once
- When financial use cases appear
- When you have $100K+ ARR to justify complexity

---

## Phase 9: Advanced Consumer Features (Dynamic Rebalancing)

**Status**: NOT STARTED
**Priority**: MEDIUM
**Effort**: 120 hours (3 weeks)
**Prerequisite**: Phase 12.4 complete

**Current State:**
- ✅ Basic consumer groups work
- ✅ Manual partition assignment works
- ❌ No dynamic rebalancing on member join/leave
- ❌ No heartbeats or liveness detection
- ❌ No generation IDs or fencing

**Features Needed:**
1. **Consumer Heartbeats** (~20 hours)
   - Periodic heartbeat from consumer to coordinator
   - Detect dead consumers (timeout = 30s)
   - Trigger rebalance on timeout

2. **Coordinator Election** (~30 hours)
   - Elect coordinator for each consumer group
   - Coordinator manages rebalance protocol
   - Leader-based approach (like Kafka)

3. **Generation IDs** (~10 hours)
   - Increment generation on each rebalance
   - Fence stale consumers with old generation
   - Prevent dual ownership of partitions

4. **Partition Assignment Strategies** (~40 hours)
   - Range assignment (contiguous partitions)
   - Round-robin assignment (distribute evenly)
   - Sticky assignment (minimize movement)
   - Custom assignor interface

5. **Incremental Cooperative Rebalancing** (~20 hours)
   - Don't stop all consumers during rebalance
   - Only reassign changed partitions
   - Minimal disruption to processing

**When to Implement:**
- When consumers frequently join/leave
- When manual assignment is too complex
- When customers request Kafka-compatible rebalancing

---

## Phase 11: Distributed Architecture

**Status**: DEFERRED
**Priority**: LOW (until single-server limits reached)
**Effort**: 160 hours (4 weeks)
**Prerequisite**: >$500K ARR, >100K req/s traffic

**Current Architecture:**
- Unified server: REST API + gRPC + WriterPool in one binary
- Works well for < 50K req/s
- Single point of failure

**Future Architecture:**
- Separate API servers (stateless) from storage agents (stateful)
- Multiple API servers behind load balancer
- Horizontal scaling for high throughput
- Multi-AZ deployment for HA

**When to Implement:**
- Traffic exceeds 50K req/s sustained
- Need 99.99% uptime SLA
- Multi-region deployment required
- Customers demand horizontal scaling

**Features:**
1. Separate API servers from storage agents
2. gRPC-based communication (API → Agent)
3. Load balancing (HAProxy, Nginx)
4. Configuration mode toggle (unified vs distributed)

---

## Business Validation Track (Parallel)

While doing technical work, run business validation in parallel:

### Phase 12.1: Customer Discovery (Weeks 1-2)
- Interview 20 potential customers
- Validate pain points
- Identify 4+ design partner candidates
- **Decision Gate**: GO/NO-GO on technical validation

### Phase 12.2: Technical Validation (Weeks 3-6)
- Chaos engineering test suite
- Performance benchmarks (10TB, 100M msgs/day)
- Cost validation (actual AWS bills)
- **Decision Gate**: GO/NO-GO on design partner POCs

### Phase 12.3: Design Partner POCs (Weeks 7-12)
- Deploy with 3 design partners
- Run 30-day pilots
- Monitor uptime, cost savings, feedback
- **Decision Gate**: GO/PIVOT/NO-GO on v1 product

---

## Priority Ranking

### Must Do (Next 2-4 Weeks)
1. **Phase 12.4.2: S3 Throttling Protection** (40h) ⚠️ CRITICAL
2. **Phase 12.4.3: Operational Runbooks** (30h) - Can start in parallel
3. **Phase 12.4.4: Monitoring & Dashboards** (40h)

**Total:** 110 hours (~3 weeks full-time)

### Should Do (Next 1-2 Months)
4. **Phase 7: Observability** (40h) - After 12.4.4
5. **Phase 10: Exactly-Once** (200h) - When customers request it
6. **Phase 9: Dynamic Rebalancing** (120h) - When needed

**Total:** 360 hours (~9 weeks full-time)

### Nice to Have (Future)
7. **Phase 11: Distributed Architecture** (160h) - When traffic demands it
8. Schema Registry Consumer Integration - Optional
9. Protobuf/JSON Schema improvements - Optional

---

## Effort Summary

| Phase | Priority | Effort | Timeline | Blocking? |
|-------|----------|--------|----------|-----------|
| 12.4.2: S3 Throttling | CRITICAL | 40h | Week 1 | YES |
| 12.4.3: Runbooks | HIGH | 30h | Week 1-2 | NO (parallel) |
| 12.4.4: Monitoring | HIGH | 40h | Week 2-3 | YES |
| 7: Observability | MEDIUM | 40h | Week 4 | NO |
| 10: Exactly-Once | HIGH | 200h | Weeks 5-9 | NO |
| 9: Rebalancing | MEDIUM | 120h | Weeks 10-12 | NO |
| 11: Distributed | LOW | 160h | Future | NO |
| **TOTAL** | | **630h** | **~16 weeks** | |

**Critical Path to Production:** 110 hours (Phase 12.4.2 + 12.4.3 + 12.4.4)

---

## Recommended Approach

### Option A: Sprint to Production (3 weeks)
Focus on critical path only:
1. Week 1: Phase 12.4.2 (S3 Throttling)
2. Week 2: Phase 12.4.3 (Runbooks) + 12.4.4 start
3. Week 3: Phase 12.4.4 (Monitoring) finish

**Outcome:** Production-ready system with core safety features

### Option B: Thorough Approach (5-6 weeks)
Add observability:
1. Weeks 1-3: Phase 12.4 (as above)
2. Week 4: Phase 7 (Observability)
3. Weeks 5-6: Testing, documentation, polish

**Outcome:** Highly observable production system

### Option C: Feature-Complete (12-16 weeks)
Full implementation:
1. Weeks 1-3: Phase 12.4
2. Week 4: Phase 7
3. Weeks 5-9: Phase 10 (Exactly-Once)
4. Weeks 10-12: Phase 9 (Rebalancing)
5. Weeks 13-16: Testing, docs, polish

**Outcome:** Feature-complete system competitive with Kafka

---

## Success Metrics

### Phase 12.4 Complete
- ✅ Zero data loss under S3 throttling
- ✅ Graceful degradation with backpressure
- ✅ Comprehensive runbooks for top 6 failure scenarios
- ✅ Real-time monitoring with 5 Grafana dashboards
- ✅ 10 alert rules configured
- ✅ All chaos tests passing

### Production Ready
- ✅ 99.9%+ uptime over 30 days
- ✅ < 5% performance overhead from safety features
- ✅ Mean time to recovery (MTTR) < 10 minutes
- ✅ All production incidents documented in runbooks
- ✅ Zero critical bugs in 30-day window

---

## Current Status Snapshot

**What Works Today:**
- ✅ Producer API (198K rec/s)
- ✅ Consumer API (32K rec/s)
- ✅ Schema Registry with Avro
- ✅ WAL for crash recovery
- ✅ PostgreSQL metadata
- ✅ MinIO/S3 storage
- ✅ Unified server binary
- ✅ Basic metrics
- ✅ Stream JOINs (INNER, LEFT, RIGHT, FULL)
- ✅ Materialized Views (continuous/periodic/manual refresh)
- ✅ SQL Workbench UI
- ✅ Client SDKs (Python, TypeScript, Java, Go)
- ✅ SDK test suites (~120 tests across all SDKs)
- ✅ Advanced Consumer Groups (coordinator, rebalancing protocol)
- ✅ Compacted topics (cleanup_policy: compact)
- ✅ Wildcard subscriptions (list_topics_matching)

**What's Missing:**
- ❌ Performance optimizations (Phase 8.2-8.5)
- ❌ Production hardening (security, HA, DR)
- ❌ Framework integrations (Spring Boot, FastAPI, etc.)
- ❌ Exactly-once semantics

**Risk Level:** MEDIUM
- Current system works but lacks production hardening
- S3 throttling could cause cascading failures
- Limited observability for debugging
- Missing exactly-once blocks financial use cases

---

## Next Step

**Recommendation:** Start with Phase 12.4.2 (S3 Throttling Protection)

**Rationale:**
- Most critical production gap
- 40 hours of focused work
- High impact on reliability
- Unblocks remaining work

**Alternative:** If you prefer documentation/operations first, start with Phase 12.4.3 (Runbooks) - can be done in parallel with development work.

**What would you like to tackle next?**
