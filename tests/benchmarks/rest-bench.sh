#!/usr/bin/env bash
# REST API Benchmark — measures single produce latency and batch produce throughput

# This script is sourced by phase 08, so common.sh variables are available

API="${TEST_HTTP}/api/v1"
BENCH_TOPIC="bench-rest"
BENCH_PARTITIONS=4

# Create benchmark topic
http_request POST "$API/topics" "{\"name\":\"$BENCH_TOPIC\",\"partitions\":$BENCH_PARTITIONS}"

# ═══════════════════════════════════════════════════════════════════════════════
# Single Produce Latency
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}Measuring single produce latency (100 iterations)...${NC}"

LATENCY_FILE="${RUN_DIR}/rest_single_latencies.txt"
> "$LATENCY_FILE"

SINGLE_ERRORS=0
for i in $(seq 1 100); do
    PARTITION=$((i % BENCH_PARTITIONS))
    TIME=$(curl -s -o /dev/null -w "%{time_total}" -X POST "$API/produce" \
        -H "Content-Type: application/json" \
        -d "{\"topic\":\"$BENCH_TOPIC\",\"key\":\"bench-$i\",\"value\":\"{\\\"seq\\\":$i,\\\"type\\\":\\\"latency-test\\\"}\"}" \
        2>/dev/null) || { SINGLE_ERRORS=$((SINGLE_ERRORS + 1)); continue; }
    echo "$TIME" >> "$LATENCY_FILE"
done

if [ -s "$LATENCY_FILE" ]; then
    # Compute percentiles
    PERCENTILES=$(sort -n "$LATENCY_FILE" | python3 -c "
import sys
lines = [float(l.strip()) for l in sys.stdin if l.strip()]
if not lines:
    print('NO_DATA')
    sys.exit()
n = len(lines)
p50 = lines[int(n * 0.50)]
p95 = lines[int(n * 0.95)]
p99 = lines[int(min(n * 0.99, n - 1))]
avg = sum(lines) / n
mx = max(lines)
print(f'{avg*1000:.1f} {p50*1000:.1f} {p95*1000:.1f} {p99*1000:.1f} {mx*1000:.1f}')
")

    if [ "$PERCENTILES" != "NO_DATA" ]; then
        AVG=$(echo "$PERCENTILES" | awk '{print $1}')
        P50=$(echo "$PERCENTILES" | awk '{print $2}')
        P95=$(echo "$PERCENTILES" | awk '{print $3}')
        P99=$(echo "$PERCENTILES" | awk '{print $4}')
        MAX=$(echo "$PERCENTILES" | awk '{print $5}')

        echo -e "  Single produce latency (ms):"
        echo -e "    avg: ${AVG}ms  p50: ${P50}ms  p95: ${P95}ms  p99: ${P99}ms  max: ${MAX}ms"
        echo -e "    errors: $SINGLE_ERRORS/100"

        # Save to results
        echo "{\"type\":\"single_produce\",\"avg_ms\":$AVG,\"p50_ms\":$P50,\"p95_ms\":$P95,\"p99_ms\":$P99,\"max_ms\":$MAX,\"errors\":$SINGLE_ERRORS}" \
            > "${RUN_DIR}/rest_single_latency.json"

        pass "REST single produce — p99=${P99}ms, p50=${P50}ms"
    else
        fail "REST single produce latency" "no data collected"
    fi
else
    fail "REST single produce latency" "no latency data"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Batch Produce Throughput
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "  ${DIM}Measuring batch produce throughput (100 batches × 100 records)...${NC}"

BATCH_RECORDS=100
BATCH_ITERATIONS=100
BATCH_ERRORS=0
BATCH_LATENCIES="${RUN_DIR}/rest_batch_latencies.txt"
> "$BATCH_LATENCIES"

BATCH_START=$(python3 -c "import time; print(time.time())")

for i in $(seq 1 $BATCH_ITERATIONS); do
    PARTITION=$((i % BENCH_PARTITIONS))

    # Generate batch payload
    PAYLOAD=$(python3 -c "
import json
records = [{'key': f'batch-{$i}-{j}', 'value': json.dumps({'i': $i, 'j': j})} for j in range($BATCH_RECORDS)]
print(json.dumps({'topic': '$BENCH_TOPIC', 'partition': $PARTITION, 'records': records}))
")

    TIME=$(curl -s -o /dev/null -w "%{time_total}" -X POST "$API/produce/batch" \
        -H "Content-Type: application/json" \
        -d "$PAYLOAD" 2>/dev/null) || { BATCH_ERRORS=$((BATCH_ERRORS + 1)); continue; }
    echo "$TIME" >> "$BATCH_LATENCIES"
done

BATCH_END=$(python3 -c "import time; print(time.time())")
BATCH_WALL=$(python3 -c "print(f'{$BATCH_END - $BATCH_START:.2f}')")
TOTAL_MSGS=$((BATCH_ITERATIONS * BATCH_RECORDS))

if [ -s "$BATCH_LATENCIES" ]; then
    THROUGHPUT=$(python3 -c "print(int($TOTAL_MSGS / ($BATCH_END - $BATCH_START)))")

    BATCH_PCTS=$(sort -n "$BATCH_LATENCIES" | python3 -c "
import sys
lines = [float(l.strip()) for l in sys.stdin if l.strip()]
if not lines:
    print('NO_DATA')
    sys.exit()
n = len(lines)
p50 = lines[int(n * 0.50)]
p95 = lines[int(n * 0.95)]
p99 = lines[int(min(n * 0.99, n - 1))]
avg = sum(lines) / n
print(f'{avg*1000:.1f} {p50*1000:.1f} {p95*1000:.1f} {p99*1000:.1f}')
")

    if [ "$BATCH_PCTS" != "NO_DATA" ]; then
        B_AVG=$(echo "$BATCH_PCTS" | awk '{print $1}')
        B_P50=$(echo "$BATCH_PCTS" | awk '{print $2}')
        B_P95=$(echo "$BATCH_PCTS" | awk '{print $3}')
        B_P99=$(echo "$BATCH_PCTS" | awk '{print $4}')

        echo -e "  Batch produce ($BATCH_RECORDS records/batch):"
        echo -e "    Throughput: ${THROUGHPUT} msgs/sec (wall: ${BATCH_WALL}s)"
        echo -e "    Batch latency — avg: ${B_AVG}ms  p50: ${B_P50}ms  p95: ${B_P95}ms  p99: ${B_P99}ms"
        echo -e "    Errors: $BATCH_ERRORS/$BATCH_ITERATIONS"

        # Save to results
        echo "{\"type\":\"batch_produce\",\"records_per_batch\":$BATCH_RECORDS,\"batches\":$BATCH_ITERATIONS,\"throughput_msgs_sec\":$THROUGHPUT,\"wall_sec\":$BATCH_WALL,\"batch_avg_ms\":$B_AVG,\"batch_p50_ms\":$B_P50,\"batch_p95_ms\":$B_P95,\"batch_p99_ms\":$B_P99,\"errors\":$BATCH_ERRORS}" \
            > "${RUN_DIR}/rest_batch_throughput.json"

        pass "REST batch produce — ${THROUGHPUT} msgs/sec, p99=${B_P99}ms"
    else
        fail "REST batch produce" "no data"
    fi
else
    fail "REST batch produce" "no latency data"
fi

# Cleanup
http_request DELETE "$API/topics/$BENCH_TOPIC" &>/dev/null || true
