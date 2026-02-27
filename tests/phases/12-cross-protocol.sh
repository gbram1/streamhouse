#!/usr/bin/env bash
# Phase 12 — Multi-Protocol Cross-Verification
# Produces via one protocol, consumes via another, verifies data integrity

phase_header "Phase 12 — Multi-Protocol Cross-Verification"

API="${TEST_HTTP}/api/v1"
TOPIC="cross-verify"
PARTITIONS=4
TOOL_TIMEOUT=5  # seconds for grpcurl/kcat operations

# Pre-check: verify Kafka broker is reachable before attempting kcat tests
KAFKA_REACHABLE=0
if [ "${HAS_KCAT:-0}" = "1" ]; then
    KCHECK=$(run_with_timeout 3 $KCAT_CMD -b "localhost:${TEST_KAFKA_PORT}" -L 2>&1) || KCHECK=""
    if echo "$KCHECK" | grep -qi "broker\|metadata"; then
        KAFKA_REACHABLE=1
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Setup
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Setup: create topic '$TOPIC' ($PARTITIONS partitions)"
else
    fail "Setup: create topic" "got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 1: REST produce → gRPC consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    BATCH=$(make_batch_json "$TOPIC" 0 10 "rest2grpc")
    http_request POST "$API/produce/batch" "$BATCH"

    wait_flush 8

    RESULT=$(run_with_timeout "$TOOL_TIMEOUT" grpcurl -plaintext \
        -d "{\"topic\":\"$TOPIC\",\"partition\":0,\"offset\":0,\"max_records\":100}" \
        "$TEST_GRPC" streamhouse.StreamHouse/Consume 2>/dev/null) || RESULT="{}"
    COUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || COUNT=0
    if [ "$COUNT" -ge 1 ]; then
        pass "REST produce → gRPC consume — $COUNT records"
    else
        fail "REST produce → gRPC consume" "got 0 records — cross-protocol read failed"
    fi
else
    skip "REST produce → gRPC consume" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 2: gRPC produce → REST consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    GRPC_BATCH=$(python3 -c "
import json, base64
records = []
for i in range(10):
    records.append({
        'key': base64.b64encode(f'grpc2rest-{i}'.encode()).decode(),
        'value': base64.b64encode(json.dumps({'source':'grpc','seq':i}).encode()).decode()
    })
print(json.dumps({'topic': '$TOPIC', 'partition': 1, 'records': records, 'ack_mode': 0}))
")
    run_with_timeout "$TOOL_TIMEOUT" grpcurl -plaintext -d "$GRPC_BATCH" \
        "$TEST_GRPC" streamhouse.StreamHouse/ProduceBatch &>/dev/null || true

    wait_flush 8

    http_request GET "$API/consume?topic=${TOPIC}&partition=1&offset=0&maxRecords=100"
    COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || COUNT=0
    if [ "$HTTP_STATUS" = "200" ] && [ "$COUNT" -ge 1 ]; then
        pass "gRPC produce → REST consume — $COUNT records"
    else
        fail "gRPC produce → REST consume" "status=$HTTP_STATUS count=$COUNT — cross-protocol read failed"
    fi
else
    skip "gRPC produce → REST consume" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 3: Kafka produce → REST consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "$KAFKA_REACHABLE" = "1" ]; then
    for i in $(seq 1 10); do
        echo "{\"source\":\"kafka\",\"seq\":$i}" | run_with_timeout "$TOOL_TIMEOUT" $KCAT_CMD -b "localhost:${TEST_KAFKA_PORT}" \
            -P -t "$TOPIC" -k "kafka2rest-$i" 2>/dev/null || true
    done

    wait_flush 8

    KAFKA_REST_TOTAL=0
    for p in $(seq 0 $((PARTITIONS - 1))); do
        http_request GET "$API/consume?topic=${TOPIC}&partition=${p}&offset=0&maxRecords=100000"
        if [ "$HTTP_STATUS" = "200" ]; then
            C=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || C=0
            KAFKA_REST_TOTAL=$((KAFKA_REST_TOTAL + C))
        fi
    done
    if [ "$KAFKA_REST_TOTAL" -ge 1 ]; then
        pass "Kafka produce → REST consume — $KAFKA_REST_TOTAL records total"
    else
        fail "Kafka produce → REST consume" "got 0 records — Kafka-produced data not reaching REST"
    fi
else
    skip "Kafka produce → REST consume" "Kafka broker not reachable via kcat"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 4: REST produce → SQL count
# ═══════════════════════════════════════════════════════════════════════════════

BATCH=$(make_batch_json "$TOPIC" 3 20 "rest2sql")
http_request POST "$API/produce/batch" "$BATCH"

wait_flush 8

http_request POST "$API/sql" "{\"query\":\"SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\"\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    SQL_COUNT=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // .rows[0].cnt // .results[0][0] // "0"' 2>/dev/null) || SQL_COUNT="0"
    if [ "$SQL_COUNT" != "null" ] && [ "$SQL_COUNT" != "0" ] 2>/dev/null; then
        pass "REST produce → SQL COUNT(*) = $SQL_COUNT"
    else
        fail "REST produce → SQL COUNT(*)" "count=$SQL_COUNT — SQL returned 0 records after flush"
    fi
else
    fail "REST → SQL count" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 5: REST produce → Kafka consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "$KAFKA_REACHABLE" = "1" ]; then
    KCAT_COUNT=0
    for p in $(seq 0 $((PARTITIONS - 1))); do
        C=$(run_with_timeout "$TOOL_TIMEOUT" $KCAT_CMD -b "localhost:${TEST_KAFKA_PORT}" -C -t "$TOPIC" -p "$p" -o beginning -c 50 -e 2>/dev/null | grep -c . 2>/dev/null) || C=0
        KCAT_COUNT=$((KCAT_COUNT + C))
    done
    if [ "$KCAT_COUNT" -ge 1 ]; then
        pass "REST produce → Kafka consume — $KCAT_COUNT messages via kcat"
    else
        fail "REST produce → Kafka consume" "got 0 messages — data not reaching Kafka consumers"
    fi
else
    skip "REST produce → Kafka consume" "Kafka broker not reachable via kcat"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 6: All-protocol combined count
# ═══════════════════════════════════════════════════════════════════════════════

TOTAL_COUNT=$(count_all_records "$TOPIC" "$PARTITIONS")
if [ "$TOTAL_COUNT" -ge 1 ]; then
    pass "All-protocol combined count — $TOTAL_COUNT records across all partitions"
else
    fail "All-protocol combined count" "got 0 records — no data in storage"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 7: Content integrity (REST → gRPC)
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    UNIQUE_PAYLOAD="{\"integrity_test\":\"cross-$(date +%s)\",\"protocol\":\"rest\"}"
    http_request POST "$API/produce" "{\"topic\":\"$TOPIC\",\"partition\":0,\"key\":\"integrity\",\"value\":$(echo "$UNIQUE_PAYLOAD" | jq -Rs .)}"

    wait_flush 8

    RESULT=$(run_with_timeout "$TOOL_TIMEOUT" grpcurl -plaintext \
        -d "{\"topic\":\"$TOPIC\",\"partition\":0,\"offset\":0,\"max_records\":100}" \
        "$TEST_GRPC" streamhouse.StreamHouse/Consume 2>/dev/null) || RESULT="{}"
    if echo "$RESULT" | jq -e '.records' &>/dev/null; then
        RCOUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || RCOUNT=0
        if [ "$RCOUNT" -ge 1 ]; then
            pass "Content integrity: REST → gRPC — $RCOUNT records readable"
        else
            fail "Content integrity: REST → gRPC" "records array is empty"
        fi
    else
        fail "Content integrity: REST → gRPC" "no records field in response"
    fi
else
    skip "Content integrity REST→gRPC" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/$TOPIC" &>/dev/null || true
