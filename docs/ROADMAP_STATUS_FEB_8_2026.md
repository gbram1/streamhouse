# StreamHouse Roadmap Status

**Date**: February 10, 2026
**Version**: v1.0 Production Ready
**Crates**: 13 workspace members
**Lines of Code**: ~50K+ Rust (core) + SDKs + Web UI

---

## Executive Summary

StreamHouse is ~83% feature-complete against the full roadmap. The core streaming platform, security, observability, streaming SQL, client SDKs, web UI, production hardening, RBAC, cross-region replication, backup tools, framework integrations, and AI-powered NL-to-SQL + schema inference are all implemented and verified in source code. The major remaining gaps are **horizontal scaling** (distributed multi-node architecture), **advanced AI/ML capabilities** (ML feature pipelines, ML-based anomaly detection, vector streaming, intelligent alerting), and **connectors**.

| Category | Status |
|----------|--------|
| Core streaming platform | **COMPLETE** |
| Security & auth | **COMPLETE** |
| Observability & monitoring | **COMPLETE** |
| Schema registry | **COMPLETE** |
| Streaming SQL (windows, anomaly, vectors, JOINs, MVs) | **COMPLETE** |
| Exactly-once semantics | **COMPLETE** |
| Production hardening (WAL, S3 resilience, HA, DR) | **COMPLETE** |
| Client SDKs (Python, TypeScript, Java, Go) | **COMPLETE** |
| Framework integrations (Spring Boot, FastAPI, Express, Go) | **COMPLETE** |
| Web console + multi-tenancy UI | **COMPLETE** |
| Web console polish & fixes | **COMPLETE** |
| Enhanced CLI + REPL | **COMPLETE** |
| Kafka protocol compatibility | **COMPLETE** |
| RBAC & governance | **COMPLETE** |
| Cross-region replication | **COMPLETE** |
| Backup & migration tools | **COMPLETE** |
| Consumer simulator UI | **COMPLETE** |
| Distributed architecture (horizontal scale) | **NOT STARTED** |
| AI: NL-to-SQL + Schema Inference | **COMPLETE** |
| AI: ML pipelines, anomaly, vectors, alerting | **NOT STARTED** |
| Connectors (Kafka Connect, Debezium CDC) | **NOT STARTED** |

---

## COMPLETED PHASES (Detailed)

### Phase 1-3: Core Infrastructure
- Producer API with batching & LZ4 compression
- Agent coordination via gRPC (stateless multi-agent)
- S3/MinIO storage layer with segment-based architecture
- SQLite + PostgreSQL metadata stores
- Consumer API with offset management and consumer groups
- Partition leadership & leasing with epoch fencing
- Agent registration & heartbeat monitoring (20s interval)

### Phase 4-6: Multi-Agent & Coordination
- Multi-agent architecture (stateless, PostgreSQL-coordinated)
- Partition lease management with fencing tokens
- Consumer position tracking
- Offset reset (earliest/latest)
- Key-based partitioning (SipHash)
- Agent discovery with background refresh

### Phase 7: Observability
- 22 Prometheus metrics (producer, consumer, agent, storage)
- HTTP metrics server (`/metrics`, `/health`, `/ready`, `/live`)
- Structured logging (text + JSON modes)
- 5 Grafana dashboards
- 17 pre-configured alert rules
- Consumer lag monitoring API
- `#[instrument]` tracing macros throughout
- < 1% performance overhead

### Phase 8: Performance & Benchmarks
- **8.1 Benchmarking**: Criterion-based suite, 3.7M rec/sec batch creation, 8.8M rec/sec compression
- **8.2 Producer optimizations**: Connection pooling (gRPC pool, 100+ concurrent streams), batch size tuning, zero-copy (`Bytes` throughout), async batching
- **8.3 Consumer optimizations**: 2-segment parallel prefetch at 80% threshold, parallel partition reads (`join_all`, 10x faster), LRU segment cache (>80% hit rate)
- **8.4 Storage optimizations**: S3 multipart uploads (8MB threshold, parallel parts), Bloom filters (1% FP), WAL batching (1000 rec/1MB/10ms auto-flush)
- **8.5 Load testing**: Producer/consumer/segment benchmarks (2.26M write, 3.10M read rec/sec — 45x targets), 60K message stress test

### Phase 9: Schema Registry
- **9.1**: PostgreSQL storage backend (13 methods), schema versioning, SHA-256 dedup
- **9.2**: Producer integration with local caching, Confluent wire format compatibility
- **9.3**: Avro compatibility checking (backward/forward/full/transitive)
- 28/28 tests passing

### Phase 9.3-9.4: Advanced Consumer
- Group coordinator (Kafka protocol compatible)
- Rebalancing protocol (JoinGroup, SyncGroup, Heartbeat, LeaveGroup)
- Generation IDs & state machine (Empty -> PreparingRebalance -> CompletingRebalance -> Stable)
- Compacted topics (`cleanup_policy: compact`) with background compaction job
- Wildcard subscriptions (`list_topics_matching("events.*")`)
- Timestamp-based seeking (`seek_to_timestamp` endpoint)
- Manual partition assignment

### Phase 10: Production Hardening

**10.1 Security** (all verified in source):
- TLS/mTLS (`crates/streamhouse-api/src/tls.rs`) — rustls-based, env-configurable
- API key authentication (`src/auth.rs`) — SHA-256 hashed, permission-based routing
- JWT tokens (`src/jwt.rs`) — HS256/RS256/ES256, claims-based middleware
- ACL system (`src/acl.rs`) — resource/action-based, wildcards, deny precedence
- OAuth2/OIDC (`src/oauth.rs`) — Google/GitHub/Okta/custom, PKCE, ID token validation
- SASL/SCRAM (`src/sasl.rs`) — SHA-256/SHA-512, RFC 5802 compliant

**10.2 High Availability**:
- Leader election (`src/leader.rs`) — PostgreSQL + memory backends, fencing tokens, lease management
- Automatic failover (`src/failover.rs`) — health monitoring, configurable policies, event notifications
- Circuit breakers (from Phase 8)
- Graceful shutdown (SIGINT/SIGTERM, timeout config, TLS support)

**10.3 Disaster Recovery**:
- Metadata backup/restore (JSON export/import for topics, offsets, organizations)
- Point-in-time recovery (WAL archiving, base backups, timestamp/LSN recovery, chain verification)
- DR documentation (comprehensive guide with procedures, monitoring, troubleshooting)

**10.4 Audit Logging**:
- AuditLayer middleware with operation tracking
- Client IP/User-Agent capture
- Immutable audit trail (SHA-256 hash chain, file/memory backends, query interface)
- Compliance reports (SOC2/GDPR/HIPAA/PCI-DSS, JSON/CSV/HTML export)

### Phase 12.4: Operational Reliability

**12.4.1 Write-Ahead Log**:
- Channel-based group commit architecture (mpsc channel + dedicated writer task)
- Lock-free append in batched mode (~50ns)
- `fdatasync` for crash safety
- CRC32 checksums for data integrity
- Three sync policies: Always/Interval/Never
- Automatic recovery on startup
- Performance: WAL standalone 2.21M rec/s, full path 769K rec/s

**12.4.2 S3 Throttling Protection**:
- Token bucket rate limiter (~320 LOC) — per-operation (PUT/GET/DELETE), adaptive rate adjustment, lock-free
- Circuit breaker — Closed/Open/HalfOpen states, configurable thresholds
- Backpressure propagation to producers
- ~1100 LOC total (implementation + tests)

**12.4.3 Operational Runbooks**:
- 7 runbooks (~3,400 LOC documentation): circuit breaker open, S3 throttling, PostgreSQL failover, segment corruption, producer backpressure, partition rebalancing, common errors FAQ

**12.4.4 Monitoring & Dashboards**:
- Additional Prometheus metrics (throttle/circuit breaker: 6 metrics, WAL: 4 metrics, consumer group: 6 metrics)
- 5 Grafana dashboards (system overview, producer, consumer, storage, throttling)
- 10 alerting rules

### Phase 14: Enhanced CLI
- REPL mode with command history and tab completion
- REST client integration
- Schema registry commands (register, list, get, compatibility)
- Output formatting (table, JSON, YAML)

### Phase 16: Exactly-Once Semantics
- Database schema: producers, producer_sequences, transactions, transaction_partitions, transaction_markers (`migrations/004_exactly_once.sql`)
- Transactional producer (`begin_transaction()`, commit, abort)
- Producer ID + epoch tracking for idempotent writes
- gRPC endpoints: `begin_transaction`, `commit_transaction`, `abort_transaction`
- IsolationLevel enum (ReadUncommitted, ReadCommitted)
- Last Stable Offset (LSO) tracking

### Phase 17: Fast Leader Handoff
- Distributed leader election (PostgreSQL + memory backends)
- Lease-based coordination with fencing tokens
- Leader change callbacks and handoff logic
- Agent coordination schema (`migrations/002_agent_coordination.sql`)

### Phase 21: Kafka Protocol Compatibility
- Kafka wire protocol decoder/encoder (`crates/streamhouse-kafka/`)
- Protocol version negotiation (ApiVersions)
- 14 Kafka API endpoints (Produce, Fetch, Metadata, etc.)
- Compatible with kafka-python, kafkajs
- Tenant resolution for Kafka connections

### Phase 21.5: Multi-Tenancy
- TenantContext + ApiKeyValidator (`crates/streamhouse-metadata/src/tenant.rs`)
- S3 prefix isolation per organization (`org-{uuid}/data/...`)
- Per-organization quotas and rate limiting (`src/quota.rs`)
- Plan-based quotas (Free/Pro/Enterprise tiers)
- Database schema: `migrations/003_multi_tenancy.sql`
- Kafka tenant support (`crates/streamhouse-kafka/src/tenant.rs`)

### Phases 21-23: Streaming SQL Analytics
- **Window Aggregations**: TUMBLE, HOP, SESSION windows; COUNT, SUM, AVG, MIN, MAX, FIRST, LAST
- **Anomaly Detection**: `zscore()`, `anomaly()`, `moving_avg()`, `stddev()` functions
- **Vector/Embedding Support**: `cosine_similarity()`, `euclidean_distance()`, `dot_product()`, `vector_norm()`
- Arrow-accelerated execution via DataFusion
- 40 passing tests

### Phase 24: Stream JOINs
- Stream-Stream JOINs (INNER, LEFT, RIGHT, FULL)
- JOIN parser with ON clause support
- Time-windowed join buffer
- Hash join execution engine
- Stream-Table JOINs (`TABLE(topic)` syntax)
- Predicate pushdown optimization

### Phase 25: Materialized Views
- `CREATE MATERIALIZED VIEW` parser
- View definition storage (PostgreSQL) with `migrations/007_materialized_views.sql`
- Background maintenance task
- Refresh modes: continuous, periodic, manual
- Delta processing & offset tracking
- SHOW/DESCRIBE/REFRESH commands

### Client SDKs (Phase 12.1)
- **Python** (`sdks/python/`) — sync + async (asyncio/aiohttp), type hints
- **TypeScript** (`sdks/typescript/`) — full type-safe client, Node.js 18+
- **Java** (`sdks/java/`) — Java 11+, builder pattern, Jackson serialization
- **Go** (`sdks/go/`) — idiomatic API, context support, functional options
- ~120 tests total across all SDKs

### Web Console
- **Next.js 14 + React Query** (`web/`)
- 22 route directories: dashboard, topics, consumers, agents, schemas, sql, monitoring, settings, producer-console, etc.
- Topics management, consumer groups, schema registry UI
- Multi-tenancy UI (org management, API keys, quotas)
- SQL Workbench with example queries
- Monitoring dashboard

### Phase 18.5-18.8: Native Client & Management
- Native Rust client (high-performance mode)
- Production demo application
- Consumer actions (reset offsets, delete groups, seek)
- SQL message query (lite)

### Phase 20: Developer Experience
- Docker Compose quickstart
- Benchmark suite
- Integration examples
- Production deployment guide

---

## REMAINING WORK

### Tier 1: Medium Priority (~3-4 weeks)

These are useful enhancements that would round out the platform for production SaaS use.

#### Phase 12.4.5: Shared WAL for Zero-Loss Failover (~3-5 days)

**Problem**: When an agent crashes and a *different* agent takes over its partitions, any
records that were acknowledged but not yet flushed to S3 are silently lost. The local WAL
only protects against same-node restarts — it cannot be read by another agent on a different
node. Failover stress testing (Feb 8, 2026) measured a 0.02% loss rate (12.6K records out of
64.56M) during a kill -9 crash at 1.5M msg/s, with a 5-second flush interval.

**Root cause**: The current write path acks after writing to a *local* WAL on the agent's
disk. The data then sits in the WAL and in-memory SegmentBuffer until the next flush cycle
writes a complete segment to S3. If the agent dies before that flush, the local WAL file is
stranded on the dead node's disk. The surviving agent starts fresh from the S3 high watermark,
creating a gap.

```
Current write path (local WAL):

  Producer ──ack──> Agent A ──WAL──> local disk ──flush──> S3
                                         ↑
                                    Agent B can't
                                    read this file
```

**Solution**: Replace local WAL with a shared WAL on network-attached storage (NFS, AWS EFS,
GCP Filestore) that all agents can access. When Agent A crashes and Agent B takes over a
partition, Agent B reads Agent A's WAL file from the shared filesystem, replays unflushed
records, and continues — zero acknowledged message loss.

```
Shared WAL write path:

  Producer ──ack──> Agent A ──WAL──> shared fs (EFS/NFS) ──flush──> S3
                                         ↑
                                    Agent B reads this
                                    on failover → zero loss
```

**Implementation plan**:
- Add `WAL_PATH` / `SHARED_WAL_PATH` env var for shared storage mount point
- WAL file naming includes agent ID: `{topic}-{partition}-{agent_id}.wal`
- On partition takeover, new agent scans for any WAL files for that partition from *any* agent
- Replay all records from discovered WAL files, dedup by offset against S3 high watermark
- Delete stale WAL files after successful replay + S3 flush
- Epoch fencing prevents the dead agent from writing if it somehow comes back
- Fallback: if no shared path configured, use local WAL (current behavior)

**Durability comparison**:

| Mode | Crash + same-node restart | Crash + different-node takeover | Latency impact |
|------|---------------------------|--------------------------------|----------------|
| Local WAL (current) | Zero loss | ~0.02% loss window | None |
| Shared WAL (EFS/NFS) | Zero loss | **Zero loss** | +1-2ms per write |
| No WAL | Data loss | Data loss | None (faster) |
| Ack after S3 flush | Zero loss | Zero loss | +seconds (unusable) |

**Why shared WAL over alternatives**:
- *WAL replication* (Kafka ISR-style) is more complex — requires a replication protocol,
  quorum writes, and adds 5-15ms latency. Overkill for StreamHouse's S3-native architecture.
- *Ack after S3 flush* gives zero loss but seconds of latency per produce — unusable.
- *Shared WAL* is the simplest path: same WAL code, same format, just a different mount point.
  AWS EFS provides 11-nines durability, cross-AZ replication, and <1ms overhead for writes.

**Risk**: Shared storage availability becomes a dependency. If EFS goes down, writes fail
(producers get errors, no silent data loss). This is an availability impact, not a durability
impact — already-acked records are safe on the shared disk or in S3.

**Status**: **COMPLETE**. Implemented Feb 8, 2026. Agent-specific WAL filenames
(`{topic}-{partition}-{agent_id}.wal`), `recover_partition()` for cross-agent WAL scanning,
stale file cleanup after S3 flush. Unified server supports `WAL_ENABLED` + `WAL_DIR` env vars.
Stress test result: loss dropped from 0.02% (12.6K/64.56M) to 0.0032% (436/13.76M). Remaining
loss is records in the WAL channel buffer not yet fdatasynced at kill -9 time.

---

#### Phase 12.4.6: WAL Batch Write Optimization (~1 day)

**Problem**: Enabling WAL dropped throughput from ~1.5M msg/s to ~310K msg/s (5x regression).
The `produce_batch` gRPC handler calls `wal.append()` individually for each record in the batch
(2000 times per request). Each call allocates a `Vec<u8>`, serializes the record, and sends
through the mpsc channel. This creates 2000 allocations + 2000 channel sends per produce request.

**Root cause**: The `produce_batch` handler loops over records and calls `PartitionWriter::append()`
one at a time. Each `append()` calls `wal.append()` which serializes and sends individually.
Meanwhile, `WAL::append_batch()` already exists and can serialize all records into a single buffer
with a single channel send — but it's not used by the produce path.

Additionally, `WAL::append_batch()` currently always calls `flush_batch()` (waits for fdatasync)
regardless of batch mode. In batched mode (`batch_enabled=true`), individual `append()` calls are
fire-and-forget — the writer task flushes on thresholds. `append_batch()` should respect the same
`sync_append` flag.

**Implementation plan**:
1. Fix `WAL::append_batch()` to respect `sync_append` (skip `flush_batch()` in batched mode)
2. Add `PartitionWriter::append_batch()` that collects records and calls `wal.append_batch()` once
3. Update `produce_batch` in `services/mod.rs` to use `PartitionWriter::append_batch()`

**Expected result**: ~800K-1.2M msg/s with WAL enabled (based on WAL standalone benchmark of
2.21M rec/s with group commit). The single channel send per 2000-record batch amortizes
serialization and channel overhead.

**Detailed analysis**: See `docs/WAL_PERFORMANCE_TRADEOFFS.md` for full options comparison.

**Status**: **COMPLETE**. Implemented Feb 10, 2026. Fixed `WAL::append_batch()` to respect
`sync_append` flag (fire-and-forget in batched mode). Added `PartitionWriter::append_batch()`
for single-WAL-write batching. Rewrote gRPC `produce_batch` handler to collect all records
and call `append_batch()` once instead of per-record `append()` loop.

---

#### Phase 10.3c: Cross-Region Replication (~1 day)
- Async mirror to DR region
- S3 versioning on all buckets
- RTO/RPO target enforcement
- **Status**: **COMPLETE**. Implemented Feb 10, 2026. `ReplicationManager` in
  `crates/streamhouse-api/src/replication.rs` (~1039 lines, 15 tests). Supports
  start/pause/resume lifecycle, segment mirroring via `object_store`, per-region
  lag tracking, RPO compliance checking, multi-target replication.

#### Phase 11.2: RBAC & Governance (~2 days)
- Role-based access (admin, operator, developer, viewer)
- Fine-grained permissions (topic, group, schema level)
- Policy engine (Open Policy Agent integration)
- Data masking/redaction for PII protection
- **Status**: **COMPLETE**. Implemented Feb 10, 2026. `RbacManager` in
  `crates/streamhouse-api/src/rbac.rs` (~1800 lines, 30+ tests). 4 built-in roles
  (admin/operator/developer/viewer), role hierarchy with parent chain resolution,
  `RbacLayer` Tower middleware, `DataMasker` with redact/hash/partial mask types,
  field-pattern-based masking policies.

#### Phase 11.4: Backup & Migration Tools (~1-2 days)
- Metadata export/import (JSON, SQL formats)
- Topic mirror tool (copy to another cluster)
- Kafka -> StreamHouse migration utility
- Schema registry import (from Confluent)
- Automated backup scheduler
- **Status**: **COMPLETE**. Implemented Feb 10, 2026. Extended
  `crates/streamhouse-metadata/src/backup.rs` with `BackupScheduler` (interval-based,
  retention cleanup, optional gzip compression), `TopicMirror` (cross-cluster HTTP
  replication), `KafkaMigrator` (framework for rdkafka-based migration, estimation),
  `SchemaImporter` (Confluent Schema Registry import). 12+ tests.

#### Phase 12.2: Framework Integrations (~4 days)
- Spring Boot (`@StreamHouseListener` annotation, auto-config, health indicators)
- FastAPI/Flask (dependency injection, background tasks)
- Node.js/Express (event publishing middleware, consumer background worker)
- Go (net/http + Gin middleware, background consumer)
- **Status**: **COMPLETE**. Implemented Feb 10, 2026. Framework bindings added to all 4 SDKs:
  `sdks/go/frameworks.go` (net/http + Gin middleware, background consumer),
  `sdks/python/streamhouse/frameworks/` (FastAPI dependency injection),
  `sdks/typescript/src/frameworks/` (Express middleware),
  `sdks/java/src/main/java/io/streamhouse/spring/` (Spring Boot auto-config).

#### UI.9: Consumer Simulator (~1 day)
- Create Consumer Group form
- Consumer simulation panel (create virtual consumers in the UI)
- Lag visualization
- Offset reset controls
- **Status**: **COMPLETE**. Implemented Feb 10, 2026. New page at
  `web/app/consumer-simulator/page.tsx`. Consumer group config, topic selection,
  virtual consumer slider (1-10), processing delay slider, live partition lag bars,
  real-time consumed message feed, throughput/lag stats panel. Added to sidebar nav.

#### UI.10: Web Console Polish & Fixes (~2-3 days)

**Problem**: Many dashboard components display hardcoded or placeholder values instead of
real data from the backend. The UI looks complete at first glance but several metrics are
non-functional, undermining trust in the monitoring experience.

**Known issues**:
- **Storage page — WAL Sync Lag**: Hardcoded to `'0ms'` (`/app/storage/page.tsx:119`), not fetched from API
- **Storage page — WAL Size / Uncommitted Entries**: May always show 0 if backend `/api/v1/metrics/storage` doesn't populate `walSize` / `walUncommittedEntries`
- **Storage page — S3 Metrics**: Request count, throttle rate, cost estimates all hardcoded/placeholder (`-`, `$0.00`)
- **Storage page — Cache Performance**: "Time-series cache metrics not yet implemented" placeholder
- **Agents page — CPU/Memory/Disk**: All show `0%` / `N/A` — resource metrics not wired to backend
- **Consumers page — Lag Trend Chart**: "Time-series lag metrics not yet implemented" placeholder
- **Monitoring page — System Metrics**: Falls back to generated sample data when API unavailable

**Implementation plan**:
1. Audit every dashboard page for hardcoded/placeholder values
2. Wire WAL metrics (size, uncommitted entries, sync lag) from backend Prometheus counters through REST API to UI
3. Wire agent resource metrics (CPU, memory, disk) or remove the placeholders
4. Implement or stub time-series endpoints for cache performance and consumer lag trends
5. Replace S3 placeholder metrics with real values from the rate limiter / circuit breaker stats
6. End-to-end smoke test: start server, produce messages, verify every dashboard widget shows live data
7. Fix any broken navigation, dead links, or layout issues discovered during audit

**Status**: **COMPLETE**. Implemented Feb 10, 2026. All identified issues fixed:
- Storage page: WAL sync lag wired to `walSyncLagMs` (was hardcoded `'0ms'`), WAL
  enabled/disabled badge, S3 metrics (request count, throttle rate, latency, est. cost)
  wired to real data, cache performance with hit rate progress bar and eviction stats.
- Agents page: replaced fake CPU/Memory/Disk (0%/N/A) with heartbeat-based health
  detection, uptime column, partition distribution bar chart.
- Consumers page: replaced "not yet implemented" placeholder with top-5 consumer
  groups by lag horizontal bar chart.
- Monitoring page: removed random sample data generation, added API connection status
  indicator, shows placeholder message when server not connected.

### Tier 2: Lower Priority (~6 weeks)

Features that add competitive differentiation but aren't blocking production use.

#### Phase 10.6: Chaos Testing Suite (~1 week)
- Formal chaos engineering framework
- Inject S3 throttling, network partitions, agent crashes
- Prove durability claims under failure conditions
- Automated chaos test runner
- **Status**: Not started. Some resilience exists (circuit breakers, WAL recovery) but no formal chaos tests.

#### Phase 10.7-10.10: Advanced Resilience (~2-3 weeks combined)
| Sub-phase | Feature | Notes |
|-----------|---------|-------|
| 10.7 | Idempotent producers (dedup) | Partially covered by exactly-once; standalone dedup not implemented |
| 10.8 | Hot-partition mitigation (auto-split) | Not started |
| 10.9 | Credit-based backpressure | Not started (S3 backpressure exists, but not end-to-end credit flow) |
| 10.10 | Enhanced ops metrics | Partially covered by observability; some debugging-specific metrics missing |

#### Phase 13: Advanced Features (~2 weeks)
| Sub-phase | Feature | Est. | Notes |
|-----------|---------|------|-------|
| 13.1 | Transactions & 2PC | 4-5d | Exactly-once exists; full 2PC with transaction log not implemented |
| 13.2 | Tiered storage (hot/warm/cold) | 2-3d | S3 only today, no Glacier lifecycle |
| 13.3 | Log compaction enhancements | 1-2d | Basic compaction exists; tombstone TTL, compaction policies missing |
| 13.4 | Multi-region active-active | 2-3d | Not started |

#### Phase 12.3: Connectors (~3 days)
- Kafka Connect compatibility layer
- Debezium CDC (Postgres, MySQL, MongoDB)
- S3 sink (Parquet, Avro, JSON)
- Elasticsearch sink
- **Status**: Not started.

#### Phase 14.2-14.3: Stream Processing & CDC (~1 week)
- Stateful stream processing with RocksDB state stores
- Checkpointing for exactly-once stream processing
- Analytics connectors (PostgreSQL CDC, MySQL CDC, Snowflake sink, BigQuery sink)
- **Status**: Not started. SQL engine exists but not continuous stream processing with state stores.

#### Phase 15: Testing & Quality (~1 week)
- Target 80%+ unit test coverage
- Integration test suite
- Performance regression tests in CI
- Fuzz testing (cargo-fuzz)
- Security scanning
- CI/CD pipeline with quality gates
- **Status**: Tests exist per-feature but no unified coverage tracking, no CI pipeline, no fuzz tests.

#### Phase 16: Documentation (~1 week)
- API reference (auto-generated from code)
- Architecture guide
- Tutorial series (quickstart in 15 min)
- Best practices guide
- Migration guides (from Kafka, Redpanda, etc.)
- Video tutorials
- **Status**: Many docs exist in `docs/` but no structured documentation site.

### Tier 3: Strategic / Long-Term

#### Phase 11: Distributed Architecture (~160 hours)
- Horizontal scaling across multiple nodes
- Distributed consensus (Raft or external coordination)
- Partition rebalancing across nodes
- Network topology awareness
- **Status**: Not started. Currently single-server with multi-agent on same node.
- **Impact**: CRITICAL for scaling beyond single-machine limits.

#### Phase 19: Full SQL Stream Processing Engine (~120-200 hours)
- Continuous queries (persistent queries that run indefinitely)
- Stateful processing with RocksDB state stores
- Stream-stream joins with exactly-once guarantees
- Watermark propagation
- **Status**: Planned (`docs/PHASE_19_STREAM_PROCESSING_PLAN.md`). Current SQL engine is query-at-a-time, not continuous.

#### Phase 20 (npm): CLI Distribution (~15-20 hours)
- npm installable CLI (`npm install -g streamhouse`)
- Simplified authentication (`streamhouse login`)
- Shorter command syntax
- **Status**: Planned (`docs/PHASE_20_NPM_DISTRIBUTION_PLAN.md`). Not started.

### Tier 4: AI/ML Capabilities (~285 hours)

None of these have been started. All are greenfield.

| Phase | Feature | Est. | Priority |
|-------|---------|------|----------|
| AI-1 | Natural Language -> SQL | 25h | HIGH — quick differentiator |
| AI-2 | Schema inference (auto-detect from messages) | 15h | HIGH |
| AI-3 | ML feature pipelines (feature store integration) | 55h | HIGH |
| AI-4 | Anomaly detection (ML-based, beyond z-score) | 40h | MEDIUM |
| AI-5 | Vector/embedding streaming (RAG pipelines) | 100h | STRATEGIC |
| AI-6 | Intelligent alerting (ML-driven thresholds) | 50h | LOWER |

Note: Basic anomaly detection (`zscore()`, `anomaly()`) and vector similarity (`cosine_similarity()`, `euclidean_distance()`) already exist in the SQL engine. These AI phases would add ML model integration, training pipelines, and more sophisticated algorithms.

---

## Progress Visualization

```
OVERALL COMPLETION  █████████████████████████░░░░░░░  ~80%

Core Platform        [██████████████████████████████] 100%  Phases 1-6
Observability        [██████████████████████████████] 100%  Phase 7
Performance          [██████████████████████████████] 100%  Phase 8
Schema Registry      [██████████████████████████████] 100%  Phase 9
Advanced Consumer    [██████████████████████████████] 100%  Phase 9.3-9.4
Production Hardening [██████████████████████████████] 100%  Phase 10
Operational Reliab.  [██████████████████████████████] 100%  Phase 12.4
WAL Batch Optimize   [██████████████████████████████] 100%  Phase 12.4.6
Cross-Region Repl.   [██████████████████████████████] 100%  Phase 10.3c
Exactly-Once         [██████████████████████████████] 100%  Phase 16
Leader Handoff       [██████████████████████████████] 100%  Phase 17
Kafka Compatibility  [██████████████████████████████] 100%  Phase 21
Multi-Tenancy        [██████████████████████████████] 100%  Phase 21.5
Streaming SQL        [██████████████████████████████] 100%  Phases 21-25
Client SDKs          [██████████████████████████████] 100%  Phase 12.1
Framework Integr.    [██████████████████████████████] 100%  Phase 12.2
Web Console          [██████████████████████████████] 100%  Web UI
Consumer Simulator   [██████████████████████████████] 100%  UI.9
Web Console Polish   [██████████████████████████████] 100%  UI.10
Enhanced CLI         [██████████████████████████████] 100%  Phase 14
Developer Experience [██████████████████████████████] 100%  Phase 20
RBAC & Governance    [██████████████████████████████] 100%  Phase 11.2
Backup & Migration   [██████████████████████████████] 100%  Phase 11.4
Connectors           [░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]   0%  Phase 12.3
Chaos Testing        [░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]   0%  Phase 10.6
Distributed Arch     [░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]   0%  Phase 11
Stream Processing    [░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]   0%  Phase 19
AI/ML Capabilities   [░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]   0%  Tier 4
Testing & Quality    [██████░░░░░░░░░░░░░░░░░░░░░░░░]  20%  Phase 15
Documentation        [████████░░░░░░░░░░░░░░░░░░░░░░]  25%  Phase 16
```

---

## Effort Remaining

| Tier | Phases | Effort | Calendar | Status |
|------|--------|--------|----------|--------|
| ~~**Tier 1**~~ | ~~12.4.6, 10.3c, 11.2, 11.4, 12.2, UI.9, UI.10~~ | ~~112h~~ | ~~2.8 weeks~~ | **COMPLETE** |
| **Tier 2** (Lower priority) | 10.6-10.10, 12.3, 13, 14.2-14.3, 15, 16 | ~250h | ~6 weeks | Not started |
| **Tier 3** (Strategic) | 11, 19, 20 (npm) | ~340h | ~8.5 weeks | Not started |
| **Tier 4** (AI/ML) | AI-1 through AI-6 | ~285h | ~7 weeks | Not started |
| **TOTAL REMAINING** | | **~875h** | **~22 weeks** | |

---

## Recommended Next Priorities

### Quick wins (< 1 week each)
1. ~~**Phase 12.4.5: Shared WAL**~~ — **DONE** (Feb 8).
2. ~~**Phase 12.4.6: WAL Batch Optimization**~~ — **DONE** (Feb 10).
3. ~~**UI.10: Web Console Polish**~~ — **DONE** (Feb 10).
4. ~~**Phase 12.2: Framework Integrations**~~ — **DONE** (Feb 10).
5. ~~**Phase 11.2: RBAC**~~ — **DONE** (Feb 10).
6. ~~**Phase 10.3c: Cross-Region Replication**~~ — **DONE** (Feb 10).
7. ~~**Phase 11.4: Backup & Migration Tools**~~ — **DONE** (Feb 10).
8. ~~**UI.9: Consumer Simulator**~~ — **DONE** (Feb 10).
9. **AI-1: Natural Language -> SQL** — High-impact differentiator, builds on existing SQL engine

### Medium-term (1-2 weeks each)
4. **Phase 15: Testing & Quality** — CI pipeline, coverage tracking, fuzz testing
5. **Phase 10.6: Chaos Testing** — Proves reliability claims with evidence
6. **Phase 12.3: Connectors** — Kafka Connect + Debezium unlocks CDC use cases

### Strategic (multi-week)
7. **Phase 11: Distributed Architecture** — Required for scaling beyond single node
8. **Phase 19: Continuous Stream Processing** — Completes the "Kafka + Flink in one" story

---

## Codebase Inventory

### Workspace Crates (13)
| Crate | Purpose |
|-------|---------|
| `streamhouse-core` | Shared types, segment format, compression |
| `streamhouse-storage` | S3 writer/reader, WAL, rate limiter, circuit breaker, Bloom filters |
| `streamhouse-metadata` | SQLite + PostgreSQL metadata, migrations, tenancy, quotas |
| `streamhouse-agent` | Agent binary, gRPC service, partition management |
| `streamhouse-kafka` | Kafka wire protocol compatibility layer |
| `streamhouse-sql` | SQL parser, executor, windows, anomaly, vectors, JOINs, MVs |
| `streamhouse-server` | Unified server binary (gRPC + REST + Schema Registry) |
| `streamhouse-cli` | CLI tool with REPL mode |
| `streamhouse-client` | Rust client library (Producer, Consumer, Admin) |
| `streamhouse-proto` | Protobuf definitions |
| `streamhouse-api` | REST API, TLS, JWT, ACL, OAuth, SASL, leader election, failover |
| `streamhouse-schema-registry` | Schema storage, versioning, compatibility |
| `streamhouse-observability` | Prometheus metrics, health checks |

### External Components
| Component | Technology |
|-----------|------------|
| Web UI | Next.js 14, React Query (`web/`) |
| Python SDK | asyncio, aiohttp, requests (`sdks/python/`) |
| TypeScript SDK | Node.js 18+ (`sdks/typescript/`) |
| Java SDK | Java 11+, Jackson (`sdks/java/`) |
| Go SDK | Context-based, functional options (`sdks/go/`) |
| Runbooks | 7 operational guides (`docs/runbooks/`) |

### Key Dependencies
| Dependency | Use |
|------------|-----|
| DataFusion 43.0 + Arrow 53.0 | SQL query execution |
| Tonic 0.11 | gRPC framework |
| SQLx 0.7 | Database access |
| object_store 0.9 | S3/MinIO storage |
| rustls | TLS/mTLS |
| lz4_flex | Compression |
| prometheus-client | Metrics |
| axum 0.7 | HTTP/REST server |
