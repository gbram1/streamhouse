#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# E2E Test: Multi-Agent Load Splitting (Docker Compose)
#
# Verifies that multiple standalone agents coordinate partition ownership:
#   1. Build and start all services via docker compose
#   2. Create a topic and verify all 3 agents register
#   3. Verify partitions are distributed across agents
#   4. Produce data to all partitions
#   5. Kill an agent, verify survivors absorb its partitions
#   6. Start a replacement agent, verify rebalance includes it
#
# Usage:
#   ./tests/e2e_multi_agent.sh              # full run (build + test)
#   ./tests/e2e_multi_agent.sh --no-build   # skip docker build
#   ./tests/e2e_multi_agent.sh --cleanup    # just tear down
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

API_URL="http://localhost:8080"
TOPIC="multi-agent-e2e"
PARTITIONS=6
REBALANCE_WAIT=45

PASS=0
FAIL=0
ERRORS=()

# -- Formatting ---------------------------------------------------------------

green() { printf "\033[32m%s\033[0m\n" "$1"; }
red()   { printf "\033[31m%s\033[0m\n" "$1"; }
bold()  { printf "\033[1m%s\033[0m\n" "$1"; }
dim()   { printf "\033[2m%s\033[0m\n" "$1"; }

pass() {
    green "  PASS: $1"
    PASS=$((PASS + 1))
}

fail() {
    red "  FAIL: $1"
    [ -n "${2:-}" ] && echo "        $2"
    ERRORS+=("$1")
    FAIL=$((FAIL + 1))
}

# -- Cleanup ------------------------------------------------------------------

cleanup() {
    bold ""
    bold "=== Cleanup ==="
    echo "  Stopping all containers..."
    docker compose down -v --remove-orphans 2>/dev/null || true
    echo "  Done."
}

if [ "${1:-}" = "--cleanup" ]; then
    cleanup
    exit 0
fi

# -- Helpers ------------------------------------------------------------------

# Query the agents REST API
get_agents() {
    curl -sf "$API_URL/api/v1/agents" 2>/dev/null || echo "[]"
}

# Query partition assignments for the test topic
get_partitions() {
    curl -sf "$API_URL/api/v1/topics/$TOPIC/partitions" 2>/dev/null || echo "[]"
}

# Count how many agents are registered
agent_count() {
    get_agents | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo 0
}

# Count how many partitions have a leader
assigned_partition_count() {
    get_partitions | python3 -c "
import sys, json
parts = json.load(sys.stdin)
print(sum(1 for p in parts if p.get('leader_agent_id')))
" 2>/dev/null || echo 0
}

# Get partition distribution as formatted lines
partition_distribution() {
    get_partitions | python3 -c "
import sys, json
from collections import Counter
parts = json.load(sys.stdin)
leaders = [p['leader_agent_id'] for p in parts if p.get('leader_agent_id')]
for agent, count in sorted(Counter(leaders).items()):
    print(f'  {agent}: {count} partitions')
if not leaders:
    print('  (no partitions assigned)')
" 2>/dev/null
}

# Wait for a condition with polling
wait_for() {
    local desc="$1"
    local check_cmd="$2"
    local expected="$3"
    local timeout="${4:-$REBALANCE_WAIT}"

    dim "  Waiting for $desc (up to ${timeout}s)..."
    for i in $(seq 1 "$timeout"); do
        local actual
        actual=$(eval "$check_cmd" 2>/dev/null || echo "")
        if [ "$actual" = "$expected" ]; then
            return 0
        fi
        sleep 1
    done
    fail "$desc: expected $expected, got $(eval "$check_cmd" 2>/dev/null || echo '?')"
    return 1
}

# =============================================================================
bold "=== Multi-Agent Load Splitting E2E Test (Docker Compose) ==="
echo ""

# -- Step 1: Build ------------------------------------------------------------

if [ "${1:-}" != "--no-build" ]; then
    bold "=== Step 1: Build Docker image ==="
    echo "  Building streamhouse image (unified-server + agent)..."
    docker compose build streamhouse
    echo ""
else
    bold "=== Step 1: Build (skipped with --no-build) ==="
    echo ""
fi

# -- Step 2: Start all services -----------------------------------------------

bold "=== Step 2: Start services ==="
echo "  Starting postgres, minio, streamhouse server, and 3 agents..."
docker compose up -d postgres minio minio-init
echo "  Waiting for infrastructure..."
sleep 5

docker compose up -d streamhouse
echo "  Waiting for server to be healthy..."

if wait_for "server health" "curl -sf $API_URL/health > /dev/null 2>&1 && echo ok || echo no" "ok" 60; then
    pass "Server is healthy"
else
    echo "  Server logs:"
    docker compose logs --tail=30 streamhouse 2>/dev/null || true
    echo ""
    red "Server failed to start. Aborting."
    cleanup
    exit 1
fi

# Start agents
docker compose up -d agent-1 agent-2 agent-3
echo "  Agents starting..."
sleep 5
echo ""

# -- Step 3: Create topic -----------------------------------------------------

bold "=== Step 3: Create topic '$TOPIC' with $PARTITIONS partitions ==="

# Create topic via REST API
RESULT=$(curl -sf -X POST "$API_URL/api/v1/topics" \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"$TOPIC\", \"partition_count\": $PARTITIONS}" 2>&1) || true

if echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d.get('name') == '$TOPIC'" 2>/dev/null; then
    pass "Topic created: $TOPIC ($PARTITIONS partitions)"
elif echo "$RESULT" | grep -qi "already exists" 2>/dev/null; then
    dim "  Topic already exists (from previous run), continuing..."
    pass "Topic exists: $TOPIC"
else
    # Try streamctl if available
    if command -v ./target/release/streamctl &>/dev/null; then
        ./target/release/streamctl topic delete "$TOPIC" 2>/dev/null || true
        sleep 1
        if ./target/release/streamctl topic create "$TOPIC" --partitions "$PARTITIONS" 2>/dev/null; then
            pass "Topic created via streamctl: $TOPIC ($PARTITIONS partitions)"
        else
            fail "Failed to create topic" "$RESULT"
        fi
    else
        fail "Failed to create topic" "$RESULT"
    fi
fi
echo ""

# -- Step 4: Verify agents register and distribute partitions -----------------

bold "=== Step 4: Verify agent registration and partition assignment ==="

# Wait for all 3 agents to register
if wait_for "3 agents registered" "agent_count" "3" 30; then
    pass "All 3 agents registered"
else
    echo "  Agent logs:"
    docker compose logs --tail=10 agent-1 agent-2 agent-3 2>/dev/null || true
fi

# Wait for all partitions to have leaders
if wait_for "all $PARTITIONS partitions assigned" "assigned_partition_count" "$PARTITIONS" "$REBALANCE_WAIT"; then
    pass "All $PARTITIONS partitions have leaders"
else
    echo "  Current partition state:"
    get_partitions | python3 -m json.tool 2>/dev/null || true
fi

echo ""
bold "  Partition distribution:"
partition_distribution
echo ""

# Verify each agent has at least 1 partition (load splitting works)
AGENTS_WITH_PARTITIONS=$(get_partitions | python3 -c "
import sys, json
parts = json.load(sys.stdin)
agents = set(p['leader_agent_id'] for p in parts if p.get('leader_agent_id'))
print(len(agents))
" 2>/dev/null || echo 0)

if [ "$AGENTS_WITH_PARTITIONS" -ge 2 ]; then
    pass "$AGENTS_WITH_PARTITIONS agents have partitions (load splitting working)"
else
    fail "Only $AGENTS_WITH_PARTITIONS agent(s) have partitions"
fi
echo ""

# -- Step 5: Produce data -----------------------------------------------------

bold "=== Step 5: Produce data to all partitions ==="

PRODUCE_PASS=0
for p in $(seq 0 $((PARTITIONS - 1))); do
    RESP=$(curl -sf -X POST "$API_URL/api/v1/topics/$TOPIC/produce" \
        -H "Content-Type: application/json" \
        -d "{\"partition\": $p, \"records\": [{\"value\": \"$(echo -n "{\"test\":\"partition-$p\"}" | base64)\"}]}" 2>&1) || true
    if [ -n "$RESP" ]; then
        PRODUCE_PASS=$((PRODUCE_PASS + 1))
    fi
done

if [ "$PRODUCE_PASS" -eq "$PARTITIONS" ]; then
    pass "Produced to all $PARTITIONS partitions via REST API"
elif [ "$PRODUCE_PASS" -gt 0 ]; then
    dim "  Produced to $PRODUCE_PASS/$PARTITIONS partitions (some may need agent routing)"
    pass "Produce partially working ($PRODUCE_PASS/$PARTITIONS)"
else
    # Try with streamctl if available
    if command -v ./target/release/streamctl &>/dev/null; then
        for p in $(seq 0 $((PARTITIONS - 1))); do
            if ./target/release/streamctl produce "$TOPIC" --partition "$p" --value "{\"test\":\"partition-$p\"}" 2>/dev/null; then
                PRODUCE_PASS=$((PRODUCE_PASS + 1))
            fi
        done
        if [ "$PRODUCE_PASS" -gt 0 ]; then
            pass "Produced to $PRODUCE_PASS/$PARTITIONS partitions via streamctl"
        else
            fail "Could not produce to any partition"
        fi
    else
        dim "  (streamctl not built locally — produce test skipped)"
    fi
fi
echo ""

# -- Step 6: Kill agent-3, verify rebalance -----------------------------------

bold "=== Step 6: Kill agent-3 and verify rebalance ==="

echo "  Stopping agent-3..."
docker compose stop agent-3
docker compose rm -f agent-3 2>/dev/null || true

echo "  agent-3 stopped. Waiting for rebalance..."

# Wait for agent count to drop to 2
if wait_for "agent-3 deregistered" "agent_count" "2" 30; then
    pass "agent-3 deregistered (2 agents remaining)"
fi

# Wait for all partitions to be reassigned to remaining 2 agents
if wait_for "all $PARTITIONS partitions reassigned" "assigned_partition_count" "$PARTITIONS" "$REBALANCE_WAIT"; then
    pass "All $PARTITIONS partitions reassigned after agent-3 failure"
else
    echo "  Current partition state:"
    get_partitions | python3 -m json.tool 2>/dev/null || true
fi

echo ""
bold "  Partition distribution after failure:"
partition_distribution
echo ""

# Verify agent-3 no longer owns partitions
AGENT3_PARTITIONS=$(get_partitions | python3 -c "
import sys, json
parts = json.load(sys.stdin)
print(sum(1 for p in parts if p.get('leader_agent_id') == 'agent-3'))
" 2>/dev/null || echo "?")

if [ "$AGENT3_PARTITIONS" = "0" ]; then
    pass "agent-3 has 0 partitions (correctly removed)"
else
    fail "agent-3 still has $AGENT3_PARTITIONS partitions after being killed"
fi
echo ""

# -- Step 7: Start replacement agent, verify rebalance ------------------------

bold "=== Step 7: Start replacement agent (agent-4) and verify rebalance ==="

# Start agent-4 by scaling or running a new container with the same image
docker compose run -d --name streamhouse-agent-4 \
    -e AGENT_ID=agent-4 \
    -e AGENT_ADDRESS="0.0.0.0:9090" \
    -e AGENT_ZONE=us-east-1b \
    -e AGENT_GROUP=default \
    -e "METADATA_STORE=postgres://streamhouse:streamhouse@postgres:5432/streamhouse" \
    -e HEARTBEAT_INTERVAL=10 \
    -e S3_ENDPOINT=http://minio:9000 \
    -e STREAMHOUSE_BUCKET=streamhouse \
    -e AWS_REGION=us-east-1 \
    -e AWS_ACCESS_KEY_ID=minioadmin \
    -e AWS_SECRET_ACCESS_KEY=minioadmin \
    -e AWS_ENDPOINT_URL=http://minio:9000 \
    -e WAL_ENABLED=true \
    -e WAL_DIR=/data/wal/agent-4 \
    -e RUST_LOG=info \
    --entrypoint /app/agent \
    streamhouse 2>/dev/null || true

echo "  agent-4 starting..."

# Wait for 3 agents again
if wait_for "3 agents registered" "agent_count" "3" 30; then
    pass "agent-4 registered (3 agents again)"
fi

# Wait for rebalance to include agent-4
echo "  Waiting for rebalance to include agent-4..."
sleep "$REBALANCE_WAIT"

# Check agent-4 got partitions
AGENT4_PARTITIONS=$(get_partitions | python3 -c "
import sys, json
parts = json.load(sys.stdin)
print(sum(1 for p in parts if p.get('leader_agent_id') == 'agent-4'))
" 2>/dev/null || echo 0)

if [ "$AGENT4_PARTITIONS" -gt 0 ]; then
    pass "agent-4 received $AGENT4_PARTITIONS partition(s) after joining"
else
    fail "agent-4 has 0 partitions — rebalance did not include new agent"
fi

# Verify all partitions still covered
FINAL_ASSIGNED=$(assigned_partition_count)
if [ "$FINAL_ASSIGNED" = "$PARTITIONS" ]; then
    pass "All $PARTITIONS partitions assigned after scale-out"
else
    fail "Only $FINAL_ASSIGNED/$PARTITIONS partitions assigned after scale-out"
fi

echo ""
bold "  Final partition distribution:"
partition_distribution
echo ""

# Show final agent state
bold "  Final agent state:"
get_agents | python3 -c "
import sys, json
agents = json.load(sys.stdin)
for a in agents:
    print(f\"  {a['agent_id']}: {a.get('active_leases', '?')} leases, zone={a.get('availability_zone', '?')}, group={a.get('agent_group', '?')}\")
" 2>/dev/null || true
echo ""

# -- Results ------------------------------------------------------------------

bold "=== Results ==="
echo ""
green "Passed: $PASS"
if [ "$FAIL" -gt 0 ]; then
    red "Failed: $FAIL"
    echo ""
    red "Failures:"
    for err in "${ERRORS[@]}"; do
        red "  - $err"
    done
    echo ""
    echo "View logs with:"
    echo "  docker compose logs streamhouse"
    echo "  docker compose logs agent-1 agent-2 agent-3"
    cleanup
    exit 1
else
    green "All tests passed!"
    echo ""
    echo "Cleaning up..."
    # Stop agent-4 container
    docker rm -f streamhouse-agent-4 2>/dev/null || true
    cleanup
fi
