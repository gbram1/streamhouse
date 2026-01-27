# StreamHouse Commands Cheat Sheet

Quick reference for running the complete StreamHouse pipeline with observability.

---

## 🚀 Quick Start (3 Commands)

```bash
# 1. Start everything
./scripts/dev-setup.sh

# 2. Start agent with metrics
source .env.dev && cargo run --release --features metrics --bin streamhouse-agent

# 3. Run demo (in another terminal)
source .env.dev && cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer
```

---

## 🌐 Web Interfaces

| Service | URL | Credentials |
|---------|-----|-------------|
| **Grafana** | http://localhost:3000 | admin / admin |
| **Prometheus** | http://localhost:9090 | (none) |
| **MinIO Console** | http://localhost:9001 | minioadmin / minioadmin |
| **Alertmanager** | http://localhost:9093 | (none) |
| **Metrics Endpoint** | http://localhost:8080/metrics | (none) |
| **Health Check** | http://localhost:8080/health | (none) |

---

## 📊 View Metrics

```bash
# All metrics
curl http://localhost:8080/metrics

# Just StreamHouse metrics
curl -s http://localhost:8080/metrics | grep "^streamhouse_"

# Count metrics
curl -s http://localhost:8080/metrics | grep -c "^streamhouse_"

# Health checks
curl http://localhost:8080/health    # Liveness
curl http://localhost:8080/ready     # Readiness
```

---

## 🔍 Prometheus Queries

```promql
# Producer throughput (records/sec)
rate(streamhouse_producer_records_sent_total[5m])

# Consumer lag
streamhouse_consumer_lag_records

# Producer P95 latency
histogram_quantile(0.95, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))

# Agent active partitions
streamhouse_agent_active_partitions

# All StreamHouse metrics
{__name__=~"streamhouse_.*"}

# System CPU usage
100 - (avg by (instance) (irate(node_cpu_seconds_total{mode="idle"}[5m])) * 100)
```

---

## 🗄️ Database Access

```bash
# PostgreSQL shell
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata

# Quick queries
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT * FROM topics;"
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT * FROM partitions;"
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT * FROM agents;"

# MinIO list
docker exec streamhouse-minio-setup /bin/sh -c "/usr/bin/mc ls myminio/streamhouse-data --recursive"
```

---

## 🎯 Run Examples

```bash
# Phase 7 complete demo
cargo run --release --features metrics -p streamhouse-client --example phase_7_complete_demo

# E2E producer-consumer
cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer

# Offset tracking
cargo run --release --features metrics -p streamhouse-client --example e2e_offset_tracking

# Observability demo (no compilation needed)
./scripts/demo-observability.sh
```

---

## 🐳 Docker Commands

```bash
# Start all services
docker-compose -f docker-compose.dev.yml up -d

# Stop all services
docker-compose -f docker-compose.dev.yml down

# View logs
docker-compose -f docker-compose.dev.yml logs -f

# Restart specific service
docker-compose -f docker-compose.dev.yml restart prometheus

# Remove everything (including volumes)
docker-compose -f docker-compose.dev.yml down -v
```

---

## 📈 Grafana Dashboard

```bash
# Open Grafana
open http://localhost:3000

# Import dashboard via API
curl -X POST http://admin:admin@localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -d @grafana/dashboards/streamhouse-overview.json

# Or manually:
# 1. Login (admin/admin)
# 2. Dashboards → Import
# 3. Upload: grafana/dashboards/streamhouse-overview.json
```

---

## 🚨 Alert Rules

**17 pre-configured rules in** `prometheus/alerts.yml`

View in browser: http://localhost:9090/alerts

```bash
# Check alerts via API
curl -s 'http://localhost:9090/api/v1/alerts' | python3 -m json.tool
```

**Categories:**
- Consumer Alerts (4): Lag, stalls
- Producer Alerts (5): Errors, latency, throughput
- Agent Alerts (5): Health, partitions, gRPC
- System Alerts (3): CPU, memory, disk

---

## 🔧 Agent Management

```bash
# Start agent (foreground)
source .env.dev
cargo run --release --features metrics --bin streamhouse-agent

# Start agent (background)
cargo run --release --features metrics --bin streamhouse-agent > /tmp/agent.log 2>&1 &

# View background logs
tail -f /tmp/agent.log

# Stop background agent
pkill -f streamhouse-agent
```

---

## 🧪 Test Specific Features

```bash
# Test producer metrics
cargo run --release --features metrics -p streamhouse-client --example simple_producer

# Test consumer lag API
# (Create a consumer and call consumer.lag("topic", partition))

# Monitor metrics in real-time
watch -n 2 'curl -s http://localhost:8080/metrics | grep streamhouse_producer_records_sent_total'

# Monitor Prometheus targets
watch -n 5 'curl -s http://localhost:9090/api/v1/targets | python3 -m json.tool'
```

---

## 📊 Metrics Available

### Producer (8 metrics)
- `streamhouse_producer_records_sent_total` - Total records sent
- `streamhouse_producer_bytes_sent_total` - Total bytes sent
- `streamhouse_producer_send_duration_seconds` - Send latency (histogram)
- `streamhouse_producer_batch_flush_duration_seconds` - Batch flush latency
- `streamhouse_producer_batch_size_records` - Batch size in records
- `streamhouse_producer_batch_size_bytes` - Batch size in bytes
- `streamhouse_producer_send_errors_total` - Send errors
- `streamhouse_producer_pending_records` - Pending records (gauge)

### Consumer (7 metrics)
- `streamhouse_consumer_records_consumed_total` - Total records consumed
- `streamhouse_consumer_bytes_consumed_total` - Total bytes consumed
- `streamhouse_consumer_poll_duration_seconds` - Poll latency (histogram)
- `streamhouse_consumer_lag_records` - Consumer lag in records (gauge)
- `streamhouse_consumer_lag_seconds` - Consumer lag in seconds (gauge)
- `streamhouse_consumer_last_committed_offset` - Last committed offset (gauge)
- `streamhouse_consumer_current_offset` - Current offset (gauge)

### Agent (7 metrics)
- `streamhouse_agent_active_partitions` - Active partitions (gauge)
- `streamhouse_agent_lease_renewals_total` - Lease renewals
- `streamhouse_agent_records_written_total` - Records written
- `streamhouse_agent_write_latency_seconds` - Write latency (histogram)
- `streamhouse_agent_active_connections` - Active connections (gauge)
- `streamhouse_agent_grpc_requests_total` - gRPC requests
- `streamhouse_agent_grpc_request_duration_seconds` - gRPC latency (histogram)

---

## 🛠️ Troubleshooting

```bash
# Check service health
curl http://localhost:9090/-/healthy     # Prometheus
curl http://localhost:3000/api/health    # Grafana
docker exec streamhouse-postgres pg_isready -U streamhouse  # PostgreSQL

# Check ports in use
lsof -i :3000   # Grafana
lsof -i :9090   # Prometheus
lsof -i :5432   # PostgreSQL
lsof -i :9000   # MinIO
lsof -i :8080   # Agent metrics

# View service logs
docker-compose -f docker-compose.dev.yml logs prometheus
docker-compose -f docker-compose.dev.yml logs grafana
docker-compose -f docker-compose.dev.yml logs postgres

# Reset everything
docker-compose -f docker-compose.dev.yml down -v
./scripts/dev-setup.sh
```

---

## 📚 Documentation

| Document | Description |
|----------|-------------|
| `QUICK_START_GUIDE.md` | Step-by-step instructions (this guide) |
| `COMMANDS_CHEAT_SHEET.md` | Quick command reference |
| `docs/OBSERVABILITY.md` | Complete observability guide (4000+ lines) |
| `OBSERVABILITY_QUICKSTART.md` | 5-minute quick start |
| `DEMO_COMPLETE.md` | Demo results and summary |
| `PHASE_7_COMPLETE.md` | Phase 7 technical details |

---

## 🎯 Common Workflows

### Workflow 1: First Time Setup
```bash
cd /Users/gabrielbram/Desktop/streamhouse
./scripts/dev-setup.sh
open http://localhost:3000  # Import dashboard
```

### Workflow 2: Daily Development
```bash
# Terminal 1: Start agent
source .env.dev
cargo run --release --features metrics --bin streamhouse-agent

# Terminal 2: Run your code
cargo run --release --features metrics -p streamhouse-client --example your_example

# Browser: Watch metrics
open http://localhost:3000
```

### Workflow 3: Demo to Someone
```bash
./scripts/demo-observability.sh
cargo run --release --features metrics -p streamhouse-client --example phase_7_complete_demo
open http://localhost:3000
```

### Workflow 4: Debug Issues
```bash
# Check metrics
curl http://localhost:8080/metrics | grep error

# Check Prometheus
open http://localhost:9090/targets
open http://localhost:9090/alerts

# Check logs
docker-compose -f docker-compose.dev.yml logs -f

# Check database
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata
```

---

## 🚀 Production Checklist

Before deploying to production:

- [ ] Change Grafana password (default: admin/admin)
- [ ] Change MinIO credentials (default: minioadmin/minioadmin)
- [ ] Configure PostgreSQL with strong password
- [ ] Set up TLS for all services
- [ ] Configure Alertmanager for notifications (Slack/PagerDuty)
- [ ] Set up log aggregation
- [ ] Configure data retention policies
- [ ] Set up backups for PostgreSQL and Prometheus
- [ ] Review and adjust alert thresholds
- [ ] Enable authentication for metrics endpoints
- [ ] Configure network policies and firewalls

---

**Quick Help:**
```bash
./scripts/dev-setup.sh              # Start everything
./scripts/demo-observability.sh     # Show what's available
cargo run --example phase_7_complete_demo  # See it in action
```

**Need detailed instructions?** See [QUICK_START_GUIDE.md](QUICK_START_GUIDE.md)
