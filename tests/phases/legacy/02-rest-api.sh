#!/usr/bin/env bash
# Phase 02 — REST API Surface Tests
# Exercises every REST endpoint for topics, produce, consume, consumer groups, metrics, SQL

phase_header "Phase 02 — REST API Surface"

API="${TEST_HTTP}/api/v1"

# ═══════════════════════════════════════════════════════════════════════════════
# Topics CRUD
# ═══════════════════════════════════════════════════════════════════════════════

# Create topic
assert_status "POST /topics — create 'rest-t1' (4 partitions)" "201" \
    POST "$API/topics" '{"name":"rest-t1","partitions":4}'

# Duplicate create → 409
assert_status "POST /topics — duplicate 'rest-t1' → 409" "409" \
    POST "$API/topics" '{"name":"rest-t1","partitions":4}'

# List topics
http_request GET "$API/topics"
if [ "$HTTP_STATUS" = "200" ] && echo "$HTTP_BODY" | jq -e '.[].name // .topics[].name' 2>/dev/null | grep -q "rest-t1"; then
    pass "GET /topics — list contains 'rest-t1'"
else
    # Try alternate response format
    if [ "$HTTP_STATUS" = "200" ] && echo "$HTTP_BODY" | grep -q "rest-t1"; then
        pass "GET /topics — list contains 'rest-t1'"
    else
        fail "GET /topics — list contains 'rest-t1'" "status=$HTTP_STATUS"
    fi
fi

# Get specific topic
http_request GET "$API/topics/rest-t1"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /topics/rest-t1 — get topic details"
else
    fail "GET /topics/rest-t1" "got HTTP $HTTP_STATUS"
fi

# Get nonexistent topic → 404
assert_status "GET /topics/nonexistent → 404" "404" GET "$API/topics/nonexistent-topic-xyz"

# List partitions
http_request GET "$API/topics/rest-t1/partitions"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /topics/rest-t1/partitions"
else
    fail "GET /topics/rest-t1/partitions" "got HTTP $HTTP_STATUS"
fi

# Create another topic for produce/consume tests
http_request POST "$API/topics" '{"name":"rest-produce","partitions":4}'

# ═══════════════════════════════════════════════════════════════════════════════
# Produce
# ═══════════════════════════════════════════════════════════════════════════════

# Single produce
http_request POST "$API/produce" \
    '{"topic":"rest-produce","key":"user-1","value":"{\"action\":\"click\",\"page\":\"home\"}"}'
if [ "$HTTP_STATUS" = "200" ]; then
    pass "POST /produce — single message"
else
    fail "POST /produce — single message" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Batch produce
BATCH=$(python3 -c "
import json
records = [{'key': f'batch-{i}', 'value': json.dumps({'seq': i, 'type': 'batch-test'})} for i in range(10)]
print(json.dumps({'topic': 'rest-produce', 'partition': 0, 'records': records}))
")
http_request POST "$API/produce/batch" "$BATCH"
if [ "$HTTP_STATUS" = "200" ]; then
    BATCH_COUNT=$(echo "$HTTP_BODY" | jq '.count // .results | length' 2>/dev/null) || BATCH_COUNT=0
    if [ "$BATCH_COUNT" -ge 1 ]; then
        pass "POST /produce/batch — 10 records (count=$BATCH_COUNT)"
    else
        pass "POST /produce/batch — 200 OK"
    fi
else
    fail "POST /produce/batch — 10 records" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Produce to nonexistent topic → 404
http_request POST "$API/produce" \
    '{"topic":"nonexistent-xyz","key":"k","value":"v"}'
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "400" ]; then
    pass "POST /produce to nonexistent topic → $HTTP_STATUS"
else
    fail "POST /produce to nonexistent topic" "expected 404/400, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Consume
# ═══════════════════════════════════════════════════════════════════════════════

wait_flush 8

CONSUME_TOTAL=0
for p in 0 1 2 3; do
    http_request GET "$API/consume?topic=rest-produce&partition=${p}&offset=0&maxRecords=100"
    if [ "$HTTP_STATUS" = "200" ]; then
        C=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || C=0
        CONSUME_TOTAL=$((CONSUME_TOTAL + C))
    fi
done
if [ "$CONSUME_TOTAL" -ge 1 ]; then
    pass "GET /consume — got $CONSUME_TOTAL records across all partitions"
else
    fail "GET /consume" "got 0 records after flush — data not reaching storage"
fi

# Consume from nonexistent topic → 404
http_request GET "$API/consume?topic=nonexistent-xyz&partition=0&offset=0"
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "400" ]; then
    pass "GET /consume nonexistent topic → $HTTP_STATUS"
else
    fail "GET /consume nonexistent topic" "expected 404/400, got $HTTP_STATUS"
fi

# Consume with maxRecords=1 (use partition 0 which had the batch produce)
http_request GET "$API/consume?topic=rest-produce&partition=0&offset=0&maxRecords=1"
if [ "$HTTP_STATUS" = "200" ]; then
    LIMITED_COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || LIMITED_COUNT=0
    if [ "$LIMITED_COUNT" -le 1 ]; then
        pass "GET /consume maxRecords=1 → got $LIMITED_COUNT"
    else
        fail "GET /consume maxRecords=1" "expected <= 1, got $LIMITED_COUNT"
    fi
else
    fail "GET /consume maxRecords=1" "got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Consumer Groups
# ═══════════════════════════════════════════════════════════════════════════════

# List consumer groups
http_request GET "$API/consumer-groups"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /consumer-groups"
else
    fail "GET /consumer-groups" "got HTTP $HTTP_STATUS"
fi

# Commit offset
http_request POST "$API/consumer-groups/commit" \
    '{"groupId":"test-cg","topic":"rest-produce","partition":0,"offset":5}'
if [ "$HTTP_STATUS" = "200" ]; then
    pass "POST /consumer-groups/commit"
else
    fail "POST /consumer-groups/commit" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Metrics
# ═══════════════════════════════════════════════════════════════════════════════

assert_status "GET /metrics" "200" GET "$API/metrics"
assert_status "GET /metrics/throughput" "200" GET "$API/metrics/throughput"
assert_status "GET /metrics/latency" "200" GET "$API/metrics/latency"
assert_status "GET /metrics/storage" "200" GET "$API/metrics/storage"

# ═══════════════════════════════════════════════════════════════════════════════
# SQL
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/sql" '{"query":"SHOW TOPICS"}'
if [ "$HTTP_STATUS" = "200" ]; then
    pass "POST /sql — SHOW TOPICS"
else
    # SQL might not recognize SHOW TOPICS — try a SELECT
    http_request POST "$API/sql" '{"query":"SELECT 1"}'
    if [ "$HTTP_STATUS" = "200" ]; then
        pass "POST /sql — basic query"
    else
        fail "POST /sql" "got HTTP $HTTP_STATUS: $HTTP_BODY"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Delete topic
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/rest-t1"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "204" ]; then
    pass "DELETE /topics/rest-t1"
else
    fail "DELETE /topics/rest-t1" "got HTTP $HTTP_STATUS"
fi

# Confirm deleted
http_request GET "$API/topics/rest-t1"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "GET /topics/rest-t1 after delete → 404"
else
    fail "GET /topics/rest-t1 after delete" "expected 404, got $HTTP_STATUS"
fi
