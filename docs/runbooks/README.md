# Streamhouse Operational Runbooks

This directory contains operational runbooks for troubleshooting and resolving common production issues in Streamhouse.

---

## Quick Reference

| Runbook | Severity | Symptoms | When to Use |
|---------|----------|----------|-------------|
| [Circuit Breaker Open](./circuit-breaker-open.md) | HIGH | UNAVAILABLE errors, S3 failures | Circuit breaker opened after repeated S3 failures |
| [High S3 Throttling Rate](./high-s3-throttling-rate.md) | MEDIUM | RESOURCE_EXHAUSTED errors, backpressure | Rate limiter rejecting requests |
| [WAL Recovery Failures](./wal-recovery-failures.md) | HIGH | Agent startup failures, corruption | WAL file corruption, recovery errors |
| [Partition Lease Conflicts](./partition-lease-conflicts.md) | HIGH | Lease conflicts, stale epoch errors | Multiple agents competing for same partition |
| [Schema Registry Errors](./schema-registry-errors.md) | MEDIUM | Incompatible schema, validation errors | Schema registration or compatibility issues |
| [Resource Pressure](./resource-pressure.md) | HIGH | OOMKilled, disk full, high CPU | Memory, disk, or CPU exhaustion |

---

## Runbook Details

### [Circuit Breaker Open](./circuit-breaker-open.md)
**Problem**: The S3 circuit breaker has opened after detecting repeated failures, rejecting all S3 operations.

**Common Causes**:
- S3 service outage or degradation
- Network connectivity issues
- Sustained S3 throttling (503 errors)
- Invalid AWS credentials

**Quick Fix**: Wait 30 seconds for auto-recovery, or manually reset circuit if S3 is confirmed healthy.

---

### [High S3 Throttling Rate](./high-s3-throttling-rate.md)
**Problem**: The rate limiter is rejecting requests to stay within S3 API limits, causing backpressure.

**Common Causes**:
- Burst traffic exceeding configured limits
- Rate limits set too conservatively
- Multiple agents sharing same S3 prefix
- Adaptive rate adjustment reduced limits after 503 errors

**Quick Fix**: Increase rate limits if workload is legitimate, or implement S3 prefix sharding.

---

### [WAL Recovery Failures](./wal-recovery-failures.md)
**Problem**: Agent fails to start due to corrupted Write-Ahead Log, or recovery takes too long.

**Common Causes**:
- Disk errors or crashes during WAL writes
- File corruption (CRC mismatch)
- Disk full preventing WAL writes
- Concurrent access from multiple agents

**Quick Fix**: Let auto-recovery skip corrupted records, or manually truncate WAL if unrecoverable.

---

### [Partition Lease Conflicts](./partition-lease-conflicts.md)
**Problem**: Multiple agents believe they own the same partition, causing lease conflicts and write failures.

**Common Causes**:
- Clock skew between nodes
- Network partitions causing lease expiration
- Lease renewal failures
- Agent restarts without graceful shutdown

**Quick Fix**: Restart conflicting agent or force lease reset in database.

---

### [Schema Registry Errors](./schema-registry-errors.md)
**Problem**: Schema registration fails due to compatibility violations, or consumers cannot resolve schemas.

**Common Causes**:
- Incompatible schema changes (breaking changes)
- Wrong compatibility mode configured
- Missing schema registration before sending messages
- Database connectivity issues

**Quick Fix**: Fix schema to be compatible, or change compatibility mode to allow the change.

---

### [Resource Pressure](./resource-pressure.md)
**Problem**: Agent experiencing memory exhaustion (OOMKilled), disk full, or high CPU usage.

**Common Causes**:
- Too many partitions assigned to single agent
- Large segment buffers consuming memory
- WAL files not being truncated after S3 upload
- Insufficient resource limits in Kubernetes

**Quick Fix**: Increase memory/disk limits, reduce partition assignment, or flush segments more frequently.

---

## Using These Runbooks

### 1. Identify the Problem

Start with symptoms:
- **Producer errors**: Check which gRPC status code is returned
  - `UNAVAILABLE (14)` → [Circuit Breaker Open](./circuit-breaker-open.md)
  - `RESOURCE_EXHAUSTED (8)` → [High S3 Throttling](./high-s3-throttling-rate.md)

- **Agent logs**: Search for error keywords
  - "circuit breaker open" → [Circuit Breaker Open](./circuit-breaker-open.md)
  - "rate limited" → [High S3 Throttling](./high-s3-throttling-rate.md)
  - "WAL recovery failed" → [WAL Recovery](./wal-recovery-failures.md)
  - "lease conflict" → [Lease Conflicts](./partition-lease-conflicts.md)
  - "schema incompatible" → [Schema Registry](./schema-registry-errors.md)
  - "OOMKilled" → [Resource Pressure](./resource-pressure.md)

### 2. Follow the Runbook

Each runbook follows this structure:
1. **Symptoms**: Detailed description of what you'll observe
2. **Root Causes**: Common reasons for the problem
3. **Investigation Steps**: How to diagnose and understand the issue
4. **Resolution**: Step-by-step solutions (multiple options)
5. **Verification**: How to confirm the fix worked
6. **Prevention**: How to avoid the issue in the future

### 3. Escalate If Needed

If the runbook doesn't resolve your issue:
1. Collect diagnostics (logs, metrics, config)
2. Check [Related Runbooks](#related-runbooks) for connected issues
3. File a GitHub issue with details
4. Contact the Streamhouse team

---

## Common Commands

### Check Agent Health
```bash
# Pod status
kubectl get pods -n streamhouse -l app=streamhouse-agent

# Resource usage
kubectl top pod -n streamhouse -l app=streamhouse-agent

# Recent logs
kubectl logs -n streamhouse deployment/streamhouse-agent --tail=100
```

### Check Database
```bash
# Partition leases
psql -h YOUR_DB_HOST -U streamhouse -c "SELECT * FROM partition_leases;"

# Schema registry
psql -h YOUR_DB_HOST -U streamhouse -c "SELECT COUNT(*) FROM schema_registry_schemas;"
```

### Check S3
```bash
# Recent uploads
aws s3 ls s3://your-bucket/orders/partition-0/ --recursive | tail -10

# S3 error rate (CloudWatch)
aws cloudwatch get-metric-statistics \
  --namespace AWS/S3 \
  --metric-name 5xxErrors \
  --dimensions Name=BucketName,Value=your-bucket \
  --start-time $(date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%S) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%S) \
  --period 300 \
  --statistics Sum
```

### Check Metrics
```bash
# Circuit breaker state
curl -s http://prometheus:9090/api/v1/query?query=circuit_breaker_state

# Throttle decisions
curl -s http://prometheus:9090/api/v1/query?query=rate(throttle_decisions_total[5m])

# Lease conflicts
curl -s http://prometheus:9090/api/v1/query?query=rate(lease_conflicts_total[5m])
```

---

## Related Documentation

- [Phase 12.4.2: S3 Throttling Implementation](../PHASE_12.4.2_S3_THROTTLING_COMPLETE.md)
- [Phase 10: WAL Implementation](../PHASE_10_WAL_COMPLETE.md)
- [Phase 4.2: Lease Management](../PHASE_4_MULTI_AGENT_COMPLETE.md)
- [Roadmap to Production](../ROADMAP_TO_PRODUCTION_JAN_31_2026.md)

---

## Runbook Maintenance

### Updating Runbooks

When updating runbooks:
1. Keep the version number and last updated date current
2. Add new sections for new error patterns discovered
3. Update examples with real production data (anonymized)
4. Cross-link to related runbooks when relevant

### Testing Runbooks

Periodically test runbooks in staging:
1. Trigger the issue intentionally (e.g., set low rate limits)
2. Follow the runbook steps exactly as written
3. Verify resolution works as documented
4. Update runbook if steps have changed

### Adding New Runbooks

Template for new runbooks:
```markdown
# Runbook: [Problem Name]

**Severity**: [HIGH/MEDIUM/LOW]
**Component**: [Component Name]
**Impact**: [Impact Description]

## Symptoms
[What you observe]

## Root Causes
[Why it happens]

## Investigation Steps
[How to diagnose]

## Resolution
[How to fix - multiple options]

## Verification
[How to confirm fix]

## Prevention
[How to avoid]

## Related Runbooks
[Links to related runbooks]
```

---

## Monitoring and Alerts

### Recommended Alert Setup

Configure these alerts to detect issues before they impact users:

```yaml
# Prometheus alert rules
groups:
- name: streamhouse-alerts
  rules:
  # Circuit breaker
  - alert: CircuitBreakerOpen
    expr: circuit_breaker_state == 1
    for: 2m
    annotations:
      severity: high
      runbook: docs/runbooks/circuit-breaker-open.md

  # S3 throttling
  - alert: HighThrottlingRate
    expr: rate(throttle_decisions_total{decision="rate_limited"}[5m]) / rate(throttle_decisions_total[5m]) > 0.10
    for: 5m
    annotations:
      severity: medium
      runbook: docs/runbooks/high-s3-throttling-rate.md

  # Lease conflicts
  - alert: LeaseConflicts
    expr: rate(lease_conflicts_total[5m]) > 0.01
    for: 2m
    annotations:
      severity: high
      runbook: docs/runbooks/partition-lease-conflicts.md

  # Resource pressure
  - alert: HighMemoryUsage
    expr: (container_memory_usage_bytes / container_spec_memory_limit_bytes) > 0.85
    for: 5m
    annotations:
      severity: high
      runbook: docs/runbooks/resource-pressure.md

  # Schema registry
  - alert: SchemaRegistrationFailures
    expr: rate(schema_registry_errors_total{type="registration"}[5m]) > 0.01
    for: 2m
    annotations:
      severity: medium
      runbook: docs/runbooks/schema-registry-errors.md
```

### Grafana Dashboard

Create a dashboard with panels for:
1. Circuit breaker state (gauge)
2. Throttle decision breakdown (pie chart)
3. Memory/CPU usage (time series)
4. Lease conflict rate (time series)
5. Schema registry error rate (time series)
6. Disk usage (gauge)

Import dashboard template: `monitoring/grafana/streamhouse-overview.json` (Phase 12.4.4)

---

## Feedback and Contributions

Found an issue with a runbook or have a suggestion?
- File an issue: https://github.com/YOUR_ORG/streamhouse/issues
- Submit a PR with improvements
- Share your production experiences in discussions

---

**Last Updated**: January 31, 2026
**Runbook Version**: 1.0
