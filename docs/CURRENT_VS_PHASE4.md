# Current System vs Phase 4 - How It Works

**Created**: 2026-01-23
**Purpose**: Explain how the system works TODAY and how Phase 4 changes it

---

## TL;DR

**Current State**: The tables exist, but they're **empty and unused**. Your demo writes directly to partitions without any coordination because there's only one writer.

**Phase 4**: Multiple agents will **use these tables** to coordinate who writes to which partition.

---

## Current System Architecture

### What You Have Now

```
Your Demo Script (demo.sh):
├─ Runs: cargo run --example write_with_real_api
│
└─ This creates:
   ├─ PartitionWriter (in-memory, single process)
   ├─ Writes to SQLite metadata store
   └─ Uploads segments to MinIO

PostgreSQL Tables:
├─ agents: EMPTY (0 rows) ❌
├─ partition_leases: EMPTY (0 rows) ❌
├─ topics: HAS DATA ✅
├─ partitions: HAS DATA ✅
└─ segments: HAS DATA ✅
```

### How Data Gets Written Today

Let's trace a write through your current system:

```rust
// demo.sh runs this:
cargo run --example write_with_real_api

// Inside write_with_real_api.rs:

// 1. Connect to metadata store (SQLite, not PostgreSQL!)
let metadata = SqliteMetadataStore::new("/tmp/streamhouse_pipeline.db").await?;

// 2. Create topics (writes to SQLite)
metadata.create_topic(TopicConfig {
    name: "orders",
    partition_count: 3,
    ...
}).await?;

// 3. Create PartitionWriter (NO lease acquisition!)
let writer = PartitionWriter::new(
    "orders",
    partition_id,
    cache,
    object_store,
    metadata,
    write_config,
).await?;

// 4. Write records (buffers in memory)
writer.append(Some(key), value, timestamp).await?;

// 5. Flush to S3 (on shutdown or every 30s)
writer.flush().await?;
//    └─ Compresses records with LZ4
//    └─ Uploads segment to MinIO
//    └─ Registers segment in SQLite

// 6. demo.sh syncs SQLite → PostgreSQL
sqlite3 /tmp/streamhouse_pipeline.db "SELECT ..." | while read ...; do
    docker exec streamhouse-postgres psql ...
done
```

**Key points**:
- ✅ No agent registration (no row in `agents` table)
- ✅ No lease acquisition (no row in `partition_leases` table)
- ✅ Writes directly to partition (single process, no coordination needed)
- ✅ Uses SQLite, then syncs to PostgreSQL afterward

---

## Why The Tables Exist But Are Empty

The `agents` and `partition_leases` tables were created in **Phase 3.1** to prepare for Phase 4:

```sql
-- Phase 3.1 migration (already applied)
CREATE TABLE agents (...);
CREATE TABLE partition_leases (...);
```

**Why add them early?**
- Avoid future schema migrations
- Test PostgreSQL setup now
- No runtime overhead if unused

**Current usage**: 0% (empty tables)

---

## How Phase 4 Will Use These Tables

### The Transformation

```
Before (Single Process):
┌────────────────────────────────────┐
│  write_with_real_api.rs            │
│                                    │
│  PartitionWriter::new()            │
│  └─ No coordination                │
│  └─ Just writes to partition       │
│                                    │
│  writer.append(...)                │
│  writer.flush()                    │
│  └─ Upload to MinIO                │
│  └─ Register in SQLite             │
└────────────────────────────────────┘

After (Multi-Agent):
┌────────────────────────────────────┐
│  Agent-1                           │
│                                    │
│  1. Register in agents table       │
│     INSERT INTO agents VALUES      │
│     ('agent-1', '10.0.1.5', ...)   │
│                                    │
│  2. Start heartbeat task           │
│     UPDATE agents                  │
│     SET last_heartbeat = now()     │
│     WHERE agent_id = 'agent-1'     │
│                                    │
│  3. Acquire lease for partition    │
│     INSERT INTO partition_leases   │
│     VALUES ('orders', 0, 'agent-1',│
│             now()+30s, 1)          │
│                                    │
│  4. NOW can write to partition     │
│     writer.append(...)             │
│     writer.flush()                 │
└────────────────────────────────────┘
```

---

## Database State: Before vs After

### Current State (After Running demo.sh)

```sql
-- agents table: EMPTY
postgres=# SELECT * FROM agents;
 agent_id | address | availability_zone | agent_group | last_heartbeat | started_at | metadata
----------+---------+-------------------+-------------+----------------+------------+----------
(0 rows)

-- partition_leases table: EMPTY
postgres=# SELECT * FROM partition_leases;
 topic | partition_id | leader_agent_id | lease_expires_at | acquired_at | epoch
-------+--------------+-----------------+------------------+-------------+-------
(0 rows)

-- partitions table: HAS DATA (but no leases!)
postgres=# SELECT topic, partition_id, high_watermark FROM partitions;
    topic    | partition_id | high_watermark
-------------+--------------+----------------
 orders      |            0 |            100
 orders      |            1 |            150
 orders      |            2 |             80
 user-events |            0 |            200
 user-events |            1 |            120
 metrics     |            0 |            250
 metrics     |            1 |            250
 metrics     |            2 |            250
 metrics     |            3 |            250
(9 rows)
```

**Notice**: Partitions exist, but no one "owns" them (no leases).

---

### Phase 4 State (With 3 Agents Running)

```sql
-- agents table: 3 agents registered
postgres=# SELECT agent_id, address, availability_zone, last_heartbeat FROM agents;
     agent_id      |    address    | availability_zone |  last_heartbeat
-------------------+---------------+-------------------+------------------
 agent-us-east-1a  | 10.0.1.5:9090 | us-east-1a        | 1769188800123
 agent-us-east-1b  | 10.0.2.5:9090 | us-east-1b        | 1769188801456
 agent-us-east-1c  | 10.0.3.5:9090 | us-east-1c        | 1769188799876
(3 rows)

-- partition_leases table: Each partition has a leader
postgres=# SELECT topic, partition_id, leader_agent_id,
                  lease_expires_at - extract(epoch from now())*1000 as ttl_ms,
                  epoch
           FROM partition_leases;
    topic    | partition_id |  leader_agent_id  | ttl_ms | epoch
-------------+--------------+-------------------+--------+-------
 orders      |            0 | agent-us-east-1a  |  25432 |    15
 orders      |            1 | agent-us-east-1b  |  27891 |     8
 orders      |            2 | agent-us-east-1c  |  23105 |    12
 user-events |            0 | agent-us-east-1a  |  26543 |     5
 user-events |            1 | agent-us-east-1b  |  24876 |     7
 metrics     |            0 | agent-us-east-1a  |  22987 |    20
 metrics     |            1 | agent-us-east-1b  |  28654 |    18
 metrics     |            2 | agent-us-east-1c  |  25432 |    22
 metrics     |            3 | agent-us-east-1a  |  27123 |    19
(9 rows)

-- partitions table: SAME (unchanged)
postgres=# SELECT topic, partition_id, high_watermark FROM partitions;
    topic    | partition_id | high_watermark
-------------+--------------+----------------
 orders      |            0 |            100
 orders      |            1 |            150
 orders      |            2 |             80
 user-events |            0 |            200
 user-events |            1 |            120
 metrics     |            0 |            250
 metrics     |            1 |            250
 metrics     |            2 |            250
 metrics     |            3 |            250
(9 rows)
```

**Notice**:
- 3 agents are alive (heartbeats updating)
- Each partition has a leader agent
- Leases expire in ~25 seconds (will be renewed)
- Epochs increment on each renewal

---

## How The Coordination Works

### Scenario: Agent-1 Wants to Write

```rust
// Producer sends record to orders/partition-0
let record = Record {
    topic: "orders",
    partition: 0,
    key: "order-123",
    value: b"{}",
};

// Agent-1 receives the write request
agent1.handle_write(record).await?;
```

**Step-by-step flow**:

```rust
// 1. Check if we have lease
let lease = self.lease_manager.get_lease("orders", 0)?;

if lease.is_none() || lease.is_expired() {
    // 2. Try to acquire lease
    let result = self.metadata_store
        .acquire_partition_lease(
            "orders",
            0,
            "agent-1",
            30_000  // 30 second lease
        )
        .await;

    match result {
        Ok(new_lease) => {
            // Success! We got the lease
            self.lease_manager.cache_lease(new_lease);
        }
        Err(Error::LeaseHeldByOther(other_agent)) => {
            // Someone else has it
            return Err(Error::NotLeader {
                partition: 0,
                current_leader: other_agent,
            });
        }
    }
}

// 3. Now we have lease, write to partition
self.partition_writer.append(record).await?;
```

**What happens in PostgreSQL**:

```sql
-- Check current lease
SELECT * FROM partition_leases
WHERE topic = 'orders' AND partition_id = 0;

-- If no lease or expired, acquire it
INSERT INTO partition_leases (
    topic, partition_id, leader_agent_id,
    lease_expires_at, acquired_at, epoch
)
VALUES (
    'orders', 0, 'agent-1',
    extract(epoch from now())*1000 + 30000,  -- expires in 30s
    extract(epoch from now())*1000,
    1
)
ON CONFLICT (topic, partition_id)
DO UPDATE SET
    leader_agent_id = EXCLUDED.leader_agent_id,
    lease_expires_at = EXCLUDED.lease_expires_at,
    epoch = partition_leases.epoch + 1  -- increment epoch!
WHERE partition_leases.lease_expires_at < extract(epoch from now())*1000;
-- Only update if current lease is expired!
```

---

## The Heartbeat Mechanism

### How Agents Stay Alive

**Background task** (runs every 20 seconds):

```rust
// In Agent::start()
let heartbeat_task = tokio::spawn(async move {
    loop {
        // Update last_heartbeat
        metadata_store.register_agent(AgentInfo {
            agent_id: self.agent_id.clone(),
            address: self.address.clone(),
            availability_zone: self.availability_zone.clone(),
            agent_group: self.agent_group.clone(),
            last_heartbeat: current_timestamp(),
            started_at: self.started_at,
            metadata: serde_json::json!({}),
        }).await?;

        tokio::time::sleep(Duration::from_secs(20)).await;
    }
});
```

**SQL executed every 20 seconds**:

```sql
-- Upsert agent record
INSERT INTO agents (
    agent_id, address, availability_zone, agent_group,
    last_heartbeat, started_at, metadata
)
VALUES (
    'agent-1', '10.0.1.5:9090', 'us-east-1a', 'prod',
    1769188820000,  -- now()
    1769188800000,  -- agent start time
    '{}'::jsonb
)
ON CONFLICT (agent_id)
DO UPDATE SET
    last_heartbeat = EXCLUDED.last_heartbeat;
```

**Stale agent detection**:

```sql
-- List alive agents (heartbeat within 60 seconds)
SELECT * FROM agents
WHERE last_heartbeat > extract(epoch from now())*1000 - 60000;
```

If Agent-1 crashes, its heartbeat stops. After 60 seconds, it's considered dead and won't appear in `list_agents()`.

---

## The Lease Renewal Mechanism

### How Agents Keep Partitions

**Background task** (runs every 20 seconds):

```rust
// In LeaseManager::start()
let renewal_task = tokio::spawn(async move {
    loop {
        let leases = self.leases.read().await.clone();

        for ((topic, partition), lease) in leases {
            // Renew lease (extend expiration + increment epoch)
            self.metadata_store
                .acquire_partition_lease(
                    &topic,
                    partition,
                    &self.agent_id,
                    30_000  // extend by 30s
                )
                .await?;
        }

        tokio::time::sleep(Duration::from_secs(20)).await;
    }
});
```

**SQL executed every 20 seconds per partition**:

```sql
-- Extend lease expiration, increment epoch
UPDATE partition_leases
SET
    lease_expires_at = extract(epoch from now())*1000 + 30000,
    epoch = epoch + 1
WHERE topic = 'orders'
  AND partition_id = 0
  AND leader_agent_id = 'agent-1';
```

**Timeline example**:

```
T=0s:   Acquire lease (epoch=1, expires T=30s)
T=20s:  Renew lease   (epoch=2, expires T=50s)
T=40s:  Renew lease   (epoch=3, expires T=70s)
T=60s:  Renew lease   (epoch=4, expires T=90s)
...
```

If Agent-1 crashes at T=45s:
- Last renewal was at T=40s (epoch=3, expires T=70s)
- Lease still valid until T=70s
- At T=70s, lease expires
- Another agent can acquire it

---

## Comparison Table

| Aspect | Current (Single Process) | Phase 4 (Multi-Agent) |
|--------|-------------------------|----------------------|
| **Agent Registration** | None | Row in `agents` table |
| **Partition Ownership** | Implicit (only one writer) | Explicit (row in `partition_leases`) |
| **Coordination** | None needed | Lease-based via PostgreSQL |
| **Failure Handling** | Process dies → system down | Agent dies → other agents take over |
| **High Availability** | None (SPOF) | Yes (any agent can fail) |
| **Metadata Store** | SQLite (local file) | PostgreSQL (shared state) |
| **Heartbeat** | None | Every 20s (updates `agents` table) |
| **Lease Renewal** | None | Every 20s (updates `partition_leases`) |
| **Epoch Fencing** | Not needed | Yes (prevents split-brain) |
| **Horizontal Scaling** | Impossible | Trivial (just add agents) |

---

## Code Comparison

### Current: Single Process Write

```rust
// write_with_real_api.rs (CURRENT)

// No agent registration
// No lease acquisition

let writer = PartitionWriter::new(
    topic,
    partition,
    cache,
    object_store,
    metadata,
    config,
).await?;

// Just write directly
writer.append(key, value, timestamp).await?;
```

### Phase 4: Multi-Agent Write

```rust
// agent.rs (PHASE 4)

// 1. Register agent
self.metadata_store.register_agent(agent_info).await?;
// → INSERT INTO agents VALUES (...)

// 2. Start heartbeat task
self.start_heartbeat().await?;
// → UPDATE agents SET last_heartbeat = now() (every 20s)

// 3. Acquire lease before writing
self.lease_manager.acquire_lease(topic, partition).await?;
// → INSERT INTO partition_leases VALUES (...)
// → If conflict, another agent has it!

// 4. Write to partition (same as before)
let writer = self.writer_pool.get_writer(topic, partition)?;
writer.append(key, value, timestamp).await?;
```

---

## Summary

### Current System
- ✅ Tables exist: `agents`, `partition_leases`
- ❌ Tables are empty: No agents registered, no leases held
- ✅ Writes work: Single process, no coordination needed
- ❌ No HA: Process dies = system down

### Phase 4 System
- ✅ Tables are used: Agents register, leases acquired
- ✅ Multiple processes: 3+ agents running
- ✅ Coordination: PostgreSQL enforces one leader per partition
- ✅ High Availability: Any agent can die, others take over

### The Bridge

Phase 4 doesn't change the data model - it just **uses the tables that are already there**:

```
Phase 3.1: Created tables (agents, partition_leases)
           └─ Ready but unused

Phase 4.1: Implement Agent struct
           └─ Writes to agents table

Phase 4.2: Implement LeaseManager
           └─ Writes to partition_leases table
```

**No schema migrations needed!** The foundation was laid in Phase 3.1. Phase 4 just builds the agents that use it.

---

## Next Step: Try It Yourself

Want to see the tables in action? You can manually test the coordination APIs:

```bash
# Start psql
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata

# Manually register an agent
INSERT INTO agents (agent_id, address, availability_zone, agent_group, last_heartbeat, started_at, metadata)
VALUES ('test-agent-1', '127.0.0.1:9090', 'local', 'test', extract(epoch from now())*1000, extract(epoch from now())*1000, '{}'::jsonb);

# Acquire a lease
INSERT INTO partition_leases (topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch)
VALUES ('orders', 0, 'test-agent-1', extract(epoch from now())*1000 + 30000, extract(epoch from now())*1000, 1);

# Check it
SELECT * FROM agents;
SELECT * FROM partition_leases;

# Wait 30 seconds, try to acquire again (should increment epoch)
-- Wait...

# Try to acquire again (lease expired)
INSERT INTO partition_leases (topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch)
VALUES ('orders', 0, 'test-agent-2', extract(epoch from now())*1000 + 30000, extract(epoch from now())*1000, 1)
ON CONFLICT (topic, partition_id)
DO UPDATE SET
    leader_agent_id = EXCLUDED.leader_agent_id,
    lease_expires_at = EXCLUDED.lease_expires_at,
    epoch = partition_leases.epoch + 1
WHERE partition_leases.lease_expires_at < extract(epoch from now())*1000;

# Check epoch incremented
SELECT topic, partition_id, leader_agent_id, epoch FROM partition_leases;
```

This is exactly what Phase 4 agents will do automatically! 🚀
