# Runbook: Memory and Disk Pressure

**Severity**: HIGH
**Component**: Agent / Storage Layer
**Impact**: Agent crashes (OOMKilled), disk full, slow performance, write failures

---

## Symptoms

### Memory Pressure

#### Agent Pod Crashes
- Pod status: `OOMKilled` (Out of Memory)
- Pod restarts frequently (CrashLoopBackOff)
- Memory usage approaching limit (>90%)

#### Agent Logs
```
WARN  High memory usage: 1.8GB / 2GB (90%)
ERROR Out of memory allocating segment buffer
FATAL Agent process killed by kernel (OOM)
```

#### System Behavior
- Slow response times
- Increased GC pauses (if applicable)
- Buffer allocation failures
- S3 upload retries (due to slow processing)

---

### Disk Pressure

#### Disk Full Errors
- WAL writes fail: `"No space left on device"`
- Agent cannot start: `"Failed to initialize WAL directory"`
- Logs truncated or missing

#### Agent Logs
```
ERROR WAL append failed: No space left on device
WARN  Disk usage: 19.5GB / 20GB (97.5%)
ERROR Failed to flush segment: Disk full
```

#### System Behavior
- WAL rotation stops
- Segments accumulate in memory (cannot flush to disk)
- Eventually triggers OOM as memory fills up

---

### CPU Pressure

#### High CPU Usage
- CPU usage >90% sustained
- Agent slow to respond to gRPC requests
- Lease renewal failures (timeouts)

#### Agent Logs
```
WARN  High CPU usage: 95% (4 cores saturated)
WARN  Lease renewal timeout: took 15s (expected <1s)
ERROR Producer timeout: append took 30s
```

#### System Behavior
- Slow request processing
- Increased latency
- Potential lease conflicts (renewals timeout)

---

## Root Causes

### Memory Pressure Causes

1. **Large Segment Buffers**: Segments grow too large before flushing to S3
2. **Too Many Partitions**: Agent assigned too many partitions, each with in-memory buffer
3. **Slow S3 Uploads**: Segments accumulate in memory waiting for S3 upload
4. **Schema Cache Growth**: Schema registry cache unbounded
5. **Memory Leak**: Bug causing memory not to be released
6. **Insufficient Memory Limits**: Kubernetes memory limit too low

### Disk Pressure Causes

1. **WAL Rotation Disabled**: Old WAL files not deleted after S3 upload
2. **Failed S3 Uploads**: Segments stuck in WAL, cannot truncate
3. **Log Accumulation**: Application logs filling disk
4. **PVC Too Small**: Persistent volume claim undersized for workload
5. **No Disk Monitoring**: Disk filled up without alerts

### CPU Pressure Causes

1. **High Write Throughput**: Too many writes for CPU capacity
2. **Compression Overhead**: CPU-intensive compression (e.g., Zstd)
3. **Schema Validation**: Expensive Avro validation on every message
4. **Concurrent Uploads**: Too many parallel S3 uploads
5. **Inefficient Serialization**: Slow encoding/decoding

---

## Investigation Steps

### Step 1: Check Pod Resource Usage

Monitor current resource consumption:
```bash
# Check pod resource usage
kubectl top pod -n streamhouse -l app=streamhouse-agent

# Expected output:
# NAME                    CPU(cores)   MEMORY(bytes)
# streamhouse-agent-abc   850m         1200Mi

# Warning signs:
# - CPU >3000m (3 cores) sustained
# - Memory >1800Mi (approaching 2Gi limit)

# Check for OOMKilled pods
kubectl get pods -n streamhouse -l app=streamhouse-agent -o jsonpath='{.items[*].status.containerStatuses[*].lastState.terminated.reason}'

# If output contains "OOMKilled" → memory pressure
```

### Step 2: Check Disk Usage

Verify disk space on agent pods:
```bash
# Check disk usage inside pod
kubectl exec -n streamhouse deployment/streamhouse-agent -- df -h

# Look for:
# Filesystem      Size  Used Avail Use% Mounted on
# /dev/sda1        20G   19G  500M  97% /data  ← Problem!

# Check WAL directory specifically
kubectl exec -n streamhouse deployment/streamhouse-agent -- du -sh /data/wal

# Expected: <2GB (should be truncated after S3 upload)
# Problem: >10GB (indicates failed truncation)
```

### Step 3: Review Resource Limits

Check Kubernetes resource configuration:
```bash
# Get pod resource limits
kubectl get pod -n streamhouse -l app=streamhouse-agent -o jsonpath='{.items[0].spec.containers[0].resources}'

# Expected output:
# {
#   "limits": {"cpu": "4", "memory": "2Gi"},
#   "requests": {"cpu": "2", "memory": "1Gi"}
# }

# Warning signs:
# - No limits set (can consume entire node)
# - Limits too low (< 1Gi memory, < 1 CPU)
```

### Step 4: Analyze Memory Breakdown

Identify what's consuming memory:
```bash
# Check agent metrics (if implemented)
curl -s http://localhost:9090/metrics | grep memory

# Look for:
# segment_buffer_size_bytes{topic="orders", partition="0"} 52428800  # 50MB
# schema_cache_entries 1523
# wal_size_bytes{topic="orders", partition="0"} 10485760  # 10MB

# Calculate total memory usage:
# Total = (num_partitions × avg_segment_buffer_size) + schema_cache + WAL
```

### Step 5: Check for Memory Leaks

Look for continuously increasing memory:
```bash
# Monitor memory over time (5 minute intervals)
kubectl top pod -n streamhouse streamhouse-agent-abc --no-headers | awk '{print $3}' > /tmp/mem.log
sleep 300
kubectl top pod -n streamhouse streamhouse-agent-abc --no-headers | awk '{print $3}' >> /tmp/mem.log

# Compare: If memory increased significantly without corresponding load → potential leak
```

### Step 6: Review Recent Changes

Check recent deployments or config changes:
```bash
# Check recent deployment rollouts
kubectl rollout history -n streamhouse deployment/streamhouse-agent

# Check recent config changes
kubectl describe configmap -n streamhouse streamhouse-config

# Look for:
# - Increased partition count
# - Larger segment sizes
# - Changed compression settings
```

---

## Resolution

### Memory Pressure Solutions

#### Option 1: Increase Memory Limits (Quick Fix)

If workload legitimately needs more memory:
```yaml
# Update deployment
apiVersion: apps/v1
kind: Deployment
metadata:
  name: streamhouse-agent
spec:
  template:
    spec:
      containers:
      - name: agent
        resources:
          requests:
            memory: "2Gi"  # Was: 1Gi
            cpu: "2"
          limits:
            memory: "4Gi"  # Was: 2Gi
            cpu: "4"
```

Apply changes:
```bash
kubectl apply -f agent-deployment.yaml
kubectl rollout status -n streamhouse deployment/streamhouse-agent
```

**When to use**: Memory usage is legitimately high due to workload growth

---

#### Option 2: Reduce Segment Buffer Size

If segments are too large:
```rust
// In WriteConfig
pub segment_max_size: usize = 50 * 1024 * 1024,  // Reduce to 50MB (was: 100MB)
pub segment_max_age_ms: u64 = 30000,  // Flush more frequently (30s instead of 60s)
```

Redeploy:
```bash
kubectl rollout restart -n streamhouse deployment/streamhouse-agent
```

**Trade-off**:
- **Benefit**: Lower memory usage (smaller buffers)
- **Cost**: More frequent S3 uploads (higher S3 request rate)

**Recommendation**: Balance segment size with S3 rate limits (target 10-100 uploads/minute)

---

#### Option 3: Reduce Partition Assignment

If agent has too many partitions:
```bash
# Check current partition assignment
psql -h YOUR_DB_HOST -U streamhouse -c "
SELECT agent_id, COUNT(*) AS partition_count
FROM partition_leases
GROUP BY agent_id;
"

# If one agent has >100 partitions → rebalance

# Scale up agents to distribute load
kubectl scale deployment/streamhouse-agent --replicas=3 -n streamhouse

# Partitions will rebalance automatically (lease expiration + acquisition)
```

**Rule of thumb**: Max 50 partitions per agent (depends on segment size)

---

#### Option 4: Implement Schema Cache Eviction

If schema cache is unbounded:
```rust
// In SchemaRegistry initialization
use moka::future::Cache;

let schema_cache = Cache::builder()
    .max_capacity(10_000)  // Limit to 10k schemas
    .time_to_live(Duration::from_secs(3600))  // Evict after 1 hour
    .build();
```

**Status**: Already implemented (Moka cache with 1-hour TTL)

---

### Disk Pressure Solutions

#### Option 1: Increase PVC Size (Quick Fix)

If disk is too small:
```bash
# 1. Check current PVC size
kubectl get pvc -n streamhouse

# 2. Resize PVC (requires StorageClass with allowVolumeExpansion: true)
kubectl patch pvc streamhouse-wal -n streamhouse -p '{"spec":{"resources":{"requests":{"storage":"50Gi"}}}}'

# 3. Wait for resize
kubectl get pvc -n streamhouse streamhouse-wal -w

# 4. Restart pod to pick up new size
kubectl rollout restart -n streamhouse deployment/streamhouse-agent
```

**Note**: Some storage classes don't support online resizing (requires pod restart)

---

#### Option 2: Clean Up Old WAL Files

If WAL rotation is broken:
```bash
# 1. Check WAL files
kubectl exec -n streamhouse deployment/streamhouse-agent -- ls -lh /data/wal

# 2. Identify old WAL files (already uploaded to S3)
# Safe to delete WAL if last_uploaded_offset > WAL_max_offset

# 3. Manually delete old WAL (emergency)
kubectl exec -n streamhouse deployment/streamhouse-agent -- rm /data/wal/orders-0.wal.old

# 4. Implement WAL truncation after S3 upload (code fix)
# In PartitionWriter::upload_to_s3():
async fn upload_to_s3(&mut self, segment: Segment) -> Result<()> {
    // Upload segment
    self.object_store.put(&key, segment_data).await?;

    // Truncate WAL after successful upload
    self.wal.truncate().await?;  // ← Ensure this is called

    Ok(())
}
```

---

#### Option 3: Enable Log Rotation

If application logs fill disk:
```yaml
# Configure log rotation in pod spec
apiVersion: v1
kind: Pod
spec:
  containers:
  - name: agent
    env:
    - name: RUST_LOG
      value: "info"  # Reduce log verbosity (was: debug)
```

Or use Kubernetes log rotation:
```yaml
# In kubelet config (node-level)
containerLogMaxSize: 10Mi
containerLogMaxFiles: 5
```

---

### CPU Pressure Solutions

#### Option 1: Scale Horizontally

Add more agent replicas:
```bash
# Scale to 3 replicas
kubectl scale deployment/streamhouse-agent --replicas=3 -n streamhouse

# Each agent handles fewer partitions → lower CPU per agent
```

---

#### Option 2: Reduce Compression Level

If compression is CPU-intensive:
```rust
// In WriteConfig
pub compression: CompressionType = CompressionType::Snappy,  // Fast (was: Zstd)

// Or disable compression entirely (not recommended)
pub compression: CompressionType = CompressionType::None,
```

**Trade-off**:
- **Benefit**: Lower CPU usage
- **Cost**: Larger S3 storage, higher network bandwidth

---

#### Option 3: Optimize Hot Paths

Profile and optimize CPU-intensive code:
```bash
# Use perf or flamegraph to identify hot spots
cargo install flamegraph
cargo flamegraph --bin agent

# Common hot paths:
# - Serialization/deserialization
# - Compression
# - Schema validation
```

---

## Verification

After resolution, verify resource usage is healthy:

### 1. Monitor Resource Metrics
```bash
# Watch resource usage over 5 minutes
watch -n 10 "kubectl top pod -n streamhouse -l app=streamhouse-agent"

# Expected:
# - CPU: <70% of limit
# - Memory: <75% of limit
```

### 2. Check Disk Usage Stabilizes
```bash
# Disk should not grow continuously
watch -n 30 "kubectl exec -n streamhouse deployment/streamhouse-agent -- df -h /data"

# Expected: Usage stable or decreasing (WAL truncation working)
```

### 3. Verify No OOMKills
```bash
# Check for OOMKills in last hour
kubectl get events -n streamhouse --field-selector reason=OOMKilled --sort-by='.lastTimestamp'

# Expected: No events
```

### 4. Test Write Throughput
```bash
# Ensure performance is acceptable
cargo run --example producer_burst -- --count=10000

# Measure latency: p99 should be <500ms
```

---

## Prevention

### 1. Set Resource Requests and Limits

Always define resource constraints:
```yaml
resources:
  requests:
    memory: "2Gi"  # Guaranteed allocation
    cpu: "2"
  limits:
    memory: "4Gi"  # Hard limit (OOMKill if exceeded)
    cpu: "4"       # Throttled if exceeded
```

**Recommendations**:
- Memory limit = 2× request (allows burst)
- CPU limit = 2× request (allows burst)

### 2. Enable Resource Monitoring

Set up alerts for resource pressure:
```yaml
# Prometheus alert rules
- alert: HighMemoryUsage
  expr: (container_memory_usage_bytes / container_spec_memory_limit_bytes) > 0.85
  for: 5m
  annotations:
    summary: "Agent memory usage >85%"

- alert: DiskSpaceLow
  expr: (node_filesystem_avail_bytes{mountpoint="/data"} / node_filesystem_size_bytes{mountpoint="/data"}) < 0.15
  for: 5m
  annotations:
    summary: "Disk space <15%"

- alert: HighCPUUsage
  expr: (rate(container_cpu_usage_seconds_total[5m])) > 3.5
  for: 10m
  annotations:
    summary: "Agent CPU usage >3.5 cores"
```

### 3. Implement Graceful Degradation

Handle resource pressure gracefully:
```rust
// Check available memory before allocating large buffers
let available_memory = get_available_memory();
if available_memory < required_memory {
    warn!("Low memory, flushing segment early");
    self.flush_segment().await?;
}
```

### 4. Use Horizontal Pod Autoscaling (HPA)

Automatically scale based on resource usage:
```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: streamhouse-agent-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: streamhouse-agent
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70  # Scale up if CPU >70%
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 75  # Scale up if memory >75%
```

### 5. Implement WAL Cleanup Job

Periodic cleanup of orphaned WAL files:
```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: wal-cleanup
spec:
  schedule: "0 */6 * * *"  # Every 6 hours
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: cleanup
            image: streamhouse-agent:latest
            command:
            - /bin/sh
            - -c
            - |
              # Delete WAL files older than 24 hours
              find /data/wal -name "*.wal" -mtime +1 -delete
          restartPolicy: OnFailure
```

### 6. Size PVCs Appropriately

Calculate PVC size based on expected workload:
```
PVC Size = (peak_write_rate × flush_interval × safety_factor)

Example:
- Peak write rate: 100 MB/s
- Flush interval: 60 seconds (worst case if S3 slow)
- Safety factor: 10× (for retries, backlog)

PVC Size = 100 MB/s × 60s × 10 = 60 GB
```

**Recommendation**: Start with 50 GB, monitor actual usage, adjust

---

## Related Runbooks

- [WAL Recovery Failures](./wal-recovery-failures.md) - Disk full prevents WAL writes
- [Circuit Breaker Open](./circuit-breaker-open.md) - Slow S3 uploads cause memory accumulation
- [Partition Lease Conflicts](./partition-lease-conflicts.md) - CPU pressure causes lease renewal timeouts

---

## Resource Sizing Guidelines

### Memory Sizing

```
Total Memory = (num_partitions × segment_buffer_size) + overhead

Example:
- 50 partitions
- 100 MB segment buffer each
- 500 MB overhead (schema cache, connection pools, etc.)

Total = (50 × 100 MB) + 500 MB = 5.5 GB

→ Set memory limit to 8 GB (1.5× safety factor)
```

### Disk Sizing

```
Disk Size = (num_partitions × wal_max_size) + logs

Example:
- 50 partitions
- 200 MB WAL max size each
- 10 GB for logs

Total = (50 × 200 MB) + 10 GB = 20 GB

→ Set PVC to 50 GB (2.5× safety factor)
```

### CPU Sizing

```
CPU Cores = (peak_write_throughput / per_core_throughput)

Example:
- Peak write: 500 MB/s
- Per-core throughput: 200 MB/s (with compression)

CPU = 500 / 200 = 2.5 cores

→ Set CPU limit to 4 cores (1.6× safety factor)
```

---

## References

- **Kubernetes Docs**: [Resource Management](https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/)
- **Code**: [writer.rs](../../crates/streamhouse-storage/src/writer.rs) (segment buffer management)
- **Code**: [wal.rs](../../crates/streamhouse-storage/src/wal.rs) (WAL disk usage)

---

**Last Updated**: January 31, 2026
**Version**: 1.0
