# StreamHouse Architecture Overview

StreamHouse is an S3-native event streaming platform with Kafka-compatible APIs. This document covers the core architecture: topics, partitions, agents, leases, and how they coordinate to provide scalable, fault-tolerant event streaming.

## The Big Picture

```
                          ┌──────────────────────────────────────────┐
                          │           Unified Server                 │
                          │                                          │
  Producers ─────────────►│  REST API (:8080)    Kafka Proto (:9092) │
                          │  gRPC     (:50051)                       │
                          └──────────┬───────────────────────────────┘
                                     │
                          ┌──────────▼───────────────────────────────┐
                          │         Agent (partition leader)          │
                          │                                          │
                          │  PartitionAssigner ◄── MetadataStore     │
                          │       │                  (Postgres)      │
                          │       ▼                                  │
                          │  LeaseManager ──► partition_leases table │
                          │       │                                  │
                          │       ▼                                  │
                          │  WriterPool                              │
                          │       │                                  │
                          │       ▼                                  │
                          │  WAL (disk) ──► SegmentBuffer (RAM)      │
                          │                       │                  │
                          └───────────────────────┼──────────────────┘
                                                  │
                                                  ▼
                                           ┌─────────────┐
                                           │     S3      │
                                           │  (segments) │
                                           └─────────────┘
```

---

## Topics

A **topic** is a named, durable stream of events — analogous to a Kafka topic. Examples: `orders`, `clicks`, `user-events`.

```rust
TopicConfig {
    name: "orders",
    partition_count: 6,
    retention_ms: 86400000,       // 24 hours
    cleanup_policy: Delete,       // Delete old segments (vs Compact)
    config: {},
}
```

When you create a topic, the metadata store atomically creates the topic row **and** all its partition rows in a single transaction. Topics are immutable once created — you cannot change the partition count after creation.

---

## Partitions

Each topic is split into **partitions** — independent, ordered sequences of records. Partitions are what give you parallelism: more partitions means more agents can write concurrently.

- Topic `orders` with 6 partitions creates partitions `0, 1, 2, 3, 4, 5`
- Each partition has a **high watermark** — the next offset to write
- Each partition has **exactly one leader** at any time (the agent holding its lease)
- Producers pick a partition via `hash(key) % partition_count`

```
Topic: orders (6 partitions)
┌──────────────┬─────────────────┬────────────────┐
│  Partition   │  Leader Agent   │  High Watermark│
├──────────────┼─────────────────┼────────────────┤
│  0           │  agent-1        │  1042          │
│  1           │  agent-2        │  887           │
│  2           │  agent-3        │  1205          │
│  3           │  agent-1        │  956           │
│  4           │  agent-2        │  1100          │
│  5           │  agent-3        │  743           │
└──────────────┴─────────────────┴────────────────┘
```

---

## Agents

An **agent** is a process that owns partitions and writes data to S3. It is the core unit of horizontal scaling in StreamHouse.

Each agent:

1. **Registers** itself in the metadata store with a unique ID and address
2. **Heartbeats** every 10–20 seconds to prove it's alive
3. **Gets assigned partitions** via consistent hashing
4. **Acquires leases** on its assigned partitions
5. **Writes data** through the write path: WAL (disk) → SegmentBuffer (RAM) → S3

```
AgentInfo {
    agent_id:          "agent-1"
    address:           "agent-1:9090"        // gRPC endpoint
    availability_zone: "us-east-1a"          // rack-aware placement
    agent_group:       "default"             // isolation group
    last_heartbeat:    1709312400000         // epoch ms
    started_at:        1709312000000
}
```

**Liveness**: An agent is considered alive if `last_heartbeat > now - 60s`. If it stops heartbeating (crash, network issue), it disappears from the active agent list after 60 seconds and its partitions are redistributed.

### Two Deployment Modes

```
Mode 1: Embedded Agent (default, simple)       Mode 2: Standalone Agents (scalable)
┌──────────────────────────┐                   ┌──────────────────────────┐
│    Unified Server        │                   │    Unified Server        │
│  ┌────────────────────┐  │                   │  DISABLE_EMBEDDED_AGENT  │
│  │  Embedded Agent    │  │                   │  (API only, no agent)    │
│  │  - Heartbeat       │  │                   └──────────────────────────┘
│  │  - Assigner        │  │                          │
│  │  - LeaseManager    │  │                   ┌──────┼──────┬─────────┐
│  └────────────────────┘  │                   ▼      ▼      ▼         ▼
│  REST + gRPC + Kafka     │               agent-1 agent-2 agent-3  agent-N
└──────────────────────────┘               (each runs heartbeat, assigner, leases)
```

| Mode | When to use | Pros | Cons |
|------|-------------|------|------|
| **Embedded** | Dev, single-node | One process, simple | Can't scale agents independently |
| **Standalone** | Production, multi-node | Scale agents separately, fault isolation | More containers to manage |

Set `DISABLE_EMBEDDED_AGENT=1` on the unified server to use standalone mode. The `docker-compose.yml` ships with 3 standalone agents by default.

---

## Partition Assignment (Round-Robin)

The **PartitionAssigner** runs on each agent every 30 seconds. It distributes partitions evenly across live agents using deterministic round-robin — all agents compute the same result independently, no central coordinator required.

### How It Works

Agents are sorted by ID. Partitions are assigned in round-robin order:

```
3 agents, 6 partitions:

  agent-1: [0, 3]     (partitions 0, 3)
  agent-2: [1, 4]     (partitions 1, 4)
  agent-3: [2, 5]     (partitions 2, 5)

  Each agent gets exactly partition_count / agent_count partitions.
```

Round-robin guarantees perfectly even distribution. Every agent gets the same number of partitions (±1 when not evenly divisible).

### The Rebalance Loop

```
Every 30 seconds on each agent:

  ┌─────────────────────────────────────────────────┐
  │  1. List live agents from metadata store        │
  │                                                 │
  │  2. Has the agent list changed?                 │
  │     ├── No  → skip (optimization)               │
  │     └── Yes → recalculate                       │
  │                                                 │
  │  3. Sort agents by ID                           │
  │  4. For each (topic, partition):                │
  │     owner = agents[partition_index % agent_count]│
  │                                                 │
  │  5. Diff against current assignments:           │
  │     to_acquire = [should own, don't yet]        │
  │     to_release = [own now, shouldn't]           │
  │                                                 │
  │  6. Release old leases                          │
  │  7. Acquire new leases                          │
  └─────────────────────────────────────────────────┘
```

---

## Leases (Leadership Coordination)

A **lease** is a time-limited lock on a partition. Only the lease holder can write to that partition. Leases prevent two agents from writing to the same partition simultaneously.

```
partition_leases table:
┌─────────┬──────────────┬──────────┬─────────────┬──────────────────┐
│ topic   │ partition_id │ agent_id │ lease_epoch │ lease_expires_at  │
├─────────┼──────────────┼──────────┼─────────────┼──────────────────┤
│ orders  │ 0            │ agent-1  │ 3           │ 1709312430000    │
│ orders  │ 1            │ agent-2  │ 1           │ 1709312428000    │
│ orders  │ 2            │ agent-3  │ 5           │ 1709312432000    │
└─────────┴──────────────┴──────────┴─────────────┴──────────────────┘
```

### Lease Lifecycle

| Property | Value |
|----------|-------|
| **Duration** | 30 seconds |
| **Renewal interval** | Every 10 seconds (background task) |
| **Acquisition** | Compare-and-swap in DB |
| **Conflict resolution** | First writer wins; others get `ConflictError` |
| **Cache** | In-memory cache avoids DB roundtrip on every write |

```
Lease Lifecycle:

  acquire (epoch=1)      renew           renew           expires
       │                   │               │               │
  ─────●───────────────────●───────────────●───────────────●─────
       │◄──── 10s ────────►│◄──── 10s ────►│◄──── 10s ────►│
       │◄──────────────── 30s TTL ─────────────────────────►│
```

### Lease Acquisition (Compare-and-Swap)

```sql
BEGIN TRANSACTION

SELECT * FROM partition_leases WHERE topic='orders' AND partition_id=0;

CASE:
  No row exists        → INSERT (epoch=1, agent=agent-1, expires=now+30s)
  Lease expired        → UPDATE (epoch++, agent=agent-1, expires=now+30s)
  Already mine         → UPDATE (expires=now+30s)  -- renewal
  Held by other agent  → ABORT (ConflictError)

COMMIT
```

### Epoch Fencing (Preventing Split-Brain)

The **epoch** increments every time a partition gets a new leader. This is a fencing token that prevents stale writes from old leaders.

```
Problem: Network partition could cause two agents to think they lead partition 0

t=0s:     agent-1 acquires lease → epoch=1
t=30s:    agent-1 loses connectivity, lease expires
t=60s:    agent-2 acquires lease → epoch=2
t=65s:    agent-1 reconnects, tries to write with epoch=1

          validate_epoch(expected=1, actual=2)
          → REJECTED: StaleEpoch error
          → agent-1's write is safely blocked
```

Every write operation validates the epoch before committing:

```
1. epoch = lease_manager.ensure_lease("orders", 0)   // epoch=5
2. ... prepare write batch ...
3. validate_epoch("orders", 0, expected=5)
   → if current epoch != 5: reject (stale leader)
   → if current epoch == 5: safe to write
4. writer.append(record, epoch=5)
```

---

## The Full Write Path

```
┌──────────┐     ┌──────────────────────────────────────────────────────┐
│ Producer │     │                    Agent                             │
│          │     │                                                      │
│  record ─┼────►│  1. Partition selection                              │
│          │     │     partition = hash(key) % partition_count           │
│          │     │                                                      │
│          │     │  2. Lease check                                      │
│          │     │     epoch = lease_manager.ensure_lease(topic, part)   │
│          │     │     (in-memory cache → DB fallback)                   │
│          │     │                                                      │
│          │     │  3. WAL append (crash safety)                        │
│          │     │     channel → batch → write_all → fdatasync          │
│          │     │     (~50ns per record in batched mode)                │
│          │     │                                                      │
│          │     │  4. Segment buffer (accumulate in RAM)               │
│          │     │     delta-encode offsets/timestamps                   │
│          │     │     LZ4 compress into blocks                         │
│          │     │                                                      │
│          │     │  5. Segment roll (when thresholds met)               │
│          │     │     size > 1MB? OR age > 10s?                        │
│          │     │     → Upload segment to S3                           │
│          │     │     → Update high watermark in metadata              │
└──────────┘     └──────────────────────────────────────────────────────┘
                                        │
                                        ▼
                                 ┌─────────────┐
                                 │  S3 Segment  │
                                 │  (.strm)     │
                                 │              │
                                 │  STRM magic  │
                                 │  Block 0     │
                                 │  Block 1     │
                                 │  ...         │
                                 │  Index       │
                                 │  Footer      │
                                 └─────────────┘
```

### Segment Format

Segments are the on-disk/S3 storage unit. Each segment is a self-contained file:

```
┌────────────────────────────────┐
│  Magic: STRM (4 bytes)        │
├────────────────────────────────┤
│  Block 0 (LZ4 or Zstd)        │
│    record, record, record...  │
├────────────────────────────────┤
│  Block 1 (LZ4 or Zstd)        │
│    record, record, record...  │
├────────────────────────────────┤
│  ...                          │
├────────────────────────────────┤
│  Index (delta-encoded)        │
│    block offset, timestamp    │
│    block offset, timestamp    │
├────────────────────────────────┤
│  Footer                       │
│    base_offset, record_count  │
│    min/max timestamp          │
│    index_offset               │
└────────────────────────────────┘
```

---

## Write-Ahead Log (WAL)

The WAL provides crash safety for the write path. When enabled, every record is written to the WAL on local disk before it enters the in-memory segment buffer.

### Group Commit

The WAL uses a channel-based group commit design for high throughput:

```
Producer threads → [mpsc channel] → WAL Writer Task → write_all → fdatasync
                                          ↓
                                   Notify all waiting producers
```

Multiple producers send records through an mpsc channel. A dedicated writer task collects a batch, writes all records in a single `write_all` call, then calls `fdatasync` once. All producers in the batch share the single fsync cost.

### Fsync Policy

| Policy | Behavior | Risk window |
|--------|----------|-------------|
| **Interval** (default) | fsync every `WAL_SYNC_INTERVAL_MS` (default 100ms) | Up to 100ms of writes lost on power failure |
| **Always** | fsync after every batch | Zero loss, highest latency |
| **None** | OS decides when to flush | Entire WAL could be lost on power failure |

Process crashes (disk survives) lose nothing regardless of fsync policy — the OS buffer cache survives.

### Recovery

On startup, `wal.recover()` reads the WAL sequentially, validates CRC32 checksums, skips corrupted trailing records, and replays everything into the segment buffer. Cross-agent recovery is also supported: if agent-A dies and agent-B acquires the partition lease, agent-B can recover agent-A's WAL files if the disk is accessible.

| Scenario | Data lost? |
|----------|-----------|
| Process crash, disk OK | No |
| Agent dies, another takes over, disk readable | No (cross-agent WAL recovery) |
| WAL disk destroyed | Yes — unflushed batch only |
| `ACK_DURABLE` mode, any failure | No (already in S3) |

### Batched Durable Acks

When using `ACK_DURABLE` mode, StreamHouse batches multiple produce requests into a single S3 upload:

```
Producers → [mpsc channel] → DurableFlushTask → roll segment → S3
                                    ↓
                              Notify all waiters
```

The durable flush task waits up to `DURABLE_BATCH_MAX_AGE_MS` (default 200ms) collecting requests, then performs a single segment roll + S3 upload. All producers in the batch share the S3 round-trip cost.

| Config | Default | Description |
|--------|---------|-------------|
| `DURABLE_BATCH_MAX_AGE_MS` | `200` | Max wait before flushing batch |
| `DURABLE_BATCH_MAX_RECORDS` | `10000` | Flush after N records |
| `DURABLE_BATCH_MAX_BYTES` | `16777216` (16 MB) | Flush after N bytes |

---

## Log Compaction

Topics with `cleanup_policy: Compact` retain only the latest value per key. This is useful for changelogs, state tables, and CDC streams.

### How It Works

A background compaction thread periodically scans compactable topics:

1. **Dirty ratio check**: If the ratio of duplicate keys exceeds the threshold (default 0.5), compaction triggers
2. **Key scan**: Read all segments, build a map of `key → latest offset`
3. **Rewrite**: Create new segments containing only the latest record per key
4. **Tombstones**: Records with null values are tombstones — kept for a configurable period (default 24 hours) then expired

| Config | Default | Description |
|--------|---------|-------------|
| `min_compaction_interval` | 600s | Min time between compaction runs |
| `tombstone_retention` | 86400s (24h) | How long tombstones are retained |
| `min_dirty_ratio` | 0.5 | Duplicate key ratio to trigger compaction |
| `max_segment_size_bytes` | 1 GB | Max output segment size |
| `compaction_threads` | 2 | Parallel compaction workers |

---

## Multi-Tenancy

Every piece of data in StreamHouse is scoped to an **organization**. Organizations provide complete isolation:

- **S3 paths**: `org-{uuid}/data/{topic}/{partition}/{base_offset}.seg`
- **Metadata**: Topics, consumer groups, connectors, transactions, schemas — all org-scoped
- **Cache**: Cache keys include `org_id` to prevent cross-org reads
- **Metrics**: Prometheus metrics carry `org_id` as a label

API keys are tied to organizations and support permissions (`read`, `write`, `admin`) and topic-level scopes with wildcard patterns (`orders-*`).

See [Authentication](authentication.md) for details.

---

## SQL Query Engine

StreamHouse includes a built-in SQL engine powered by Apache DataFusion and Apache Arrow for columnar execution.

### Supported SQL

- Standard: `SELECT`, `WHERE`, `GROUP BY`, `ORDER BY`, `LIMIT`, `OFFSET`
- Aggregations: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`
- JSON operators: `json_extract()`, `->`, `->>`
- Window functions: `TUMBLE()`, `HOP()`, `SESSION()` for time-based windowed aggregations
- Utility: `SHOW TOPICS`, `DESCRIBE <topic>`

### Streaming Windows

```sql
-- 5-minute tumbling window: count events per window
SELECT TUMBLE(timestamp, '5 minutes') as window, COUNT(*) as cnt
FROM events
GROUP BY window

-- 1-hour hopping window with 15-minute slide
SELECT HOP(timestamp, '15 minutes', '1 hour') as window, SUM(amount) as total
FROM orders
GROUP BY window
```

Window state is backed by RocksDB for durability across restarts.

---

## Observability

### Prometheus Metrics

StreamHouse exposes a Prometheus scrape endpoint at `/metrics`. Key metrics:

- `streamhouse_produce_total` — Records produced (by org, topic, protocol)
- `streamhouse_consume_total` — Records consumed
- `streamhouse_segment_upload_duration_seconds` — S3 upload latency
- `streamhouse_wal_records_total` — WAL throughput
- `streamhouse_cache_hit_ratio` — Segment cache hit rate

### Grafana Dashboards

Pre-built dashboards are included at `grafana/dashboards/`:
- Throughput and latency overview
- Per-topic storage and partition distribution
- Agent health and lease status
- Load test dashboard

### WebSocket Real-Time Metrics

The REST API supports WebSocket connections for live metric streaming:
- `/ws/metrics` — Real-time cluster metrics
- `/ws/topics/{name}` — Live message stream for a topic

---

## What Happens When Things Change

### Agent Joins

```
t=0s:    2 agents own 6 partitions (3 each)
         agent-1: [0, 2, 4]    agent-2: [1, 3, 5]

t=1s:    agent-3 registers, starts heartbeating

t=30s:   Rebalance runs on all agents
         Round-robin recalculates → each agent gets 2

         agent-1: releases [4]       → keeps [0, 2]
         agent-2: releases [5]       → keeps [1, 3]
         agent-3: acquires [4, 5]    → new partitions
```

```
Before:                              After:
┌──────────┬──────────┐             ┌──────────┬──────────┬──────────┐
│ agent-1  │ agent-2  │             │ agent-1  │ agent-2  │ agent-3  │
│ [0,2,4]  │ [1,3,5]  │    ───►    │  [0,2]   │  [1,3]   │  [4,5]   │
│ 3 parts  │ 3 parts  │             │ 2 parts  │ 2 parts  │ 2 parts  │
└──────────┴──────────┘             └──────────┴──────────┴──────────┘
```

### Agent Dies

```
t=0s:    3 agents, 2 partitions each
         agent-1: [0,2]  agent-2: [1,3]  agent-3: [4,5]

t=5s:    agent-3 crashes (no more heartbeats)

t=65s:   agent-3 falls out of live agent list (60s timeout)

t=90s:   Rebalance runs on agent-1 and agent-2
         agent-3's leases have expired (30s TTL)
         Round-robin redistributes:

         agent-1: [0,2] + [4] = [0,2,4]
         agent-2: [1,3] + [5] = [1,3,5]
```

```
Before:                                        After:
┌──────────┬──────────┬──────────┐            ┌──────────┬──────────┐
│ agent-1  │ agent-2  │ agent-3  │            │ agent-1  │ agent-2  │
│  [0,2]   │  [1,3]   │  [4,5]   │   ───►    │ [0,2,4]  │ [1,3,5]  │
│ 2 parts  │ 2 parts  │ 2 parts  │            │ 3 parts  │ 3 parts  │
└──────────┴──────────┴──────────┘            └──────────┴──────────┘
                          ╳ crash
```

### Graceful Shutdown

```
agent.stop():
  1. Stop assigner         → no more rebalances
  2. Stop lease renewal    → leases will expire naturally
  3. Flush pending writes  → all data safely on S3
  4. Release all leases    → immediate, no 30s wait
  5. Deregister            → removed from agent list instantly

  Other agents detect topology change on next rebalance cycle.
  Much faster than crash recovery (no 60s heartbeat timeout).
```

---

## Consumer Read Path

```
┌──────────┐     ┌──────────────────────────────────────────────────┐
│ Consumer │     │               Unified Server                     │
│          │     │                                                  │
│  fetch() ┼────►│  1. Look up committed offset                    │
│          │     │     offset = get_committed_offset(group, topic)  │
│          │     │                                                  │
│          │     │  2. Find segment containing offset               │
│          │     │     segment = find_segment_for_offset(topic,     │
│          │     │                partition, offset)                │
│          │     │                                                  │
│          │     │  3. Read from cache or S3                        │
│          │◄────┼     bytes = cache.get(segment.key)               │
│          │     │            OR object_store.get(segment.key)      │
│          │     │                                                  │
│          │     │  4. Decompress + deserialize                     │
│          │     │     LZ4 decompress → decode records              │
│          │     │                                                  │
│  commit()┼────►│  5. Commit progress                              │
│          │     │     commit_offset(group, topic, part, new_offset)│
└──────────┘     └──────────────────────────────────────────────────┘
```

---

## Lease Transfers (Graceful Handoff)

When an agent shuts down gracefully, it can **transfer** leases to other agents instead of just releasing them. This avoids a gap where partitions have no leader.

```
Transfer State Machine:

  ┌─────────┐    accept    ┌──────────┐    complete   ┌───────────┐
  │ Pending ├─────────────►│ Accepted ├──────────────►│ Completed │
  └────┬────┘              └────┬─────┘               └───────────┘
       │                        │
       │  reject                │  timeout
       ▼                        ▼
  ┌──────────┐            ┌──────────┐
  │ Rejected │            │  Failed  │
  └──────────┘            └──────────┘
```

```
agent-1 shutting down, transferring orders/0 to agent-2:

1. agent-1 → metadata: initiate_transfer(from=agent-1, to=agent-2)
2. agent-2 checks get_incoming_transfers() → sees pending transfer
3. agent-2 → metadata: accept_transfer(transfer_id)
4. agent-1 flushes all pending writes to S3
5. agent-1 → metadata: complete_transfer(transfer_id)
   → Atomically transfers lease to agent-2 with new epoch
6. agent-2 is now the leader for orders/0
```

---

## Disaster Recovery

StreamHouse is built around a key principle: **S3 is the source of truth**. Metadata snapshots taken after S3 flushes are always a subset of S3. This makes recovery straightforward.

### Snapshot Ordering

```
  Write data ──→ Upload segment to S3 ──→ Take metadata snapshot
                                              │
                                              ▼
                                    _snapshots/{timestamp}.json.gz
                                    (per-org, gzip compressed)
```

Snapshots are taken automatically on a configurable interval (`SNAPSHOT_INTERVAL_SECS`, default 3600). Each org gets its own snapshot path:
- Default org: `_snapshots/{timestamp}.json.gz`
- Per-org: `org-{uuid}/_snapshots/{timestamp}.json.gz`

### Automatic Recovery

On startup, if metadata is empty, the server self-heals:

```
  1. Discover orgs from S3 (list org-* prefixes)
  2. For each org, find latest snapshot
  3. Download, decompress, restore snapshot
  4. Run reconcile-from-s3 to fill gaps
  5. Server is fully recovered — no manual intervention
```

### Manual Recovery

```bash
# Run reverse reconciliation (scans S3, registers missing segments)
RECONCILE_FROM_S3=1 ./target/release/unified-server
```

### S3 Reconciler

Two modes:
- **Orphan cleanup** (`reconcile()`): Mark-and-sweep — deletes S3 segments with no metadata entry, but only if older than the grace period (default 1 hour). This prevents deleting segments that are in-flight (uploaded to S3 but metadata not yet committed).
- **Reverse reconciliation** (`reconcile_from_s3()`): Scans S3, reads segment headers, and registers missing segments in metadata. Used for DR recovery.

Both scan all org prefixes automatically.

| Config | Default | Description |
|--------|---------|-------------|
| `RECONCILE_INTERVAL` | `3600` (1 hour) | How often the orphan cleanup runs |
| `RECONCILE_GRACE` | `3600` (1 hour) | Grace period before orphan deletion |
| `RECONCILE_FROM_S3` | _(unset)_ | Set to `1` to run reverse reconciliation and exit |

---

## Component Summary

| Component | What it is | Coordination |
|-----------|-----------|--------------|
| **Topic** | Named event stream | Created atomically with partitions |
| **Partition** | Parallel shard of a topic | One leader at a time, high watermark tracking |
| **Agent** | Process that writes data to S3 | Heartbeat every 20s, 60s liveness timeout |
| **Lease** | Time-limited partition lock | 30s TTL, 10s renewal, epoch fencing |
| **Assigner** | Distributes partitions to agents | Deterministic round-robin, rebalance every 30s |
| **WAL** | Crash recovery log | Channel-based group commit, fdatasync |
| **Segment** | S3 data file | STRM format, LZ4, delta-encoded, block-based |
| **Epoch** | Leadership generation counter | Prevents stale writes from old leaders |

## Timing Summary

| Event | Duration |
|-------|----------|
| Heartbeat interval | 10–20 seconds |
| Heartbeat liveness timeout | 60 seconds |
| Lease duration (TTL) | 30 seconds |
| Lease renewal interval | 10 seconds |
| Rebalance interval | 30 seconds |
| WAL batch max age | 10 milliseconds |
| Segment max age | 10 seconds (configurable via `SEGMENT_MAX_AGE_MS`) |
| Segment max size | 1 MB (configurable, up to 100 MB) |

## Docker Compose Services

The default `docker-compose.yml` runs StreamHouse in standalone agent mode:

```
┌───────────────────────────────────────────────────────────────┐
│                     Docker Compose                            │
│                                                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────────────┐│
│  │ Postgres │  │  MinIO   │  │   Unified Server             ││
│  │  :5432   │  │  :9000   │  │   :8080 (REST)               ││
│  │          │  │  :9001   │  │   :50051 (gRPC)              ││
│  │ metadata │  │  segments│  │   DISABLE_EMBEDDED_AGENT=1   ││
│  └──────────┘  └──────────┘  └──────────────────────────────┘│
│       ▲             ▲               ▲                         │
│       │             │               │                         │
│  ┌────┴─────────────┴───────────────┴──────────────────────┐ │
│  │                                                          │ │
│  │  ┌─────────┐     ┌─────────┐     ┌─────────┐           │ │
│  │  │ agent-1 │     │ agent-2 │     │ agent-3 │           │ │
│  │  │ zone: a │     │ zone: b │     │ zone: a │           │ │
│  │  └─────────┘     └─────────┘     └─────────┘           │ │
│  │                                                          │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌────────────┐  ┌─────────┐                                 │
│  │ Prometheus │  │ Grafana │  (monitoring)                    │
│  │   :9091    │  │  :3001  │                                  │
│  └────────────┘  └─────────┘                                  │
└───────────────────────────────────────────────────────────────┘
```

### Quick Start

```bash
# Build and start everything
docker compose build
docker compose up -d

# Verify agents registered
curl -s http://localhost:8080/api/v1/agents | python3 -m json.tool

# Create a topic
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "my-topic", "partition_count": 6}'

# Check partition distribution
curl -s http://localhost:8080/api/v1/topics/my-topic/partitions | python3 -m json.tool

# Run the full e2e test
./tests/e2e_multi_agent.sh
```
