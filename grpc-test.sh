#!/bin/bash
# gRPC API Test Script
# Tests all gRPC endpoints for StreamHouse

GRPC_ADDR="${GRPC_ADDR:-localhost:50051}"

echo "╔═══════════════════════════════════════════════════════════╗"
echo "║         StreamHouse gRPC API Test                         ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "gRPC endpoint: ${GRPC_ADDR}"
echo ""

# Check if grpcurl is available
if ! command -v grpcurl &> /dev/null; then
    echo "Error: grpcurl is not installed"
    echo "Install with: brew install grpcurl"
    exit 1
fi

# Test 1: List services
echo "═══════════════════════════════════════════════════════════"
echo "Test 1: List gRPC Services"
echo "═══════════════════════════════════════════════════════════"
grpcurl -plaintext ${GRPC_ADDR} list
echo ""

# Test 2: List topics
echo "═══════════════════════════════════════════════════════════"
echo "Test 2: List Topics (gRPC)"
echo "═══════════════════════════════════════════════════════════"
grpcurl -plaintext ${GRPC_ADDR} streamhouse.StreamHouse/ListTopics
echo ""

# Test 3: Create a topic
echo "═══════════════════════════════════════════════════════════"
echo "Test 3: Create Topic (gRPC)"
echo "═══════════════════════════════════════════════════════════"
grpcurl -plaintext -d '{
  "name": "grpc-test-topic",
  "partition_count": 4,
  "retention_ms": 604800000
}' ${GRPC_ADDR} streamhouse.StreamHouse/CreateTopic
echo ""

# Test 4: Get topic details
echo "═══════════════════════════════════════════════════════════"
echo "Test 4: Get Topic Details (gRPC)"
echo "═══════════════════════════════════════════════════════════"
grpcurl -plaintext -d '{
  "name": "grpc-test-topic"
}' ${GRPC_ADDR} streamhouse.StreamHouse/GetTopic
echo ""

# Test 5: Produce a single message
echo "═══════════════════════════════════════════════════════════"
echo "Test 5: Produce Single Message (gRPC)"
echo "═══════════════════════════════════════════════════════════"
# Base64 encode the key and value
KEY=$(echo -n "grpc-key-1" | base64)
VALUE=$(echo -n '{"id":1,"data":"hello from grpc"}' | base64)
grpcurl -plaintext -d "{
  \"topic\": \"grpc-test-topic\",
  \"partition\": 0,
  \"key\": \"${KEY}\",
  \"value\": \"${VALUE}\"
}" ${GRPC_ADDR} streamhouse.StreamHouse/Produce
echo ""

# Test 6: Produce batch
echo "═══════════════════════════════════════════════════════════"
echo "Test 6: Produce Batch (gRPC)"
echo "═══════════════════════════════════════════════════════════"
# Generate 10 records
RECORDS=""
for i in $(seq 1 10); do
    KEY=$(echo -n "key-$i" | base64)
    VALUE=$(echo -n "{\"id\":$i,\"batch\":true}" | base64)
    if [ -n "$RECORDS" ]; then
        RECORDS="$RECORDS,"
    fi
    RECORDS="$RECORDS{\"key\":\"$KEY\",\"value\":\"$VALUE\"}"
done

grpcurl -plaintext -d "{
  \"topic\": \"grpc-test-topic\",
  \"partition\": 1,
  \"records\": [$RECORDS]
}" ${GRPC_ADDR} streamhouse.StreamHouse/ProduceBatch
echo ""

# Test 7: Consume messages
echo "═══════════════════════════════════════════════════════════"
echo "Test 7: Consume Messages (gRPC)"
echo "═══════════════════════════════════════════════════════════"
grpcurl -plaintext -d '{
  "topic": "grpc-test-topic",
  "partition": 0,
  "offset": 0,
  "max_records": 5
}' ${GRPC_ADDR} streamhouse.StreamHouse/Consume
echo ""

# Test 8: Commit offset
echo "═══════════════════════════════════════════════════════════"
echo "Test 8: Commit Offset (gRPC)"
echo "═══════════════════════════════════════════════════════════"
grpcurl -plaintext -d '{
  "consumer_group": "grpc-test-group",
  "topic": "grpc-test-topic",
  "partition": 0,
  "offset": 5
}' ${GRPC_ADDR} streamhouse.StreamHouse/CommitOffset
echo ""

# Test 9: Get committed offset
echo "═══════════════════════════════════════════════════════════"
echo "Test 9: Get Committed Offset (gRPC)"
echo "═══════════════════════════════════════════════════════════"
grpcurl -plaintext -d '{
  "consumer_group": "grpc-test-group",
  "topic": "grpc-test-topic",
  "partition": 0
}' ${GRPC_ADDR} streamhouse.StreamHouse/GetOffset
echo ""

echo "═══════════════════════════════════════════════════════════"
echo "All gRPC tests completed!"
echo "═══════════════════════════════════════════════════════════"
