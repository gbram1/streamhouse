#!/bin/bash

echo "========================================="
echo "Simple Phase 2.1 Manual Test"
echo "========================================="
echo ""
echo "Prerequisites:"
echo "1. Server is running on localhost:9090"
echo "2. grpcurl is installed"
echo ""

# Test 1: Create topic
echo "Test 1: Creating topic..."
grpcurl -plaintext -d '{
  "name": "demo",
  "partition_count": 2
}' localhost:9090 streamhouse.StreamHouse/CreateTopic

echo ""
echo "✅ Topic created"
echo ""

# Test 2: Produce records
echo "Test 2: Producing 3 records..."
for i in {1..3}; do
    grpcurl -plaintext -d "{
      \"topic\": \"demo\",
      \"partition\": 0,
      \"value\": \"$(echo -n "Message $i" | base64)\"
    }" localhost:9090 streamhouse.StreamHouse/Produce
    echo ""
done

echo "✅ Records produced"
echo ""

#Test 3: Wait for flush
echo "Test 3: Waiting 6 seconds for background flush..."
sleep 6
echo "✅ Flush should have happened"
echo ""

# Test 4: Consume
echo "Test 4: Consuming records..."
grpcurl -plaintext -d '{
  "topic": "demo",
  "partition": 0,
  "offset": 0,
  "max_records": 10
}' localhost:9090 streamhouse.StreamHouse/Consume

echo ""
echo "========================================="
echo "If you see records above, Phase 2.1 works!"
echo "========================================="
