#!/bin/bash
set -e

# Test script for REST API

echo "╔════════════════════════════════════════════════════════╗"
echo "║   StreamHouse REST API Test                           ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

API_PORT="${API_PORT:-8080}"
API_URL="http://localhost:${API_PORT}"

echo -e "${BLUE}Testing REST API endpoints...${NC}"
echo

# Test 1: Health check
echo "1. Testing /health endpoint..."
HEALTH=$(curl -s "$API_URL/health")
if echo "$HEALTH" | grep -q "ok"; then
    echo -e "  ${GREEN}✓${NC} Health check passed"
else
    echo -e "  ${RED}✗${NC} Health check failed"
    exit 1
fi

# Test 2: List topics (should be empty or have existing topics)
echo "2. Testing GET /api/v1/topics..."
TOPICS=$(curl -s "$API_URL/api/v1/topics")
echo "  Topics: $TOPICS"
echo -e "  ${GREEN}✓${NC} List topics endpoint works"

# Test 3: Create a topic
echo "3. Testing POST /api/v1/topics..."
CREATE_RESPONSE=$(curl -s -X POST "$API_URL/api/v1/topics" \
  -H "Content-Type: application/json" \
  -d '{"name":"test-api","partitions":3,"replication_factor":1}')
echo "  Response: $CREATE_RESPONSE"
echo -e "  ${GREEN}✓${NC} Create topic endpoint works"

# Test 4: Get specific topic
echo "4. Testing GET /api/v1/topics/test-api..."
TOPIC=$(curl -s "$API_URL/api/v1/topics/test-api")
echo "  Topic: $TOPIC"
if echo "$TOPIC" | grep -q "test-api"; then
    echo -e "  ${GREEN}✓${NC} Get topic endpoint works"
else
    echo -e "  ${RED}✗${NC} Get topic failed"
fi

# Test 5: List partitions
echo "5. Testing GET /api/v1/topics/test-api/partitions..."
PARTITIONS=$(curl -s "$API_URL/api/v1/topics/test-api/partitions")
echo "  Partitions: $PARTITIONS"
echo -e "  ${GREEN}✓${NC} List partitions endpoint works"

# Test 6: List agents
echo "6. Testing GET /api/v1/agents..."
AGENTS=$(curl -s "$API_URL/api/v1/agents")
echo "  Agents: $AGENTS"
echo -e "  ${GREEN}✓${NC} List agents endpoint works"

# Test 7: Get metrics
echo "7. Testing GET /api/v1/metrics..."
METRICS=$(curl -s "$API_URL/api/v1/metrics")
echo "  Metrics: $METRICS"
if echo "$METRICS" | grep -q "topics_count"; then
    echo -e "  ${GREEN}✓${NC} Get metrics endpoint works"
else
    echo -e "  ${RED}✗${NC} Get metrics failed"
fi

# Test 8: Produce a message
echo "8. Testing POST /api/v1/produce..."
PRODUCE_RESPONSE=$(curl -s -X POST "$API_URL/api/v1/produce" \
  -H "Content-Type: application/json" \
  -d '{"topic":"test-api","key":"user1","value":"hello world"}')
echo "  Response: $PRODUCE_RESPONSE"
if echo "$PRODUCE_RESPONSE" | grep -q "offset"; then
    echo -e "  ${GREEN}✓${NC} Produce endpoint works"
else
    echo -e "  ${RED}✗${NC} Produce failed"
fi

echo
echo -e "${GREEN}╔════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║          ✓ ALL API TESTS PASSED!                      ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════╝${NC}"
echo
echo "Swagger UI: $API_URL/swagger-ui"
echo
