#!/bin/bash
set -e

# StreamHouse Observability Demo
# Demonstrates the complete monitoring stack (Phase 7)

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

print_header() {
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  $1${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

# Check services
check_services() {
    print_header "Checking Observability Stack"

    local all_good=true

    if curl -s http://localhost:9090/-/healthy &> /dev/null; then
        log_success "Prometheus is running at http://localhost:9090"
    else
        log_error "Prometheus is not running"
        all_good=false
    fi

    if curl -s http://localhost:3000/api/health &> /dev/null; then
        log_success "Grafana is running at http://localhost:3000"
    else
        log_error "Grafana is not running"
        all_good=false
    fi

    if curl -s http://localhost:9093/-/healthy &> /dev/null; then
        log_success "Alertmanager is running at http://localhost:9093"
    else
        log_warning "Alertmanager is not running (optional)"
    fi

    if curl -s http://localhost:9100/metrics &> /dev/null; then
        log_success "Node Exporter is running at http://localhost:9100"
    else
        log_warning "Node Exporter is not running (optional)"
    fi

    if [ "$all_good" = false ]; then
        log_error "Some critical services are not running. Run: ./scripts/dev-setup.sh"
        exit 1
    fi

    echo ""
}

# Show Prometheus metrics
show_prometheus() {
    print_header "Prometheus Configuration"

    echo -e "${CYAN}📊 Prometheus is scraping the following targets:${NC}"
    echo ""

    local targets=$(curl -s 'http://localhost:9090/api/v1/targets' | python3 -c "
import sys, json
data = json.load(sys.stdin)
for target in data.get('data', {}).get('activeTargets', []):
    job = target['labels'].get('job', 'unknown')
    instance = target['labels'].get('instance', 'unknown')
    health = target['health']
    print(f'  [{health.upper()}] {job:<25} {instance}')
" 2>/dev/null || echo "  (Could not parse targets)")

    echo ""
    echo -e "${CYAN}📈 Available metrics:${NC}"
    echo "  - prometheus_*          (Prometheus internal metrics)"
    echo "  - node_*                (System metrics from Node Exporter)"
    echo "  - streamhouse_*         (StreamHouse application metrics)"
    echo ""

    echo -e "${CYAN}💡 Example queries:${NC}"
    echo "  • rate(prometheus_http_requests_total[5m])"
    echo "  • node_cpu_seconds_total"
    echo "  • up{job='streamhouse-agents'}"
    echo "  • rate(streamhouse_producer_records_sent_total[5m])"
    echo "  • streamhouse_consumer_lag_records"
    echo ""

    echo -e "${YELLOW}➜ Open Prometheus:${NC} http://localhost:9090"
    echo ""
}

# Show Grafana setup
show_grafana() {
    print_header "Grafana Dashboards"

    echo -e "${CYAN}📊 Grafana is ready for visualization:${NC}"
    echo ""
    echo "  URL:      http://localhost:3000"
    echo "  Username: admin"
    echo "  Password: admin"
    echo ""

    echo -e "${CYAN}📁 Available dashboards:${NC}"
    echo "  • $PROJECT_ROOT/grafana/dashboards/streamhouse-overview.json"
    echo ""

    echo -e "${CYAN}💡 To import the StreamHouse dashboard:${NC}"
    echo "  1. Open http://localhost:3000"
    echo "  2. Login with admin/admin"
    echo "  3. Go to Dashboards → Import"
    echo "  4. Upload: grafana/dashboards/streamhouse-overview.json"
    echo ""

    echo -e "${CYAN}📊 Dashboard includes:${NC}"
    echo "  • Producer throughput (records/sec)"
    echo "  • Consumer throughput (records/sec)"
    echo "  • Consumer lag (records)"
    echo "  • Producer latency (P50/P95/P99)"
    echo "  • Agent active partitions"
    echo "  • Error rates"
    echo "  • Batch sizes"
    echo ""

    echo -e "${YELLOW}➜ Open Grafana:${NC} http://localhost:3000"
    echo ""
}

# Show alerting
show_alerting() {
    print_header "Alerting Configuration"

    echo -e "${CYAN}🚨 Configured alert rules (${PROJECT_ROOT}/prometheus/alerts.yml):${NC}"
    echo ""

    # Count alert rules
    local alert_count=$(grep -c 'alert:' $PROJECT_ROOT/prometheus/alerts.yml 2>/dev/null || echo "0")
    echo "  Total alert rules: ${alert_count}"
    echo ""

    echo -e "${CYAN}📋 Alert categories:${NC}"
    echo "  • Consumer Alerts:"
    echo "    - HighConsumerLag (lag > 10K records for 5m)"
    echo "    - CriticalConsumerLag (lag > 100K records for 5m)"
    echo "    - ConsumerLagGrowing (lag increased by 5K in 10m)"
    echo "    - ConsumerStalled (no records consumed for 10m)"
    echo ""

    echo "  • Producer Alerts:"
    echo "    - HighProducerErrorRate (>10 errors/sec for 5m)"
    echo "    - CriticalProducerErrorRate (>100 errors/sec for 2m)"
    echo "    - HighProducerLatency (P99 > 100ms for 5m)"
    echo "    - VeryHighProducerLatency (P99 > 1s for 2m)"
    echo "    - ProducerThroughputDrop (>50% drop for 10m)"
    echo ""

    echo "  • Agent Alerts:"
    echo "    - AgentDown (agent not responding for 1m)"
    echo "    - AgentNotReady (agent up but not ready for 5m)"
    echo "    - NoActivePartitions (no partitions for 5m)"
    echo "    - HighAgentgRPCErrors (>10 gRPC errors/sec for 5m)"
    echo "    - HighAgentWriteLatency (P99 > 500ms for 5m)"
    echo ""

    echo "  • System Alerts:"
    echo "    - HighCPUUsage (>80% for 10m)"
    echo "    - HighMemoryUsage (>90% for 5m)"
    echo "    - DiskSpaceLow (<10% remaining for 5m)"
    echo ""

    # Check alert status
    echo -e "${CYAN}📊 Current alert status:${NC}"
    local alerts=$(curl -s 'http://localhost:9090/api/v1/alerts' | python3 -c "
import sys, json
data = json.load(sys.stdin)
alerts = data.get('data', {}).get('alerts', [])
if alerts:
    for alert in alerts:
        name = alert.get('labels', {}).get('alertname', 'unknown')
        state = alert.get('state', 'unknown')
        print(f'  [{state.upper()}] {name}')
else:
    print('  ✓ No active alerts')
" 2>/dev/null || echo "  (Could not fetch alerts)")

    echo ""
    echo -e "${YELLOW}➜ View Alerts:${NC} http://localhost:9090/alerts"
    echo -e "${YELLOW}➜ Alertmanager:${NC} http://localhost:9093"
    echo ""
}

# Show metrics integration
show_integration() {
    print_header "StreamHouse Metrics Integration"

    echo -e "${CYAN}🔌 How to enable metrics in your code:${NC}"
    echo ""

    cat << 'EOF'
  // Producer with metrics
  use prometheus_client::registry::Registry;
  use streamhouse_client::ProducerMetrics;

  let mut registry = Registry::default();
  let metrics = Arc::new(ProducerMetrics::new(&mut registry));

  let producer = Producer::builder()
      .metadata_store(metadata_store)
      .metrics(Some(metrics))
      .build()
      .await?;

  // Consumer with metrics
  let consumer_metrics = Arc::new(ConsumerMetrics::new(&mut registry));

  let consumer = Consumer::builder()
      .group_id("my-group")
      .topics(vec!["orders"])
      .metadata_store(metadata_store)
      .object_store(object_store)
      .metrics(Some(consumer_metrics))
      .build()
      .await?;

  // Agent with metrics (automatically enabled with --features metrics)
  cargo run --features metrics --bin streamhouse-agent
EOF

    echo ""
    echo -e "${CYAN}📊 Metrics are exposed at:${NC}"
    echo "  • Agent:    http://localhost:8080/metrics"
    echo "  • Producer: (via agent metrics endpoint)"
    echo "  • Consumer: (via agent metrics endpoint)"
    echo ""

    echo -e "${CYAN}🏥 Health check endpoints:${NC}"
    echo "  • Liveness:  http://localhost:8080/health"
    echo "  • Readiness: http://localhost:8080/ready"
    echo ""
}

# Show documentation
show_docs() {
    print_header "Documentation & Resources"

    echo -e "${CYAN}📚 Available documentation:${NC}"
    echo "  • Observability Guide:    docs/OBSERVABILITY.md"
    echo "  • Quick Start:            OBSERVABILITY_QUICKSTART.md"
    echo "  • Prometheus Config:      prometheus/prometheus.yml"
    echo "  • Alert Rules:            prometheus/alerts.yml"
    echo "  • Grafana Dashboard:      grafana/dashboards/streamhouse-overview.json"
    echo ""

    echo -e "${CYAN}🔗 Useful links:${NC}"
    echo "  • Prometheus:       http://localhost:9090"
    echo "  • Grafana:          http://localhost:3000"
    echo "  • Alertmanager:     http://localhost:9093"
    echo "  • Node Exporter:    http://localhost:9100/metrics"
    echo "  • PostgreSQL:       postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
    echo "  • MinIO Console:    http://localhost:9001"
    echo ""

    echo -e "${CYAN}📖 Learn more:${NC}"
    echo "  • Prometheus docs:  https://prometheus.io/docs/"
    echo "  • Grafana docs:     https://grafana.com/docs/"
    echo "  • PromQL guide:     https://prometheus.io/docs/prometheus/latest/querying/basics/"
    echo ""
}

# Main execution
main() {
    print_header "StreamHouse Observability Demo (Phase 7)"

    check_services
    show_prometheus
    show_grafana
    show_alerting
    show_integration
    show_docs

    print_header "Next Steps"

    echo -e "${YELLOW}Ready to explore the observability stack!${NC}"
    echo ""
    echo "1. Open Grafana and import the dashboard:"
    echo -e "   ${BLUE}open http://localhost:3000${NC}"
    echo ""
    echo "2. Explore Prometheus metrics:"
    echo -e "   ${BLUE}open http://localhost:9090${NC}"
    echo ""
    echo "3. Run StreamHouse with metrics enabled:"
    echo -e "   ${BLUE}source .env.dev${NC}"
    echo -e "   ${BLUE}cargo run --features metrics --bin streamhouse-agent${NC}"
    echo ""
    echo "4. Check out the documentation:"
    echo -e "   ${BLUE}cat docs/OBSERVABILITY.md${NC}"
    echo ""
    echo -e "${GREEN}Happy monitoring! 📊${NC}"
    echo ""
}

main
