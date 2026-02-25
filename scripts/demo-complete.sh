#!/bin/bash
set -e

# StreamHouse Complete Demo - All Steps in One Script
# This script starts the server, creates topics, produces/consumes messages, and shows all data

echo "╔════════════════════════════════════════════════════════╗"
echo "║     StreamHouse Complete Demo - All Steps             ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Trap to ensure cleanup on exit
cleanup() {
    if [ -n "$SERVER_PID" ] && kill -0 $SERVER_PID 2>/dev/null; then
        echo
        echo -e "${YELLOW}Stopping server (PID: $SERVER_PID)...${NC}"
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

# ==============================================================================
# STEP 1: Check/Start Server
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 1: Server Setup]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

# Always start fresh server for demo (stop existing if running)
if lsof -i :50051 > /dev/null 2>&1; then
    EXISTING_PID=$(lsof -ti :50051)
    echo "Stopping existing server (PID: $EXISTING_PID)..."
    kill $EXISTING_PID 2>/dev/null || true
    sleep 2
fi

# Start new server
SERVER_WAS_RUNNING=false

echo "Starting StreamHouse server..."

    # Load environment (SQLite for metadata, MinIO for object storage)
    unset DATABASE_URL  # Don't set this to avoid sqlx compile-time issues
    unset USE_LOCAL_STORAGE  # Use S3/MinIO instead of local filesystem
    export AWS_ENDPOINT_URL=http://localhost:9000  # object_store crate looks for this
    export S3_ENDPOINT=http://localhost:9000       # Also set this for WriteConfig
    export STREAMHOUSE_BUCKET=streamhouse-data
    export AWS_REGION=us-east-1
    export AWS_ACCESS_KEY_ID=minioadmin
    export AWS_SECRET_ACCESS_KEY=minioadmin
    export AWS_ALLOW_HTTP=true                     # Allow HTTP (not HTTPS) for MinIO
    export STREAMHOUSE_ADDR=0.0.0.0:50051
    export RUST_LOG=info

    # Build and start server
    echo "  Building and starting server (this may take 1-2 minutes)..."
    mkdir -p ./data
    nohup cargo run --release --bin unified-server > ./data/demo-server.log 2>&1 &
    SERVER_PID=$!

    echo "  Server PID: $SERVER_PID"
    echo "  Waiting for server to start (build + startup may take 2 minutes)..."

    # Wait for server (max 120 seconds for build + startup)
    for i in {1..120}; do
        if lsof -i :50051 > /dev/null 2>&1; then
            echo -e "  ${GREEN}✓${NC} Server started on port 50051"
            break
        fi
        if [ $i -eq 120 ]; then
            echo -e "  ${RED}✗${NC} Server failed to start within 2 minutes"
            echo "Last 50 lines of server log:"
            tail -50 ./data/demo-server.log
            exit 1
        fi
        # Show progress every 15 seconds
        if [ $((i % 15)) -eq 0 ]; then
            echo "  Still waiting... ($i seconds elapsed)"
        fi
        sleep 1
    done

echo
export STREAMHOUSE_ADDR=http://localhost:50051

# ==============================================================================
# STEP 2: Create Topic
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 2: Create Topic]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

TOPIC="my-orders"
echo "Creating topic: $TOPIC (4 partitions)"
echo

CREATE_OUTPUT=$(cargo run --release --bin streamctl -- topic create $TOPIC --partitions 4 2>&1 | grep -v "Compiling\|Finished\|Running" || true)

if echo "$CREATE_OUTPUT" | grep -q "already exists"; then
    echo -e "${YELLOW}⚠${NC} Topic already exists, using existing topic"
else
    echo "$CREATE_OUTPUT" | sed 's/^/  /'
    echo -e "${GREEN}✓${NC} Topic created"
fi

echo

# ==============================================================================
# STEP 3: List Topics
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 3: List All Topics]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

cargo run --release --bin streamctl -- topic list 2>&1 | grep -v "Compiling\|Finished\|Running" | sed 's/^/  /'

echo

# ==============================================================================
# STEP 4: Create Additional Topics
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 4: Create Additional Topics]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

echo "Creating additional topics for full pipeline demo..."
echo

# Create user-events topic (2 partitions)
USER_EVENTS="user-events"
CREATE_OUTPUT=$(cargo run --release --bin streamctl -- topic create $USER_EVENTS --partitions 2 2>&1 | grep -v "Compiling\|Finished\|Running" || true)
if echo "$CREATE_OUTPUT" | grep -q "already exists"; then
    echo -e "  ${YELLOW}⚠${NC} $USER_EVENTS already exists"
else
    echo -e "  ${GREEN}✓${NC} Created $USER_EVENTS (2 partitions)"
fi

# Create metrics topic (4 partitions)
METRICS="metrics"
CREATE_OUTPUT=$(cargo run --release --bin streamctl -- topic create $METRICS --partitions 4 2>&1 | grep -v "Compiling\|Finished\|Running" || true)
if echo "$CREATE_OUTPUT" | grep -q "already exists"; then
    echo -e "  ${YELLOW}⚠${NC} $METRICS already exists"
else
    echo -e "  ${GREEN}✓${NC} Created $METRICS (4 partitions)"
fi

echo

# ==============================================================================
# STEP 5: Produce Messages to All Topics
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 5: Produce Messages to All Topics]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

echo "Producing to 'my-orders' (20 messages across 4 partitions)..."
for i in {1..20}; do
    PARTITION=$((i % 4))
    VALUE="{\"order_id\": $i, \"customer\": \"user_$i\", \"amount\": $((100 + RANDOM % 900))}"

    cargo run --release --bin streamctl -- produce $TOPIC --partition $PARTITION --value "$VALUE" 2>&1 > /dev/null

    if [ $((i % 10)) -eq 0 ]; then
        echo -e "  ${GREEN}✓${NC} [$i/20] messages sent"
    fi
done
echo -e "  ${GREEN}✓${NC} All 20 messages produced to $TOPIC"
echo

echo "Producing to 'user-events' (30 messages across 2 partitions)..."
for i in {1..30}; do
    PARTITION=$((i % 2))
    EVENT_TYPE=$( [ $((i % 3)) -eq 0 ] && echo "click" || [ $((i % 3)) -eq 1 ] && echo "view" || echo "purchase" )
    VALUE="{\"event_type\": \"$EVENT_TYPE\", \"user_id\": $((i % 50)), \"timestamp\": $(date +%s)000}"

    cargo run --release --bin streamctl -- produce $USER_EVENTS --partition $PARTITION --value "$VALUE" 2>&1 > /dev/null

    if [ $((i % 15)) -eq 0 ]; then
        echo -e "  ${GREEN}✓${NC} [$i/30] messages sent"
    fi
done
echo -e "  ${GREEN}✓${NC} All 30 messages produced to $USER_EVENTS"
echo

echo "Producing to 'metrics' (40 messages across 4 partitions)..."
for i in {1..40}; do
    PARTITION=$((i % 4))
    VALUE="{\"metric\": \"cpu_usage\", \"host\": \"host-$PARTITION\", \"value\": $((20 + RANDOM % 80)), \"timestamp\": $(date +%s)000}"

    cargo run --release --bin streamctl -- produce $METRICS --partition $PARTITION --value "$VALUE" 2>&1 > /dev/null

    if [ $((i % 20)) -eq 0 ]; then
        echo -e "  ${GREEN}✓${NC} [$i/40] messages sent"
    fi
done
echo -e "  ${GREEN}✓${NC} All 40 messages produced to $METRICS"
echo

# ==============================================================================
# STEP 6: Consume Messages from All Topics
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 6: Consume Messages from All Topics]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

echo "Consuming from 'my-orders' partition 0 (first 5 messages):"
echo
cargo run --release --bin streamctl -- consume $TOPIC --partition 0 --limit 5 2>&1 | grep -v "Compiling\|Finished\|Running" | sed 's/^/  /'
echo

echo "Consuming from 'user-events' partition 0 (first 5 messages):"
echo
cargo run --release --bin streamctl -- consume $USER_EVENTS --partition 0 --limit 5 2>&1 | grep -v "Compiling\|Finished\|Running" | sed 's/^/  /'
echo

echo "Consuming from 'metrics' partition 0 (first 5 messages):"
echo
cargo run --release --bin streamctl -- consume $METRICS --partition 0 --limit 5 2>&1 | grep -v "Compiling\|Finished\|Running" | sed 's/^/  /'
echo

# ==============================================================================
# STEP 7: Verify Data in Storage
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 7: Verify Data in Storage]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

echo "📊 SQLite Metadata:"
echo
echo "  All Topics:"
sqlite3 ./data/metadata.db "SELECT '    ' || name || ' (' || partition_count || ' partitions)' FROM topics;" 2>/dev/null || echo "    (No topics in database yet)"
echo

echo "  Partition Details:"
sqlite3 ./data/metadata.db "
SELECT '    ' || t.name || ' / partition-' || p.partition_id || ': ' || p.high_watermark || ' messages'
FROM partitions p
JOIN topics t ON p.topic_id = t.id
ORDER BY t.name, p.partition_id;
" 2>/dev/null || echo "    (No partition data yet)"
echo

echo "💾 Data Segments (Local Filesystem):"
echo
SEGMENT_FILES=$(find ./data/storage -name "*.seg" 2>/dev/null | grep "$TOPIC" || true)
if [ -n "$SEGMENT_FILES" ]; then
    echo "$SEGMENT_FILES" | while read file; do
        SIZE=$(ls -lh "$file" | awk '{print $5}')
        echo "  $file ($SIZE)"
    done
else
    echo "  (No segment files found - may still be in cache)"
fi
echo

TOTAL_SEGMENTS=$(find ./data/storage -name "*.seg" 2>/dev/null | wc -l | tr -d ' ')
TOTAL_SIZE=$(du -sh ./data/storage 2>/dev/null | cut -f1 || echo "0B")
echo "  Total segments: $TOTAL_SEGMENTS files"
echo "  Total storage: $TOTAL_SIZE"
echo

# ==============================================================================
# STEP 7: View Data in Different Layers
# ==============================================================================

echo -e "${BLUE}${BOLD}[Step 7: Data in Different Layers]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

# PostgreSQL
echo "🐘 PostgreSQL:"
if docker ps --format '{{.Names}}' | grep -q "streamhouse-postgres"; then
    POSTGRES_TABLES=$(docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public';" 2>/dev/null | tr -d ' \n' || echo "0")

    if [ "$POSTGRES_TABLES" -gt 0 ]; then
        echo -e "  ${GREEN}✓${NC} PostgreSQL is running and has $POSTGRES_TABLES tables"
        echo "  Connection: postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
        echo "  View tables: docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c '\\dt'"
    else
        echo -e "  ${YELLOW}!${NC} PostgreSQL is running but empty (using SQLite for metadata)"
        echo "  To use PostgreSQL: Configure server with DATABASE_URL"
    fi
else
    echo -e "  ${YELLOW}!${NC} PostgreSQL not running"
fi
echo

# MinIO
echo "🗄️  MinIO:"
if docker ps --format '{{.Names}}' | grep -q "streamhouse-minio"; then
    MINIO_OBJECTS=$(docker exec streamhouse-minio mc ls myminio/streamhouse-data --recursive 2>/dev/null | wc -l | tr -d ' ' || echo "0")

    if [ "$MINIO_OBJECTS" -gt 0 ]; then
        echo -e "  ${GREEN}✓${NC} MinIO has $MINIO_OBJECTS objects"
        echo "  Console: http://localhost:9001 (minioadmin/minioadmin)"
    else
        echo -e "  ${YELLOW}!${NC} MinIO is running but empty (using local filesystem)"
        echo "  Console: http://localhost:9001 (minioadmin/minioadmin)"
        echo "  To use MinIO: Configure server with S3_ENDPOINT"
    fi
else
    echo -e "  ${YELLOW}!${NC} MinIO not running"
fi
echo

# Prometheus
echo "📊 Prometheus:"
if curl -s http://localhost:9090/api/v1/query?query=up > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Prometheus is accessible"
    echo "  URL: http://localhost:9090"
    echo -e "  ${YELLOW}!${NC} No StreamHouse metrics yet (server needs HTTP metrics endpoint)"
else
    echo -e "  ${YELLOW}!${NC} Prometheus not accessible"
fi
echo

# Grafana
echo "📈 Grafana:"
if curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Grafana is accessible"
    echo "  URL: http://localhost:3000 (admin/admin)"
    echo "  Dashboard: grafana/dashboards/streamhouse-overview.json"
else
    echo -e "  ${YELLOW}!${NC} Grafana not accessible"
fi
echo

# ==============================================================================
# SUMMARY
# ==============================================================================

echo "╔════════════════════════════════════════════════════════╗"
echo "║                   DEMO SUMMARY                         ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

echo -e "${CYAN}${BOLD}What We Did:${NC}"
echo "  1. ✓ Started StreamHouse server on port 50051"
echo "  2. ✓ Created 3 topics (my-orders, user-events, metrics)"
echo "  3. ✓ Produced 90 total messages:"
echo "      - my-orders: 20 messages (4 partitions)"
echo "      - user-events: 30 messages (2 partitions)"
echo "      - metrics: 40 messages (4 partitions)"
echo "  4. ✓ Consumed messages from all topics"
echo "  5. ✓ Verified data in storage layers"
echo

echo -e "${CYAN}${BOLD}Data Storage:${NC}"
echo "  Metadata:  ./data/metadata.db (SQLite)"
echo "  Segments:  ./data/storage/ ($TOTAL_SIZE, $TOTAL_SEGMENTS files)"
echo "  Logs:      ./data/demo-server.log"
echo

echo -e "${CYAN}${BOLD}Quick Commands:${NC}"
echo "  # Produce more messages"
echo "  export STREAMHOUSE_ADDR=http://localhost:50051"
echo "  cargo run --bin streamctl -- produce $TOPIC --partition 0 --value '{\"test\": \"data\"}'"
echo
echo "  # Consume messages"
echo "  cargo run --bin streamctl -- consume $TOPIC --partition 0 --limit 10"
echo
echo "  # List topics"
echo "  cargo run --bin streamctl -- topic list"
echo
echo "  # Query metadata"
echo "  sqlite3 ./data/metadata.db 'SELECT * FROM topics;'"
echo
echo "  # View segments"
echo "  find ./data/storage -name '*.seg'"
echo

echo -e "${CYAN}${BOLD}Web Interfaces:${NC}"
echo "  PostgreSQL: docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata"
echo "  MinIO:      http://localhost:9001 (minioadmin/minioadmin)"
echo "  Prometheus: http://localhost:9090"
echo "  Grafana:    http://localhost:3000 (admin/admin)"
echo

if [ "$SERVER_WAS_RUNNING" = false ]; then
    echo -e "${YELLOW}${BOLD}Server Control:${NC}"
    echo "  Server PID: $SERVER_PID"
    echo "  Stop server: kill $SERVER_PID"
    echo "  View logs: tail -f ./data/demo-server.log"
    echo
    echo -e "${YELLOW}Note: Server will stop when you exit this script${NC}"
    echo -e "${YELLOW}To keep it running: Press Ctrl+Z, then run 'bg' and 'disown'${NC}"
else
    echo -e "${GREEN}${BOLD}Server Status:${NC}"
    echo "  Server was already running (not started by this script)"
    echo "  Server will continue running after script exits"
fi

echo
echo -e "${GREEN}${BOLD}╔════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}${BOLD}║              ✓ DEMO COMPLETE!                          ║${NC}"
echo -e "${GREEN}${BOLD}╚════════════════════════════════════════════════════════╝${NC}"
echo

# If we started the server, wait for user input before cleanup
if [ "$SERVER_WAS_RUNNING" = false ]; then
    echo -e "${CYAN}Press Enter to stop the server and exit...${NC}"
    read -r
fi
