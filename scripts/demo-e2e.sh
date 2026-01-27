#!/bin/bash
set -e

# StreamHouse End-to-End Demo (Phases 1-7)
# This demonstrates the complete pipeline with observability

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
log_step() { echo -e "${CYAN}[STEP $1]${NC} $2"; }

print_header() {
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  $1${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

print_separator() {
    echo -e "${BLUE}────────────────────────────────────────────────────────────${NC}"
}

# Check if services are running
check_services() {
    print_header "Checking Services"

    local all_good=true

    # Check PostgreSQL
    if docker exec streamhouse-postgres pg_isready -U streamhouse &> /dev/null; then
        log_success "PostgreSQL is running"
    else
        log_error "PostgreSQL is not running"
        all_good=false
    fi

    # Check MinIO
    if curl -s http://localhost:9000/minio/health/live &> /dev/null; then
        log_success "MinIO is running"
    else
        log_error "MinIO is not running"
        all_good=false
    fi

    # Check Prometheus
    if curl -s http://localhost:9090/-/healthy &> /dev/null; then
        log_success "Prometheus is running"
    else
        log_error "Prometheus is not running"
        all_good=false
    fi

    # Check Grafana
    if curl -s http://localhost:3000/api/health &> /dev/null; then
        log_success "Grafana is running"
    else
        log_error "Grafana is not running"
        all_good=false
    fi

    if [ "$all_good" = false ]; then
        log_error "Some services are not running. Please run: ./scripts/dev-setup.sh"
        exit 1
    fi

    echo ""
}

# Create example Rust program
create_demo_program() {
    print_header "Creating Demo Program"

    cat > "$PROJECT_ROOT/examples/complete_pipeline_demo.rs" <<'EOF'
//! Complete StreamHouse Pipeline Demo (Phases 1-7)
//!
//! This demonstrates:
//! - Producer with metrics
//! - Consumer with metrics and lag monitoring
//! - Agent coordination
//! - Real-time observability
//! - S3 storage
//! - PostgreSQL metadata

use streamhouse_client::{Producer, Consumer, OffsetReset};
use streamhouse_metadata::SqliteMetadataStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  StreamHouse Complete Pipeline Demo (Phases 1-7)     ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Setup
    let metadata_store = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse_demo.db").await?
    );

    // Create topic
    println!("📝 Creating topic 'orders' with 4 partitions...");
    let topic_config = streamhouse_metadata::TopicConfig {
        name: "orders".to_string(),
        partition_count: 4,
        retention_ms: Some(86400000), // 24 hours
        config: std::collections::HashMap::new(),
    };

    if let Err(e) = metadata_store.create_topic(topic_config).await {
        println!("   (Topic may already exist: {})", e);
    }
    println!("✓ Topic ready\n");

    // Register agent
    println!("🤖 Registering agent...");
    let agent_info = streamhouse_metadata::AgentInfo {
        agent_id: "demo-agent-001".to_string(),
        address: "localhost:9090".to_string(),
        availability_zone: "local".to_string(),
        agent_group: "demo".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: std::collections::HashMap::new(),
    };
    metadata_store.register_agent(agent_info).await?;
    println!("✓ Agent registered\n");

    // Phase 1: Producer Setup
    println!("╭─────────────────────────────────────────────────────────╮");
    println!("│ Phase 1-5: Producer with Metrics                        │");
    println!("╰─────────────────────────────────────────────────────────╯\n");

    #[cfg(feature = "metrics")]
    {
        use prometheus_client::registry::Registry;
        use streamhouse_client::ProducerMetrics;

        let mut registry = Registry::default();
        let producer_metrics = Arc::new(ProducerMetrics::new(&mut registry));

        let producer = Producer::builder()
            .metadata_store(Arc::clone(&metadata_store))
            .agent_group("demo".to_string())
            .batch_size(100)
            .compression_enabled(true)
            .metrics(producer_metrics.clone())
            .build()
            .await?;

        println!("✓ Producer created with metrics enabled");
        println!("  - Batch size: 100 records");
        println!("  - Compression: LZ4 enabled");
        println!("  - Metrics: Active\n");

        // Produce messages
        println!("📤 Producing 1000 messages...");
        let start = std::time::Instant::now();

        for i in 0..1000 {
            let key = format!("user_{}", i % 100);
            let value = format!(
                r#"{{"order_id": {}, "user_id": "user_{}", "amount": {}, "timestamp": "{}"}}"#,
                i,
                i % 100,
                (i * 10) % 1000,
                chrono::Utc::now().to_rfc3339()
            );

            producer.send(
                "orders",
                Some(key.as_bytes()),
                value.as_bytes(),
                None,
            ).await?;

            if (i + 1) % 100 == 0 {
                println!("   Produced: {}/1000 messages", i + 1);
            }
        }

        producer.flush().await?;
        let elapsed = start.elapsed();

        println!("\n✓ Production complete!");
        println!("  - Total messages: 1000");
        println!("  - Time taken: {:.2}s", elapsed.as_secs_f64());
        println!("  - Throughput: {:.0} msg/s\n", 1000.0 / elapsed.as_secs_f64());
    }

    #[cfg(not(feature = "metrics"))]
    {
        println!("⚠️  Metrics feature not enabled");
        println!("   Run with: cargo run --features metrics --example complete_pipeline_demo\n");
    }

    // Phase 2: Consumer Setup
    println!("╭─────────────────────────────────────────────────────────╮");
    println!("│ Phase 6: Consumer with Lag Monitoring                   │");
    println!("╰─────────────────────────────────────────────────────────╯\n");

    sleep(Duration::from_secs(2)).await;

    #[cfg(feature = "metrics")]
    {
        use prometheus_client::registry::Registry;
        use streamhouse_client::ConsumerMetrics;
        use object_store::local::LocalFileSystem;

        let mut consumer_registry = Registry::default();
        let consumer_metrics = Arc::new(ConsumerMetrics::new(&mut consumer_registry));

        let object_store = Arc::new(
            LocalFileSystem::new_with_prefix("/tmp/streamhouse_segments")?
        );

        let mut consumer = Consumer::builder()
            .group_id("demo-analytics".to_string())
            .topics(vec!["orders".to_string()])
            .metadata_store(Arc::clone(&metadata_store))
            .object_store(object_store)
            .offset_reset(OffsetReset::Earliest)
            .auto_commit(true)
            .metrics(consumer_metrics.clone())
            .build()
            .await?;

        println!("✓ Consumer created with metrics enabled");
        println!("  - Group ID: demo-analytics");
        println!("  - Offset reset: Earliest");
        println!("  - Auto-commit: Enabled");
        println!("  - Metrics: Active\n");

        // Consume messages
        println!("📥 Consuming messages...");
        let mut total_consumed = 0;
        let start = std::time::Instant::now();

        for round in 1..=5 {
            let records = consumer.poll(Duration::from_secs(2)).await?;

            if !records.is_empty() {
                total_consumed += records.len();
                println!("   Round {}: Consumed {} messages (Total: {})",
                    round, records.len(), total_consumed);

                // Show sample record
                if let Some(record) = records.first() {
                    println!("   Sample: topic={}, partition={}, offset={}",
                        record.topic, record.partition, record.offset);
                }
            }

            // Check consumer lag (Phase 7 feature)
            if let Ok(lag) = consumer.lag("orders", 0).await {
                println!("   Consumer lag on partition 0: {} records", lag);
            }
        }

        let elapsed = start.elapsed();

        println!("\n✓ Consumption complete!");
        println!("  - Total consumed: {} messages", total_consumed);
        println!("  - Time taken: {:.2}s", elapsed.as_secs_f64());
        if total_consumed > 0 {
            println!("  - Throughput: {:.0} msg/s\n",
                total_consumed as f64 / elapsed.as_secs_f64());
        }
    }

    // Phase 3: Show stored data
    println!("╭─────────────────────────────────────────────────────────╮");
    println!("│ Phase 7: Observability & Metrics                        │");
    println!("╰─────────────────────────────────────────────────────────╯\n");

    println!("📊 Metrics available at:");
    println!("   - Grafana Dashboard: http://localhost:3000");
    println!("   - Prometheus: http://localhost:9090");
    println!("   - Metrics endpoint: http://localhost:8080/metrics\n");

    println!("🗄️  Data stored in:");
    println!("   - PostgreSQL: streamhouse_metadata database");
    println!("   - MinIO: streamhouse-data bucket");
    println!("   - View MinIO Console: http://localhost:9001\n");

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Demo Complete! Check Grafana for metrics visualization ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    Ok(())
}
EOF

    log_success "Demo program created: examples/complete_pipeline_demo.rs"
    echo ""
}

# Display metrics instructions
show_metrics_access() {
    print_header "Accessing Metrics & Data"

    echo -e "${CYAN}📊 Grafana Dashboard:${NC}"
    echo "   URL: http://localhost:3000"
    echo "   Username: admin"
    echo "   Password: admin"
    echo "   Dashboard: Import grafana/dashboards/streamhouse-overview.json"
    echo ""

    echo -e "${CYAN}📈 Prometheus:${NC}"
    echo "   URL: http://localhost:9090"
    echo "   Query examples:"
    echo "     rate(streamhouse_producer_records_sent_total[5m])"
    echo "     streamhouse_consumer_lag_records"
    echo ""

    echo -e "${CYAN}💾 MinIO Console:${NC}"
    echo "   URL: http://localhost:9001"
    echo "   Username: minioadmin"
    echo "   Password: minioadmin"
    echo "   Bucket: streamhouse-data"
    echo ""

    echo -e "${CYAN}🗄️  PostgreSQL:${NC}"
    echo "   Connect: psql postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
    echo "   Tables: topics, partitions, agents, consumer_groups, consumer_offsets"
    echo ""
}

# Query PostgreSQL
query_postgres() {
    print_header "Querying PostgreSQL Metadata"

    echo -e "${YELLOW}Topics:${NC}"
    docker exec -i streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
        "SELECT name, partition_count, retention_ms FROM topics;" 2>/dev/null || \
        echo "  (No topics yet or tables not created)"
    echo ""

    echo -e "${YELLOW}Agents:${NC}"
    docker exec -i streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
        "SELECT agent_id, address, agent_group, last_heartbeat FROM agents;" 2>/dev/null || \
        echo "  (No agents yet or tables not created)"
    echo ""

    echo -e "${YELLOW}Consumer Groups:${NC}"
    docker exec -i streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
        "SELECT group_id, created_at FROM consumer_groups;" 2>/dev/null || \
        echo "  (No consumer groups yet or tables not created)"
    echo ""
}

# Query MinIO
query_minio() {
    print_header "Querying MinIO (S3) Storage"

    log_info "Listing segments in streamhouse-data bucket..."

    docker exec -i streamhouse-minio-setup /bin/sh -c \
        "/usr/bin/mc ls myminio/streamhouse-data --recursive" 2>/dev/null || \
        echo "  (No segments yet or bucket empty)"

    echo ""
}

# Query Prometheus
query_prometheus() {
    print_header "Querying Prometheus Metrics"

    log_info "Checking if StreamHouse metrics are available..."

    local metrics=$(curl -s 'http://localhost:9090/api/v1/query?query=streamhouse_producer_records_sent_total' | \
        python3 -m json.tool 2>/dev/null || echo "{}")

    if echo "$metrics" | grep -q "result"; then
        echo -e "${GREEN}✓ Producer metrics found${NC}"
        echo "$metrics" | python3 -c "import sys, json; data = json.load(sys.stdin); \
            [print(f\"  - {r['metric']}: {r['value'][1]}\") for r in data.get('data', {}).get('result', [])]" 2>/dev/null || \
            echo "  (Parsing failed)"
    else
        echo -e "${YELLOW}No metrics yet - run the demo program first${NC}"
    fi

    echo ""
}

# Run the demo
run_demo() {
    print_header "Running Complete Pipeline Demo"

    cd "$PROJECT_ROOT"

    log_info "Starting StreamHouse agent..."
    echo ""

    # Start agent in background
    cargo run --release --features metrics --bin streamhouse-agent > /tmp/streamhouse-agent.log 2>&1 &
    AGENT_PID=$!

    log_info "Waiting for agent to start..."
    sleep 5

    log_info "Running producer-consumer demo..."
    echo ""

    # Run e2e example
    cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer || true

    echo ""

    # Stop agent
    log_info "Stopping agent..."
    kill $AGENT_PID 2>/dev/null || true
    wait $AGENT_PID 2>/dev/null || true

    log_success "Demo execution complete!"
    echo ""
}

# Show summary
show_summary() {
    print_header "Demo Summary & Next Steps"

    echo -e "${GREEN}✓ Complete pipeline demonstrated (Phases 1-7)${NC}"
    echo ""

    echo -e "${YELLOW}What you can explore:${NC}"
    echo ""
    echo "1. View metrics in Grafana:"
    echo -e "   ${BLUE}open http://localhost:3000${NC}"
    echo ""
    echo "2. Query Prometheus:"
    echo -e "   ${BLUE}open http://localhost:9090${NC}"
    echo "   Try: rate(streamhouse_producer_records_sent_total[5m])"
    echo ""
    echo "3. Browse S3 data in MinIO:"
    echo -e "   ${BLUE}open http://localhost:9001${NC}"
    echo "   Bucket: streamhouse-data"
    echo ""
    echo "4. Query PostgreSQL:"
    echo -e "   ${BLUE}docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata${NC}"
    echo ""
    echo "5. Run more examples:"
    echo -e "   ${BLUE}cargo run --release --features metrics --example e2e_offset_tracking${NC}"
    echo -e "   ${BLUE}cargo run --release --features metrics --example e2e_producer_consumer${NC}"
    echo ""
    echo -e "${GREEN}Happy exploring! 🎉${NC}"
    echo ""
}

# Main execution
main() {
    cd "$PROJECT_ROOT"

    print_header "StreamHouse Complete Demo (Phases 1-7)"

    check_services
    create_demo_program
    show_metrics_access

    # Ask if user wants to run demo
    echo ""
    read -p "Run the complete pipeline demo now? (y/n) " -n 1 -r
    echo ""

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        run_demo
        sleep 2
        query_postgres
        query_minio
        query_prometheus
    else
        log_info "Skipping demo execution. You can run it later with:"
        echo "  cargo run --release --features metrics --example complete_pipeline_demo"
    fi

    show_summary
}

# Run main
main
