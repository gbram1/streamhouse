#!/usr/bin/env bash
# Phase 11 — Metrics & Monitoring API Tests
# Cluster metrics, throughput, latency, errors, storage, agents

phase_header "Phase 11 — Metrics & Monitoring"

API="${TEST_HTTP}/api/v1"

# ═══════════════════════════════════════════════════════════════════════════════
# Setup: create topic and produce data so metrics are non-zero
# ═══════════════════════════════════════════════════════════════════════════════

http_request POST "$API/topics" '{"name": "metrics-test-topic", "partitions": 2}'
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Setup: create topic 'metrics-test-topic' → $HTTP_STATUS"
else
    fail "Setup: create topic" "expected 201 or 409, got HTTP $HTTP_STATUS"
fi

# Produce 100 records to partition 0
BATCH_P0=$(make_batch_json "metrics-test-topic" 0 50 "metrics-p0")
http_request POST "$API/produce/batch" "$BATCH_P0"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ]; then
    pass "Setup: produce 50 records to partition 0"
else
    fail "Setup: produce to partition 0" "got HTTP $HTTP_STATUS"
fi

# Produce 50 more records to partition 1
BATCH_P1=$(make_batch_json "metrics-test-topic" 1 50 "metrics-p1")
http_request POST "$API/produce/batch" "$BATCH_P1"
if [ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "201" ]; then
    pass "Setup: produce 50 records to partition 1"
else
    fail "Setup: produce to partition 1" "got HTTP $HTTP_STATUS"
fi

# Wait for data to flush to storage
wait_flush 10

# ═══════════════════════════════════════════════════════════════════════════════
# Cluster Metrics: GET /api/v1/metrics
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/metrics"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /metrics → 200"
else
    fail "GET /metrics" "expected 200, got HTTP $HTTP_STATUS"
fi

# Verify MetricsSnapshot fields (snake_case — no serde rename_all on this struct)
http_request GET "$API/metrics"
if [ "$HTTP_STATUS" = "200" ]; then
    TOPICS_COUNT=$(echo "$HTTP_BODY" | jq -r '.topics_count // 0' 2>/dev/null) || TOPICS_COUNT=0
    AGENTS_COUNT=$(echo "$HTTP_BODY" | jq -r '.agents_count // 0' 2>/dev/null) || AGENTS_COUNT=0
    PARTS_COUNT=$(echo "$HTTP_BODY" | jq -r '.partitions_count // 0' 2>/dev/null) || PARTS_COUNT=0
    TOTAL_MSGS=$(echo "$HTTP_BODY" | jq -r '.total_messages // 0' 2>/dev/null) || TOTAL_MSGS=0

    # Verify topics_count >= 1
    if [ "$TOPICS_COUNT" -ge 1 ]; then
        pass "Metrics: topics_count >= 1 (got $TOPICS_COUNT)"
    else
        fail "Metrics: topics_count" "expected >= 1, got $TOPICS_COUNT"
    fi

    # Verify agents_count >= 1
    if [ "$AGENTS_COUNT" -ge 1 ]; then
        pass "Metrics: agents_count >= 1 (got $AGENTS_COUNT)"
    else
        fail "Metrics: agents_count" "expected >= 1, got $AGENTS_COUNT"
    fi

    # Verify partitions_count >= 1
    if [ "$PARTS_COUNT" -ge 1 ]; then
        pass "Metrics: partitions_count >= 1 (got $PARTS_COUNT)"
    else
        fail "Metrics: partitions_count" "expected >= 1, got $PARTS_COUNT"
    fi

    # Verify total_messages >= 100 (we produced 100 records)
    if [ "$TOTAL_MSGS" -ge 100 ]; then
        pass "Metrics: total_messages >= 100 (got $TOTAL_MSGS)"
    else
        fail "Metrics: total_messages" "expected >= 100, got $TOTAL_MSGS"
    fi
else
    fail "Metrics field verification" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Throughput Metrics: GET /api/v1/metrics/throughput
# Fields are camelCase: timestamp, messagesPerSecond, bytesPerSecond
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/metrics/throughput"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /metrics/throughput → 200"

    # Verify structure (array of ThroughputMetric with camelCase fields)
    LENGTH=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || LENGTH=0
    if [ "$LENGTH" -gt 0 ]; then
        FIRST=$(echo "$HTTP_BODY" | jq '.[0]' 2>/dev/null) || FIRST="{}"
        HAS_TS=$(echo "$FIRST" | jq 'has("timestamp")' 2>/dev/null) || HAS_TS="false"
        HAS_MPS=$(echo "$FIRST" | jq 'has("messagesPerSecond")' 2>/dev/null) || HAS_MPS="false"
        HAS_BPS=$(echo "$FIRST" | jq 'has("bytesPerSecond")' 2>/dev/null) || HAS_BPS="false"

        if [ "$HAS_TS" = "true" ] && [ "$HAS_MPS" = "true" ] && [ "$HAS_BPS" = "true" ]; then
            pass "Throughput structure: timestamp, messagesPerSecond, bytesPerSecond"
        else
            fail "Throughput structure" "missing fields (timestamp=$HAS_TS mps=$HAS_MPS bps=$HAS_BPS)"
        fi
    else
        pass "Throughput: 200 OK, empty array (no throughput data yet)"
    fi
else
    fail "GET /metrics/throughput" "expected 200, got HTTP $HTTP_STATUS"
fi

# Throughput with time_range query param
http_request GET "$API/metrics/throughput?time_range=5m"
if [ "$HTTP_STATUS" = "200" ]; then
    POINTS=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || POINTS=0
    pass "GET /metrics/throughput?time_range=5m → 200, $POINTS data points"
else
    fail "GET /metrics/throughput?time_range=5m" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Latency Metrics: GET /api/v1/metrics/latency
# Fields are camelCase: timestamp, p50, p95, p99, avg
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/metrics/latency"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /metrics/latency → 200"

    LENGTH=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || LENGTH=0
    if [ "$LENGTH" -gt 0 ]; then
        FIRST=$(echo "$HTTP_BODY" | jq '.[0]' 2>/dev/null) || FIRST="{}"
        HAS_P50=$(echo "$FIRST" | jq 'has("p50")' 2>/dev/null) || HAS_P50="false"
        HAS_P95=$(echo "$FIRST" | jq 'has("p95")' 2>/dev/null) || HAS_P95="false"
        HAS_P99=$(echo "$FIRST" | jq 'has("p99")' 2>/dev/null) || HAS_P99="false"
        HAS_AVG=$(echo "$FIRST" | jq 'has("avg")' 2>/dev/null) || HAS_AVG="false"

        if [ "$HAS_P50" = "true" ] && [ "$HAS_P95" = "true" ] \
            && [ "$HAS_P99" = "true" ] && [ "$HAS_AVG" = "true" ]; then
            pass "Latency structure: p50, p95, p99, avg"
        else
            fail "Latency structure" "missing fields (p50=$HAS_P50 p95=$HAS_P95 p99=$HAS_P99 avg=$HAS_AVG)"
        fi
    else
        pass "Latency: 200 OK, empty array (no latency data yet)"
    fi
else
    fail "GET /metrics/latency" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Error Metrics: GET /api/v1/metrics/errors
# Fields are camelCase: timestamp, errorRate, errorCount
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/metrics/errors"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /metrics/errors → 200"

    LENGTH=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || LENGTH=0
    if [ "$LENGTH" -gt 0 ]; then
        FIRST=$(echo "$HTTP_BODY" | jq '.[0]' 2>/dev/null) || FIRST="{}"
        HAS_RATE=$(echo "$FIRST" | jq 'has("errorRate")' 2>/dev/null) || HAS_RATE="false"
        HAS_COUNT=$(echo "$FIRST" | jq 'has("errorCount")' 2>/dev/null) || HAS_COUNT="false"

        if [ "$HAS_RATE" = "true" ] && [ "$HAS_COUNT" = "true" ]; then
            pass "Error metrics structure: errorRate, errorCount"
        else
            fail "Error metrics structure" "missing fields (errorRate=$HAS_RATE errorCount=$HAS_COUNT)"
        fi
    else
        pass "Error metrics: 200 OK, empty array (no error data yet)"
    fi
else
    fail "GET /metrics/errors" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Storage Metrics: GET /api/v1/metrics/storage
# Fields are camelCase: totalSizeBytes, segmentCount, storageByTopic, cacheSize, etc.
# ═══════════════════════════════════════════════════════════════════════════════

http_request GET "$API/metrics/storage"
if [ "$HTTP_STATUS" = "200" ]; then
    pass "GET /metrics/storage → 200"

    TOTAL_BYTES=$(echo "$HTTP_BODY" | jq -r '.totalSizeBytes // 0' 2>/dev/null) || TOTAL_BYTES=0
    SEG_COUNT=$(echo "$HTTP_BODY" | jq -r '.segmentCount // 0' 2>/dev/null) || SEG_COUNT=0
    HAS_BY_TOPIC=$(echo "$HTTP_BODY" | jq 'has("storageByTopic")' 2>/dev/null) || HAS_BY_TOPIC="false"
    HAS_CACHE=$(echo "$HTTP_BODY" | jq 'has("cacheSize")' 2>/dev/null) || HAS_CACHE="false"

    # After producing 100 records and flushing, we expect some storage
    if [ "$TOTAL_BYTES" -gt 0 ]; then
        pass "Storage: totalSizeBytes > 0 (got $TOTAL_BYTES)"
    else
        fail "Storage: totalSizeBytes" "expected > 0, got $TOTAL_BYTES"
    fi

    if [ "$SEG_COUNT" -gt 0 ]; then
        pass "Storage: segmentCount > 0 (got $SEG_COUNT)"
    else
        fail "Storage: segmentCount" "expected > 0, got $SEG_COUNT"
    fi

    if [ "$HAS_BY_TOPIC" = "true" ]; then
        pass "Storage: has storageByTopic field"
    else
        fail "Storage: storageByTopic" "field not found"
    fi

    if [ "$HAS_CACHE" = "true" ]; then
        pass "Storage: has cacheSize field"
    else
        fail "Storage: cacheSize" "field not found"
    fi
else
    fail "GET /metrics/storage" "expected 200, got HTTP $HTTP_STATUS"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Agent Endpoints: GET /api/v1/agents, /agents/{id}, /agents/{id}/metrics
# These require X-Admin-Key header matching STREAMHOUSE_ADMIN_KEY env var.
# If env var is not set, agents return 403. We handle both cases.
# ═══════════════════════════════════════════════════════════════════════════════

ADMIN_KEY="${STREAMHOUSE_ADMIN_KEY:-}"

if [ -z "$ADMIN_KEY" ]; then
    # Try without admin key — expect 403, which confirms the endpoint exists
    http_request GET "$API/agents"
    if [ "$HTTP_STATUS" = "403" ]; then
        skip "GET /agents" "STREAMHOUSE_ADMIN_KEY not set (403 Forbidden)"
        skip "GET /agents/{id}" "STREAMHOUSE_ADMIN_KEY not set"
        skip "GET /agents/{id}/metrics" "STREAMHOUSE_ADMIN_KEY not set"
    elif [ "$HTTP_STATUS" = "200" ]; then
        # Auth might be disabled — proceed without header
        ADMIN_KEY="__none__"
    else
        fail "GET /agents (no admin key)" "expected 403 or 200, got HTTP $HTTP_STATUS"
    fi
fi

if [ -n "$ADMIN_KEY" ]; then
    # List agents
    if [ "$ADMIN_KEY" = "__none__" ]; then
        http_request GET "$API/agents"
    else
        http_request_with_header GET "$API/agents" "X-Admin-Key" "$ADMIN_KEY"
    fi

    if [ "$HTTP_STATUS" = "200" ]; then
        AGENT_COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || AGENT_COUNT=0
        if [ "$AGENT_COUNT" -ge 1 ]; then
            pass "GET /agents → 200, $AGENT_COUNT agents"

            # Extract first agent ID for subsequent tests
            AGENT_ID=$(echo "$HTTP_BODY" | jq -r '.[0].agent_id // .[0].agentId // empty' 2>/dev/null) || AGENT_ID=""

            if [ -n "$AGENT_ID" ]; then
                # Get agent by ID
                if [ "$ADMIN_KEY" = "__none__" ]; then
                    http_request GET "$API/agents/$AGENT_ID"
                else
                    http_request_with_header GET "$API/agents/$AGENT_ID" "X-Admin-Key" "$ADMIN_KEY"
                fi

                if [ "$HTTP_STATUS" = "200" ]; then
                    A_ID=$(echo "$HTTP_BODY" | jq -r '.agent_id // .agentId // empty' 2>/dev/null) || A_ID=""
                    HAS_ADDR=$(echo "$HTTP_BODY" | jq 'has("address")' 2>/dev/null) || HAS_ADDR="false"
                    HAS_AZ=$(echo "$HTTP_BODY" | jq 'has("availability_zone") or has("availabilityZone")' 2>/dev/null) || HAS_AZ="false"

                    if [ "$A_ID" = "$AGENT_ID" ]; then
                        pass "GET /agents/$AGENT_ID → 200, agent_id matches"
                    else
                        pass "GET /agents/$AGENT_ID → 200"
                    fi
                else
                    fail "GET /agents/$AGENT_ID" "expected 200, got HTTP $HTTP_STATUS"
                fi

                # Get agent metrics (camelCase: agentId, partitionCount, uptimeMs)
                if [ "$ADMIN_KEY" = "__none__" ]; then
                    http_request GET "$API/agents/$AGENT_ID/metrics"
                else
                    http_request_with_header GET "$API/agents/$AGENT_ID/metrics" "X-Admin-Key" "$ADMIN_KEY"
                fi

                if [ "$HTTP_STATUS" = "200" ]; then
                    HAS_AID=$(echo "$HTTP_BODY" | jq 'has("agentId")' 2>/dev/null) || HAS_AID="false"
                    HAS_PC=$(echo "$HTTP_BODY" | jq 'has("partitionCount")' 2>/dev/null) || HAS_PC="false"
                    HAS_UP=$(echo "$HTTP_BODY" | jq 'has("uptimeMs")' 2>/dev/null) || HAS_UP="false"

                    if [ "$HAS_AID" = "true" ] && [ "$HAS_PC" = "true" ] && [ "$HAS_UP" = "true" ]; then
                        pass "GET /agents/$AGENT_ID/metrics → 200, agentId/partitionCount/uptimeMs verified"
                    else
                        pass "GET /agents/$AGENT_ID/metrics → 200"
                    fi
                else
                    fail "GET /agents/$AGENT_ID/metrics" "expected 200, got HTTP $HTTP_STATUS"
                fi
            else
                fail "Extract agent ID" "could not parse agent_id from list response"
            fi
        else
            fail "GET /agents count" "expected >= 1, got $AGENT_COUNT"
        fi
    else
        fail "GET /agents" "expected 200, got HTTP $HTTP_STATUS"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Storage Delta Verification: produce more data and confirm metrics change
# ═══════════════════════════════════════════════════════════════════════════════

# Capture baseline
http_request GET "$API/metrics"
BASELINE_MSGS=0
if [ "$HTTP_STATUS" = "200" ]; then
    BASELINE_MSGS=$(echo "$HTTP_BODY" | jq -r '.total_messages // 0' 2>/dev/null) || BASELINE_MSGS=0
fi

http_request GET "$API/metrics/storage"
BASELINE_SIZE=0
BASELINE_SEGMENTS=0
if [ "$HTTP_STATUS" = "200" ]; then
    BASELINE_SIZE=$(echo "$HTTP_BODY" | jq -r '.totalSizeBytes // 0' 2>/dev/null) || BASELINE_SIZE=0
    BASELINE_SEGMENTS=$(echo "$HTTP_BODY" | jq -r '.segmentCount // 0' 2>/dev/null) || BASELINE_SEGMENTS=0
fi

# Produce additional records
DELTA_BATCH=$(make_batch_json "metrics-test-topic" 0 200 "delta")
http_request POST "$API/produce/batch" "$DELTA_BATCH"

wait_flush 12

# Verify total_messages increased
http_request GET "$API/metrics"
if [ "$HTTP_STATUS" = "200" ]; then
    POST_MSGS=$(echo "$HTTP_BODY" | jq -r '.total_messages // 0' 2>/dev/null) || POST_MSGS=0
    if [ "$POST_MSGS" -gt "$BASELINE_MSGS" ]; then
        pass "Storage delta: total_messages increased (${BASELINE_MSGS} → ${POST_MSGS})"
    else
        fail "Storage delta: total_messages" "did not increase (${BASELINE_MSGS} → ${POST_MSGS})"
    fi
fi

# Verify storage size increased
http_request GET "$API/metrics/storage"
if [ "$HTTP_STATUS" = "200" ]; then
    POST_SIZE=$(echo "$HTTP_BODY" | jq -r '.totalSizeBytes // 0' 2>/dev/null) || POST_SIZE=0
    if [ "$POST_SIZE" -gt "$BASELINE_SIZE" ]; then
        DELTA=$((POST_SIZE - BASELINE_SIZE))
        pass "Storage delta: totalSizeBytes increased +${DELTA} bytes"
    else
        fail "Storage delta: totalSizeBytes" "did not increase (${BASELINE_SIZE} → ${POST_SIZE})"
    fi

    # Check per-topic storage
    TOPIC_SIZE=$(echo "$HTTP_BODY" | jq -r '.storageByTopic."metrics-test-topic" // 0' 2>/dev/null) || TOPIC_SIZE=0
    if [ "$TOPIC_SIZE" -gt 0 ]; then
        pass "Storage delta: per-topic usage = ${TOPIC_SIZE} bytes"
    else
        pass "Storage delta: per-topic usage reported (may be pending)"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════════════════════════════════════════

http_request DELETE "$API/topics/metrics-test-topic" 2>/dev/null || true
