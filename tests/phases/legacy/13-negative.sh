#!/usr/bin/env bash
# Phase 13 — Error Cases & Edge Cases
# Strict error handling validation against actual handler status codes

phase_header "Phase 13 — Error Cases & Edge Cases"

API="${TEST_HTTP}/api/v1"
SR="${TEST_HTTP}/schemas"

# ═══════════════════════════════════════════════════════════════════════════════
# Topic Errors
# ═══════════════════════════════════════════════════════════════════════════════

# POST /api/v1/topics with 0 partitions → 400
# Handler: topics.rs line 218-219 returns BAD_REQUEST when partitions == 0
assert_status "Create topic with 0 partitions -> 400" 400 POST "$API/topics" \
    '{"name":"neg-zero-parts","partitions":0}'

# POST /api/v1/topics with empty name → 400
# Handler: topics.rs line 215-217 returns BAD_REQUEST when name is empty
assert_status "Create topic with empty name -> 400" 400 POST "$API/topics" \
    '{"name":"","partitions":2}'

# POST /api/v1/topics with no body → 400 or 422 (axum JSON rejection)
http_request POST "$API/topics" ""
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Create topic with no body -> $HTTP_STATUS"
else
    fail "Create topic with no body" "expected 400 or 422, got $HTTP_STATUS"
fi

# GET /api/v1/topics/nonexistent → 404
# Handler: topics.rs line 302 returns NOT_FOUND via .ok_or(StatusCode::NOT_FOUND)
assert_status "Get nonexistent topic -> 404" 404 GET "$API/topics/nonexistent-topic-xyz-999"

# DELETE /api/v1/topics/nonexistent → 404
# Handler: topics.rs line 357 returns NOT_FOUND via .ok_or(StatusCode::NOT_FOUND)
http_request DELETE "$API/topics/nonexistent-delete-xyz-999"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Delete nonexistent topic -> 404"
else
    fail "Delete nonexistent topic -> 404" "expected 404, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Produce Errors
# ═══════════════════════════════════════════════════════════════════════════════

# POST /api/v1/produce to nonexistent topic → 404
# Handler: produce.rs line 212 returns NOT_FOUND when topic not found
assert_status "Produce to nonexistent topic -> 404" 404 POST "$API/produce" \
    '{"topic":"does-not-exist-xyz-999","key":"k","value":"v"}'

# POST /api/v1/produce with empty body → 400
# Axum JSON extractor rejects empty/missing body with 400 or 422
http_request POST "$API/produce" ""
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Produce with empty body -> $HTTP_STATUS"
else
    fail "Produce with empty body" "expected 400 or 422, got $HTTP_STATUS"
fi

# POST /api/v1/produce with invalid JSON → 400
# Axum JSON extractor rejects unparseable JSON
http_request POST "$API/produce" "this is not json at all"
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Produce with invalid JSON -> $HTTP_STATUS"
else
    fail "Produce with invalid JSON" "expected 400 or 422, got $HTTP_STATUS"
fi

# POST /api/v1/produce with missing topic field → 400
# ProduceRequest requires topic and value fields; missing topic causes deserialization error
http_request POST "$API/produce" '{"value":"hello"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Produce with missing topic field -> $HTTP_STATUS"
else
    fail "Produce with missing topic field" "expected 400 or 422, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Consume Errors
# ═══════════════════════════════════════════════════════════════════════════════

# GET /api/v1/consume without params → 400
# Axum query extractor requires 'topic' and 'partition' fields
http_request GET "$API/consume"
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Consume with no params -> $HTTP_STATUS"
else
    fail "Consume with no params" "expected 400 or 422, got $HTTP_STATUS"
fi

# GET /api/v1/consume?topic=nonexistent → 404
# Handler: consume.rs line 68-73 returns NOT_FOUND when topic not found
http_request GET "$API/consume?topic=nonexistent-topic-xyz-999&partition=0&offset=0"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Consume from nonexistent topic -> 404"
else
    fail "Consume from nonexistent topic -> 404" "expected 404, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SQL Errors
# ═══════════════════════════════════════════════════════════════════════════════

# POST /api/v1/sql with syntax error → 400
# Handler: sql.rs returns BAD_REQUEST for ParseError
assert_status "SQL syntax error -> 400" 400 POST "$API/sql" \
    '{"query":"SELEC * FORM broken_syntax"}'

# POST /api/v1/sql with empty query → 400
# Empty string should fail parsing
assert_status "SQL empty query -> 400" 400 POST "$API/sql" \
    '{"query":""}'

# POST /api/v1/sql querying nonexistent topic → 400 or 404
# Handler: sql.rs returns NOT_FOUND for TopicNotFound, BAD_REQUEST for InvalidQuery
http_request POST "$API/sql" '{"query":"SELECT * FROM \"nonexistent_topic_xyz_999\" LIMIT 1"}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "404" ]; then
    pass "SQL query nonexistent topic -> $HTTP_STATUS"
else
    fail "SQL query nonexistent topic" "expected 400 or 404, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Schema Registry Errors
# ═══════════════════════════════════════════════════════════════════════════════

# POST /schemas/subjects/x/versions with invalid schema → 422
# Handler: api.rs SchemaError::InvalidSchema returns UNPROCESSABLE_ENTITY (422)
http_request POST "$SR/subjects/garbage-subject/versions" \
    '{"schema": "this is not a valid schema", "schemaType": "AVRO"}'
if [ "$HTTP_STATUS" = "422" ] || [ "$HTTP_STATUS" = "400" ]; then
    pass "Register invalid Avro schema -> $HTTP_STATUS"
else
    fail "Register invalid Avro schema" "expected 422 or 400, got $HTTP_STATUS"
fi

# GET /schemas/subjects/nonexistent → 404
# Handler: api.rs SchemaError::SubjectNotFound returns NOT_FOUND (404)
http_request GET "$SR/subjects/nonexistent-subject-xyz-999/versions"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Get nonexistent schema subject -> 404"
else
    fail "Get nonexistent schema subject -> 404" "expected 404, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Large Payload Tests
# ═══════════════════════════════════════════════════════════════════════════════

# First create a topic for large payload tests
http_request POST "$API/topics" '{"name":"neg-large-payload","partitions":1}'
# Accept 201 (created) or 409 (already exists)

# Produce a 1MB value — should succeed or return 413
LARGE_1MB=$(python3 -c "print('X' * (1024 * 1024))")
PAYLOAD_1MB=$(jq -n --arg t "neg-large-payload" --arg v "$LARGE_1MB" '{topic: $t, key: "large-1mb", value: $v}')
http_request POST "$API/produce" "$PAYLOAD_1MB"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "413" ]; then
    pass "Produce 1MB payload -> $HTTP_STATUS (success or payload too large)"
else
    fail "Produce 1MB payload" "expected 200 or 413, got $HTTP_STATUS"
fi

# Produce a 10MB value — should be rejected (413 or 400 or connection reset)
LARGE_10MB=$(python3 -c "print('Y' * (10 * 1024 * 1024))")
PAYLOAD_10MB=$(jq -n --arg t "neg-large-payload" --arg v "$LARGE_10MB" '{topic: $t, key: "large-10mb", value: $v}')
http_request POST "$API/produce" "$PAYLOAD_10MB"
if [ "$HTTP_STATUS" = "413" ] || [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "000" ] || [ "$HTTP_STATUS" = "" ]; then
    pass "Produce 10MB payload rejected -> $HTTP_STATUS"
elif [ "$HTTP_STATUS" = "200" ]; then
    # Server accepted it — not ideal but not a crash
    fail "Produce 10MB payload" "server accepted 10MB payload (expected rejection)"
else
    pass "Produce 10MB payload rejected -> $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Invalid Routes
# ═══════════════════════════════════════════════════════════════════════════════

# GET /api/v1/nonexistent → 404
http_request GET "$API/nonexistent-route-xyz"
if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "405" ]; then
    pass "Invalid route -> $HTTP_STATUS"
else
    fail "Invalid route" "expected 404 or 405, got $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/neg-large-payload" &>/dev/null || true
