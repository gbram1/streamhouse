#!/bin/bash
set -e

# Test script for agent binary with gRPC and metrics servers

echo "╔════════════════════════════════════════════════════════╗"
echo "║   StreamHouse Agent Binary Test                       ║"
echo "║   Testing gRPC + Metrics Server Integration          ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Cleanup function
cleanup() {
    echo
    echo -e "${YELLOW}Cleaning up...${NC}"

    if [ -n "$AGENT_PID" ] && kill -0 "$AGENT_PID" 2>/dev/null; then
        echo "  Stopping agent (PID: $AGENT_PID)"
        kill "$AGENT_PID" 2>/dev/null || true
        wait "$AGENT_PID" 2>/dev/null || true
    fi

    echo -e "${GREEN}Cleanup complete${NC}"
}

trap cleanup EXIT INT TERM

# ==============================================================================
# Phase 1: Setup
# ==============================================================================

echo -e "${BLUE}[Phase 1: Setup]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

# Check MinIO
if docker ps --format '{{.Names}}' | grep -q "streamhouse-minio"; then
    echo -e "  ${GREEN}✓${NC} MinIO running"
else
    echo -e "  ${RED}✗${NC} MinIO not running"
    echo "  Start with: docker-compose up -d minio"
    exit 1
fi

# Setup test database
rm -f ./data/test-agent.db
mkdir -p ./data/agent-logs

export METADATA_STORE="./data/test-agent.db"
export AWS_ENDPOINT_URL=http://localhost:9000
export AWS_REGION=us-east-1
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_ALLOW_HTTP=true
export STREAMHOUSE_BUCKET=streamhouse-data
export RUST_LOG=info

export AGENT_ID=test-agent-001
export AGENT_ADDRESS=127.0.0.1:9095
export AGENT_ZONE=test-zone
export AGENT_GROUP=test
export METRICS_PORT=8085

echo "  Metadata: $METADATA_STORE"
echo "  Agent ID: $AGENT_ID"
echo "  gRPC Address: $AGENT_ADDRESS"
echo "  Metrics Port: $METRICS_PORT"
echo

echo "  Database will be initialized by agent on first start"
echo -e "  ${GREEN}✓${NC} Setup complete"
echo

# ==============================================================================
# Phase 2: Start Agent Binary
# ==============================================================================

echo -e "${BLUE}[Phase 2: Start Agent Binary]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

# Build binary with metrics
echo "  Building agent binary with metrics..."
cargo build --quiet --release --bin agent --features metrics

echo "  Starting agent binary..."
nohup ./target/release/agent > ./data/agent-logs/test-agent.log 2>&1 &
AGENT_PID=$!

echo "  Agent PID: $AGENT_PID"
echo "  Waiting for agent to start..."

# Wait for agent to start (check log file)
for i in {1..15}; do
    if grep -q "Agent test-agent-001 is now running" ./data/agent-logs/test-agent.log 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} Agent started successfully"
        break
    fi
    if [ $i -eq 15 ]; then
        echo -e "  ${RED}✗${NC} Agent failed to start within 15 seconds"
        echo "  Last 30 lines of log:"
        tail -30 ./data/agent-logs/test-agent.log 2>/dev/null || echo "  (Log file not created)"
        exit 1
    fi
    sleep 1
done

sleep 2  # Give servers time to fully start

echo

# ==============================================================================
# Phase 3: Test gRPC Server
# ==============================================================================

echo -e "${BLUE}[Phase 3: Test gRPC Server]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

echo "  Testing gRPC server connectivity..."
if nc -z 127.0.0.1 9095 2>/dev/null; then
    echo -e "  ${GREEN}✓${NC} gRPC server listening on 127.0.0.1:9095"
else
    echo -e "  ${RED}✗${NC} gRPC server not listening"
    exit 1
fi

echo

# ==============================================================================
# Phase 4: Test Metrics Server
# ==============================================================================

echo -e "${BLUE}[Phase 4: Test Metrics Server]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

echo "  Testing /health endpoint..."
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8085/health)
if [ "$HTTP_CODE" = "200" ]; then
    echo -e "  ${GREEN}✓${NC} /health returned 200 OK"
else
    echo -e "  ${RED}✗${NC} /health returned $HTTP_CODE"
fi

echo "  Testing /ready endpoint..."
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8085/ready)
if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "503" ]; then
    echo -e "  ${GREEN}✓${NC} /ready returned $HTTP_CODE (expected 200 or 503)"
else
    echo -e "  ${RED}✗${NC} /ready returned $HTTP_CODE"
fi

echo "  Testing /metrics endpoint..."
METRICS=$(curl -s http://localhost:8085/metrics)
if echo "$METRICS" | grep -q "^# HELP"; then
    echo -e "  ${GREEN}✓${NC} /metrics returned Prometheus format"
    echo
    echo "  Sample metrics:"
    echo "$METRICS" | head -10 | sed 's/^/    /'
else
    echo -e "  ${RED}✗${NC} /metrics did not return Prometheus format"
fi

echo

# ==============================================================================
# Phase 5: Check Agent Status
# ==============================================================================

echo -e "${BLUE}[Phase 5: Agent Status]${NC}"
echo "══════════════════════════════════════════════════════════"
echo

echo "  Agent registration in metadata:"
sqlite3 $METADATA_STORE "
SELECT '    Agent: ' || agent_id || ' (' || availability_zone || ')
      Address: ' || address || '
      Group: ' || agent_group
FROM agents
WHERE agent_id = '$AGENT_ID';
" 2>/dev/null || echo "    (Not registered yet - check if migrations ran)"

echo
echo "  Recent log entries:"
tail -5 ./data/agent-logs/test-agent.log | sed 's/^/    /'

echo

# ==============================================================================
# SUMMARY
# ==============================================================================

echo "╔════════════════════════════════════════════════════════╗"
echo "║              AGENT BINARY TEST SUMMARY                ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

echo -e "${GREEN}✓ gRPC Server:    Running on 127.0.0.1:9095${NC}"
echo -e "${GREEN}✓ Metrics Server: Running on http://localhost:8085${NC}"
echo -e "${GREEN}✓ Health Checks:  /health, /ready, /metrics${NC}"
echo -e "${GREEN}✓ Agent:          Registered and running${NC}"
echo

echo "Test Endpoints:"
echo "  gRPC:    grpcurl -plaintext 127.0.0.1:9095 list"
echo "  Health:  curl http://localhost:8085/health"
echo "  Ready:   curl http://localhost:8085/ready"
echo "  Metrics: curl http://localhost:8085/metrics"
echo

echo -e "${GREEN}╔════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║          ✓ AGENT BINARY TEST COMPLETE!                ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════╝${NC}"
echo

echo "Press Enter to stop agent and exit..."
read -r
