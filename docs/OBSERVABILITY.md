# StreamHouse Observability Guide

Complete guide for monitoring StreamHouse in production using Prometheus and Grafana.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Enabling Metrics](#enabling-metrics)
3. [Prometheus Setup](#prometheus-setup)
4. [Grafana Dashboards](#grafana-dashboards)
5. [Key Metrics Reference](#key-metrics-reference)
6. [Alerting Rules](#alerting-rules)
7. [Troubleshooting](#troubleshooting)

## Quick Start

### 1. Enable Metrics in Your Application

Add the `metrics` feature to your dependencies:

```toml
[dependencies]
streamhouse-client = { path = "../streamhouse-client", features = ["metrics"] }
streamhouse-agent = { path = "../streamhouse-agent", features = ["metrics"] }
```

### 2. Configure Producer with Metrics

```rust
use streamhouse_client::{Producer, ProducerMetrics};
use prometheus_client::registry::Registry;
use std::sync::Arc;

// Create Prometheus registry
let mut registry = Registry::default();

// Create producer metrics
let producer_metrics = Arc::new(ProducerMetrics::new(&mut registry));

// Build producer with metrics enabled
let producer = Producer::builder()
    .metadata_store(metadata_store)
    .agent_group("production")
    .batch_size(1000)
    .compression_enabled(true)
    .metrics(producer_metrics)  // Enable metrics
    .build()
    .await?;

// Produce records - metrics automatically recorded
producer.send("orders", Some(b"user123"), b"order_data", None).await?;
```

### 3. Configure Consumer with Metrics

```rust
use streamhouse_client::{Consumer, ConsumerMetrics, OffsetReset};

// Create consumer metrics
let consumer_metrics = Arc::new(ConsumerMetrics::new(&mut registry));

// Build consumer with metrics enabled
let mut consumer = Consumer::builder()
    .group_id("analytics-group")
    .topics(vec!["orders".to_string()])
    .metadata_store(metadata_store)
    .object_store(object_store)
    .offset_reset(OffsetReset::Earliest)
    .metrics(consumer_metrics)  // Enable metrics
    .build()
    .await?;

// Poll for records - metrics automatically recorded
let records = consumer.poll(Duration::from_secs(1)).await?;

// Query consumer lag programmatically
let lag = consumer.lag("orders", 0).await?;
println!("Current lag: {} records", lag);
```

### 4. Start Agent with Metrics Server

```rust
use streamhouse_agent::{Agent, AgentMetrics, MetricsServer};
use std::net::SocketAddr;

// Create agent metrics
let agent_metrics = Arc::new(AgentMetrics::new(&mut registry));

// Start agent with metrics
let agent = Agent::builder()
    .agent_id("agent-us-east-1a-001")
    .address("10.0.1.5:9090")
    .availability_zone("us-east-1a")
    .agent_group("production")
    .build()
    .await?;

// Start metrics HTTP server (separate task)
let metrics_server = MetricsServer::new(
    "0.0.0.0:8080".parse()?,
    Arc::new(registry),
    Arc::new(move || agent.has_active_leases()),
);

tokio::spawn(async move {
    metrics_server.start().await.expect("Metrics server failed");
});

// Start agent
agent.start().await?;
```

## Enabling Metrics

StreamHouse uses feature flags for zero-overhead observability when not needed.

### Feature Flags

**streamhouse-client**:
```toml
[features]
metrics = ["prometheus-client"]
```

**streamhouse-agent**:
```toml
[features]
metrics = ["prometheus-client", "axum"]
```

### Building with Metrics

```bash
# Build with metrics enabled
cargo build --release --features metrics

# Run tests with metrics
cargo test --features metrics

# Run examples with metrics
cargo run --release --features metrics --example producer_with_metrics
```

### Performance Impact

| Configuration | Overhead | Use Case |
|---------------|----------|----------|
| **No metrics** (default) | 0% | Development, testing |
| **With metrics** | < 1% | Production monitoring |

Metrics use lock-free atomic operations:
- Counter increment: ~5-10ns
- Histogram sample: ~50-100ns
- Total impact on 200K rec/s producer: < 1%

## Prometheus Setup

### 1. Expose Metrics Endpoint

Each StreamHouse agent exposes metrics on port 8080:

- `GET /health` - Liveness probe (always 200 OK)
- `GET /ready` - Readiness probe (200 OK if agent has active leases)
- `GET /metrics` - Prometheus exposition format

### 2. Prometheus Configuration

Create `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  # StreamHouse Agents
  - job_name: 'streamhouse-agents'
    static_configs:
      - targets:
          - 'agent-1.example.com:8080'
          - 'agent-2.example.com:8080'
          - 'agent-3.example.com:8080'
    relabel_configs:
      - source_labels: [__address__]
        target_label: instance
        regex: '([^:]+):.*'
        replacement: '${1}'

  # StreamHouse Producers (if exposing metrics)
  - job_name: 'streamhouse-producers'
    static_configs:
      - targets:
          - 'producer-1.example.com:8080'
          - 'producer-2.example.com:8080'

  # StreamHouse Consumers (if exposing metrics)
  - job_name: 'streamhouse-consumers'
    static_configs:
      - targets:
          - 'consumer-1.example.com:8080'
          - 'consumer-2.example.com:8080'
```

### 3. Service Discovery (Kubernetes)

For Kubernetes deployments, use ServiceMonitor:

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: streamhouse-agents
  namespace: streamhouse
spec:
  selector:
    matchLabels:
      app: streamhouse-agent
  endpoints:
  - port: metrics
    interval: 15s
    path: /metrics
```

### 4. Docker Compose Example

```yaml
version: '3.8'

services:
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
      - GF_INSTALL_PLUGINS=grafana-piechart-panel
    volumes:
      - grafana-data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
      - ./grafana/datasources.yml:/etc/grafana/provisioning/datasources/datasources.yml

volumes:
  prometheus-data:
  grafana-data:
```

### 5. Test Prometheus Scraping

```bash
# Start Prometheus
docker-compose up -d prometheus

# Check targets
open http://localhost:9090/targets

# Test query
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=streamhouse_producer_records_sent_total'
```

## Grafana Dashboards

### 1. Add Prometheus Data Source

`grafana/datasources.yml`:

```yaml
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: false
```

### 2. StreamHouse Overview Dashboard

Save as `grafana/dashboards/streamhouse-overview.json`:

```json
{
  "dashboard": {
    "title": "StreamHouse Overview",
    "tags": ["streamhouse"],
    "timezone": "browser",
    "panels": [
      {
        "title": "Producer Throughput",
        "targets": [
          {
            "expr": "rate(streamhouse_producer_records_sent_total[5m])",
            "legendFormat": "{{instance}} - {{topic}}"
          }
        ],
        "type": "graph",
        "gridPos": {"h": 8, "w": 12, "x": 0, "y": 0}
      },
      {
        "title": "Consumer Throughput",
        "targets": [
          {
            "expr": "rate(streamhouse_consumer_records_consumed_total[5m])",
            "legendFormat": "{{instance}} - {{topic}}"
          }
        ],
        "type": "graph",
        "gridPos": {"h": 8, "w": 12, "x": 12, "y": 0}
      },
      {
        "title": "Consumer Lag",
        "targets": [
          {
            "expr": "streamhouse_consumer_lag_records",
            "legendFormat": "{{topic}}/{{partition}} - {{group_id}}"
          }
        ],
        "type": "graph",
        "gridPos": {"h": 8, "w": 12, "x": 0, "y": 8}
      },
      {
        "title": "Producer Latency (P99)",
        "targets": [
          {
            "expr": "histogram_quantile(0.99, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))",
            "legendFormat": "{{instance}}"
          }
        ],
        "type": "graph",
        "gridPos": {"h": 8, "w": 12, "x": 12, "y": 8}
      }
    ]
  }
}
```

### 3. Key Dashboard Panels

#### Producer Metrics Panel

**Query**: Producer throughput
```promql
rate(streamhouse_producer_records_sent_total[5m])
```

**Query**: Producer error rate
```promql
rate(streamhouse_producer_send_errors_total[5m])
```

**Query**: Producer P99 latency
```promql
histogram_quantile(0.99, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))
```

**Query**: Batch size (average)
```promql
rate(streamhouse_producer_batch_size_records_sum[5m]) / rate(streamhouse_producer_batch_size_records_count[5m])
```

#### Consumer Metrics Panel

**Query**: Consumer throughput
```promql
rate(streamhouse_consumer_records_consumed_total[5m])
```

**Query**: Consumer lag by topic
```promql
streamhouse_consumer_lag_records{topic="orders"}
```

**Query**: Consumer lag trend
```promql
delta(streamhouse_consumer_lag_records[5m])
```

**Query**: Current vs committed offset
```promql
streamhouse_consumer_current_offset - streamhouse_consumer_last_committed_offset
```

#### Agent Metrics Panel

**Query**: Active partitions per agent
```promql
streamhouse_agent_active_partitions
```

**Query**: Records written per second
```promql
rate(streamhouse_agent_records_written_total[5m])
```

**Query**: gRPC request latency
```promql
histogram_quantile(0.99, rate(streamhouse_agent_grpc_request_duration_seconds_bucket[5m]))
```

### 4. Pre-built Dashboard JSON

Complete dashboard available at: [grafana/streamhouse-dashboard.json](../grafana/streamhouse-dashboard.json)

Import steps:
1. Go to Grafana → Dashboards → Import
2. Upload `streamhouse-dashboard.json`
3. Select Prometheus data source
4. Click "Import"

## Key Metrics Reference

### Producer Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `streamhouse_producer_records_sent_total` | Counter | topic, partition | Total records sent |
| `streamhouse_producer_bytes_sent_total` | Counter | topic | Total bytes sent |
| `streamhouse_producer_send_duration_seconds` | Histogram | topic | Latency of send() calls |
| `streamhouse_producer_batch_flush_duration_seconds` | Histogram | topic | Batch flush duration |
| `streamhouse_producer_batch_size_records` | Histogram | topic | Records per batch |
| `streamhouse_producer_batch_size_bytes` | Histogram | topic | Bytes per batch |
| `streamhouse_producer_send_errors_total` | Counter | topic, error_type | Total send errors |
| `streamhouse_producer_pending_records` | Gauge | topic, partition | Pending records awaiting offset |

### Consumer Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `streamhouse_consumer_records_consumed_total` | Counter | topic, partition, group_id | Total records consumed |
| `streamhouse_consumer_bytes_consumed_total` | Counter | topic, group_id | Total bytes consumed |
| `streamhouse_consumer_poll_duration_seconds` | Histogram | topic | Duration of poll() calls |
| `streamhouse_consumer_lag_records` | Gauge | topic, partition, group_id | Consumer lag in records |
| `streamhouse_consumer_lag_seconds` | Gauge | topic, partition, group_id | Consumer lag in time |
| `streamhouse_consumer_last_committed_offset` | Gauge | topic, partition, group_id | Last committed offset |
| `streamhouse_consumer_current_offset` | Gauge | topic, partition, group_id | Current read position |

### Agent Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `streamhouse_agent_active_partitions` | Gauge | topic | Active partitions |
| `streamhouse_agent_lease_renewals_total` | Counter | status | Lease renewal attempts |
| `streamhouse_agent_records_written_total` | Counter | topic, partition | Records written |
| `streamhouse_agent_write_latency_seconds` | Histogram | topic | Write latency |
| `streamhouse_agent_active_connections` | Gauge | - | Active gRPC connections |
| `streamhouse_agent_grpc_requests_total` | Counter | method, status | Total gRPC requests |
| `streamhouse_agent_grpc_request_duration_seconds` | Histogram | method | gRPC request duration |

### Common Queries

**Producer throughput by topic**:
```promql
sum by (topic) (rate(streamhouse_producer_records_sent_total[5m]))
```

**Consumer lag greater than 1000**:
```promql
streamhouse_consumer_lag_records > 1000
```

**P50, P95, P99 latencies**:
```promql
histogram_quantile(0.50, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))
histogram_quantile(0.95, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))
histogram_quantile(0.99, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))
```

**Error rate percentage**:
```promql
100 * rate(streamhouse_producer_send_errors_total[5m]) / rate(streamhouse_producer_records_sent_total[5m])
```

## Alerting Rules

### Prometheus Alerting Rules

Create `prometheus/alerts.yml`:

```yaml
groups:
  - name: streamhouse_alerts
    interval: 30s
    rules:
      # High Consumer Lag
      - alert: HighConsumerLag
        expr: streamhouse_consumer_lag_records > 10000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High consumer lag detected"
          description: "Consumer {{ $labels.group_id }} has {{ $value }} records lag on {{ $labels.topic }}/{{ $labels.partition }}"

      # Very High Consumer Lag (Critical)
      - alert: CriticalConsumerLag
        expr: streamhouse_consumer_lag_records > 100000
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Critical consumer lag detected"
          description: "Consumer {{ $labels.group_id }} has {{ $value }} records lag on {{ $labels.topic }}/{{ $labels.partition }}"

      # High Error Rate
      - alert: HighProducerErrorRate
        expr: rate(streamhouse_producer_send_errors_total[5m]) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High producer error rate"
          description: "Producer on {{ $labels.instance }} has {{ $value }} errors/sec"

      # High Latency
      - alert: HighProducerLatency
        expr: histogram_quantile(0.99, rate(streamhouse_producer_send_duration_seconds_bucket[5m])) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High producer latency"
          description: "P99 latency is {{ $value }}s on {{ $labels.instance }}"

      # Agent Down
      - alert: AgentDown
        expr: up{job="streamhouse-agents"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "StreamHouse agent is down"
          description: "Agent {{ $labels.instance }} is not responding"

      # No Active Partitions
      - alert: NoActivePartitions
        expr: streamhouse_agent_active_partitions == 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Agent has no active partitions"
          description: "Agent {{ $labels.instance }} has no active partition leases"

      # Consumer Not Consuming
      - alert: ConsumerStalled
        expr: rate(streamhouse_consumer_records_consumed_total[5m]) == 0
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Consumer is stalled"
          description: "Consumer {{ $labels.group_id }} has not consumed any records for 10 minutes"
```

Update `prometheus.yml` to load alerts:

```yaml
rule_files:
  - 'alerts.yml'

alerting:
  alertmanagers:
    - static_configs:
        - targets:
            - 'alertmanager:9093'
```

### Alertmanager Configuration

Create `alertmanager.yml`:

```yaml
global:
  resolve_timeout: 5m

route:
  group_by: ['alertname', 'cluster']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'slack'

receivers:
  - name: 'slack'
    slack_configs:
      - api_url: 'https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK'
        channel: '#streamhouse-alerts'
        title: '{{ template "slack.default.title" . }}'
        text: '{{ template "slack.default.text" . }}'

  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: 'YOUR_PAGERDUTY_KEY'
        description: '{{ template "pagerduty.default.description" . }}'
```

## Troubleshooting

### Metrics Not Appearing

**Check 1: Feature flag enabled**
```bash
cargo build --features metrics
```

**Check 2: Metrics endpoint accessible**
```bash
curl http://agent-host:8080/metrics
```

**Check 3: Prometheus scraping**
```bash
# Check Prometheus targets
open http://prometheus:9090/targets

# Check for specific metric
curl -G http://prometheus:9090/api/v1/query \
  --data-urlencode 'query=up{job="streamhouse-agents"}'
```

**Check 4: Registry configured**
```rust
// Ensure metrics are registered
let mut registry = Registry::default();
let metrics = Arc::new(ProducerMetrics::new(&mut registry));
```

### High Cardinality Issues

Avoid high-cardinality labels:

**Bad** ❌:
```rust
// Don't use unique IDs as labels
metrics.record_send_with_label("user_id", user_id);
```

**Good** ✅:
```rust
// Use low-cardinality labels like topic
metrics.record_send();  // Automatically labeled by topic
```

### Performance Impact

If metrics are causing performance issues:

1. **Disable metrics in development**:
   ```bash
   cargo build  # Without --features metrics
   ```

2. **Increase scrape interval**:
   ```yaml
   scrape_interval: 60s  # From 15s
   ```

3. **Sample histograms** (not yet implemented):
   ```rust
   // Future: Sample 10% of requests
   if rand::random::<f64>() < 0.1 {
       metrics.record_send(...);
   }
   ```

### Missing Metrics After Restart

**Issue**: Metrics reset to zero after restart

**Solution**: This is expected behavior. Prometheus records historical data:

```promql
# Use rate() for throughput (works across restarts)
rate(streamhouse_producer_records_sent_total[5m])

# Use delta() for changes
delta(streamhouse_consumer_lag_records[5m])
```

### Consumer Lag Spikes

**Query to investigate**:
```promql
# Lag increase rate
rate(streamhouse_consumer_lag_records[5m])

# Producer vs Consumer throughput
rate(streamhouse_producer_records_sent_total[5m]) -
rate(streamhouse_consumer_records_consumed_total[5m])
```

**Common causes**:
1. Consumer slower than producer
2. Consumer rebalancing (Phase 8)
3. Network issues
4. Consumer processing errors

## Production Checklist

- [ ] Metrics feature enabled in production builds
- [ ] Prometheus scraping configured for all agents
- [ ] Grafana dashboards imported and tested
- [ ] Alert rules configured and tested
- [ ] Alertmanager integrated with notification channels
- [ ] Consumer lag alerts configured
- [ ] High latency alerts configured
- [ ] Error rate alerts configured
- [ ] Agent health checks configured
- [ ] Metrics retention configured (30 days recommended)
- [ ] Backup strategy for Prometheus data
- [ ] Grafana authentication configured
- [ ] Dashboard permissions configured

## Next Steps

1. **Import Pre-built Dashboards**: Use the provided JSON templates
2. **Configure Alerts**: Set up Alertmanager for your notification channels
3. **Test Failover**: Verify alerts trigger correctly during agent failures
4. **Tune Scrape Intervals**: Balance between granularity and load
5. **Set Up Long-term Storage**: Configure Prometheus remote write for retention > 30 days

## Additional Resources

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [StreamHouse Phase 7 Summary](../PHASE_7_OBSERVABILITY_FOUNDATION.md)
- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
- [PromQL Cheat Sheet](https://promlabs.com/promql-cheat-sheet/)

---

**Last Updated**: January 26, 2026
**StreamHouse Version**: v0.3.0+
**Status**: Production Ready
