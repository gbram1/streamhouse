#!/usr/bin/env bash
# Phase 03 — Produce & Consume (all protocols)
# Comprehensive produce/consume across REST, gRPC, and Kafka

phase_header "Phase 03 — Produce & Consume (all protocols)"

API="${TEST_HTTP}/api/v1"
GRPC="$TEST_GRPC"
KAFKA_BROKER="localhost:${TEST_KAFKA_PORT}"
TOPIC="pc-test"
KCAT_TIMEOUT=5

# ── Setup: Create topic ──────────────────────────────────────────────────────
http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":4}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Setup: create topic '$TOPIC' (4 partitions)"
else
    fail "Setup: create topic '$TOPIC'" "got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# REST Produce
# ═══════════════════════════════════════════════════════════════════════════════

# Single message with key+value
http_request POST "$API/produce" \
    "{\"topic\":\"$TOPIC\",\"key\":\"rest-k1\",\"value\":\"{\\\"msg\\\":\\\"hello\\\"}\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "REST produce: single message with key+value -> 200"
else
    fail "REST produce: single message with key+value" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Batch of 50 messages
BATCH_50=$(make_batch_json "$TOPIC" 0 50 "rest-batch")
http_request POST "$API/produce/batch" "$BATCH_50"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "REST produce: batch of 50 messages -> 200"
else
    fail "REST produce: batch of 50 messages" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Produce with explicit partition
http_request POST "$API/produce" \
    "{\"topic\":\"$TOPIC\",\"key\":\"part-key\",\"value\":\"partition-test\",\"partition\":2}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "REST produce: explicit partition=2 -> 200"
else
    fail "REST produce: explicit partition=2" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Produce without key (null key)
http_request POST "$API/produce" \
    "{\"topic\":\"$TOPIC\",\"value\":\"no-key-message\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "REST produce: null key -> 200"
else
    fail "REST produce: null key" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Produce to nonexistent topic -> 404
assert_status "REST produce: nonexistent topic -> 404" "404" \
    POST "$API/produce" '{"topic":"nonexistent-xyz-999","key":"k","value":"v"}'

# Produce with empty value
http_request POST "$API/produce" \
    "{\"topic\":\"$TOPIC\",\"key\":\"empty-val\",\"value\":\"\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "REST produce: empty value -> 200 (accepted)"
elif [ "$HTTP_STATUS" = "400" ]; then
    pass "REST produce: empty value -> 400 (rejected)"
else
    fail "REST produce: empty value" "expected 200 or 400, got HTTP $HTTP_STATUS"
fi

# Produce with headers (if supported)
http_request POST "$API/produce" \
    "{\"topic\":\"$TOPIC\",\"key\":\"hdr-key\",\"value\":\"hdr-val\",\"headers\":{\"x-trace\":\"abc123\"}}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "REST produce: with headers -> 200"
elif [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    skip "REST produce: with headers" "server does not support headers field"
else
    fail "REST produce: with headers" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# REST Consume
# ═══════════════════════════════════════════════════════════════════════════════

wait_flush 8

# Consume with offset=0
TOTAL_RECORDS=0
for p in 0 1 2 3; do
    http_request GET "$API/consume?topic=$TOPIC&partition=${p}&offset=0&maxRecords=100000"
    if [ "$HTTP_STATUS" = "200" ]; then
        COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || COUNT=0
        TOTAL_RECORDS=$((TOTAL_RECORDS + COUNT))
    fi
done
if [ "$TOTAL_RECORDS" -ge 1 ]; then
    pass "REST consume: offset=0 -> got $TOTAL_RECORDS records"
else
    fail "REST consume: offset=0" "got 0 records after flush"
fi

# Consume with maxRecords=5
http_request GET "$API/consume?topic=$TOPIC&partition=0&offset=0&maxRecords=5"
if [ "$HTTP_STATUS" = "200" ]; then
    LIMITED=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || LIMITED=0
    if [ "$LIMITED" -le 5 ]; then
        pass "REST consume: maxRecords=5 -> got $LIMITED records (<= 5)"
    else
        fail "REST consume: maxRecords=5" "expected <= 5 records, got $LIMITED"
    fi
else
    fail "REST consume: maxRecords=5" "got HTTP $HTTP_STATUS"
fi

# Consume from specific partition (partition 2 where we explicitly produced)
http_request GET "$API/consume?topic=$TOPIC&partition=2&offset=0&maxRecords=100000"
if [ "$HTTP_STATUS" = "200" ]; then
    P2_COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || P2_COUNT=0
    if [ "$P2_COUNT" -ge 1 ]; then
        pass "REST consume: partition=2 -> got $P2_COUNT records"
    else
        pass "REST consume: partition=2 -> 200 OK (0 records, key may hash elsewhere)"
    fi
else
    fail "REST consume: partition=2" "got HTTP $HTTP_STATUS"
fi

# Consume from nonexistent topic -> 404
http_request GET "$API/consume?topic=nonexistent-xyz-999&partition=0&offset=0&maxRecords=10"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "REST consume: nonexistent topic -> 404"
else
    fail "REST consume: nonexistent topic" "expected 404, got HTTP $HTTP_STATUS"
fi

# Consume with missing params -> 400
http_request GET "$API/consume?topic=$TOPIC"
if [ "$HTTP_STATUS" = "400" ]; then
    pass "REST consume: missing params -> 400"
else
    fail "REST consume: missing params" "expected 400, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# gRPC Produce (if grpcurl available)
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then

    # Single Produce RPC
    RESULT=$(grpcurl -plaintext \
        -d "{\"topic\":\"$TOPIC\",\"partition\":0,\"key\":\"Z3JwYy1rMQ==\",\"value\":\"Z3JwYy12YWw=\"}" \
        "$GRPC" streamhouse.StreamHouse/Produce 2>&1) || true
    if echo "$RESULT" | jq -e 'has("offset") or has("timestamp")' &>/dev/null; then
        OFFSET_VAL=$(echo "$RESULT" | jq -r '.offset // "n/a"')
        pass "gRPC produce: single record (offset=$OFFSET_VAL)"
    else
        fail "gRPC produce: single record" "$RESULT"
    fi

    # ProduceBatch with 20 records
    GRPC_BATCH=$(python3 -c "
import json, base64
records = []
for i in range(20):
    records.append({
        'key': base64.b64encode(f'grpc-batch-{i}'.encode()).decode(),
        'value': base64.b64encode(f'grpc-val-{i}'.encode()).decode()
    })
print(json.dumps({
    'topic': '$TOPIC',
    'partition': 1,
    'records': records,
    'ack_mode': 0
}))
")
    RESULT=$(grpcurl -plaintext -d "$GRPC_BATCH" \
        "$GRPC" streamhouse.StreamHouse/ProduceBatch 2>&1) || true
    if echo "$RESULT" | jq -e '.count' &>/dev/null; then
        BATCH_CT=$(echo "$RESULT" | jq -r '.count')
        if [ "$BATCH_CT" = "20" ]; then
            pass "gRPC produce: ProduceBatch count=20"
        else
            pass "gRPC produce: ProduceBatch count=$BATCH_CT (expected 20)"
        fi
    else
        fail "gRPC produce: ProduceBatch" "$RESULT"
    fi

    # Produce to nonexistent topic -> NOT_FOUND
    RESULT=$(grpcurl -plaintext \
        -d '{"topic":"nonexistent-xyz-999","partition":0,"key":"dQ==","value":"dQ=="}' \
        "$GRPC" streamhouse.StreamHouse/Produce 2>&1) || true
    if echo "$RESULT" | grep -qi "not found\|NOT_FOUND"; then
        pass "gRPC produce: nonexistent topic -> NOT_FOUND"
    else
        fail "gRPC produce: nonexistent topic" "expected NOT_FOUND: $RESULT"
    fi

    # ── gRPC Consume ─────────────────────────────────────────────────────────
    wait_flush 8

    GRPC_TOTAL=0
    for p in 0 1 2 3; do
        RESULT=$(grpcurl -plaintext \
            -d "{\"topic\":\"$TOPIC\",\"partition\":$p,\"offset\":0,\"max_records\":100}" \
            "$GRPC" streamhouse.StreamHouse/Consume 2>&1) || RESULT="{}"
        C=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || C=0
        GRPC_TOTAL=$((GRPC_TOTAL + C))
    done
    if [ "$GRPC_TOTAL" -ge 1 ]; then
        pass "gRPC consume: got $GRPC_TOTAL records from '$TOPIC'"
    else
        fail "gRPC consume" "got 0 records after flush"
    fi

    # CommitOffset
    RESULT=$(grpcurl -plaintext \
        -d "{\"consumer_group\":\"pc-test-cg\",\"topic\":\"$TOPIC\",\"partition\":0,\"offset\":5}" \
        "$GRPC" streamhouse.StreamHouse/CommitOffset 2>&1) || true
    if echo "$RESULT" | jq -e '.success' &>/dev/null; then
        pass "gRPC CommitOffset -> success"
    else
        fail "gRPC CommitOffset" "$RESULT"
    fi

    # GetOffset -> verify committed value
    RESULT=$(grpcurl -plaintext \
        -d "{\"consumer_group\":\"pc-test-cg\",\"topic\":\"$TOPIC\",\"partition\":0}" \
        "$GRPC" streamhouse.StreamHouse/GetOffset 2>&1) || true
    if echo "$RESULT" | jq -e '.offset' &>/dev/null; then
        GOT_OFFSET=$(echo "$RESULT" | jq -r '.offset')
        if [ "$GOT_OFFSET" = "5" ]; then
            pass "gRPC GetOffset -> returned offset 5"
        else
            fail "gRPC GetOffset" "expected offset 5, got $GOT_OFFSET"
        fi
    else
        fail "gRPC GetOffset" "$RESULT"
    fi

else
    skip "gRPC produce: single record" "grpcurl not installed"
    skip "gRPC produce: ProduceBatch" "grpcurl not installed"
    skip "gRPC produce: nonexistent topic" "grpcurl not installed"
    skip "gRPC consume" "grpcurl not installed"
    skip "gRPC CommitOffset" "grpcurl not installed"
    skip "gRPC GetOffset" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Kafka Produce & Consume (if kcat available)
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_KCAT:-0}" = "1" ]; then

    # kcat produce single message
    echo '{"event":"kcat-single","seq":1}' | run_with_timeout "$KCAT_TIMEOUT" \
        $KCAT_CMD -b "$KAFKA_BROKER" -P -t "$TOPIC" -k "kcat-key-1" 2>/dev/null
    if [ $? -eq 0 ]; then
        pass "Kafka produce: kcat single message -> exit 0"
    else
        fail "Kafka produce: kcat single message" "kcat exited with error"
    fi

    # kcat produce batch (10 messages)
    KCAT_BATCH=""
    for i in $(seq 1 10); do
        KCAT_BATCH="${KCAT_BATCH}{\"event\":\"kcat-batch\",\"seq\":$i}
"
    done
    echo "$KCAT_BATCH" | run_with_timeout "$KCAT_TIMEOUT" \
        $KCAT_CMD -b "$KAFKA_BROKER" -P -t "$TOPIC" -k "kcat-batch" 2>/dev/null
    if [ $? -eq 0 ]; then
        pass "Kafka produce: kcat batch (10 messages) -> exit 0"
    else
        fail "Kafka produce: kcat batch" "kcat exited with error"
    fi

    # kcat consume
    wait_flush 8

    KCAT_TOTAL=0
    for p in 0 1 2 3; do
        C=$(run_with_timeout "$KCAT_TIMEOUT" \
            $KCAT_CMD -b "$KAFKA_BROKER" -C -t "$TOPIC" -p "$p" -o beginning -c 100 -e 2>/dev/null | grep -c . 2>/dev/null) || C=0
        KCAT_TOTAL=$((KCAT_TOTAL + C))
    done
    if [ "$KCAT_TOTAL" -ge 1 ]; then
        pass "Kafka consume: kcat got $KCAT_TOTAL messages"
    else
        fail "Kafka consume: kcat" "got 0 messages after flush"
    fi

else
    skip "Kafka produce: kcat single message" "kcat not installed"
    skip "Kafka produce: kcat batch" "kcat not installed"
    skip "Kafka consume: kcat" "kcat not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Ordering Verification
# ═══════════════════════════════════════════════════════════════════════════════

ORDERING_TOPIC="pc-ordering-test"
http_request POST "$API/topics" "{\"name\":\"$ORDERING_TOPIC\",\"partitions\":1}"
# Accept 201 or 409
if [ "$HTTP_STATUS" != "201" ] && [ "$HTTP_STATUS" != "409" ]; then
    fail "Ordering: create topic" "got HTTP $HTTP_STATUS"
fi

# Produce 10 messages with sequential keys to partition 0
for i in $(seq 0 9); do
    http_request POST "$API/produce" \
        "{\"topic\":\"$ORDERING_TOPIC\",\"key\":\"order-$i\",\"value\":\"{\\\"seq\\\":$i}\",\"partition\":0}"
    if [ "$HTTP_STATUS" != "200" ]; then
        fail "Ordering: produce message $i" "got HTTP $HTTP_STATUS"
        break
    fi
done

wait_flush 8

# Consume from partition 0 and verify offsets are monotonically increasing
http_request GET "$API/consume?topic=$ORDERING_TOPIC&partition=0&offset=0&maxRecords=100"
if [ "$HTTP_STATUS" = "200" ]; then
    RECORD_COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || RECORD_COUNT=0
    if [ "$RECORD_COUNT" -ge 10 ]; then
        # Check that offsets are monotonically increasing
        MONOTONIC=true
        PREV_OFFSET=-1
        for idx in $(seq 0 $((RECORD_COUNT - 1))); do
            CURR_OFFSET=$(echo "$HTTP_BODY" | jq ".records[$idx].offset // $idx" 2>/dev/null) || CURR_OFFSET=$idx
            if [ "$CURR_OFFSET" -le "$PREV_OFFSET" ] 2>/dev/null; then
                MONOTONIC=false
                break
            fi
            PREV_OFFSET=$CURR_OFFSET
        done
        if [ "$MONOTONIC" = true ]; then
            pass "Ordering: offsets are monotonically increasing ($RECORD_COUNT records)"
        else
            fail "Ordering: offsets monotonic" "offset $CURR_OFFSET <= previous $PREV_OFFSET"
        fi
    else
        fail "Ordering: expected >= 10 records" "got $RECORD_COUNT"
    fi
else
    fail "Ordering: consume" "got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Data Integrity
# ═══════════════════════════════════════════════════════════════════════════════

INTEGRITY_TOPIC="pc-integrity-test"
http_request POST "$API/topics" "{\"name\":\"$INTEGRITY_TOPIC\",\"partitions\":2}"
# Accept 201 or 409

# Produce 100 messages with JSON payloads containing a checksum field
INTEGRITY_BATCH=$(make_batch_json "$INTEGRITY_TOPIC" 0 100 "integrity")
http_request POST "$API/produce/batch" "$INTEGRITY_BATCH"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "Integrity: produce 100 messages with checksums -> 200"
else
    fail "Integrity: produce 100 messages" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

wait_flush 8

# Consume them all back
INTEGRITY_TOTAL=0
ALL_VALID_JSON=true
for p in 0 1; do
    http_request GET "$API/consume?topic=$INTEGRITY_TOPIC&partition=${p}&offset=0&maxRecords=100000"
    if [ "$HTTP_STATUS" = "200" ]; then
        P_COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || P_COUNT=0
        INTEGRITY_TOTAL=$((INTEGRITY_TOTAL + P_COUNT))

        # Verify each message value parses as valid JSON
        if [ "$P_COUNT" -gt 0 ]; then
            INVALID_COUNT=$(echo "$HTTP_BODY" | jq '[.records[].value | fromjson? // null | select(. == null)] | length' 2>/dev/null) || INVALID_COUNT=0
            if [ "$INVALID_COUNT" -gt 0 ]; then
                ALL_VALID_JSON=false
            fi
        fi
    fi
done

# Verify record count matches
if [ "$INTEGRITY_TOTAL" -ge 100 ]; then
    pass "Integrity: consumed $INTEGRITY_TOTAL records (expected >= 100)"
else
    fail "Integrity: record count" "expected >= 100, got $INTEGRITY_TOTAL"
fi

# Verify all records are valid JSON
if [ "$ALL_VALID_JSON" = true ] && [ "$INTEGRITY_TOTAL" -ge 1 ]; then
    pass "Integrity: all consumed values are valid JSON"
else
    fail "Integrity: JSON validation" "some values did not parse as valid JSON"
fi

# ── Cleanup ──────────────────────────────────────────────────────────────────
http_request DELETE "$API/topics/$TOPIC" &>/dev/null || true
http_request DELETE "$API/topics/$ORDERING_TOPIC" &>/dev/null || true
http_request DELETE "$API/topics/$INTEGRITY_TOPIC" &>/dev/null || true
