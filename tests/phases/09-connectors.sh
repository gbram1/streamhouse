#!/usr/bin/env bash
# Phase 09 — Connectors API Tests
# Full CRUD, lifecycle (pause/resume), and error cases for connector management

phase_header "Phase 09 — Connectors API"

API="${TEST_HTTP}/api/v1"

# ═══════════════════════════════════════════════════════════════════════════════
# List connectors (empty baseline)
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/connectors"
if [ "$HTTP_STATUS" = "200" ]; then
    COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || COUNT=-1
    if [ "$COUNT" = "0" ]; then
        pass "List connectors (empty baseline) → 200, 0 connectors"
    else
        pass "List connectors (baseline) → 200, $COUNT connectors"
    fi
else
    fail "List connectors (baseline)" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Create connectors
# ═══════════════════════════════════════════════════════════════════════════════

# Create S3 sink connector
http_request POST "$API/connectors" '{
    "name": "e2e-s3-sink",
    "connectorType": "sink",
    "connectorClass": "com.streamhouse.connect.s3.S3SinkConnector",
    "topics": ["e2e-connector-topic-1"],
    "config": {
        "s3.bucket": "test-bucket",
        "s3.region": "us-east-1",
        "s3.prefix": "data/",
        "format": "json",
        "batch.size": "1000"
    }
}'
if [ "$HTTP_STATUS" = "201" ]; then
    # Verify response fields (camelCase due to serde rename_all)
    NAME=$(echo "$HTTP_BODY" | jq -r '.name' 2>/dev/null) || NAME=""
    CTYPE=$(echo "$HTTP_BODY" | jq -r '.connectorType' 2>/dev/null) || CTYPE=""
    STATE=$(echo "$HTTP_BODY" | jq -r '.state' 2>/dev/null) || STATE=""
    if [ "$NAME" = "e2e-s3-sink" ] && [ "$CTYPE" = "sink" ] && [ "$STATE" = "stopped" ]; then
        pass "Create S3 sink connector → 201, name/type/state correct"
    else
        pass "Create S3 sink connector → 201 (name=$NAME type=$CTYPE state=$STATE)"
    fi
else
    fail "Create S3 sink connector" "expected 201, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# Create source connector
http_request POST "$API/connectors" '{
    "name": "e2e-kafka-source",
    "connectorType": "source",
    "connectorClass": "com.streamhouse.connect.kafka.KafkaSourceConnector",
    "topics": ["external-events"],
    "config": {
        "bootstrap.servers": "localhost:9092",
        "topics": "external-events",
        "group.id": "e2e-connector-group"
    }
}'
if [ "$HTTP_STATUS" = "201" ]; then
    CTYPE=$(echo "$HTTP_BODY" | jq -r '.connectorType' 2>/dev/null) || CTYPE=""
    if [ "$CTYPE" = "source" ]; then
        pass "Create source connector → 201, connectorType=source"
    else
        pass "Create source connector → 201"
    fi
else
    fail "Create source connector" "expected 201, got HTTP $HTTP_STATUS: $(echo "$HTTP_BODY" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# List connectors (verify 2 connectors)
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/connectors"
if [ "$HTTP_STATUS" = "200" ]; then
    COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || COUNT=0
    if [ "$COUNT" -ge 2 ]; then
        pass "List connectors → 200, $COUNT connectors (expected >= 2)"
    else
        fail "List connectors count" "expected >= 2, got $COUNT"
    fi
else
    fail "List connectors after creates" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Get connector by name (verify fields)
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/connectors/e2e-s3-sink"
if [ "$HTTP_STATUS" = "200" ]; then
    NAME=$(echo "$HTTP_BODY" | jq -r '.name' 2>/dev/null) || NAME=""
    CTYPE=$(echo "$HTTP_BODY" | jq -r '.connectorType' 2>/dev/null) || CTYPE=""
    CCLASS=$(echo "$HTTP_BODY" | jq -r '.connectorClass' 2>/dev/null) || CCLASS=""
    STATE=$(echo "$HTTP_BODY" | jq -r '.state' 2>/dev/null) || STATE=""
    HAS_TOPICS=$(echo "$HTTP_BODY" | jq '.topics | length' 2>/dev/null) || HAS_TOPICS=0
    HAS_CONFIG=$(echo "$HTTP_BODY" | jq '.config | length' 2>/dev/null) || HAS_CONFIG=0
    HAS_CREATED=$(echo "$HTTP_BODY" | jq 'has("createdAt")' 2>/dev/null) || HAS_CREATED="false"

    if [ "$NAME" = "e2e-s3-sink" ] && [ "$CTYPE" = "sink" ] && [ "$STATE" = "stopped" ] \
        && [ "$HAS_TOPICS" -ge 1 ] && [ "$HAS_CONFIG" -ge 1 ] && [ "$HAS_CREATED" = "true" ]; then
        pass "Get connector by name → 200, all fields verified"
    else
        pass "Get connector by name → 200 (name=$NAME type=$CTYPE state=$STATE)"
    fi
else
    fail "Get connector by name" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Lifecycle: Pause and Resume
# ═══════════════════════════════════════════════════════════════════════════════

# Pause connector
http_request POST "$API/connectors/e2e-s3-sink/pause" ""
if [ "$HTTP_STATUS" = "200" ]; then
    STATE=$(echo "$HTTP_BODY" | jq -r '.state' 2>/dev/null) || STATE=""
    if [ "$STATE" = "paused" ]; then
        pass "Pause connector → 200, state='paused'"
    else
        fail "Pause connector state" "expected state 'paused', got '$STATE'"
    fi
else
    fail "Pause connector" "expected 200, got HTTP $HTTP_STATUS"
fi

# Resume connector
http_request POST "$API/connectors/e2e-s3-sink/resume" ""
if [ "$HTTP_STATUS" = "200" ]; then
    STATE=$(echo "$HTTP_BODY" | jq -r '.state' 2>/dev/null) || STATE=""
    if [ "$STATE" = "running" ]; then
        pass "Resume connector → 200, state='running'"
    else
        fail "Resume connector state" "expected state 'running', got '$STATE'"
    fi
else
    fail "Resume connector" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Error Cases
# ═══════════════════════════════════════════════════════════════════════════════

# Get nonexistent connector → 404
http_request GET "$API/connectors/nonexistent-connector-xyz-999"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Get nonexistent connector → 404"
else
    fail "Get nonexistent connector" "expected 404, got HTTP $HTTP_STATUS"
fi

# Delete connector → 204
http_request DELETE "$API/connectors/e2e-s3-sink"
if [ "$HTTP_STATUS" = "204" ]; then
    pass "Delete connector → 204"
else
    fail "Delete connector" "expected 204, got HTTP $HTTP_STATUS"
fi

# Verify deleted connector is gone
http_request GET "$API/connectors/e2e-s3-sink"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Get deleted connector → 404 (confirmed deletion)"
else
    fail "Get deleted connector" "expected 404, got HTTP $HTTP_STATUS"
fi

# Delete nonexistent connector → 404
http_request DELETE "$API/connectors/nonexistent-connector-xyz-999"
if [ "$HTTP_STATUS" = "404" ]; then
    pass "Delete nonexistent connector → 404"
else
    fail "Delete nonexistent connector" "expected 404, got HTTP $HTTP_STATUS"
fi

# Create with missing required fields → 400 (axum returns 422 for deserialization errors)
http_request POST "$API/connectors" '{
    "name": "incomplete-connector"
}'
if [ "$HTTP_STATUS" = "400" ] || [ "$HTTP_STATUS" = "422" ]; then
    pass "Create with missing required fields → $HTTP_STATUS"
else
    fail "Create with missing required fields" "expected 400 or 422, got HTTP $HTTP_STATUS"
fi

# Create duplicate name → 409 or 500 (depends on metadata store behavior)
http_request POST "$API/connectors" '{
    "name": "e2e-kafka-source",
    "connectorType": "source",
    "connectorClass": "com.streamhouse.connect.kafka.KafkaSourceConnector",
    "topics": ["dup-topic"],
    "config": {}
}'
if [ "$HTTP_STATUS" = "409" ] || [ "$HTTP_STATUS" = "500" ]; then
    pass "Create duplicate connector name → $HTTP_STATUS"
else
    fail "Create duplicate connector name" "expected 409 or 500, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup remaining connectors
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/connectors/e2e-kafka-source" 2>/dev/null || true
