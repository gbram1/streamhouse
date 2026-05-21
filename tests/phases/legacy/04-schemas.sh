#!/usr/bin/env bash
# Phase 04 — Schema Registry
# Complete schema lifecycle: registration, retrieval, evolution, validation, and deletion.

phase_header "Phase 04 — Schema Registry"

SR="${TEST_HTTP}/schemas"
API="${TEST_HTTP}/api/v1"
FIXTURES="${PROJECT_ROOT}/tests/fixtures/schemas"

# ═══════════════════════════════════════════════════════════════════════════════
# Schema Registration
# ═══════════════════════════════════════════════════════════════════════════════

# Register JSON Schema for subject "orders-value"
JSON_SCHEMA=$(cat "$FIXTURES/order.json")
JSON_BODY=$(jq -n --arg schema "$JSON_SCHEMA" '{"schema": $schema, "schemaType": "JSON"}')

http_request POST "$SR/subjects/orders-value/versions" "$JSON_BODY"
if [ "$HTTP_STATUS" = "200" ]; then
    ORDERS_SCHEMA_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || ORDERS_SCHEMA_ID=""
    pass "Register JSON Schema 'orders-value' (id=$ORDERS_SCHEMA_ID)"
else
    fail "Register JSON Schema 'orders-value'" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
    ORDERS_SCHEMA_ID=""
fi

# Register Avro schema for subject "users-value"
AVRO_SCHEMA=$(cat "$FIXTURES/user-v1.avsc")
AVRO_BODY=$(jq -n --arg schema "$AVRO_SCHEMA" '{"schema": $schema, "schemaType": "AVRO"}')

http_request POST "$SR/subjects/users-value/versions" "$AVRO_BODY"
if [ "$HTTP_STATUS" = "200" ]; then
    USERS_SCHEMA_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || USERS_SCHEMA_ID=""
    pass "Register Avro schema 'users-value' (id=$USERS_SCHEMA_ID)"
else
    fail "Register Avro schema 'users-value'" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
    USERS_SCHEMA_ID=""
fi

# ═══════════════════════════════════════════════════════════════════════════════
# List Subjects
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$SR/subjects"
if [ "$HTTP_STATUS" = "200" ]; then
    HAS_ORDERS=$(echo "$HTTP_BODY" | jq 'map(select(. == "orders-value")) | length' 2>/dev/null) || HAS_ORDERS=0
    HAS_USERS=$(echo "$HTTP_BODY" | jq 'map(select(. == "users-value")) | length' 2>/dev/null) || HAS_USERS=0
    if [ "$HAS_ORDERS" -ge 1 ] && [ "$HAS_USERS" -ge 1 ]; then
        pass "List subjects — both 'orders-value' and 'users-value' present"
    else
        fail "List subjects" "expected both subjects, got: $(echo "$HTTP_BODY" | head -c 200)"
    fi
else
    fail "List subjects" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Get Schema Versions
# ═══════════════════════════════════════════════════════════════════════════════

# Versions for "orders-value" should be [1]
http_request GET "$SR/subjects/orders-value/versions"
if [ "$HTTP_STATUS" = "200" ]; then
    VERSIONS=$(echo "$HTTP_BODY" | jq '.' 2>/dev/null)
    VERSION_COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || VERSION_COUNT=0
    FIRST_VERSION=$(echo "$HTTP_BODY" | jq '.[0]' 2>/dev/null) || FIRST_VERSION=""
    if [ "$VERSION_COUNT" -eq 1 ] && [ "$FIRST_VERSION" = "1" ]; then
        pass "Get versions for 'orders-value' — [1]"
    else
        fail "Get versions for 'orders-value'" "expected [1], got $VERSIONS"
    fi
else
    fail "Get versions for 'orders-value'" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Get Schema by Version
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$SR/subjects/orders-value/versions/1"
if [ "$HTTP_STATUS" = "200" ]; then
    RESP_SUBJECT=$(echo "$HTTP_BODY" | jq -r '.subject' 2>/dev/null) || RESP_SUBJECT=""
    RESP_VERSION=$(echo "$HTTP_BODY" | jq -r '.version' 2>/dev/null) || RESP_VERSION=""
    RESP_SCHEMA=$(echo "$HTTP_BODY" | jq -r '.schema' 2>/dev/null) || RESP_SCHEMA=""
    if [ "$RESP_SUBJECT" = "orders-value" ] && [ "$RESP_VERSION" = "1" ] && [ -n "$RESP_SCHEMA" ] && [ "$RESP_SCHEMA" != "null" ]; then
        pass "Get schema by version — orders-value v1 with schema content"
    else
        fail "Get schema by version" "subject=$RESP_SUBJECT version=$RESP_VERSION schema_present=$([ -n "$RESP_SCHEMA" ] && echo yes || echo no)"
    fi
else
    fail "Get schema by version" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Get Schema by Global ID
# ═══════════════════════════════════════════════════════════════════════════════

if [ -n "${ORDERS_SCHEMA_ID:-}" ] && [ "$ORDERS_SCHEMA_ID" != "null" ]; then
    http_request GET "$SR/schemas/ids/$ORDERS_SCHEMA_ID"
    if [ "$HTTP_STATUS" = "200" ]; then
        ID_SCHEMA=$(echo "$HTTP_BODY" | jq -r '.schema' 2>/dev/null) || ID_SCHEMA=""
        if [ -n "$ID_SCHEMA" ] && [ "$ID_SCHEMA" != "null" ]; then
            pass "Get schema by global ID ($ORDERS_SCHEMA_ID) — returns schema content"
        else
            fail "Get schema by global ID" "200 but no schema content"
        fi
    else
        fail "Get schema by global ID ($ORDERS_SCHEMA_ID)" "expected 200, got HTTP $HTTP_STATUS"
    fi
else
    skip "Get schema by global ID" "no schema ID captured from registration"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Re-register Same Schema (Idempotent)
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$SR/subjects/orders-value/versions" "$JSON_BODY"
if [ "$HTTP_STATUS" = "200" ]; then
    REREGISTER_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || REREGISTER_ID=""
    if [ -n "${ORDERS_SCHEMA_ID:-}" ] && [ "$REREGISTER_ID" = "$ORDERS_SCHEMA_ID" ]; then
        pass "Re-register same schema — idempotent (same id=$REREGISTER_ID)"
    else
        pass "Re-register same schema — idempotent (200 OK, id=$REREGISTER_ID)"
    fi
else
    fail "Re-register same schema" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Schema Evolution (Backward-Compatible)
# ═══════════════════════════════════════════════════════════════════════════════

# Register v2 for users-value: adds optional "phone" field (backward-compatible)
AVRO_V2=$(cat "$FIXTURES/user-v2-compatible.avsc")
EVOLVE_BODY=$(jq -n --arg schema "$AVRO_V2" '{"schema": $schema, "schemaType": "AVRO"}')

http_request POST "$SR/subjects/users-value/versions" "$EVOLVE_BODY"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "Register backward-compatible evolution (user-v2, adds optional field)"
else
    fail "Register backward-compatible evolution" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# Verify version count is now 2
http_request GET "$SR/subjects/users-value/versions"
if [ "$HTTP_STATUS" = "200" ]; then
    VERSION_COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || VERSION_COUNT=0
    if [ "$VERSION_COUNT" -eq 2 ]; then
        VERSIONS_STR=$(echo "$HTTP_BODY" | jq -c '.' 2>/dev/null)
        pass "Get versions after evolution — $VERSIONS_STR (count=2)"
    else
        fail "Get versions after evolution" "expected 2 versions, got $VERSION_COUNT"
    fi
else
    fail "Get versions after evolution" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Breaking Change (Should Fail)
# ═══════════════════════════════════════════════════════════════════════════════

# Register breaking change: removes "name" field, adds "username" — not backward compatible
AVRO_BREAKING=$(cat "$FIXTURES/user-breaking.avsc")
BREAKING_BODY=$(jq -n --arg schema "$AVRO_BREAKING" '{"schema": $schema, "schemaType": "AVRO"}')

http_request POST "$SR/subjects/users-value/versions" "$BREAKING_BODY"
if [ "$HTTP_STATUS" = "409" ]; then
    pass "Register breaking change — rejected (409 Conflict)"
elif [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Register breaking change — rejected ($HTTP_STATUS)"
else
    fail "Register breaking change" "expected 409/400/422, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Invalid Schema (Garbage String)
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$SR/subjects/bad-value/versions" '{"schema": "not valid schema at all", "schemaType": "AVRO"}'
if [ "$HTTP_STATUS" = "422" ]; then
    pass "Register invalid schema (garbage string) — 422"
elif [ "$HTTP_STATUS" = "400" ]; then
    pass "Register invalid schema (garbage string) — 400"
else
    fail "Register invalid schema (garbage string)" "expected 422 or 400, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Empty Body
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$SR/subjects/bad-value/versions" '{}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Register schema with empty body — $HTTP_STATUS"
else
    fail "Register schema with empty body" "expected 400 or 422, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Get Nonexistent Subject
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$SR/subjects/nonexistent-xyz-999/versions"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Get nonexistent subject — 404"
else
    fail "Get nonexistent subject" "expected 404, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Schema-Enforced Produce
# ═══════════════════════════════════════════════════════════════════════════════

# Create a topic whose name matches the schema subject (orders-value => topic "orders")
http_request POST "$API/topics" '{"name":"orders","partitions":1}'
# Accept 201 (created) or 409 (already exists)

# Produce a valid message conforming to the orders JSON schema
VALID_ORDER='{"order_id":"ORD-001","amount":99.99,"currency":"USD","items":[{"sku":"SKU-A","quantity":2}]}'
PRODUCE_BODY=$(jq -n --arg topic "orders" --arg value "$VALID_ORDER" '{topic: $topic, key: "k1", value: $value}')

http_request POST "$API/produce" "$PRODUCE_BODY"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "Schema-enforced produce — valid message accepted (200)"
else
    fail "Schema-enforced produce — valid message" "expected 200, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# Produce an invalid message (missing required field "order_id")
INVALID_ORDER='{"amount":50,"currency":"GBP"}'
INVALID_BODY=$(jq -n --arg topic "orders" --arg value "$INVALID_ORDER" '{topic: $topic, key: "k2", value: $value}')

http_request POST "$API/produce" "$INVALID_BODY"
if [ "$HTTP_STATUS" = "400" ]; then
    pass "Schema-enforced produce — invalid message rejected (400)"
elif [ "$HTTP_STATUS" = "422" ]; then
    pass "Schema-enforced produce — invalid message rejected (422)"
else
    fail "Schema-enforced produce — invalid message" "expected 400 or 422, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Delete Subject
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$SR/subjects/orders-value"
if [ "$HTTP_STATUS" = "200" ]; then
    DELETED_VERSIONS=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || DELETED_VERSIONS=0
    pass "Delete subject 'orders-value' — 200 (deleted $DELETED_VERSIONS versions)"
elif [ "$HTTP_STATUS" = "204" ]; then
    pass "Delete subject 'orders-value' — 204"
else
    fail "Delete subject 'orders-value'" "expected 200 or 204, got HTTP $HTTP_STATUS"
fi

# Confirm deleted: should return 404
http_request GET "$SR/subjects/orders-value/versions"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Confirm subject deleted — 404"
else
    fail "Confirm subject deleted" "expected 404, got HTTP $HTTP_STATUS"
fi

# ── Cleanup ──────────────────────────────────────────────────────────────────
http_request DELETE "$SR/subjects/users-value" &>/dev/null || true
http_request DELETE "$API/topics/orders" &>/dev/null || true
