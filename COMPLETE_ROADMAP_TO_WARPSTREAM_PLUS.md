# StreamHouse: Complete Roadmap to "WarpStream but Better"

**Created**: 2026-01-28
**Goal**: Reach WarpStream feature parity + SQL stream processing to replace Flink
**Timeline**: 9 months (36 weeks) to full production managed service

---

## Executive Summary

**Current State** (Phase 21 Complete):
- ✅ Core distributed streaming platform (Phases 1-7)
- ✅ Multi-agent coordination with partition leases
- ✅ S3-native storage with LZ4 compression
- ✅ gRPC API + CLI tool (streamctl)
- ✅ Agent binary with gRPC + metrics servers
- ✅ Observability (Prometheus metrics, health checks)
- ✅ **Kafka Protocol Compatibility** (Phase 21) - port 9092

**Target State** (Month 9):
- 🎯 **WarpStream parity**: All core streaming features
- 🎯 **SQL stream processing**: Built-in Flink replacement
- 🎯 **Production managed service**: Multi-tenant SaaS with billing
- 🎯 **CLI as primary interface**: Production-ready streamctl

---

## Gap Analysis: What's Missing vs WarpStream

### ✅ Already Have (Phases 1-7)
- Core streaming (produce/consume, topics, partitions)
- Distributed agents with partition leases
- S3-native storage with compression
- Multi-agent coordination
- Consumer groups with offset management
- gRPC API
- CLI tool (streamctl)
- Observability (metrics, health checks)
- Agent binary (gRPC + metrics servers)

### ❌ Missing for WarpStream Parity

#### 1. **Kafka Protocol Compatibility** ✅ COMPLETE
**Status**: ✅ Implemented (Phase 21)
**Impact**: HIGH - Many users need Kafka wire protocol

Implemented in `streamhouse-kafka` crate:
- ✅ Kafka wire protocol decoder/encoder
- ✅ Protocol version negotiation (ApiVersions API)
- ✅ 14 Kafka API endpoints (Produce, Fetch, Metadata, etc.)
- ✅ Compatible with kafka-python, kafkajs (tested)
- ⚠️ Fetch API returns empty (needs PartitionReader integration)
- 📖 See: `docs/KAFKA_PROTOCOL.md`

#### 2. **REST API** (User Experience Gap)
**Status**: ❌ Not implemented
**Impact**: MEDIUM - Needed for web console and HTTP clients
**Work**: ~2 weeks

Need:
- REST API for topic management
- REST API for producing/consuming
- OpenAPI/Swagger documentation
- HTTP authentication/authorization

#### 3. **Web Console** (User Interface Gap)
**Status**: ❌ Not implemented
**Impact**: HIGH - Essential for managed service
**Work**: ~6 weeks

Need:
- Topic management UI
- Real-time metrics dashboard
- Consumer group monitoring
- User/organization management
- Billing dashboard

#### 4. **Multi-Tenancy** (Production Gap)
**Status**: ⚠️ Partially planned (MULTI_TENANT_DATABASE_STRATEGY.md exists)
**Impact**: CRITICAL - Blocker for managed service
**Work**: ~3 weeks

Need:
- `organization_id` in all tables
- Row-level security (RLS)
- S3 prefix isolation per org
- Quota enforcement
- Tenant isolation guarantees

#### 5. **Exactly-Once Semantics** (Feature Gap)
**Status**: ❌ Not implemented
**Impact**: MEDIUM-HIGH - Required for financial use cases
**Work**: ~4 weeks

Need:
- Idempotent producer with sequence numbers
- Transactional writes (begin/commit/abort)
- Transaction coordinator
- Consumer read-committed isolation

#### 6. **Schema Registry** (Feature Gap)
**Status**: ❌ Not implemented
**Impact**: MEDIUM - Important for production workloads
**Work**: ~3 weeks

Need:
- Schema storage (Avro, Protobuf, JSON Schema)
- Schema versioning and compatibility checks
- Schema validation on produce/consume
- REST API for schema management

#### 7. **Auto-scaling** (Operations Gap)
**Status**: ❌ Not implemented
**Impact**: MEDIUM - Important for cost optimization
**Work**: ~2 weeks

Need:
- Kubernetes HPA integration
- Custom metrics for scaling (consumer lag, partition count)
- Agent graceful shutdown on scale-down
- Partition rebalancing on scale events

#### 8. **CLI Enhancements** (User Experience Gap)
**Status**: ⚠️ Basic CLI exists, needs production polish
**Impact**: MEDIUM - CLI is primary interface for many users
**Work**: ~2 weeks

Current CLI (`streamctl`) has basic commands but needs:
- **Interactive mode**: `streamctl shell` for REPL experience
- **Configuration profiles**: Multiple server configs (dev, staging, prod)
- **Auto-completion**: Bash/Zsh completion scripts
- **Output formats**: JSON, YAML, table, CSV
- **Batch operations**: Produce from file, consume to file
- **Consumer group management**: List groups, describe group, reset offsets
- **Monitoring commands**: `streamctl top` (like Kafka's kafka-consumer-groups)
- **Schema integration**: Register schemas, validate messages
- **Better error messages**: User-friendly errors with suggestions
- **Progress indicators**: Spinners for long operations
- **Piping support**: Unix-friendly stdin/stdout
- **Cluster management**: Add/remove agents, view cluster health
- **Credentials management**: Store/rotate API keys

---

## 🎯 THE BIG DIFFERENTIATOR: SQL Stream Processing

### Why This Matters

**Current Streaming Stack**:
```
Kafka/Kinesis → Flink → Output
```

**StreamHouse Vision**:
```
StreamHouse (with built-in SQL) → Output
```

**Value Proposition**:
- ❌ **No Flink cluster to manage** (saves $$$)
- ❌ **No JVM memory tuning** (Rust performance)
- ❌ **No separate deployment** (one system)
- ✅ **Embedded processing** (lower latency)
- ✅ **Unified SQL dialect** (no translation)
- ✅ **Cost-effective** (S3 storage + serverless compute)

### Features Needed (Phase 12: SQL Processing)

#### 1. **Streaming SQL Engine** (~6 weeks)
Built on Apache DataFusion (Rust query engine):

```sql
-- Create a streaming view
CREATE STREAM revenue_by_product AS
SELECT
    product_id,
    SUM(amount) as total_revenue,
    COUNT(*) as order_count,
    TUMBLE_START(order_time, INTERVAL '1' HOUR) as window_start
FROM orders
GROUP BY product_id, TUMBLE(order_time, INTERVAL '1' HOUR);

-- Continuous query that outputs to another topic
INSERT INTO high_value_orders
SELECT * FROM orders
WHERE amount > 1000;

-- Streaming join (event-time)
SELECT
    o.order_id,
    o.amount,
    u.name as customer_name
FROM orders o
JOIN users u
    ON o.user_id = u.user_id
    AND u.event_time BETWEEN o.event_time - INTERVAL '1' HOUR
                         AND o.event_time + INTERVAL '5' MINUTES;
```

**Components**:
- DataFusion integration (SQL parser, planner, optimizer)
- Custom streaming table provider (reads from StreamHouse topics)
- Window functions (TUMBLE, HOP, SESSION)
- Watermark tracking for late data
- State management (RocksDB for aggregations)
- Checkpointing for fault tolerance
- Output sink to topics

#### 2. **Streaming Windows** (~2 weeks)
```sql
-- Tumbling window (fixed, non-overlapping)
SELECT COUNT(*) FROM events
GROUP BY TUMBLE(event_time, INTERVAL '5' MINUTE);

-- Hopping window (overlapping)
SELECT COUNT(*) FROM events
GROUP BY HOP(event_time, INTERVAL '5' MINUTE, INTERVAL '1' MINUTE);

-- Session window (dynamic, gap-based)
SELECT user_id, COUNT(*) FROM events
GROUP BY user_id, SESSION(event_time, INTERVAL '30' MINUTE);
```

#### 3. **Stateful Aggregations** (~3 weeks)
- COUNT, SUM, AVG, MIN, MAX over windows
- Custom UDAFs (User-Defined Aggregate Functions)
- State stored in RocksDB
- State TTL for garbage collection
- State backup/recovery

#### 4. **Stream Joins** (~3 weeks)
- Inner, left, right, full outer joins
- Event-time joins with time bounds
- State-backed join (store both streams in RocksDB)
- Join optimization (smaller stream as build side)

#### 5. **Late Data Handling** (~1 week)
- Watermark propagation
- Allowed lateness configuration
- Side output for late data

#### 6. **SQL Job Management** (~2 weeks)
- Submit SQL job via CLI/API
- Job lifecycle (start, stop, pause, resume)
- Job status monitoring
- Automatic restarts on failure
- Savepoint/checkpoint management

---

## Revised Phase Plan (Phases 8-15)

### Phase 8: Production Infrastructure (Weeks 1-4) ✅ **UNCHANGED**
**Goal**: Deploy on AWS with Kubernetes, RDS, S3

- Week 1-2: Kubernetes Helm charts + StatefulSets
- Week 3: RDS PostgreSQL setup
- Week 4: S3 setup + Prometheus monitoring

**Deliverables**:
- Kubernetes deployment
- PostgreSQL RDS (multi-AZ)
- S3 bucket with lifecycle policies
- Prometheus + Grafana dashboards

---

### Phase 9: Multi-Tenancy Foundation (Weeks 5-7) ✅ **UNCHANGED**
**Goal**: Database-level tenant isolation

- Week 5: Add `organization_id` to schema
- Week 6: Update MetadataStore trait with org_id
- Week 7: S3 prefix isolation + quotas

**Deliverables**:
- Multi-tenant data model
- S3 isolation: `org-{uuid}/data/{topic}/{partition}/`
- Quota enforcement

---

### Phase 10: REST API + Web Console (Weeks 8-13) 🆕 **NEW/ENHANCED**
**Goal**: User-facing interfaces for managed service

#### Week 8-9: REST API
- **Topic Management**: Create, list, delete topics
- **Record API**: Produce/consume via HTTP
- **Consumer Groups**: List, describe, reset offsets
- **Metrics API**: Fetch topic metrics, consumer lag
- **Authentication**: API key-based auth
- **Rate limiting**: Per-org quotas
- **OpenAPI spec**: Auto-generated docs

**Tech Stack**: Axum (async Rust web framework)

#### Week 10-13: Web Console
- **Frontend**: React + TypeScript + Tailwind CSS
- **Features**:
  - Topic management UI
  - Real-time metrics dashboard (Chart.js)
  - Consumer group monitoring
  - Organization settings
  - API key management
  - Billing dashboard (usage + costs)
- **Authentication**: Auth0 or Clerk
- **Hosting**: Vercel or CloudFront + S3

**Deliverables**:
- REST API with OpenAPI docs
- Web console (responsive UI)
- User authentication

---

### Phase 11: Kafka Protocol Compatibility (Weeks 14-17) ✅ **COMPLETE**
**Goal**: Support Kafka clients without modification

#### ✅ Week 14-15: Protocol Implementation
- ✅ Kafka wire protocol decoder/encoder (`streamhouse-kafka` crate)
- ✅ Protocol version negotiation (ApiVersions API)
- ✅ Request/response handling with compact protocol support

#### ✅ Week 16: Core APIs
- ✅ **Produce API (0)**: Write records (RecordBatch v2 format)
- ⚠️ **Fetch API (1)**: Stubbed (needs PartitionReader integration)
- ✅ **Metadata API (3)**: Topic/partition/broker info
- ✅ **ListOffsets API (2)**: Find start/end offsets
- ✅ **OffsetCommit/Fetch (8/9)**: Save and retrieve offsets
- ✅ **Group Coordinator (10-14)**: JoinGroup, SyncGroup, Heartbeat, LeaveGroup
- ✅ **CreateTopics/DeleteTopics (19/20)**: Topic management

#### Week 17: Testing + Compatibility
- ✅ Tested with `kafka-python` (produce working)
- ⚠️ `kcat`/librdkafka has compact protocol issues
- Remaining: Test with Java, Go, Node.js clients

**Deliverables**:
- ✅ Kafka protocol support (14 APIs)
- ✅ Server integrated into unified-server.rs (port 9092)
- 📖 See: `docs/KAFKA_PROTOCOL.md` for usage

#### ⚠️ Known Limitations (Follow-up Work)

| Limitation | Impact | Fix Required |
|------------|--------|--------------|
| **Consume not working** | HIGH | Fetch API returns empty batches - needs PartitionReader/SegmentCache integration to read stored records |
| **RecordBatch v2 only** | MEDIUM | Only modern clients (Kafka 0.11+) supported - older Message Set format not implemented |
| **kcat/librdkafka issues** | LOW | Compact protocol encoding causes assertion failures in librdkafka - use kafka-python or kafkajs instead |
| **No transactions** | LOW | Idempotent producer and transactions not yet implemented (Phase 14) |

**Priority Fix**: Implement Fetch API to read from storage (~1-2 days work)

---

### Phase 12: CLI Production Enhancements (Weeks 18-19) 🆕 **NEW**
**Goal**: Make `streamctl` production-ready

#### Week 18: Core Enhancements
- **Configuration profiles**: `~/.streamhouse/config.toml`
  ```toml
  [profiles.dev]
  endpoint = "http://localhost:9090"

  [profiles.prod]
  endpoint = "https://api.streamhouse.io"
  api_key = "sk_live_..."
  ```
- **Output formats**: `--output json|yaml|table|csv`
- **Interactive shell**: `streamctl shell`
- **Auto-completion**: Bash/Zsh scripts
- **Batch operations**:
  - `streamctl produce orders < data.jsonl`
  - `streamctl consume orders > output.jsonl`

#### Week 19: Advanced Features
- **Consumer group commands**:
  - `streamctl groups list`
  - `streamctl groups describe analytics`
  - `streamctl groups reset --group analytics --topic orders --to-earliest`
- **Monitoring commands**:
  - `streamctl top` (live consumer lag)
  - `streamctl cluster health`
  - `streamctl agents list`
- **Schema commands** (if schema registry enabled):
  - `streamctl schema register orders schema.avsc`
  - `streamctl schema list`

**Deliverables**:
- Production-ready CLI
- Comprehensive man pages
- Auto-completion scripts

---

### Phase 13: Schema Registry (Weeks 20-22) 🆕 **REORDERED**
**Goal**: Message validation and schema evolution

#### Week 20: Schema Storage
- PostgreSQL schema for schema metadata
- REST API for schema management
- Schema validation library (Avro, Protobuf, JSON Schema)

#### Week 21: Producer/Consumer Integration
- Producer schema validation on send
- Consumer schema deserialization
- Schema ID embedding in messages (4-byte header)

#### Week 22: Schema Evolution
- Compatibility checking (backward, forward, full)
- Schema versioning
- Breaking change detection

**Deliverables**:
- Schema registry service
- Client SDK integration
- CLI commands for schema management

---

### Phase 14: Exactly-Once Semantics (Weeks 23-26) 🆕 **REORDERED**
**Goal**: Transactional guarantees for critical workloads

#### Week 23-24: Idempotent Producer
- Producer ID + sequence numbers
- Server-side deduplication
- Transparent retries

#### Week 25: Transactional Writes
- Transaction coordinator service
- Begin/commit/abort API
- Atomic multi-partition writes

#### Week 26: Consumer Integration
- Read-committed isolation level
- Skip aborted transactions
- Testing + documentation

**Deliverables**:
- Exactly-once semantics
- Transaction API
- Financial use case examples

---

### Phase 15: SQL Stream Processing (Weeks 27-36) 🎯 **FLAGSHIP FEATURE**
**Goal**: Built-in stream processing to replace Flink

#### Week 27-28: DataFusion Integration
- Custom streaming table provider
- Stream scan execution plan
- Integration with Consumer API

#### Week 29-30: Window Functions
- TUMBLE (tumbling windows)
- HOP (hopping windows)
- SESSION (session windows)
- Watermark tracking

#### Week 31-32: Stateful Aggregations
- COUNT, SUM, AVG, MIN, MAX
- RocksDB state backend
- State TTL and compaction

#### Week 33-34: Stream Joins
- Event-time joins with bounds
- State-backed joins (store both sides)
- Join optimization

#### Week 35: Late Data Handling
- Watermark propagation
- Allowed lateness
- Side outputs

#### Week 36: Job Management
- SQL job submission via CLI/API
- Job monitoring and restarts
- Checkpointing and recovery

**Example Queries**:
```sql
-- Real-time revenue tracking
CREATE STREAM hourly_revenue AS
SELECT
    TUMBLE_START(order_time, INTERVAL '1' HOUR) as hour,
    SUM(amount) as revenue,
    COUNT(*) as orders
FROM orders
GROUP BY TUMBLE(order_time, INTERVAL '1' HOUR);

-- Fraud detection (pattern matching)
CREATE STREAM suspicious_activity AS
SELECT
    user_id,
    COUNT(*) as failed_attempts,
    window_start
FROM login_attempts
WHERE success = false
GROUP BY user_id, SESSION(event_time, INTERVAL '5' MINUTE)
HAVING COUNT(*) >= 5;

-- Stream enrichment (join)
CREATE STREAM enriched_orders AS
SELECT
    o.order_id,
    o.amount,
    u.name,
    u.email,
    u.tier
FROM orders o
JOIN users u
    ON o.user_id = u.user_id
    AND u.event_time BETWEEN o.event_time - INTERVAL '1' HOUR
                         AND o.event_time;
```

**Deliverables**:
- SQL streaming engine
- Window functions
- Joins and aggregations
- State management
- CLI: `streamctl sql submit query.sql`

---

## Optional Post-Launch Phases (Weeks 37+)

### Phase 16: Auto-Scaling (Weeks 37-38)
- Kubernetes HPA with custom metrics
- Agent graceful shutdown
- Partition rebalancing

### Phase 17: Multi-Region Replication (Weeks 39-42)
- Cross-region topic replication
- Disaster recovery
- Active-active setup

### Phase 18: Tiered Storage (Weeks 43-44)
- Hot/cold/archive tiers
- Automatic data lifecycle
- Cost optimization

### Phase 19: Native SDKs (Weeks 45-48)
**Goal**: Clean, modern SDKs for developers who don't want Kafka baggage

#### Why Native SDKs (even with Kafka protocol)?
- **Simpler API**: No consumer groups/partitions unless needed
- **Schema-native**: Built-in Avro/JSON validation
- **Faster**: Direct gRPC, no protocol translation
- **Cleaner deps**: No kafka-python, librdkafka, etc.
- **StreamHouse features**: SQL queries, schema evolution

#### Languages (prioritized by demand)
1. **Python** (`streamhouse-py`) - Week 45
   ```python
   from streamhouse import Client
   client = Client("localhost:50051")
   await client.produce("orders", {"order_id": 123})
   async for record in client.consume("orders"):
       print(record.value)
   ```

2. **TypeScript/JavaScript** (`streamhouse-js`) - Week 46
   ```typescript
   import { StreamHouse } from 'streamhouse';
   const client = new StreamHouse('localhost:50051');
   await client.produce('orders', { orderId: 123 });
   ```

3. **Go** (`streamhouse-go`) - Week 47
   ```go
   client := streamhouse.NewClient("localhost:50051")
   client.Produce("orders", map[string]any{"order_id": 123})
   ```

4. **Java** (`streamhouse-java`) - Week 48
   ```java
   StreamHouseClient client = StreamHouse.connect("localhost:50051");
   client.produce("orders", Map.of("orderId", 123));
   ```

**Deliverables**:
- 4 native SDKs (Python, JS, Go, Java)
- Auto-generated from gRPC/Protobuf definitions
- Schema validation built-in
- Published to PyPI, npm, pkg.go.dev, Maven

### Phase 20: Advanced Ecosystem (Weeks 49-52)
- Kafka Connect compatibility
- Iceberg table integration (Tableflow)
- Embedded pipelines (Bento-style)

---

## Feature Comparison: Final State

| Feature | WarpStream | Kafka + Flink | StreamHouse |
|---------|-----------|---------------|-------------|
| **Protocol** | Kafka | Kafka | Kafka + gRPC + REST |
| **Storage** | S3-native | Local disks | S3-native ✅ |
| **Metadata** | PostgreSQL/Spanner | ZooKeeper | PostgreSQL ✅ |
| **Architecture** | Stateless agents | Stateful brokers | Stateless agents ✅ |
| **SQL Processing** | ❌ None | ✅ Flink (separate) | ✅ Built-in 🎯 |
| **Web Console** | ✅ Yes | ❌ No | ✅ Yes |
| **Multi-tenancy** | ✅ Yes | ❌ No | ✅ Yes |
| **Schema Registry** | ✅ Yes | ✅ Separate | ✅ Built-in |
| **Exactly-once** | ✅ Yes | ✅ Yes | ✅ Yes |
| **CLI** | ✅ warpctl | ✅ kafka-* | ✅ streamctl |
| **Managed Service** | ✅ Yes | ❌ Complex | ✅ Yes |
| **Cost** | $$$ | $$$$ | $$ 🎯 |

**StreamHouse Advantages**:
1. 🎯 **Built-in SQL processing** (no Flink needed)
2. 🎯 **Lower operational cost** (serverless S3 + lightweight agents)
3. 🎯 **Simpler architecture** (one system, not two)
4. 🎯 **Rust performance** (no JVM, lower memory)
5. 🎯 **Modern API** (gRPC + REST + Kafka)

---

## Timeline Summary

| Phase | Weeks | Focus | Key Deliverable |
|-------|-------|-------|----------------|
| Phase 8 | 1-4 | Infrastructure | Kubernetes + RDS + S3 |
| Phase 9 | 5-7 | Multi-tenancy | Tenant isolation |
| Phase 10 | 8-13 | UI/UX | REST API + Web Console |
| Phase 11 | 14-17 | Compatibility | Kafka protocol |
| Phase 12 | 18-19 | CLI | Production streamctl |
| Phase 13 | 20-22 | Schemas | Schema registry |
| Phase 14 | 23-26 | Transactions | Exactly-once |
| Phase 15 | 27-36 | **SQL** 🎯 | **Stream processing** |

**Total**: 36 weeks (9 months) to full production with SQL

---

## Success Metrics

### Month 3 (After Phase 10)
- ✅ 100 beta users signed up
- ✅ Web console live
- ✅ REST API documented
- ✅ First paying customer

### Month 6 (After Phase 14)
- ✅ Kafka compatibility proven (3+ client libraries)
- ✅ Exactly-once semantics in production
- ✅ 500+ topics created
- ✅ $10K MRR

### Month 9 (After Phase 15)
- ✅ **SQL stream processing GA** 🎯
- ✅ First customer migrates from Flink
- ✅ "WarpStream but better" positioning validated
- ✅ $50K MRR

---

## Go-to-Market Positioning

**"The only S3-native streaming platform with built-in SQL processing"**

**Target Customers**:
1. **Teams running Kafka + Flink** → "Replace both with one system"
2. **Startups on Kinesis** → "Get SQL processing without learning Flink"
3. **WarpStream users** → "Same benefits + stream processing"
4. **Cost-conscious enterprises** → "Save 60% vs managed Kafka + Flink"

**Key Messages**:
- ✅ "No Flink cluster to manage"
- ✅ "Write SQL, get results - no Java required"
- ✅ "S3-native storage + serverless compute = cost-effective"
- ✅ "One system, not two"

---

## Implementation Priorities

### Must-Have (for Managed Service Launch)
1. ✅ Phase 8: Kubernetes deployment
2. ✅ Phase 9: Multi-tenancy
3. ✅ Phase 10: REST API + Web Console
4. ✅ Phase 11: Kafka protocol
5. ✅ Phase 12: Production CLI

### Should-Have (for Enterprise)
6. ✅ Phase 13: Schema registry
7. ✅ Phase 14: Exactly-once

### Differentiator (for Market Leadership)
8. 🎯 **Phase 15: SQL stream processing**

---

## Current CLI Status & Roadmap

### ✅ What CLI Has Now (Phase 1)
- Basic topic management (create, list, delete)
- Record produce/consume
- Consumer offset management
- gRPC client integration
- Environment variable config

### 🎯 What CLI Needs (Phase 12)
- Configuration profiles (multi-environment)
- Interactive shell mode
- Auto-completion
- Better output formats (JSON, YAML, table)
- Batch operations (produce from file)
- Consumer group management commands
- Monitoring commands (top, cluster health)
- Schema registry integration
- SQL job submission (Phase 15)

**CLI Evolution**:
```
Phase 1:  Basic commands ✅
Phase 12: Production-ready (profiles, shell, monitoring) 🎯
Phase 15: SQL integration (submit/monitor SQL jobs) 🎯
```

---

## Next Steps

### Immediate (This Week)
1. Review this roadmap - confirm prioritization
2. Decide: Start Phase 8 (Kubernetes) or Phase 10 (REST API + Web Console)?
3. Set up project tracking (GitHub Projects or Linear)

### Recommendation
**Start with Phase 10 (REST API + Web Console)** before Phase 8:

**Why?**
- REST API is needed for web console
- Web console needed for user testing
- Can deploy to Kubernetes later once UI is validated
- Faster feedback loop with beta users

**Alternative Path**:
```
Phase 10 (REST API + Web Console, 6 weeks)
  ↓
Phase 9 (Multi-tenancy, 3 weeks)
  ↓
Phase 8 (Kubernetes deployment, 4 weeks)
  ↓
Phase 11+ (Continue with Kafka, Schema, Transactions, SQL)
```

This gets you to a **testable managed service in 9 weeks** (2 months) instead of waiting 13 weeks.

---

**Status**: Ready to execute 🚀
**Next Decision**: Choose Phase 10 vs Phase 8 first?
