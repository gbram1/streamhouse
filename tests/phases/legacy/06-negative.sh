#!/usr/bin/env bash
# Phase 06 — Negative / Error Cases
# Tests malformed data, missing resources, invalid inputs, and proper error responses

phase_header "Phase 06 — Negative / Error Cases"

API="${TEST_HTTP}/api/v1"
SR="${TEST_HTTP}/schemas"

# Create a topic for negative tests
http_request POST "$API/topics" '{"name":"negative-test","partitions":2}'

# ═══════════════════════════════════════════════════════════════════════════════
# REST — Produce Errors
# ═══════════════════════════════════════════════════════════════════════════════

# Produce to nonexistent topic
http_request POST "$API/produce" '{"topic":"does-not-exist-xyz","key":"k","value":"v"}'
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Produce to nonexistent topic → $HTTP_STATUS"
else
    fail "Produce to nonexistent topic" "expected 4xx/5xx, got $HTTP_STATUS"
fi

# Produce with missing value field
http_request POST "$API/produce" '{"topic":"negative-test","key":"k"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "200" ]; then
    pass "Produce with missing value → $HTTP_STATUS"
else
    fail "Produce with missing value" "got $HTTP_STATUS"
fi

# Produce with invalid JSON body
http_request POST "$API/produce" 'this is not json at all'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Produce with invalid JSON → $HTTP_STATUS"
else
    fail "Produce with invalid JSON" "expected 400/422, got $HTTP_STATUS"
fi

# Produce with empty body
http_request POST "$API/produce" ''
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "411" ] || [ "$HTTP_STATUS" = "415" ]; then
    pass "Produce with empty body → $HTTP_STATUS"
else
    fail "Produce with empty body" "expected 4xx, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# REST — Consume Errors
# ═══════════════════════════════════════════════════════════════════════════════

# Consume from nonexistent topic
http_request GET "$API/consume?topic=does-not-exist-xyz&partition=0&offset=0"
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Consume from nonexistent topic → $HTTP_STATUS"
else
    fail "Consume from nonexistent topic" "expected 4xx/5xx, got $HTTP_STATUS"
fi

# Consume with missing required params
http_request GET "$API/consume"
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Consume with no params → $HTTP_STATUS"
else
    fail "Consume with no params" "expected 400/422, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# REST — Topic Errors
# ═══════════════════════════════════════════════════════════════════════════════

# Create topic with 0 partitions
http_request POST "$API/topics" '{"name":"zero-parts","partitions":0}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Create topic with 0 partitions → $HTTP_STATUS"
else
    fail "Create topic with 0 partitions" "expected error, got $HTTP_STATUS"
fi

# Create topic with empty name
http_request POST "$API/topics" '{"name":"","partitions":2}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Create topic with empty name → $HTTP_STATUS"
else
    fail "Create topic with empty name" "expected error, got $HTTP_STATUS"
fi

# Create topic with invalid JSON
http_request POST "$API/topics" 'not json'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Create topic with invalid JSON → $HTTP_STATUS"
else
    fail "Create topic with invalid JSON" "expected 400/422, got $HTTP_STATUS"
fi

# Delete nonexistent topic
http_request DELETE "$API/topics/nonexistent-delete-xyz"
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "204" ]; then
    pass "Delete nonexistent topic → $HTTP_STATUS"
else
    fail "Delete nonexistent topic" "expected 404/2xx, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# REST — SQL Errors
# ═══════════════════════════════════════════════════════════════════════════════

# SQL syntax error
http_request POST "$API/sql" '{"query":"SELEC * FORM broken"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "SQL syntax error → $HTTP_STATUS"
else
    fail "SQL syntax error" "expected 4xx/5xx, got $HTTP_STATUS"
fi

# SQL against nonexistent topic
http_request POST "$API/sql" '{"query":"SELECT * FROM \"nonexistent_topic_xyz\" LIMIT 1"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "SQL query nonexistent topic → $HTTP_STATUS"
else
    fail "SQL query nonexistent topic" "expected error, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# REST — Unknown Route
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/this-route-does-not-exist"
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "405" ]; then
    pass "Unknown route → $HTTP_STATUS"
else
    fail "Unknown route" "expected 404/405, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Schema Registry — Errors
# ═══════════════════════════════════════════════════════════════════════════════

# Register garbage as Avro
http_request POST "$SR/subjects/garbage-value/versions" \
    '{"schema": "this is definitely not avro", "schemaType": "AVRO"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Register garbage as Avro → $HTTP_STATUS"
else
    fail "Register garbage as Avro" "expected error, got $HTTP_STATUS"
fi

# Register empty schema
http_request POST "$SR/subjects/empty-value/versions" '{"schema": "", "schemaType": "AVRO"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Register empty schema → $HTTP_STATUS"
else
    fail "Register empty schema" "expected error, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# gRPC — Error Cases
# ═══════════════════════════════════════════════════════════════════════════════

if [ "${HAS_GRPCURL:-0}" = "1" ]; then
    # Produce with empty topic
    RESULT=$(grpcurl -plaintext \
        -d '{"topic":"","partition":0,"key":"dQ==","value":"dQ=="}' \
        "$TEST_GRPC" streamhouse.StreamHouse/Produce 2>&1) || true
    if echo "$RESULT" | grep -qi "error\|invalid\|not found\|INVALID_ARGUMENT\|NOT_FOUND"; then
        pass "gRPC: Produce with empty topic → error"
    else
        fail "gRPC: Produce with empty topic" "expected error: $(echo "$RESULT" | head -c 200)"
    fi

    # ProduceBatch with empty records
    RESULT=$(grpcurl -plaintext \
        -d '{"topic":"negative-test","partition":0,"records":[]}' \
        "$TEST_GRPC" streamhouse.StreamHouse/ProduceBatch 2>&1) || true
    if echo "$RESULT" | grep -qi "error\|invalid\|INVALID_ARGUMENT" || echo "$RESULT" | jq -e '.count == 0' &>/dev/null; then
        pass "gRPC: ProduceBatch with empty records → error/zero"
    else
        fail "gRPC: ProduceBatch with empty records" "$(echo "$RESULT" | head -c 200)"
    fi
else
    skip "gRPC error cases" "grpcurl not installed"
fi

# ── Cleanup ───────────────────────────────────────────────────────────────────
http_request DELETE "$API/topics/negative-test" &>/dev/null || true
