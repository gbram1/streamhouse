# StreamHouse Production Guide

## How Users Will Use StreamHouse

### Client Libraries & APIs

Users will interact with StreamHouse through **three main interfaces**:

#### 1. **Client SDK (Rust)**
Your primary production interface - embedded in user applications:

```rust
use streamhouse_client::{Producer, Consumer, ProducerConfig, ConsumerConfig};

// Producer - Send events to StreamHouse
let producer = Producer::builder()
    .broker_addresses(vec!["streamhouse-lb.example.com:50051".to_string()])
    .build()
    .await?;

// Send messages
producer.send("user-events", None, event_data, None).await?;
producer.flush().await?; // Ensure delivery

// Consumer - Read events from StreamHouse
let consumer = Consumer::builder()
    .broker_addresses(vec!["streamhouse-lb.example.com:50051".to_string()])
    .group_id("my-app-consumer-group")
    .topics(vec!["user-events".to_string()])
    .build()
    .await?;

// Consume messages
loop {
    let records = consumer.poll(Duration::from_secs(1)).await?;
    for record in records {
        process_event(&record.value).await?;
    }
    consumer.commit().await?;
}
```

#### 2. **CLI Tool (`streamctl`)**
For operators and automation scripts:

```bash
# Topic management
streamctl topic create orders --partitions 12
streamctl topic list
streamctl topic describe orders

# Message inspection
streamctl consume orders --partition 0 --limit 10
streamctl produce orders --partition 0 --value '{"order_id": 123}'

# Consumer group management
streamctl consumer-groups list
streamctl consumer-groups describe my-app-consumer-group
```

#### 3. **gRPC API (Direct)**
For non-Rust clients (Python, Go, Java, etc.):

```python
import grpc
from streamhouse_pb2 import ProduceRequest, ConsumeRequest
from streamhouse_pb2_grpc import StreamHouseStub

# Connect
channel = grpc.insecure_channel('streamhouse-lb.example.com:50051')
stub = StreamHouseStub(channel)

# Produce
request = ProduceRequest(
    topic="user-events",
    partition=0,
    records=[Record(value=b'{"user_id": 123}')]
)
response = stub.Produce(request)

# Consume
request = ConsumeRequest(
    topic="user-events",
    partition=0,
    offset=0,
    limit=100
)
response = stub.Consume(request)
```

## Production Architecture

```
                              ┌─────────────┐
                              │   Users     │
                              │ Applications│
                              └──────┬──────┘
                                     │
                              ┌──────▼──────┐
                              │ Load Balancer│
                              │   (gRPC)     │
                              └──────┬──────┘
                                     │
         ┌───────────────────────────┼───────────────────────────┐
         │                           │                           │
    ┌────▼─────┐              ┌─────▼────┐              ┌───────▼───┐
    │ Agent 1  │              │ Agent 2  │              │ Agent 3   │
    │ (zone-a) │              │ (zone-b) │              │ (zone-c)  │
    │ Port 9090│              │ Port 9090│              │ Port 9090 │
    └────┬─────┘              └─────┬────┘              └─────┬─────┘
         │                          │                          │
         └──────────────────────────┼──────────────────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
         ┌────▼────┐         ┌──────▼─────┐       ┌──────▼──────┐
         │PostgreSQL│         │   MinIO    │       │ Prometheus  │
         │Metadata │         │  Storage   │       │  Metrics    │
         └─────────┘         └────────────┘       └─────────────┘
```

## How to Know Everything is Working

### 1. Health Check Endpoints

**Per-Agent Health Checks** (when Phase 7 HTTP endpoints are enabled):

```bash
# Agent is alive
curl http://agent-1:8080/health
# Expected: 200 OK

# Agent has active partition leases
curl http://agent-2:8080/ready
# Expected: 200 OK (if agent owns partitions)
#          503 Service Unavailable (if agent has no leases)

# View agent metrics
curl http://agent-3:8080/metrics
# Expected: Prometheus format metrics
```

**Current Workaround** (without HTTP endpoints):
```bash
# Check agent process is running
ps aux | grep streamhouse-agent

# Check gRPC port is listening
lsof -i :9090 | grep LISTEN

# Query metadata for agent heartbeats
sqlite3 metadata.db "
SELECT agent_id,
       CAST((julianday('now') - julianday(last_heartbeat_at)) * 86400 AS INTEGER) as seconds_ago
FROM agents
WHERE last_heartbeat_at > datetime('now', '-60 seconds');"
```

### 2. Partition Health Monitoring

**Check Partition Assignments**:
```sql
-- All active partition leases
SELECT
    topic,
    partition_id,
    owner_agent_id,
    epoch,
    expires_at
FROM partition_leases
WHERE expires_at > datetime('now')
ORDER BY topic, partition_id;
```

**Expected Result**:
- Every partition has exactly ONE owner
- No partition has multiple owners (split-brain)
- All partitions across all topics are covered

**Red Flags**:
- Partition with no owner → Data loss risk for new messages
- Partition with expired lease → Failover in progress
- Multiple owners (different epochs) → Rebalance in progress

### 3. Data Flow Health

**Producer Health**:
```bash
# Test producing to each partition
for p in {0..11}; do
  streamctl produce test-topic --partition $p --value "{\"test\": true}"
done

# Verify offsets increased
sqlite3 metadata.db "SELECT partition_id, high_watermark FROM partitions WHERE topic='test-topic';"
```

**Consumer Health**:
```bash
# Test consuming from each partition
streamctl consume test-topic --partition 0 --limit 1

# Check consumer lag
sqlite3 metadata.db "
SELECT
    p.topic,
    p.partition_id,
    p.high_watermark,
    COALESCE(co.committed_offset, 0) as consumer_offset,
    p.high_watermark - COALESCE(co.committed_offset, 0) as lag
FROM partitions p
LEFT JOIN consumer_offsets co ON p.topic = co.topic AND p.partition_id = co.partition_id
WHERE co.group_id = 'your-consumer-group';"
```

**Expected Lag**:
- Production system: < 1000 messages per partition
- If lag > 10,000: Consumer is falling behind
- If lag growing: Scale consumer group

### 4. Storage Health (MinIO)

**Check Segment Files Are Being Written**:
```bash
# Count segments per topic
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive | \
  grep '\.seg' | \
  awk '{print $NF}' | \
  cut -d'/' -f2 | \
  sort | uniq -c

# Expected output:
#   234 my-topic
#   567 user-events
#   890 orders
```

**Monitor Storage Growth**:
```bash
# Total bucket size
docker exec streamhouse-minio mc du myminio/streamhouse-data

# Segments created in last hour
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive | \
  awk -v d="$(date -u -v-1H '+%Y-%m-%d %H:')" '$0 > d'
```

**Red Flags**:
- No new segments in 10+ minutes → Agents not flushing
- Segment size always tiny (< 100 bytes) → Not batching
- 403/404 errors in logs → MinIO credentials/permissions issue

### 5. Prometheus Metrics (When Enabled)

**Key Metrics to Monitor**:

```promql
# Producer throughput (messages/sec)
rate(streamhouse_records_sent_total[1m])

# Producer latency (p99)
histogram_quantile(0.99, streamhouse_send_duration_seconds)

# Consumer lag (messages behind)
streamhouse_consumer_lag_records

# Active partition leases per agent
streamhouse_active_partitions

# Lease renewal success rate
rate(streamhouse_lease_renewals_total{status="success"}[5m]) /
rate(streamhouse_lease_renewals_total[5m])

# Write throughput to MinIO
rate(streamhouse_records_written_total[1m])

# Agent heartbeat failures
rate(streamhouse_heartbeat_failures_total[5m])
```

**Alerting Rules** (copy to prometheus.yml):

```yaml
groups:
  - name: streamhouse
    interval: 30s
    rules:
      # Critical: Partition has no owner
      - alert: PartitionOrphaned
        expr: streamhouse_partition_without_lease > 0
        for: 2m
        annotations:
          summary: "Partition {{ $labels.topic }}/{{ $labels.partition }} has no owner"

      # Critical: Consumer lag too high
      - alert: ConsumerLagHigh
        expr: streamhouse_consumer_lag_records > 10000
        for: 5m
        annotations:
          summary: "Consumer {{ $labels.group_id }} lag: {{ $value }} messages"

      # Warning: No heartbeats from agent
      - alert: AgentNoHeartbeat
        expr: time() - streamhouse_agent_last_heartbeat_seconds > 60
        for: 1m
        annotations:
          summary: "Agent {{ $labels.agent_id }} hasn't sent heartbeat in 60s"

      # Warning: Low write throughput
      - alert: LowWriteThroughput
        expr: rate(streamhouse_records_written_total[5m]) < 10
        for: 10m
        annotations:
          summary: "Write throughput dropped to {{ $value }}/sec"
```

### 6. Grafana Dashboards

**Dashboard Panels to Create**:

1. **System Overview**
   - Total throughput (messages/sec)
   - Total active partitions
   - Number of healthy agents
   - Total consumer lag across all groups

2. **Agent Health**
   - Agents by status (healthy/unhealthy)
   - Partitions per agent
   - Heartbeat latency
   - Lease renewal success rate

3. **Producer Metrics**
   - Messages sent per topic
   - Producer latency (p50, p95, p99)
   - Batch size distribution
   - Send error rate

4. **Consumer Metrics**
   - Messages consumed per group
   - Consumer lag per partition
   - Poll latency
   - Commit success rate

5. **Storage Metrics**
   - Segment flush rate
   - Compression ratio
   - MinIO upload latency
   - Storage used per topic

**Import Dashboard**:
```bash
# Located at
grafana/dashboards/streamhouse-overview.json

# Import via Grafana UI:
# 1. Open http://localhost:3000
# 2. Click "+" → Import
# 3. Upload streamhouse-overview.json
```

### 7. End-to-End Test Script

**Automated Health Check** (run every 5 minutes):

```bash
#!/bin/bash
# e2e-health-check.sh

set -e

TOPIC="health-check-$(date +%s)"
PASS=0
FAIL=0

echo "=== StreamHouse Health Check ==="

# 1. Check agents are registered
AGENT_COUNT=$(sqlite3 metadata.db "SELECT COUNT(*) FROM agents WHERE last_heartbeat_at > datetime('now', '-60 seconds');")
if [ "$AGENT_COUNT" -ge 3 ]; then
    echo "✓ Agents healthy ($AGENT_COUNT registered)"
    ((PASS++))
else
    echo "✗ Not enough agents (expected ≥3, got $AGENT_COUNT)"
    ((FAIL++))
fi

# 2. Check partition coverage
ORPHAN_PARTITIONS=$(sqlite3 metadata.db "
    SELECT COUNT(*) FROM partitions p
    LEFT JOIN partition_leases pl ON p.topic = pl.topic AND p.partition_id = pl.partition_id AND pl.expires_at > datetime('now')
    WHERE pl.owner_agent_id IS NULL;
")
if [ "$ORPHAN_PARTITIONS" -eq 0 ]; then
    echo "✓ All partitions have owners"
    ((PASS++))
else
    echo "✗ Orphaned partitions: $ORPHAN_PARTITIONS"
    ((FAIL++))
fi

# 3. Test produce/consume cycle
streamctl topic create $TOPIC --partitions 1 2>/dev/null || true
TEST_MSG="{\"timestamp\": $(date +%s), \"health_check\": true}"

if streamctl produce $TOPIC --partition 0 --value "$TEST_MSG" 2>&1 | grep -q "Offset"; then
    echo "✓ Produce successful"
    ((PASS++))
else
    echo "✗ Produce failed"
    ((FAIL++))
fi

if streamctl consume $TOPIC --partition 0 --limit 1 2>&1 | grep -q "health_check"; then
    echo "✓ Consume successful"
    ((PASS++))
else
    echo "✗ Consume failed"
    ((FAIL++))
fi

# 4. Check MinIO storage
MINIO_ACCESSIBLE=$(docker exec streamhouse-minio mc ls myminio/streamhouse-data 2>/dev/null && echo "yes" || echo "no")
if [ "$MINIO_ACCESSIBLE" = "yes" ]; then
    echo "✓ MinIO accessible"
    ((PASS++))
else
    echo "✗ MinIO not accessible"
    ((FAIL++))
fi

# Summary
echo
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ $FAIL -eq 0 ]; then
    echo "Status: HEALTHY ✓"
    exit 0
else
    echo "Status: DEGRADED ✗"
    exit 1
fi
```

### 8. Log Monitoring

**Critical Log Patterns to Alert On**:

```bash
# Agent logs (watch for errors)
tail -f /var/log/streamhouse/agent-*.log | grep -E "ERROR|WARN|panic"

# Patterns indicating problems:
grep "Failed to acquire lease" agent.log          # Lease contention
grep "Heartbeat timeout" agent.log                # Network issues
grep "S3 upload failed" agent.log                 # Storage issues
grep "Metadata store unavailable" agent.log       # Database down
grep "Partition rebalance failed" agent.log       # Coordination issues
```

**Server/Agent Logs Structure**:
```
/var/log/streamhouse/
├── agent-1.log          # Agent 1 activity
├── agent-2.log          # Agent 2 activity
├── agent-3.log          # Agent 3 activity
├── coordinator.log      # Partition assignment coordinator
└── access.log           # Client request logs
```

## Production Deployment Checklist

### Before Go-Live

- [ ] All agents running in at least 2 availability zones
- [ ] PostgreSQL replicated with failover
- [ ] MinIO in distributed mode (4+ nodes)
- [ ] Prometheus scraping all agents
- [ ] Grafana dashboards configured
- [ ] Alerts configured in Prometheus
- [ ] Health check script running via cron
- [ ] Log aggregation (ELK/Datadog) configured
- [ ] Backup strategy for PostgreSQL metadata
- [ ] S3 lifecycle policy for old segments
- [ ] Load balancer configured for agent gRPC endpoints
- [ ] TLS certificates for production
- [ ] Authentication/authorization enabled

### Daily Operations

**Morning Check** (5 minutes):
```bash
# 1. Check overnight health
./scripts/e2e-health-check.sh

# 2. Review Grafana "System Overview" dashboard
open http://grafana.example.com/d/streamhouse-overview

# 3. Check for alerts
curl http://prometheus.example.com/api/v1/alerts | jq '.data.alerts[] | select(.state=="firing")'

# 4. Review agent status
sqlite3 metadata.db "SELECT agent_id, last_heartbeat_at FROM agents ORDER BY last_heartbeat_at DESC LIMIT 10;"
```

**When Issues Occur**:

1. **Agent Down**:
   - Check logs: `tail -100 /var/log/streamhouse/agent-X.log`
   - Restart agent: `systemctl restart streamhouse-agent@X`
   - Verify rebalance: Watch partition leases reassign

2. **Consumer Lag Spike**:
   - Check consumer logs for errors
   - Scale consumer group (add instances)
   - Verify network between consumers and agents

3. **Storage Full**:
   - Run retention cleanup: `streamctl admin cleanup --older-than 7d`
   - Check MinIO bucket lifecycle policies
   - Add MinIO nodes if needed

4. **Metadata DB Slow**:
   - Check PostgreSQL connection pool
   - Review slow query log
   - Consider read replicas for agent heartbeats

## User Onboarding

**Quick Start for New Users**:

1. **Get Credentials**:
   ```bash
   # Users receive:
   - StreamHouse broker address: streamhouse-lb.example.com:50051
   - TLS certificate (if enabled)
   - API key (if auth enabled)
   ```

2. **Add Dependency**:
   ```toml
   [dependencies]
   streamhouse-client = "0.1.0"
   ```

3. **Example Code** (provide as template):
   ```rust
   // See examples in docs/client-quickstart.md
   ```

4. **Create Topic**:
   ```bash
   streamctl topic create my-app-events --partitions 12
   ```

5. **Monitor Their Usage**:
   ```sql
   -- Per-user metrics
   SELECT topic, SUM(record_count) as messages
   FROM segments
   WHERE topic LIKE 'user-123-%'
   GROUP BY topic;
   ```

## Summary: "Is Everything Working?"

### Quick Answer:
```bash
# One command to check everything:
./scripts/e2e-health-check.sh && \
curl -s http://prometheus:9090/api/v1/query?query=up | jq '.data.result[] | select(.metric.job=="streamhouse") | .value[1]'
```

### Detailed Answer:
1. ✅ **Agents**: 3+ healthy agents with recent heartbeats
2. ✅ **Partitions**: All partitions have exactly one owner
3. ✅ **Storage**: New segments appearing in MinIO every few minutes
4. ✅ **Metadata**: PostgreSQL accessible, queries < 100ms
5. ✅ **Pipeline**: Can produce + consume test message in < 1 second
6. ✅ **Metrics**: All metrics endpoints returning data
7. ✅ **Alerts**: No firing alerts in Prometheus
8. ✅ **Lag**: Consumer lag < threshold for all groups

If all 8 are green → **System is healthy** ✓
