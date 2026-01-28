# StreamHouse Go-Live Checklist

**Complete checklist for taking StreamHouse to production**

## Pre-Launch Verification

### 1. Infrastructure Setup ✓

```bash
# Start all services
docker-compose up -d

# Verify services are running
docker ps | grep streamhouse

# Setup MinIO bucket permissions
./scripts/setup-minio.sh

# Run health check
./scripts/check-system-health.sh
```

**Expected**: All services healthy (PostgreSQL, MinIO, Prometheus, Grafana)

### 2. Test Distributed System ✓

```bash
# Run complete distributed demo
./scripts/demo-distributed-real.sh
```

**Expected Output**:
- 3 agents registered with heartbeats
- Partition leases distributed across agents
- Segments written to MinIO
- No orphaned partitions

**Verification**:
```bash
# Check agents
sqlite3 ./data/distributed-agents.db "
SELECT agent_id,
       COUNT(*) as partitions,
       CAST((julianday('now') - julianday(last_heartbeat_at)) * 86400 AS INTEGER) as seconds_ago
FROM agents a
LEFT JOIN partition_leases pl ON a.agent_id = pl.owner_agent_id
WHERE last_heartbeat_at > datetime('now', '-60 seconds')
GROUP BY agent_id;
"

# Check MinIO segments
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive | wc -l
```

### 3. Test End-to-End Pipeline ✓

```bash
# Run complete pipeline demo
./scripts/demo-complete.sh
```

**Expected Output**:
- 3 topics created (my-orders, user-events, metrics)
- 90+ messages produced
- 60+ segments in MinIO
- All data queryable

**Verification**:
```bash
# Check topics
cargo run --bin streamctl -- topic list

# Check segments in MinIO
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/my-orders/ --recursive
```

## Production Deployment

### 1. Deploy Agents

**For each agent node**:

```bash
# Set environment variables
export AGENT_ID="agent-$(hostname)"
export METADATA_DB="postgresql://streamhouse:password@postgres-host:5432/streamhouse_metadata"
export AWS_ENDPOINT_URL="http://minio-host:9000"
export AWS_ACCESS_KEY_ID="your-access-key"
export AWS_SECRET_ACCESS_KEY="your-secret-key"
export STREAMHOUSE_BUCKET="streamhouse-data"
export AWS_REGION="us-east-1"
export AVAILABILITY_ZONE="us-east-1a"
export RUST_LOG="info"

# Start agent (using systemd service or supervisor)
cargo run --release --bin streamhouse-agent
```

**Or use Docker**:
```bash
docker run -d \
  --name streamhouse-agent-1 \
  -e AGENT_ID=agent-1 \
  -e METADATA_DB=postgresql://... \
  -e AWS_ENDPOINT_URL=http://minio:9000 \
  -e AWS_ACCESS_KEY_ID=minioadmin \
  -e AWS_SECRET_ACCESS_KEY=minioadmin \
  -e STREAMHOUSE_BUCKET=streamhouse-data \
  streamhouse:latest
```

### 2. Deploy gRPC Server

```bash
# Start server
export STREAMHOUSE_ADDR="0.0.0.0:50051"
export METADATA_DB="postgresql://streamhouse:password@postgres-host:5432/streamhouse_metadata"
export AWS_ENDPOINT_URL="http://minio-host:9000"
export AWS_ACCESS_KEY_ID="your-access-key"
export AWS_SECRET_ACCESS_KEY="your-secret-key"
export STREAMHOUSE_BUCKET="streamhouse-data"
export RUST_LOG="info"

cargo run --release --bin streamhouse-server
```

### 3. Create Topics

```bash
# Set server address
export STREAMHOUSE_ADDR="http://your-server-lb:50051"

# Create production topics
cargo run --bin streamctl -- topic create orders --partitions 12
cargo run --bin streamctl -- topic create user-events --partitions 6
cargo run --bin streamctl -- topic create metrics --partitions 4
```

### 4. Configure Load Balancer

**Example Nginx config** for gRPC:
```nginx
upstream streamhouse_backend {
    server agent1:50051;
    server agent2:50051;
    server agent3:50051;
}

server {
    listen 50051 http2;

    location / {
        grpc_pass grpc://streamhouse_backend;
        grpc_set_header Host $host;
    }
}
```

## Monitoring Setup

### 1. Configure Prometheus

Edit `prometheus/prometheus.yml`:
```yaml
scrape_configs:
  - job_name: 'streamhouse-agents'
    static_configs:
      - targets:
        - 'agent1:8080'
        - 'agent2:8080'
        - 'agent3:8080'
    scrape_interval: 15s
```

### 2. Import Grafana Dashboards

```bash
# Open Grafana
open http://localhost:3000

# Login: admin / admin

# Import dashboard
# Dashboard → Import → Upload JSON
# File: grafana/dashboards/streamhouse-overview.json
```

### 3. Setup Alerts

**Critical alerts to configure**:

1. **Agent Down** (check every 30s):
```sql
SELECT COUNT(*) FROM agents
WHERE last_heartbeat_at < datetime('now', '-120 seconds');
-- Alert if > 0
```

2. **Orphaned Partitions** (check every 1 min):
```sql
SELECT COUNT(*) FROM partitions p
LEFT JOIN partition_leases pl ON p.topic = pl.topic AND p.partition_id = pl.partition_id
WHERE pl.owner_agent_id IS NULL OR pl.expires_at < datetime('now');
-- Alert if > 0
```

3. **High Consumer Lag** (check every 2 min):
```sql
SELECT MAX(p.high_watermark - co.committed_offset) as max_lag
FROM consumer_offsets co
JOIN partitions p ON co.topic = p.topic AND co.partition_id = p.partition_id;
-- Alert if > 10000
```

4. **MinIO Storage** (check every 5 min):
```bash
docker exec streamhouse-minio mc du myminio/streamhouse-data
# Alert if > 80% of capacity
```

### 4. Setup Automated Health Checks

**Cron job** (run every 5 minutes):
```bash
# Add to crontab
*/5 * * * * /path/to/streamhouse/scripts/check-system-health.sh >> /var/log/streamhouse-health.log 2>&1 || /usr/local/bin/alert-pagerduty.sh
```

## User Access

### 1. Client SDK (Rust)

**Provide users with connection details**:
```rust
// Example: client_config.rs
use streamhouse_client::{Producer, Consumer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Produce messages
    let producer = Producer::builder()
        .broker_addresses(vec!["streamhouse-lb.example.com:50051".to_string()])
        .build()
        .await?;

    producer.send("orders", None, b"order data", None).await?;
    producer.flush().await?;

    // Consume messages
    let consumer = Consumer::builder()
        .broker_addresses(vec!["streamhouse-lb.example.com:50051".to_string()])
        .group_id("order-processor")
        .topics(vec!["orders".to_string()])
        .build()
        .await?;

    let records = consumer.poll(Duration::from_secs(5)).await?;
    consumer.commit().await?;

    Ok(())
}
```

### 2. CLI Tool (streamctl)

**Deploy CLI to user machines**:
```bash
# Install
cargo install --path crates/streamctl

# Configure
export STREAMHOUSE_ADDR="http://streamhouse-lb.example.com:50051"

# Use
streamctl topic create my-topic --partitions 4
streamctl produce my-topic --partition 0 --value '{"key": "value"}'
streamctl consume my-topic --partition 0 --limit 10
```

### 3. gRPC API (Any language)

**Provide .proto files** from `crates/streamhouse-proto/proto/`:
- `streamhouse.proto` - Producer/Consumer API
- `metadata.proto` - Topic management

**Example clients**:
- Python: `pip install grpcio grpcio-tools`
- Go: `go get google.golang.org/grpc`
- Java: `implementation 'io.grpc:grpc-protobuf:1.58.0'`

## Daily Operations

### Morning Health Check (5 minutes)

```bash
# 1. Run automated health check
./scripts/check-system-health.sh

# 2. Check for errors
tail -100 /var/log/streamhouse-server.log | grep ERROR

# 3. Check Grafana dashboard
open http://grafana:3000/d/streamhouse-overview

# 4. Verify data flow
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive | tail -20
```

### Weekly Maintenance (15 minutes)

```bash
# 1. Check disk usage
docker exec streamhouse-minio mc du myminio/streamhouse-data

# 2. Review consumer lag trends
sqlite3 $METADATA_DB "
SELECT group_id, topic, AVG(lag) as avg_lag
FROM (
    SELECT co.group_id, co.topic,
           p.high_watermark - co.committed_offset as lag
    FROM consumer_offsets co
    JOIN partitions p ON co.topic = p.topic AND co.partition_id = p.partition_id
)
GROUP BY group_id, topic;
"

# 3. Check for stale topics
sqlite3 $METADATA_DB "
SELECT t.name,
       COUNT(DISTINCT pl.owner_agent_id) as agent_count,
       SUM(p.high_watermark) as total_messages
FROM topics t
LEFT JOIN partitions p ON t.name = p.topic
LEFT JOIN partition_leases pl ON t.name = pl.topic AND pl.expires_at > datetime('now')
GROUP BY t.name;
"
```

## Troubleshooting Quick Reference

### Problem: Agents not getting leases
**Check**:
```bash
sqlite3 $METADATA_DB "SELECT * FROM agents WHERE last_heartbeat_at > datetime('now', '-120 seconds');"
```
**Fix**: Restart agents, check network connectivity to metadata store

### Problem: High consumer lag
**Check**:
```bash
sqlite3 $METADATA_DB "
SELECT co.group_id, co.topic, co.partition_id,
       p.high_watermark - co.committed_offset as lag
FROM consumer_offsets co
JOIN partitions p ON co.topic = p.topic AND co.partition_id = p.partition_id
ORDER BY lag DESC
LIMIT 10;
"
```
**Fix**: Add more consumer instances, scale up consumers

### Problem: MinIO storage full
**Check**:
```bash
docker exec streamhouse-minio mc du myminio/streamhouse-data
```
**Fix**: Implement retention policy, archive old segments, increase storage

### Problem: Orphaned partitions
**Check**:
```bash
sqlite3 $METADATA_DB "
SELECT p.topic, p.partition_id
FROM partitions p
LEFT JOIN partition_leases pl ON p.topic = pl.topic AND p.partition_id = pl.partition_id
WHERE pl.owner_agent_id IS NULL OR pl.expires_at < datetime('now');
"
```
**Fix**: Wait for rebalance (30s), or restart coordinator

## Success Metrics

**System is healthy if**:
- ✅ All agents have heartbeats within last 60 seconds
- ✅ No orphaned partitions
- ✅ Consumer lag < 1000 messages
- ✅ MinIO storage < 80% full
- ✅ New segments created within last 5 minutes
- ✅ Server responding to gRPC requests (< 100ms)

## Support Documentation

- [PRODUCTION_GUIDE.md](./PRODUCTION_GUIDE.md) - Complete production guide
- [MONITORING_CHECKLIST.md](./MONITORING_CHECKLIST.md) - Daily monitoring tasks
- [DISTRIBUTED_SYSTEM_WORKING.md](./DISTRIBUTED_SYSTEM_WORKING.md) - Architecture details
- [QUICK_START.md](./QUICK_START.md) - Quick reference
- [scripts/check-system-health.sh](./scripts/check-system-health.sh) - Automated health check

## Emergency Contacts

- **On-call engineer**: [Your team's contact]
- **Alert channel**: #streamhouse-alerts
- **Escalation**: [Engineering manager contact]

---

**✅ Pre-Launch Checklist**

- [ ] All infrastructure services running (PostgreSQL, MinIO, Prometheus, Grafana)
- [ ] MinIO bucket configured with permissions
- [ ] Distributed agent demo passes (3 agents, partition distribution)
- [ ] End-to-end pipeline demo passes (90+ messages, 60+ segments)
- [ ] Health check script runs successfully
- [ ] Prometheus scraping metrics
- [ ] Grafana dashboards imported
- [ ] Alerts configured (agent down, orphaned partitions, consumer lag)
- [ ] Load balancer configured for gRPC
- [ ] Client SDK tested
- [ ] CLI tool deployed
- [ ] Documentation shared with users
- [ ] Daily health check scheduled
- [ ] On-call rotation established

**After all items checked, you're ready to go live! 🚀**
