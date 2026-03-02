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

## Partition Assignment (Consistent Hashing)

The **PartitionAssigner** runs on each agent every 30 seconds. It determines which agent owns which partition using consistent hashing with virtual nodes. All agents compute the same result independently — no central coordinator required.

### How Consistent Hashing Works

Each agent gets **150 virtual nodes (vnodes)** placed on a hash ring. Each partition hashes to a point on the ring and is assigned to the nearest agent vnode clockwise.

```
                         Hash Ring (0 ──────────────► u64::MAX)

    agent-1:vn0    agent-2:vn42        agent-3:vn87    agent-1:vn149
         │              │                    │              │
         ▼              ▼                    ▼              ▼
    ─────●──────────────●────────────────────●──────────────●──────
              ▲                   ▲                   ▲
         orders:0            orders:1            orders:2
         → agent-2           → agent-3           → agent-1

    (Each partition is assigned to the nearest vnode clockwise)
```

### Hash Function

StreamHouse uses **FNV-1a with a Murmur3 finalizer** for hashing. This is critical:

- **Deterministic** — All agents compute the same hash for the same input, so they agree on assignments without coordination
- **Good avalanche** — Similar inputs (`agent-1:vn0`, `agent-1:vn1`) produce well-distributed hashes
- **Cross-process safe** — Unlike Rust's `DefaultHasher` (SipHash), which is randomly seeded per process

### Why Consistent Hashing?

| Scenario | Partitions moved |
|----------|-----------------|
| Add 1 agent to 3 | ~1/4 of partitions (only those that hash to new agent) |
| Remove 1 agent from 3 | ~1/3 of partitions (only the dead agent's) |
| No change | 0 partitions moved |

Compare this to modulo hashing (`partition % N`), where changing N reshuffles almost everything.

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
  │  3. For each (topic, partition):                │
  │     winner = consistent_hash(topic, part, agents)│
  │                                                 │
  │  4. Diff against current assignments:           │
  │     to_acquire = [should own, don't yet]        │
  │     to_release = [own now, shouldn't]           │
  │                                                 │
  │  5. Release old leases                          │
  │  6. Acquire new leases                          │
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
│          │     │     size > 1MB? OR age > 60s?                        │
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
│  Block 0 (LZ4 compressed)     │
│    record, record, record...  │
├────────────────────────────────┤
│  Block 1 (LZ4 compressed)     │
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

## What Happens When Things Change

### Agent Joins

```
t=0s:    2 agents own 6 partitions (3 each)
         agent-1: [0, 2, 4]    agent-2: [1, 3, 5]

t=1s:    agent-3 registers, starts heartbeating

t=30s:   Rebalance runs on all agents
         Consistent hash recalculates → each agent gets ~2

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
         Consistent hash redistributes:

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

## Component Summary

| Component | What it is | Coordination |
|-----------|-----------|--------------|
| **Topic** | Named event stream | Created atomically with partitions |
| **Partition** | Parallel shard of a topic | One leader at a time, high watermark tracking |
| **Agent** | Process that writes data to S3 | Heartbeat every 20s, 60s liveness timeout |
| **Lease** | Time-limited partition lock | 30s TTL, 10s renewal, epoch fencing |
| **Assigner** | Distributes partitions to agents | Consistent hash, 150 vnodes, rebalance every 30s |
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
| Segment max age | 60 seconds (configurable) |
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
