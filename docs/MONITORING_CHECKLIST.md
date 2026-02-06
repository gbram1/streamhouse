# StreamHouse Monitoring Checklist

## 📊 At-a-Glance Health Dashboard

### Current System Status

Run this command for instant health check:
```bash
./scripts/check-system-health.sh
```

Expected output:
```
╔══════════════════════════════════════════════════╗
║        StreamHouse System Health Check          ║
╚══════════════════════════════════════════════════╝

Infrastructure:
  ✓ PostgreSQL   [HEALTHY]  5ms latency
  ✓ MinIO        [HEALTHY]  234 GB used
  ✓ Prometheus   [HEALTHY]  98.2% uptime
  ✓ Grafana      [HEALTHY]  23 dashboards

Agents:
  ✓ agent-1      [HEALTHY]  4 partitions | last heartbeat: 2s ago
  ✓ agent-2      [HEALTHY]  5 partitions | last heartbeat: 1s ago
  ✓ agent-3      [HEALTHY]  3 partitions | last heartbeat: 3s ago

Topics:
  ✓ orders          12 partitions | 1.2M messages | 0 orphans
  ✓ user-events      6 partitions | 450K messages | 0 orphans
  ✓ metrics         24 partitions | 5.4M messages | 0 orphans

Consumers:
  ✓ analytics-group    lag: 234 messages   (HEALTHY)
  ✓ warehouse-group    lag: 12 messages    (HEALTHY)
  ⚠ ml-pipeline-group  lag: 12,456 messages (WARNING)

Data Flow (last 5 min):
  ✓ Produce rate:  1,234 msg/sec
  ✓ Consume rate:  1,189 msg/sec
  ✓ Storage rate:  456 KB/sec to MinIO

Overall Status: HEALTHY ✓
```

---

## 🎯 Critical Metrics (Monitor 24/7)

### 1. Agent Health
**What**: Are all agents alive and handling their partitions?

```sql
-- Run every 30 seconds
SELECT
    agent_id,
    COUNT(*) as partition_count,
    CAST((julianday('now') - julianday(last_heartbeat_at)) * 86400 AS INTEGER) as seconds_since_heartbeat
FROM agents a
LEFT JOIN partition_leases pl ON a.agent_id = pl.owner_agent_id AND pl.expires_at > datetime('now')
WHERE a.last_heartbeat_at > datetime('now', '-60 seconds')
GROUP BY agent_id;
```

**🚨 Alert if**:
- Any agent's `seconds_since_heartbeat` > 30
- Any agent has 0 partitions for > 5 minutes
- Total agent count < expected minimum

---

### 2. Partition Coverage
**What**: Does every partition have an owner?

```sql
-- Run every 1 minute
SELECT
    p.topic,
    p.partition_id,
    CASE
        WHEN pl.owner_agent_id IS NULL THEN 'ORPHANED'
        WHEN pl.expires_at < datetime('now') THEN 'EXPIRED'
        ELSE pl.owner_agent_id
    END as status
FROM partitions p
LEFT JOIN partition_leases pl ON p.topic = pl.topic AND p.partition_id = pl.partition_id
WHERE pl.owner_agent_id IS NULL OR pl.expires_at < datetime('now');
```

**🚨 Alert if**:
- ANY partition shows ORPHANED status
- More than 2 partitions show EXPIRED (rebalance taking too long)

---

### 3. Consumer Lag
**What**: Are consumers keeping up with producers?

```sql
-- Run every 2 minutes
SELECT
    co.group_id,
    co.topic,
    co.partition_id,
    p.high_watermark - co.committed_offset as lag_messages,
    co.updated_at as last_commit_time
FROM consumer_offsets co
JOIN partitions p ON co.topic = p.topic AND co.partition_id = p.partition_id
WHERE p.high_watermark - co.committed_offset > 100
ORDER BY lag_messages DESC
LIMIT 20;
```

**🚨 Alert if**:
- Lag > 10,000 messages for any partition
- Lag growing consistently over 15 minutes
- `last_commit_time` > 5 minutes ago

---

### 4. Storage Health
**What**: Is data being persisted to MinIO?

```bash
# Run every 5 minutes
RECENT_SEGMENTS=$(docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive | \
  awk -v d="$(date -u -v-10M '+%Y-%m-%d %H:%M')" '$0 > d' | wc -l)

echo "Segments created in last 10 minutes: $RECENT_SEGMENTS"
```

**🚨 Alert if**:
- `RECENT_SEGMENTS` = 0 (no writes in 10 minutes)
- MinIO bucket size not growing
- 403/500 errors in agent logs

---

### 5. End-to-End Latency
**What**: Can we write and read in < 1 second?

```bash
# Run every 3 minutes
START=$(date +%s%N)
TEST_VALUE="{\"health_check\": true, \"timestamp\": $START}"

# Write
streamctl produce health-check --partition 0 --value "$TEST_VALUE"

# Read
streamctl consume health-check --partition 0 --limit 1 | grep "$START"

END=$(date +%s%N)
LATENCY=$(( ($END - $START) / 1000000 )) # Convert to milliseconds

echo "E2E latency: ${LATENCY}ms"
```

**🚨 Alert if**:
- Latency > 1000ms (1 second)
- Any step fails (produce or consume)

---

## 📈 Prometheus Queries (For Dashboards)

### Agent Metrics

```promql
# Healthy agents
count(streamhouse_agent_heartbeat_timestamp_seconds > (time() - 60))

# Partitions per agent
sum by (agent_id) (streamhouse_active_partitions)

# Lease renewal success rate
rate(streamhouse_lease_renewals_total{status="success"}[5m]) /
rate(streamhouse_lease_renewals_total[5m]) * 100
```

### Throughput Metrics

```promql
# Messages produced per second
sum(rate(streamhouse_records_sent_total[1m]))

# Messages consumed per second
sum(rate(streamhouse_records_consumed_total[1m]))

# Bytes written to storage per second
sum(rate(streamhouse_bytes_written_total[1m]))
```

### Latency Metrics

```promql
# Producer p99 latency
histogram_quantile(0.99, sum(rate(streamhouse_send_duration_seconds_bucket[5m])) by (le, topic))

# Consumer poll latency
histogram_quantile(0.95, sum(rate(streamhouse_poll_duration_seconds_bucket[5m])) by (le))

# MinIO upload latency
histogram_quantile(0.99, sum(rate(streamhouse_write_latency_seconds_bucket[5m])) by (le))
```

### Consumer Lag

```promql
# Lag in messages
streamhouse_consumer_lag_records

# Lag in seconds (time-based)
streamhouse_consumer_lag_seconds

# Lag growth rate (messages/second)
rate(streamhouse_consumer_lag_records[5m])
```

---

## 🔍 Debugging Guide

### Symptom: Messages not being produced

**Check**:
1. Producer errors in application logs
2. Agent gRPC ports accessible: `telnet agent-1 9090`
3. Load balancer health: `curl http://lb/health`
4. Partition has owner: Query partition_leases table

**Fix**:
```bash
# Check which agent owns the partition
streamctl topic describe <topic>

# Test direct connection to agent
grpcurl -plaintext agent-1:9090 list
```

---

### Symptom: Messages not being consumed

**Check**:
1. Consumer group registered in metadata
2. Offsets being committed
3. Partition high watermark increasing
4. No network issues between consumer and agents

**Fix**:
```sql
-- Check consumer offsets
SELECT * FROM consumer_offsets WHERE group_id = 'your-group';

-- Check partition high watermarks
SELECT topic, partition_id, high_watermark FROM partitions;

-- Reset offsets if needed
DELETE FROM consumer_offsets WHERE group_id = 'your-group';
```

---

### Symptom: Consumer lag increasing

**Check**:
1. Consumer processing time (slow application logic?)
2. Number of consumer instances vs partitions
3. Network latency between consumer and agents
4. Consumer poll timeout too low

**Fix**:
```bash
# Scale consumer group (run more instances)
# Each instance will get assigned different partitions

# Check consumer is committing
streamctl consumer-groups describe your-group

# Increase consumer parallelism
# Add more consumer instances (auto-rebalances)
```

---

### Symptom: Agent not responding

**Check**:
1. Agent process running: `ps aux | grep streamhouse-agent`
2. Agent logs: `tail -100 /var/log/streamhouse/agent-X.log`
3. Database connectivity: `psql -h postgres -U streamhouse`
4. MinIO accessibility: `mc ls myminio/streamhouse-data`

**Fix**:
```bash
# Restart agent
systemctl restart streamhouse-agent@1

# Agent will:
# 1. Re-register with metadata store
# 2. Send heartbeat
# 3. Acquire partition leases in next rebalance (< 30s)
# 4. Resume serving traffic

# No data loss - partitions reassigned automatically
```

---

## 📱 Monitoring Tools Summary

| Tool | Purpose | Access | Frequency |
|------|---------|--------|-----------|
| **Prometheus** | Time-series metrics | http://prometheus:9090 | Real-time |
| **Grafana** | Visual dashboards | http://grafana:3000 | Real-time |
| **SQL Queries** | Metadata inspection | `sqlite3 metadata.db` | On-demand |
| **MinIO Console** | Storage browser | http://minio:9001 | Daily |
| **streamctl** | CLI inspection | Terminal | As needed |
| **Health Check Script** | E2E validation | `./scripts/check-health.sh` | Every 5 min |
| **Agent Logs** | Detailed debugging | `/var/log/streamhouse/` | When issues occur |

---

## ✅ Daily Checklist (5 minutes)

```
[ ] Run health check script - all green?
[ ] Check Grafana dashboard - any red panels?
[ ] Review Prometheus alerts - any firing?
[ ] Check consumer lag - all < 1000?
[ ] Check agent count - all online?
[ ] Check MinIO usage - growing as expected?
[ ] Check recent errors in logs - none?
[ ] Test produce/consume - works?
```

If all ✅ → **System is healthy, go back to sleep** 😴

If any ❌ → **Follow debugging guide above** 🔧
