#!/bin/bash
set -e

# Test StreamHouse Server Connection
# This script verifies the server is running and accessible

echo "=========================================="
echo "StreamHouse Server Connection Test"
echo "=========================================="
echo

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Step 1: Check if server is running
echo -e "${BLUE}[1/5]${NC} Checking if server is running..."
if lsof -i :50051 > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Server is listening on port 50051"
    PID=$(lsof -ti :50051)
    echo "  Process ID: $PID"
else
    echo -e "  ${RED}✗${NC} No server found on port 50051"
    echo "  Start the server with:"
    echo "    source .env.dev && cargo run --release --features metrics --bin streamhouse-server"
    exit 1
fi
echo

# Step 2: Check metrics endpoint
echo -e "${BLUE}[2/5]${NC} Checking metrics endpoint..."
if curl -sf http://localhost:8080/metrics > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Metrics endpoint is accessible at http://localhost:8080/metrics"

    # Count metrics
    METRIC_LINES=$(curl -s http://localhost:8080/metrics 2>/dev/null | grep -c "^streamhouse_" || echo "0")
    echo "  Found $METRIC_LINES StreamHouse metric lines"
else
    echo -e "  ${YELLOW}!${NC} Metrics endpoint not accessible"
    echo "  Server may not be compiled with --features metrics"
fi
echo

# Step 3: Check health endpoint
echo -e "${BLUE}[3/5]${NC} Checking health endpoint..."
if curl -sf http://localhost:8080/health > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Health endpoint responding"
else
    echo -e "  ${YELLOW}!${NC} Health endpoint not accessible"
fi
echo

# Step 4: Check readiness endpoint
echo -e "${BLUE}[4/5]${NC} Checking readiness endpoint..."
if curl -sf http://localhost:8080/ready > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Server is ready (has active leases)"
else
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/ready 2>/dev/null || echo "000")
    if [ "$HTTP_CODE" = "503" ]; then
        echo -e "  ${YELLOW}!${NC} Server is not ready (no active leases yet)"
    else
        echo -e "  ${YELLOW}!${NC} Readiness endpoint not accessible"
    fi
fi
echo

# Step 5: Check PostgreSQL connection
echo -e "${BLUE}[5/5]${NC} Checking PostgreSQL metadata..."
if docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c '\dt' > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} PostgreSQL is accessible"

    # Check if tables exist
    TABLE_COUNT=$(docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';" 2>/dev/null | tr -d ' \n')

    if [ "$TABLE_COUNT" -gt 0 ] 2>/dev/null; then
        echo "  Found $TABLE_COUNT tables in database"
        echo
        echo "  Tables:"
        docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c '\dt' 2>/dev/null | grep -E "public|---" | sed 's/^/    /'
    else
        echo -e "  ${YELLOW}!${NC} No tables found (database not initialized yet)"
        echo "  Tables will be created when first topic is created"
    fi
else
    echo -e "  ${RED}✗${NC} Cannot connect to PostgreSQL"
fi
echo

# Summary
echo "=========================================="
echo -e "${GREEN}Server Status Summary${NC}"
echo "=========================================="
echo
echo "gRPC Server:     ✓ Running on port 50051"
echo "Metrics:         http://localhost:8080/metrics"
echo "Health:          http://localhost:8080/health"
echo "Readiness:       http://localhost:8080/ready"
echo "PostgreSQL:      postgresql://localhost:5432/streamhouse_metadata"
echo
echo "Next Steps:"
echo "  1. Use streamctl to create topics and produce/consume messages"
echo "  2. Check metrics in Prometheus: http://localhost:9090"
echo "  3. Import Grafana dashboard: http://localhost:3000"
echo
echo -e "${GREEN}Server is ready to accept connections!${NC}"
echo "=========================================="
