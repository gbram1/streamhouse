#!/usr/bin/env bash
# Phase 03 — gRPC API Surface Tests
# Uses grpcurl with server reflection to test all gRPC RPCs

phase_header "Phase 03 — gRPC API Surface"

if [ "${HAS_GRPCURL:-0}" = "0" ]; then
    skip "All gRPC tests" "grpcurl not installed (brew install grpcurl)"
    return 0 2>/dev/null || exit 0
fi

GRPC="$TEST_GRPC"

# ── Reflection / Service Discovery ────────────────────────────────────────────
SERVICES=$(grpcurl -plaintext "$GRPC" list 2>/dev/null) || SERVICES=""
if echo "$SERVICES" | grep -q "streamhouse.StreamHouse"; then
    pass "gRPC reflection — streamhouse.StreamHouse service found"
else
    fail "gRPC reflection" "StreamHouse service not found in: $SERVICES"
    skip "Remaining gRPC tests" "reflection failed"
    return 0 2>/dev/null || exit 0
fi

# ── CreateTopic ───────────────────────────────────────────────────────────────
RESULT=$(grpcurl -plaintext -d '{"name":"grpc-t1","partition_count":4}' \
    "$GRPC" streamhouse.StreamHouse/CreateTopic 2>&1) || true
if echo "$RESULT" | jq -e '.topicId // .topic_id' &>/dev/null; then
    pass "CreateTopic — created 'grpc-t1'"
elif echo "$RESULT" | grep -qi "already exists"; then
    pass "CreateTopic — 'grpc-t1' already exists"
else
    fail "CreateTopic" "$RESULT"
fi

# ── GetTopic ──────────────────────────────────────────────────────────────────
RESULT=$(grpcurl -plaintext -d '{"name":"grpc-t1"}' \
    "$GRPC" streamhouse.StreamHouse/GetTopic 2>&1) || true
if echo "$RESULT" | jq -e '.topic.name' &>/dev/null; then
    pass "GetTopic — retrieved 'grpc-t1'"
else
    fail "GetTopic" "$RESULT"
fi

# ── ListTopics ────────────────────────────────────────────────────────────────
RESULT=$(grpcurl -plaintext -d '{}' \
    "$GRPC" streamhouse.StreamHouse/ListTopics 2>&1) || true
if echo "$RESULT" | grep -q "grpc-t1"; then
    pass "ListTopics — contains 'grpc-t1'"
else
    fail "ListTopics" "grpc-t1 not in list"
fi

# ── Produce (single) ─────────────────────────────────────────────────────────
RESULT=$(grpcurl -plaintext \
    -d '{"topic":"grpc-t1","partition":0,"key":"dXNlcg==","value":"dGVzdA=="}' \
    "$GRPC" streamhouse.StreamHouse/Produce 2>&1) || true
if echo "$RESULT" | jq -e 'has("offset")' &>/dev/null; then
    OFFSET_VAL=$(echo "$RESULT" | jq -r '.offset')
    pass "Produce — single record to grpc-t1:0 (offset=$OFFSET_VAL)"
elif echo "$RESULT" | jq -e 'has("timestamp")' &>/dev/null; then
    pass "Produce — single record to grpc-t1:0 (got response with timestamp)"
else
    fail "Produce — single record" "$RESULT"
fi

# ── ProduceBatch ──────────────────────────────────────────────────────────────
BATCH_DATA=$(python3 -c "
import json, base64
records = []
for i in range(10):
    records.append({
        'key': base64.b64encode(f'k{i}'.encode()).decode(),
        'value': base64.b64encode(f'val-{i}'.encode()).decode()
    })
print(json.dumps({
    'topic': 'grpc-t1',
    'partition': 1,
    'records': records,
    'ack_mode': 0
}))
")
RESULT=$(grpcurl -plaintext -d "$BATCH_DATA" \
    "$GRPC" streamhouse.StreamHouse/ProduceBatch 2>&1) || true
if echo "$RESULT" | jq -e '.count' &>/dev/null; then
    COUNT=$(echo "$RESULT" | jq -r '.count')
    pass "ProduceBatch — $COUNT records to grpc-t1:1"
else
    fail "ProduceBatch" "$RESULT"
fi

# ── Consume ───────────────────────────────────────────────────────────────────
wait_flush 8

GRPC_CONSUME_TOTAL=0
for p in 0 1 2 3; do
    RESULT=$(grpcurl -plaintext \
        -d "{\"topic\":\"grpc-t1\",\"partition\":$p,\"offset\":0,\"max_records\":100}" \
        "$GRPC" streamhouse.StreamHouse/Consume 2>&1) || RESULT="{}"
    C=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || C=0
    GRPC_CONSUME_TOTAL=$((GRPC_CONSUME_TOTAL + C))
done
if [ "$GRPC_CONSUME_TOTAL" -ge 1 ]; then
    pass "Consume — got $GRPC_CONSUME_TOTAL records from grpc-t1"
else
    fail "Consume" "got 0 records after flush — data not reaching storage"
fi

# ── CommitOffset ──────────────────────────────────────────────────────────────
RESULT=$(grpcurl -plaintext \
    -d '{"consumer_group":"grpc-cg","topic":"grpc-t1","partition":0,"offset":10}' \
    "$GRPC" streamhouse.StreamHouse/CommitOffset 2>&1) || true
if echo "$RESULT" | jq -e '.success' &>/dev/null; then
    pass "CommitOffset — committed offset 10"
else
    fail "CommitOffset" "$RESULT"
fi

# ── GetOffset ─────────────────────────────────────────────────────────────────
RESULT=$(grpcurl -plaintext \
    -d '{"consumer_group":"grpc-cg","topic":"grpc-t1","partition":0}' \
    "$GRPC" streamhouse.StreamHouse/GetOffset 2>&1) || true
if echo "$RESULT" | jq -e '.offset' &>/dev/null; then
    OFFSET=$(echo "$RESULT" | jq -r '.offset')
    if [ "$OFFSET" = "10" ]; then
        pass "GetOffset — returned offset 10"
    else
        fail "GetOffset" "expected 10, got $OFFSET"
    fi
else
    fail "GetOffset" "$RESULT"
fi

# ── Error Cases ───────────────────────────────────────────────────────────────

# Produce to nonexistent topic
RESULT=$(grpcurl -plaintext \
    -d '{"topic":"grpc-nonexistent","partition":0,"key":"dQ==","value":"dQ=="}' \
    "$GRPC" streamhouse.StreamHouse/Produce 2>&1) || true
if echo "$RESULT" | grep -qi "not found\|NOT_FOUND"; then
    pass "Produce to nonexistent topic → NOT_FOUND"
else
    fail "Produce to nonexistent topic" "expected NOT_FOUND: $RESULT"
fi

# Produce to invalid partition
RESULT=$(grpcurl -plaintext \
    -d '{"topic":"grpc-t1","partition":999,"key":"dQ==","value":"dQ=="}' \
    "$GRPC" streamhouse.StreamHouse/Produce 2>&1) || true
if echo "$RESULT" | grep -qi "invalid\|not found\|INVALID_ARGUMENT\|NOT_FOUND"; then
    pass "Produce to invalid partition → error"
else
    fail "Produce to invalid partition" "expected error: $RESULT"
fi

# GetTopic for nonexistent
RESULT=$(grpcurl -plaintext -d '{"name":"grpc-does-not-exist"}' \
    "$GRPC" streamhouse.StreamHouse/GetTopic 2>&1) || true
if echo "$RESULT" | grep -qi "not found\|NOT_FOUND"; then
    pass "GetTopic nonexistent → NOT_FOUND"
else
    fail "GetTopic nonexistent" "expected NOT_FOUND: $RESULT"
fi

# ── Cleanup ───────────────────────────────────────────────────────────────────
grpcurl -plaintext -d '{"name":"grpc-t1"}' \
    "$GRPC" streamhouse.StreamHouse/DeleteTopic &>/dev/null || true
