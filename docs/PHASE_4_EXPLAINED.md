# Phase 4: Multi-Agent Architecture - Explained Simply

**Created**: 2026-01-23
**For**: Understanding what we're building and why

---

## The Problem We're Solving

Right now, StreamHouse runs as a **single process**. If that process crashes, your entire system goes down. You can't scale horizontally because there's only one writer per topic.

```
Current State (Single Agent):
┌─────────────┐
│  Producer 1 │───┐
└─────────────┘   │
┌─────────────┐   │    ┌──────────────┐
│  Producer 2 │───┼───▶│  Agent-1     │──▶ PostgreSQL
└─────────────┘   │    │  (SPOF!)     │──▶ S3
┌─────────────┐   │    └──────────────┘
│  Producer 3 │───┘
└─────────────┘

Problem: If Agent-1 dies, EVERYTHING stops!
```

---

## What We're Building

**Multiple agents that coordinate** to handle traffic. Any agent can die, and the system keeps running.

```
Phase 4 (Multi-Agent):
┌─────────────┐
│  Producer 1 │───┐
└─────────────┘   │
┌─────────────┐   │    ┌──────────┐
│  Producer 2 │───┼───▶│ Agent-1  │────┐
└─────────────┘   │    └──────────┘    │
┌─────────────┐   │    ┌──────────┐    │
│  Producer 3 │───┼───▶│ Agent-2  │────┼──▶ PostgreSQL
└─────────────┘   │    └──────────┘    │    (Coordination)
┌─────────────┐   │    ┌──────────┐    │
│  Producer 4 │───┼───▶│ Agent-3  │────┘
└─────────────┘   │    └──────────┘
┌─────────────┐   │           │
│  Producer 5 │───┘           ▼
└─────────────┘              S3
                          (Storage)

Benefit: Agent-1 dies? No problem! Agent-2 and Agent-3 take over.
```

---

## Core Concepts

### 1. **Stateless Agents**

Each agent is **stateless** - all state lives in PostgreSQL + S3. This means:

```rust
// Agent dies
Agent-1: *crashes* 💥

// Data is safe!
PostgreSQL: Still has all metadata ✅
S3: Still has all segments ✅

// New agent starts
Agent-4: *boots up* 🚀
Agent-4: "What partitions need a leader?"
PostgreSQL: "orders/partition-0 has no leader"
Agent-4: "I'll take it!" (acquires lease)

// System keeps running
Producer: *writes to orders/partition-0*
Agent-4: "Got it!" (writes to S3)
```

**Why this matters**: You can kill any agent at any time. No "rebalancing", no "state migration", no complex recovery. Just boot a new agent and it figures out what to do.

---

### 2. **Partition Leadership**

Each partition has **one leader at a time**. The leader is the only agent allowed to write to that partition.

```
Topic: orders (3 partitions)
├─ Partition 0: Leader = Agent-1
├─ Partition 1: Leader = Agent-2
└─ Partition 2: Leader = Agent-3

Topic: user-events (2 partitions)
├─ Partition 0: Leader = Agent-1
└─ Partition 1: Leader = Agent-2
```

**How it works**:

```sql
-- partition_leases table
topic          | partition | leader    | expires_at  | epoch
---------------|-----------|-----------|-------------|------
orders         | 0         | agent-1   | T+30s       | 5
orders         | 1         | agent-2   | T+30s       | 3
user-events    | 0         | agent-1   | T+30s       | 2
```

When Agent-1 wants to write to `orders/partition-0`:
1. Check lease: "Do I own this partition?"
2. If yes → write
3. If no → reject (someone else owns it)
4. If expired → acquire lease, then write

---

### 3. **Lease-Based Coordination**

**Leases** are time-based locks. They automatically expire.

```
Timeline of a Lease:

T=0s: Agent-1 acquires lease for orders/partition-0
      └─ Lease valid until T=30s
      └─ Epoch = 1

T=20s: Agent-1 renews lease (heartbeat)
       └─ Lease extended to T=50s
       └─ Epoch = 2 (incremented)

T=25s: Agent-1 crashes! 💥
       └─ Lease still valid until T=50s
       └─ But heartbeat stops

T=50s: Lease expires automatically
       └─ orders/partition-0 has no leader

T=55s: Agent-2 notices expired lease
       └─ Acquires lease for orders/partition-0
       └─ Epoch = 3 (incremented)
       └─ Agent-2 is now the leader

T=60s: Agent-1 recovers (network restored)
       └─ Tries to write with epoch=2
       └─ PostgreSQL rejects (current epoch=3)
       └─ Agent-1: "Oh, I lost leadership"
       └─ Agent-1: Flushes buffers, releases partition
```

**Key points**:
- Leases expire automatically (default: 30 seconds)
- No infinite blocking - failed agents can't hold partitions forever
- Epoch prevents "zombie" agents from corrupting data

---

### 4. **Epoch Fencing**

**Problem**: What if Agent-1 thinks it's the leader, but it actually lost the lease?

```
Network Partition Scenario:

T=0s: Agent-1 has lease (epoch=1)
T=10s: Network partition! Agent-1 can't reach PostgreSQL
T=30s: Lease expires (Agent-1 doesn't know)
T=35s: Agent-2 acquires lease (epoch=2)
T=40s: Network restored
T=45s: Agent-1 tries to write (still thinks it has epoch=1)
       Agent-2 is writing (has epoch=2)

Without epoch fencing: Both agents write → data corruption! 💥
With epoch fencing: Agent-1's write rejected (stale epoch)
```

**How it works**:

```rust
// Agent-1 tries to write
agent1.write("orders", partition=0, epoch=1, data);

// PostgreSQL checks current lease
current_lease = get_lease("orders", 0);
if current_lease.epoch > 1 {
    return Error::StaleEpoch;  // Reject!
}

// Agent-1 realizes it lost leadership
agent1.release_partition("orders", 0);
```

---

## What We're Actually Building

### Phase 4.1: Agent Infrastructure (Weeks 1-2)

**The `Agent` struct**:

```rust
pub struct Agent {
    // Identity
    agent_id: String,              // "agent-us-east-1a-001"
    address: String,               // "10.0.1.5:9090"
    availability_zone: String,     // "us-east-1a"
    agent_group: String,           // "prod"

    // Connections
    metadata_store: Arc<dyn MetadataStore>,  // PostgreSQL
    object_store: Arc<dyn ObjectStore>,      // S3/MinIO

    // Components
    writer_pool: Arc<WriterPool>,            // Write to partitions
    lease_manager: Arc<LeaseManager>,        // Manage leases
    heartbeat_task: JoinHandle<()>,          // Keep agent alive
}
```

**What it does**:

1. **Registration**
   ```rust
   agent.start().await?;
   // Inserts into `agents` table:
   // agent_id='agent-1', address='10.0.1.5:9090', last_heartbeat=now()
   ```

2. **Heartbeat** (background task, every 20 seconds)
   ```rust
   loop {
       metadata_store.register_agent(agent_info).await?;
       // Updates last_heartbeat in database
       tokio::time::sleep(Duration::from_secs(20)).await;
   }
   ```

3. **Graceful Shutdown**
   ```rust
   agent.stop().await?;
   // 1. Stop accepting new writes
   // 2. Flush all buffered data
   // 3. Release all partition leases
   // 4. Deregister from agents table
   ```

---

### Phase 4.2: Partition Leadership (Weeks 3-4)

**The `LeaseManager` struct**:

```rust
pub struct LeaseManager {
    agent_id: String,
    metadata_store: Arc<dyn MetadataStore>,

    // Cache of leases we currently hold
    leases: Arc<RwLock<HashMap<(String, u32), PartitionLease>>>,

    // Background task to renew leases
    renewal_task: JoinHandle<()>,
}
```

**What it does**:

1. **Acquire Lease** (on first write to partition)
   ```rust
   async fn acquire_lease(&self, topic: &str, partition: u32) -> Result<()> {
       // Try to acquire lease in PostgreSQL
       let lease = self.metadata_store
           .acquire_partition_lease(topic, partition, self.agent_id, 30_000)
           .await?;

       // Cache it locally
       self.leases.write().await.insert((topic, partition), lease);

       Ok(())
   }
   ```

2. **Renew Leases** (background task, every 20 seconds)
   ```rust
   loop {
       let leases = self.leases.read().await.clone();

       for ((topic, partition), lease) in leases {
           // Extend lease expiration
           self.metadata_store
               .acquire_partition_lease(topic, partition, self.agent_id, 30_000)
               .await?;
       }

       tokio::time::sleep(Duration::from_secs(20)).await;
   }
   ```

3. **Check Lease** (before every write)
   ```rust
   async fn check_lease(&self, topic: &str, partition: u32) -> Result<bool> {
       // Do we have this lease cached?
       let leases = self.leases.read().await;

       if let Some(lease) = leases.get(&(topic, partition)) {
           // Is it still valid?
           if lease.lease_expires_at > now() {
               return Ok(true);
           }
       }

       Ok(false)
   }
   ```

---

### Phase 4.3: Multi-AZ Deployment (Week 5)

**AZ-Aware Partition Assignment**:

```rust
// Prefer local AZ for partition leadership
async fn acquire_partition(&self, topic: &str, partition: u32) -> Result<()> {
    // Get all agents
    let agents = self.metadata_store.list_agents().await?;

    // Filter by my AZ
    let local_agents = agents.iter()
        .filter(|a| a.availability_zone == self.availability_zone)
        .count();

    // If no local agents have this partition, I'll take it
    let lease = self.metadata_store
        .get_partition_lease(topic, partition)
        .await?;

    if lease.is_none() || lease.leader_agent_id.availability_zone != self.availability_zone {
        // Acquire lease
        self.lease_manager.acquire_lease(topic, partition).await?;
    }
}
```

**Why this matters**: Agents in us-east-1a lead partitions they write to. Producers in us-east-1a connect to those agents. Result: **Zero cross-AZ data transfer costs**.

---

### Phase 4.4: Agent Groups (Week 6)

**Network Isolation**:

```yaml
# prod-agents namespace
agent-1: agent_group="prod"
agent-2: agent_group="prod"

# staging-agents namespace
agent-3: agent_group="staging"
agent-4: agent_group="staging"

# Topics specify preferred group
orders: agent_group="prod"
test-orders: agent_group="staging"
```

```rust
async fn acquire_lease(&self, topic: &str, partition: u32) -> Result<()> {
    // Get topic config
    let topic_config = self.metadata_store.get_topic(topic).await?;

    // Check agent group affinity
    if let Some(preferred_group) = topic_config.agent_group {
        if preferred_group != self.agent_group {
            return Err(Error::WrongAgentGroup);
        }
    }

    // Acquire lease
    self.metadata_store
        .acquire_partition_lease(topic, partition, self.agent_id, 30_000)
        .await?;

    Ok(())
}
```

**Why this matters**: Production traffic isolated from staging. Multi-tenant SaaS with customer isolation.

---

### Phase 4.5: Testing & Validation (Weeks 7-8)

**Key scenarios to test**:

1. **Agent Failure During Write**
   ```
   1. Agent-1 writing to orders/partition-0
   2. Kill Agent-1 mid-write
   3. Agent-2 takes over
   4. Verify: No data loss, offsets sequential
   ```

2. **Network Partition**
   ```
   1. Agent-1 has lease (epoch=1)
   2. Network partition (Agent-1 isolated)
   3. Lease expires, Agent-2 acquires (epoch=2)
   4. Network restored
   5. Agent-1 tries to write with epoch=1
   6. Verify: Write rejected (stale epoch)
   ```

3. **Rolling Upgrade**
   ```
   1. 5 agents serving traffic
   2. Restart agents one at a time
   3. Verify: Zero downtime, no errors
   ```

---

## Real-World Example

Let's trace a write through the multi-agent system:

```
1. Producer sends record to orders/partition-0

2. Load balancer routes to Agent-2 (random)

3. Agent-2 receives write request
   └─ Check: "Do I have lease for orders/partition-0?"
   └─ Answer: No
   └─ Action: Try to acquire lease

4. Agent-2 queries PostgreSQL
   └─ SELECT * FROM partition_leases
       WHERE topic='orders' AND partition_id=0
   └─ Result: Lease held by Agent-1, expires in 15 seconds
   └─ Action: Reject write (LeaseHeldByOther)

5. Producer retries, routes to Agent-1

6. Agent-1 receives write request
   └─ Check: "Do I have lease for orders/partition-0?"
   └─ Answer: Yes (cached in LeaseManager)
   └─ Action: Write to partition

7. Agent-1 writes record
   └─ Buffer in memory (PartitionWriter)
   └─ Assign offset: 12345
   └─ Return success to producer

8. Background flush (30s later)
   └─ Compress buffered records with LZ4
   └─ Upload segment to S3
   └─ Register segment in PostgreSQL
   └─ Update high_watermark

9. Consumer reads records
   └─ Connects to any agent (Agent-3)
   └─ Agent-3: Query PostgreSQL for segment
   └─ Agent-3: Download from S3 (or hit cache)
   └─ Agent-3: Decompress and return records
```

**Key points**:
- Producers can connect to **any agent**
- If agent doesn't have lease, write fails (producer retries)
- If agent does have lease, write succeeds
- All agents share same PostgreSQL + S3, so reads work from any agent

---

## Benefits of Multi-Agent Architecture

### 1. **High Availability**

```
Before: Single agent dies = system down
After: Any agent dies = other agents take over
```

### 2. **Horizontal Scalability**

```
Traffic increases?
→ Add more agents (just boot new pods in K8s)
→ They automatically discover partitions needing leaders
→ Start serving traffic
```

### 3. **Zero-Downtime Deployments**

```
Rolling upgrade:
1. Stop Agent-1 (graceful shutdown, flushes data)
2. Agent-2 & Agent-3 take over Agent-1's partitions
3. Deploy new version of Agent-1
4. Start Agent-1 (acquires new partitions)
5. Repeat for Agent-2, Agent-3, ...
```

### 4. **Cost Optimization**

```
Kubernetes auto-scaling:
- Low traffic: 6 agents (save money)
- High traffic: 20 agents (handle load)
- Use spot instances (80% cost savings)
- No "rebalancing" needed (agents are stateless)
```

### 5. **Multi-AZ Deployment**

```
us-east-1a: Agent-1, Agent-2 (lead local partitions)
us-east-1b: Agent-3, Agent-4 (lead local partitions)
us-east-1c: Agent-5, Agent-6 (lead local partitions)

Benefit: Zero cross-AZ data transfer costs (saves $$$)
```

---

## Comparison: Before vs After

| Feature | Single Agent (Now) | Multi-Agent (Phase 4) |
|---------|-------------------|----------------------|
| High Availability | ❌ SPOF | ✅ Any agent can die |
| Horizontal Scaling | ❌ Can't add agents | ✅ Add/remove anytime |
| Zero Downtime | ❌ Restart = downtime | ✅ Rolling updates |
| Multi-AZ | ❌ Single AZ | ✅ Spread across 3 AZs |
| Auto-Scaling | ❌ Manual | ✅ HPA in K8s |
| Cost Optimization | ❌ Fixed capacity | ✅ Scale up/down |
| Prod/Staging Isolation | ❌ Shared agent | ✅ Agent groups |

---

## What Makes This "Simple"?

Compared to Kafka's architecture:

**Kafka**:
- ZooKeeper for coordination (separate cluster!)
- Controller election (complex Raft protocol)
- Partition replicas (multiple copies of data)
- ISR management (in-sync replicas)
- Rebalancing on broker failure (minutes)

**StreamHouse**:
- PostgreSQL for coordination (already have it!)
- Lease-based leadership (simple time-based locks)
- S3 for storage (single copy, highly durable)
- No ISR (S3 handles durability)
- No rebalancing (agents stateless, just acquire new leases)

**Result**: 10x simpler architecture, same reliability.

---

## Summary

**Phase 4 builds**:

1. **Agent struct** - Lifecycle management (start, stop, heartbeat)
2. **LeaseManager** - Partition leadership with epoch fencing
3. **Multi-AZ support** - Zero cross-AZ costs
4. **Agent groups** - Network isolation for multi-tenancy
5. **Testing** - Comprehensive failure scenario validation

**End result**: Production-ready distributed system that can:
- Scale horizontally (add agents anytime)
- Survive failures (any agent can die)
- Deploy without downtime (rolling updates)
- Run multi-tenant workloads (agent groups)
- Optimize costs (auto-scaling, spot instances)

This achieves **WarpStream feature parity** for the core transport layer! 🎯
