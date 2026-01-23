# StreamHouse - Agent Failure Detection

**Last Updated**: 2026-01-23
**Phase**: 4.1 (Agent Infrastructure)
**Status**: 🚧 In Progress

---

## Overview

Agent failure detection is how StreamHouse knows when an agent has died and needs to trigger failover. Multiple overlapping mechanisms ensure fast, reliable detection.

## Detection Mechanisms

### 1. Heartbeat Mechanism (Primary)

**How it works**:
```rust
// Each agent updates PostgreSQL every 20 seconds
loop {
    metadata_store.register_agent(AgentInfo {
        agent_id: "agent-us-east-1a",
        address: "10.0.1.5:9090",
        availability_zone: "us-east-1a",
        agent_group: "prod",
        last_heartbeat: now_ms(),  // Current timestamp
        started_at: agent_start_time,
        metadata: "{}",
    }).await?;

    tokio::time::sleep(Duration::from_secs(20)).await;
}
```

**Database query**:
```sql
-- Upsert agent heartbeat
INSERT INTO agents (agent_id, address, availability_zone, agent_group,
                    last_heartbeat, started_at, metadata)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (agent_id) DO UPDATE SET
    address = EXCLUDED.address,
    availability_zone = EXCLUDED.availability_zone,
    agent_group = EXCLUDED.agent_group,
    last_heartbeat = EXCLUDED.last_heartbeat,
    metadata = EXCLUDED.metadata;
```

**Detection**:
```sql
-- Query for alive agents (heartbeat within 60s)
SELECT agent_id, address, availability_zone, last_heartbeat
FROM agents
WHERE last_heartbeat > (EXTRACT(EPOCH FROM NOW()) * 1000 - 60000);
```

**Configuration**:
| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Heartbeat interval | 20s | Frequent enough for fast detection |
| Heartbeat timeout | 60s | 3× interval (allows 2 missed beats) |
| Detection latency | 60s | Time until agent considered dead |

**Timeline Example**:
```
T=0s:   Agent-1 starts, heartbeat = 1769188800000
T=20s:  Agent-1 updates heartbeat = 1769188820000  ✅
T=40s:  Agent-1 updates heartbeat = 1769188840000  ✅
T=45s:  Agent-1 CRASHES 💥
T=60s:  Agent-1 misses heartbeat (no update)
T=80s:  Agent-1 misses 2nd heartbeat
T=105s: Other agents detect: "last_heartbeat = 60s ago → DEAD"
        └─> Agent-1 marked as dead in monitoring
        └─> Alerts fire
```

---

### 2. Lease Expiration (Automatic Failover)

**Why it's better than heartbeat**: Leases expire **automatically** without needing to detect failures.

**How it works**:
```sql
CREATE TABLE partition_leases (
    topic VARCHAR(255),
    partition_id INT,
    agent_id VARCHAR(255),
    lease_epoch BIGINT,
    lease_expires_at BIGINT,  -- Absolute timestamp!
    PRIMARY KEY (topic, partition_id)
);
```

**Lease lifecycle**:
```
T=0s:   Agent-1 acquires lease
        └─> INSERT partition_leases (agent_id='agent-1',
                                     lease_expires_at=T+30s,
                                     lease_epoch=1)

T=15s:  Agent-1 renews lease
        └─> UPDATE partition_leases
            SET lease_expires_at=T+45s, lease_epoch=2
            WHERE agent_id='agent-1'

T=20s:  Agent-1 CRASHES 💥
        └─> No renewal happens

T=45s:  Lease EXPIRES automatically
        └─> lease_expires_at < current_time

T=50s:  Agent-2 tries to write
        └─> SELECT * FROM partition_leases
            WHERE topic='orders' AND partition_id=0
        └─> Sees lease_expires_at=T+45s (5s ago!)
        └─> Lease is expired → Agent-2 acquires it
        └─> UPDATE partition_leases
            SET agent_id='agent-2',
                lease_expires_at=T+80s,
                lease_epoch=3
```

**Key insight**: Failover happens in **30 seconds** without needing to detect Agent-1 died!

**Configuration**:
| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Lease duration | 30s | Balance between failover speed and renewal overhead |
| Renewal interval | 20s | Before lease expires (10s buffer) |
| Failover time | 30s | Lease expiration time |

---

### 3. Kubernetes Liveness Probes

**How it works**:
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: streamhouse-agent
spec:
  template:
    spec:
      containers:
      - name: agent
        image: streamhouse/agent:v1.0.0
        livenessProbe:
          httpGet:
            path: /health
            port: 9091
          initialDelaySeconds: 10
          periodSeconds: 10        # Check every 10s
          timeoutSeconds: 5        # Health check must respond in 5s
          failureThreshold: 3      # 3 failures = pod restart
```

**Health check endpoint**:
```rust
// GET /health
{
    "status": "healthy",
    "agent_id": "agent-us-east-1a",
    "uptime_seconds": 3600,
    "active_leases": 15,
    "last_heartbeat_success": true,
    "metadata_store_connected": true,
    "object_store_connected": true
}
```

**Failure scenarios**:

**Scenario A: Agent hangs (deadlock, infinite loop)**
```
T=0s:   Agent-1 deadlocks
T=10s:  Kubernetes liveness probe times out (no response)
T=20s:  2nd probe times out
T=30s:  3rd probe times out → failureThreshold reached
T=30s:  Kubernetes kills pod (SIGTERM → SIGKILL)
T=35s:  New pod starts
T=65s:  New agent online and acquiring leases
```

**Scenario B: Agent OOM (out of memory)**
```
T=0s:   Agent-1 runs out of memory
T=0s:   Linux OOM killer terminates process immediately
T=0s:   Kubernetes detects pod exit
T=5s:   New pod starts
T=35s:  New agent online
```

**Scenario C: Node failure (EC2 instance dies)**
```
T=0s:   EC2 instance terminated
T=0s:   Kubernetes node becomes NotReady
T=40s:  Kubernetes evicts pods from dead node
T=45s:  New pods scheduled on healthy nodes
T=75s:  New agents online
```

---

### 4. Client Write Timeouts

**How it works**:
```rust
// Producer client code
let mut retries = 0;
const MAX_RETRIES: u32 = 3;

loop {
    let result = client
        .produce(topic, partition, record)
        .timeout(Duration::from_secs(30))  // 30s timeout
        .await;

    match result {
        Ok(_) => break,
        Err(ProduceError::Timeout) => {
            retries += 1;
            if retries >= MAX_RETRIES {
                return Err("Max retries exceeded");
            }
            // Exponential backoff: 1s, 2s, 4s
            let backoff = Duration::from_secs(2_u64.pow(retries));
            tokio::time::sleep(backoff).await;
            // Retry may hit different agent
        }
        Err(ProduceError::LeaseNotHeld) => {
            // Agent lost leadership mid-write
            // Immediate retry (no backoff)
            continue;
        }
        Err(e) => return Err(e),
    }
}
```

**Timeline**:
```
T=0s:   Client sends write to Agent-1 (via load balancer)
T=1s:   Agent-1 CRASHES 💥
T=30s:  Client timeout (no response from Agent-1)
T=30s:  Client retries (exponential backoff = 1s)
T=31s:  Load balancer routes to Agent-2
T=31s:  Agent-2 acquires lease (lease expired at T+30s)
T=31s:  Write succeeds ✅
```

**Client-side failover**: 30s timeout + 1s retry = **31s total**

---

### 5. PostgreSQL Connection Monitoring

**How it works**:
```rust
// Agent startup
let pool = PgPoolOptions::new()
    .max_connections(20)
    .acquire_timeout(Duration::from_secs(5))
    .test_before_acquire(true)  // Ping connection before using
    .after_connect(|conn, _meta| {
        Box::pin(async move {
            conn.execute("SET application_name = 'streamhouse-agent'").await?;
            Ok(())
        })
    })
    .connect(&database_url).await?;

// Connection health check (every 30s)
loop {
    match pool.acquire().await {
        Ok(_) => {
            metrics.record_metadata_store_healthy();
        }
        Err(e) => {
            error!("PostgreSQL connection failed: {}", e);
            metrics.record_metadata_store_unhealthy();
            // Health endpoint returns unhealthy
            // Kubernetes liveness probe will fail
        }
    }
    tokio::time::sleep(Duration::from_secs(30)).await;
}
```

**If PostgreSQL becomes unreachable**:
```
T=0s:   PostgreSQL connection lost (network partition, RDS failure)
T=0s:   Agent-1 cannot update heartbeat
T=0s:   Agent-1 cannot renew leases
T=0s:   Health endpoint returns "unhealthy"
T=30s:  Kubernetes kills Agent-1 pod (liveness probe failure)
T=30s:  Agent-1's leases expire
T=35s:  New pod starts (may connect to read replica if available)
```

---

### 6. Agent Registry Monitoring (Dashboard)

**Grafana dashboard queries**:

**Live agents count**:
```sql
SELECT COUNT(*) as live_agents
FROM agents
WHERE last_heartbeat > (EXTRACT(EPOCH FROM NOW()) * 1000 - 60000);
```

**Expected**: 6 agents (for 6-agent deployment)
**Alert if**: < 4 agents (lost 2+ agents)

**Agent heartbeat freshness**:
```sql
SELECT
    agent_id,
    (EXTRACT(EPOCH FROM NOW()) * 1000 - last_heartbeat) / 1000 AS seconds_since_heartbeat
FROM agents
ORDER BY last_heartbeat DESC;
```

**Visualization**:
```
┌─────────────────────────────────────────┐
│ StreamHouse Agent Health                │
├─────────────────────────────────────────┤
│ Live Agents: 6 / 6              ✅      │
│                                         │
│ Agent-1: ████████████ (10s ago)        │
│ Agent-2: ████████████ (15s ago)        │
│ Agent-3: ████████████ (8s ago)         │
│ Agent-4: ████████████ (12s ago)        │
│ Agent-5: ████████████ (20s ago)        │
│ Agent-6: ████████████ (5s ago)         │
└─────────────────────────────────────────┘
```

**Orphaned partitions (no leader)**:
```sql
SELECT topic, partition_id, agent_id, lease_expires_at
FROM partition_leases
WHERE lease_expires_at < (EXTRACT(EPOCH FROM NOW()) * 1000);
```

**Alert if**: Any rows returned (partitions without leader)

---

## Complete Failure Detection Flow

### Scenario: Agent-1 Crashes Hard (kernel panic, power loss)

**Detection Timeline**:

| Time | Event | Mechanism | Action |
|------|-------|-----------|--------|
| T=0s | Agent-1 crashes | N/A | Process dies immediately |
| T=0s | Kubernetes detects pod exit | Container runtime | Starts new pod |
| T=10s | Liveness probe fails (1st) | K8s health check | Warning |
| T=20s | Liveness probe fails (2nd) | K8s health check | Warning |
| T=30s | Agent-1's leases expire | Automatic expiration | Partitions available for takeover |
| T=30s | Liveness probe fails (3rd) | K8s health check | Pod marked dead |
| T=35s | New pod starts | Kubernetes scheduler | New agent registering |
| T=60s | Heartbeat timeout reached | PostgreSQL query | Agent-1 marked dead in monitoring |
| T=65s | New agent acquires leases | Lease acquisition | Partitions back online |

**Total downtime per partition**: 30-65 seconds (depends on when clients retry)

**Detection latency**: 0-60 seconds (depending on mechanism)

**Failover latency**: 30 seconds (lease expiration)

---

### Scenario: Agent-1 Network Partition (can't reach PostgreSQL)

**Detection Timeline**:

| Time | Event | Mechanism | Action |
|------|-------|-----------|--------|
| T=0s | Network partition | N/A | Agent-1 isolated |
| T=0s | Heartbeat fails (cannot UPDATE agents) | PostgreSQL connection error | Logged locally |
| T=0s | Lease renewal fails | PostgreSQL connection error | Logged locally |
| T=0s | Health endpoint returns unhealthy | Agent self-monitoring | `/health` returns 503 |
| T=10s | Liveness probe fails (1st) | K8s health check | Warning |
| T=20s | Liveness probe fails (2nd) | K8s health check | Warning |
| T=30s | Liveness probe fails (3rd) | K8s health check | Kubernetes kills pod |
| T=30s | Agent-1's leases expire | Automatic expiration | Partitions available |
| T=35s | New pod starts (on healthy node) | Kubernetes scheduler | New agent online |
| T=60s | Heartbeat timeout | PostgreSQL query | Agent-1 marked dead |

**Total downtime**: 30 seconds (lease expiration triggers failover)

---

### Scenario: Agent-1 Graceful Shutdown (deployment update)

**Detection Timeline**:

| Time | Event | Mechanism | Action |
|------|-------|-----------|--------|
| T=0s | SIGTERM received | Kubernetes rolling update | Agent begins graceful shutdown |
| T=0s | Stop accepting new writes | Agent shutdown handler | Returns 503 to new requests |
| T=1s | Flush all buffered data | Agent shutdown handler | Upload pending segments to S3 |
| T=2s | Release all leases | Agent shutdown handler | UPDATE partition_leases (clear agent_id) |
| T=2s | Deregister agent | Agent shutdown handler | DELETE FROM agents WHERE agent_id=... |
| T=2s | Stop heartbeat task | Agent shutdown handler | Cancel background task |
| T=2s | Close connections | Agent shutdown handler | Close PostgreSQL + S3 connections |
| T=2s | Exit process | Agent shutdown handler | Return 0 (success) |
| T=2s | Kubernetes starts new pod | Rolling update | New agent starts |
| T=32s | New agent online | Agent startup | New agent acquiring leases |

**Total downtime**: 0 seconds (leases released immediately, other agents take over)

---

## Metrics & Alerting

### Prometheus Metrics

**Agent-level**:
```
# Agent alive status (1 = alive, 0 = dead)
agent_up{agent_id="agent-us-east-1a"} 1

# Seconds since last heartbeat
agent_heartbeat_age_seconds{agent_id="agent-us-east-1a"} 15

# Number of partitions this agent leads
agent_lease_count{agent_id="agent-us-east-1a"} 42

# Heartbeat success/failure rate
agent_heartbeat_success_total{agent_id="agent-us-east-1a"} 3600
agent_heartbeat_failure_total{agent_id="agent-us-east-1a"} 2
```

**Partition-level**:
```
# Which agent leads this partition
partition_leader_agent_id{topic="orders",partition="0"} "agent-us-east-1a"

# Current epoch (increments on failover)
partition_lease_epoch{topic="orders",partition="0"} 42

# How many times leadership changed
partition_failover_count{topic="orders",partition="0"} 3
```

**System-level**:
```
# Total live agents
streamhouse_live_agents 6

# Total partitions
streamhouse_total_partitions 1000

# Orphaned partitions (no leader)
streamhouse_orphaned_partitions 0
```

---

### Alert Rules

**Critical: Agent Down**
```yaml
- alert: AgentDown
  expr: agent_up == 0 OR agent_heartbeat_age_seconds > 120
  for: 2m
  labels:
    severity: critical
  annotations:
    summary: "StreamHouse agent {{ $labels.agent_id }} is down"
    description: "Agent {{ $labels.agent_id }} has not sent heartbeat for {{ $value }}s"
```

**Critical: Multiple Agents Down**
```yaml
- alert: MultipleAgentsDown
  expr: streamhouse_live_agents < 4
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: "Only {{ $value }} StreamHouse agents are alive"
    description: "Expected 6 agents, but only {{ $value }} are responding"
```

**Critical: Orphaned Partitions**
```yaml
- alert: OrphanedPartitions
  expr: streamhouse_orphaned_partitions > 0
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: "{{ $value }} partitions have no leader"
    description: "Partitions without a leader cannot accept writes"
```

**Warning: High Failover Rate**
```yaml
- alert: HighFailoverRate
  expr: rate(partition_failover_count[5m]) > 2
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Partition {{ $labels.topic }}/{{ $labels.partition }} failing over frequently"
    description: "{{ $value }} failovers in 5 minutes (may indicate unstable agent)"
```

**Warning: Heartbeat Failures**
```yaml
- alert: HeartbeatFailures
  expr: rate(agent_heartbeat_failure_total[5m]) > 0.1
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Agent {{ $labels.agent_id }} having heartbeat failures"
    description: "{{ $value }} failures/sec (may indicate PostgreSQL connectivity issues)"
```

---

## Implementation Checklist (Phase 4.1)

### Heartbeat Mechanism

- [ ] Create `HeartbeatTask` struct in `crates/streamhouse-agent/src/heartbeat.rs`
- [ ] Implement background loop (20s interval)
- [ ] Call `metadata_store.register_agent()` with current timestamp
- [ ] Handle PostgreSQL connection errors gracefully
- [ ] Log heartbeat success/failure
- [ ] Emit metrics for monitoring

### Health Check Endpoint

- [ ] Add HTTP server to agent (port 9091)
- [ ] Implement `GET /health` endpoint
- [ ] Check PostgreSQL connection alive
- [ ] Check S3 connection alive
- [ ] Return JSON with agent status
- [ ] Return 200 if healthy, 503 if unhealthy

### Graceful Shutdown

- [ ] Register SIGTERM handler
- [ ] Stop accepting new writes (return 503)
- [ ] Flush all buffered segments to S3
- [ ] Release all partition leases
- [ ] Stop heartbeat task
- [ ] Deregister agent from metadata store
- [ ] Close connections gracefully
- [ ] Exit with code 0

### Agent Discovery

- [ ] Implement `list_agents()` wrapper
- [ ] Filter agents by availability zone (optional)
- [ ] Filter agents by agent group (optional)
- [ ] Return live agents only (heartbeat < 60s)

### Testing

- [ ] Unit test: Heartbeat task updates timestamp
- [ ] Unit test: Health check returns correct status
- [ ] Integration test: Agent registers on startup
- [ ] Integration test: Graceful shutdown releases leases
- [ ] Integration test: Dead agent filtered out after 60s
- [ ] Chaos test: Kill agent, verify failover < 30s

---

## Configuration

**Agent config** (`agent.toml`):
```toml
[agent]
agent_id = "agent-us-east-1a-001"
address = "10.0.1.5:9090"
availability_zone = "us-east-1a"
agent_group = "prod"

[heartbeat]
interval_seconds = 20      # How often to update heartbeat
timeout_seconds = 60       # When to consider agent dead

[health_check]
port = 9091
path = "/health"

[graceful_shutdown]
timeout_seconds = 30       # Max time to flush data before exit
```

**Kubernetes config**:
```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 9091
  initialDelaySeconds: 10
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /health
    port: 9091
  initialDelaySeconds: 5
  periodSeconds: 5
  timeoutSeconds: 3
  successThreshold: 1
  failureThreshold: 2
```

---

## Summary

**How we detect agent failures**:

1. ✅ **Heartbeat mechanism** (60s detection, requires PostgreSQL queries)
2. ✅ **Lease expiration** (30s automatic failover, no detection needed!)
3. ✅ **Kubernetes liveness probes** (30s pod restart)
4. ✅ **Write timeouts** (30s client-side detection)
5. ✅ **PostgreSQL monitoring** (connection health checks)
6. ✅ **Grafana dashboards** (real-time visibility)
7. ✅ **Automated alerts** (PagerDuty/Slack notifications)

**Fastest detection**: Lease expiration (30s)
**Most reliable**: Multiple overlapping mechanisms
**No single point of failure**: If one detection method fails, others compensate

**Phase 4.1 deliverables** (this week):
- Heartbeat background task
- Health check HTTP endpoint
- Graceful shutdown handler
- Agent discovery

**Phase 4.2 deliverables** (next 2 weeks):
- LeaseManager with automatic renewal
- Failover on lease expiration
- Epoch fencing for split-brain prevention

---

**Next**: Implement `HeartbeatTask` in `crates/streamhouse-agent/src/heartbeat.rs`
