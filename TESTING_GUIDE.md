# StreamHouse Testing Guide

Complete guide for testing Phases 1-7 (Producer → Agent → Storage → Consumer → Observability)

## Quick Start (3 Commands)

```bash
# 1. Start infrastructure (one-time setup)
./scripts/dev-setup.sh

# 2. Start StreamHouse server (Terminal 1)
source .env.dev && cargo run --release --features metrics --bin streamhouse-server

# 3. Run complete pipeline test (Terminal 2)
./scripts/test-complete-pipeline.sh
```

---

## Detailed Testing Steps

### Step 1: Start Infrastructure

Start all supporting services (PostgreSQL, MinIO, Prometheus, Grafana):

```bash
./scripts/dev-setup.sh
```

**What starts**:
- PostgreSQL (port 5432) - Metadata storage
- MinIO (ports 9000, 9001) - Object storage
- Prometheus (port 9090) - Metrics collection
- Grafana (port 3000) - Visualization
- Alertmanager (port 9093) - Alert routing
- Node Exporter (port 9100) - System metrics

**Verify**:
```bash
docker ps
# Should show 6 running containers
```

### Step 2: Start StreamHouse Server

In **Terminal 1**:

```bash
cd /Users/gabrielbram/Desktop/streamhouse
source .env.dev
cargo run --release --features metrics --bin streamhouse-server
```

**Expected output**:
```
INFO streamhouse_server: Initializing metadata store at ./data/metadata.db
INFO streamhouse_server: Initializing object store (bucket: streamhouse-data)
INFO streamhouse_server: StreamHouse server starting on 0.0.0.0:50051
```

**Verify server is running**:
```bash
# In Terminal 2
lsof -i :50051
# Should show streamhouse-server listening
```

### Step 3: Run Complete Pipeline Test

In **Terminal 2**:

```bash
./scripts/test-complete-pipeline.sh
```

**What this tests**:
- ✅ Infrastructure services running
- ✅ Server connectivity
- ✅ Topic creation in PostgreSQL
- ✅ Producer sending records
- ✅ Consumer receiving records
- ✅ Data stored in MinIO
- ✅ Metrics available in Prometheus
- ✅ Grafana accessible

### Step 4: View Metrics in Prometheus

Open Prometheus:
```bash
open http://localhost:9090
```

**Example queries**:
```promql
# Producer throughput (records/sec)
rate(streamhouse_producer_records_sent_total[5m])

# Consumer lag
streamhouse_consumer_lag_records

# Producer latency (P99)
histogram_quantile(0.99, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))

# Active partitions
streamhouse_agent_active_partitions
```

### Step 5: View Dashboards in Grafana

1. Open Grafana:
   ```bash
   open http://localhost:3000
   ```

2. Login:
   - Username: `admin`
   - Password: `admin`

3. Import dashboard:
   - Click **Dashboards** → **Import**
   - Click **Upload JSON file**
   - Select: `grafana/dashboards/streamhouse-overview.json`
   - Click **Import**

4. View the dashboard:
   - 7 panels showing producer/consumer metrics
   - Real-time updates every 5 seconds

### Step 6: Query PostgreSQL Metadata

```bash
# Connect to PostgreSQL
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata

# List all topics
SELECT * FROM topics;

# List all partitions
SELECT t.name, p.partition_id, p.high_watermark
FROM partitions p
JOIN topics t ON p.topic_id = t.id
ORDER BY t.name, p.partition_id;

# List consumer groups
SELECT * FROM consumer_groups;

# List consumer offsets
SELECT cg.group_id, t.name, co.partition_id, co.committed_offset
FROM consumer_offsets co
JOIN consumer_groups cg ON co.group_id = cg.id
JOIN topics t ON co.topic_id = t.id;

# Exit
\q
```

### Step 7: Browse MinIO Storage

**Web Console**:
```bash
open http://localhost:9001
```
- Username: `minioadmin`
- Password: `minioadmin`
- Browse bucket: `streamhouse-data`

**CLI**:
```bash
# Configure MinIO client (one-time)
docker exec streamhouse-minio mc alias set myminio http://localhost:9000 minioadmin minioadmin

# List buckets
docker exec streamhouse-minio mc ls myminio

# List objects in streamhouse-data bucket
docker exec streamhouse-minio mc ls myminio/streamhouse-data --recursive

# Download a segment
docker exec streamhouse-minio mc cp myminio/streamhouse-data/orders/0/segment_0000000000.dat /tmp/
```

### Step 8: Run Additional Examples

```bash
# Producer-Consumer end-to-end
cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer

# Offset tracking
cargo run --release --features metrics -p streamhouse-client --example e2e_offset_tracking

# Phase 7 complete demo (standalone)
cargo run --release --features metrics -p streamhouse-client --example phase_7_complete_demo
```

---

## Verification Checklist

After running the tests, verify:

- [ ] Server is running on port 50051
- [ ] PostgreSQL contains topics and partitions
- [ ] MinIO contains segment files
- [ ] Prometheus shows StreamHouse metrics
- [ ] Grafana dashboard displays data
- [ ] Producer successfully sends records
- [ ] Consumer successfully receives records
- [ ] Consumer lag is tracking correctly

---

## Troubleshooting

### Server won't start - "Address already in use"

**Problem**: Port 50051 is already in use

**Solution**:
```bash
# Find process using port
lsof -i :50051

# Kill the process
kill -9 <PID>

# Restart server
source .env.dev && cargo run --release --features metrics --bin streamhouse-server
```

### "No such binary: streamhouse-agent"

**Problem**: Incorrect binary name

**Solution**: Use `streamhouse-server` instead of `streamhouse-agent`

### Prometheus shows no StreamHouse metrics

**Problem**: Server not exposing metrics endpoint

**Check**:
```bash
curl http://localhost:8080/metrics
```

If this fails, ensure server was built with `--features metrics` flag.

### Docker containers not running

**Problem**: Infrastructure services stopped

**Solution**:
```bash
# Restart all services
cd /Users/gabrielbram/Desktop/streamhouse
docker-compose -f docker-compose.dev.yml up -d

# Check status
docker ps
```

### PostgreSQL connection refused

**Problem**: Database not ready

**Solution**:
```bash
# Check PostgreSQL health
docker exec streamhouse-postgres pg_isready -U streamhouse

# View logs
docker logs streamhouse-postgres
```

### MinIO not accessible

**Problem**: MinIO service not started

**Solution**:
```bash
# Check MinIO health
docker exec streamhouse-minio mc admin info myminio

# View logs
docker logs streamhouse-minio
```

---

## Port Reference

| Service | Port | Purpose |
|---------|------|---------|
| StreamHouse gRPC | 50051 | Producer/Consumer connections |
| StreamHouse Metrics | 8080 | Prometheus scraping |
| PostgreSQL | 5432 | Metadata storage |
| MinIO API | 9000 | S3-compatible storage |
| MinIO Console | 9001 | Web UI |
| Prometheus | 9090 | Metrics database |
| Grafana | 3000 | Dashboards |
| Alertmanager | 9093 | Alert routing |
| Node Exporter | 9100 | System metrics |

---

## Performance Benchmarks

Expected performance (from Phase 5 testing):

| Metric | Value |
|--------|-------|
| Producer throughput | 200K+ records/sec |
| Consumer throughput | 150K+ records/sec |
| Producer P99 latency | < 50ms |
| Consumer P99 latency | < 30ms |
| Metrics overhead | < 1% CPU |
| Memory overhead | < 100KB |

---

## Next Steps

1. **Explore the stack**:
   - Browse Grafana dashboards
   - Query Prometheus metrics
   - Inspect PostgreSQL metadata
   - Download MinIO segments

2. **Run load tests**:
   - Increase record count in examples
   - Monitor metrics in real-time
   - Check for alerts in Prometheus

3. **Customize observability**:
   - Add custom Grafana panels
   - Create new alert rules
   - Configure notification channels

4. **Production deployment**:
   - Review [OBSERVABILITY.md](docs/OBSERVABILITY.md)
   - Configure TLS for all services
   - Set up persistent storage
   - Enable authentication

---

## Resources

- **Complete Observability Guide**: [docs/OBSERVABILITY.md](docs/OBSERVABILITY.md)
- **Quick Start**: [OBSERVABILITY_QUICKSTART.md](OBSERVABILITY_QUICKSTART.md)
- **Phase 7 Details**: [PHASE_7_COMPLETE.md](PHASE_7_COMPLETE.md)
- **Demo Results**: [DEMO_COMPLETE.md](DEMO_COMPLETE.md)
- **Commands Cheat Sheet**: [COMMANDS_CHEAT_SHEET.md](COMMANDS_CHEAT_SHEET.md)

---

**Happy testing!** 🚀

Phase 7 (Observability) is complete and ready for production use.
