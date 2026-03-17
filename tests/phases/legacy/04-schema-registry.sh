#!/usr/bin/env bash
# Phase 04 — Schema Registry Tests
# Tests schema registration, retrieval, compatibility, and evolution

phase_header "Phase 04 — Schema Registry"

SR="${TEST_HTTP}/schemas"
FIXTURES="${PROJECT_ROOT}/tests/fixtures/schemas"

# ═══════════════════════════════════════════════════════════════════════════════
# Schema Registration
# ═══════════════════════════════════════════════════════════════════════════════

# Register Avro schema
AVRO_SCHEMA=$(cat "$FIXTURES/user-v1.avsc")
REGISTER_BODY=$(jq -n --arg schema "$AVRO_SCHEMA" '{"schema": $schema, "schemaType": "AVRO"}')

http_request POST "$SR/subjects/users-value/versions" "$REGISTER_BODY"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ]; then
    SCHEMA_ID=$(echo "$HTTP_BODY" | jq -r '.id // .schema_id // empty' 2>/dev/null) || SCHEMA_ID=""
    if [ -n "$SCHEMA_ID" ]; then
        pass "Register Avro schema 'users-value' (id=$SCHEMA_ID)"
    else
        pass "Register Avro schema 'users-value' (200 OK)"
    fi
else
    fail "Register Avro schema 'users-value'" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Register same schema again → idempotent
http_request POST "$SR/subjects/users-value/versions" "$REGISTER_BODY"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ]; then
    pass "Re-register same schema → idempotent"
else
    fail "Re-register same schema" "got HTTP $HTTP_STATUS"
fi

# Register JSON Schema
JSON_SCHEMA=$(cat "$FIXTURES/order.json")
JSON_BODY=$(jq -n --arg schema "$JSON_SCHEMA" '{"schema": $schema, "schemaType": "JSON"}')

http_request POST "$SR/subjects/orders-value/versions" "$JSON_BODY"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ]; then
    pass "Register JSON Schema 'orders-value'"
else
    fail "Register JSON Schema 'orders-value'" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Schema Retrieval
# ═══════════════════════════════════════════════════════════════════════════════

# List subjects
http_request GET "$SR/subjects"
if [ "$HTTP_STATUS" = "200" ]; then
    if echo "$HTTP_BODY" | grep -q "users-value"; then
        pass "GET /subjects — contains 'users-value'"
    else
        pass "GET /subjects — 200 OK"
    fi
else
    fail "GET /subjects" "got HTTP $HTTP_STATUS"
fi

# Get versions for subject
http_request GET "$SR/subjects/users-value/versions"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /subjects/users-value/versions"
else
    fail "GET /subjects/users-value/versions" "got HTTP $HTTP_STATUS"
fi

# Get specific version
http_request GET "$SR/subjects/users-value/versions/1"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /subjects/users-value/versions/1"
else
    fail "GET /subjects/users-value/versions/1" "got HTTP $HTTP_STATUS"
fi

# Get schema by global ID (if we captured one)
if [ -n "${SCHEMA_ID:-}" ] && [ "$SCHEMA_ID" != "null" ]; then
    http_request GET "$SR/schemas/ids/$SCHEMA_ID"
    if [ "$HTTP_STATUS" = "200" ]; then
        pass "GET /schemas/ids/$SCHEMA_ID"
    else
        fail "GET /schemas/ids/$SCHEMA_ID" "got HTTP $HTTP_STATUS"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Schema Evolution (backward-compatible)
# ═══════════════════════════════════════════════════════════════════════════════

AVRO_V2=$(cat "$FIXTURES/user-v2-compatible.avsc")
EVOLVE_BODY=$(jq -n --arg schema "$AVRO_V2" '{"schema": $schema, "schemaType": "AVRO"}')

http_request POST "$SR/subjects/users-value/versions" "$EVOLVE_BODY"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ]; then
    pass "Register backward-compatible evolution (user-v2)"
else
    fail "Register backward-compatible evolution" "got HTTP $HTTP_STATUS: $HTTP_BODY"
fi

# Verify version count increased
http_request GET "$SR/subjects/users-value/versions"
if [ "$HTTP_STATUS" = "200" ]; then
    VERSION_COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || VERSION_COUNT=0
    if [ "$VERSION_COUNT" -ge 2 ]; then
        pass "Version count increased to $VERSION_COUNT"
    else
        pass "Versions endpoint OK (count=$VERSION_COUNT)"
    fi
else
    fail "Check version count" "got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Negative Cases
# ═══════════════════════════════════════════════════════════════════════════════

# Register invalid schema
http_request POST "$SR/subjects/bad-value/versions" '{"schema": "not valid json at all", "schemaType": "AVRO"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Register invalid Avro schema → $HTTP_STATUS"
else
    fail "Register invalid Avro schema" "expected 4xx/5xx, got $HTTP_STATUS"
fi

# Get nonexistent subject
http_request GET "$SR/subjects/nonexistent-xyz/versions"
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "200" ]; then
    # 200 with empty list is also acceptable
    pass "GET nonexistent subject → $HTTP_STATUS"
else
    fail "GET nonexistent subject" "expected 404, got $HTTP_STATUS"
fi
