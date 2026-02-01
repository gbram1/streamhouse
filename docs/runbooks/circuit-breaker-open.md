# Runbook: S3 Circuit Breaker Open

**Severity**: HIGH
**Component**: Storage Layer (PartitionWriter)
**Impact**: Producers receive `UNAVAILABLE` errors, writes are rejected

---

## Symptoms

### Producer Side
- Producers receive gRPC `UNAVAILABLE (14)` status errors
- Error message: `"S3 service is temporarily unavailable - circuit breaker open"`
- Requests fail immediately without attempting S3 upload (fail-fast)

### Agent Logs
```
ERROR S3 circuit breaker open
ERROR Circuit state transitioned to Open after 5 consecutive failures
```

### Monitoring
- Metric `circuit_breaker_state` shows `state="open"`
- Metric `circuit_breaker_transitions_total` increments
- S3 error rate (`s3_errors_total`) was elevated before circuit opened

---

## Root Causes

The circuit breaker opens after **5 consecutive S3 failures** (default threshold). Common causes:

1. **S3 Outage**: AWS S3 region/service degradation
2. **Network Issues**: Connectivity problems between agent and S3
3. **Authentication Failures**: Invalid AWS credentials or expired tokens
4. **Bucket Misconfiguration**: Incorrect bucket name, region, or permissions
5. **S3 503 SlowDown Errors**: Sustained throttling (rate limiter failed to prevent)

---

## Investigation Steps

### Step 1: Check Circuit Breaker State

View current state in agent logs:
```bash
# Check recent logs for circuit transitions
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=100 | grep -i "circuit"

# Look for:
# - "Circuit state: Open"
# - "Circuit breaker opened at <timestamp>"
# - Failure count before opening
```

Expected output:
```
[2026-01-31T10:15:23Z] WARN Circuit breaker failure count: 4
[2026-01-31T10:15:25Z] ERROR Circuit breaker opened after 5 failures
[2026-01-31T10:15:25Z] INFO Circuit state transitioned: Closed -> Open
```

### Step 2: Review S3 Error Logs

Identify the underlying S3 failures that triggered the circuit:
```bash
# Check for S3 errors in the last 5 minutes
kubectl logs -n streamhouse deployment/streamhouse-agent --since=5m | grep -i "s3.*error"

# Common error patterns to look for:
# - "503 SlowDown" → S3 throttling
# - "Connection refused" → Network/connectivity
# - "Access Denied" → Permissions
# - "NoSuchBucket" → Configuration
```

### Step 3: Check S3 Service Health

Verify S3 availability:
```bash
# Test S3 connectivity from agent pod
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  aws s3 ls s3://YOUR_BUCKET_NAME --region YOUR_REGION

# Check AWS Service Health Dashboard
# https://health.aws.amazon.com/health/status
```

### Step 4: Review Metrics

Check S3 error rate and circuit breaker metrics:
```bash
# S3 error rate (last 10 minutes)
curl -s 'http://prometheus:9090/api/v1/query?query=rate(s3_errors_total{operation="PUT"}[10m])'

# Circuit breaker state
curl -s 'http://prometheus:9090/api/v1/query?query=circuit_breaker_state'

# Recent transitions
curl -s 'http://prometheus:9090/api/v1/query?query=increase(circuit_breaker_transitions_total[1h])'
```

---

## Resolution

### Option 1: Auto-Recovery (Recommended)

The circuit breaker automatically transitions to **HalfOpen** after **30 seconds** (default timeout). In HalfOpen state:

1. The circuit allows limited requests to **test S3 health**
2. If **2 consecutive successes** occur → circuit closes (normal operation resumes)
3. If **any failure** occurs → circuit reopens (waits another 30s)

**Action**: Wait for auto-recovery. Monitor logs:
```bash
kubectl logs -n streamhouse deployment/streamhouse-agent --follow | grep -i "circuit"
```

Expected recovery sequence:
```
[10:15:55Z] INFO Circuit breaker timeout elapsed, transitioning to HalfOpen
[10:15:56Z] INFO S3 PUT succeeded in HalfOpen state (success count: 1/2)
[10:15:57Z] INFO S3 PUT succeeded in HalfOpen state (success count: 2/2)
[10:15:57Z] INFO Circuit state transitioned: HalfOpen -> Closed
```

**Timeline**: Recovery in 30-90 seconds (30s timeout + 2 successful requests)

---

### Option 2: Manual Circuit Reset (Use with Caution)

If auto-recovery is too slow or the circuit is stuck, manually reset:

**⚠️ Warning**: Only reset if you've **verified S3 is healthy**. Resetting prematurely can cause cascading failures.

```bash
# Option A: Restart agent pod (forces new circuit breaker instance)
kubectl rollout restart -n streamhouse deployment/streamhouse-agent

# Option B: Call internal reset API (if implemented)
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  curl -X POST http://localhost:8080/internal/circuit-breaker/reset
```

**When to use**:
- S3 is confirmed healthy (manual test succeeded)
- Circuit has been open for >5 minutes
- Auto-recovery failed multiple times

**When NOT to use**:
- S3 is still failing (check Step 3 first)
- Root cause not identified
- Within first 2 minutes of circuit opening

---

### Option 3: Fix Root Cause

Address the underlying S3 issue based on Step 2 findings:

#### For S3 Throttling (503 SlowDown):
```bash
# Verify rate limiter configuration
kubectl get configmap -n streamhouse streamhouse-config -o yaml | grep THROTTLE

# Expected:
# THROTTLE_ENABLED: "true"
# PUT rate should be ≤3000/sec (S3 prefix limit: 3500/sec)

# If rate limiter disabled, re-enable:
kubectl set env deployment/streamhouse-agent THROTTLE_ENABLED=true

# If limits too high, tune down:
kubectl set env deployment/streamhouse-agent THROTTLE_PUT_RATE=2500
```

**See**: [Runbook: High S3 Throttling Rate](./high-s3-throttling-rate.md)

#### For Network Issues:
```bash
# Test connectivity from agent pod
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  curl -I https://s3.YOUR_REGION.amazonaws.com

# Check security groups, VPC endpoints, DNS resolution
```

#### For Authentication Failures:
```bash
# Verify IAM credentials/role
kubectl exec -n streamhouse deployment/streamhouse-agent -- \
  aws sts get-caller-identity

# Rotate credentials if needed
kubectl create secret generic aws-credentials \
  --from-literal=AWS_ACCESS_KEY_ID=YOUR_KEY \
  --from-literal=AWS_SECRET_ACCESS_KEY=YOUR_SECRET \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl rollout restart -n streamhouse deployment/streamhouse-agent
```

#### For Bucket Misconfiguration:
```bash
# Verify bucket exists and is accessible
aws s3 ls s3://YOUR_BUCKET_NAME --region YOUR_REGION

# Check WriteConfig in agent startup logs
kubectl logs -n streamhouse deployment/streamhouse-agent | grep "WriteConfig"

# Expected:
# s3_bucket: "your-bucket-name"
# s3_region: "us-east-1"
```

---

## Verification

After resolution, verify circuit is closed and writes succeed:

### 1. Check Circuit State
```bash
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=20 | grep "circuit"
# Expected: "Circuit state: Closed"
```

### 2. Test Producer
```bash
# Send test message
cargo run --example producer_simple

# Expected: No UNAVAILABLE errors, offset returned
```

### 3. Monitor Metrics
```bash
# Circuit should be closed
curl -s 'http://prometheus:9090/api/v1/query?query=circuit_breaker_state' | jq '.data.result[0].value[1]'
# Expected: "0" (Closed=0, Open=1, HalfOpen=2)

# S3 error rate should be low
curl -s 'http://prometheus:9090/api/v1/query?query=rate(s3_errors_total[5m])' | jq '.data.result[0].value[1]'
# Expected: <0.01 (less than 1% error rate)
```

---

## Prevention

### 1. Enable Throttling (Default)

Ensure rate limiting is enabled to prevent S3 503 errors:
```bash
kubectl set env deployment/streamhouse-agent THROTTLE_ENABLED=true
```

**Default limits**:
- PUT: 3000/sec (15% headroom from S3's 3500/sec)
- GET: 5000/sec (10% headroom from S3's 5500/sec)

### 2. Tune Circuit Breaker Thresholds

If circuit opens too aggressively, increase failure threshold:
```rust
// In WriteConfig (requires code change + redeploy)
CircuitBreakerConfig {
    failure_threshold: 10,  // Tolerate more failures (default: 5)
    success_threshold: 2,
    timeout: Duration::from_secs(30),
}
```

**Trade-off**: Higher threshold = slower detection of S3 outages

### 3. Monitor S3 Error Rates

Set up alerts **before** circuit opens:
```yaml
# Prometheus alert rule
- alert: HighS3ErrorRate
  expr: rate(s3_errors_total{operation="PUT"}[5m]) > 0.05
  for: 2m
  annotations:
    summary: "S3 error rate >5% (circuit may open soon)"
```

### 4. Use S3 VPC Endpoints

Reduce network-related failures:
```bash
# Create VPC endpoint for S3 (if in private subnet)
aws ec2 create-vpc-endpoint \
  --vpc-id vpc-XXXXXX \
  --service-name com.amazonaws.YOUR_REGION.s3 \
  --route-table-ids rtb-XXXXXX
```

### 5. Implement S3 Prefix Sharding

If hitting per-prefix limits (3500 PUT/sec), use hash-based sharding:
```
# Instead of:
s3://bucket/topic/partition-0/segment-1.log

# Use:
s3://bucket/a7/topic/partition-0/segment-1.log  # Hash prefix
s3://bucket/3f/topic/partition-0/segment-2.log
```

**Benefit**: Distribute load across multiple S3 prefixes (each gets separate rate limits)

---

## Related Runbooks

- [High S3 Throttling Rate](./high-s3-throttling-rate.md) - When rate limiter is rejecting requests
- [WAL Recovery Failures](./wal-recovery-failures.md) - If S3 failures persist after circuit closes
- [Memory/Disk Pressure](./resource-pressure.md) - May cause S3 upload retries to fail

---

## Metrics Reference

| Metric Name | Description | Healthy Value |
|-------------|-------------|---------------|
| `circuit_breaker_state` | Current state (0=Closed, 1=Open, 2=HalfOpen) | `0` (Closed) |
| `circuit_breaker_transitions_total` | Total state transitions | Increasing slowly (<5/hour) |
| `s3_errors_total{operation="PUT"}` | S3 PUT error count | <1% of total requests |
| `s3_requests_total{operation="PUT"}` | Total S3 PUT requests | Steady or increasing |

---

## References

- **Code**: [circuit_breaker.rs](../../crates/streamhouse-storage/src/circuit_breaker.rs)
- **Tests**: [throttle_integration_test.rs](../../crates/streamhouse-storage/tests/throttle_integration_test.rs)
- **Design**: [PHASE_12.4.2_S3_THROTTLING_COMPLETE.md](../PHASE_12.4.2_S3_THROTTLING_COMPLETE.md)

---

**Last Updated**: January 31, 2026
**Version**: 1.0
