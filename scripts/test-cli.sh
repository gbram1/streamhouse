#!/bin/bash
# StreamHouse CLI Testing Script
#
# Tests the streamctl CLI tool end-to-end

set -e

TOPIC="cli-test"
CLI="./target/release/streamctl"

echo "🧪 StreamHouse CLI Test Suite"
echo "=============================="
echo ""

# Check if server is running
if ! nc -z localhost 9090 2>/dev/null; then
    echo "❌ Server not running on port 9090"
    echo "   Start it with: ./start-dev.sh"
    exit 1
fi

echo "✅ Server is running"
echo ""

# Build CLI if needed
if [ ! -f "$CLI" ]; then
    echo "📦 Building streamctl CLI..."
    cargo build --release -p streamhouse-cli
    echo ""
fi

echo "Using CLI: $CLI"
echo ""

# Clean up any existing test topic
echo "🧹 Cleaning up old test data..."
$CLI topic delete $TOPIC 2>/dev/null || true
sleep 1
echo ""

# Test 1: Create topic
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 1: Create Topic"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI topic create $TOPIC --partitions 2
echo ""

# Test 2: List topics
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 2: List Topics"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI topic list
echo ""

# Test 3: Get topic info
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 3: Get Topic Info"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI topic get $TOPIC
echo ""

# Test 4: Produce with JSON value
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 4: Produce Record (JSON value)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI produce $TOPIC --partition 0 --value '{"user": "alice", "action": "login"}'
echo ""

# Test 5: Produce with key and value
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 5: Produce Record (with key)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI produce $TOPIC --partition 0 --key "user-123" --value '{"event": "purchase", "amount": 99.99}'
echo ""

# Test 6: Produce multiple records
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 6: Produce Multiple Records"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
for i in {1..5}; do
    $CLI produce $TOPIC --partition 1 --value "{\"id\": $i, \"message\": \"test $i\"}" > /dev/null
done
echo "✅ Produced 5 records to partition 1"
echo ""

# Test 7: Consume from partition 0
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 7: Consume from Partition 0"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI consume $TOPIC --partition 0 --offset 0
echo ""

# Test 8: Consume from partition 1 with limit
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 8: Consume with Limit"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI consume $TOPIC --partition 1 --offset 0 --limit 3
echo ""

# Test 9: Commit offset
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 9: Commit Consumer Offset"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI offset commit --group test-consumer --topic $TOPIC --partition 0 --offset 2
echo ""

# Test 10: Get committed offset
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 10: Get Committed Offset"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI offset get --group test-consumer --topic $TOPIC --partition 0
echo ""

# Test 11: Delete topic
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 11: Delete Topic"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
$CLI topic delete $TOPIC
echo ""

# Test 12: Verify topic is gone
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 12: Verify Deletion"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if $CLI topic get $TOPIC 2>&1 | grep -q "not found"; then
    echo "✅ Topic successfully deleted"
else
    echo "❌ Topic still exists"
fi
echo ""

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ All CLI tests completed successfully!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Try these commands yourself:"
echo "  $CLI --help"
echo "  $CLI topic create my-topic --partitions 3"
echo "  $CLI produce my-topic -p 0 -v '{\"test\": \"data\"}'"
echo "  $CLI consume my-topic -p 0 -o 0"
echo ""
