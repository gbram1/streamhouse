# StreamHouse: Integrated Technical & Business Roadmap

**Last Updated:** February 4, 2026
**Current Status:** Phase 13 (UI), Phase 16 (Exactly-Once), Phase 17 (Fast Leader Handoff), Phase 20 (AI Queries), Phase 20.1 (Schema Inference) Complete

---

## Overview

This roadmap integrates **technical development** with **business validation** based on comprehensive market analysis. All original technical phases are preserved, but execution order is reorganized around business priorities.

**Key Insight:** Before building more features, we must validate:
1. Customer demand is real (not just interesting)
2. Product reliability is provable (99.9%+ uptime)
3. Cost savings materialize (60%+ in practice)
4. Archive tier creates enough pull (minimum viable wedge)

---

## Current State: Where We Are Now

### ✅ Completed Phases (Technical Foundation)

**Phase 1-6:** Core streaming platform (storage, agents, Producer/Consumer APIs)
**Phase 8:** Schema Registry
**Phase 8.5:** Unified Server (gRPC + REST + Schema Registry in one binary)

**What Works:**
- ✅ Produce messages via REST API → WriterPool → S3/MinIO
- ✅ Consume messages with consumer groups
- ✅ Schema registry for schema evolution
- ✅ PostgreSQL metadata store
- ✅ 198K rec/s producer throughput
- ✅ 32K rec/s consumer throughput

**What's Missing (Critical Gaps):**
- ❌ No Write-Ahead Log (data loss risk during S3 flush)
- ❌ No S3 throttling protection (cascading failures)
- ❌ No exactly-once semantics (blocks financial workloads)
- ❌ No operational runbooks (customers can't fix issues)
- ❌ No chaos testing (reliability unproven)
- ❌ No monitoring dashboards (can't see what's happening)
- ❌ No managed service (high ops burden)

---

## Execution Strategy: Two Parallel Tracks

### Track A: Business Validation (Weeks 1-12, CRITICAL PATH)
Validates market demand and product-market fit

### Track B: Operational Maturity (Weeks 1-8, PARALLEL)
Fixes critical technical gaps to enable customer pilots

**Both tracks must succeed to proceed with company.**

---

## Track A: Business Validation Phases

### Phase 12: Business Validation & Market Discovery ⏳ IN PROGRESS

**Timeline:** Weeks 1-12 (Next 3 months)
**Goal:** Validate customer demand and prove reliability before scaling
**Status:** Starting now

#### Phase 12.1: Customer Discovery (Weeks 1-2)

**Objectives:**
- [ ] Interview 20 potential customers
  - 8 companies with high Kafka bills (>$5K/month)
  - 6 companies building new data pipelines
  - 6 companies with compliance/archival needs
- [ ] Validate pain points (target: 15/20 say "painful")
- [ ] Validate willingness to try archive tier (target: 10/20 say "yes")
- [ ] Identify 4+ design partner candidates

**Deliverables:**
- Interview notes spreadsheet
- Pain point analysis report
- Design partner candidate list
- Go/No-Go decision for Phase 12.2

**Exit Criteria:**
- ✅ GO: 15+ validate pain, 4+ design partners identified
- ❌ NO-GO: Lukewarm response, no design partners → **PIVOT or SHELVE**

**Effort:** 40 hours (interviews, analysis, outreach)

#### Phase 12.2: Technical Validation (Weeks 3-6)

**Objectives:**
- [ ] Chaos engineering test suite (S3 throttling, failures, partitions)
- [ ] Performance benchmarks (10TB dataset, 100M msgs/day, 7 days)
- [ ] Cost validation (actual AWS bill for 10TB target: <$600)
- [ ] Latency validation (p50/p95/p99 under load)
- [ ] Build Kafka Connect source plugin (archive tier connector)

**Chaos Tests Required:**
```
1. S3 throttling (reduce rate to 100 req/s)
   Expected: Graceful backpressure, no data loss

2. S3 complete outage (5 minutes)
   Expected: WAL prevents data loss, auto-recovery

3. PostgreSQL failover (primary → replica)
   Expected: <10s downtime, no data loss

4. Network partition (split-brain)
   Expected: Prevent dual writes, fence old leader

5. Producer burst (10x normal rate)
   Expected: Backpressure, no OOM crash
```

**Performance Targets:**
- Produce p99: <20ms
- Consume p99 (hot): <20ms
- Consume p99 (cold): <200ms
- Uptime: >99.9% over 7 days
- Cost: <$600 for 10TB/month

**Deliverables:**
- Chaos test suite (automated)
- Performance benchmark report
- Cost breakdown (actual AWS bill)
- Kafka Connect plugin (initial version)
- Go/No-Go decision for Phase 12.3

**Exit Criteria:**
- ✅ GO: All chaos tests pass, 99.9% uptime, 60%+ savings
- ❌ NO-GO: Can't solve S3 throttling, costs too high → **PIVOT or SHELVE**

**Effort:** 80 hours (testing infrastructure, connector)

#### Phase 12.3: Design Partner POCs (Weeks 7-12)

**Objectives:**
- [ ] Deploy archive tier with 3 design partners
- [ ] Run for 30 days each (monitor uptime, cost, feedback)
- [ ] Weekly check-ins with each partner
- [ ] Iterate based on feedback
- [ ] Collect testimonials and metrics

**Design Partner Criteria:**
- Currently using Kafka in production
- Kafka bill >$2K/month
- Long retention needs (30+ days)
- Willing to provide feedback
- Potential reference customer

**Metrics to Track:**
```
Partner 1: ___________________
├─ Uptime: _____% (target: >99.9%)
├─ Cost savings: _____% (target: >60%)
├─ Support hours/week: _____ (target: <5)
├─ Would pay: ☐ Yes ☐ No ☐ Maybe
└─ Would recommend: ☐ Yes ☐ No ☐ Maybe

Partner 2: ___________________
[same metrics]

Partner 3: ___________________
[same metrics]
```

**Deliverables:**
- 3 successful POC deployments
- 30-day uptime reports
- Cost savings analysis (before/after)
- Customer feedback summaries
- Case study drafts (if successful)
- Final Go/No-Go decision

**Exit Criteria:**
- ✅ STRONG GO: 3/3 successful, 2+ would pay, 60%+ savings, 99.9%+ uptime → **Proceed to Phase 13**
- ⚠️ QUALIFIED GO: 2/3 successful, mixed feedback → **3-month improvement cycle, reassess**
- ❌ NO-GO: <2 successful, major incidents, partners churned → **PIVOT or SHELVE**

**Effort:** 100 hours (deployment, monitoring, support)

**Total Phase 12 Effort:** 220 hours (~6 weeks full-time)

---

## Track B: Operational Maturity Phases (Parallel with Phase 12)

### Phase 12.4: Critical Operational Fixes ⏳ PARALLEL (Weeks 1-8)

**Goal:** Fix gaps that would block customer pilots
**Runs in parallel with Phase 12.1-12.2**

#### Sub-Phase 12.4.1: Write-Ahead Log (Week 1-2)

**Problem:** Data loss if server crashes before S3 flush
**Impact:** CRITICAL (trust/correctness issue)

**Implementation:**
- [ ] WAL writer (append-only local disk log)
- [ ] Replay on startup (recover unflushed messages)
- [ ] Configurable retention (delete after S3 confirmation)
- [ ] Performance testing (measure overhead)

**Deliverables:**
- WAL implementation (~500 LOC)
- Unit tests (10+)
- Integration tests (5+)
- Performance report (overhead <5%)

**Effort:** 40 hours

#### Sub-Phase 12.4.2: S3 Throttling Protection (Week 3-4)

**Problem:** Cascading failures during S3 rate limit events
**Impact:** CRITICAL (reliability issue)

**Implementation:**
- [ ] Rate limiter per S3 operation type (GET/PUT)
- [ ] Exponential backoff with jitter
- [ ] Circuit breaker (fail fast after N retries)
- [ ] Backpressure to producers (reject when over limit)
- [ ] Prefix sharding (distribute load across S3 prefixes)

**Deliverables:**
- Rate limiter implementation (~300 LOC)
- Circuit breaker (~200 LOC)
- Chaos test (trigger throttling, verify graceful degradation)
- Configuration guide

**Effort:** 40 hours

#### Sub-Phase 12.4.3: Operational Runbooks (Week 5-6)

**Problem:** Customers can't debug/fix issues
**Impact:** HIGH (adoption blocker)

**Deliverables:**
- [ ] Runbook: Consumer lag troubleshooting
- [ ] Runbook: S3 throttling response
- [ ] Runbook: PostgreSQL failover
- [ ] Runbook: Segment corruption recovery
- [ ] Runbook: Producer backpressure
- [ ] Runbook: Partition rebalancing issues
- [ ] FAQ: Common errors and solutions
- [ ] Error message improvements (actionable guidance)

**Effort:** 30 hours

#### Sub-Phase 12.4.4: Monitoring & Dashboards (Week 7-8)

**Problem:** Can't see what's happening (blind operations)
**Impact:** HIGH (operational burden)

**Implementation:**
- [ ] Prometheus metrics exporter
  - Producer throughput, latency, errors
  - Consumer lag, throughput, errors
  - S3 request rate, throttling events
  - PostgreSQL query latency
  - Segment flush rate
- [ ] Grafana dashboard templates
  - Overview dashboard
  - Producer metrics
  - Consumer lag monitoring
  - Storage metrics
  - Error rates
- [ ] Alerting rules (Prometheus AlertManager)
  - Consumer lag >1M messages
  - S3 throttling sustained >1 minute
  - Uptime <99.9%
  - Error rate >1%

**Deliverables:**
- Prometheus exporter (~400 LOC)
- 5 Grafana dashboards (JSON)
- 10 alert rules (YAML)
- Monitoring setup guide

**Effort:** 40 hours

**Total Phase 12.4 Effort:** 150 hours (~4 weeks full-time)

---

## Phase 13: Archive Tier v1 (Minimum Viable Product)

**Timeline:** Weeks 13-20 (2 months)
**Prerequisite:** Phase 12 STRONG GO decision
**Goal:** Ship minimal product that creates customer pull

### Phase 13.1: Kafka Connect Plugin (Weeks 13-15)

**Scope:**
- [ ] Kafka Connect source connector
- [ ] Copy messages from Kafka topics to StreamHouse
- [ ] Configurable topic mapping
- [ ] Offset tracking (resume from last position)
- [ ] Error handling and retries
- [ ] Documentation and examples

**What's Included:**
- Connector JAR for Kafka Connect
- Configuration schema
- Integration with unified server
- SMT (Single Message Transform) support
- Exactly-once copying (no duplicates)

**What's NOT Included:**
- Real-time consumers (batch only)
- Direct producers (still use Kafka)
- Consumer groups (single reader only)
- Schema evolution (basic support only)

**Deliverables:**
- Kafka Connect plugin JAR
- Configuration guide
- Docker Compose example
- 15 integration tests
- User documentation

**Effort:** 80 hours

### Phase 13.2: Managed Deployment (Weeks 16-18)

**Scope:**
- [ ] Terraform modules (AWS/GCP/Azure)
- [ ] Docker images (published to registry)
- [ ] Helm charts (Kubernetes)
- [ ] Deployment scripts
- [ ] Configuration templates
- [ ] Backup/restore procedures

**Deliverables:**
- Terraform modules for AWS/GCP/Azure
- Docker images on Docker Hub
- Helm chart on artifact registry
- Deployment documentation
- Architecture diagrams

**Effort:** 60 hours

### Phase 13.3: Customer Onboarding (Weeks 19-20)

**Scope:**
- [ ] Onboarding checklist
- [ ] Migration guide (Kafka → StreamHouse)
- [ ] Cost calculator (estimate savings)
- [ ] ROI calculator
- [ ] Video tutorials (5-10 minutes each)
- [ ] Support portal setup

**Deliverables:**
- Onboarding documentation
- Migration playbook
- Cost/ROI calculators (web-based)
- 5 tutorial videos
- Support ticketing system

**Effort:** 40 hours

**Total Phase 13 Effort:** 180 hours (~5 weeks full-time)

---

## Original Technical Phases (Reorganized by Priority)

### TIER 1: Critical for Product-Market Fit

#### Phase 7: Observability and Metrics ⏳ PARTIALLY COMPLETE (via Phase 12.4.4)

**Status:** 60% complete (monitoring done in Phase 12.4.4)
**Remaining Work:**
- [ ] Distributed tracing (OpenTelemetry)
- [ ] Log aggregation (Loki/ELK)
- [ ] Performance profiling dashboards
- [ ] Cost attribution by topic/partition

**Effort:** 40 hours

#### Phase 10: Transactions and Exactly-Once ⏳ CRITICAL GAP

**Status:** PLANNED
**Priority:** HIGH (blocks financial use cases)
**Prerequisite:** Phase 13 complete, $100K+ ARR

**Features:**
- [ ] Idempotent producer (deduplication)
- [ ] Transactional writes (atomic multi-partition)
- [ ] Exactly-once semantics (producer → consumer)
- [ ] Transaction coordinator
- [ ] Fencing (prevent zombie producers)

**Complexity:** HIGH (complex distributed systems problem)
**Effort:** 200+ hours (4-6 weeks)

**Implementation Phases:**
10.1. Idempotent Producer (dedupe based on producer ID + sequence)
10.2. Transaction Log (track in-flight transactions)
10.3. Two-Phase Commit (coordinator-based atomic commit)
10.4. Consumer Transaction Isolation (read committed)
10.5. Testing and Verification

#### Phase 10.5: Producer Ack Modes ⏳ PLANNED

**Status:** PLANNED
**Priority:** HIGH (durability control for customers)
**Prerequisite:** Core producer API complete

**Problem:** Producers currently have no control over durability vs latency trade-off. Data buffered in agent memory can be lost if agent crashes before S3 flush.

**Features:**
- [ ] `AckMode::None` - Fire and forget (fastest, can lose data)
- [ ] `AckMode::Buffered` - Ack after buffer (default, ~1ms, ~30s loss window)
- [ ] `AckMode::Durable` - Ack after S3 flush (~150ms, zero data loss)
- [ ] Kafka protocol mapping (`acks=0/1/all` → AckMode)
- [ ] Per-request ack mode override
- [ ] Metrics: ack latency by mode, data-at-risk bytes

**Trade-offs:**
| Mode | Latency | Durability | Use Case |
|------|---------|------------|----------|
| None | <1ms | Can lose | Metrics, logs |
| Buffered | ~1ms | ~30s window | Default |
| Durable | ~150ms | Zero loss | Financial, critical |

**Effort:** 25 hours (1 week)

#### Phase 10.6: Chaos Testing Suite ⏳ PLANNED

**Status:** PLANNED
**Priority:** HIGH (proves durability claims)
**Prerequisite:** Ack modes complete

**Problem:** Publishing benchmarks means nothing if you can't prove data survives crashes.

**Crash Drill Scenarios:**
- [ ] Kill writer during segment rollover → verify monotonic offsets, no duplicates
- [ ] Power yank simulation → verify WAL recovery, no data loss for durable acks
- [ ] Network partition (split-brain) → verify epoch fencing, no duplicate writes
- [ ] S3 outage/throttling → verify backpressure, circuit breaker, recovery
- [ ] PostgreSQL failover → verify reconnection, < 10s downtime

**Deliverables:** Chaos test framework, 5 automated scripts, CI integration, published durability report

**Effort:** 40 hours (1 week)

#### Phase 10.7: Idempotent Producers ⏳ PLANNED

**Status:** PLANNED
**Priority:** HIGH (table stakes for production)
**Prerequisite:** Core producer API

**Problem:** Without idempotent producers, network retries cause duplicates.

**Features:**
- [ ] Producer ID assignment and persistence
- [ ] Per-partition sequence number tracking
- [ ] Server-side deduplication logic
- [ ] Epoch fencing for zombie producers
- [ ] Kafka protocol mapping (enable.idempotence=true)

**Effort:** 35 hours (1 week)

#### Phase 10.8: Hot-Partition Mitigation ⏳ PLANNED

**Status:** PLANNED
**Priority:** MEDIUM (scalability)
**Prerequisite:** Core partition assignment

**Problem:** One hot partition (100x traffic) overwhelms single agent.

**Features:**
- [ ] Dynamic shard splitting (auto-split hot partitions)
- [ ] Fast leader handoff (< 1s vs 30s lease expiry)
- [ ] Credit-based load shedding per partition

**Effort:** 45 hours (1.5 weeks)

#### Phase 10.9: Credit-Based Backpressure ⏳ PLANNED

**Status:** PLANNED
**Priority:** MEDIUM (prevents cascading failures)
**Prerequisite:** Agent forwarding

**Problem:** Head-of-line blocking when forwarding between agents.

**Features:**
- [ ] Credit system per destination agent
- [ ] Async forwarding queues (bounded)
- [ ] Forwarding loop prevention (hop count, visited set)

**Effort:** 30 hours (1 week)

#### Phase 10.10: Enhanced Operational Metrics ⏳ PLANNED

**Status:** PLANNED
**Priority:** HIGH (production debugging)
**Prerequisite:** Basic Prometheus metrics

**Problem:** Operators need to know WHY things fail, not just that they failed.

**New Metrics:**
- [ ] Leader change tracking (reason, old/new leader, gap duration)
- [ ] Routing staleness (cache age, misdirected requests, metadata TTL)
- [ ] Retry storm detection (retry rate by cause, backoff time)
- [ ] Per-partition operational metrics (lag, write/read rate, segments)
- [ ] Compaction stats (progress, bytes reclaimed, errors)

**Effort:** 25 hours (1 week)

---

### TIER 2: Important for Scalability

#### Phase 9: Advanced Consumer Features ⏳ PLANNED

**Status:** PLANNED
**Priority:** MEDIUM
**Prerequisite:** Phase 13 complete

**Features:**
- [ ] Dynamic consumer group rebalancing
- [ ] Consumer heartbeats and liveness detection
- [ ] Coordinator election for groups
- [ ] Generation IDs and fencing
- [ ] Partition assignment strategies
  - Range assignment
  - Round-robin assignment
  - Sticky assignment (minimize movement)
  - Custom assignors
- [ ] Incremental cooperative rebalancing

**Effort:** 120 hours (3 weeks)

#### Phase 11: Distributed Architecture ⏳ DEFERRED

**Status:** DEFERRED (see [PHASE_11_PLAN.md](PHASE_11_PLAN.md))
**Priority:** LOW (until single-server limits reached)
**Prerequisite:** >$500K ARR, >100K req/s traffic

**Features:**
- [ ] Separate API servers from storage agents
- [ ] gRPC-based communication (API → Agent)
- [ ] Horizontal scaling (multiple API servers)
- [ ] Load balancing (HAProxy, Nginx)
- [ ] High availability (multi-AZ deployment)
- [ ] Configuration mode toggle (unified vs distributed)

**Effort:** 160 hours (4 weeks)

**When to Implement:**
- Traffic exceeds single-server capacity (>50K req/s sustained)
- Need 99.99% uptime SLA
- Multi-region deployment required
- Customer demands horizontal scaling

---

### TIER 3: Nice-to-Have Features

#### Phase 14: Stream Processing (Future)

**Status:** NOT PLANNED
**Priority:** LOW
**Prerequisite:** Market validation that customers need this

**Features:**
- [ ] Stateless transformations (map, filter)
- [ ] Stateful aggregations (windowing, grouping)
- [ ] Join operations (stream-stream, stream-table)
- [ ] SQL-like query interface
- [ ] Integration with Flink/Spark

**Effort:** 400+ hours (3+ months)

**Note:** Kafka + Flink already exists. Only pursue if differentiation is clear.

#### Phase 15: Tiered Storage (Future)

**Status:** NOT PLANNED
**Priority:** MEDIUM
**Prerequisite:** Customers request faster reads

**Features:**
- [ ] Hot tier (NVMe SSD, 0-24 hours, 1-5ms reads)
- [ ] Warm tier (S3 Standard, 1-30 days, 10-50ms reads)
- [ ] Cold tier (S3 Glacier IR, 30+ days, 50-200ms reads)
- [ ] Automatic tiering based on age/access patterns
- [ ] Cost optimization (auto-move to cheaper storage)

**Effort:** 120 hours (3 weeks)

#### Phase 16: Multi-Region Replication (Future)

**Status:** NOT PLANNED
**Priority:** MEDIUM
**Prerequisite:** Enterprise customers with multi-region needs

**Features:**
- [ ] Cross-region replication (async)
- [ ] Active-active writes (conflict resolution)
- [ ] Geo-aware routing (write to nearest region)
- [ ] Disaster recovery (region failover)
- [ ] Compliance (data residency)

**Effort:** 200+ hours (5+ weeks)

---

### TIER 4: AI Capabilities (Strategic Differentiation)

These AI features position StreamHouse for the AI-native era. Ranked by value vs effort.

#### Phase 20: Natural Language → SQL Queries ✅ COMPLETE

**Status:** COMPLETE
**Priority:** HIGH (demos well, expands user base)
**Effort:** 2-3 days (20-25 hours)

**Problem:** SQL is powerful but intimidating for non-engineers. Let users query streams in plain English.

**Example:**
```
User: "Show me orders over $100 in the last hour by region"

→ LLM generates:
SELECT region, COUNT(*) as orders, SUM(amount) as total
FROM orders
WHERE amount > 100
  AND event_time > NOW() - INTERVAL '1 hour'
GROUP BY region
```

**Implementation:**
```rust
// New API endpoint
POST /api/v1/query/ask
{
  "question": "What were the top 5 products by revenue today?",
  "topics": ["orders"]
}
```

**Features:**
- [x] Natural language query endpoint (`/api/v1/query/ask`)
- [x] Schema-aware prompt generation (fetch topic schemas)
- [x] SQL validation before execution
- [x] Query explanation (show generated SQL to user)
- [x] Query history and refinement (`/api/v1/query/history`, `/refine`)
- [x] Cost estimation before execution (`/api/v1/query/estimate`)

**Why it matters:** "Query your streams in plain English" is a compelling headline. Expands market beyond engineers.

#### Phase 20.1: Schema Inference & Documentation ✅ COMPLETE

**Status:** COMPLETE
**Priority:** HIGH (improves onboarding)
**Effort:** 1-2 days (10-15 hours)

**Problem:** Users struggle to define schemas correctly. Auto-generate from sample data.

**Example:**
```
User uploads sample JSON →

AI infers:
- Field types (string, int, timestamp, etc.)
- Nullable fields
- Suggested indexes
- Human-readable documentation
- Compatibility recommendations
```

**Features:**
- [x] Sample data → JSON schema inference (`POST /api/v1/schema/infer`)
- [x] Nullable field detection (tracks null counts and occurrence rates)
- [x] Type detection (string, integer, number, boolean, array, object)
- [x] Auto-generated field descriptions (AI-powered)
- [x] Index recommendations based on field patterns (ID, status, timestamp fields)
- [x] Suggested SQL types for each field

**API Endpoint:** `POST /api/v1/schema/infer`
```json
{
  "topic": "orders",
  "sampleSize": 100,
  "generateDescriptions": true
}
```

**Completed:** February 4, 2026

#### Phase 21: ML Feature Pipelines ⏳ STRATEGIC

**Status:** PLANNED (positioning first)
**Priority:** HIGH (marketing differentiation)
**Effort:** Documentation + 40 hours (features)

**Problem:** Every ML team needs real-time features. Flink is painful. SQL is easier.

**Architecture:**
```
Raw Events → StreamHouse SQL → Feature Store → Model Serving
                   ↓
         "user_purchase_count_7d"
         "avg_session_duration_24h"
         "items_in_cart_now"
```

**Features:**
- [ ] Window aggregation functions (tumbling, sliding, session)
- [ ] Point-in-time correct joins
- [ ] Feature materialization to external stores
- [ ] Feature versioning and lineage
- [ ] Integration with Feast/Tecton/Hopsworks
- [ ] Pre-built feature templates

**Marketing Angle:** "Real-time ML features in SQL, not Java"

**Phase 21.1: Documentation & Positioning**
- [ ] Blog post: "Real-time features without Flink"
- [ ] Tutorial: Building a recommendation engine
- [ ] Comparison: StreamHouse SQL vs Flink Java
- [ ] Landing page for ML use case

**Phase 21.2: Implementation**
- [ ] TUMBLE/HOP/SESSION window functions
- [ ] OVER() window aggregations
- [ ] Feature store connectors (S3 Parquet, Feast)
- [ ] Backfill support for historical features

**Effort:** Docs (15h) + Implementation (40h)

#### Phase 22: Built-in Anomaly Detection ⏳ MEDIUM PRIORITY

**Status:** PLANNED
**Priority:** MEDIUM (built-in differentiation)
**Effort:** 1 week (40 hours)

**Problem:** Every observability use case needs anomaly detection. Built-in = differentiation.

**SQL Interface:**
```sql
-- Simple threshold anomaly
SELECT * FROM orders
WHERE amount > ANOMALY_THRESHOLD(amount, '1 hour')

-- Statistical anomaly (z-score)
SELECT * FROM metrics
WHERE cpu_usage > ZSCORE(cpu_usage, 3, '1 hour')  -- 3 standard deviations

-- Seasonal anomaly
SELECT * FROM traffic
WHERE requests > SEASONAL_BASELINE(requests, '1 day', '1 week')
```

**Features:**
- [ ] `ANOMALY_THRESHOLD(column, window)` - rolling percentile-based
- [ ] `ZSCORE(column, threshold, window)` - standard deviation based
- [ ] `SEASONAL_BASELINE(column, period, history)` - seasonal patterns
- [ ] `FORECAST(column, horizon)` - simple time series prediction
- [ ] Anomaly alerting integration
- [ ] Anomaly dashboard in UI

**Algorithms (start simple):**
- Rolling statistics (mean, stddev, percentiles)
- Z-score thresholds
- Exponential smoothing
- Optional: Prophet integration for seasonal

**Effort:** 40 hours

#### Phase 23: Vector/Embedding Streaming (RAG Pipelines) ⏳ STRATEGIC

**Status:** PLANNED
**Priority:** MEDIUM (AI-native positioning)
**Effort:** 2-3 weeks (80-120 hours)

**Problem:** RAG pipelines need real-time document ingestion. Position for AI-native companies.

**Schema Example:**
```sql
CREATE TOPIC documents (
  id STRING,
  content STRING,
  embedding VECTOR(1536)  -- OpenAI embedding dimension
);

-- Semantic search on streaming data
SELECT * FROM documents
WHERE cosine_similarity(embedding, ?) > 0.8
ORDER BY cosine_similarity(embedding, ?) DESC
LIMIT 10;
```

**Features:**
- [ ] VECTOR(N) data type in schema registry
- [ ] Vector serialization (binary, protobuf)
- [ ] `cosine_similarity(v1, v2)` SQL function
- [ ] `euclidean_distance(v1, v2)` SQL function
- [ ] `dot_product(v1, v2)` SQL function
- [ ] Approximate nearest neighbor (ANN) index
- [ ] Integration with embedding APIs (OpenAI, Cohere, etc.)
- [ ] Real-time RAG pipeline example

**Why it matters:** Every AI application needs document ingestion. Real-time vector streaming is underserved.

**Effort:** 80-120 hours

#### Phase 24: Intelligent Alerting ⏳ LOWER PRIORITY

**Status:** PLANNED
**Priority:** LOWER (nice-to-have)
**Effort:** 40-60 hours

**Problem:** Alert fatigue from duplicate/correlated alerts.

**Features:**
```
100 alerts fire → AI groups into 3 incidents
                → Suggests root cause
                → Auto-resolves duplicates
                → Ranks by severity
```

**Implementation:**
- [ ] Alert deduplication (fingerprinting)
- [ ] Alert correlation (time, labels, topology)
- [ ] AI-powered grouping (LLM clustering)
- [ ] Root cause suggestion
- [ ] Auto-silence for known patterns
- [ ] Incident timeline reconstruction

**Effort:** 40-60 hours

---

### AI Capabilities Summary

| Priority | Feature | Effort | Impact | Status |
|----------|---------|--------|--------|--------|
| 1 | Natural Language → SQL | 2-3 days | HIGH - demos well | ✅ COMPLETE |
| 2 | Schema Inference | 1-2 days | MEDIUM - onboarding | PLANNED |
| 3 | ML Feature Pipelines | Docs + 1 week | HIGH - positioning | PLANNED |
| 4 | Anomaly Detection | 1 week | MEDIUM - built-in value | PLANNED |
| 5 | Vector/Embedding Support | 2-3 weeks | STRATEGIC - AI-native | PLANNED |
| 6 | Intelligent Alerting | 1-2 weeks | LOWER - nice-to-have | PLANNED |

**Total AI Effort:** ~200-250 hours (~5-6 weeks)

### What NOT to Build (AI Anti-Patterns)

| Avoid | Why |
|-------|-----|
| "AI-powered auto-scaling" | Kubernetes already does this well |
| "AI query optimization" | Premature - need users first |
| "AI-generated dashboards" | Not core value prop |
| "Chatbot for docs" | Low value, everyone has this |
| "AI infrastructure management" | Commoditized, not differentiating |

### Quick Win: Add This Week

**Recommended first AI feature:**

```bash
curl -X POST http://localhost:8080/api/v1/query/ask \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What were the top 5 products by revenue today?",
    "topics": ["orders"]
  }'
```

**Implementation steps:**
1. Fetch topic schemas from Schema Registry
2. Send to Claude: "Generate SQL for this question given these schemas"
3. Validate generated SQL
4. Execute SQL via existing query engine
5. Return results + show generated SQL

**Demo value is huge.** "Query your streams in plain English" headlines well.

---

## Decision Gates & Milestones

### Week 2 Decision Gate: Customer Discovery

**Question:** Is the pain real and large enough?

**Success Criteria:**
- 15+/20 validate pain is severe
- 10+/20 willing to try archive tier
- 4+ design partner candidates identified

**Decision:**
- ✅ GO → Continue to Phase 12.2 (Technical Validation)
- ❌ NO-GO → **PIVOT** (try different market segment) or **SHELVE**

### Week 6 Decision Gate: Technical Validation

**Question:** Can we prove reliability and cost savings?

**Success Criteria:**
- All 5 chaos tests pass
- 99.9%+ uptime over 7 days
- 60%+ cost savings validated
- p99 latencies within targets

**Decision:**
- ✅ GO → Continue to Phase 12.3 (Design Partner POCs)
- ❌ NO-GO → **PIVOT** (different architecture) or **SHELVE**

### Week 13 Decision Gate: Design Partner POCs

**Question:** Will customers pay for this?

**Success Criteria:**
- 3/3 design partners successful
- 2+/3 would pay for product
- 99.9%+ average uptime
- 60%+ average cost savings
- 0 data loss incidents

**Decision:**
- ✅ STRONG GO (200+ points) → **Commit full-time, raise pre-seed, proceed to Phase 13**
- ⚠️ QUALIFIED GO (150-199) → **3-month improvement cycle, reassess**
- 🔄 PIVOT (100-149) → **Try different positioning (compliance, multi-cloud, etc.)**
- ❌ NO-GO (<100) → **Shelve or move to alternative path**

### Month 6 Milestone: First Paying Customer

**Target:** $5K-10K contract signed

**Success Criteria:**
- Archive tier deployed in production
- Customer paying (not just pilot)
- 99.9%+ uptime maintained
- Positive NPS (would recommend)

**If missed:**
- Extend timeline 3 months
- Reassess pricing/positioning
- Consider pivoting to different wedge

### Month 12 Milestone: Product-Market Fit

**Target:** $100K ARR (10-20 customers)

**Success Criteria:**
- 10+ paying customers
- <50% annual churn
- 60%+ cost savings proven
- Reference customers willing to speak
- Positive unit economics (LTV/CAC >3)

**If missed:**
- Reassess go-to-market strategy
- Consider alternative paths (bootstrap, acquihire)
- May need to accept "lifestyle business" outcome

### Month 18 Milestone: Series A Ready

**Target:** $500K ARR (50-60 customers)

**Success Criteria:**
- $500K+ ARR
- 20%+ MoM growth
- <40% annual churn
- 99.9%+ uptime track record
- 3+ customers >$50K/year
- Seed funding secured

**If achieved:**
- Raise Series A ($5-10M at $30-50M valuation)
- Hire aggressively (15-20 people)
- Expand to full platform (not just archive tier)

---

## Effort Summary

### Immediate (Weeks 1-12)
| Phase | Effort | Status |
|-------|--------|--------|
| Phase 12.1: Customer Discovery | 40h | ⏳ Starting |
| Phase 12.2: Technical Validation | 80h | ⏳ Planned |
| Phase 12.3: Design Partner POCs | 100h | ⏳ Planned |
| Phase 12.4: Operational Fixes | 150h | ⏳ Starting |
| **Total** | **370h** | **~9 weeks full-time** |

### v1 Product (Weeks 13-20)
| Phase | Effort | Status |
|-------|--------|--------|
| Phase 13.1: Kafka Connect Plugin | 80h | ⏳ Pending validation |
| Phase 13.2: Managed Deployment | 60h | ⏳ Pending validation |
| Phase 13.3: Customer Onboarding | 40h | ⏳ Pending validation |
| **Total** | **180h** | **~5 weeks full-time** |

### Critical Features (Post-v1)
| Phase | Effort | Priority |
|-------|--------|----------|
| Phase 7: Observability (remaining) | 40h | HIGH |
| Phase 10: Exactly-Once | 200h | HIGH |
| Phase 10.5: Producer Ack Modes | 25h | HIGH |
| Phase 10.6: Chaos Testing Suite | 40h | HIGH |
| Phase 10.7: Idempotent Producers | 35h | HIGH |
| Phase 10.8: Hot-Partition Mitigation | 45h | MEDIUM |
| Phase 10.9: Credit-Based Backpressure | 30h | MEDIUM |
| Phase 10.10: Enhanced Operational Metrics | 25h | HIGH |
| Phase 9: Advanced Consumer | 120h | MEDIUM |
| Phase 11: Distributed Architecture | 160h | LOW |
| **Total** | **720h** | **~18 weeks** |

### AI Capabilities (Strategic)
| Phase | Effort | Priority |
|-------|--------|----------|
| Phase 20: Natural Language → SQL | 25h | ✅ COMPLETE |
| Phase 20.1: Schema Inference | 15h | ✅ COMPLETE |
| Phase 21: ML Feature Pipelines | 55h | HIGH |
| Phase 22: Anomaly Detection | 40h | MEDIUM |
| Phase 23: Vector/Embedding Streaming | 100h | STRATEGIC |
| Phase 24: Intelligent Alerting | 50h | LOWER |
| **Total** | **285h** | **~7 weeks** |

**Grand Total (to Production-Ready Full Platform + AI):** ~1,555 hours (~39 weeks full-time)

---

## Resource Requirements

### Current (Solo Founder)
- **Time commitment:** 60-80 hours/week
- **Runway needed:** 6 months savings ($50-100K)
- **Infrastructure costs:** ~$500/month (AWS, monitoring tools)

### After Validation (Pre-seed Funded)
- **Team:** 2 engineers (founders)
- **Funding:** $500K pre-seed
- **Burn rate:** ~$40K/month
- **Runway:** 12 months to $100K ARR

### After v1 (Seed Funded)
- **Team:** 5-7 people (3-4 eng, 1-2 sales/GTM, 1 support)
- **Funding:** $2-3M seed
- **Burn rate:** ~$150K/month
- **Runway:** 18 months to Series A

---

## Risks & Mitigation

### Top 5 Risks

| Risk | Probability | Impact | Mitigation | Phase |
|------|-------------|--------|------------|-------|
| **Can't find design partners** | 40% | CRITICAL | Expand outreach, adjust positioning | 12.1 |
| **S3 throttling unsolvable** | 30% | CRITICAL | Multi-bucket, aggressive rate limiting | 12.4.2 |
| **Migration risk > savings** | 60% | HIGH | Start with archive tier (no migration) | 13.1 |
| **Kafka tiered storage wins** | 50% | HIGH | Focus on multi-cloud, simpler ops | Ongoing |
| **Can't prove 99.9% uptime** | 40% | HIGH | Extensive chaos testing, WAL | 12.2, 12.4 |

---

## Alternative Outcomes

If full VC-backed company doesn't pan out:

### 1. Lifestyle Business (Month 18+)
- Target: $200-500K/year profit
- Customers: 30-50 @ $10-20K each
- Team: Solo or 2-3 people
- Effort: Continue with Phases 13-15, skip expensive features

### 2. Acquihire (Month 6-12)
- Build impressive tech demo
- Network with Confluent, Redpanda, AWS, Databricks
- Target: $500K-2M acquisition for team
- Effort: Focus on technical excellence, less on GTM

### 3. Open Source + Consulting (Ongoing)
- 100% open source core
- Revenue: Consulting, support contracts
- Effort: Build community, less on managed service

### 4. Portfolio Piece (Ongoing)
- Keep as side project
- Use for blog posts, talks, interviews
- No revenue pressure
- Effort: Maintain OSS, no new features

---

## Next Actions (This Week)

### Immediate Tasks
- [ ] Review and approve this integrated roadmap
- [ ] Set up tracking spreadsheet (interviews, metrics, milestones)
- [ ] Create list of 50 potential interview targets
- [ ] Draft customer interview script
- [ ] Begin Phase 12.4.1 (WAL implementation)
- [ ] Schedule first 5 customer interviews

### Week 1 Goals
- [ ] Complete 10 customer interviews
- [ ] WAL implementation 50% complete
- [ ] Chaos test infrastructure set up
- [ ] Design partner candidate list (initial 10)

### Week 2 Checkpoint
- [ ] Customer discovery results analyzed
- [ ] Go/No-Go decision for Phase 12.2
- [ ] WAL implementation complete
- [ ] Design partner outreach begun

---

## Success Metrics Dashboard

Track these weekly:

| Metric | Current | Week 4 Target | Week 8 Target | Week 12 Target |
|--------|---------|---------------|---------------|----------------|
| Customer interviews | 0 | 20 | 20 | 20 |
| Design partners identified | 0 | 4 | 4 | 4 |
| Design partners deployed | 0 | 0 | 1 | 3 |
| Uptime % (POC) | N/A | N/A | 99.5% | 99.9% |
| Cost savings validated | N/A | Yes | Yes | Yes |
| Chaos tests passing | 0/5 | 3/5 | 5/5 | 5/5 |
| Runbooks written | 0 | 2 | 5 | 7 |
| Would pay (partners) | 0 | N/A | N/A | 2+/3 |

---

## Conclusion

**Bottom Line:**
- ✅ Technical foundation is solid (Phases 1-8.5 complete)
- ⚠️ Business validation is critical next step (Phase 12)
- ⚠️ Operational maturity gaps must be fixed (Phase 12.4)
- 🎯 Goal: Validate archive tier wedge in 12 weeks
- 🚀 If successful: Ship v1, raise pre-seed, grow to $500K ARR

**This roadmap preserves all original technical work while prioritizing business validation to avoid building features nobody wants.**

**Next Review:** End of Week 4 (after customer discovery + initial validation)
