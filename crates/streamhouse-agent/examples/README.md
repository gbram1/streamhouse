# StreamHouse Agent Examples

This directory contains demonstration scripts for testing StreamHouse agent functionality.

## Available Examples

### 1. Simple Agent (`simple_agent.rs`)

Basic agent lifecycle demonstration showing registration, heartbeat, and graceful shutdown.

**Run:**
```bash
cargo run --package streamhouse-agent --example simple_agent
```

**What it demonstrates:**
- Agent creation and configuration
- Registration with metadata store
- Heartbeat mechanism
- Graceful shutdown and deregistration

**Duration:** ~5 seconds

---

### 2. Multi-Agent Demo (`demo_phase_4_multi_agent.rs`)

Demonstrates multi-agent coordination with automatic partition assignment.

**Run:**
```bash
cargo run --package streamhouse-agent --example demo_phase_4_multi_agent
```

**What it demonstrates:**
- Starting 3 agents with auto-assignment
- Initial partition distribution
- Agent failure simulation (agent-2 crashes)
- Automatic rebalancing after failure
- Agent recovery
- Final balanced distribution
- Graceful shutdown

**Duration:** ~2 minutes (includes wait times for rebalancing)

**Topics Created:**
- `orders`: 6 partitions
- `users`: 3 partitions

**Total:** 9 partitions distributed across 3 agents

---

### 3. Local Workflow Test (`local_workflow_test.rs`) ⭐ **Recommended**

Comprehensive end-to-end test of the full StreamHouse workflow.

**Run:**
```bash
cargo run --package streamhouse-agent --example local_workflow_test
```

**What it demonstrates:**
1. ✅ **Setup**: Create SQLite metadata store
2. ✅ **Topics**: Create 3 topics with 21 total partitions
3. ✅ **Agents**: Start 3 agents with auto-assignment
4. ✅ **Initial Assignment**: Show partition distribution
5. ✅ **Lease Status**: Display active leases and epochs
6. ✅ **Failure Simulation**: Stop agent-2 (crash scenario)
7. ✅ **Rebalancing**: Automatic partition redistribution
8. ✅ **Recovery**: Restart failed agent
9. ✅ **Health Check**: Cluster health summary
10. ✅ **Discovery**: Agent discovery queries (by group, by zone)
11. ✅ **Verification**: Lease ownership validation
12. ✅ **Shutdown**: Clean shutdown with full cleanup

**Duration:** ~2.5 minutes

**Topics Created:**
- `orders`: 6 partitions
- `users`: 3 partitions
- `analytics`: 12 partitions

**Total:** 21 partitions distributed across 3 agents

**Sample Output:**
```
🎯 StreamHouse Local Workflow Test
============================================================
✓ Created topic 'orders' with 6 partitions
✓ Created topic 'users' with 3 partitions
✓ Created topic 'analytics' with 12 partitions

📊 Total: 3 topics, 21 partitions

✓ Agent 1 started in zone-2
✓ Agent 2 started in zone-1
✓ Agent 3 started in zone-2

📊 Initial Partition Distribution:
Agent: agent-1
  Assigned partitions (12):
    analytics: [0, 2, 4, 5, 6, 7, 9, 10]
    orders: [0, 4]
    users: [1, 2]

💥 Simulating failure: stopping agent-2...
✓ Agent-2 stopped gracefully

⏳ Waiting for rebalancing (35 seconds)...

[Partitions automatically redistributed to agent-1 and agent-3]

🔄 Restarting agent-2...
✓ Agent-2 restarted successfully

[Final balanced distribution achieved]

✅ All resources cleaned up successfully!
```

---

## Quick Comparison

| Example | Duration | Agents | Topics | Partitions | Features |
|---------|----------|--------|--------|------------|----------|
| **simple_agent** | 5s | 1 | 0 | 0 | Basic lifecycle |
| **demo_phase_4_multi_agent** | 2m | 3 | 2 | 9 | Multi-agent coordination |
| **local_workflow_test** | 2.5m | 3 | 3 | 21 | Full workflow ⭐ |

---

## What to Watch For

### Initial Assignment
When agents start, you'll see:
```
INFO streamhouse_agent::assigner: Topology changed, performing rebalance
INFO streamhouse_agent::assigner: Acquired partition topic=orders partition_id=0 epoch=1
```

### Lease Renewal
Every 10 seconds (in background):
```
INFO streamhouse_agent::lease_manager: Renewed 5 leases
```

### Rebalancing
When topology changes (agent joins/leaves):
```
INFO streamhouse_agent::assigner: Topology changed, performing rebalance
INFO streamhouse_agent::assigner: Released partition topic=orders partition_id=2
INFO streamhouse_agent::assigner: Acquired partition topic=orders partition_id=5 epoch=2
```

### Graceful Shutdown
```
INFO streamhouse_agent::agent: Stopping agent gracefully
INFO streamhouse_agent::lease_manager: Released partition lease topic=orders partition_id=0
INFO streamhouse_agent::agent: Agent stopped successfully
```

---

## Testing Scenarios

### Test Failover
1. Run `local_workflow_test`
2. Observe initial distribution
3. Watch agent-2 crash
4. See partitions redistribute to remaining agents
5. Verify no data loss or split-brain

### Test Load Balancing
1. Run `local_workflow_test`
2. Check "Load Distribution" section
3. Verify partitions spread relatively evenly
4. Note: Consistent hashing may cause slight imbalance (acceptable)

### Test Discovery
1. Run `local_workflow_test`
2. Check "Agent Discovery" section
3. See agents filtered by group and zone
4. Verify zone-aware distribution

### Test Cleanup
1. Run any example
2. Wait for completion
3. Check final cleanup stats
4. Verify: `Remaining agents: 0`, `Remaining leases: 0`

---

## Troubleshooting

### "Failed to acquire lease"
**Cause:** Another agent holds the lease
**Solution:** This is expected behavior - partition is already assigned

### "Topology changed" logs repeated
**Cause:** Agents joining/leaving during rebalance window
**Solution:** Normal during startup, should stabilize

### High lease count on one agent
**Cause:** Consistent hashing distribution
**Solution:** This is expected - small variance is normal

### Test hangs during "Waiting for rebalancing"
**Cause:** Rebalance interval is 30 seconds
**Solution:** Be patient - this is intentional for demonstration

---

## Performance Notes

### Metadata Store
All examples use **SQLite** (in-memory or temp file)
- Perfect for local testing
- Production should use PostgreSQL

### Timing
- Heartbeat interval: **5 seconds** (faster than production)
- Lease renewal: **10 seconds** (production default)
- Rebalance check: **30 seconds** (production default)

### Logs
All examples use `tracing` with `INFO` level
- To see more: Set `RUST_LOG=debug`
- To see less: Set `RUST_LOG=warn`

**Example:**
```bash
RUST_LOG=debug cargo run --package streamhouse-agent --example local_workflow_test
```

---

## Next Steps

After running the examples:

1. **Read the docs**: [Phase 4 Overview](../../../docs/phases/PHASE_4_OVERVIEW.md)
2. **Run tests**: `cargo test --package streamhouse-agent`
3. **Try PostgreSQL**: Modify examples to use PostgreSQL instead of SQLite
4. **Add more agents**: Change agent count from 3 to 6 or 10
5. **Increase partitions**: Create topics with 100+ partitions

---

## Example Code Snippets

### Create an Agent with Auto-Assignment
```rust
use streamhouse_agent::Agent;
use std::sync::Arc;
use std::time::Duration;

let agent = Agent::builder()
    .agent_id("my-agent-1")
    .address("127.0.0.1:9090")
    .availability_zone("us-east-1a")
    .agent_group("prod")
    .heartbeat_interval(Duration::from_secs(20))
    .metadata_store(metadata_store)
    .managed_topics(vec!["orders".to_string(), "users".to_string()])
    .build()
    .await?;

agent.start().await?;
```

### Manual Lease Management (No Auto-Assignment)
```rust
let agent = Agent::builder()
    .agent_id("my-agent-1")
    .address("127.0.0.1:9090")
    .metadata_store(metadata_store)
    // Don't set managed_topics - manual control
    .build()
    .await?;

agent.start().await?;

// Manually acquire lease
let epoch = agent.lease_manager()
    .ensure_lease("orders", 0)
    .await?;

// Use the partition...

// Manually release
agent.lease_manager()
    .release_lease("orders", 0)
    .await?;
```

---

**Happy Testing!** 🎯
