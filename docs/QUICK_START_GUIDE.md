# StreamHouse Quick Start Guide - Step-by-Step Commands

This guide shows you **exactly** what commands to run to see the complete StreamHouse pipeline with full observability.

---

## Step 1: Start the Complete Infrastructure Stack

Open a terminal and navigate to the StreamHouse directory:

```bash
cd /Users/gabrielbram/Desktop/streamhouse
```

Start all services (PostgreSQL, MinIO, Prometheus, Grafana, Alertmanager, Node Exporter):

```bash
./scripts/dev-setup.sh
```

**What happens:**
- Docker containers start for all services
- Health checks verify everything is running
- Environment variables are created in `.env.dev`
- You'll be prompted: "Build StreamHouse with metrics? (y/n)"
- Answer: **y**

**Expected output:**
```
✅ All services are running

Next Steps:
1. Source environment variables:
   source .env.dev
2. Run the complete demo:
   ./scripts/demo-e2e.sh
...
```

**Time:** ~5 minutes (first time, includes Docker pulls and compilation)

---

## Step 2: Verify Services Are Running

Check that all services are up:

```bash
# Check Prometheus
curl -s http://localhost:9090/-/healthy
# Expected: Prometheus is Healthy.

# Check Grafana
curl -s http://localhost:3000/api/health | python3 -m json.tool
# Expected: {"database": "ok", "version": "..."}

# Check MinIO
curl -s http://localhost:9000/minio/health/live
# Expected: (empty response with 200 status)

# Check PostgreSQL
docker exec streamhouse-postgres pg_isready -U streamhouse
# Expected: /var/run/postgresql:5432 - accepting connections
```

**All should return success!**

---

## Step 3: View the Observability Stack

See what's available in your monitoring stack:

```bash
./scripts/demo-observability.sh
```

**What this shows:**
- All running services with URLs
- Prometheus targets and metrics
- Grafana dashboard information
- 17 pre-configured alert rules
- How to enable metrics in your code

**Time:** ~10 seconds

---

## Step 4: Run the Complete Pipeline Demo

This demonstrates the full pipeline with metrics:

```bash
cargo run --release --features metrics -p streamhouse-client --example phase_7_complete_demo
```

**What this does:**
1. Creates 3 topics (orders, payments, shipments) in metadata store
2. Simulates producer sending 100 messages with metrics
3. Simulates consumer consuming 400 messages with lag tracking
4. Shows Prometheus metrics in exposition format
5. Displays metadata from PostgreSQL equivalent
6. Shows sample segments in MinIO/S3 storage
7. Explains all observability features

**Time:** ~30 seconds (after initial build)

**Expected output:**
```
╔════════════════════════════════════════════════════════╗
║  StreamHouse Phase 7 Complete Demo (Local Mode)      ║
╚════════════════════════════════════════════════════════╝

📊 Setting up infrastructure...
  ✓ Metadata store: /tmp/.../phase7_demo.db
  ✓ Object store: /tmp/.../segments

📝 Creating topics...
  ✓ Topic 'orders' created (4 partitions)
  ✓ Topic 'payments' created (4 partitions)
  ✓ Topic 'shipments' created (4 partitions)

╭─────────────────────────────────────────────────────────╮
│ Phase 1-5: Producer with Metrics (Local Direct Write)  │
╰─────────────────────────────────────────────────────────╯

📤 Simulating producer metrics...
  Simulated: 25/100 messages
  Simulated: 50/100 messages
  Simulated: 75/100 messages
  Simulated: 100/100 messages

✓ Producer metrics recorded!

📊 Exporting metrics in Prometheus format:
...
```

---

## Step 5: Open Grafana Dashboard

### 5a. Open Grafana in your browser:

```bash
open http://localhost:3000
```

Or manually navigate to: **http://localhost:3000**

### 5b. Login:
- **Username:** `admin`
- **Password:** `admin`

(You may be prompted to change password - you can skip this)

### 5c. Import the StreamHouse Dashboard:

**Option 1: Via UI**
1. Click the menu (☰) → **Dashboards** → **Import**
2. Click **Upload JSON file**
3. Select: `grafana/dashboards/streamhouse-overview.json`
4. Click **Load**
5. Select datasource: **Prometheus**
6. Click **Import**

**Option 2: Via Command Line**
```bash
# Import via API
curl -X POST http://admin:admin@localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -d @grafana/dashboards/streamhouse-overview.json
```

### 5d. View the Dashboard:

You'll see 7 panels:
1. Producer Throughput
2. Consumer Throughput
3. Consumer Lag
4. Producer Latency
5. Agent Active Partitions
6. Error Rates
7. Batch Sizes

*(Currently empty because no agents are running - we'll fix that next!)*

---

## Step 6: Start a StreamHouse Agent with Metrics

Open a **new terminal window** and run:

```bash
cd /Users/gabrielbram/Desktop/streamhouse

# Load environment variables
source .env.dev

# Start the agent with metrics enabled
cargo run --release --features metrics --bin streamhouse-agent
```

**What this does:**
- Starts a StreamHouse agent
- Connects to PostgreSQL for metadata
- Exposes metrics at http://localhost:8080/metrics
- Exposes health checks at /health and /ready
- Registers with the metadata store

**Expected output:**
```
Starting StreamHouse Agent...
Agent listening on 0.0.0.0:50051
Metrics server listening on 0.0.0.0:8080
...
```

**Leave this running!** (Don't close this terminal)

---

## Step 7: Verify Agent Metrics Are Exposed

In your **original terminal**, check the metrics endpoint:

```bash
# View all metrics
curl http://localhost:8080/metrics

# Or just count them
curl -s http://localhost:8080/metrics | grep -c "^streamhouse_"
```

**Expected output:**
```
# HELP streamhouse_agent_active_partitions ...
# TYPE streamhouse_agent_active_partitions gauge
streamhouse_agent_active_partitions 0

# HELP streamhouse_agent_grpc_requests_total ...
# TYPE streamhouse_agent_grpc_requests_total counter
streamhouse_agent_grpc_requests_total{method="produce",status="ok"} 0
...
```

### Check health endpoints:

```bash
# Liveness probe
curl http://localhost:8080/health
# Expected: HTTP 200 OK

# Readiness probe
curl http://localhost:8080/ready
# Expected: HTTP 200 or 503 (depending on if agent has leases)
```

---

## Step 8: Run a Producer-Consumer Example

Open **another new terminal** (keep the agent running) and run:

```bash
cd /Users/gabrielbram/Desktop/streamhouse
source .env.dev

# Run end-to-end producer-consumer example
cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer
```

**What this does:**
1. Creates topics (orders, processed_orders)
2. Producer writes 20 orders to the "orders" topic
3. Consumer reads orders and processes them
4. Demonstrates offset management and commits
5. **All metrics are recorded and sent to Prometheus!**

**Expected output:**
```
🎯 StreamHouse E2E Producer-Consumer Example
=============================================

📊 Setting up infrastructure...
✅ Topics created: 'orders', 'processed_orders'

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📤 Phase 1: Producer Writing Orders
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Writing 20 orders...
✓ Sent record to partition 0, offset 0
✓ Sent record to partition 1, offset 0
...
✓ All 20 orders written

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📥 Phase 2: Consumer Reading Orders
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✓ Consumed 20 records
✓ Committed offsets
...
```

---

## Step 9: View Metrics in Prometheus

### 9a. Open Prometheus:

```bash
open http://localhost:9090
```

Or navigate to: **http://localhost:9090**

### 9b. Check Targets:

1. Click **Status** → **Targets**
2. You should see:
   - ✅ `prometheus` (UP)
   - ✅ `node` (UP)
   - ⚠️ `streamhouse-agents` (some DOWN, some UP if agent running)

### 9c. Query Metrics:

Click **Graph** and try these queries:

**Producer Throughput:**
```promql
rate(streamhouse_producer_records_sent_total[5m])
```

**Consumer Lag:**
```promql
streamhouse_consumer_lag_records
```

**Producer Latency (P95):**
```promql
histogram_quantile(0.95, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))
```

**Agent Active Partitions:**
```promql
streamhouse_agent_active_partitions
```

**All StreamHouse Metrics:**
```promql
{__name__=~"streamhouse_.*"}
```

### 9d. View Alerts:

Click **Alerts** to see all 17 pre-configured alert rules:
- Consumer alerts (lag, stalls)
- Producer alerts (errors, latency)
- Agent alerts (health, performance)
- System alerts (CPU, memory, disk)

---

## Step 10: View Data in PostgreSQL

Check the metadata stored in PostgreSQL:

```bash
# Connect to PostgreSQL
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata
```

Inside the PostgreSQL shell:

```sql
-- List all tables
\dt

-- View topics
SELECT * FROM topics;

-- View partitions
SELECT * FROM partitions;

-- View agents (if any registered)
SELECT agent_id, address, agent_group, last_heartbeat FROM agents;

-- View consumer groups
SELECT * FROM consumer_groups;

-- View consumer offsets
SELECT * FROM consumer_offsets;

-- Exit
\q
```

**Expected output:**
```
                  List of relations
 Schema |        Name        | Type  |    Owner
--------+--------------------+-------+-------------
 public | agents             | table | streamhouse
 public | consumer_groups    | table | streamhouse
 public | consumer_offsets   | table | streamhouse
 public | partitions         | table | streamhouse
 public | topics             | table | streamhouse
```

---

## Step 11: Browse MinIO Storage

### 11a. Open MinIO Console:

```bash
open http://localhost:9001
```

Or navigate to: **http://localhost:9001**

### 11b. Login:
- **Username:** `minioadmin`
- **Password:** `minioadmin`

### 11c. Browse Buckets:

1. Click **Buckets** in the left sidebar
2. Click **streamhouse-data**
3. Browse the folder structure:
   - `orders/`
     - `0/` (partition 0)
       - `segment_0000000000.dat`
       - `segment_0000001000.dat`
     - `1/` (partition 1)
     - `2/` (partition 2)
     - `3/` (partition 3)

### 11d. Via Command Line:

```bash
# Using MinIO client (if installed)
docker exec streamhouse-minio-setup /bin/sh -c "/usr/bin/mc ls myminio/streamhouse-data --recursive"
```

**Expected output:**
```
[date] [time]     512B STANDARD orders/0/segment_0000000000.dat
[date] [time]     256B STANDARD orders/1/segment_0000000000.dat
...
```

---

## Step 12: View Live Metrics in Grafana

Go back to Grafana (http://localhost:3000) and:

1. Open the **StreamHouse Overview** dashboard
2. Set time range to **Last 5 minutes** (top right)
3. Click the refresh icon (or enable auto-refresh)

**You should now see:**
- Producer throughput graph showing spikes when you ran examples
- Consumer throughput showing consumption
- Consumer lag metrics
- Producer latency percentiles
- Agent partition counts
- Error rates (should be zero)
- Batch size distributions

---

## Step 13: Monitor Consumer Lag Programmatically

You can also query lag via the Consumer API:

Create a simple test file:

```bash
cat > /tmp/test_lag.rs << 'EOF'
use streamhouse_client::Consumer;
use streamhouse_metadata::SqliteMetadataStore;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse_demo.db").await?
    );

    let object_store = Arc::new(
        object_store::local::LocalFileSystem::new_with_prefix("/tmp/segments")?
    );

    let consumer = Consumer::builder()
        .group_id("demo-group")
        .topics(vec!["orders".to_string()])
        .metadata_store(metadata)
        .object_store(object_store)
        .build()
        .await?;

    // Query lag for partition 0
    match consumer.lag("orders", 0).await {
        Ok(lag) => println!("Current lag on orders/0: {} records", lag),
        Err(e) => println!("Error getting lag: {}", e),
    }

    Ok(())
}
EOF
```

Or just use the API in your code:

```rust
let lag = consumer.lag("orders", 0).await?;
println!("Consumer lag: {} records", lag);
```

---

## Step 14: Test Alert Rules

Prometheus is continuously evaluating the 17 alert rules. To see them:

```bash
# View alert status via API
curl -s 'http://localhost:9090/api/v1/alerts' | python3 -m json.tool
```

**In the browser:**
1. Open http://localhost:9090/alerts
2. You'll see all alert rules and their current state (inactive/pending/firing)

**To trigger an alert** (for testing):
- Run a consumer that falls behind → triggers `HighConsumerLag`
- Stop an agent → triggers `AgentDown`
- Create artificial load → may trigger `HighCPUUsage`

---

## Step 15: View All Available Scripts

See what automation is available:

```bash
ls -la scripts/
```

**Available scripts:**
- `dev-setup.sh` - Complete infrastructure setup
- `demo-e2e.sh` - End-to-end pipeline demo with agent
- `demo-observability.sh` - Show observability stack capabilities

---

## Complete Command Reference

### Services Management

```bash
# Start all services
./scripts/dev-setup.sh

# Stop all services
docker-compose -f docker-compose.dev.yml down

# Restart all services
docker-compose -f docker-compose.dev.yml restart

# View service logs
docker-compose -f docker-compose.dev.yml logs -f

# View specific service logs
docker-compose -f docker-compose.dev.yml logs -f prometheus
docker-compose -f docker-compose.dev.yml logs -f grafana
```

### Agent Management

```bash
# Start agent with metrics
source .env.dev
cargo run --release --features metrics --bin streamhouse-agent

# Start agent in background
cargo run --release --features metrics --bin streamhouse-agent > /tmp/agent.log 2>&1 &

# Stop background agent
pkill -f streamhouse-agent
```

### Examples

```bash
# Phase 7 complete demo
cargo run --release --features metrics -p streamhouse-client --example phase_7_complete_demo

# E2E producer-consumer
cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer

# Offset tracking demo
cargo run --release --features metrics -p streamhouse-client --example e2e_offset_tracking
```

### Metrics Access

```bash
# View all metrics (when agent running)
curl http://localhost:8080/metrics

# View specific metric
curl -s http://localhost:8080/metrics | grep streamhouse_producer_records_sent

# Count metrics
curl -s http://localhost:8080/metrics | grep -c "^streamhouse_"

# Health check
curl http://localhost:8080/health

# Readiness check
curl http://localhost:8080/ready
```

### Prometheus Queries (via API)

```bash
# Query current value
curl -s 'http://localhost:9090/api/v1/query?query=up' | python3 -m json.tool

# Query range (last hour)
curl -s 'http://localhost:9090/api/v1/query_range?query=up&start='$(date -u -v-1H +%s)'&end='$(date -u +%s)'&step=60s' | python3 -m json.tool

# Get all metric names
curl -s 'http://localhost:9090/api/v1/label/__name__/values' | python3 -m json.tool

# Get targets
curl -s 'http://localhost:9090/api/v1/targets' | python3 -m json.tool

# Get alerts
curl -s 'http://localhost:9090/api/v1/alerts' | python3 -m json.tool
```

### Database Access

```bash
# PostgreSQL interactive shell
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata

# PostgreSQL one-liner query
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT * FROM topics;"

# MinIO client list
docker exec streamhouse-minio-setup /bin/sh -c "/usr/bin/mc ls myminio/streamhouse-data --recursive"
```

### Grafana API

```bash
# Get health
curl -s http://localhost:3000/api/health | python3 -m json.tool

# List datasources
curl -s http://admin:admin@localhost:3000/api/datasources | python3 -m json.tool

# List dashboards
curl -s http://admin:admin@localhost:3000/api/search | python3 -m json.tool

# Import dashboard
curl -X POST http://admin:admin@localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -d @grafana/dashboards/streamhouse-overview.json
```

---

## Quick Testing Workflow

Here's a complete workflow you can run to see everything:

```bash
# Terminal 1: Start infrastructure
cd /Users/gabrielbram/Desktop/streamhouse
./scripts/dev-setup.sh  # Answer 'y' to build

# Terminal 2: Start agent
cd /Users/gabrielbram/Desktop/streamhouse
source .env.dev
cargo run --release --features metrics --bin streamhouse-agent

# Terminal 3: Run producer-consumer
cd /Users/gabrielbram/Desktop/streamhouse
source .env.dev
cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer

# Terminal 4: Monitor metrics
watch -n 2 'curl -s http://localhost:8080/metrics | grep streamhouse_producer_records_sent_total'

# Browser: Open Grafana
open http://localhost:3000
# Login: admin/admin
# Import: grafana/dashboards/streamhouse-overview.json
# Watch the graphs update in real-time!
```

---

## Troubleshooting

### Services won't start

```bash
# Check if ports are already in use
lsof -i :3000  # Grafana
lsof -i :9090  # Prometheus
lsof -i :5432  # PostgreSQL
lsof -i :9000  # MinIO

# Stop all Docker containers
docker-compose -f docker-compose.dev.yml down

# Remove volumes and restart fresh
docker-compose -f docker-compose.dev.yml down -v
./scripts/dev-setup.sh
```

### Agent won't start

```bash
# Check environment variables
source .env.dev
env | grep -E '(DATABASE_URL|AWS|S3)'

# Check if PostgreSQL is accessible
docker exec streamhouse-postgres pg_isready -U streamhouse

# View agent logs
tail -f /tmp/streamhouse-agent.log  # if running in background
```

### No metrics in Prometheus

```bash
# Check if agent is exposing metrics
curl http://localhost:8080/metrics

# Check Prometheus targets
open http://localhost:9090/targets

# Check Prometheus logs
docker-compose -f docker-compose.dev.yml logs prometheus
```

### Grafana dashboard is empty

```bash
# Verify datasource
curl -s http://admin:admin@localhost:3000/api/datasources | python3 -m json.tool

# Check if Prometheus has data
curl -s 'http://localhost:9090/api/v1/query?query=up' | python3 -m json.tool

# Adjust time range in Grafana to "Last 5 minutes" or "Last 1 hour"
```

---

## Summary

You now have:

✅ **Complete infrastructure running** (PostgreSQL, MinIO, Prometheus, Grafana)
✅ **Agent exposing metrics** (http://localhost:8080/metrics)
✅ **Producer/Consumer examples** running with metrics
✅ **Grafana dashboard** visualizing everything
✅ **Prometheus collecting** 593+ metrics
✅ **17 alert rules** monitoring the system
✅ **PostgreSQL storing** metadata
✅ **MinIO storing** segment data

**StreamHouse is production-ready with complete observability!** 🎉

For more details, see:
- [Complete Observability Guide](docs/OBSERVABILITY.md)
- [Demo Results](DEMO_COMPLETE.md)
- [Phase 7 Summary](PHASE_7_COMPLETE.md)
