# Runbook: High S3 Throttling Rate

**Severity**: MEDIUM
**Component**: Storage Layer (RateLimiter)
**Impact**: Producers receive `RESOURCE_EXHAUSTED` errors, backpressure applied

---

## Symptoms

### Producer Side
- Producers receive gRPC `RESOURCE_EXHAUSTED (8)` status errors
- Error message: `"S3 rate limit exceeded - please slow down"`
- Requests succeed initially, then get rejected during bursts
- Producers should implement exponential backoff

### Agent Logs
```
WARN S3 rate limited - rejecting with backpressure
WARN Rate limiter: No tokens available for PUT operation
INFO Adaptive rate reduced: 3000.0 -> 1500.0 ops/sec
```

### Monitoring
- Metric `throttle_decisions_total{decision="rate_limited"}` increasing
- Metric `throttle_rate_current{operation="put"}` decreasing (adaptive backoff)
- Producer throughput drops due to backpressure

---

## Root Causes

The rate limiter rejects requests when **token bucket is empty**. Common causes:

1. **Burst Traffic**: Producer sends large batch faster than sustained rate limit
2. **Misconfigured Limits**: Rate limits set too low for actual workload
3. **S3 Throttling Feedback**: Adaptive rate adjustment reduced limits after 503 errors
4. **Multiple Agents**: Competing agents sharing same S3 prefix (hitting per-prefix limits)
5. **Insufficient Burst Capacity**: Burst size too small for traffic patterns

---

## Investigation Steps

### Step 1: Check Current Rate Limiter State

View current token bucket state and rates:
```bash
# Check agent logs for rate limiter status
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=100 | grep -i "rate"

# Look for:
# - "Rate limiter: No tokens available"
# - "Adaptive rate reduced: X -> Y"
# - "Current PUT rate: X ops/sec"
```

Expected patterns:
```
[2026-01-31T10:20:15Z] WARN Rate limiter: No tokens available for PUT operation
[2026-01-31T10:20:15Z] INFO Current rates: PUT=2500.0, GET=5000.0, DELETE=3000.0
[2026-01-31T10:20:20Z] INFO Adaptive rate reduced: 2500.0 -> 1250.0 ops/sec (after 503)
```

### Step 2: Review Throttling Metrics

Check throttle decision breakdown:
```bash
# Rate limited requests (last 5 minutes)
curl -s 'http://prometheus:9090/api/v1/query?query=rate(throttle_decisions_total{decision="rate_limited"}[5m])'

# Allow vs rate_limited ratio
curl -s 'http://prometheus:9090/api/v1/query?query=
  sum(rate(throttle_decisions_total{decision="allow"}[5m])) /
  sum(rate(throttle_decisions_total[5m]))'

# Expected healthy ratio: >0.95 (>95% allowed)

# Current throttle rate
curl -s 'http://prometheus:9090/api/v1/query?query=throttle_rate_current{operation="put"}'
```

### Step 3: Check Producer Behavior

Identify if throttling is due to burst traffic or sustained overload:
```bash
# Producer throughput (messages/sec)
curl -s 'http://prometheus:9090/api/v1/query?query=rate(producer_messages_sent_total[1m])'

# Compare to configured rate limit (default: 3000/sec)
# If producer rate > configured limit → sustained overload
# If producer rate < configured limit → burst issue
```

### Step 4: Review S3 Actual Error Rate

Determine if adaptive rate adjustment is working:
```bash
# S3 503 SlowDown errors (should be near zero if throttling is working)
curl -s 'http://prometheus:9090/api/v1/query?query=rate(s3_errors_total{error_type="503"}[5m])'

# Expected: <0.001 (less than 0.1% of requests)

# If S3 errors are still high → rate limits may be too high
# If S3 errors are zero → rate limits may be too conservative
```

### Step 5: Identify Traffic Patterns

Analyze request distribution to understand burst behavior:
```bash
# View request histogram (1-minute buckets)
curl -s 'http://prometheus:9090/api/v1/query_range?query=rate(s3_requests_total{operation="PUT"}[1m])&start=1h&step=60s'

# Look for:
# - Spike patterns (sudden bursts)
# - Sustained plateaus (steady overload)
# - Periodic waves (batch processing)
```

---

## Resolution

### Option 1: Wait for Burst to Complete (Temporary Spike)

If throttling is caused by a **short burst** (e.g., initial load, batch import):

**Action**: No intervention needed. The rate limiter will:
1. Reject excess requests during burst (backpressure)
2. Refill tokens continuously (e.g., 3000 tokens/second)
3. Return to normal operation once burst completes

**Timeline**: Recovery in 1-10 seconds after burst ends

**Verification**:
```bash
# Monitor token refill (rate_limited count should stop increasing)
kubectl logs -n streamhouse deployment/streamhouse-agent --follow | grep "rate limited"

# Should see decreasing frequency of "No tokens available" messages
```

**When to use**:
- Throttle rate <10% of requests
- Duration <30 seconds
- Infrequent occurrence (<1/hour)

---

### Option 2: Increase Rate Limits (Sustained Overload)

If throttling is **sustained** (>30 seconds) and workload is legitimate:

#### Option 2a: Increase PUT Rate Limit

**⚠️ Warning**: Only increase if your S3 bucket can handle it (check S3 CloudWatch metrics first)

```bash
# Current default: 3000 PUT/sec
# S3 per-prefix limit: 3500 PUT/sec

# Increase to 3200/sec (still 9% headroom)
kubectl set env deployment/streamhouse-agent THROTTLE_PUT_RATE=3200

# Restart to apply
kubectl rollout restart -n streamhouse deployment/streamhouse-agent
```

**Recommendation**: Increase in **small increments** (200-500/sec) and monitor S3 error rate.

#### Option 2b: Increase Burst Capacity

If traffic is bursty but average rate is acceptable:
```bash
# Increase burst capacity from default (default burst = rate)
# This requires code change in ThrottleConfig

# In crates/streamhouse-storage/src/throttle.rs:
BucketConfig {
    rate: 3000.0,
    burst: 5000,  // Allow 5000 tokens in burst (was 3000)
    min_rate: 1.0,
    max_rate: 10000.0,
}

# Redeploy agent
```

**Benefit**: Handles larger bursts without rejecting requests

---

### Option 3: Scale Horizontally (Multiple Agents)

If single agent can't handle load, distribute across multiple agents:

**Architecture**:
```
Producer 1 ──→ Agent 1 (partitions 0-3) ──→ S3 prefix: /a7/
Producer 2 ──→ Agent 2 (partitions 4-7) ──→ S3 prefix: /3f/
Producer 3 ──→ Agent 3 (partitions 8-11) ──→ S3 prefix: /9b/
```

**Steps**:
```bash
# Scale agent deployment
kubectl scale deployment/streamhouse-agent --replicas=3 -n streamhouse

# Configure partition assignment per agent (requires sharding logic)
# Agent 1: Partitions 0-3
# Agent 2: Partitions 4-7
# Agent 3: Partitions 8-11
```

**Benefit**: Each agent gets independent rate limits (3000/sec × 3 = 9000/sec total)

**Requirements**:
- Implement S3 prefix sharding (distribute load across prefixes)
- Partition assignment coordination (each partition handled by one agent)

---

### Option 4: Implement S3 Prefix Sharding

If hitting **per-prefix S3 limits** (3500 PUT/sec), distribute across prefixes:

**Current structure** (all writes to same prefix):
```
s3://bucket/orders/partition-0/segment-1.log   ← Same prefix
s3://bucket/orders/partition-0/segment-2.log   ← Same prefix
s3://bucket/orders/partition-1/segment-1.log   ← Same prefix
```

**Sharded structure** (hash-based distribution):
```
s3://bucket/a7/orders/partition-0/segment-1.log  ← Prefix: a7
s3://bucket/3f/orders/partition-0/segment-2.log  ← Prefix: 3f
s3://bucket/9b/orders/partition-1/segment-1.log  ← Prefix: 9b
```

**Implementation** (requires code change):
```rust
// In PartitionWriter::upload_to_s3()
let prefix = hash_shard(segment_id);  // Compute hash prefix
let key = format!("{}/{}/{}/segment-{}.log", prefix, topic, partition, segment_id);
```

**Benefit**: Each prefix gets separate 3500 PUT/sec limit (10 prefixes = 35,000 PUT/sec)

**Status**: Not yet implemented (Phase 12.4.5 stretch goal)

---

### Option 5: Tune Adaptive Rate Adjustment (If Overaggressive)

If rate limiter is reducing limits **too aggressively** after 503 errors:

**Current behavior**:
- On 503: rate ← rate × 0.5 (50% reduction)
- On success: rate ← rate × 1.05 (5% increase)

**Tuning** (requires code change):
```rust
// In crates/streamhouse-storage/src/rate_limiter.rs
impl TokenBucket {
    pub async fn report_throttled(&self) {
        // Less aggressive backoff (75% instead of 50%)
        let new_rate = current_rate * 0.75;  // Was: 0.5
        // ...
    }

    pub async fn report_success(&self) {
        // Faster recovery (10% instead of 5%)
        let new_rate = current_rate * 1.10;  // Was: 1.05
        // ...
    }
}
```

**Trade-off**: Faster recovery but higher risk of repeated S3 throttling

---

## Verification

After resolution, verify throttling rate is acceptable:

### 1. Check Throttle Decision Ratio
```bash
# Should be >95% allowed
curl -s 'http://prometheus:9090/api/v1/query?query=
  sum(rate(throttle_decisions_total{decision="allow"}[5m])) /
  sum(rate(throttle_decisions_total[5m]))' | jq '.data.result[0].value[1]'

# Expected: >0.95
```

### 2. Monitor S3 Error Rate
```bash
# S3 503 errors should remain near zero
curl -s 'http://prometheus:9090/api/v1/query?query=rate(s3_errors_total{error_type="503"}[5m])'

# Expected: <0.001 (less than 0.1%)
```

### 3. Test Producer Throughput
```bash
# Send burst of messages
cargo run --example producer_burst -- --count=10000

# Check success rate
# Expected: >95% success (some throttling acceptable during burst)
```

### 4. Review Adaptive Rate Behavior
```bash
# Rate should stabilize (not constantly decreasing)
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=50 | grep "Current rates"

# Expected: Rate stable or slowly increasing (recovery phase)
```

---

## Prevention

### 1. Right-Size Rate Limits for Workload

Measure actual throughput needs and configure accordingly:
```bash
# Measure peak producer throughput over 1 week
curl -s 'http://prometheus:9090/api/v1/query?query=max_over_time(rate(producer_messages_sent_total[1m])[1w])'

# Set rate limit = peak × 1.2 (20% headroom)
# Example: Peak = 2500/sec → Set limit = 3000/sec
```

### 2. Configure Burst Capacity for Traffic Patterns

For bursty workloads (e.g., batch processing every hour):
```
Burst capacity = peak_burst_size / average_message_size

Example:
- Batch import: 50,000 messages over 10 seconds = 5,000 msg/sec
- Set burst = 5,000 tokens
- Set rate = 3,000 tokens/sec (for sustained load)
```

### 3. Set Up Alerts for High Throttling Rate

Alert before it impacts users:
```yaml
# Prometheus alert rule
- alert: HighThrottlingRate
  expr: |
    sum(rate(throttle_decisions_total{decision="rate_limited"}[5m])) /
    sum(rate(throttle_decisions_total[5m])) > 0.10
  for: 2m
  annotations:
    summary: "S3 throttling rate >10% (backpressure active)"
    description: "{{ $value | humanizePercentage }} of requests are rate limited"
```

### 4. Implement Producer-Side Backoff

Producers should handle `RESOURCE_EXHAUSTED` errors gracefully:
```rust
// Example producer retry logic
let mut backoff = Duration::from_millis(100);

loop {
    match producer.send(topic, key, value, partition).await {
        Ok(result) => return Ok(result),
        Err(e) if e.code() == StatusCode::RESOURCE_EXHAUSTED => {
            // Exponential backoff
            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, Duration::from_secs(5));
        }
        Err(e) => return Err(e),
    }
}
```

### 5. Use Load Shedding at Producer

If backpressure is frequent, implement producer-side load shedding:
```rust
// Drop low-priority messages during backpressure
if is_rate_limited && message.priority == Priority::Low {
    warn!("Dropping low-priority message due to backpressure");
    return Ok(());  // Drop instead of retry
}
```

---

## Related Runbooks

- [Circuit Breaker Open](./circuit-breaker-open.md) - If throttling escalates to circuit opening
- [Memory/Disk Pressure](./resource-pressure.md) - May cause slower S3 uploads, triggering throttling
- [WAL Recovery Failures](./wal-recovery-failures.md) - If S3 throttling prevents WAL replay

---

## Configuration Reference

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `THROTTLE_ENABLED` | `true` | Enable/disable S3 throttling |
| `THROTTLE_PUT_RATE` | `3000` | PUT operations per second |
| `THROTTLE_GET_RATE` | `5000` | GET operations per second |
| `THROTTLE_DELETE_RATE` | `3000` | DELETE operations per second |

### Default Rate Limits

| Operation | Rate (ops/sec) | Burst | S3 Limit | Headroom |
|-----------|---------------|-------|----------|----------|
| PUT | 3000 | 3000 | 3500 | 15% |
| GET | 5000 | 5000 | 5500 | 10% |
| DELETE | 3000 | 3000 | 3500 | 15% |

---

## Metrics Reference

| Metric Name | Description | Healthy Value |
|-------------|-------------|---------------|
| `throttle_decisions_total{decision="allow"}` | Allowed requests | >95% of total |
| `throttle_decisions_total{decision="rate_limited"}` | Rate limited requests | <5% of total |
| `throttle_rate_current{operation="put"}` | Current PUT rate limit | 2500-3500 |
| `s3_errors_total{error_type="503"}` | S3 503 SlowDown errors | <0.1% |

---

## References

- **Code**: [rate_limiter.rs](../../crates/streamhouse-storage/src/rate_limiter.rs)
- **Tests**: [throttle_integration_test.rs](../../crates/streamhouse-storage/tests/throttle_integration_test.rs)
- **Design**: [PHASE_12.4.2_S3_THROTTLING_COMPLETE.md](../PHASE_12.4.2_S3_THROTTLING_COMPLETE.md)
- **AWS Docs**: [S3 Request Rate Performance Guidelines](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance.html)

---

**Last Updated**: January 31, 2026
**Version**: 1.0
