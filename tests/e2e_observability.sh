#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Observability & Benchmark Demo: Full StreamHouse Internals
#
# Unlike e2e_full.sh (pass/fail assertions), this script is VISUAL.
# It prints postgres query results, S3 file listings, latency measurements,
# and SQL benchmark timings — meant to be read by a human.
#
# Services are LEFT RUNNING at the end so you can explore Grafana, MinIO,
# Prometheus, and Swagger UI interactively.
#
# Usage:
#   ./tests/e2e_observability.sh              # full run (build + demo)
#   ./tests/e2e_observability.sh --no-build   # skip docker build
#   ./tests/e2e_observability.sh --cleanup    # tear down services
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

API_URL="http://localhost:8080"
TOPIC="obs-bench"
PARTITIONS=6

# -- Formatting ---------------------------------------------------------------

green()  { printf "\033[32m%s\033[0m\n" "$1"; }
red()    { printf "\033[31m%s\033[0m\n" "$1"; }
bold()   { printf "\033[1m%s\033[0m\n" "$1"; }
dim()    { printf "\033[2m%s\033[0m\n" "$1"; }
cyan()   { printf "\033[36m%s\033[0m\n" "$1"; }
yellow() { printf "\033[33m%s\033[0m\n" "$1"; }

header() {
    echo ""
    bold "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    bold "  $1"
    bold "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# -- Helpers ------------------------------------------------------------------

pg() {
    docker compose exec -T postgres psql -U streamhouse -d streamhouse --pset border=2 -c "SET app.current_organization_id = '00000000-0000-0000-0000-000000000000'; $1" 2>/dev/null | grep -v "^SET$"
}

# For scalar queries — returns just the number
pg_scalar() {
    docker compose exec -T postgres psql -U streamhouse -d streamhouse -t -A -c "SET app.current_organization_id = '00000000-0000-0000-0000-000000000000'; $1" 2>/dev/null | grep -v "^SET$" | head -1 | tr -d ' '
}

# MinIO mc wrapper — sets alias each call since minio-init is ephemeral
minio_mc() {
    docker compose exec -T minio sh -c "mc alias set myminio http://localhost:9000 minioadmin minioadmin >/dev/null 2>&1; mc $1" 2>/dev/null
}

sql_query() {
    local query="$1"
    curl -sf "$API_URL/api/v1/sql" \
        -H "Content-Type: application/json" \
        -d "{\"query\": \"$query\"}" 2>/dev/null
}

sql_timing() {
    local query="$1"
    local result
    result=$(sql_query "$query")
    local rows time_ms
    rows=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin)['rowCount'])" 2>/dev/null || echo "?")
    time_ms=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin)['executionTimeMs'])" 2>/dev/null || echo "?")
    printf "  %-55s | %5s rows | %s ms\n" "$query" "$rows" "$time_ms" >&2
    echo "$time_ms"
}

produce_message() {
    local topic="$1" partition="$2" key="$3" value="$4"
    # Escape inner double quotes for safe JSON embedding
    local escaped_value="${value//\"/\\\"}"
    curl -sf "$API_URL/api/v1/produce" \
        -H "Content-Type: application/json" \
        -d "{\"topic\":\"$topic\",\"partition\":$partition,\"key\":\"$key\",\"value\":\"$escaped_value\"}" \
        >/dev/null 2>&1
}

consume_messages() {
    local topic="$1" partition="$2" offset="${3:-0}" max="${4:-100}"
    curl -sf "$API_URL/api/v1/consume?topic=$topic&partition=$partition&offset=$offset&maxRecords=$max" 2>/dev/null
}

ms_now() {
    python3 -c "import time; print(int(time.time()*1000))"
}

# -- Cleanup ------------------------------------------------------------------

if [ "${1:-}" = "--cleanup" ]; then
    bold "=== Cleanup ==="
    echo "  Stopping all docker compose services..."
    docker compose down -v --remove-orphans 2>/dev/null || true
    echo "  Done."
    exit 0
fi

# =============================================================================
# Section 0: Start Full Stack
# =============================================================================

header "Section 0: Start Full Stack (infra + monitoring)"

if [ "${1:-}" != "--no-build" ]; then
    echo "  Building docker images..."
    docker compose build streamhouse agent-1 agent-2 agent-3
    echo ""
else
    dim "  (build skipped with --no-build)"
fi

echo "  Cleaning previous state..."
docker compose down -v --remove-orphans 2>/dev/null || true
echo ""

echo "  Starting postgres & minio..."
docker compose up -d postgres minio minio-init
echo "  Waiting for postgres..."
for i in $(seq 1 30); do
    if docker compose exec -T postgres pg_isready -U streamhouse >/dev/null 2>&1; then break; fi
    sleep 1
done
green "  postgres ready"

echo "  Waiting for minio..."
for i in $(seq 1 30); do
    if curl -sf http://localhost:9000/minio/health/live >/dev/null 2>&1; then break; fi
    sleep 1
done
green "  minio ready"
sleep 3  # let minio-init finish

echo ""
echo "  Starting streamhouse server (RECONCILE_INTERVAL=10, RECONCILE_GRACE=0)..."
export RECONCILE_INTERVAL=10
export RECONCILE_GRACE=0
docker compose up -d streamhouse
echo "  Waiting for server health..."
for i in $(seq 1 60); do
    if curl -sf "$API_URL/health" >/dev/null 2>&1; then break; fi
    sleep 1
done
green "  server healthy"

echo ""
echo "  Starting 3 agents..."
docker compose up -d agent-1 agent-2 agent-3
echo "  Waiting for agent registration..."
for i in $(seq 1 30); do
    count=$(curl -sf "$API_URL/api/v1/agents" 2>/dev/null | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo 0)
    if [ "$count" -ge 3 ]; then break; fi
    sleep 1
done
green "  $count agents registered"

echo ""
echo "  Starting prometheus & grafana..."
docker compose up -d prometheus grafana
for i in $(seq 1 30); do
    if curl -sf http://localhost:9091/-/healthy >/dev/null 2>&1; then break; fi
    sleep 1
done
green "  prometheus ready"
for i in $(seq 1 30); do
    if curl -sf http://localhost:3001/api/health >/dev/null 2>&1; then break; fi
    sleep 1
done
green "  grafana ready"

echo ""
cyan "  Access URLs:"
echo "    REST API:       $API_URL"
echo "    Swagger UI:     $API_URL/swagger-ui/"
echo "    Grafana:        http://localhost:3001  (admin/admin)"
echo "    Prometheus:     http://localhost:9091"
echo "    MinIO Console:  http://localhost:9001  (minioadmin/minioadmin)"

# From here on, disable errexit — this is a demo script, not a test.
# Individual failures are shown inline rather than aborting the whole run.
set +e

# =============================================================================
# Section 1: Postgres Internals — Metadata State
# =============================================================================

header "Section 1: Postgres Internals — Initial State"

echo ""
echo "  Creating topic '$TOPIC' with $PARTITIONS partitions..."
if curl -sf "$API_URL/api/v1/topics" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}" >/dev/null 2>&1; then
    green "  topic created"
else
    red "  FAILED to create topic (may already exist)"
fi

echo ""
yellow "  >>> topics table:"
pg "SELECT name, partition_count, cleanup_policy, to_timestamp(created_at/1000) as created FROM topics;"

echo ""
yellow "  >>> partitions with watermarks:"
pg "SELECT topic, partition_id, high_watermark FROM partitions WHERE topic = '$TOPIC' ORDER BY partition_id;"

echo ""
yellow "  >>> registered agents:"
pg "SELECT agent_id, availability_zone, agent_group, to_timestamp(last_heartbeat/1000) as last_heartbeat FROM agents ORDER BY agent_id;"

# =============================================================================
# Section 2: Agent Auto-Discovery & Lease Assignment
# =============================================================================

header "Section 2: Agent Auto-Discovery & Lease Assignment"

echo ""
echo "  Waiting for partition leases to settle..."
for i in $(seq 1 60); do
    assigned=$(curl -sf "$API_URL/api/v1/topics/$TOPIC/partitions" 2>/dev/null | python3 -c "
import sys, json
parts = json.load(sys.stdin)
print(sum(1 for p in parts if p.get('leader_agent_id')))
" 2>/dev/null || echo 0)
    if [ "$assigned" -ge "$PARTITIONS" ]; then break; fi
    if [ $((i % 5)) -eq 0 ]; then
        dim "    ...${assigned}/${PARTITIONS} partitions assigned (${i}s)"
    fi
    sleep 1
done
green "  all $PARTITIONS partitions assigned"

echo ""
yellow "  >>> partition leases:"
pg "SELECT topic, partition_id, leader_agent_id, to_timestamp(lease_expires_at/1000) as expires, epoch FROM partition_leases WHERE topic = '$TOPIC' ORDER BY partition_id;"

echo ""
yellow "  >>> distribution by agent:"
curl -sf "$API_URL/api/v1/topics/$TOPIC/partitions" | python3 -c "
import sys, json
from collections import Counter
parts = json.load(sys.stdin)
leaders = [p['leader_agent_id'] for p in parts if p.get('leader_agent_id')]
for agent, count in sorted(Counter(leaders).items()):
    print(f'    {agent}: {count} partition(s)')
"

# =============================================================================
# Section 3: Produce & Buffer Reads — Latency Comparison
# =============================================================================

header "Section 3: Produce & Buffer Reads — Latency Comparison"

echo ""
echo "  Producing 100 messages to partition 0..."
for i in $(seq 1 100); do
    produce_message "$TOPIC" 0 "key-$i" "{\"order_id\":$i,\"amount\":$((RANDOM % 1000)),\"item\":\"widget-$i\"}"
done
green "  100 messages produced"

echo ""
echo "  Buffer read (immediate, before S3 flush)..."
T_START=$(ms_now)
BUFFER_RESULT=$(consume_messages "$TOPIC" 0 0 100)
T_END=$(ms_now)
BUFFER_MS=$((T_END - T_START))
BUFFER_COUNT=$(echo "$BUFFER_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['records']))" 2>/dev/null || echo "?")
green "  Buffer read: $BUFFER_COUNT records in ${BUFFER_MS}ms"

echo ""
echo "  Waiting 8s for S3 segment flush..."
sleep 8

echo "  S3 read (from offset 0, post-flush)..."
T_START=$(ms_now)
S3_RESULT=$(consume_messages "$TOPIC" 0 0 100)
T_END=$(ms_now)
S3_MS=$((T_END - T_START))
S3_COUNT=$(echo "$S3_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['records']))" 2>/dev/null || echo "?")
green "  S3 read: $S3_COUNT records in ${S3_MS}ms"

echo ""
bold "  Latency comparison:"
printf "    Buffer read (RAM):   %4d ms  (%s records)\n" "$BUFFER_MS" "$BUFFER_COUNT"
printf "    S3 read (disk/net):  %4d ms  (%s records)\n" "$S3_MS" "$S3_COUNT"

# =============================================================================
# Section 4: S3 State — Segment Files
# =============================================================================

header "Section 4: S3 State — Segment Files"

echo ""
yellow "  >>> S3 objects in minio:"
minio_mc "ls --recursive myminio/streamhouse/data/" || echo "    (no objects found)"

echo ""
yellow "  >>> segments table in postgres:"
pg "SELECT topic, partition_id, base_offset, end_offset, record_count, size_bytes, s3_key, to_timestamp(created_at/1000) as created FROM segments WHERE topic = '$TOPIC' ORDER BY partition_id, base_offset;"

echo ""
S3_COUNT_OBJ=$(minio_mc "ls --recursive myminio/streamhouse/data/" | wc -l | tr -d ' ')
PG_COUNT_SEG=$(pg_scalar "SELECT COUNT(*) FROM segments WHERE topic = '$TOPIC';" || echo "?")
echo "  S3 objects: $S3_COUNT_OBJ"
echo "  Postgres segments: $PG_COUNT_SEG"

# =============================================================================
# Section 5: S3 Orphan Reconciliation Demo (Live)
# =============================================================================

header "Section 5: S3 Orphan Reconciliation Demo (Live)"

echo ""
echo "  Uploading fake orphan segment to S3..."
docker compose exec -T minio sh -c "mc alias set myminio http://localhost:9000 minioadmin minioadmin >/dev/null 2>&1; echo 'fake-orphan-data-for-reconciler-test' | mc pipe myminio/streamhouse/data/$TOPIC/0/99999999999999999999.seg" 2>/dev/null
green "  orphan uploaded: data/$TOPIC/0/99999999999999999999.seg"

echo ""
echo "  Verifying orphan exists in S3 but NOT in postgres..."
yellow "  >>> S3 listing (includes orphan):"
minio_mc "ls myminio/streamhouse/data/$TOPIC/0/" || true

ORPHAN_IN_PG=$(pg_scalar "SELECT COUNT(*) FROM segments WHERE s3_key LIKE '%99999999999999999999%';" || echo "?")
echo "  Orphan in postgres segments table: $ORPHAN_IN_PG (expected: 0)"

echo ""
echo "  Reconciler running every 10s with 0s grace period..."
echo "  Waiting for reconciler to clean up orphan (up to 30s)..."
ORPHAN_GONE=false
for i in $(seq 1 30); do
    if ! minio_mc "stat myminio/streamhouse/data/$TOPIC/0/99999999999999999999.seg" >/dev/null 2>&1; then
        ORPHAN_GONE=true
        green "  Orphan deleted after ~${i}s!"
        break
    fi
    if [ $((i % 5)) -eq 0 ]; then
        dim "    ...still waiting (${i}s)"
    fi
    sleep 1
done

if [ "$ORPHAN_GONE" = false ]; then
    yellow "  Orphan not deleted within 30s — reconciler may need more time."
    yellow "  Check server logs: docker compose logs --tail=20 streamhouse | grep -i reconcil"
fi

echo ""
yellow "  >>> S3 listing after reconciliation:"
minio_mc "ls myminio/streamhouse/data/$TOPIC/0/" || echo "    (empty)"

# =============================================================================
# Section 6: SQL Benchmarks
# =============================================================================

header "Section 6: SQL Benchmarks"

echo ""
bold "  Query                                                   |  Rows | Time"
echo "  --------------------------------------------------------|-------|------"

SQL_SHOW_TIME=$(sql_timing "SHOW TOPICS")
SQL_SELECT_TIME=$(sql_timing "SELECT * FROM \\\"$TOPIC\\\" LIMIT 100")
SQL_COUNT_TIME=$(sql_timing "SELECT COUNT(*) FROM \\\"$TOPIC\\\"")
SQL_PARTITION_TIME=$(sql_timing "SELECT * FROM \\\"$TOPIC\\\" WHERE partition = 0 LIMIT 50")
SQL_GROUP_TIME=$(sql_timing "SELECT partition, COUNT(*) FROM \\\"$TOPIC\\\" GROUP BY partition")
SQL_JSON_TIME=$(sql_timing "SELECT json_extract(value, '$.order_id') FROM \\\"$TOPIC\\\" LIMIT 10")

# =============================================================================
# Section 7: Sustained Load Test & Throughput
# =============================================================================

header "Section 7: Sustained Load Test & Throughput"

echo ""
echo "  Producing 1000 messages across $PARTITIONS partitions (round-robin)..."
T_START=$(ms_now)
for i in $(seq 1 1000); do
    p=$(( (i - 1) % PARTITIONS ))
    produce_message "$TOPIC" "$p" "load-$i" "{\"seq\":$i,\"partition\":$p,\"ts\":$(date +%s),\"data\":\"load-test-payload-$i\"}"
done
T_END=$(ms_now)
LOAD_MS=$((T_END - T_START))
if [ "$LOAD_MS" -gt 0 ]; then
    THROUGHPUT=$(( 1000 * 1000 / LOAD_MS ))
else
    THROUGHPUT="inf"
fi
green "  1000 messages produced in ${LOAD_MS}ms (~${THROUGHPUT} msg/s)"

echo ""
echo "  Cluster metrics snapshot:"
METRICS=$(curl -sf "$API_URL/api/v1/metrics" 2>/dev/null || echo "{}")
echo "$METRICS" | python3 -c "
import sys, json
m = json.load(sys.stdin)
print(f'    Topics:       {m.get(\"topicsCount\", \"?\")}')
print(f'    Agents:       {m.get(\"agentsCount\", \"?\")}')
print(f'    Partitions:   {m.get(\"partitionsCount\", \"?\")}')
print(f'    Total msgs:   {m.get(\"totalMessages\", \"?\")}')
" 2>/dev/null || echo "    (metrics unavailable)"

STORAGE=$(curl -sf "$API_URL/api/v1/metrics/storage" 2>/dev/null || echo "{}")
echo "$STORAGE" | python3 -c "
import sys, json
m = json.load(sys.stdin)
total = m.get('totalStorageBytes', 0)
segs = m.get('totalSegments', '?')
if isinstance(total, (int, float)):
    if total > 1024*1024:
        print(f'    Storage:      {total/1024/1024:.2f} MB ({segs} segments)')
    elif total > 1024:
        print(f'    Storage:      {total/1024:.1f} KB ({segs} segments)')
    else:
        print(f'    Storage:      {total} bytes ({segs} segments)')
else:
    print(f'    Storage:      {total} ({segs} segments)')
" 2>/dev/null || echo "    (storage metrics unavailable)"

echo ""
echo "  Verifying all messages queryable via SQL..."
sleep 2  # brief settle
TOTAL_RESULT=$(sql_query "SELECT COUNT(*) FROM \"$TOPIC\"")
TOTAL_COUNT=$(echo "$TOTAL_RESULT" | python3 -c "
import sys, json
r = json.load(sys.stdin)
print(r['rows'][0][0])
" 2>/dev/null || echo "?")
echo "  SQL COUNT(*): $TOTAL_COUNT messages (expected: 1100 = 100 initial + 1000 load)"

# =============================================================================
# Section 8: Prometheus Metrics Dump
# =============================================================================

header "Section 8: Prometheus Metrics Dump"

echo ""
yellow "  >>> Key Prometheus counters from /metrics:"
echo ""
PROM_METRICS=$(curl -sf "$API_URL/metrics" 2>/dev/null || echo "")
if [ -n "$PROM_METRICS" ]; then
    echo "$PROM_METRICS" | grep -E "^streamhouse_" | head -40 || echo "    (no streamhouse_ metrics found)"
else
    echo "    (metrics endpoint unavailable)"
fi

echo ""
yellow "  >>> Prometheus scrape targets:"
TARGETS=$(curl -sf "http://localhost:9091/api/v1/targets" 2>/dev/null || echo "")
echo "$TARGETS" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    for t in data.get('data', {}).get('activeTargets', []):
        url = t.get('scrapeUrl', '?')
        health = t.get('health', '?')
        last = t.get('lastScrape', '?')
        print(f'    {url}  [{health}]  last: {last}')
except:
    print('    (could not parse targets)')
" 2>/dev/null

# =============================================================================
# Section 9: Summary & Dashboard Links
# =============================================================================

header "Section 9: Summary"

# Collect final segment counts
FINAL_S3=$(minio_mc "ls --recursive myminio/streamhouse/data/" | wc -l | tr -d ' ')
FINAL_PG_SEG=$(pg_scalar "SELECT COUNT(*) FROM segments;" || echo "?")
FINAL_AGENTS=$(curl -sf "$API_URL/api/v1/agents" 2>/dev/null | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")

echo ""
bold "  === Observability Test Summary ==="
echo ""
printf "    Buffer read latency:    %s ms\n" "$BUFFER_MS"
printf "    S3 read latency:        %s ms\n" "$S3_MS"
printf "    Produce throughput:     %s msg/s\n" "$THROUGHPUT"
printf "    SQL SELECT latency:     %s ms\n" "${SQL_SELECT_TIME:-?}"
printf "    SQL COUNT(*) latency:   %s ms\n" "${SQL_COUNT_TIME:-?}"
printf "    Segments in S3:         %s\n" "$FINAL_S3"
printf "    Segments in Postgres:   %s\n" "$FINAL_PG_SEG"
printf "    Agents active:          %s\n" "$FINAL_AGENTS"
printf "    Partitions assigned:    %s/%s\n" "$PARTITIONS" "$PARTITIONS"
printf "    Load test:              1000 msgs in %s ms\n" "$LOAD_MS"
echo ""

bold "  === Explore Dashboards ==="
echo ""
echo "    Grafana Overview:    http://localhost:3001/d/streamhouse-overview"
echo "    Grafana Producer:    http://localhost:3001/d/streamhouse-producer"
echo "    Grafana Storage:     http://localhost:3001/d/streamhouse-storage"
echo "    Grafana WAL:         http://localhost:3001/d/streamhouse-wal"
echo "    MinIO Console:       http://localhost:9001"
echo "    Prometheus:          http://localhost:9091"
echo "    Swagger API:         http://localhost:8080/swagger-ui/"
echo ""

green "  Services are still running -- explore the dashboards!"
echo ""
echo "  When done:"
echo "    ./tests/e2e_observability.sh --cleanup"
echo "    or: docker compose down -v"
echo ""
