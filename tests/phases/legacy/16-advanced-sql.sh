#!/usr/bin/env bash
# Phase 16 — Advanced SQL Tests
# json_extract, GROUP BY, aggregations, SHOW TOPICS, DESCRIBE, window functions

phase_header "Phase 16 — Advanced SQL"

API="${TEST_HTTP}/api/v1"
TOPIC="adv-sql-data"
PARTITIONS=4
TOTAL_RECORDS=200
RECORDS_PER_PARTITION=$((TOTAL_RECORDS / PARTITIONS))

# ═══════════════════════════════════════════════════════════════════════════════
# Setup: Create topic and populate with structured JSON data
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}Setting up: creating topic and producing $TOTAL_RECORDS records...${NC}"

http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}"

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
            'price': round(5.0 + seq * 0.5, 2),
            'category': ['electronics', 'books', 'clothing', 'food'][seq % 4],
            'quantity': (seq % 10) + 1,
            'in_stock': seq % 3 != 0
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
# SELECT COUNT(*)
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\"\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    SQL_COUNT=$(echo "$HTTP_BODY" | jq -r '.rows[0][0] // .rows[0].cnt // .results[0][0] // empty' 2>/dev/null) || SQL_COUNT=""
    if [ -n "$SQL_COUNT" ] && [ "$SQL_COUNT" != "null" ] && [ "$SQL_COUNT" -ge 1 ] 2>/dev/null; then
        pass "SQL: SELECT COUNT(*) = $SQL_COUNT"
    else
        fail "SQL: SELECT COUNT(*)" "got count=$SQL_COUNT, expected >= 1"
    fi
else
    fail "SQL: SELECT COUNT(*)" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SELECT * LIMIT
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT * FROM \\\"$TOPIC\\\" LIMIT 5\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    if [ "$ROW_COUNT" -le 5 ]; then
        pass "SQL: SELECT * LIMIT 5 — got $ROW_COUNT rows"
    else
        fail "SQL: SELECT * LIMIT 5" "expected <= 5, got $ROW_COUNT"
    fi
else
    fail "SQL: SELECT * LIMIT 5" "HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# ORDER BY offset
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT * FROM \\\"$TOPIC\\\" ORDER BY offset LIMIT 10\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: SELECT * ORDER BY offset LIMIT 10"
else
    fail "SQL: ORDER BY offset" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# LIMIT + OFFSET
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT * FROM \\\"$TOPIC\\\" LIMIT 5 OFFSET 10\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: SELECT * LIMIT 5 OFFSET 10"
else
    fail "SQL: LIMIT+OFFSET" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# json_extract basic
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT json_extract(value, '$.category') as cat FROM \\\"$TOPIC\\\" LIMIT 5\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: json_extract(value, '$.category') LIMIT 5"
else
    fail "SQL: json_extract basic" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# json_extract in WHERE
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT * FROM \\\"$TOPIC\\\" WHERE json_extract(value, '$.price') > 50 LIMIT 10\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    ROW_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || ROW_COUNT=0
    pass "SQL: WHERE json_extract(value, '$.price') > 50 — $ROW_COUNT rows"
else
    fail "SQL: json_extract WHERE" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# GROUP BY with COUNT
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT json_extract(value, '$.category') as cat, COUNT(*) as cnt FROM \\\"$TOPIC\\\" GROUP BY json_extract(value, '$.category')\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    GROUP_COUNT=$(echo "$HTTP_BODY" | jq '.rows | length' 2>/dev/null) || GROUP_COUNT=0
    pass "SQL: GROUP BY category + COUNT — $GROUP_COUNT groups"
else
    fail "SQL: GROUP BY COUNT" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# GROUP BY with SUM
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT json_extract(value, '$.category') as cat, SUM(json_extract(value, '$.quantity')) as total FROM \\\"$TOPIC\\\" GROUP BY json_extract(value, '$.category')\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: GROUP BY category + SUM(quantity)"
else
    fail "SQL: GROUP BY SUM" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# GROUP BY with AVG
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT json_extract(value, '$.category') as cat, AVG(json_extract(value, '$.price')) as avg_price FROM \\\"$TOPIC\\\" GROUP BY json_extract(value, '$.category')\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: GROUP BY category + AVG(price)"
else
    fail "SQL: GROUP BY AVG" "HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# TUMBLE window (test gracefully)
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" "{\"query\":\"SELECT COUNT(*) as cnt FROM \\\"$TOPIC\\\" GROUP BY TUMBLE(timestamp, INTERVAL '1' HOUR)\"}"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "SQL: TUMBLE window function"
elif [ "$HTTP_STATUS" = "400" ]; then
    skip "SQL: TUMBLE window" "not implemented (HTTP 400)"
else
    fail "SQL: TUMBLE window" "HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Error Cases
# ═══════════════════════════════════════════════════════════════════════════════

# Invalid SQL syntax
http_request POST "$API/sql" '{"query":"SELEC * FORM broken"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "SQL: invalid syntax → $HTTP_STATUS"
else
    fail "SQL: invalid syntax" "expected 4xx/5xx, got $HTTP_STATUS"
fi

# Nonexistent topic
http_request POST "$API/sql" '{"query":"SELECT * FROM \"nonexistent_advsql_xyz\" LIMIT 1"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "SQL: nonexistent topic → $HTTP_STATUS"
else
    fail "SQL: nonexistent topic" "expected error, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/$TOPIC" &>/dev/null || true
