#!/bin/bash
# Parallel stress test using background jobs

API_URL="${API_URL:-http://localhost:8080}"
MESSAGES=1000
PARALLELISM=50

echo "Parallel Stress Test: ${MESSAGES} messages with ${PARALLELISM} concurrent workers"
echo ""

send_batch() {
    local topic=$1
    local start=$2
    local count=$3
    for i in $(seq $start $((start + count - 1))); do
        partition=$((i % 6))
        curl -s -X POST "${API_URL}/api/v1/produce" \
          -H "Content-Type: application/json" \
          -d "{\"topic\":\"${topic}\",\"partition\":${partition},\"key\":\"k${i}\",\"value\":\"{\\\"id\\\":${i}}\"}" > /dev/null 2>&1
    done
}

echo "Starting parallel test to 'orders' topic..."
start_time=$(date +%s.%N)

# Split into batches and run in parallel
batch_size=$((MESSAGES / PARALLELISM))
for i in $(seq 0 $((PARALLELISM - 1))); do
    start_idx=$((i * batch_size + 1))
    send_batch "orders" $start_idx $batch_size &
done

# Wait for all background jobs
wait

end_time=$(date +%s.%N)
duration=$(echo "$end_time - $start_time" | bc)
rate=$(echo "scale=2; $MESSAGES / $duration" | bc)

echo ""
echo "Results:"
echo "  Messages: ${MESSAGES}"
echo "  Duration: ${duration}s"
echo "  Rate: ${rate} msg/s"
echo ""
echo "That's $(echo "scale=0; $rate / 66" | bc)x faster than sequential!"
