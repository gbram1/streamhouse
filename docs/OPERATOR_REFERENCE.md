# StreamHouse Operator Reference Card

**Quick reference for daily operations and troubleshooting**

---

## Daily Health Check (2 minutes)

```bash
# Automated check (returns 0=healthy, 1=degraded, 2=critical)
./scripts/check-system-health.sh

# Manual checks
docker ps | grep streamhouse                    # All services running
curl -s http://localhost:9090/-/healthy         # Prometheus
curl -s http://localhost:3000/api/health        # Grafana
```

---

## Critical Queries

### Agent Health
```sql
SELECT agent_id,
       COUNT(*) as partitions,
       CAST((julianday('now') - julianday(last_heartbeat_at)) * 86400 AS INTEGER) as seconds_ago
FROM agents a
LEFT JOIN partition_leases pl ON a.agent_id = pl.owner_agent_id
WHERE last_heartbeat_at > datetime('now', '-60 seconds')
GROUP BY agent_id;
```
**Alert if**: seconds_ago > 60 or agent count < expected

### Partition Coverage
```sql
SELECT COUNT(*) as orphaned_partitions
FROM partitions p
LEFT JOIN partition_leases pl ON p.topic = pl.topic AND p.partition_id = pl.partition_id
WHERE pl.owner_agent_id IS NULL OR pl.expires_at < datetime('now');
```
**Alert if**: orphaned_partitions > 0

### Consumer Lag
```sql
SELECT co.group_id, co.topic,
       SUM(p.high_watermark - co.committed_offset) as total_lag
FROM consumer_offsets co
JOIN partitions p ON co.topic = p.topic AND co.partition_id = p.partition_id
GROUP BY co.group_id, co.topic
HAVING total_lag > 1000;
```
**Alert if**: total_lag > 10,000

---

## Quick Troubleshooting

### Problem: Agent not responding
```bash
# Check agent status
sqlite3 $METADATA_DB "SELECT * FROM agents WHERE agent_id = 'agent-1';"

# Check logs
tail -50 /var/log/streamhouse-agent-1.log | grep ERROR

# Restart agent
systemctl restart streamhouse-agent-1
```

### Problem: Orphaned partitions
```bash
# List orphans
sqlite3 $METADATA_DB "
SELECT p.topic, p.partition_id
FROM partitions p
LEFT JOIN partition_leases pl ON p.topic = pl.topic AND p.partition_id = pl.partition_id
WHERE pl.owner_agent_id IS NULL OR pl.expires_at < datetime('now');"

# Fix: Wait 30s for rebalance or restart coordinator
```

### Problem: High consumer lag
```bash
# Identify lagging partitions
sqlite3 $METADATA_DB "
SELECT co.group_id, co.topic, co.partition_id,
       p.high_watermark - co.committed_offset as lag
FROM consumer_offsets co
JOIN partitions p ON co.topic = p.topic AND co.partition_id = p.partition_id
ORDER BY lag DESC LIMIT 10;"

# Fix: Scale up consumers, check consumer logs
```

### Problem: MinIO not accessible
```bash
# Check MinIO status
docker ps | grep minio
docker logs streamhouse-minio | tail -50

# Test access
docker exec streamhouse-minio mc ls myminio/streamhouse-data

# Reconfigure bucket (if 403 errors)
./scripts/setup-minio.sh
```

---

## Common Commands

### Topic Management
```bash
# List topics
cargo run --bin streamctl -- topic list

# Create topic
cargo run --bin streamctl -- topic create orders --partitions 12

# Describe topic
cargo run --bin streamctl -- topic describe orders
```

### Message Operations
```bash
# Produce message
cargo run --bin streamctl -- produce orders --partition 0 --value '{"id": 123}'

# Consume messages
cargo run --bin streamctl -- consume orders --partition 0 --limit 10

# Consume from offset
cargo run --bin streamctl -- consume orders --partition 0 --offset 100 --limit 20
```

### Data Verification
```bash
# Count segments in MinIO
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive | grep -c '\.seg'

# Check storage size
docker exec streamhouse-minio mc du myminio/streamhouse-data

# View recent segments
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/orders/0/ --recursive | tail -10
```

### Metadata Queries
```bash
# View all topics
sqlite3 $METADATA_DB "SELECT * FROM topics;"

# View partitions
sqlite3 $METADATA_DB "SELECT * FROM partitions ORDER BY topic, partition_id;"

# View active leases
sqlite3 $METADATA_DB "
SELECT topic, partition_id, owner_agent_id, epoch,
       CAST((julianday(expires_at) - julianday('now')) * 86400 AS INTEGER) as expires_in_seconds
FROM partition_leases
WHERE expires_at > datetime('now')
ORDER BY topic, partition_id;"
```

---

## Alert Thresholds

| Metric | Threshold | Severity | Action |
|--------|-----------|----------|--------|
| Agent heartbeat age | > 120s | CRITICAL | Restart agent |
| Orphaned partitions | > 0 | CRITICAL | Wait 30s or restart |
| Consumer lag | > 10,000 | WARNING | Scale consumers |
| Consumer lag | > 100,000 | CRITICAL | Scale consumers urgently |
| MinIO storage | > 80% | WARNING | Archive old data |
| MinIO storage | > 95% | CRITICAL | Increase storage |
| Segments/5min | = 0 | WARNING | Check producers |
| Server latency | > 100ms | WARNING | Investigate load |

---

## Service URLs

| Service | URL | Credentials |
|---------|-----|-------------|
| MinIO Console | http://localhost:9001 | minioadmin / minioadmin |
| Prometheus | http://localhost:9090 | - |
| Grafana | http://localhost:3000 | admin / admin |
| PostgreSQL | localhost:5432 | streamhouse / streamhouse_dev |
| gRPC Server | localhost:50051 | - |

---

## Emergency Procedures

### Complete System Restart
```bash
# 1. Stop all agents
systemctl stop streamhouse-agent-*

# 2. Stop server
systemctl stop streamhouse-server

# 3. Restart infrastructure
docker-compose restart

# 4. Verify infrastructure
./scripts/check-system-health.sh

# 5. Start server
systemctl start streamhouse-server

# 6. Start agents
systemctl start streamhouse-agent-*

# 7. Verify agents registered
sqlite3 $METADATA_DB "SELECT * FROM agents;"
```

### Data Recovery
```bash
# List all segments
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive > segments_backup.txt

# Download segment
docker exec streamhouse-minio mc cp myminio/streamhouse-data/data/orders/0/00000000000000000000.seg ./local_backup/

# Restore segment
docker exec streamhouse-minio mc cp ./local_backup/00000000000000000000.seg myminio/streamhouse-data/data/orders/0/
```

---

## Environment Variables

### Server
```bash
export STREAMHOUSE_ADDR="0.0.0.0:50051"
export METADATA_DB="postgresql://streamhouse:password@localhost:5432/streamhouse_metadata"
export AWS_ENDPOINT_URL="http://localhost:9000"
export AWS_ACCESS_KEY_ID="minioadmin"
export AWS_SECRET_ACCESS_KEY="minioadmin"
export STREAMHOUSE_BUCKET="streamhouse-data"
export AWS_REGION="us-east-1"
export AWS_ALLOW_HTTP="true"
export RUST_LOG="info"
```

### Agent
```bash
export AGENT_ID="agent-$(hostname)"
export METADATA_DB="postgresql://streamhouse:password@localhost:5432/streamhouse_metadata"
export AWS_ENDPOINT_URL="http://localhost:9000"
export AWS_ACCESS_KEY_ID="minioadmin"
export AWS_SECRET_ACCESS_KEY="minioadmin"
export STREAMHOUSE_BUCKET="streamhouse-data"
export AWS_REGION="us-east-1"
export AVAILABILITY_ZONE="us-east-1a"
export RUST_LOG="info"
```

### CLI
```bash
export STREAMHOUSE_ADDR="http://localhost:50051"
```

---

## Log Locations

```bash
# Server logs
tail -f ./data/demo-server.log
tail -f /var/log/streamhouse-server.log

# Agent logs
tail -f /var/log/streamhouse-agent-1.log

# System health log
tail -f /var/log/streamhouse-health.log

# Docker logs
docker logs -f streamhouse-postgres
docker logs -f streamhouse-minio
docker logs -f streamhouse-prometheus
docker logs -f streamhouse-grafana
```

---

## Performance Benchmarks

**Normal Operation**:
- Throughput: 1,000-10,000 msg/sec per agent
- Producer latency: 10-50ms (p99)
- Consumer lag: < 1,000 messages
- Heartbeat age: < 30 seconds
- Segments created: 5-50 per 5 minutes (depends on load)

---

## Quick Links

- **Full Guide**: [PRODUCTION_GUIDE.md](./PRODUCTION_GUIDE.md)
- **Monitoring**: [MONITORING_CHECKLIST.md](./MONITORING_CHECKLIST.md)
- **Architecture**: [DISTRIBUTED_SYSTEM_WORKING.md](./DISTRIBUTED_SYSTEM_WORKING.md)
- **Quick Start**: [QUICK_START.md](./QUICK_START.md)
- **Go-Live**: [GO_LIVE_CHECKLIST.md](./GO_LIVE_CHECKLIST.md)
- **System Status**: [SYSTEM_COMPLETE.md](./SYSTEM_COMPLETE.md)

---

**Keep this card accessible for quick reference during operations 🔧**
