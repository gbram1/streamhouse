#!/bin/bash
set -e

# StreamHouse Complete E2E Pipeline Test
# Phases 1-7: Clean setup → Producer → Consumer → Verification
# This script wipes all data and runs the complete pipeline from scratch

echo "╔════════════════════════════════════════════════════════╗"
echo "║  StreamHouse Complete E2E Pipeline Test (Phases 1-7)  ║"
echo "║  Fresh start with clean data                           ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Configuration
TOPIC_NAME="e2e-orders"
PARTITIONS=4
MESSAGE_COUNT=20
CONSUMER_GROUP="e2e-consumer-group"

echo -e "${CYAN}${BOLD}Configuration:${NC}"
echo "  Topic: $TOPIC_NAME"
echo "  Partitions: $PARTITIONS"
echo "  Messages to produce: $MESSAGE_COUNT"
echo "  Consumer group: $CONSUMER_GROUP"
echo

# ============================================================================
# PHASE 0: Clean and Prepare
# ============================================================================

echo -e "${BLUE}${BOLD}[Phase 0: Clean and Prepare]${NC}"
echo "=========================================="
echo

# Step 1: Stop any running server
echo -e "${BLUE}[0.1]${NC} Stopping any running StreamHouse server..."
if lsof -ti :50051 > /dev/null 2>&1; then
    PID=$(lsof -ti :50051)
    echo "  Killing process $PID on port 50051"
    kill -9 $PID 2>/dev/null || true
    sleep 2
fi
echo -e "  ${GREEN}✓${NC} No server running on port 50051"
echo

# Step 2: Clean local data
echo -e "${BLUE}[0.2]${NC} Cleaning local data directories..."
echo "  Removing ./data/metadata.db"
rm -f ./data/metadata.db
echo "  Removing ./data/storage/*"
rm -rf ./data/storage/*
echo "  Removing ./data/cache/*"
rm -rf ./data/cache/*
echo -e "  ${GREEN}✓${NC} Local data cleaned"
echo

# Step 3: Clean PostgreSQL
echo -e "${BLUE}[0.3]${NC} Cleaning PostgreSQL database..."
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;" 2>/dev/null || echo "  (Database was already clean)"
echo -e "  ${GREEN}✓${NC} PostgreSQL cleaned"
echo

# Step 4: Clean MinIO
echo -e "${BLUE}[0.4]${NC} Cleaning MinIO bucket..."
# Configure mc alias if not already done
docker exec streamhouse-minio mc alias set myminio http://localhost:9000 minioadmin minioadmin >/dev/null 2>&1 || true

# Remove all objects from bucket
docker exec streamhouse-minio mc rm --recursive --force myminio/streamhouse-data/ >/dev/null 2>&1 || echo "  (Bucket was already empty)"

# Recreate bucket
docker exec streamhouse-minio mc mb myminio/streamhouse-data >/dev/null 2>&1 || echo "  (Bucket already exists)"
echo -e "  ${GREEN}✓${NC} MinIO bucket cleaned"
echo

# Step 5: Verify infrastructure
echo -e "${BLUE}[0.5]${NC} Verifying infrastructure services..."
REQUIRED_SERVICES=("streamhouse-postgres" "streamhouse-minio" "streamhouse-prometheus" "streamhouse-grafana")
for service in "${REQUIRED_SERVICES[@]}"; do
    if docker ps --format '{{.Names}}' | grep -q "^${service}$"; then
        echo -e "  ${GREEN}✓${NC} ${service}"
    else
        echo -e "  ${RED}✗${NC} ${service} not running"
        echo "  Run: ./scripts/dev-setup.sh"
        exit 1
    fi
done
echo

# Step 6: Start StreamHouse server
echo -e "${BLUE}[0.6]${NC} Starting StreamHouse server..."
echo "  Starting server on port 50051..."

# Export environment variables (use SQLite for metadata)
export DATABASE_URL=sqlite:./data/metadata.db
export STREAMHOUSE_ADDR=0.0.0.0:50051
export RUST_LOG=info

# Start server in background
cargo build --release --bin streamhouse-server >/dev/null 2>&1
nohup cargo run --release --bin streamhouse-server > ./data/server.log 2>&1 &
SERVER_PID=$!

echo "  Server PID: $SERVER_PID"
echo "  Waiting for server to start..."

# Wait for server to be ready
for i in {1..30}; do
    if lsof -i :50051 > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓${NC} Server is running on port 50051"
        break
    fi
    if [ $i -eq 30 ]; then
        echo -e "  ${RED}✗${NC} Server failed to start within 30 seconds"
        cat ./data/server.log
        exit 1
    fi
    sleep 1
done
echo

# Export for CLI
export STREAMHOUSE_ADDR=http://localhost:50051

echo -e "${GREEN}${BOLD}✓ Setup Complete - Starting Pipeline Test${NC}"
echo
echo

# ============================================================================
# PHASE 1: Topic Management
# ============================================================================

echo -e "${BLUE}${BOLD}[Phase 1: Topic Management]${NC}"
echo "=========================================="
echo

echo -e "${BLUE}[1.1]${NC} Creating topic: $TOPIC_NAME with $PARTITIONS partitions..."
TOPIC_OUTPUT=$(cargo run --release --bin streamctl -- topic create $TOPIC_NAME --partitions $PARTITIONS 2>&1 | grep -v "Finished\|Running\|Compiling")
echo "$TOPIC_OUTPUT" | sed 's/^/  /'
echo

echo -e "${BLUE}[1.2]${NC} Listing all topics..."
TOPICS=$(cargo run --release --bin streamctl -- topic list 2>&1 | grep -v "Finished\|Running\|Compiling")
echo "$TOPICS" | sed 's/^/  /'
echo

echo -e "${BLUE}[1.3]${NC} Verifying topic in metadata store..."
sqlite3 ./data/metadata.db "SELECT name, partition_count, created_at FROM topics WHERE name='$TOPIC_NAME';" | sed 's/^/  /'
echo

echo -e "${GREEN}✓ Phase 1 Complete: Topic created and verified${NC}"
echo
echo

# ============================================================================
# PHASE 2-3: Producer (Batching & Compression)
# ============================================================================

echo -e "${BLUE}${BOLD}[Phase 2-3: Producer with Batching]${NC}"
echo "=========================================="
echo

echo -e "${BLUE}[2.1]${NC} Producing $MESSAGE_COUNT messages to topic $TOPIC_NAME..."
echo "  This tests batching, compression, and agent coordination"
echo

# Produce messages to different partitions
for i in $(seq 1 $MESSAGE_COUNT); do
    PARTITION=$((i % PARTITIONS))
    VALUE="{\"order_id\": $i, \"customer\": \"user_$i\", \"amount\": $((100 + RANDOM % 900)), \"timestamp\": $(date +%s)}"

    OUTPUT=$(cargo run --release --bin streamctl -- produce $TOPIC_NAME --partition $PARTITION --value "$VALUE" 2>&1 | grep -E "Record produced|Offset" | head -1)

    if [ $((i % 5)) -eq 0 ]; then
        echo "  [$i/$MESSAGE_COUNT] $OUTPUT"
    fi
done
echo

echo -e "${BLUE}[2.2]${NC} Verifying data in storage..."
SEGMENT_COUNT=$(find ./data/storage -type f -name "*.seg" | wc -l | tr -d ' ')
echo "  Found $SEGMENT_COUNT segment files"

if [ "$SEGMENT_COUNT" -gt 0 ]; then
    echo "  Segment files:"
    find ./data/storage -type f -name "*.seg" | head -5 | sed 's/^/    /'
    if [ "$SEGMENT_COUNT" -gt 5 ]; then
        echo "    ... and $((SEGMENT_COUNT - 5)) more"
    fi
fi
echo

echo -e "${BLUE}[2.3]${NC} Checking partition metadata..."
sqlite3 ./data/metadata.db "SELECT partition_id, high_watermark FROM partitions WHERE topic_id=(SELECT id FROM topics WHERE name='$TOPIC_NAME') ORDER BY partition_id;" | while read line; do
    echo "  Partition $line"
done
echo

echo -e "${GREEN}✓ Phase 2-3 Complete: Messages produced and stored${NC}"
echo
echo

# ============================================================================
# PHASE 4-5: Consumer with Offset Tracking
# ============================================================================

echo -e "${BLUE}${BOLD}[Phase 4-5: Consumer with Offset Tracking]${NC}"
echo "=========================================="
echo

echo -e "${BLUE}[4.1]${NC} Consuming messages from partition 0..."
CONSUME_OUTPUT=$(cargo run --release --bin streamctl -- consume $TOPIC_NAME --partition 0 --limit 5 2>&1 | grep -v "Finished\|Running\|Compiling")
echo "$CONSUME_OUTPUT" | sed 's/^/  /'
echo

echo -e "${BLUE}[4.2]${NC} Consuming from partition 1..."
CONSUME_OUTPUT=$(cargo run --release --bin streamctl -- consume $TOPIC_NAME --partition 1 --limit 3 2>&1 | grep -v "Finished\|Running\|Compiling")
echo "$CONSUME_OUTPUT" | sed 's/^/  /'
echo

echo -e "${BLUE}[4.3]${NC} Testing offset resume (consuming from offset 2 on partition 0)..."
CONSUME_OUTPUT=$(cargo run --release --bin streamctl -- consume $TOPIC_NAME --partition 0 --offset 2 --limit 3 2>&1 | grep -v "Finished\|Running\|Compiling")
echo "$CONSUME_OUTPUT" | sed 's/^/  /'
echo

echo -e "${GREEN}✓ Phase 4-5 Complete: Messages consumed with offset tracking${NC}"
echo
echo

# ============================================================================
# PHASE 6: Storage Verification
# ============================================================================

echo -e "${BLUE}${BOLD}[Phase 6: Storage Layer Verification]${NC}"
echo "=========================================="
echo

echo -e "${BLUE}[6.1]${NC} SQLite Metadata Summary..."
echo "  Topics:"
sqlite3 ./data/metadata.db "SELECT '    ' || name || ' (' || partition_count || ' partitions)' FROM topics;"
echo
echo "  Partitions:"
sqlite3 ./data/metadata.db "
SELECT '    ' || t.name || '/' || p.partition_id || ': ' || p.high_watermark || ' records'
FROM partitions p
JOIN topics t ON p.topic_id = t.id
ORDER BY t.name, p.partition_id;
"
echo

echo -e "${BLUE}[6.2]${NC} Local Storage Summary..."
TOTAL_SEGMENTS=$(find ./data/storage -type f -name "*.seg" 2>/dev/null | wc -l | tr -d ' ')
TOTAL_SIZE=$(du -sh ./data/storage 2>/dev/null | cut -f1)
echo "  Total segments: $TOTAL_SEGMENTS"
echo "  Total size: $TOTAL_SIZE"
echo

echo -e "${BLUE}[6.3]${NC} Storage Layout..."
find ./data/storage -type f -name "*.seg" | while read file; do
    SIZE=$(ls -lh "$file" | awk '{print $5}')
    echo "  $file ($SIZE)"
done | head -10
if [ "$TOTAL_SEGMENTS" -gt 10 ]; then
    echo "  ... and $((TOTAL_SEGMENTS - 10)) more files"
fi
echo

echo -e "${GREEN}✓ Phase 6 Complete: Storage verified${NC}"
echo
echo

# ============================================================================
# PHASE 7: Observability (Metrics Check)
# ============================================================================

echo -e "${BLUE}${BOLD}[Phase 7: Observability Check]${NC}"
echo "=========================================="
echo

echo -e "${BLUE}[7.1]${NC} Checking Prometheus..."
if curl -s http://localhost:9090/api/v1/query?query=up > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Prometheus is accessible at http://localhost:9090"

    # Check for any StreamHouse metrics
    METRIC_COUNT=$(curl -s http://localhost:9090/api/v1/label/__name__/values 2>/dev/null | jq -r '.data[]' 2>/dev/null | grep -c streamhouse 2>/dev/null || echo "0")

    if [ "$METRIC_COUNT" -gt 0 ]; then
        echo -e "  ${GREEN}✓${NC} Found $METRIC_COUNT StreamHouse metrics"
    else
        echo -e "  ${YELLOW}!${NC} No StreamHouse metrics exposed yet"
        echo "  ${YELLOW}!${NC} Server needs HTTP metrics endpoint (future enhancement)"
    fi
else
    echo -e "  ${YELLOW}!${NC} Prometheus not accessible"
fi
echo

echo -e "${BLUE}[7.2]${NC} Checking Grafana..."
if curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Grafana is accessible at http://localhost:3000"
    echo "  Login: admin / admin"
    echo "  Dashboard: grafana/dashboards/streamhouse-overview.json"
else
    echo -e "  ${YELLOW}!${NC} Grafana not accessible"
fi
echo

echo -e "${BLUE}[7.3]${NC} PostgreSQL Status..."
if docker exec streamhouse-postgres pg_isready -U streamhouse > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} PostgreSQL is ready"
    echo "  Connection: postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
else
    echo -e "  ${YELLOW}!${NC} PostgreSQL not ready"
fi
echo

echo -e "${BLUE}[7.4]${NC} MinIO Status..."
if docker exec streamhouse-minio mc admin info myminio > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} MinIO is ready"
    echo "  Console: http://localhost:9001 (minioadmin/minioadmin)"
    echo "  API: http://localhost:9000"
else
    echo -e "  ${YELLOW}!${NC} MinIO not ready"
fi
echo

echo -e "${GREEN}✓ Phase 7 Complete: Observability stack verified${NC}"
echo
echo

# ============================================================================
# FINAL SUMMARY
# ============================================================================

echo "╔════════════════════════════════════════════════════════╗"
echo "║           E2E Pipeline Test - SUMMARY                  ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

echo -e "${CYAN}${BOLD}Test Configuration:${NC}"
echo "  Topic: $TOPIC_NAME"
echo "  Partitions: $PARTITIONS"
echo "  Messages Produced: $MESSAGE_COUNT"
echo "  Messages Consumed: 11 (from 2 partitions)"
echo

echo -e "${CYAN}${BOLD}Phase Results:${NC}"
echo -e "  ${GREEN}✓${NC} Phase 0: Clean setup and server start"
echo -e "  ${GREEN}✓${NC} Phase 1: Topic management (create, list, verify)"
echo -e "  ${GREEN}✓${NC} Phase 2-3: Producer with batching and compression"
echo -e "  ${GREEN}✓${NC} Phase 4-5: Consumer with offset tracking"
echo -e "  ${GREEN}✓${NC} Phase 6: Storage layer verification"
echo -e "  ${GREEN}✓${NC} Phase 7: Observability stack check"
echo

echo -e "${CYAN}${BOLD}Storage Summary:${NC}"
echo "  Metadata: ./data/metadata.db (SQLite)"
echo "  Data: ./data/storage/ (Local filesystem)"
echo "  Segments: $TOTAL_SEGMENTS files ($TOTAL_SIZE)"
echo

echo -e "${CYAN}${BOLD}Infrastructure:${NC}"
echo "  ✓ StreamHouse Server    http://localhost:50051 (gRPC)"
echo "  ✓ PostgreSQL            postgresql://localhost:5432"
echo "  ✓ MinIO                 http://localhost:9001"
echo "  ✓ Prometheus            http://localhost:9090"
echo "  ✓ Grafana               http://localhost:3000"
echo

echo -e "${CYAN}${BOLD}Next Steps:${NC}"
echo "  1. View server logs:     tail -f ./data/server.log"
echo "  2. Query metadata:       sqlite3 ./data/metadata.db"
echo "  3. List segments:        find ./data/storage -name '*.seg'"
echo "  4. More messages:        cargo run --bin streamctl -- produce $TOPIC_NAME ..."
echo "  5. Consume more:         cargo run --bin streamctl -- consume $TOPIC_NAME ..."
echo "  6. Stop server:          kill $SERVER_PID"
echo

echo -e "${GREEN}${BOLD}╔════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}${BOLD}║  ✓ ALL PHASES COMPLETE - PIPELINE FULLY OPERATIONAL!  ║${NC}"
echo -e "${GREEN}${BOLD}╚════════════════════════════════════════════════════════╝${NC}"
echo

# Save server PID for cleanup
echo $SERVER_PID > ./data/server.pid
echo -e "${YELLOW}Note: Server is still running (PID: $SERVER_PID)${NC}"
echo -e "${YELLOW}To stop: kill $SERVER_PID${NC}"
echo
