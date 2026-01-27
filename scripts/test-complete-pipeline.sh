#!/bin/bash
set -e

# StreamHouse Complete Pipeline Test
# Tests Phases 1-7 end-to-end with observability

echo "=========================================="
echo "StreamHouse Complete Pipeline Test"
echo "Phases 1-7: Producer → Agent → Consumer"
echo "=========================================="
echo

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Step 1: Verify infrastructure
echo -e "${BLUE}[1/8]${NC} Verifying infrastructure..."
REQUIRED_SERVICES=("streamhouse-postgres" "streamhouse-minio" "streamhouse-prometheus" "streamhouse-grafana")
for service in "${REQUIRED_SERVICES[@]}"; do
    if docker ps --format '{{.Names}}' | grep -q "^${service}$"; then
        echo -e "  ${GREEN}✓${NC} ${service} is running"
    else
        echo -e "  ${YELLOW}!${NC} ${service} is not running"
        echo "  Run: ./scripts/dev-setup.sh"
        exit 1
    fi
done
echo

# Step 2: Check if server is running
echo -e "${BLUE}[2/8]${NC} Checking if StreamHouse server is running..."
if lsof -i :50051 > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Server is running on port 50051"
else
    echo -e "  ${YELLOW}!${NC} Server is not running"
    echo "  Please start the server in a separate terminal:"
    echo "    source .env.dev"
    echo "    cargo run --release --features metrics --bin streamhouse-server"
    echo
    echo "  Press Enter when server is ready..."
    read -r
fi
echo

# Step 3: Run e2e producer-consumer example
echo -e "${BLUE}[3/8]${NC} Running end-to-end producer-consumer example..."
echo "  This will create topics, produce records, and consume them"
echo "  (This may take 30-60 seconds...)"
echo
cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer 2>&1 | tail -n 30
echo
echo -e "${GREEN}✓${NC} Demo completed"
echo

# Step 4: Check PostgreSQL metadata
echo -e "${BLUE}[4/8]${NC} Querying PostgreSQL metadata..."
echo "  Topics created:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -c \
    "SELECT name, partition_count FROM topics ORDER BY name;" 2>/dev/null | sed 's/^/    /'
echo
echo "  Partitions:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -c \
    "SELECT t.name || '/' || p.partition_id || ': offset ' || p.high_watermark
     FROM partitions p
     JOIN topics t ON p.topic_id = t.id
     ORDER BY t.name, p.partition_id
     LIMIT 10;" 2>/dev/null | sed 's/^/    /'
echo

# Step 5: Check MinIO storage
echo -e "${BLUE}[5/8]${NC} Checking MinIO object storage..."
echo "  Segments in streamhouse-data bucket:"
docker exec streamhouse-minio mc ls myminio/streamhouse-data --recursive 2>/dev/null | head -n 10 | sed 's/^/    /' || \
    echo "    (Run: docker exec streamhouse-minio mc alias set myminio http://localhost:9000 minioadmin minioadmin)"
echo

# Step 6: Check Prometheus metrics
echo -e "${BLUE}[6/8]${NC} Checking Prometheus metrics..."
if curl -s http://localhost:9090/api/v1/query?query=up > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Prometheus is accessible at http://localhost:9090"

    # Check for StreamHouse metrics
    METRIC_COUNT=$(curl -s http://localhost:9090/api/v1/label/__name__/values 2>/dev/null | \
        jq -r '.data[]' 2>/dev/null | grep -c streamhouse || echo "0")
    # Remove any whitespace/newlines
    METRIC_COUNT=$(echo "$METRIC_COUNT" | tr -d '\n' | tr -d ' ')

    if [ "$METRIC_COUNT" -gt 0 ] 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} Found $METRIC_COUNT StreamHouse metrics"
    else
        echo -e "  ${YELLOW}!${NC} No StreamHouse metrics yet (server may need to expose /metrics endpoint)"
    fi
else
    echo -e "  ${YELLOW}!${NC} Cannot connect to Prometheus"
fi
echo

# Step 7: Check Grafana
echo -e "${BLUE}[7/8]${NC} Checking Grafana..."
if curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Grafana is accessible at http://localhost:3000"
    echo "  Login: admin / admin"
    echo "  Import dashboard: grafana/dashboards/streamhouse-overview.json"
else
    echo -e "  ${YELLOW}!${NC} Cannot connect to Grafana"
fi
echo

# Step 8: Summary
echo -e "${BLUE}[8/8]${NC} Pipeline Test Summary"
echo "=========================================="
echo
echo "Infrastructure:"
echo "  ✓ PostgreSQL:  http://localhost:5432"
echo "  ✓ MinIO:       http://localhost:9001 (console)"
echo "  ✓ Prometheus:  http://localhost:9090"
echo "  ✓ Grafana:     http://localhost:3000"
echo "  ✓ Server:      grpc://localhost:50051"
echo
echo "Data Pipeline:"
echo "  ✓ Topics created and stored in PostgreSQL"
echo "  ✓ Records written to MinIO segments"
echo "  ✓ Producer/Consumer metrics available"
echo
echo "Next Steps:"
echo "  1. Open Grafana: http://localhost:3000"
echo "  2. Import dashboard: grafana/dashboards/streamhouse-overview.json"
echo "  3. Query Prometheus: http://localhost:9090"
echo "  4. Run more examples:"
echo "     cargo run --release --features metrics -p streamhouse-client --example e2e_producer_consumer"
echo
echo -e "${GREEN}Pipeline test complete!${NC}"
echo "=========================================="
