# StreamHouse Observability Quick Start

Get StreamHouse monitoring up and running in 5 minutes with Prometheus and Grafana.

## Prerequisites

- Docker and Docker Compose installed
- StreamHouse agents running with metrics enabled
- Basic understanding of Prometheus and Grafana

## Quick Start

### 1. Start Monitoring Stack

```bash
# Start Prometheus, Grafana, and Alertmanager
docker-compose -f docker-compose.observability.yml up -d

# Check services are running
docker-compose -f docker-compose.observability.yml ps
```

Expected output:
```
NAME                         STATUS    PORTS
streamhouse-prometheus       Up        0.0.0.0:9090->9090/tcp
streamhouse-grafana          Up        0.0.0.0:3000->3000/tcp
streamhouse-alertmanager     Up        0.0.0.0:9093->9093/tcp
streamhouse-node-exporter    Up        0.0.0.0:9100->9100/tcp
```

### 2. Configure Prometheus Targets

Edit `prometheus/prometheus.yml` and add your StreamHouse agent addresses:

```yaml
scrape_configs:
  - job_name: 'streamhouse-agents'
    static_configs:
      - targets:
          - 'your-agent-1:8080'
          - 'your-agent-2:8080'
          - 'your-agent-3:8080'
```

Reload Prometheus config:
```bash
curl -X POST http://localhost:9090/-/reload
```

### 3. Access Dashboards

**Grafana**: http://localhost:3000
- Username: `admin`
- Password: `admin`

**Prometheus**: http://localhost:9090

**Alertmanager**: http://localhost:9093

### 4. Import Pre-built Dashboard

In Grafana:
1. Go to Dashboards → Import
2. Upload `grafana/dashboards/streamhouse-overview.json`
3. Select "Prometheus" as data source
4. Click "Import"

### 5. Test Metrics

Check if metrics are being collected:

```bash
# Query Prometheus
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=streamhouse_producer_records_sent_total'

# Check agent health
curl http://your-agent:8080/health
curl http://your-agent:8080/ready
curl http://your-agent:8080/metrics
```

## Configuration

### Enable Metrics in StreamHouse

**Build with metrics feature**:
```bash
cargo build --release --features metrics
```

**Producer example**:
```rust
use streamhouse_client::{Producer, ProducerMetrics};
use prometheus_client::registry::Registry;

let mut registry = Registry::default();
let metrics = Arc::new(ProducerMetrics::new(&mut registry));

let producer = Producer::builder()
    .metadata_store(metadata_store)
    .metrics(metrics)
    .build()
    .await?;
```

**Consumer example**:
```rust
use streamhouse_client::{Consumer, ConsumerMetrics};

let metrics = Arc::new(ConsumerMetrics::new(&mut registry));

let consumer = Consumer::builder()
    .group_id("my-group")
    .topics(vec!["orders".to_string()])
    .metadata_store(metadata_store)
    .object_store(object_store)
    .metrics(metrics)
    .build()
    .await?;
```

### Configure Alerts

1. **Edit alert rules**: `prometheus/alerts.yml`
2. **Configure notifications**: `alertmanager/alertmanager.yml`
3. **Add Slack webhook**: Replace `YOUR_SLACK_WEBHOOK_URL` with your webhook URL
4. **Reload configs**:
   ```bash
   docker-compose -f docker-compose.observability.yml restart prometheus alertmanager
   ```

## Key Metrics to Monitor

### Producer Health
- **Throughput**: `rate(streamhouse_producer_records_sent_total[5m])`
- **Latency (P99)**: `histogram_quantile(0.99, rate(streamhouse_producer_send_duration_seconds_bucket[5m]))`
- **Error Rate**: `rate(streamhouse_producer_send_errors_total[5m])`

### Consumer Health
- **Throughput**: `rate(streamhouse_consumer_records_consumed_total[5m])`
- **Lag**: `streamhouse_consumer_lag_records`
- **Lag Trend**: `delta(streamhouse_consumer_lag_records[5m])`

### Agent Health
- **Active Partitions**: `streamhouse_agent_active_partitions`
- **Write Throughput**: `rate(streamhouse_agent_records_written_total[5m])`
- **gRPC Latency**: `histogram_quantile(0.99, rate(streamhouse_agent_grpc_request_duration_seconds_bucket[5m]))`

## Common Queries

### Producer Performance
```promql
# Total throughput across all producers
sum(rate(streamhouse_producer_records_sent_total[5m]))

# Throughput by topic
sum by (topic) (rate(streamhouse_producer_records_sent_total[5m]))

# Average batch size
rate(streamhouse_producer_batch_size_records_sum[5m]) /
rate(streamhouse_producer_batch_size_records_count[5m])
```

### Consumer Performance
```promql
# Consumer lag by group
sum by (group_id, topic) (streamhouse_consumer_lag_records)

# Consumer falling behind?
rate(streamhouse_producer_records_sent_total[5m]) -
rate(streamhouse_consumer_records_consumed_total[5m])

# Consumer poll latency
histogram_quantile(0.99, rate(streamhouse_consumer_poll_duration_seconds_bucket[5m]))
```

### Agent Performance
```promql
# Total agents up
count(up{job="streamhouse-agents"} == 1)

# Partitions per agent
sum by (instance) (streamhouse_agent_active_partitions)

# gRPC error rate
rate(streamhouse_agent_grpc_requests_total{status!="OK"}[5m])
```

## Alerting

Pre-configured alerts include:

**Critical Alerts** (PagerDuty + Slack):
- Agent down
- Critical consumer lag (>100K records)
- Critical producer error rate (>100 errors/sec)
- Disk space < 10%

**Warning Alerts** (Slack only):
- High consumer lag (>10K records)
- Consumer stalled (no consumption for 10min)
- High producer latency (P99 >100ms)
- High error rate (>10 errors/sec)

Test alerts:
```bash
# Simulate critical alert
curl -X POST http://localhost:9093/api/v1/alerts -d '[{
  "labels": {
    "alertname": "TestAlert",
    "severity": "critical"
  },
  "annotations": {
    "summary": "This is a test alert"
  }
}]'
```

## Troubleshooting

### Metrics Not Appearing

1. **Check feature flag**:
   ```bash
   cargo build --features metrics
   ```

2. **Verify endpoint**:
   ```bash
   curl http://agent-host:8080/metrics
   ```

3. **Check Prometheus targets**:
   - Open http://localhost:9090/targets
   - Verify all targets are "UP"

4. **Check logs**:
   ```bash
   docker-compose -f docker-compose.observability.yml logs prometheus
   ```

### Dashboard Not Loading

1. **Verify data source**:
   - Grafana → Configuration → Data Sources
   - Test connection to Prometheus

2. **Check metrics exist**:
   - Prometheus → Graph
   - Try query: `streamhouse_producer_records_sent_total`

3. **Reload dashboard**:
   - Dashboards → Manage → StreamHouse Overview → Settings → JSON Model
   - Re-import the dashboard

### Alerts Not Firing

1. **Check alert rules**:
   - Prometheus → Alerts
   - Verify rules are loaded and evaluating

2. **Test Alertmanager**:
   ```bash
   # Check status
   curl http://localhost:9093/-/healthy

   # List active alerts
   curl http://localhost:9093/api/v2/alerts
   ```

3. **Verify webhook/email config**:
   - Check `alertmanager/alertmanager.yml`
   - Test webhook URL manually

## Production Deployment

For production deployments:

1. **Secure Grafana**:
   - Change default admin password
   - Enable HTTPS
   - Configure authentication (LDAP, OAuth)

2. **Persistent Storage**:
   - Use named volumes for data persistence
   - Configure backup strategy

3. **High Availability**:
   - Run multiple Prometheus instances
   - Use Thanos for long-term storage
   - Deploy Alertmanager cluster

4. **Resource Limits**:
   ```yaml
   services:
     prometheus:
       deploy:
         resources:
           limits:
             cpus: '2'
             memory: 4G
   ```

5. **Retention**:
   ```yaml
   command:
     - '--storage.tsdb.retention.time=90d'
     - '--storage.tsdb.retention.size=50GB'
   ```

## Next Steps

1. **Customize Dashboards**: Create panels for your specific use cases
2. **Set Up Alerts**: Configure Slack/PagerDuty for your team
3. **Enable Long-term Storage**: Set up Thanos or Cortex
4. **Create Runbooks**: Document incident response procedures
5. **Add More Metrics**: Instrument your application code

## Resources

- [Full Documentation](docs/OBSERVABILITY.md)
- [Phase 7 Summary](PHASE_7_OBSERVABILITY_FOUNDATION.md)
- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [PromQL Cheat Sheet](https://promlabs.com/promql-cheat-sheet/)

## Support

For issues or questions:
- GitHub Issues: https://github.com/yourusername/streamhouse/issues
- Documentation: https://streamhouse.io/docs
- Community: https://discord.gg/streamhouse

---

**Quick Start Complete!** 🎉

Your StreamHouse monitoring stack is now running. Access Grafana at http://localhost:3000 and start monitoring your cluster!
