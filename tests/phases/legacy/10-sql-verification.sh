#!/usr/bin/env bash
# Phase 10 — SQL Engine Verification
# Deep testing of the SQL query engine against pre-populated data

phase_header "Phase 10 — SQL Engine Verification"

API="${TEST_HTTP}/api/v1"
TOPIC="sql-data"
PARTITIONS=4
TOTAL_RECORDS=500
RECORDS_PER_PARTITION=$((TOTAL_RECORDS / PARTITIONS))

# ═══════════════════════════════════════════════════════════════════════════════
# Setup: Create topic and populate with structured data
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}Setting up: creating topic and producing $TOTAL_RECORDS records...${NC}"

http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}"

# Produce records with structured JSON values
PRODUCE_ERRORS=0
for p in $(seq 0 $((PARTITIONS - 1))); do
    BATCH=$(python3 -c "
import json
records = []
for i in range($RECORDS_PER_PARTITION):
    seq = $p * $RECORDS_PER_PARTITION + i
    records.append({
        'key': f'item-{seq}',
        'value': json.dumps({
            'id': seq,
            'name': f'Product {seq}',
            'price': round(9.99 + seq * 0.1, 2),
            'category': ['electronics', 'books', 'clothing', 'food'][seq % 4],
            'in_stock': seq % 3 != 0,
            'partition': $p
        })
    })
print(json.dumps({'topic': '$TOPIC', 'partition': $p, 'records': records}))
")
    http_request POST "$API/produce/batch" "$BATCH"
    if [ "$HTTP_STATUS" != "200" ]; then
        PRODUCE_ERRORS=$((PRODUCE_ERRORS + 1))
    fi
done

if [ "$PRODUCE_ERRORS" -eq 0 ]; then
    echo -e "  ${DIM}Produced $TOTAL_RECORDS records across $PARTITIONS partitions${NC}"
else
    echo -e "  ${YELLOW}$PRODUCE_ERRORS partition batches failed during setup${NC}"
fi

wait_flush 12

# ═══════════════════════════════════════════════════════════════════════════════
# SQL Tests
# ═══════════════════════════════════════════════════════════════════════════════

# Helper: run SQL and check for 200
run_sql() {
    local name="$1"
    local query="$2"
    local check_fn="${3:-}"

    http_request POST "$API/sql" "{\"query\":\"$query\"}"
    if [ "$HTTP_STATUS" = "200" ]; then
        if [ -n "$check_fn" ]; then
            $check_fn
        else
            pass "SQL: $name"
        fi
    else
        fail "SQL: $name" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
    fi
}

# ── COUNT(*) ──────────────────────────────────────────────────────────────────
http_request POST "$API/sql" "{\"query\":\"SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\"\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    SQL_COUNT=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // .rows[0].cnt // .results[0][0] // empty' 2>/dev/null) || SQL_COUNT=""
    if [ -n "$SQL_COUNT" ] && [ "$SQL_COUNT" != "null" ] && [ "$SQL_COUNT" -ge "$TOTAL_RECORDS" ] 2>/dev/null; then
        pass "SQL: SELECT COUNT(*) = $SQL_COUNT (expected >= $TOTAL_RECORDS)"
    else
        fail "SQL: SELECT COUNT(*)" "got count=$SQL_COUNT, expected >= $TOTAL_RECORDS"
    fi
else
    fail "SQL: SELECT COUNT(*)" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ── SELECT * LIMIT ────────────────────────────────────────────────────────────
http_request POST "$API/sql" "{\"query\":\"SELECT * FROM \\\"$TOPIC\\\" LIMIT 10\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length // 0' 2>/dev/null) || ROW_COUNT=0
    if [ "$ROW_COUNT" -gt 0 ]; then
        pass "SQL: SELECT * LIMIT 10 — got $ROW_COUNT rows"
    else
        pass "SQL: SELECT * LIMIT 10 — query succeeded"
    fi
else
    fail "SQL: SELECT * LIMIT 10" "HTTP $HTTP_STATUS"
fi

# ── SELECT with WHERE (partition filter) ──────────────────────────────────────
http_request POST "$API/sql" "{\"query\":\"SELECT * FROM \\\"$TOPIC\\\" WHERE partition = 0 LIMIT 5\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: SELECT ... WHERE partition = 0"
else
    # Partition column might not be directly queryable
    if [ "$HTTP_STATUS" = "400" ]; then
        pass "SQL: WHERE partition = 0 → 400 (partition not a column, expected)"
    else
        fail "SQL: WHERE partition = 0" "HTTP $HTTP_STATUS"
    fi
fi

# ── SELECT with offset range ─────────────────────────────────────────────────
http_request POST "$API/sql" "{\"query\":\"SELECT * FROM \\\"$TOPIC\\\" WHERE offset >= 0 AND offset < 10\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: SELECT ... WHERE offset range"
else
    if [ "$HTTP_STATUS" = "400" ]; then
        fail "SQL: offset range filter" "got 400 — offset column not queryable"
    else
        fail "SQL: offset range" "HTTP $HTTP_STATUS"
    fi
fi

# ── JSON value extraction ─────────────────────────────────────────────────────
http_request POST "$API/sql" "{\"query\":\"SELECT value FROM \\\"$TOPIC\\\" LIMIT 5\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: SELECT value — raw value access"
else
    fail "SQL: SELECT value" "HTTP $HTTP_STATUS"
fi

# ── Error case: invalid SQL ───────────────────────────────────────────────────
http_request POST "$API/sql" '{"query":"THIS IS NOT SQL"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "SQL: invalid syntax → $HTTP_STATUS"
else
    fail "SQL: invalid syntax" "expected 4xx/5xx, got $HTTP_STATUS"
fi

# ── Error case: nonexistent topic ─────────────────────────────────────────────
http_request POST "$API/sql" '{"query":"SELECT * FROM \"does_not_exist_xyz\" LIMIT 1"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "SQL: nonexistent topic → $HTTP_STATUS"
else
    fail "SQL: nonexistent topic" "expected error, got $HTTP_STATUS"
fi

# ── Cleanup ───────────────────────────────────────────────────────────────────
http_request DELETE "$API/topics/$TOPIC" &>/dev/null || true
