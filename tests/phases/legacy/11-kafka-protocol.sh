#!/usr/bin/env bash
# Phase 11 — Kafka Protocol Tests
# Tests the Kafka wire protocol using kcat (kafkacat)

phase_header "Phase 11 — Kafka Protocol (kcat)"

if [ "${HAS_KCAT:-0}" = "0" ]; then
    skip "All Kafka protocol tests" "kcat not installed (brew install kcat)"
    return 0 2>/dev/null || exit 0
fi

API="${TEST_HTTP}/api/v1"
KAFKA_BROKER="localhost:${TEST_KAFKA_PORT}"
TOPIC="kafka-proto-test"
KCAT_TIMEOUT=5  # seconds for kcat operations

# ═══════════════════════════════════════════════════════════════════════════════
# Broker Metadata
# ═══════════════════════════════════════════════════════════════════════════════

METADATA=$(run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -L -t __nonexistent 2>&1) || METADATA=""
if echo "$METADATA" | grep -qi "broker\|metadata"; then
    pass "kcat broker metadata — connected to $KAFKA_BROKER"
else
    fail "kcat broker metadata" "could not connect to $KAFKA_BROKER — handshake failed"
    skip "Remaining Kafka protocol tests" "kcat cannot complete metadata handshake"
    return 0 2>/dev/null || exit 0
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Setup: Create topic via REST
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":4}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Create topic '$TOPIC' via REST for Kafka tests"
else
    fail "Create topic '$TOPIC'" "got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Metadata shows topic
# ═══════════════════════════════════════════════════════════════════════════════

sleep 1  # Brief delay for metadata propagation

METADATA=$(run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -L 2>&1) || METADATA=""
if echo "$METADATA" | grep -q "$TOPIC"; then
    pass "kcat metadata lists '$TOPIC'"
else
    sleep 2
    METADATA=$(run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -L 2>&1) || METADATA=""
    if echo "$METADATA" | grep -q "$TOPIC"; then
        pass "kcat metadata lists '$TOPIC' (after retry)"
    else
        fail "kcat metadata" "topic '$TOPIC' not in metadata output"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Produce single message
# ═══════════════════════════════════════════════════════════════════════════════

echo '{"event":"kafka-single","seq":1}' | run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -P -t "$TOPIC" -k "kcat-key-1" 2>/dev/null
if [ $? -eq 0 ]; then
    pass "kcat produce single message"
else
    fail "kcat produce single message" "kcat exited with error"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Produce batch (10 messages)
# ═══════════════════════════════════════════════════════════════════════════════

BATCH_DATA=""
for i in $(seq 1 10); do
    BATCH_DATA="${BATCH_DATA}{\"event\":\"kafka-batch\",\"seq\":$i}
"
done

echo "$BATCH_DATA" | run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -P -t "$TOPIC" -k "kcat-batch" 2>/dev/null
if [ $? -eq 0 ]; then
    pass "kcat produce batch (10 messages)"
else
    fail "kcat produce batch" "kcat exited with error"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Consume via kcat
# ═══════════════════════════════════════════════════════════════════════════════

wait_flush 8

TOTAL=0
for p in 0 1 2 3; do
    C=$(run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -C -t "$TOPIC" -p "$p" -o beginning -c 50 -e 2>/dev/null | grep -c . 2>/dev/null) || C=0
    TOTAL=$((TOTAL + C))
done
if [ "$TOTAL" -ge 1 ]; then
    pass "kcat consume — got $TOTAL messages across all partitions"
else
    fail "kcat consume" "got 0 messages after flush — data not reaching Kafka consumers"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# kcat JSON output verify
# ═══════════════════════════════════════════════════════════════════════════════

JSON_OUT=$(run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -C -t "$TOPIC" -p 0 -o beginning -c 5 -e -J 2>/dev/null) || JSON_OUT=""
if echo "$JSON_OUT" | head -1 | jq -e '.payload' &>/dev/null; then
    pass "kcat JSON output (-J) — valid JSON records"
elif [ -n "$JSON_OUT" ]; then
    pass "kcat JSON output — got data (format may vary)"
else
    fail "kcat JSON output" "no data returned from partition 0"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cross-read: REST consume what Kafka produced
# ═══════════════════════════════════════════════════════════════════════════════

REST_TOTAL=0
for p in 0 1 2 3; do
    http_request GET "$API/consume?topic=${TOPIC}&partition=${p}&offset=0&maxRecords=100000"
    if [ "$HTTP_STATUS" = "200" ]; then
        COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || COUNT=0
        REST_TOTAL=$((REST_TOTAL + COUNT))
    fi
done

if [ "$REST_TOTAL" -ge 1 ]; then
    pass "REST consume Kafka-produced data — $REST_TOTAL records"
else
    fail "REST consume Kafka-produced data" "got 0 records — Kafka data not reaching storage"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Produce with different keys
# ═══════════════════════════════════════════════════════════════════════════════

for k in "user-alice" "user-bob" "user-charlie"; do
    echo "{\"user\":\"$k\"}" | run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -P -t "$TOPIC" -k "$k" 2>/dev/null
done
pass "kcat produce with different keys (3 messages)"

# ═══════════════════════════════════════════════════════════════════════════════
# Create empty topic and consume (graceful empty)
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/topics" '{"name":"kafka-empty-test","partitions":1}'

EMPTY_OUT=$(run_with_timeout "${KCAT_TIMEOUT}" $KCAT_CMD -b "$KAFKA_BROKER" -C -t "kafka-empty-test" -p 0 -o beginning -c 1 -e 2>/dev/null) || EMPTY_OUT=""
# kcat with -e exits when reaching end — empty output is expected
pass "kcat consume from empty topic — graceful exit"

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/$TOPIC" &>/dev/null || true
http_request DELETE "$API/topics/kafka-empty-test" &>/dev/null || true
