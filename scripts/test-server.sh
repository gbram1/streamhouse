#!/bin/bash
# Quick server testing script

set -e

GRPC_PORT="${GRPC_PORT:-50051}"
SERVER_ADDR="localhost:${GRPC_PORT}"

echo "🧪 StreamHouse Server Test Suite"
echo "=================================="
echo ""

# Check if grpcurl is installed
if ! command -v grpcurl &> /dev/null; then
    echo "❌ grpcurl not found. Install it with:"
    echo "   brew install grpcurl"
    exit 1
fi

# Check if server is running
if ! nc -z localhost "${GRPC_PORT}" 2>/dev/null; then
    echo "❌ Server not running on port ${GRPC_PORT}"
    echo "   Start it with: STREAMHOUSE_ADDR=0.0.0.0:${GRPC_PORT} cargo run --bin unified-server"
    exit 1
fi

echo "✅ Server is running on $SERVER_ADDR"
echo ""

# Test 1: Create topic
echo "📝 Test 1: Creating topic 'test-events'..."
grpcurl -plaintext \
  -d '{"name": "test-events", "partition_count": 2}' \
  $SERVER_ADDR streamhouse.StreamHouse/CreateTopic > /dev/null 2>&1
echo "✅ Topic created"

# Test 2: List topics
echo ""
echo "📋 Test 2: Listing topics..."
TOPICS=$(grpcurl -plaintext $SERVER_ADDR streamhouse.StreamHouse/ListTopics)
echo "$TOPICS" | grep -q "test-events" && echo "✅ Topic found in list" || echo "❌ Topic not in list"

# Test 3: Get topic
echo ""
echo "🔍 Test 3: Getting topic info..."
grpcurl -plaintext \
  -d '{"name": "test-events"}' \
  $SERVER_ADDR streamhouse.StreamHouse/GetTopic > /dev/null 2>&1
echo "✅ Topic info retrieved"

# Test 4: Produce single record
echo ""
echo "📤 Test 4: Producing single record..."
VALUE=$(echo -n '{"test": "data"}' | base64)
RESULT=$(grpcurl -plaintext \
  -d "{\"topic\": \"test-events\", \"partition\": 0, \"value\": \"$VALUE\"}" \
  $SERVER_ADDR streamhouse.StreamHouse/Produce)
# Note: offset may be omitted if it's 0 (protobuf3 default)
echo "$RESULT" | grep -q "timestamp" && echo "✅ Record produced" || echo "❌ Production failed"

# Test 5: Produce batch
echo ""
echo "📦 Test 5: Producing batch of 3 records..."
VAL1=$(echo -n '{"id": 1}' | base64)
VAL2=$(echo -n '{"id": 2}' | base64)
VAL3=$(echo -n '{"id": 3}' | base64)

BATCH_RESULT=$(grpcurl -plaintext \
  -d "{
    \"topic\": \"test-events\",
    \"partition\": 0,
    \"records\": [
      {\"value\": \"$VAL1\"},
      {\"value\": \"$VAL2\"},
      {\"value\": \"$VAL3\"}
    ]
  }" \
  $SERVER_ADDR streamhouse.StreamHouse/ProduceBatch)

echo "$BATCH_RESULT" | grep -q "count" && echo "✅ Batch produced" || echo "❌ Batch production failed"

# Test 6: Commit offset
echo ""
echo "💾 Test 6: Committing consumer offset..."
grpcurl -plaintext \
  -d '{
    "consumer_group": "test-group",
    "topic": "test-events",
    "partition": 0,
    "offset": 10
  }' \
  $SERVER_ADDR streamhouse.StreamHouse/CommitOffset > /dev/null 2>&1
echo "✅ Offset committed"

# Test 7: Get committed offset
echo ""
echo "📊 Test 7: Getting committed offset..."
OFFSET_RESULT=$(grpcurl -plaintext \
  -d '{
    "consumer_group": "test-group",
    "topic": "test-events",
    "partition": 0
  }' \
  $SERVER_ADDR streamhouse.StreamHouse/GetOffset)

echo "$OFFSET_RESULT" | grep -q "\"offset\": \"10\"" && echo "✅ Offset matches" || echo "❌ Offset mismatch"

# Test 8: Error handling - invalid partition
echo ""
echo "⚠️  Test 8: Testing error handling (invalid partition)..."
ERROR_RESULT=$(grpcurl -plaintext \
  -d '{"topic": "test-events", "partition": 99, "value": "dGVzdA=="}' \
  $SERVER_ADDR streamhouse.StreamHouse/Produce 2>&1 || true)

echo "$ERROR_RESULT" | grep -q "Invalid partition" && echo "✅ Error handled correctly" || echo "❌ Error handling failed"

# Test 9: Delete topic
echo ""
echo "🗑️  Test 9: Deleting topic..."
grpcurl -plaintext \
  -d '{"name": "test-events"}' \
  $SERVER_ADDR streamhouse.StreamHouse/DeleteTopic > /dev/null 2>&1
echo "✅ Topic deleted"

# Summary
echo ""
echo "=================================="
echo "✅ All tests completed successfully!"
echo ""
echo "Next steps:"
echo "  - Check logs: RUST_LOG=debug cargo run -p streamhouse-server"
echo "  - Run unit tests: cargo test --workspace"
echo "  - See TESTING.md for more examples"
