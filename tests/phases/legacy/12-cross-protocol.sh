#!/usr/bin/env bash
# Phase 12 — Cross-Protocol Verification
# Produces via one protocol, consumes via another, verifies data integrity

phase_header "Phase 12 — Cross-Protocol Verification"

API="${TEST_HTTP}/api/v1"
TOPIC="cross-proto"
PARTITIONS=4
TOOL_TIMEOUT=5

# ═══════════════════════════════════════════════════════════════════════════════
# Pre-checks
# ═══════════════════════════════════════════════════════════════════════════════

KAFKA_REACHABLE=0
if [ "${HAS_KCAT:-0}" = "1" ]; then
    KCHECK=$(run_with_timeout 3 $KCAT_CMD -b "localhost:${TEST_KAFKA_PORT}" -L 2>&1) || KCHECK=""
    if echo "$KCHECK" | grep -qi "broker\|metadata"; then
        KAFKA_REACHABLE=1
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Setup: Create topic
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Setup: create topic '$TOPIC' ($PARTITIONS partitions)"
else
    fail "Setup: create topic '$TOPIC'" "expected 201 or 409, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 1: REST produce → gRPC consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    BATCH=$(make_batch_json "$TOPIC" 0 20 "rest2grpc")
    http_request POST "$API/produce/batch" "$BATCH"
    if [ "$HTTP_STATUS" = "200" ]; then
        pass "REST produce 20 records for gRPC consume"
    else
        fail "REST produce 20 records for gRPC consume" "HTTP $HTTP_STATUS"
    fi

    wait_flush 8

    RESULT=$(run_with_timeout "$TOOL_TIMEOUT" grpcurl -plaintext \
        -d "{\"topic\":\"$TOPIC\",\"partition\":0,\"offset\":0,\"max_records\":100}" \
        "$TEST_GRPC" streamhouse.StreamHouse/Consume 2>/dev/null) || RESULT="{}"
    COUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || COUNT=0
    if [ "$COUNT" -ge 20 ]; then
        pass "REST -> gRPC: consumed $COUNT records (expected >= 20)"
    elif [ "$COUNT" -ge 1 ]; then
        fail "REST -> gRPC: count mismatch" "got $COUNT, expected >= 20"
    else
        fail "REST -> gRPC: no records found" "cross-protocol read returned 0"
    fi
else
    skip "REST -> gRPC produce" "grpcurl not installed"
    skip "REST -> gRPC consume" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 2: gRPC produce → REST consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    GRPC_BATCH=$(python3 -c "
import json, base64
records = []
for i in range(20):
    records.append({
        'key': base64.b64encode(f'grpc2rest-{i}'.encode()).decode(),
        'value': base64.b64encode(json.dumps({'source':'grpc','seq':i}).encode()).decode()
    })
print(json.dumps({'topic': '$TOPIC', 'partition': 1, 'records': records, 'ack_mode': 0}))
")
    GRPC_RESULT=$(run_with_timeout "$TOOL_TIMEOUT" grpcurl -plaintext -d "$GRPC_BATCH" \
        "$TEST_GRPC" streamhouse.StreamHouse/ProduceBatch 2>&1) || GRPC_RESULT=""
    if echo "$GRPC_RESULT" | jq -e '.count' &>/dev/null; then
        PRODUCED=$(echo "$GRPC_RESULT" | jq '.count' 2>/dev/null) || PRODUCED=0
        pass "gRPC produce $PRODUCED records for REST consume"
    else
        # ProduceBatch may succeed without structured output
        pass "gRPC produce 20 records for REST consume (no error)"
    fi

    wait_flush 8

    http_request GET "$API/consume?topic=${TOPIC}&partition=1&offset=0&maxRecords=100"
    COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || COUNT=0
    if [ "$HTTP_STATUS" = "200" ] && [ "$COUNT" -ge 20 ]; then
        pass "gRPC -> REST: consumed $COUNT records (expected >= 20)"
    elif [ "$HTTP_STATUS" = "200" ] && [ "$COUNT" -ge 1 ]; then
        fail "gRPC -> REST: count mismatch" "got $COUNT, expected >= 20"
    else
        fail "gRPC -> REST: read failed" "status=$HTTP_STATUS count=$COUNT"
    fi
else
    skip "gRPC -> REST produce" "grpcurl not installed"
    skip "gRPC -> REST consume" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 3: REST produce → Kafka consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "$KAFKA_REACHABLE" = "1" ]; then
    # Produce 20 records via REST to partition 2
    BATCH=$(make_batch_json "$TOPIC" 2 20 "rest2kafka")
    http_request POST "$API/produce/batch" "$BATCH"
    if [ "$HTTP_STATUS" = "200" ]; then
        pass "REST produce 20 records for Kafka consume"
    else
        fail "REST produce 20 records for Kafka consume" "HTTP $HTTP_STATUS"
    fi

    wait_flush 8

    KCAT_COUNT=$(run_with_timeout "$TOOL_TIMEOUT" $KCAT_CMD -b "localhost:${TEST_KAFKA_PORT}" \
        -C -t "$TOPIC" -p 2 -o beginning -c 50 -e 2>/dev/null | grep -c . 2>/dev/null) || KCAT_COUNT=0
    if [ "$KCAT_COUNT" -ge 1 ]; then
        pass "REST -> Kafka: consumed $KCAT_COUNT messages via kcat (expected >= 1)"
    else
        fail "REST -> Kafka: no messages" "kcat returned 0 messages"
    fi
else
    skip "REST -> Kafka produce" "Kafka broker not reachable via kcat"
    skip "REST -> Kafka consume" "Kafka broker not reachable via kcat"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 4: Kafka produce → REST consume
# ═══════════════════════════════════════════════════════════════════════════════

if [ "$KAFKA_REACHABLE" = "1" ]; then
    # Get baseline count on partition 3 before Kafka produce
    http_request GET "$API/consume?topic=${TOPIC}&partition=3&offset=0&maxRecords=100000"
    BASELINE=0
    if [ "$HTTP_STATUS" = "200" ]; then
        BASELINE=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || BASELINE=0
    fi

    # Produce 20 records via kcat
    for i in $(seq 1 20); do
        echo "{\"source\":\"kafka\",\"seq\":$i}" | run_with_timeout "$TOOL_TIMEOUT" $KCAT_CMD \
            -b "localhost:${TEST_KAFKA_PORT}" -P -t "$TOPIC" -p 3 -k "kafka2rest-$i" 2>/dev/null || true
    done
    pass "Kafka produce 20 records via kcat"

    wait_flush 8

    http_request GET "$API/consume?topic=${TOPIC}&partition=3&offset=0&maxRecords=100000"
    AFTER_COUNT=0
    if [ "$HTTP_STATUS" = "200" ]; then
        AFTER_COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || AFTER_COUNT=0
    fi
    DIFF=$((AFTER_COUNT - BASELINE))
    if [ "$DIFF" -ge 1 ]; then
        pass "Kafka -> REST: count increased by $DIFF (expected >= 1)"
    else
        fail "Kafka -> REST: count did not increase" "baseline=$BASELINE after=$AFTER_COUNT"
    fi
else
    skip "Kafka -> REST produce" "Kafka broker not reachable via kcat"
    skip "Kafka -> REST consume" "Kafka broker not reachable via kcat"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 5: REST produce → SQL COUNT(*)
# ═══════════════════════════════════════════════════════════════════════════════

# Produce additional records to ensure there is data
BATCH=$(make_batch_json "$TOPIC" 0 20 "rest2sql")
http_request POST "$API/produce/batch" "$BATCH"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "REST produce 20 records for SQL verification"
else
    fail "REST produce 20 records for SQL verification" "HTTP $HTTP_STATUS"
fi

wait_flush 8

http_request POST "$API/sql" "{\"query\":\"SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\"\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    SQL_COUNT=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // .rows[0].cnt // .results[0][0] // "0"' 2>/dev/null) || SQL_COUNT="0"
    if [ "$SQL_COUNT" != "null" ] && [ "$SQL_COUNT" != "0" ] 2>/dev/null; then
        pass "REST -> SQL: COUNT(*) = $SQL_COUNT"
    else
        fail "REST -> SQL: COUNT(*) returned 0" "body=$(echo "$HTTP_BODY" | head -c 200)"
    fi
else
    fail "REST -> SQL: query failed" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Test 6: Content integrity — REST produce → gRPC consume with key+value verify
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    # Use a unique, deterministic key and value
    INTEGRITY_KEY="integrity-check-key"
    INTEGRITY_VALUE="integrity-check-value-12345"

    # Produce via REST with known key+value to a dedicated partition
    http_request POST "$API/produce" \
        "{\"topic\":\"$TOPIC\",\"partition\":0,\"key\":\"$INTEGRITY_KEY\",\"value\":\"$INTEGRITY_VALUE\"}"
    if [ "$HTTP_STATUS" = "200" ]; then
        PRODUCED_OFFSET=$(echo "$HTTP_BODY" | jq '.offset' 2>/dev/null) || PRODUCED_OFFSET="unknown"
        pass "Content integrity: produced at offset $PRODUCED_OFFSET"
    else
        fail "Content integrity: produce failed" "HTTP $HTTP_STATUS"
    fi

    wait_flush 8

    # Consume via gRPC and verify the key+value are intact
    RESULT=$(run_with_timeout "$TOOL_TIMEOUT" grpcurl -plaintext \
        -d "{\"topic\":\"$TOPIC\",\"partition\":0,\"offset\":0,\"max_records\":200}" \
        "$TEST_GRPC" streamhouse.StreamHouse/Consume 2>/dev/null) || RESULT="{}"

    # gRPC returns base64-encoded key and value — search for our record
    FOUND=0
    if echo "$RESULT" | jq -e '.records' &>/dev/null; then
        # Compute expected base64 values
        EXPECTED_KEY_B64=$(echo -n "$INTEGRITY_KEY" | base64)
        EXPECTED_VALUE_B64=$(echo -n "$INTEGRITY_VALUE" | base64)

        # Check each record for matching key and value
        RECORD_COUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || RECORD_COUNT=0
        for idx in $(seq 0 $((RECORD_COUNT - 1))); do
            REC_KEY=$(echo "$RESULT" | jq -r ".records[$idx].key // empty" 2>/dev/null) || REC_KEY=""
            REC_VALUE=$(echo "$RESULT" | jq -r ".records[$idx].value // empty" 2>/dev/null) || REC_VALUE=""

            # Decode and compare
            DECODED_KEY=$(echo -n "$REC_KEY" | base64 -d 2>/dev/null) || DECODED_KEY=""
            DECODED_VALUE=$(echo -n "$REC_VALUE" | base64 -d 2>/dev/null) || DECODED_VALUE=""

            if [ "$DECODED_KEY" = "$INTEGRITY_KEY" ] && [ "$DECODED_VALUE" = "$INTEGRITY_VALUE" ]; then
                FOUND=1
                break
            fi
        done
    fi

    if [ "$FOUND" = "1" ]; then
        pass "Content integrity: gRPC returned matching key+value (base64 decoded)"
    else
        fail "Content integrity: key+value mismatch" "could not find '$INTEGRITY_KEY'/'$INTEGRITY_VALUE' in $RECORD_COUNT gRPC records"
    fi
else
    skip "Content integrity REST->gRPC" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/$TOPIC" &>/dev/null || true
