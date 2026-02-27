#!/usr/bin/env bash
# Phase 01 — Smoke Tests
# Quick health checks and basic produce/consume to validate the server is working

phase_header "Phase 01 — Smoke Tests"

# ── Health Checks ─────────────────────────────────────────────────────────────
assert_status "GET /health returns 200" "200" GET "${TEST_HTTP}/health"

assert_status "GET /live returns 200" "200" GET "${TEST_HTTP}/live"

assert_status "GET /ready returns 200" "200" GET "${TEST_HTTP}/ready"

# ── Basic Produce / Consume ───────────────────────────────────────────────────

# Create topic
http_request POST "${TEST_HTTP}/api/v1/topics" '{"name":"smoke-test","partitions":2}'
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Create topic 'smoke-test'"
else
    fail "Create topic 'smoke-test'" "got HTTP $HTTP_STATUS"
fi

# Produce a message
http_request POST "${TEST_HTTP}/api/v1/produce" \
    '{"topic":"smoke-test","key":"k1","value":"{\"hello\":\"world\"}"}'
if [ "$HTTP_STATUS" = "200" ]; then
    pass "Produce single message"
else
    fail "Produce single message" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Wait for flush (background flush runs every 5s)
wait_flush 8

# Consume it back — STRICT: data MUST be consumable after flush
# Note: 500 on a partition means no segments exist there (empty partition) — that's OK
# as long as the total across all partitions is >= 1
SMOKE_TOTAL=0
for p in 0 1; do
    http_request GET "${TEST_HTTP}/api/v1/consume?topic=smoke-test&partition=${p}&offset=0&maxRecords=100"
    if [ "$HTTP_STATUS" = "200" ]; then
        COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || COUNT=0
        SMOKE_TOTAL=$((SMOKE_TOTAL + COUNT))
    fi
    # 500 = "offset not found" on empty partition — not an error, just no data on that partition
done

if [ "$SMOKE_TOTAL" -ge 1 ]; then
    pass "Consume message back (got $SMOKE_TOTAL records)"
else
    fail "Consume message back" "got 0 records after flush — data not reaching storage"
fi
