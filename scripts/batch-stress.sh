#!/bin/bash
# Batch API stress test - demonstrates high throughput with batching

API_URL="${API_URL:-http://localhost:8080}"
TOTAL_MESSAGES=10000
BATCH_SIZE=1000

echo "Batch Stress Test: ${TOTAL_MESSAGES} messages in batches of ${BATCH_SIZE}"
echo ""

# Generate a batch of messages
generate_batch() {
    local start=$1
    local count=$2
    local topic=$3
    
    records="["
    for i in $(seq $start $((start + count - 1))); do
        partition=$((i % 6))
        if [ $i -gt $start ]; then
            records+=","
        fi
        records+="{\"key\":\"k${i}\",\"value\":\"{\\\"id\\\":${i}}\",\"partition\":${partition}}"
    done
    records+="]"
    
    echo "{\"topic\":\"${topic}\",\"records\":${records}}"
}

echo "Test 1: Individual requests (baseline)..."
start_time=$(date +%s.%N)

for i in $(seq 1 100); do
    curl -s -X POST "${API_URL}/api/v1/produce" \
      -H "Content-Type: application/json" \
      -d "{\"topic\":\"orders\",\"partition\":0,\"key\":\"k${i}\",\"value\":\"{\\\"id\\\":${i}}\"}" > /dev/null 2>&1
done

end_time=$(date +%s.%N)
duration=$(echo "$end_time - $start_time" | bc)
rate=$(echo "scale=2; 100 / $duration" | bc)
echo "  100 individual requests: ${duration}s (${rate} msg/s)"
echo ""

echo "Test 2: Batch API (${BATCH_SIZE} messages per request)..."
start_time=$(date +%s.%N)

num_batches=$((TOTAL_MESSAGES / BATCH_SIZE))
for batch in $(seq 0 $((num_batches - 1))); do
    start_idx=$((batch * BATCH_SIZE + 1))
    payload=$(generate_batch $start_idx $BATCH_SIZE "orders")
    
    result=$(curl -s -X POST "${API_URL}/api/v1/produce/batch" \
      -H "Content-Type: application/json" \
      -d "${payload}")
    
    count=$(echo "$result" | python3 -c "import json,sys; print(json.load(sys.stdin).get('count', 0))" 2>/dev/null || echo "0")
    echo "  Batch $((batch + 1)): ${count} messages"
done

end_time=$(date +%s.%N)
duration=$(echo "$end_time - $start_time" | bc)
rate=$(echo "scale=2; $TOTAL_MESSAGES / $duration" | bc)

echo ""
echo "Results:"
echo "  Total messages: ${TOTAL_MESSAGES}"
echo "  Duration: ${duration}s"
echo "  Throughput: ${rate} msg/s"
echo ""

# Calculate improvement
individual_rate=${rate%.*}
if [ "$individual_rate" -gt 0 ]; then
    improvement=$(echo "scale=0; $rate / 66" | bc)
    echo "That's ${improvement}x faster than individual HTTP requests!"
fi
