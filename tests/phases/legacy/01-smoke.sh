#!/usr/bin/env bash
# Phase 01 — Smoke Tests
# Quick validation that the server is alive and basic roundtrip works

phase_header "Phase 01 — Smoke Tests"

TOPIC="smoke-test"
API="${TEST_HTTP}/api/v1"

# ── Health Checks ─────────────────────────────────────────────────────────────
assert_status "GET /health returns 200" "200" GET "${TEST_HTTP}/health"

assert_status "GET /live returns 200" "200" GET "${TEST_HTTP}/live"

assert_status "GET /ready returns 200" "200" GET "${TEST_HTTP}/ready"

# ── Create Topic ──────────────────────────────────────────────────────────────
http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":2}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Create topic '$TOPIC' (2 partitions) -> $HTTP_STATUS"
else
    fail "Create topic '$TOPIC'" "expected 201 or 409, got HTTP $HTTP_STATUS"
fi

# ── Produce Single Message ───────────────────────────────────────────────────
http_request POST "$API/produce" \
    '{"topic":"smoke-test","key":"smoke-k1","value":"{\"hello\":\"world\"}"}'
if [ "$HTTP_STATUS" = "200" ]; then
    pass "Produce single message"
else
    fail "Produce single message" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# ── Wait and Consume ─────────────────────────────────────────────────────────
wait_flush 8

SMOKE_TOTAL=0
for p in 0 1; do
    http_request GET "$API/consume?topic=$TOPIC&partition=${p}&offset=0&maxRecords=100"
    if [ "$HTTP_STATUS" = "200" ]; then
        COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || COUNT=0
        SMOKE_TOTAL=$((SMOKE_TOTAL + COUNT))
    fi
done

if [ "$SMOKE_TOTAL" -ge 1 ]; then
    pass "Consume message back (got $SMOKE_TOTAL records)"
else
    fail "Consume message back" "got 0 records after flush — data not reaching storage"
fi

# ── Produce Batch of 10 ──────────────────────────────────────────────────────
BATCH=$(make_batch_json "$TOPIC" 0 10 "smoke-batch")
http_request POST "$API/produce/batch" "$BATCH"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "Produce batch of 10 messages"
else
    fail "Produce batch of 10 messages" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# ── Wait and Verify Count Increased ──────────────────────────────────────────
wait_flush 8

SMOKE_TOTAL_AFTER=0
for p in 0 1; do
    http_request GET "$API/consume?topic=$TOPIC&partition=${p}&offset=0&maxRecords=100000"
    if [ "$HTTP_STATUS" = "200" ]; then
        COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || COUNT=0
        SMOKE_TOTAL_AFTER=$((SMOKE_TOTAL_AFTER + COUNT))
    fi
done

if [ "$SMOKE_TOTAL_AFTER" -gt "$SMOKE_TOTAL" ]; then
    pass "Record count increased after batch produce ($SMOKE_TOTAL -> $SMOKE_TOTAL_AFTER)"
elif [ "$SMOKE_TOTAL_AFTER" -ge 11 ]; then
    pass "Total record count >= 11 after batch produce (got $SMOKE_TOTAL_AFTER)"
else
    fail "Record count after batch produce" "expected > $SMOKE_TOTAL, got $SMOKE_TOTAL_AFTER"
fi
