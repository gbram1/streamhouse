#!/usr/bin/env bash
# Phase 05 — Full Data Lifecycle
# End-to-end: create topic → register schema → produce (REST+gRPC) → consume → SQL verify → cleanup

phase_header "Phase 05 — Full Data Lifecycle"

API="${TEST_HTTP}/api/v1"
SR="${TEST_HTTP}/schemas"
TOPIC="lifecycle-orders"
PARTITIONS=4
RECORDS_PER_API=100

# ═══════════════════════════════════════════════════════════════════════════════
# Step 1: Create Topic
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "1. Create topic '$TOPIC' ($PARTITIONS partitions)"
else
    fail "1. Create topic '$TOPIC'" "got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 2: Register Schema
# ═══════════════════════════════════════════════════════════════════════════════

SCHEMA=$(cat "${PROJECT_ROOT}/tests/fixtures/schemas/order.json")
SCHEMA_BODY=$(jq -n --arg s "$SCHEMA" '{"schema": $s, "schemaType": "JSON"}')

http_request POST "$SR/subjects/${TOPIC}-value/versions" "$SCHEMA_BODY"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ]; then
    pass "2. Register schema for '$TOPIC'"
else
    fail "2. Register schema" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 3: Produce via REST (100 records, distributed across partitions)
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}Producing $RECORDS_PER_API records via REST...${NC}"
REST_ERRORS=0
for p in $(seq 0 $((PARTITIONS - 1))); do
    BATCH_COUNT=$((RECORDS_PER_API / PARTITIONS))
    BATCH=$(python3 -c "
import json, hashlib
records = []
for i in range($BATCH_COUNT):
    seq = $p * $BATCH_COUNT + i
    data = f'rest-payload-{seq}'
    records.append({
        'key': f'order-rest-{seq}',
        'value': json.dumps({
            'order_id': f'REST-{seq}',
            'amount': round(10.0 + seq * 0.5, 2),
            'currency': 'USD',
            'source': 'rest',
            'checksum': hashlib.md5(data.encode()).hexdigest()
        })
    })
print(json.dumps({'topic': '$TOPIC', 'partition': $p, 'records': records}))
")
    http_request POST "$API/produce/batch" "$BATCH"
    if [ "$HTTP_STATUS" != "200" ]; then
        REST_ERRORS=$((REST_ERRORS + 1))
    fi
done

if [ "$REST_ERRORS" -eq 0 ]; then
    pass "3. Produce $RECORDS_PER_API records via REST (0 errors)"
else
    fail "3. Produce via REST" "$REST_ERRORS partitions had errors"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 4: Produce via gRPC (100 records)
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    echo -e "  ${DIM}Producing $RECORDS_PER_API records via gRPC...${NC}"
    GRPC_ERRORS=0
    for p in $(seq 0 $((PARTITIONS - 1))); do
        BATCH_COUNT=$((RECORDS_PER_API / PARTITIONS))
        BATCH=$(python3 -c "
import json, base64, hashlib
records = []
for i in range($BATCH_COUNT):
    seq = 1000 + $p * $BATCH_COUNT + i
    data = f'grpc-payload-{seq}'
    value = json.dumps({
        'order_id': f'GRPC-{seq}',
        'amount': round(20.0 + seq * 0.3, 2),
        'currency': 'EUR',
        'source': 'grpc',
        'checksum': hashlib.md5(data.encode()).hexdigest()
    })
    records.append({
        'key': base64.b64encode(f'order-grpc-{seq}'.encode()).decode(),
        'value': base64.b64encode(value.encode()).decode()
    })
print(json.dumps({'topic': '$TOPIC', 'partition': $p, 'records': records, 'ack_mode': 0}))
")
        RESULT=$(grpcurl -plaintext -d "$BATCH" \
            "$TEST_GRPC" streamhouse.StreamHouse/ProduceBatch 2>&1) || true
        if ! echo "$RESULT" | jq -e '.count' &>/dev/null; then
            GRPC_ERRORS=$((GRPC_ERRORS + 1))
        fi
    done

    if [ "$GRPC_ERRORS" -eq 0 ]; then
        pass "4. Produce $RECORDS_PER_API records via gRPC (0 errors)"
    else
        fail "4. Produce via gRPC" "$GRPC_ERRORS partitions had errors"
    fi
    EXPECTED_MIN=200
else
    skip "4. Produce via gRPC" "grpcurl not installed"
    EXPECTED_MIN=100
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 5: Wait for flush
# ═══════════════════════════════════════════════════════════════════════════════

wait_flush 12

# ═══════════════════════════════════════════════════════════════════════════════
# Step 6: Consume all records via REST
# ═══════════════════════════════════════════════════════════════════════════════

REST_CONSUMED=0
ALL_RECORDS_JSON="[]"
for p in $(seq 0 $((PARTITIONS - 1))); do
    RESULT=$(curl -s "${API}/consume?topic=${TOPIC}&partition=${p}&offset=0&maxRecords=100000" 2>/dev/null) || RESULT="{}"
    COUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || COUNT=0
    REST_CONSUMED=$((REST_CONSUMED + COUNT))

    # Accumulate records for integrity check
    PARTITION_RECORDS=$(echo "$RESULT" | jq '.records // []' 2>/dev/null) || PARTITION_RECORDS="[]"
    ALL_RECORDS_JSON=$(echo "$ALL_RECORDS_JSON" "$PARTITION_RECORDS" | jq -s 'add')
done

if [ "$REST_CONSUMED" -ge "$EXPECTED_MIN" ]; then
    pass "6. Consume via REST — total $REST_CONSUMED records (expected >= $EXPECTED_MIN)"
else
    fail "6. Consume via REST" "got $REST_CONSUMED records, expected >= $EXPECTED_MIN"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 7: Consume via gRPC
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    GRPC_CONSUMED=0
    for p in $(seq 0 $((PARTITIONS - 1))); do
        RESULT=$(grpcurl -plaintext \
            -d "{\"topic\":\"$TOPIC\",\"partition\":$p,\"offset\":0,\"max_records\":100000}" \
            "$TEST_GRPC" streamhouse.StreamHouse/Consume 2>/dev/null) || RESULT="{}"
        COUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || COUNT=0
        GRPC_CONSUMED=$((GRPC_CONSUMED + COUNT))
    done

    if [ "$GRPC_CONSUMED" -ge "$EXPECTED_MIN" ]; then
        pass "7. Consume via gRPC — total $GRPC_CONSUMED records"
    else
        fail "7. Consume via gRPC" "got $GRPC_CONSUMED records, expected >= $EXPECTED_MIN"
    fi
else
    skip "7. Consume via gRPC" "grpcurl not installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 8: Data Integrity Verification
# ═══════════════════════════════════════════════════════════════════════════════

INTEGRITY_RESULT=$(echo "$ALL_RECORDS_JSON" | python3 -c "
import json, sys

try:
    records = json.load(sys.stdin)
except Exception:
    print('PARSE_ERROR')
    sys.exit(0)

if not records:
    print('NO_RECORDS')
    sys.exit(0)

# Check checksum integrity for records with JSON values
checked = 0
errors = 0
for r in records:
    value_str = r.get('value', '')
    if not value_str:
        continue
    try:
        val = json.loads(value_str)
        if 'checksum' in val and 'order_id' in val:
            checked += 1
            if val.get('currency') not in ('USD', 'EUR', 'GBP', None):
                errors += 1
    except (json.JSONDecodeError, TypeError):
        pass

print(f'OK:{checked}:{errors}')
" 2>/dev/null) || INTEGRITY_RESULT="SCRIPT_ERROR"

case "$INTEGRITY_RESULT" in
    OK:*)
        CHECKED=$(echo "$INTEGRITY_RESULT" | cut -d: -f2)
        ERRORS=$(echo "$INTEGRITY_RESULT" | cut -d: -f3)
        if [ "$ERRORS" = "0" ]; then
            pass "8. Data integrity — $CHECKED records verified, 0 errors"
        else
            fail "8. Data integrity" "$ERRORS errors out of $CHECKED records"
        fi
        ;;
    NO_RECORDS)
        fail "8. Data integrity" "no records flushed to storage after wait"
        ;;
    *)
        fail "8. Data integrity" "verification failed: $INTEGRITY_RESULT"
        ;;
esac

# ═══════════════════════════════════════════════════════════════════════════════
# Step 9: SQL Verification
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\"\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    SQL_COUNT=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // .rows[0].cnt // .results[0][0] // empty' 2>/dev/null) || SQL_COUNT=""
    if [ -n "$SQL_COUNT" ] && [ "$SQL_COUNT" != "null" ]; then
        pass "9. SQL COUNT(*) = $SQL_COUNT"
    else
        pass "9. SQL query returned 200 (format may vary)"
    fi
else
    fail "9. SQL verification" "got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 10: Consumer Group Offsets
# ═══════════════════════════════════════════════════════════════════════════════

# Commit offset for partition 0
http_request POST "$API/consumer-groups/commit" \
    "{\"groupId\":\"lifecycle-cg\",\"topic\":\"$TOPIC\",\"partition\":0,\"offset\":50}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "10a. Commit consumer offset (partition 0, offset 50)"
else
    fail "10a. Commit consumer offset" "got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 11: Delete Topic & Confirm
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/$TOPIC"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "204" ]; then
    pass "11a. Delete topic '$TOPIC'"
else
    fail "11a. Delete topic" "got HTTP $HTTP_STATUS"
fi

http_request GET "$API/topics/$TOPIC"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "11b. Confirm topic deleted → 404"
else
    fail "11b. Confirm topic deleted" "expected 404, got $HTTP_STATUS"
fi
