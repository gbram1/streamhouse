# Phase 4: Multi-Agent Architecture

**Status**: 🚧 In Progress (Started 2026-01-23)
**Goal**: Enable horizontal scaling with multiple stateless agents
**Duration**: 6-8 weeks

---

## Overview

Phase 4 transforms StreamHouse into a distributed system where multiple agents coordinate to serve traffic. This is the WarpStream-style architecture that enables massive scale.

### What Phase 4 Delivers

```
Before (Single Agent):
┌─────────┐
│ Client  │
└────┬────┘
     │
     ▼
┌─────────┐
│ Agent-1 │  ← Single point of failure
└────┬────┘
     │
     ▼
┌─────────┐
│ Storage │
└─────────┘

After (Multi-Agent):
┌─────────┐   ┌─────────┐   ┌─────────┐
│Client-1 │   │Client-2 │   │Client-3 │
└────┬────┘   └────┬────┘   └────┬────┘
     │            │            │
     ├────────────┼────────────┤
     │            │            │
     ▼            ▼            ▼
┌─────────┐  ┌─────────┐  ┌─────────┐
│ Agent-1 │  │ Agent-2 │  │ Agent-3 │  ← Any can fail
└────┬────┘  └────┬────┘  └────┬────┘
     │            │            │
     └────────────┼────────────┘
                  │
                  ▼
         ┌─────────────────┐
         │ Metadata Store  │
         │ (Coordination)  │
         └────────┬────────┘
                  │
                  ▼
         ┌─────────────────┐
         │  Object Store   │
         │   (Segments)    │
         └─────────────────┘
```

### Key Features

1. **Stateless Agents**
   - No local state (everything in metadata store + S3)
   - Kill and restart anytime
   - Auto-scaling without rebalancing

2. **Lease-Based Leadership**
   - Each partition has one leader at a time
   - 30-second leases with automatic renewal
   - Fast failover on agent death

3. **Epoch Fencing**
   - Prevents split-brain scenarios
   - Zombie agents can't corrupt data
   - Monotonic epoch counter

4. **Multi-AZ Support**
   - Agents in multiple availability zones
   - Zero inter-AZ data transfer costs
   - AZ-aware partition assignment

5. **Agent Groups**
   - Network-isolated agent pools
   - Multi-tenant deployments
   - Prod/staging separation

---

## Sub-Phases

### Phase 4.1: Agent Infrastructure (Weeks 1-2) - 🚧 CURRENT

**Status**: 70% Complete
**Goal**: Build core Agent struct and lifecycle

**Deliverables**:
- ✅ Agent struct with configuration
- ✅ Agent registration with metadata store
- ✅ Heartbeat mechanism (background task)
- ✅ Graceful start/stop
- ✅ Agent discovery
- ✅ Example demonstrating agent lifecycle
- 🚧 Health check HTTP endpoint

**Files**:
- `crates/streamhouse-agent/src/agent.rs`
- `crates/streamhouse-agent/src/heartbeat.rs`
- `crates/streamhouse-agent/src/error.rs`

---

### Phase 4.2: Partition Leadership (Weeks 3-4)

**Status**: Planned
**Goal**: Implement lease-based partition coordination

**Deliverables**:
- LeaseManager struct
- Lease acquisition logic
- Lease renewal background task
- Epoch fencing
- Leadership transfer
- Split-brain prevention

**Files**:
- `crates/streamhouse-agent/src/lease_manager.rs`

---

### Phase 4.3: Multi-AZ Deployment (Week 5)

**Status**: Planned
**Goal**: Enable agents across availability zones

**Deliverables**:
- AZ-aware partition assignment
- Client routing preferences
- S3 multi-AZ access patterns
- Cost optimization metrics

---

### Phase 4.4: Agent Groups (Week 6)

**Status**: Planned
**Goal**: Network isolation for multi-tenancy

**Deliverables**:
- Agent group configuration
- Topic→group affinity
- Network isolation validation
- Multi-tenant examples

---

### Phase 4.5: Testing & Validation (Weeks 7-8)

**Status**: Planned
**Goal**: Comprehensive multi-agent testing

**Test Scenarios**:
- Agent failure during write
- Agent failure during read
- Network partition simulation
- Rolling upgrades
- Thundering herd
- Performance benchmarks

---

## Progress Tracking

| Component | Status | Progress |
|-----------|--------|----------|
| Agent struct | ✅ Complete | 100% |
| Heartbeat mechanism | ✅ Complete | 100% |
| LeaseManager (stub) | ✅ Complete | 20% |
| Epoch fencing | 📋 Planned | 0% |
| Multi-AZ support | 📋 Planned | 0% |
| Agent groups | 📋 Planned | 0% |
| Integration tests | 📋 Planned | 0% |
| Documentation | 🚧 In Progress | 30% |

**Overall Phase 4 Progress**: 30% (6/20 Phase 4.1 tasks complete)

---

## Architecture Decisions

### 1. Why Lease-Based Leadership?

**Alternatives Considered**:
- Raft consensus (too complex for partition leadership)
- ZooKeeper-style coordination (unnecessary dependency)
- Paxos (overkill for our use case)

**Decision**: Lease-based with PostgreSQL
- Simpler implementation
- Fast failover (lease timeout)
- Leverages existing metadata store
- Good enough for S3-native architecture

### 2. Why 30-Second Leases?

**Balance**:
- Short enough: Fast failover
- Long enough: Low renewal overhead
- 3× heartbeat interval (20s)

### 3. Why Stateless Agents?

**Benefits**:
- Trivial auto-scaling (no state migration)
- Kill anytime (no rebalancing)
- Any agent can lead any partition
- Simpler operational model

**Trade-off**: Need lease acquisition for writes
- Cost: 1 metadata query per new partition
- Amortized: Lease valid for 30s
- Acceptable overhead

---

## Key Metrics

### Agent Health
- `agent_up` (1 if alive, 0 if dead)
- `agent_heartbeat_age_seconds`
- `agent_lease_count`

### Leadership
- `partition_leader_agent_id`
- `partition_lease_epoch`
- `partition_failover_count`

### Performance
- `agent_write_latency_seconds`
- `agent_lease_acquisition_duration_seconds`
- `metadata_store_query_duration_seconds`

---

## Documentation

- [Phase 4 Plan](../PHASE_4_PLAN.md) - Complete implementation guide
- [Agent API](../../AGENT_API.md) - Agent lifecycle and APIs
- [Multi-Agent Operations](../../OPERATING_MULTI_AGENT.md) - Deployment guide

---

## Testing

```bash
# Run agent tests
cargo test -p streamhouse-agent

# Run multi-agent integration tests
cargo test --workspace --test multi_agent

# Performance benchmarks
cargo bench --bench agent_throughput
```

---

## Next Steps

1. **This Week** (Phase 4.1):
   - [ ] Complete Agent struct
   - [ ] Implement heartbeat mechanism
   - [ ] Write agent lifecycle tests
   - [ ] Document agent configuration

2. **Next 2 Weeks** (Phase 4.2):
   - [ ] Build LeaseManager
   - [ ] Implement epoch fencing
   - [ ] Test leadership transfer
   - [ ] Validate split-brain prevention

3. **Weeks 5-6** (Phase 4.3-4.4):
   - [ ] Multi-AZ deployment
   - [ ] Agent groups
   - [ ] Cost optimization

4. **Weeks 7-8** (Phase 4.5):
   - [ ] Comprehensive testing
   - [ ] Performance validation
   - [ ] Documentation complete

---

**Current Sprint**: Phase 4.1 (Agent Infrastructure)
**Started**: 2026-01-23
**Target Completion**: 2026-03-20 (8 weeks)
