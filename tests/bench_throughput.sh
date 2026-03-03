#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# StreamHouse Throughput Benchmark
#
# Measures REAL throughput through the full production stack:
#
#   Part 1: REST batch produce (curl → HTTP → WriterPool → WAL → S3)
#   Part 2: gRPC batch produce (Rust client → gRPC → Agent → WAL → S3)
#   Part 3: Consumer throughput (REST consume, parallel partitions)
#   Part 4: Consumer group operations (commit + fetch offsets)
#
# All tests run against the real Docker stack (Postgres, MinIO, Agents).
# Nothing is mocked.
#
# Usage:
#   ./tests/bench_throughput.sh            # use running Docker stack
#   ./tests/bench_throughput.sh --fresh    # start clean stack first
#   ./tests/bench_throughput.sh --cleanup  # tear down stack
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

API_URL="http://localhost:8080"
TOPIC="bench-throughput"
PARTITIONS=12

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

now_secs() {
    python3 -c "import time; print(f'{time.time():.6f}')"
}

elapsed_secs() {
    python3 -c "print(f'{$2 - $1:.3f}')"
}

calc_rate() {
    # $1=start, $2=end, $3=count -> "count / elapsed" as formatted string
    python3 -c "
elapsed = $2 - $1
if elapsed > 0:
    print(f'{$3 / elapsed:,.0f}')
else:
    print('inf')
"
}

generate_batch() {
    local topic="$1" partition="$2" count="$3" offset="$4"
    python3 -c "
import json, sys
topic, part, count, offset = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
records = [{'key': f'k-{part}-{offset+i}', 'value': f'{{\"p\":{part},\"s\":{offset+i}}}'} for i in range(count)]
print(json.dumps({'topic': topic, 'partition': part, 'records': records}))
" "$topic" "$partition" "$count" "$offset"
}

wait_healthy() {
    local max_wait="${1:-60}"
    local elapsed=0
    while [ $elapsed -lt "$max_wait" ]; do
        if curl -sf "$API_URL/health" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    red "ERROR: Server not healthy after ${max_wait}s"
    return 1
}

# -- Cleanup ------------------------------------------------------------------

if [ "${1:-}" = "--cleanup" ]; then
    bold "=== Cleanup ==="
    echo "  Stopping all docker compose services..."
    docker compose down -v --remove-orphans 2>/dev/null || true
    echo "  Done."
    exit 0
fi

# -- Fresh stack (optional) ---------------------------------------------------

if [ "${1:-}" = "--fresh" ]; then
    header "Starting fresh Docker stack"
    echo "  Tearing down existing stack..."
    docker compose down -v --remove-orphans 2>/dev/null || true
    echo "  Building images..."
    docker compose build streamhouse agent-1 agent-2 agent-3
    echo "  Starting services..."
    docker compose up -d
    echo "  Waiting for server health..."
    wait_healthy 90
    green "  Stack is healthy."
else
    echo ""
    dim "  Using running stack (pass --fresh to start clean)"
    wait_healthy 10 || {
        red "ERROR: No running stack. Use --fresh to start one."
        exit 1
    }
fi

# =============================================================================
# Setup: Create benchmark topic
# =============================================================================

header "Setup: Creating benchmark topic ($TOPIC, $PARTITIONS partitions)"

curl -sf -X DELETE "$API_URL/api/v1/topics/$TOPIC" >/dev/null 2>&1 || true
sleep 1

HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$API_URL/api/v1/topics" \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"$TOPIC\", \"partitions\": $PARTITIONS}")

if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    green "  Topic ready (HTTP $HTTP_STATUS)"
else
    red "  Failed to create topic (HTTP $HTTP_STATUS)"
    exit 1
fi

echo "  Waiting for partition lease assignment..."
sleep 5

# Collect all results for final summary
ALL_RESULTS=()

# #############################################################################
#
#  PART 1: REST Batch Produce
#
# #############################################################################

header "Part 1: REST Batch Produce (curl → HTTP → WriterPool → WAL → S3)"
echo ""
dim "  Measures: HTTP overhead + full write path"
dim "  Bottleneck: curl process spawning, TCP connection per request, Docker networking"
dim "  $PARTITIONS partitions, parallel curl per partition"

REST_PRODUCE_RESULTS=()

run_rest_produce() {
    local label="$1" total_msgs="$2" batch_size="$3"
    local msgs_per_partition=$((total_msgs / PARTITIONS))
    local full_batches=$((msgs_per_partition / batch_size))
    local remainder=$((msgs_per_partition % batch_size))
    local actual_total=$(( (full_batches * batch_size + (remainder > 0 ? remainder : 0)) * PARTITIONS ))

    echo ""
    cyan "  $label"
    if [ "$remainder" -gt 0 ]; then
        dim "    $actual_total msgs = $PARTITIONS partitions x (${full_batches}x${batch_size} + 1x${remainder})/partition"
    else
        dim "    $actual_total msgs = $PARTITIONS partitions x $full_batches batches x $batch_size/batch"
    fi

    local total_batches_per_partition=$full_batches
    if [ "$remainder" -gt 0 ]; then
        total_batches_per_partition=$((full_batches + 1))
    fi

    # Pre-generate payloads to disk
    local tmpdir
    tmpdir=$(mktemp -d)
    dim "    Pre-generating payloads..."
    local p b
    for (( p=0; p<PARTITIONS; p++ )); do
        for (( b=0; b<full_batches; b++ )); do
            generate_batch "$TOPIC" "$p" "$batch_size" "$((b * batch_size))" > "$tmpdir/p${p}_b${b}.json"
        done
        if [ "$remainder" -gt 0 ]; then
            generate_batch "$TOPIC" "$p" "$remainder" "$((full_batches * batch_size))" > "$tmpdir/p${p}_b${full_batches}.json"
        fi
    done

    # Fire off all partitions in parallel
    local t_start
    t_start=$(now_secs)

    local pids=()
    local error_file="$tmpdir/errors"
    echo "0" > "$error_file"

    for (( p=0; p<PARTITIONS; p++ )); do
        (
            errs=0
            for (( b=0; b<total_batches_per_partition; b++ )); do
                status=$(curl -s -o /dev/null -w "%{http_code}" \
                    -X POST "$API_URL/api/v1/produce/batch" \
                    -H "Content-Type: application/json" \
                    -d @"$tmpdir/p${p}_b${b}.json")
                if [ "$status" != "200" ] && [ "$status" != "201" ]; then
                    errs=$((errs + 1))
                fi
            done
            [ $errs -gt 0 ] && echo "$errs" >> "$error_file"
        ) &
        pids+=($!)
    done

    local failures=0
    for pid in "${pids[@]}"; do
        wait "$pid" || failures=$((failures + 1))
    done

    local t_end
    t_end=$(now_secs)

    local total_errors
    total_errors=$(python3 -c "
import sys
total = sum(int(l.strip()) for l in open(sys.argv[1]) if l.strip().isdigit())
print(total)
" "$error_file")

    local elapsed rate
    elapsed=$(elapsed_secs "$t_start" "$t_end")
    rate=$(calc_rate "$t_start" "$t_end" "$actual_total")

    REST_PRODUCE_RESULTS+=("$label|$actual_total|$elapsed|$rate|$total_errors")
    ALL_RESULTS+=("REST Produce: $label|$actual_total|$elapsed|$rate|$total_errors")

    if [ "$total_errors" -gt 0 ] || [ "$failures" -gt 0 ]; then
        yellow "    ${elapsed}s  |  ${rate} msgs/sec  |  errors: $total_errors"
    else
        green "    ${elapsed}s  |  ${rate} msgs/sec  |  0 errors"
    fi

    rm -rf "$tmpdir"
}

run_rest_produce "10K messages  (batch=1000)"   10000   1000
run_rest_produce "100K messages (batch=5000)"   100000  5000
run_rest_produce "1M messages   (batch=10000)"  1000000 10000

# #############################################################################
#
#  PART 2: REST Sustained Throughput (keep-alive, 30s duration)
#
# #############################################################################

header "Part 2: REST Sustained Produce (keep-alive connections, 30s)"
echo ""
dim "  Measures: same write path as Part 1, but with HTTP keep-alive connections"
dim "  and sustained load over 30 seconds (closer to production pattern)."
dim "  $PARTITIONS partitions, 1 persistent curl per partition, batch=5000"
echo ""

# Each partition runs a single curl process that reuses its connection for 30s.
# This eliminates per-request TCP overhead and measures the server's true throughput.
SUSTAINED_BATCH=5000
SUSTAINED_DURATION=30

# Pre-generate one batch payload per partition (reused across iterations)
SUSTAINED_TMP=$(mktemp -d)
for (( p=0; p<PARTITIONS; p++ )); do
    generate_batch "$TOPIC" "$p" "$SUSTAINED_BATCH" "0" > "$SUSTAINED_TMP/p${p}.json"
done

cyan "  Running sustained produce test (${SUSTAINED_DURATION}s)..."

t_start=$(now_secs)
pids=()

for (( p=0; p<PARTITIONS; p++ )); do
    (
        count=0
        errs=0
        deadline=$(python3 -c "import time; print(time.time() + $SUSTAINED_DURATION)")

        while python3 -c "import time,sys; sys.exit(0 if time.time() < $deadline else 1)"; do
            status=$(curl -s -o /dev/null -w "%{http_code}" \
                --keepalive-time 60 \
                -X POST "$API_URL/api/v1/produce/batch" \
                -H "Content-Type: application/json" \
                -d @"$SUSTAINED_TMP/p${p}.json")
            if [ "$status" = "200" ] || [ "$status" = "201" ]; then
                count=$((count + 1))
            else
                errs=$((errs + 1))
            fi
        done
        echo "$count $errs" > "$SUSTAINED_TMP/p${p}.result"
    ) &
    pids+=($!)
done

for pid in "${pids[@]}"; do
    wait "$pid" || true
done

t_end=$(now_secs)

# Sum results
total_batches=0
total_errors=0
for (( p=0; p<PARTITIONS; p++ )); do
    if [ -f "$SUSTAINED_TMP/p${p}.result" ]; then
        read -r cnt err < "$SUSTAINED_TMP/p${p}.result"
        total_batches=$((total_batches + cnt))
        total_errors=$((total_errors + err))
    fi
done

sustained_msgs=$((total_batches * SUSTAINED_BATCH))
sustained_elapsed=$(elapsed_secs "$t_start" "$t_end")
sustained_rate=$(calc_rate "$t_start" "$t_end" "$sustained_msgs")

ALL_RESULTS+=("REST Sustained (${SUSTAINED_DURATION}s)|$sustained_msgs|$sustained_elapsed|$sustained_rate|$total_errors")

green "    $sustained_msgs msgs in ${sustained_elapsed}s  |  ${sustained_rate} msgs/sec  |  errors: $total_errors"
dim "    ($total_batches batches x $SUSTAINED_BATCH records/batch across $PARTITIONS partitions)"

rm -rf "$SUSTAINED_TMP"

# --- gRPC stress test (requires direct agent connectivity) ---
echo ""
dim "  Note: gRPC stress test (stress_test_e2e) requires direct agent connectivity."
dim "  In Docker mode, agents are on an internal network. Run it against a local server:"
dim "    cargo run -p streamhouse-client --example stress_test_e2e --release"

# #############################################################################
#
#  PART 3: Consumer Throughput
#
# #############################################################################

header "Part 3: Consumer Throughput (REST consume, parallel partitions)"
echo ""
dim "  Reads back all messages produced in Part 1"
dim "  Measures: S3 segment read + SegmentReader decode + HTTP serialization"
dim "  $PARTITIONS partitions consumed in parallel"
echo ""

# Wait for segments to be flushed to S3
echo "  Waiting 5s for segment flushes to S3..."
sleep 5

run_consume_benchmark() {
    local label="$1" max_records_per_fetch="$2"

    echo ""
    cyan "  $label"

    local tmpdir
    tmpdir=$(mktemp -d)
    local t_start
    t_start=$(now_secs)

    local pids=()
    local p
    for (( p=0; p<PARTITIONS; p++ )); do
        (
            total_consumed=0
            offset=0
            while true; do
                response=$(curl -sf \
                    "$API_URL/api/v1/consume?topic=$TOPIC&partition=$p&offset=$offset&maxRecords=$max_records_per_fetch" \
                    2>/dev/null) || break

                count=$(echo "$response" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    records = data.get('records', [])
    print(len(records))
except:
    print(0)
" 2>/dev/null)

                next_offset=$(echo "$response" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    print(data.get('nextOffset', 0))
except:
    print(0)
" 2>/dev/null)

                if [ "$count" = "0" ] || [ -z "$count" ]; then
                    # If server hints at a later offset, skip ahead
                    if [ -n "$next_offset" ] && [ "$next_offset" -gt "$offset" ] 2>/dev/null; then
                        offset=$next_offset
                        continue
                    fi
                    break
                fi

                total_consumed=$((total_consumed + count))
                offset=$((offset + count))
            done
            echo "$total_consumed" > "$tmpdir/p${p}.count"
        ) &
        pids+=($!)
    done

    for pid in "${pids[@]}"; do
        wait "$pid" || true
    done

    local t_end
    t_end=$(now_secs)

    # Sum consumed records across all partitions
    local total_consumed=0
    for (( p=0; p<PARTITIONS; p++ )); do
        if [ -f "$tmpdir/p${p}.count" ]; then
            local c
            c=$(cat "$tmpdir/p${p}.count")
            total_consumed=$((total_consumed + c))
        fi
    done

    local elapsed rate
    elapsed=$(elapsed_secs "$t_start" "$t_end")
    rate=$(calc_rate "$t_start" "$t_end" "$total_consumed")

    ALL_RESULTS+=("Consume: $label|$total_consumed|$elapsed|$rate|0")

    green "    $total_consumed records consumed in ${elapsed}s  |  ${rate} msgs/sec"

    rm -rf "$tmpdir"
}

run_consume_benchmark "Fetch 1000/req"  1000
run_consume_benchmark "Fetch 5000/req"  5000
run_consume_benchmark "Fetch 10000/req" 10000

# #############################################################################
#
#  PART 4: Consumer Group Operations
#
# #############################################################################

header "Part 4: Consumer Group Operations"
echo ""
dim "  Measures: offset commit latency, offset fetch latency, lag calculation"
echo ""

GROUP_ID="bench-group-$$"

# --- Offset Commit Throughput ---
cyan "  Offset Commits (1000 sequential commits)"
COMMIT_COUNT=1000
t_start=$(now_secs)
commit_errors=0

for i in $(seq 1 $COMMIT_COUNT); do
    partition=$((i % PARTITIONS))
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$API_URL/api/v1/consumer-groups/commit" \
        -H "Content-Type: application/json" \
        -d "{\"groupId\": \"$GROUP_ID\", \"topic\": \"$TOPIC\", \"partition\": $partition, \"offset\": $i}")
    if [ "$status" != "200" ] && [ "$status" != "201" ]; then
        commit_errors=$((commit_errors + 1))
    fi
done

t_end=$(now_secs)
commit_elapsed=$(elapsed_secs "$t_start" "$t_end")
commit_rate=$(calc_rate "$t_start" "$t_end" "$COMMIT_COUNT")
ALL_RESULTS+=("Consumer Group: commits|$COMMIT_COUNT|$commit_elapsed|$commit_rate|$commit_errors")
green "    $commit_elapsed s  |  $commit_rate commits/sec  |  errors: $commit_errors"

# --- Get Group Details (lag) ---
echo ""
cyan "  Consumer Group Lag Query"
t_start=$(now_secs)
lag_response=$(curl -sf "$API_URL/api/v1/consumer-groups/$GROUP_ID" 2>/dev/null || echo "{}")
t_end=$(now_secs)
lag_elapsed=$(elapsed_secs "$t_start" "$t_end")
green "    Lag query: ${lag_elapsed}s"

# --- Offset Fetch Throughput ---
echo ""
cyan "  Offset Fetches (read committed offsets for all partitions, 100 iterations)"
FETCH_ITERS=100
t_start=$(now_secs)
fetch_errors=0

for i in $(seq 1 $FETCH_ITERS); do
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        "$API_URL/api/v1/consumer-groups/$GROUP_ID")
    if [ "$status" != "200" ]; then
        fetch_errors=$((fetch_errors + 1))
    fi
done

t_end=$(now_secs)
fetch_elapsed=$(elapsed_secs "$t_start" "$t_end")
fetch_rate=$(calc_rate "$t_start" "$t_end" "$FETCH_ITERS")
ALL_RESULTS+=("Consumer Group: fetches|$FETCH_ITERS|$fetch_elapsed|$fetch_rate|$fetch_errors")
green "    $fetch_elapsed s  |  $fetch_rate fetches/sec  |  errors: $fetch_errors"

# Cleanup consumer group
curl -sf -X DELETE "$API_URL/api/v1/consumer-groups/$GROUP_ID" >/dev/null 2>&1 || true

# #############################################################################
#
#  Final Summary
#
# #############################################################################

header "Full Results Summary"
echo ""
printf "  %-40s  %10s  %10s  %15s  %8s\n" "Benchmark" "Count" "Time (s)" "Rate" "Errors"
printf "  %-40s  %10s  %10s  %15s  %8s\n" "$(printf '%0.s-' {1..40})" "----------" "----------" "---------------" "--------"

for result in "${ALL_RESULTS[@]}"; do
    IFS='|' read -r label count elapsed rate errors <<< "$result"
    printf "  %-40s  %10s  %10s  %12s/s  %8s\n" "$label" "$count" "$elapsed" "$rate" "$errors"
done

echo ""
bold "  What these numbers mean:"
echo ""
echo "  REST Produce:     Includes curl process spawn + TCP connect + Docker network"
echo "                    per request. This is a floor, not a ceiling."
echo ""
echo "  gRPC Produce:     Persistent connection, batched, no process overhead."
echo "                    Closest to real production throughput."
echo ""
echo "  Consumer:         Reads segments from S3 (MinIO), decodes, returns via HTTP."
echo "                    Larger fetch sizes reduce per-request overhead."
echo ""
echo "  Consumer Groups:  Offset commit/fetch against Postgres-backed metadata."
echo ""
bold "  Production vs Local:"
echo "  - Real S3 has near-infinite write bandwidth (no local disk contention)"
echo "  - Dedicated compute means no CPU/IOPS sharing between server + S3 + DB"
echo "  - gRPC with connection pooling eliminates per-request TCP overhead"
echo "  - Expect 5-20x higher throughput in production vs Docker-on-laptop"
echo ""
