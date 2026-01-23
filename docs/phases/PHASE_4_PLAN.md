# Phase 4: Multi-Agent Architecture - Implementation Plan

**Created**: 2026-01-23
**Status**: 📋 Ready to Start
**Goal**: Enable multiple stateless StreamHouse agents to coordinate writes and reads across partitions
**Duration**: 6-8 weeks (Phases 4.1-4.5)

---

## Executive Summary

Phase 4 transforms StreamHouse from a single-agent system to a **distributed multi-agent architecture** like WarpStream. This enables:

- ✅ **Horizontal scalability** - Run multiple agents for high availability
- ✅ **Zero downtime** - Agent failures don't affect availability
- ✅ **Multi-AZ deployment** - Agents in different availability zones
- ✅ **Agent groups** - Network-isolated agent pools for multi-tenancy
- ✅ **Auto-scaling** - Add/remove agents without rebalancing

### What We're Building

```
┌──────────────────────────────────────────────────────────────┐
│                    Phase 4 Architecture                       │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  Producers/Consumers                                          │
│        │                                                      │
│        ├─────────┬─────────┬─────────┬─────────┐            │
│        │         │         │         │         │            │
│        ▼         ▼         ▼         ▼         ▼            │
│    ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐            │
│    │Agt-1│  │Agt-2│  │Agt-3│  │Agt-4│  │Agt-5│            │
│    │us-e1│  │us-e1│  │us-e2│  │us-e2│  │us-w1│            │
│    └──┬──┘  └──┬──┘  └──┬──┘  └──┬──┘  └──┬──┘            │
│       │        │        │        │        │                │
│       └────────┴────────┴────────┴────────┘                │
│                        │                                    │
│                        ▼                                    │
│              ┌──────────────────┐                           │
│              │ PostgreSQL       │                           │
│              │ Metadata Store   │                           │
│              │                  │                           │
│              │ • partition_leases                          │
│              │ • agents                                     │
│              └──────────────────┘                           │
│                        │                                    │
│                        ▼                                    │
│              ┌──────────────────┐                           │
│              │   MinIO / S3     │                           │
│              │  Segment Storage │                           │
│              └──────────────────┘                           │
│                                                               │
│  Features:                                                   │
│  • Any agent can lead any partition                         │
│  • Lease-based coordination (no Raft/Paxos)               │
│  • Automatic failover (30s lease timeout)                  │
│  • Epoch fencing prevents split-brain                       │
│  • AZ-aware routing (zero inter-AZ costs)                  │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

### Foundation Already Built (Phase 3.1)

Phase 3.1 laid the groundwork:
- ✅ `MetadataStore` trait with 8 agent coordination methods
- ✅ `agents` table for registration and heartbeats
- ✅ `partition_leases` table with epoch fencing
- ✅ SQLite + PostgreSQL implementations complete

Phase 4 builds the actual agent infrastructure on top of this foundation.

---

## Phase 4 Breakdown

### Phase 4.1: Agent Infrastructure (Weeks 1-2)

**Goal**: Create the core `Agent` struct and lifecycle management

#### Deliverables

1. **Agent Struct** (`crates/streamhouse-agent/src/lib.rs`)
   ```rust
   pub struct Agent {
       agent_id: String,
       address: String,  // e.g., "10.0.1.5:9090"
       availability_zone: String,
       agent_group: String,
       metadata_store: Arc<dyn MetadataStore>,
       object_store: Arc<dyn ObjectStore>,
       writer_pool: Arc<WriterPool>,
       lease_manager: Arc<LeaseManager>,
       heartbeat_task: JoinHandle<()>,
   }
   ```

2. **Agent Lifecycle**
   - `Agent::start()` - Register agent, start heartbeat
   - `Agent::stop()` - Deregister, release leases, flush buffers
   - `Agent::health_check()` - Return liveness status

3. **Heartbeat Mechanism**
   - Background task updates `last_heartbeat` every 20s
   - Metadata store filters agents with heartbeat > 60s ago
   - Graceful shutdown flushes all data before deregistering

4. **Agent Discovery**
   - `list_agents()` returns all live agents
   - Clients can connect to any agent
   - No single point of failure

#### Example Usage

```rust
let agent = Agent::builder()
    .agent_id("agent-us-east-1a-001")
    .address("10.0.1.5:9090")
    .availability_zone("us-east-1a")
    .agent_group("prod")
    .metadata_store(postgres_store)
    .object_store(s3_store)
    .build()
    .await?;

agent.start().await?;  // Register + start heartbeat

// ... serve traffic ...

agent.stop().await?;   // Flush + deregister
```

#### Success Criteria

- ✅ Multiple agents can register simultaneously
- ✅ Heartbeat keeps agents alive
- ✅ Stale agents (>60s) filtered out
- ✅ Graceful shutdown flushes all data
- ✅ No data loss on agent restart

---

### Phase 4.2: Partition Leadership (Weeks 3-4)

**Goal**: Implement lease-based partition leadership with epoch fencing

#### Deliverables

1. **LeaseManager Struct** (`crates/streamhouse-agent/src/lease_manager.rs`)
   ```rust
   pub struct LeaseManager {
       agent_id: String,
       metadata_store: Arc<dyn MetadataStore>,
       leases: Arc<RwLock<HashMap<(String, u32), PartitionLease>>>,
       renewal_task: JoinHandle<()>,
   }
   ```

2. **Lease Acquisition**
   - Try to acquire lease on first write to partition
   - If lease held by another agent, reject write
   - If lease expired, acquire with incremented epoch
   - CAS operation prevents race conditions

3. **Lease Renewal**
   - Background task renews leases every 20s
   - Extends `lease_expires_at` by 30s
   - Increments `epoch` on each renewal
   - Lost lease = agent must flush and release partition

4. **Epoch Fencing**
   - Writers include `epoch` with every write
   - Metadata store rejects writes with stale epochs
   - Prevents zombie agents from corrupting data

#### Leadership Flow

```
Time: T=0
  Agent-1: Write to orders/partition-0
  → Check lease: None exists
  → acquire_partition_lease("orders", 0, duration=30s)
  → PostgreSQL: INSERT INTO partition_leases
                 (topic, partition_id, leader_agent_id, epoch)
                 VALUES ('orders', 0, 'agent-1', 1)
  → Lease granted! epoch=1

Time: T=15s
  Agent-1: Renewal task runs
  → renew_lease("orders", 0)
  → PostgreSQL: UPDATE partition_leases
                 SET lease_expires_at = now() + 30s,
                     epoch = epoch + 1
                 WHERE topic='orders' AND partition_id=0
                   AND leader_agent_id='agent-1'
  → Lease renewed! epoch=2

Time: T=20s (Agent-1 crashes!)
  Agent-1: Network partition, heartbeat stops
  Lease expires at T=45s

Time: T=50s
  Agent-2: Write to orders/partition-0
  → Check lease: Exists but expired (T=45s < T=50s)
  → acquire_partition_lease("orders", 0, duration=30s)
  → PostgreSQL: UPDATE partition_leases
                 SET leader_agent_id='agent-2',
                     lease_expires_at=now()+30s,
                     epoch=3
                 WHERE topic='orders' AND partition_id=0
  → Lease granted! epoch=3

Time: T=60s (Agent-1 recovers!)
  Agent-1: Tries to write with epoch=2
  → Metadata store checks: current epoch=3
  → Write REJECTED (stale epoch)
  → Agent-1: Release partition, flush buffers
```

#### Success Criteria

- ✅ Only one agent can lead each partition
- ✅ Automatic failover on agent death (< 30s)
- ✅ Epoch fencing prevents split-brain
- ✅ No data loss during leadership transfer
- ✅ CAS prevents race conditions

---

### Phase 4.3: Multi-AZ Deployment (Week 5)

**Goal**: Enable agents across multiple availability zones with zero inter-AZ costs

#### Deliverables

1. **AZ-Aware Partition Assignment**
   - Prefer local AZ for partition leadership
   - Only acquire remote partitions if local AZ unavailable
   - Reduces cross-AZ data transfer

2. **Client Routing**
   - Clients discover agents via metadata store
   - Connect to agents in same AZ when possible
   - Fallback to other AZs if needed

3. **S3 Multi-AZ Access**
   - Each agent writes to S3 from its AZ
   - S3 replicates automatically (no inter-AZ cost for replication)
   - Reads from S3 stay within AZ

#### Example Deployment

```yaml
# us-east-1a
agents:
  - agent-1: 10.0.1.5:9090
  - agent-2: 10.0.1.6:9090

# us-east-1b
agents:
  - agent-3: 10.0.2.5:9090
  - agent-4: 10.0.2.6:9090

# Partition assignment (example)
orders/partition-0 → agent-1 (us-east-1a)
orders/partition-1 → agent-3 (us-east-1b)
orders/partition-2 → agent-2 (us-east-1a)
```

#### Cost Savings

**Before Multi-AZ**:
- All traffic routes through single AZ
- Cross-AZ reads = $0.01/GB

**After Multi-AZ**:
- Clients connect to local AZ agent
- S3 writes stay within AZ
- Cross-AZ costs → $0/GB (eliminated)

**Savings**: ~80% of networking costs

#### Success Criteria

- ✅ Agents in multiple AZs coordinate correctly
- ✅ Cross-AZ data transfer minimized
- ✅ Clients prefer local-AZ agents
- ✅ Failover works across AZs
- ✅ No single-AZ dependency

---

### Phase 4.4: Agent Groups (Week 6)

**Goal**: Network-isolated agent pools for multi-tenancy and security

#### Deliverables

1. **Agent Group Configuration**
   ```yaml
   agent_groups:
     prod:
       vpc: vpc-prod-123
       subnets: [subnet-a, subnet-b]
       security_group: sg-prod

     staging:
       vpc: vpc-staging-456
       subnets: [subnet-c, subnet-d]
       security_group: sg-staging
   ```

2. **Topic→Group Affinity**
   - Topics can specify preferred agent group
   - Prevents prod/staging cross-contamination
   - Enforced at partition lease acquisition

3. **Isolation Guarantees**
   - Agents in different groups can't share partitions
   - Different VPCs = network isolation
   - Separate metadata if needed (virtual clusters)

#### Use Cases

1. **Prod/Staging Separation**
   ```
   prod-agents:    Handle orders, payments, users
   staging-agents: Handle test data
   ```

2. **Multi-Region**
   ```
   us-east-agents: Serve US customers
   eu-west-agents: Serve EU customers (GDPR compliance)
   ```

3. **SaaS Multi-Tenancy**
   ```
   tenant-A-agents: Customer A's data
   tenant-B-agents: Customer B's data
   ```

#### Success Criteria

- ✅ Agent groups provide hard network isolation
- ✅ Topics can specify preferred group
- ✅ No cross-group partition sharing
- ✅ Supports multi-region deployments
- ✅ Enables SaaS multi-tenancy

---

### Phase 4.5: Testing & Validation (Weeks 7-8)

**Goal**: Comprehensive testing of multi-agent scenarios

#### Test Scenarios

1. **Agent Failure During Write**
   ```rust
   // Start 3 agents
   let agent1 = start_agent("agent-1").await?;
   let agent2 = start_agent("agent-2").await?;
   let agent3 = start_agent("agent-3").await?;

   // Agent-1 acquires lease for partition-0
   produce_to(agent1, "orders", 0, records).await?;

   // Kill agent-1 mid-write
   agent1.kill().await?;

   // Agent-2 takes over (should succeed)
   produce_to(agent2, "orders", 0, more_records).await?;

   // Verify: No data loss, offsets sequential
   assert_offsets_sequential(consumer.read_all().await?);
   ```

2. **Agent Failure During Read**
   ```rust
   let agent1 = start_agent("agent-1").await?;
   let agent2 = start_agent("agent-2").await?;

   // Agent-1 serves read
   let stream = consume_from(agent1, "orders", 0, offset=0).await?;

   // Kill agent-1 mid-stream
   agent1.kill().await?;

   // Client reconnects to agent-2 (should succeed)
   let stream2 = consume_from(agent2, "orders", 0, offset=50).await?;

   // Verify: No duplicate records
   assert_no_duplicates(stream.chain(stream2).await?);
   ```

3. **Network Partition**
   ```rust
   let agent1 = start_agent("agent-1").await?;
   let agent2 = start_agent("agent-2").await?;

   // Agent-1 leads partition-0
   produce_to(agent1, "orders", 0, records).await?;

   // Simulate network partition (agent-1 can't reach metadata store)
   firewall_block(agent1, postgres_host).await?;

   // Agent-1 lease expires
   tokio::time::sleep(Duration::from_secs(31)).await;

   // Agent-2 acquires lease
   produce_to(agent2, "orders", 0, more_records).await?;

   // Agent-1 recovers
   firewall_unblock(agent1, postgres_host).await?;

   // Agent-1 tries to write (should be rejected - stale epoch)
   let result = produce_to(agent1, "orders", 0, bad_records).await;
   assert!(result.is_err());  // Stale epoch
   ```

4. **Rolling Upgrade**
   ```rust
   // Start 5 agents
   let agents = start_agents(5).await?;

   // All agents serving traffic
   for i in 0..1000 {
       produce_to(agents[i % 5], "orders", i % 10, records).await?;
   }

   // Rolling restart (one at a time)
   for agent in agents {
       agent.stop().await?;  // Graceful shutdown
       let new_agent = start_agent_v2(agent.id()).await?;
       tokio::time::sleep(Duration::from_secs(5)).await;
   }

   // Verify: Zero downtime, no data loss
   assert_no_errors(consumer.read_all().await?);
   ```

5. **Thundering Herd**
   ```rust
   // Start 50 agents simultaneously
   let agents = join_all((0..50).map(|i| {
       start_agent(format!("agent-{}", i))
   })).await?;

   // All try to acquire same partition
   let results = join_all(agents.iter().map(|agent| {
       produce_to(agent, "orders", 0, records)
   })).await;

   // Verify: Only one succeeds, others get LeaseHeldByOther error
   assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
   ```

#### Performance Testing

1. **Throughput**
   - Target: Same as single-agent (no regression)
   - Measure: Write/read RPS per agent
   - Scale: Linear with number of agents

2. **Latency**
   - Target: p99 < 50ms (including lease check)
   - Measure: Write latency distribution
   - Monitor: Lease acquisition overhead

3. **Metadata Load**
   - Target: < 100 QPS to PostgreSQL
   - Measure: Lease renewals + heartbeats
   - Optimize: Batching, caching

#### Success Criteria

- ✅ All failure scenarios handled gracefully
- ✅ Zero data loss on agent failures
- ✅ No split-brain scenarios
- ✅ Rolling upgrades work without downtime
- ✅ Performance scales linearly with agents
- ✅ Metadata store not overloaded

---

## Implementation Roadmap

### Week 1-2: Agent Infrastructure
- [ ] Create `streamhouse-agent` crate
- [ ] Implement `Agent` struct
- [ ] Heartbeat mechanism
- [ ] Agent registration/deregistration
- [ ] Health check endpoints
- [ ] Integration tests

### Week 3-4: Partition Leadership
- [ ] Implement `LeaseManager`
- [ ] Lease acquisition logic
- [ ] Lease renewal background task
- [ ] Epoch fencing
- [ ] Leadership transfer
- [ ] Split-brain prevention tests

### Week 5: Multi-AZ Deployment
- [ ] AZ-aware partition assignment
- [ ] Client routing logic
- [ ] S3 multi-AZ configuration
- [ ] Cost optimization metrics
- [ ] Multi-AZ integration tests

### Week 6: Agent Groups
- [ ] Agent group configuration
- [ ] Topic→group affinity
- [ ] Network isolation validation
- [ ] Multi-tenant example deployment
- [ ] Security tests

### Week 7-8: Testing & Validation
- [ ] Agent failure tests (write/read)
- [ ] Network partition simulation
- [ ] Rolling upgrade tests
- [ ] Thundering herd tests
- [ ] Performance benchmarks
- [ ] Documentation

---

## Architecture Decisions

### 1. Lease-Based Leadership vs. Raft

**Decision**: Use lease-based leadership with PostgreSQL

**Rationale**:
- ✅ Simpler implementation (no Raft library needed)
- ✅ PostgreSQL already provides consistency
- ✅ Fast failover (lease timeout = 30s)
- ✅ Stateless agents (no replicated log)
- ❌ Brief unavailability during failover (acceptable)

**Alternative**: Raft consensus
- Would add complexity (replicated log, leader election)
- Overkill for partition leadership
- Still need metadata store for segment pointers

### 2. Heartbeat Interval: 20s, Timeout: 60s

**Decision**: 20s heartbeat interval, 60s timeout

**Rationale**:
- 20s interval = 3 heartbeats per timeout window
- Handles transient failures (GC pauses, network blips)
- Fast enough for production failover
- Low metadata store load (N agents × 3 QPS)

**Alternative**: Faster (5s/15s)
- More false positives
- Higher database load
- Not needed for our use case

### 3. Epoch Fencing vs. Generation Numbers

**Decision**: Use monotonically increasing epoch counter

**Rationale**:
- Simple to implement (integer increment)
- Easy to reason about (higher = newer)
- Compact (8 bytes)
- Same as Kafka's leader epoch

**Alternative**: UUID generation
- Harder to compare (need timestamp parsing)
- More storage (16 bytes)
- No clear ordering

### 4. Stateless Agents vs. Stateful

**Decision**: Agents are completely stateless

**Rationale**:
- ✅ Can be killed and restarted anytime
- ✅ Auto-scaling trivial (no state migration)
- ✅ No complex rebalancing protocol
- ✅ Any agent can lead any partition

**Trade-off**: Need lease acquisition for writes
- Cost: 1 database roundtrip per new partition
- Amortized: Lease valid for 30s
- Acceptable for our workload

---

## Key Metrics & Monitoring

### Agent Health
- `agent_heartbeat_age_seconds` - Time since last heartbeat
- `agent_lease_count` - Number of partitions led by agent
- `agent_uptime_seconds` - Time since agent start

### Partition Leadership
- `partition_lease_acquisitions_total` - Lease acquisition count
- `partition_lease_expirations_total` - Lease expiration count
- `partition_lease_epoch` - Current epoch per partition
- `partition_lease_duration_seconds` - Time agent has held lease

### Failover
- `agent_failures_total` - Agent death count
- `partition_failover_duration_seconds` - Time to transfer leadership
- `partition_data_loss_events_total` - Data loss events (should be 0)

### Performance
- `agent_write_latency_seconds` - Write latency including lease check
- `agent_read_latency_seconds` - Read latency
- `metadata_store_query_duration_seconds` - Metadata query latency

---

## Migration Path (Phase 3 → Phase 4)

### Current State (Phase 3)
- Single agent serving all traffic
- No coordination needed
- Direct writes to partitions

### Transition (Phase 4.1-4.2)
- Keep single-agent mode as default
- Add `--multi-agent` flag for new mode
- Backward compatible

### Future State (Phase 4.5)
- Multi-agent is the default
- Single-agent mode deprecated
- All production deployments use multiple agents

### Configuration Example

```yaml
# Phase 3 (single-agent)
streamhouse:
  agent_id: "agent-1"
  address: "10.0.1.5:9090"

# Phase 4 (multi-agent)
streamhouse:
  agent_id: "agent-1"
  address: "10.0.1.5:9090"
  availability_zone: "us-east-1a"
  agent_group: "prod"
  multi_agent: true  # NEW
  lease_duration: 30s  # NEW
  heartbeat_interval: 20s  # NEW
```

---

## Dependencies

### Required for Phase 4
- ✅ Phase 3.1 (Agent coordination API) - Complete
- ✅ Phase 3.2 (PostgreSQL backend) - Complete
- ✅ Phase 3.3 (Metadata caching) - Complete
- ✅ Phase 3.4 (Segment index) - Complete

### Optional (Can be parallel)
- Phase 3.5 (CockroachDB) - Helps with 100K+ partitions
- Phase 3.6 (DynamoDB) - AWS-native deployments
- Phase 3.7 (Pathological testing) - Validates under stress

---

## Risks & Mitigation

### Risk 1: Lease Thundering Herd

**Risk**: 100 agents all renewing leases simultaneously

**Mitigation**:
- Stagger renewal times (jitter = ±10s)
- Connection pooling (limit concurrent DB connections)
- Batch renewals (one query per agent, multiple partitions)

### Risk 2: Split-Brain During Network Partition

**Risk**: Agent thinks it has lease but database disagrees

**Mitigation**:
- Epoch fencing (strict checking)
- Metadata store validates epoch on every write
- Agent loses lease → flush immediately

### Risk 3: Metadata Store Overload

**Risk**: Too many lease renewals overwhelm PostgreSQL

**Mitigation**:
- Phase 3.3 caching reduces query load
- Lease renewals are cheap (single UPDATE)
- Can scale PostgreSQL vertically or use CockroachDB

### Risk 4: Agent Failure Mid-Segment

**Risk**: Agent dies while writing segment, segment corrupted

**Mitigation**:
- Segments are append-only, atomic writes
- Partial segments discarded on recovery
- High watermark tracks committed records

---

## Success Criteria (Phase 4 Complete)

### Functional
- ✅ Multiple agents can run simultaneously
- ✅ Any agent can lead any partition
- ✅ Automatic failover on agent death
- ✅ Zero data loss during failover
- ✅ No split-brain scenarios
- ✅ Multi-AZ deployment working
- ✅ Agent groups provide isolation

### Performance
- ✅ Throughput scales linearly with agents
- ✅ Write latency < 50ms p99
- ✅ Failover time < 30 seconds
- ✅ Metadata store < 100 QPS

### Operational
- ✅ Rolling upgrades without downtime
- ✅ Auto-scaling without rebalancing
- ✅ Comprehensive monitoring
- ✅ Documentation complete
- ✅ Integration tests passing

---

## Documentation Deliverables

1. **Architecture Guide** (`docs/MULTI_AGENT_ARCHITECTURE.md`)
   - System design
   - Lease protocol
   - Failure scenarios

2. **Operations Guide** (`docs/OPERATING_MULTI_AGENT.md`)
   - Deployment patterns
   - Monitoring setup
   - Troubleshooting

3. **API Documentation** (`docs/AGENT_API.md`)
   - Agent lifecycle
   - Lease management
   - Health checks

4. **Phase 4 Completion Report** (`docs/phases/PHASE_4_COMPLETE.md`)
   - What was built
   - Test results
   - Performance metrics

---

## Next Steps

1. **Immediate** (This Week):
   - Review this plan with team
   - Set up Phase 4 tracking
   - Create `streamhouse-agent` crate skeleton

2. **Short Term** (Next 2 Weeks):
   - Implement Agent struct and lifecycle
   - Build heartbeat mechanism
   - Write agent registration tests

3. **Medium Term** (Next 4 Weeks):
   - Implement LeaseManager
   - Build epoch fencing
   - Multi-AZ deployment

4. **Long Term** (Weeks 7-8):
   - Comprehensive testing
   - Performance optimization
   - Documentation

---

**Status**: 📋 Ready to Start → Moving to Phase 4.1 (Agent Infrastructure)

---

## Resources

### Code References
- [MetadataStore Trait](../../crates/streamhouse-metadata/src/lib.rs)
- [Partition Leases Schema](../../crates/streamhouse-metadata/migrations-postgres/002_agent_coordination.sql)
- [Phase 3 Status](phase3/STATUS.md)

### External References
- [WarpStream Architecture](https://www.warpstream.com/blog/architecture)
- [Kafka Leader Election](https://kafka.apache.org/documentation/#replication)
- [Chubby Lock Service](https://research.google/pubs/pub27897/)

### Testing
```bash
# Run agent tests (once implemented)
cargo test -p streamhouse-agent

# Run integration tests
cargo test --workspace --test multi_agent

# Benchmark
cargo bench --bench agent_throughput
```
