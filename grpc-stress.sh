#!/bin/bash
# gRPC Stress Test - Compare gRPC batch performance with HTTP

GRPC_ADDR="${GRPC_ADDR:-localhost:50051}"
TOTAL_MESSAGES=10000
BATCH_SIZE=1000

echo "╔═══════════════════════════════════════════════════════════╗"
echo "║         gRPC Stress Test                                  ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "Target: ${TOTAL_MESSAGES} messages in batches of ${BATCH_SIZE}"
echo "gRPC endpoint: ${GRPC_ADDR}"
echo ""

# Ensure topic exists
echo "Ensuring 'grpc-stress-topic' exists..."
grpcurl -plaintext -d '{
  "name": "grpc-stress-topic",
  "partition_count": 6
}' ${GRPC_ADDR} streamhouse.StreamHouse/CreateTopic 2>/dev/null || true
echo ""

# Generate batch function
generate_grpc_batch() {
    local count=$1
    local partition=$2

    local records=""
    for i in $(seq 1 $count); do
        local value=$(echo -n "{\"id\":$i,\"ts\":$(date +%s)}" | base64)
        local key=$(echo -n "k$i" | base64)
        if [ -n "$records" ]; then
            records="$records,"
        fi
        records="$records{\"key\":\"$key\",\"value\":\"$value\"}"
    done

    echo "{\"topic\":\"grpc-stress-topic\",\"partition\":$partition,\"records\":[$records]}"
}

# Test 1: Individual gRPC calls (baseline)
echo "═══════════════════════════════════════════════════════════"
echo "Test 1: Individual gRPC Produce calls (100 messages)"
echo "═══════════════════════════════════════════════════════════"
start_time=$(date +%s.%N)

for i in $(seq 1 100); do
    key=$(echo -n "single-$i" | base64)
    value=$(echo -n "{\"id\":$i}" | base64)
    grpcurl -plaintext -d "{
      \"topic\": \"grpc-stress-topic\",
      \"partition\": $((i % 6)),
      \"key\": \"$key\",
      \"value\": \"$value\"
    }" ${GRPC_ADDR} streamhouse.StreamHouse/Produce > /dev/null 2>&1
done

end_time=$(date +%s.%N)
duration=$(echo "$end_time - $start_time" | bc)
rate=$(echo "scale=2; 100 / $duration" | bc)
echo "  100 individual gRPC calls: ${duration}s (${rate} msg/s)"
echo ""

# Test 2: gRPC Batch calls
echo "═══════════════════════════════════════════════════════════"
echo "Test 2: gRPC ProduceBatch (${BATCH_SIZE} messages per call)"
echo "═══════════════════════════════════════════════════════════"
start_time=$(date +%s.%N)

num_batches=$((TOTAL_MESSAGES / BATCH_SIZE))
for batch in $(seq 0 $((num_batches - 1))); do
    partition=$((batch % 6))
    payload=$(generate_grpc_batch $BATCH_SIZE $partition)

    result=$(grpcurl -plaintext -d "$payload" ${GRPC_ADDR} streamhouse.StreamHouse/ProduceBatch 2>&1)
    count=$(echo "$result" | grep -o '"count":[0-9]*' | cut -d: -f2 || echo "?")
    echo "  Batch $((batch + 1)): ${count} messages"
done

end_time=$(date +%s.%N)
duration=$(echo "$end_time - $start_time" | bc)
grpc_rate=$(echo "scale=2; $TOTAL_MESSAGES / $duration" | bc)

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "Results Summary"
echo "═══════════════════════════════════════════════════════════"
echo "  Total messages:     ${TOTAL_MESSAGES}"
echo "  gRPC batch duration: ${duration}s"
echo "  gRPC throughput:     ${grpc_rate} msg/s"
echo ""

# Compare with HTTP if available
echo "═══════════════════════════════════════════════════════════"
echo "Protocol Comparison (approximate)"
echo "═══════════════════════════════════════════════════════════"
echo "  Individual HTTP:  ~80 msg/s"
echo "  Batch HTTP:       ~15,000 msg/s"
echo "  Individual gRPC:  ~${rate} msg/s"
echo "  Batch gRPC:       ~${grpc_rate} msg/s"
echo ""

improvement=$(echo "scale=0; $grpc_rate / 80" | bc 2>/dev/null || echo "?")
echo "  gRPC batch is ~${improvement}x faster than individual HTTP!"
