# Runbook: WAL Recovery Failures

**Severity**: HIGH
**Component**: Storage Layer (WAL)
**Impact**: Agent fails to start, unflushed data may be lost, partitions unavailable

---

## Symptoms

### Agent Startup Failures
- Agent crashes or hangs during startup
- Error message: `"Failed to recover WAL"` or `"WAL corruption detected"`
- Agent restarts in a loop (CrashLoopBackOff in Kubernetes)

### Agent Logs
```
ERROR Failed to recover WAL for partition orders-0: CRC mismatch at offset 1024
ERROR WAL file corrupted: Unexpected EOF at record 42
WARN  Skipping corrupted WAL record at offset 2048
INFO  WAL recovery: 1523 records recovered, 12 records skipped (corruption)
```

### Impact
- **Partition unavailable**: Affected partition cannot accept writes until recovery completes
- **Data loss risk**: If WAL is truncated/deleted, unflushed records are lost
- **Increased recovery time**: Agent startup delayed by seconds to minutes

---

## Root Causes

WAL recovery can fail due to:

1. **File Corruption**: Disk errors, incomplete writes, or crashes during fsync
2. **CRC Mismatch**: Record checksums don't match (partial writes, bit flips)
3. **Unexpected EOF**: WAL file truncated mid-record
4. **Disk Full**: WAL writes failed silently due to no disk space
5. **Permission Issues**: WAL directory not writable after deployment/config change
6. **Concurrent Access**: Multiple agent instances writing to same WAL (lease conflicts)

---

## Investigation Steps

### Step 1: Check Agent Logs for WAL Errors

Identify the specific WAL failure:
```bash
# Check recent logs for WAL errors
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=200 | grep -i "wal"

# Look for:
# - "Failed to recover WAL for partition X"
# - "CRC mismatch at offset Y"
# - "WAL file corrupted"
# - "Skipping corrupted WAL record"
```

Expected failure patterns:
```
[2026-01-31T11:00:15Z] INFO  Starting WAL recovery for orders-0
[2026-01-31T11:00:16Z] ERROR WAL recovery failed: CRC mismatch at offset 4096
[2026-01-31T11:00:16Z] ERROR Expected CRC: 0xdeadbeef, actual: 0x12345678
[2026-01-31T11:00:16Z] INFO  WAL stats: 2048 bytes read, 128 records recovered, 1 corrupted
```

### Step 2: Locate WAL Files

Find the problematic WAL file(s):
```bash
# SSH/exec into agent pod
kubectl exec -it -n streamhouse deployment/streamhouse-agent -- bash

# List WAL files
ls -lh /data/wal/

# Expected structure:
# /data/wal/orders-0.wal
# /data/wal/orders-1.wal
# /data/wal/inventory-0.wal

# Check file sizes and timestamps
ls -lh /data/wal/ | grep -E "orders-0"
```

### Step 3: Inspect WAL File Health

Check for obvious corruption indicators:
```bash
# Check if file is accessible
cat /data/wal/orders-0.wal > /dev/null
echo $?  # Should be 0 (success)

# Check disk space
df -h /data

# Check file permissions
ls -la /data/wal/orders-0.wal
# Expected: -rw-r--r-- 1 streamhouse streamhouse

# Check for disk errors (on host)
dmesg | grep -i "i/o error"
```

### Step 4: Review Recent Events

Determine when corruption occurred:
```bash
# Check agent restart history
kubectl get pods -n streamhouse -o wide | grep streamhouse-agent

# Check for node reboots or crashes
kubectl get events -n streamhouse --sort-by='.lastTimestamp' | grep -i "oomkilled\|evicted\|node"

# Check for disk pressure
kubectl describe node <node-name> | grep -i "disk\|pressure"
```

### Step 5: Estimate Data Loss Risk

Determine how much unflushed data is in WAL:
```bash
# Check last S3 upload timestamp for partition
aws s3 ls s3://your-bucket/orders/partition-0/ --recursive | tail -5

# Compare to WAL file modification time
stat /data/wal/orders-0.wal

# Gap = potential data loss window
# Example: Last S3 upload at 10:55:30, WAL modified at 10:56:45 → 75 seconds of unflushed data
```

---

## Resolution

### Option 1: Automatic Recovery with Corruption Skipping (Default)

The WAL implementation has **built-in corruption handling**:

1. On CRC mismatch: Skip corrupted record, continue with next valid record
2. On unexpected EOF: Stop recovery at truncation point
3. Log all skipped records for audit trail

**Action**: Let agent attempt auto-recovery. Monitor logs:
```bash
kubectl logs -n streamhouse deployment/streamhouse-agent --follow | grep "WAL recovery"

# Expected success output:
# INFO WAL recovery completed: 1523 records recovered
# WARN 12 records skipped due to corruption (1.2% data loss)
# INFO Partition orders-0 ready for writes
```

**Data Loss**: Only corrupted records are lost (typically <1% of unflushed data)

**When to use**:
- Corruption is isolated to a few records
- Auto-recovery completes successfully
- Acceptable data loss (<5% of unflushed data)

---

### Option 2: Manual WAL Truncation (Severe Corruption)

If auto-recovery fails completely (e.g., file header corrupted):

**⚠️ Warning**: This **deletes the entire WAL file** and loses all unflushed data. Only use if:
- Auto-recovery is stuck or crashing
- WAL file is completely unreadable
- Acceptable to lose unflushed data (last few seconds/minutes)

**Steps**:
```bash
# 1. Stop agent to prevent concurrent access
kubectl scale deployment/streamhouse-agent --replicas=0 -n streamhouse

# 2. Backup corrupted WAL (for forensics)
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  cp /data/wal/orders-0.wal /tmp/orders-0.wal.backup

# 3. Delete corrupted WAL file
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  rm /data/wal/orders-0.wal

# 4. Restart agent (creates new WAL)
kubectl scale deployment/streamhouse-agent --replicas=1 -n streamhouse

# 5. Verify partition is healthy
kubectl logs -n streamhouse deployment/streamhouse-agent | grep "orders-0"
# Expected: "Partition orders-0 initialized (WAL created)"
```

**Data Loss**: **All unflushed data lost** (typically last 5-60 seconds before crash)

---

### Option 3: Restore from S3 Checkpoint + Replay WAL

If WAL is corrupted but S3 data is intact, rebuild state:

**Workflow**:
1. Start agent with empty WAL (creates new partition state from S3)
2. Agent scans S3 to find last committed offset
3. Producer resumes from last committed offset (may re-send duplicates)

**Steps**:
```bash
# 1. Stop agent
kubectl scale deployment/streamhouse-agent --replicas=0 -n streamhouse

# 2. Rename WAL (preserve for investigation)
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  mv /data/wal/orders-0.wal /data/wal/orders-0.wal.old

# 3. Start agent (rebuilds from S3)
kubectl scale deployment/streamhouse-agent --replicas=1 -n streamhouse

# 4. Monitor rebuild
kubectl logs -n streamhouse deployment/streamhouse-agent --follow | grep "S3 checkpoint"
# Expected: "Loaded 42 segments from S3 for partition orders-0"
```

**Data Loss**: None (S3 data is authoritative source)

**Trade-off**: Potential duplicates if producer retries messages

---

### Option 4: Force Fsync on Healthy Partitions

If only one partition's WAL is corrupted, preserve others:

**Steps**:
```bash
# 1. Identify healthy partitions
kubectl exec -n streamhouse deployment/streamhouse-agent -- ls -l /data/wal/

# 2. Manually fsync healthy WALs before restart
kubectl exec -n streamhouse deployment/streamhouse-agent -- sh -c '
  for wal in /data/wal/*.wal; do
    sync $wal || echo "Failed to sync $wal"
  done
'

# 3. Delete only corrupted WAL
kubectl exec -n streamhouse deployment/streamhouse-agent -- rm /data/wal/orders-0.wal

# 4. Restart agent
kubectl rollout restart -n streamhouse deployment/streamhouse-agent
```

**Benefit**: Minimize data loss by preserving healthy WALs

---

### Option 5: Repair Partially Corrupted WAL (Advanced)

For advanced users: manually extract valid records from corrupted WAL.

**Requirements**:
- Understanding of WAL binary format (see [wal.rs](../../crates/streamhouse-storage/src/wal.rs))
- Custom recovery script

**Steps**:
```bash
# 1. Copy WAL to local machine
kubectl cp streamhouse/streamhouse-agent-xyz:/data/wal/orders-0.wal ./corrupted.wal

# 2. Use recovery script (example - requires implementation)
cargo run --bin wal-repair -- \
  --input ./corrupted.wal \
  --output ./repaired.wal \
  --skip-corrupted

# 3. Upload repaired WAL back to pod
kubectl cp ./repaired.wal streamhouse/streamhouse-agent-xyz:/data/wal/orders-0.wal

# 4. Restart agent
kubectl rollout restart -n streamhouse deployment/streamhouse-agent
```

**Status**: Not yet implemented (requires custom tooling)

---

## Verification

After recovery, verify partition is healthy:

### 1. Check Agent Startup
```bash
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=50 | grep "orders-0"

# Expected success patterns:
# INFO WAL recovery completed: 1523 records recovered
# INFO Partition orders-0 ready for writes
# INFO Acquired lease for partition orders-0
```

### 2. Test Producer Writes
```bash
# Send test message to affected partition
cargo run --example producer_simple -- --topic orders --partition 0

# Expected: Success, offset returned
```

### 3. Verify Data Integrity
```bash
# Consume from partition and check for gaps
cargo run --example consumer_simple -- --topic orders --partition 0

# Check if offsets are continuous or have gaps (data loss indicator)
```

### 4. Monitor WAL Health
```bash
# Check new WAL file is being written
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  watch -n 1 "ls -lh /data/wal/orders-0.wal"

# File size should increase as writes happen
```

---

## Prevention

### 1. Configure Proper Sync Policy

Balance durability vs performance:
```rust
// In WALConfig
pub sync_policy: SyncPolicy::Interval {
    interval: Duration::from_millis(100)  // Sync every 100ms
}

// Trade-offs:
// - SyncPolicy::Always → Safest, ~10x slower
// - SyncPolicy::Interval(100ms) → Balanced (default)
// - SyncPolicy::Never → Fastest, data loss on crash
```

**Recommendation**: Use `Interval(100ms)` for production (default)

### 2. Use Durable Storage for WAL

Ensure WAL directory is on durable disk:
```yaml
# Kubernetes PVC configuration
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: streamhouse-wal
spec:
  accessModes: ["ReadWriteOnce"]
  resources:
    requests:
      storage: 20Gi
  storageClassName: gp3  # Use SSDs with replication
```

**Avoid**: EmptyDir or hostPath (ephemeral, lost on pod restart)

### 3. Monitor Disk Space

Set up alerts for WAL disk usage:
```yaml
# Prometheus alert
- alert: WALDiskSpaceLow
  expr: (node_filesystem_avail_bytes{mountpoint="/data"} / node_filesystem_size_bytes{mountpoint="/data"}) < 0.10
  for: 5m
  annotations:
    summary: "WAL disk space <10% (WAL writes may fail)"
```

### 4. Enable WAL Rotation

Prevent unbounded WAL growth:
```rust
// In WALConfig
pub max_size_bytes: 1024 * 1024 * 1024  // 1GB max WAL file

// On reaching limit:
// - Create new WAL file (e.g., orders-0.wal.1)
// - Delete old WAL after S3 upload completes
```

**Status**: Not yet implemented (Phase 11 WAL enhancements)

### 5. Implement WAL Checkpoints

Regularly flush WAL to S3 and truncate:
```rust
// After segment flush to S3
wal.truncate().await?;  // Clear WAL (all data now in S3)

// Recommended checkpoint frequency: Every 60 seconds or 10MB
```

**Current implementation**: Manual truncate after segment flush (already implemented)

### 6. Set Up Periodic Health Checks

Proactively detect WAL issues:
```bash
# Cron job to check WAL health
kubectl exec -n streamhouse deployment/streamhouse-agent -- sh -c '
  for wal in /data/wal/*.wal; do
    if ! head -c 1024 $wal > /dev/null 2>&1; then
      echo "WAL health check failed: $wal"
    fi
  done
'
```

---

## Related Runbooks

- [Circuit Breaker Open](./circuit-breaker-open.md) - If S3 failures prevent WAL flush
- [Partition Lease Conflicts](./partition-lease-conflicts.md) - Concurrent WAL access causes corruption
- [Memory/Disk Pressure](./resource-pressure.md) - Disk full prevents WAL writes

---

## Data Loss Estimation

### Typical Data Loss Scenarios

| Scenario | WAL State | Data Loss |
|----------|-----------|-----------|
| Single record CRC mismatch | 99% intact | <0.1% (1-10 records) |
| Partial file corruption | 50% intact | 1-5% (10-100 records) |
| Complete WAL deletion | Empty | 100% unflushed (last 5-60s) |
| WAL truncated mid-record | Truncated | 10-50% (partial segment) |
| S3 checkpoint + WAL corrupt | S3 OK, WAL bad | 0% (rebuild from S3) |

### Recovery Time Estimates

| Recovery Method | Duration | Data Loss |
|----------------|----------|-----------|
| Auto-recovery (skip corrupted) | 1-10 seconds | <5% unflushed |
| Manual WAL truncation | 30-60 seconds | 100% unflushed |
| Rebuild from S3 | 1-5 minutes | 0% (duplicates possible) |
| Manual WAL repair | 10-30 minutes | Minimal |

---

## Diagnostic Commands

### Check WAL File Integrity
```bash
# CRC32 check (requires custom tool)
cargo run --bin wal-verify -- /data/wal/orders-0.wal

# Basic readability test
dd if=/data/wal/orders-0.wal of=/dev/null bs=4096
echo $?  # 0 = readable, 1 = I/O error
```

### Analyze WAL Contents
```bash
# Dump WAL records (requires custom tool)
cargo run --bin wal-dump -- /data/wal/orders-0.wal | head -20

# Expected output:
# Record 0: offset=0, crc=0xdeadbeef, key_size=12, value_size=128
# Record 1: offset=152, crc=0x12345678, key_size=8, value_size=256
```

### Monitor WAL Growth
```bash
# Watch WAL file size (should increase during writes)
watch -n 1 "ls -lh /data/wal/*.wal"

# Check WAL write rate
iostat -x 1 | grep sda  # Assuming WAL on /dev/sda
```

---

## References

- **Code**: [wal.rs](../../crates/streamhouse-storage/src/wal.rs)
- **Tests**: [wal_test.rs](../../crates/streamhouse-storage/tests/wal_test.rs)
- **Design**: [Phase 10: WAL Implementation](../PHASE_10_WAL_COMPLETE.md)

---

**Last Updated**: January 31, 2026
**Version**: 1.0
