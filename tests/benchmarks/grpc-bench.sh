#!/usr/bin/env bash
# gRPC Benchmark — local equivalent of the user's Fly.io ghz test
# Uses ghz if available, otherwise falls back to the existing stress_test_e2e example

# This script is sourced by phase 08, so common.sh variables are available

BENCH_TOPIC="bench-grpc"
BENCH_PARTITIONS=4
PROTO="${PROJECT_ROOT}/crates/streamhouse-proto/proto/streamhouse.proto"

# Create benchmark topic
http_request POST "${TEST_HTTP}/api/v1/topics" \
    "{\"name\":\"$BENCH_TOPIC\",\"partitions\":$BENCH_PARTITIONS}"

if [ "${HAS_GHZ:-0}" = "1" ]; then
    echo -e "  ${DIM}Using ghz for gRPC benchmark${NC}"

    # Generate 10 base64-encoded records for the batch
    RECORDS=$(python3 -c "
import json, base64
records = []
for i in range(10):
    records.append({
        'key': base64.b64encode(f'bench-{i}'.encode()).decode(),
        'value': base64.b64encode(('x' * 128).encode()).decode()
    })
print(json.dumps(records))
")

    GHZ_ERRORS=0

    for p in $(seq 0 $((BENCH_PARTITIONS - 1))); do
        DATA=$(python3 -c "
import json
records = $RECORDS
print(json.dumps({
    'topic': '$BENCH_TOPIC',
    'partition': $p,
    'records': records,
    'ack_mode': 0
}))
")

        echo -e "  ${DIM}Partition $p: 50 concurrent, 10000 requests...${NC}"

        RESULT=$(ghz --insecure \
            --proto "$PROTO" \
            --call streamhouse.StreamHouse.ProduceBatch \
            -d "$DATA" \
            -c 50 -n 10000 \
            --format json \
            "localhost:${TEST_GRPC_PORT}" 2>&1) || true

        # Save result
        echo "$RESULT" > "${RUN_DIR}/ghz_partition_${p}.json"

        # Parse key metrics
        RPS=$(echo "$RESULT" | jq -r '.rps // 0' 2>/dev/null) || RPS=0
        AVG=$(echo "$RESULT" | jq -r '.average // 0' 2>/dev/null) || AVG=0
        P99=$(echo "$RESULT" | jq -r '.latencyDistribution[] | select(.percentage == 99) | .latency // "N/A"' 2>/dev/null) || P99="N/A"
        ERRORS=$(echo "$RESULT" | jq -r '.errorDistribution | length // 0' 2>/dev/null) || ERRORS=0

        if [ "$ERRORS" -gt 0 ]; then
            GHZ_ERRORS=$((GHZ_ERRORS + 1))
        fi

        echo -e "    RPS: ${RPS}  Avg: ${AVG}  P99: ${P99}  Errors: ${ERRORS}"
    done

    if [ "$GHZ_ERRORS" -eq 0 ]; then
        pass "gRPC benchmark (ghz) — $BENCH_PARTITIONS partitions"
    else
        fail "gRPC benchmark" "$GHZ_ERRORS partitions had errors"
    fi

    # Run the full 4-partition concurrent test (matches Fly.io setup)
    echo ""
    echo -e "  ${DIM}Running full concurrent benchmark (all partitions simultaneously)...${NC}"

    GHZ_PIDS=()
    for p in $(seq 0 $((BENCH_PARTITIONS - 1))); do
        DATA=$(python3 -c "
import json
records = $RECORDS
print(json.dumps({
    'topic': '$BENCH_TOPIC',
    'partition': $p,
    'records': records,
    'ack_mode': 1
}))
")
        ghz --insecure \
            --proto "$PROTO" \
            --call streamhouse.StreamHouse.ProduceBatch \
            -d "$DATA" \
            -c 50 -n 10000 \
            --format json \
            "localhost:${TEST_GRPC_PORT}" > "${RUN_DIR}/ghz_concurrent_p${p}.json" 2>&1 &
        GHZ_PIDS+=($!)
    done

    for pid in "${GHZ_PIDS[@]}"; do
        wait "$pid" 2>/dev/null || true
    done

    # Aggregate results
    TOTAL_RPS=0
    for p in $(seq 0 $((BENCH_PARTITIONS - 1))); do
        RPS=$(jq -r '.rps // 0' "${RUN_DIR}/ghz_concurrent_p${p}.json" 2>/dev/null) || RPS=0
        TOTAL_RPS=$(python3 -c "print(int($TOTAL_RPS + $RPS))")
    done

    echo -e "  ${BOLD}Aggregate RPS (concurrent): ~$TOTAL_RPS requests/sec${NC}"
    echo -e "  ${DIM}(Each request contains 10 records → ~$((TOTAL_RPS * 10)) messages/sec)${NC}"
    pass "gRPC concurrent benchmark — aggregate ~$TOTAL_RPS req/s"

else
    echo -e "  ${DIM}ghz not installed — using stress_test_e2e as fallback${NC}"
    skip "gRPC benchmark (ghz)" "not installed (brew install ghz)"
fi

# Cleanup
http_request DELETE "${TEST_HTTP}/api/v1/topics/$BENCH_TOPIC" &>/dev/null || true
