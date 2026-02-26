#!/usr/bin/env bash
set -euo pipefail

# StreamHouse Quick Start
# Builds, starts the server, creates a topic, produces messages, and consumes them.

API="http://localhost:8080"
TOPIC="demo"
PARTITIONS=4
BINARY="./target/release/unified-server"
PID_FILE="/tmp/streamhouse-quickstart.pid"

RED='\033[0;31m'
GREEN='\033[0;32m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

cleanup() {
    if [ -f "$PID_FILE" ]; then
        local pid
        pid=$(cat "$PID_FILE")
        if kill -0 "$pid" 2>/dev/null; then
            echo ""
            echo -e "${DIM}Stopping server (pid $pid)...${NC}"
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi
}

trap cleanup EXIT

# ── Build ────────────────────────────────────────────────────────────────────

if [ ! -f "$BINARY" ]; then
    echo -e "${BOLD}Building StreamHouse...${NC}"
    cargo build --release -p streamhouse-server --bin unified-server
    echo ""
fi

# ── Start server ─────────────────────────────────────────────────────────────

if curl -sf "$API/health" > /dev/null 2>&1; then
    echo -e "${GREEN}Server already running${NC}"
    STARTED_HERE=false
else
    echo -e "Starting server ${DIM}(local storage, no cloud services needed)${NC}"
    USE_LOCAL_STORAGE=1 RUST_LOG=info "$BINARY" > /tmp/streamhouse-quickstart.log 2>&1 &
    echo $! > "$PID_FILE"
    STARTED_HERE=true

    # Wait for health
    for i in $(seq 1 30); do
        if curl -sf "$API/health" > /dev/null 2>&1; then
            break
        fi
        if [ "$i" -eq 30 ]; then
            echo -e "${RED}Server failed to start. Logs:${NC}"
            tail -20 /tmp/streamhouse-quickstart.log
            exit 1
        fi
        sleep 0.5
    done
    echo -e "${GREEN}Server ready${NC}  ${DIM}REST=$API  gRPC=localhost:50051  Kafka=localhost:9092${NC}"
fi

echo ""

# ── Create topic ─────────────────────────────────────────────────────────────

echo -e "${BOLD}Create topic${NC}  ${DIM}POST /api/v1/topics${NC}"

RESULT=$(curl -s -w "\n%{http_code}" -X POST "$API/api/v1/topics" \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"$TOPIC\", \"partitions\": $PARTITIONS}")

HTTP_CODE=$(echo "$RESULT" | tail -1)
BODY=$(echo "$RESULT" | sed '$d')

if [ "$HTTP_CODE" = "201" ]; then
    echo -e "  ${GREEN}Created${NC} '$TOPIC' with $PARTITIONS partitions"
elif [ "$HTTP_CODE" = "409" ]; then
    echo -e "  ${DIM}Topic '$TOPIC' already exists${NC}"
else
    echo -e "  ${RED}Failed ($HTTP_CODE):${NC} $BODY"
    echo -e "  ${DIM}Server logs:${NC}"
    tail -5 /tmp/streamhouse-quickstart.log 2>/dev/null || true
    exit 1
fi

echo ""

# ── Produce messages ─────────────────────────────────────────────────────────

echo -e "${BOLD}Produce messages${NC}  ${DIM}POST /api/v1/produce${NC}"

MESSAGES=(
    '{"event":"signup","user":"alice","plan":"pro"}'
    '{"event":"purchase","user":"bob","item":"widget","amount":29.99}'
    '{"event":"login","user":"charlie","ip":"192.168.1.1"}'
    '{"event":"signup","user":"diana","plan":"free"}'
    '{"event":"purchase","user":"alice","item":"gadget","amount":49.99}'
    '{"event":"logout","user":"bob"}'
    '{"event":"login","user":"alice","ip":"10.0.0.1"}'
    '{"event":"purchase","user":"charlie","item":"widget","amount":29.99}'
)

for i in "${!MESSAGES[@]}"; do
    KEY="user-$((i % 4))"
    VALUE="${MESSAGES[$i]}"

    RESULT=$(curl -s -X POST "$API/api/v1/produce" \
        -H "Content-Type: application/json" \
        -d "{\"topic\": \"$TOPIC\", \"key\": \"$KEY\", \"value\": $(echo "$VALUE" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))')}")

    OFFSET=$(echo "$RESULT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'partition={d[\"partition\"]} offset={d[\"offset\"]}')" 2>/dev/null || echo "$RESULT")
    echo -e "  ${GREEN}+${NC} $OFFSET  ${DIM}$KEY${NC}"
done

echo ""

# ── Consume messages ─────────────────────────────────────────────────────────

echo -e "${BOLD}Consume messages${NC}  ${DIM}GET /api/v1/consume${NC}"

# Small delay to let buffered writes flush
sleep 2

TOTAL=0
for p in $(seq 0 $((PARTITIONS - 1))); do
    RESULT=$(curl -s "$API/api/v1/consume?topic=$TOPIC&partition=$p&offset=0")
    COUNT=$(echo "$RESULT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('records', [])))" 2>/dev/null || echo "0")
    TOTAL=$((TOTAL + COUNT))

    if [ "$COUNT" -gt "0" ]; then
        echo -e "  Partition $p: ${GREEN}$COUNT records${NC}"
        echo "$RESULT" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for r in d.get('records', [])[:3]:
    val = r.get('value', '')
    try:
        val = json.loads(val)
        val = json.dumps(val, separators=(',', ':'))
    except: pass
    key = r.get('key', '')
    print(f'    offset={r[\"offset\"]}  key={key}  {val}')
if len(d.get('records', [])) > 3:
    print(f'    ... and {len(d[\"records\"]) - 3} more')
" 2>/dev/null || true
    fi
done

if [ "$TOTAL" -eq "0" ]; then
    echo -e "  ${DIM}No records yet (messages are buffered, try again in a few seconds)${NC}"
    echo -e "  ${DIM}Retry: curl '$API/api/v1/consume?topic=$TOPIC&partition=0&offset=0'${NC}"
fi

echo ""

# ── Summary ──────────────────────────────────────────────────────────────────

echo -e "${BOLD}What's running${NC}"
echo "  REST API     $API"
echo "  gRPC         localhost:50051"
echo "  Kafka proto  localhost:9092"
echo ""
echo -e "${BOLD}Try it yourself${NC}"
echo "  # Produce"
echo "  curl -X POST $API/api/v1/produce \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"topic\": \"$TOPIC\", \"key\": \"my-key\", \"value\": \"{\\\"hello\\\": \\\"world\\\"}\"}'  "
echo ""
echo "  # Consume"
echo "  curl '$API/api/v1/consume?topic=$TOPIC&partition=0&offset=0'"
echo ""
echo "  # List topics"
echo "  curl $API/api/v1/topics"
echo ""

if [ "$STARTED_HERE" = true ]; then
    echo -e "${DIM}Server running in background (pid $(cat "$PID_FILE")). Press Ctrl+C to stop.${NC}"
    wait "$(cat "$PID_FILE")" 2>/dev/null || true
fi
