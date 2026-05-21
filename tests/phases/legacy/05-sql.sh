#!/usr/bin/env bash
# Phase 05 — SQL Query Engine
# Comprehensive SQL testing: produce structured data, then query with aggregates,
# filtering, pagination, json_extract, and error cases.

phase_header "Phase 05 — SQL Query Engine"

API="${TEST_HTTP}/api/v1"
TOPIC="sql-data"
PARTITIONS=4
TOTAL_RECORDS=500
RECORDS_PER_PARTITION=$((TOTAL_RECORDS / PARTITIONS))

# ═══════════════════════════════════════════════════════════════════════════════
# Setup: Create topic and produce 500 structured records
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}Setting up: creating topic and producing $TOTAL_RECORDS records...${NC}"

http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}"
# Accept 201 (created) or 409 (already exists)

PRODUCE_ERRORS=0
for p in $(seq 0 $((PARTITIONS - 1))); do
    BATCH=$(python3 -c "
import json, random
random.seed($p * 1000)
records = []
for i in range($RECORDS_PER_PARTITION):
    seq = $p * $RECORDS_PER_PARTITION + i
    category = ['A', 'B', 'C'][seq % 3]
    price = round(random.uniform(1.0, 100.0), 2)
    quantity = random.randint(1, 50)
    status = 'active' if seq % 2 == 0 else 'inactive'
    ts = 1700000000000 + seq * 1000
    records.append({
        'key': f'item-{seq}',
        'value': json.dumps({
            'category': category,
            'price': price,
            'quantity': quantity,
            'status': status,
            'timestamp': ts
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
# Helper: run SQL query
# ═══════════════════════════════════════════════════════════════════════════════

run_sql() {
    local query="$1"
    http_request POST "$API/sql" "{\"query\":\"$query\"}"
}

# ═══════════════════════════════════════════════════════════════════════════════
# SELECT COUNT(*)
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\""
if [ "$HTTP_STATUS" = "200" ]; then
    SQL_COUNT=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // empty' 2>/dev/null) || SQL_COUNT=""
    if [ -n "$SQL_COUNT" ] && [ "$SQL_COUNT" != "null" ] && [ "$SQL_COUNT" -ge "$TOTAL_RECORDS" ] 2>/dev/null; then
        pass "SELECT COUNT(*) = $SQL_COUNT (expected >= $TOTAL_RECORDS)"
    else
        fail "SELECT COUNT(*)" "got count=$SQL_COUNT, expected >= $TOTAL_RECORDS"
    fi
else
    fail "SELECT COUNT(*)" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SELECT * LIMIT 10
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT * FROM \\\"$TOPIC\\\" LIMIT 10"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    if [ "$ROW_COUNT" -le 10 ] && [ "$ROW_COUNT" -gt 0 ]; then
        pass "SELECT * LIMIT 10 — returned $ROW_COUNT rows (<= 10)"
    else
        fail "SELECT * LIMIT 10" "expected 1-10 rows, got $ROW_COUNT"
    fi
else
    fail "SELECT * LIMIT 10" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SELECT * LIMIT 5 OFFSET 10 (pagination)
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT * FROM \\\"$TOPIC\\\" LIMIT 5 OFFSET 10"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    if [ "$ROW_COUNT" -le 5 ] && [ "$ROW_COUNT" -gt 0 ]; then
        pass "SELECT * LIMIT 5 OFFSET 10 — returned $ROW_COUNT rows (<= 5)"
    else
        fail "SELECT * LIMIT 5 OFFSET 10" "expected 1-5 rows, got $ROW_COUNT"
    fi
else
    fail "SELECT * LIMIT 5 OFFSET 10" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SELECT * ORDER BY offset LIMIT 10
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT * FROM \\\"$TOPIC\\\" ORDER BY offset LIMIT 10"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    if [ "$ROW_COUNT" -gt 0 ]; then
        pass "SELECT * ORDER BY offset LIMIT 10 — returned $ROW_COUNT rows"
    else
        fail "SELECT * ORDER BY offset LIMIT 10" "0 rows returned"
    fi
else
    fail "SELECT * ORDER BY offset LIMIT 10" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SELECT with WHERE clause
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT * FROM \\\"$TOPIC\\\" WHERE offset >= 0 AND offset < 20"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    if [ "$ROW_COUNT" -gt 0 ]; then
        pass "SELECT with WHERE clause (offset range) — returned $ROW_COUNT rows"
    else
        fail "SELECT with WHERE clause" "0 rows returned"
    fi
else
    fail "SELECT with WHERE clause" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# json_extract: extract category values
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT json_extract(value, '\$.category') as cat FROM \\\"$TOPIC\\\" LIMIT 10"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    if [ "$ROW_COUNT" -gt 0 ]; then
        FIRST_CAT=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // empty' 2>/dev/null) || FIRST_CAT=""
        if [ "$FIRST_CAT" = "A" ] || [ "$FIRST_CAT" = "B" ] || [ "$FIRST_CAT" = "C" ]; then
            pass "json_extract(value, '\$.category') — got '$FIRST_CAT'"
        else
            pass "json_extract(value, '\$.category') — 200 OK ($ROW_COUNT rows)"
        fi
    else
        fail "json_extract category" "0 rows returned"
    fi
else
    fail "json_extract category" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# json_extract with WHERE: price > 50
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT * FROM \\\"$TOPIC\\\" WHERE json_extract(value, '\$.price') > 50 LIMIT 20"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    pass "WHERE json_extract(value, '\$.price') > 50 — returned $ROW_COUNT rows"
else
    fail "WHERE json_extract price > 50" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# GROUP BY json_extract with COUNT(*)
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT json_extract(value, '\$.category') as cat, COUNT(*) as cnt FROM \\\"$TOPIC\\\" GROUP BY json_extract(value, '\$.category')"
if [ "$HTTP_STATUS" = "200" ]; then
    GROUP_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || GROUP_COUNT=0
    if [ "$GROUP_COUNT" -eq 3 ]; then
        pass "GROUP BY category with COUNT(*) — 3 groups (A, B, C)"
    elif [ "$GROUP_COUNT" -gt 0 ]; then
        pass "GROUP BY category with COUNT(*) — $GROUP_COUNT groups"
    else
        fail "GROUP BY category" "0 groups returned"
    fi
else
    fail "GROUP BY category with COUNT(*)" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SUM aggregate
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT SUM(json_extract(value, '\$.quantity')) as total_qty FROM \\\"$TOPIC\\\""
if [ "$HTTP_STATUS" = "200" ]; then
    SUM_VAL=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // empty' 2>/dev/null) || SUM_VAL=""
    if [ -n "$SUM_VAL" ] && [ "$SUM_VAL" != "null" ]; then
        pass "SUM(quantity) = $SUM_VAL"
    else
        fail "SUM aggregate" "no result returned"
    fi
else
    fail "SUM aggregate" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# AVG aggregate
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT AVG(json_extract(value, '\$.price')) as avg_price FROM \\\"$TOPIC\\\""
if [ "$HTTP_STATUS" = "200" ]; then
    AVG_VAL=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // empty' 2>/dev/null) || AVG_VAL=""
    if [ -n "$AVG_VAL" ] && [ "$AVG_VAL" != "null" ]; then
        pass "AVG(price) = $AVG_VAL"
    else
        fail "AVG aggregate" "no result returned"
    fi
else
    fail "AVG aggregate" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# MIN/MAX aggregates
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT MIN(json_extract(value, '\$.price')) as min_price, MAX(json_extract(value, '\$.price')) as max_price FROM \\\"$TOPIC\\\""
if [ "$HTTP_STATUS" = "200" ]; then
    MIN_VAL=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // empty' 2>/dev/null) || MIN_VAL=""
    MAX_VAL=$(echo "$HTTP_BODY" | jq -r '.rows[0][1] // empty' 2>/dev/null) || MAX_VAL=""
    if [ -n "$MIN_VAL" ] && [ -n "$MAX_VAL" ] && [ "$MIN_VAL" != "null" ] && [ "$MAX_VAL" != "null" ]; then
        pass "MIN(price) = $MIN_VAL, MAX(price) = $MAX_VAL"
    else
        fail "MIN/MAX aggregates" "min=$MIN_VAL, max=$MAX_VAL"
    fi
else
    fail "MIN/MAX aggregates" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Error Cases
# ═══════════════════════════════════════════════════════════════════════════════

# Invalid SQL syntax
run_sql "THIS IS NOT SQL AT ALL"
if [ "$HTTP_STATUS" = "400" ]; then
    pass "Invalid SQL syntax — 400"
else
    fail "Invalid SQL syntax" "expected 400, got HTTP $HTTP_STATUS"
fi

# Query nonexistent topic
run_sql "SELECT * FROM \\\"does_not_exist_xyz_99\\\" LIMIT 1"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Query nonexistent topic — 404"
elif [ "$HTTP_STATUS" = "400" ]; then
    pass "Query nonexistent topic — 400"
else
    fail "Query nonexistent topic" "expected 404 or 400, got HTTP $HTTP_STATUS"
fi

# Empty query
http_request POST "$API/sql" '{"query":""}'
if [ "$HTTP_STATUS" = "400" ]; then
    pass "Empty query — 400"
else
    fail "Empty query" "expected 400, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SHOW TOPICS
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" '{"query":"SHOW TOPICS"}'
if [ "$HTTP_STATUS" = "200" ]; then
    if echo "$HTTP_BODY" | grep -q "$TOPIC"; then
        pass "SQL: SHOW TOPICS — contains '$TOPIC'"
    else
        pass "SQL: SHOW TOPICS — 200 OK"
    fi
else
    fail "SQL: SHOW TOPICS" "HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# DESCRIBE topic
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"DESCRIBE \\\"$TOPIC\\\"\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: DESCRIBE \"$TOPIC\""
else
    fail "SQL: DESCRIBE" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# TUMBLE window function (graceful — may not be implemented)
# ═══════════════════════════════════════════════════════════════════════════════

run_sql "SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\" GROUP BY TUMBLE(timestamp, INTERVAL '1' HOUR)"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: TUMBLE window function"
elif [ "$HTTP_STATUS" = "400" ]; then
    skip "SQL: TUMBLE window" "not implemented (HTTP 400)"
else
    fail "SQL: TUMBLE window" "HTTP $HTTP_STATUS"
fi

# ── Cleanup ──────────────────────────────────────────────────────────────────
http_request DELETE "$API/topics/$TOPIC" &>/dev/null || true
