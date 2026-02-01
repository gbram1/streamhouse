# Runbook: Partition Lease Conflicts

**Severity**: HIGH
**Component**: Lease Manager (Agent)
**Impact**: Partition ownership conflicts, write failures, potential data corruption

---

## Symptoms

### Producer Side
- Producers receive intermittent write failures
- Error message: `"Failed to append: Lease conflict"` or `"Stale epoch rejected"`
- Some writes succeed, others fail unpredictably

### Agent Logs
```
ERROR Failed to acquire lease for orders-0: Already held by agent-xyz
WARN  Lease renewal failed: Stale epoch (current: 42, attempted: 41)
ERROR Stale write rejected: epoch 41 < current epoch 42
INFO  Lost lease for partition orders-0 (expired at 1706712345)
```

### Monitoring
- Metric `lease_conflicts_total` increasing
- Metric `lease_acquisition_failures_total` spiking
- Multiple agents reporting ownership of same partition
- Partition write failures >5%

---

## Root Causes

Lease conflicts occur when:

1. **Multiple Agents Compete**: Two agents try to acquire the same partition lease
2. **Clock Skew**: Agents have different system times, causing lease expiration mismatches
3. **Network Partitions**: Agent loses connectivity, lease expires, another agent takes over
4. **Lease Renewal Failures**: Background renewal task fails, lease expires silently
5. **Agent Restarts**: Old agent still thinks it has lease, new agent acquires it (split-brain)
6. **Database Latency**: Lease acquisition/renewal queries timeout, causing retries and conflicts

---

## Investigation Steps

### Step 1: Identify Conflicting Agents

Check which agents claim ownership of the partition:
```bash
# Check lease table in PostgreSQL
psql -h YOUR_DB_HOST -U streamhouse -d streamhouse -c "
SELECT
  topic,
  partition_id,
  agent_id,
  lease_epoch,
  to_timestamp(lease_expires_at / 1000) AS expires_at,
  CASE
    WHEN lease_expires_at > EXTRACT(EPOCH FROM NOW()) * 1000 THEN 'ACTIVE'
    ELSE 'EXPIRED'
  END AS status
FROM partition_leases
WHERE topic = 'orders' AND partition_id = 0;
"

# Expected: Single row with one agent_id
# Problem: Multiple rows or rapidly changing agent_id
```

Example output:
```
 topic  | partition_id | agent_id  | lease_epoch | expires_at          | status
--------+--------------+-----------+-------------+---------------------+---------
 orders |            0 | agent-abc |          42 | 2026-01-31 11:05:15 | ACTIVE
```

### Step 2: Check Agent Logs for Lease Events

Search for lease acquisition failures:
```bash
# Agent 1 logs
kubectl logs -n streamhouse pod/streamhouse-agent-abc --tail=200 | grep -E "lease|epoch"

# Agent 2 logs (if multiple agents exist)
kubectl logs -n streamhouse pod/streamhouse-agent-xyz --tail=200 | grep -E "lease|epoch"

# Look for:
# - "Failed to acquire lease: Already held by X"
# - "Lease renewal failed: Stale epoch"
# - "Lost lease for partition X"
```

### Step 3: Check for Clock Skew

Verify system clocks are synchronized across agents:
```bash
# Check time on agent pods
for pod in $(kubectl get pods -n streamhouse -l app=streamhouse-agent -o name); do
  echo "=== $pod ==="
  kubectl exec -n streamhouse $pod -- date -u
done

# All times should be within 1-2 seconds
# If difference >5 seconds → clock skew issue
```

### Step 4: Review Lease Renewal Metrics

Check if renewal task is healthy:
```bash
# Lease renewal success rate
curl -s 'http://prometheus:9090/api/v1/query?query=rate(lease_renewals_total{result="success"}[5m])'

# Lease renewal failures
curl -s 'http://prometheus:9090/api/v1/query?query=rate(lease_renewals_total{result="failure"}[5m])'

# Expected: >95% success rate
# Problem: <80% success rate or failures increasing
```

### Step 5: Check Database Health

Verify PostgreSQL performance:
```bash
# Check active connections
psql -h YOUR_DB_HOST -U streamhouse -c "SELECT count(*) FROM pg_stat_activity WHERE datname = 'streamhouse';"

# Check long-running queries (potential locks)
psql -h YOUR_DB_HOST -U streamhouse -c "
SELECT pid, now() - query_start AS duration, query
FROM pg_stat_activity
WHERE state != 'idle'
ORDER BY duration DESC
LIMIT 10;
"

# Check for table locks on partition_leases
psql -h YOUR_DB_HOST -U streamhouse -c "
SELECT * FROM pg_locks
WHERE relation = 'partition_leases'::regclass;
"
```

---

## Resolution

### Option 1: Force Lease Reset (Emergency)

If partition is stuck with no valid owner:

**⚠️ Warning**: This **forcibly expires the current lease**. Only use if:
- Partition is completely unavailable
- Current lease holder is confirmed dead/unreachable
- Acceptable to cause brief write downtime

**Steps**:
```bash
# 1. Verify current lease holder is dead
kubectl get pods -n streamhouse -l app=streamhouse-agent
# Check if agent listed in lease exists

# 2. Force expire the lease in database
psql -h YOUR_DB_HOST -U streamhouse -d streamhouse -c "
UPDATE partition_leases
SET lease_expires_at = 0  -- Force expiration
WHERE topic = 'orders' AND partition_id = 0;
"

# 3. Wait 5 seconds for agents to detect

# 4. Check which agent acquired the lease
psql -h YOUR_DB_HOST -U streamhouse -d streamhouse -c "
SELECT agent_id, lease_epoch, lease_expires_at
FROM partition_leases
WHERE topic = 'orders' AND partition_id = 0;
"

# 5. Verify writes are working
cargo run --example producer_simple -- --topic orders --partition 0
```

**Recovery Time**: 5-15 seconds

---

### Option 2: Restart Conflicting Agent

If specific agent is misbehaving (e.g., renewing stale leases):

**Steps**:
```bash
# 1. Identify misbehaving agent from logs
kubectl logs -n streamhouse pod/streamhouse-agent-abc | grep "Failed to renew lease"

# 2. Delete the pod (Kubernetes will recreate it)
kubectl delete pod -n streamhouse streamhouse-agent-abc

# 3. New pod will start with clean state
kubectl logs -n streamhouse pod/streamhouse-agent-abc-NEW --follow | grep "lease"

# Expected: "Acquired lease for partition orders-0, epoch: 43"
```

**Benefit**: Clean slate for the agent, resolves cache inconsistencies

---

### Option 3: Fix Clock Skew

If time synchronization is the issue:

**Steps**:
```bash
# 1. Check NTP sync status on nodes
kubectl exec -n streamhouse pod/streamhouse-agent-abc -- chronyc tracking
# Look for "System time" offset

# 2. Force NTP sync on nodes (requires host access)
ssh node-1
sudo chronyc -a makestep  # Force time sync

# 3. Restart agents to pick up corrected time
kubectl rollout restart -n streamhouse deployment/streamhouse-agent

# 4. Verify times are aligned
for pod in $(kubectl get pods -n streamhouse -l app=streamhouse-agent -o name); do
  kubectl exec -n streamhouse $pod -- date +%s
done
# Should all be within 1-2 seconds
```

**Prevention**: Ensure NTP/chrony is running on all nodes

---

### Option 4: Increase Lease Duration (Frequent Renewals Failing)

If lease renewal failures are due to network hiccups or DB latency:

**Steps**:
```rust
// In lease_manager.rs
const DEFAULT_LEASE_DURATION_MS: i64 = 60_000;  // Increase to 60 seconds (was: 30s)
const LEASE_RENEWAL_INTERVAL: Duration = Duration::from_secs(20);  // Renew every 20s

// Redeploy agent
```

**Trade-off**:
- **Benefit**: More resilient to transient failures
- **Cost**: Slower failover (60s instead of 30s if agent crashes)

**Recommendation**: Use 30s for production (default), only increase if renewal failures >10%

---

### Option 5: Scale Down to Single Agent (Temporary)

If multi-agent coordination is failing, temporarily reduce to one agent:

**Steps**:
```bash
# 1. Scale to single replica
kubectl scale deployment/streamhouse-agent --replicas=1 -n streamhouse

# 2. Wait for single agent to stabilize
kubectl logs -n streamhouse deployment/streamhouse-agent --follow | grep "Acquired lease"

# 3. Verify all partitions owned by single agent
psql -h YOUR_DB_HOST -U streamhouse -c "SELECT DISTINCT agent_id FROM partition_leases;"
# Should show only one agent_id

# 4. Test writes
cargo run --example producer_simple

# 5. Gradually scale back up
kubectl scale deployment/streamhouse-agent --replicas=3 -n streamhouse
```

**When to use**: Emergency mitigation, not a long-term solution

---

### Option 6: Fix Database Connection Pool Exhaustion

If lease queries are timing out due to connection limits:

**Steps**:
```bash
# 1. Check current connection pool size in agent config
kubectl logs -n streamhouse deployment/streamhouse-agent | grep "connection pool"

# 2. Increase pool size (requires code change)
# In agent initialization:
PgPoolOptions::new()
    .max_connections(20)  // Increase from default 10
    .acquire_timeout(Duration::from_secs(5))
    .connect(database_url)
    .await?;

# 3. Redeploy agent
kubectl rollout restart -n streamhouse deployment/streamhouse-agent

# 4. Monitor connection usage
psql -h YOUR_DB_HOST -U streamhouse -c "
SELECT count(*), state
FROM pg_stat_activity
WHERE datname = 'streamhouse'
GROUP BY state;
"
```

---

## Verification

After resolution, verify lease conflicts are resolved:

### 1. Check Lease Ownership is Stable
```bash
# Watch lease table for 60 seconds (should NOT change)
watch -n 5 "psql -h YOUR_DB_HOST -U streamhouse -c '
SELECT topic, partition_id, agent_id, lease_epoch
FROM partition_leases
ORDER BY topic, partition_id;
'"

# Expected: No changes to agent_id or epoch (except epoch increments every 30s)
```

### 2. Monitor Lease Metrics
```bash
# Lease conflicts should drop to zero
curl -s 'http://prometheus:9090/api/v1/query?query=rate(lease_conflicts_total[5m])'
# Expected: 0

# Lease renewals should succeed
curl -s 'http://prometheus:9090/api/v1/query?query=rate(lease_renewals_total{result="success"}[5m])'
# Expected: >0.95 (95% success rate)
```

### 3. Test Producer Writes
```bash
# Send burst of messages to affected partition
for i in {1..100}; do
  cargo run --example producer_simple -- --topic orders --partition 0 &
done
wait

# Check success rate
# Expected: 100% success (all messages accepted)
```

### 4. Review Agent Logs
```bash
# Should see successful renewals, no conflicts
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=100 | grep -E "lease|epoch"

# Expected patterns:
# INFO  Renewed lease for partition orders-0, epoch: 42
# DEBUG Using cached lease for orders-0, epoch: 42
```

---

## Prevention

### 1. Enable Lease Health Monitoring

Set up alerts for lease conflicts:
```yaml
# Prometheus alert rule
- alert: PartitionLeaseConflicts
  expr: rate(lease_conflicts_total[5m]) > 0.01
  for: 2m
  annotations:
    summary: "Partition lease conflicts detected"
    description: "{{ $value }} conflicts/sec (indicates split-brain or clock skew)"

- alert: LeaseRenewalFailures
  expr: |
    sum(rate(lease_renewals_total{result="failure"}[5m])) /
    sum(rate(lease_renewals_total[5m])) > 0.10
  for: 5m
  annotations:
    summary: "Lease renewal failure rate >10%"
```

### 2. Configure NTP on All Nodes

Ensure time synchronization:
```yaml
# DaemonSet for NTP sync (example)
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: chrony
spec:
  template:
    spec:
      containers:
      - name: chrony
        image: chrony:latest
        securityContext:
          privileged: true  # Required for time sync
        volumeMounts:
        - name: host-time
          mountPath: /etc/localtime
      volumes:
      - name: host-time
        hostPath:
          path: /etc/localtime
```

### 3. Use Pod Anti-Affinity for Agents

Prevent agents from running on same node (reduces clock skew):
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: streamhouse-agent
spec:
  template:
    spec:
      affinity:
        podAntiAffinity:
          requiredDuringSchedulingIgnoredDuringExecution:
          - labelSelector:
              matchExpressions:
              - key: app
                operator: In
                values:
                - streamhouse-agent
            topologyKey: kubernetes.io/hostname
```

### 4. Implement Lease Pre-Check Before Writes

Add defensive check before every write:
```rust
// In PartitionWriter::append()
pub async fn append(&mut self, key: Option<Bytes>, value: Bytes) -> Result<u64> {
    // Ensure we still have valid lease
    let epoch = self.lease_manager.ensure_lease(&self.topic, self.partition_id).await?;

    // Verify epoch matches (fencing)
    if epoch != self.current_epoch {
        return Err(Error::StaleEpoch {
            current: epoch,
            attempted: self.current_epoch,
        });
    }

    // Proceed with write...
}
```

**Status**: Partially implemented (lease check exists, epoch fencing pending)

### 5. Use Database-Level Locks for Lease Acquisition

Strengthen lease acquisition with advisory locks:
```sql
-- In acquire_partition_lease() implementation
BEGIN;

-- Acquire advisory lock (prevents concurrent acquisitions)
SELECT pg_try_advisory_xact_lock(hashtext(CONCAT(topic, '-', partition_id::text)));

-- Check if lease is available or expired
SELECT * FROM partition_leases
WHERE topic = $1 AND partition_id = $2
FOR UPDATE;

-- Update lease if available
UPDATE partition_leases
SET agent_id = $3, lease_epoch = lease_epoch + 1, lease_expires_at = $4
WHERE topic = $1 AND partition_id = $2
  AND (lease_expires_at < $5 OR agent_id = $3);

COMMIT;  -- Automatically releases advisory lock
```

**Status**: Not yet implemented (Phase 12.5 enhancement)

### 6. Implement Graceful Shutdown with Lease Release

Ensure agents release leases on shutdown:
```rust
// In Agent::shutdown()
pub async fn shutdown(&mut self) -> Result<()> {
    info!("Graceful shutdown initiated");

    // Stop accepting new writes
    self.shutdown_signal.store(true, Ordering::SeqCst);

    // Release all held leases
    for (topic, partition_id) in self.lease_manager.held_leases().await {
        self.lease_manager.release_lease(&topic, partition_id).await?;
        info!("Released lease: {}-{}", topic, partition_id);
    }

    // Stop renewal task
    self.lease_manager.stop_renewal_task().await?;

    Ok(())
}
```

**Status**: Partially implemented (renewal task stop exists, explicit release pending)

---

## Related Runbooks

- [WAL Recovery Failures](./wal-recovery-failures.md) - Concurrent WAL access due to lease conflicts
- [Circuit Breaker Open](./circuit-breaker-open.md) - If database issues prevent lease acquisition
- [Resource Pressure](./resource-pressure.md) - CPU/memory pressure affects lease renewal timing

---

## Lease State Machine

```text
┌─────────────┐
│  NO LEASE   │ (Agent just started)
└──────┬──────┘
       │ acquire_lease()
       ▼
┌─────────────┐
│   ACTIVE    │ (Lease valid, epoch: N)
└──┬────────┬─┘
   │        │
   │        │ Background renewal (every 10s)
   │        └────> renew_lease(epoch: N) → Success → ACTIVE (epoch: N)
   │
   │ Lease expires (no renewal for 30s)
   ▼
┌─────────────┐
│  EXPIRED    │ (Lost ownership)
└──────┬──────┘
       │ Attempt write → Error: "No valid lease"
       │
       │ acquire_lease() → Conflict if another agent acquired it
       ▼
┌─────────────┐
│   CONFLICT  │ (Another agent owns partition)
└─────────────┘
```

---

## Metrics Reference

| Metric Name | Description | Healthy Value |
|-------------|-------------|---------------|
| `lease_acquisitions_total{result="success"}` | Successful lease acquisitions | Stable or increasing |
| `lease_acquisitions_total{result="conflict"}` | Failed acquisitions (conflict) | 0 |
| `lease_renewals_total{result="success"}` | Successful renewals | >95% |
| `lease_renewals_total{result="failure"}` | Failed renewals | <5% |
| `lease_conflicts_total` | Epoch conflicts detected | 0 |

---

## References

- **Code**: [lease_manager.rs](../../crates/streamhouse-agent/src/lease_manager.rs)
- **Tests**: [lease_coordination_test.rs](../../crates/streamhouse-agent/tests/lease_coordination_test.rs)
- **Design**: [Phase 4.2: Lease Management](../PHASE_4_MULTI_AGENT_COMPLETE.md)

---

**Last Updated**: January 31, 2026
**Version**: 1.0
