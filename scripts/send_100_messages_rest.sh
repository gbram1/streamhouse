#!/bin/bash
# Send 100 messages via REST API (no gRPC needed)

set -e

echo ""
echo "🚀 StreamHouse Pipeline Test - 100 Messages (REST API)"
echo "======================================================="
echo ""

TOPIC="test-topic"
PARTITIONS=3
MESSAGES_PER_PARTITION=$((100 / PARTITIONS))
API_URL="http://localhost:8080/api/v1/produce"

echo "📤 Sending 100 messages to topic '$TOPIC' via REST API"
echo "   Distributing across $PARTITIONS partitions"
echo ""

total_sent=0

for partition in $(seq 0 $((PARTITIONS - 1))); do
    echo -n "   Partition $partition: "

    for i in $(seq 0 $((MESSAGES_PER_PARTITION - 1))); do
        msg_id=$((partition * MESSAGES_PER_PARTITION + i))
        timestamp=$(date +%s%3N)

        # Create JSON payload for REST API
        key="key_${msg_id}"
        value="Test message ${msg_id} at ${timestamp}"

        # Send via REST API
        response=$(curl -s -X POST "$API_URL" \
            -H "Content-Type: application/json" \
            -d "{\"topic\": \"${TOPIC}\", \"partition\": ${partition}, \"key\": \"${key}\", \"value\": \"${value}\"}" \
            2>&1)

        # Check if successful
        if echo "$response" | grep -q "offset"; then
            total_sent=$((total_sent + 1))
        else
            echo -e "\n   ❌ Error: $response"
        fi

        # Progress indicator
        if [ $((($i + 1) % 10)) -eq 0 ]; then
            echo -n "."
        fi
    done

    echo " ✅ $MESSAGES_PER_PARTITION messages sent"
done

echo ""
echo "✅ Total messages sent: $total_sent"
echo ""

# Give server time to flush
echo "⏳ Waiting for flush to MinIO (5 seconds)..."
sleep 5
echo ""

# Check MinIO
echo "📦 Checking MinIO for segments..."
if command -v docker &> /dev/null; then
    for p in 0 1 2; do
        echo -n "   Partition $p: "
        docker exec streamhouse-minio mc ls local/streamhouse/data/${TOPIC}/$p/ 2>/dev/null | tail -1 || echo "(no segments)"
    done
else
    echo "   (docker not available to check MinIO)"
fi
echo ""

# Check metrics
echo "📊 Checking metrics..."
segment_writes=$(curl -s http://localhost:8080/metrics 2>/dev/null | grep 'streamhouse_segment_writes_total' | tail -3)
if [ -n "$segment_writes" ]; then
    echo "$segment_writes" | sed 's/^/   /'
fi
echo ""

echo "════════════════════════════════════════"
echo "✅ Pipeline Test Complete (REST API)!"
echo "════════════════════════════════════════"
echo ""

echo "Verification:"
echo "  1. MinIO Console: http://localhost:9001"
echo "     Login: minioadmin / minioadmin"
echo "     Browse: streamhouse/${TOPIC}/"
echo ""
echo "  2. View all metrics:"
echo "     curl http://localhost:8080/metrics | grep streamhouse"
echo ""
echo "  3. Check PostgreSQL segments:"
echo "     docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata \\"
echo "       -c \"SELECT topic, partition_id, record_count FROM segments WHERE topic='${TOPIC}';\""
echo ""
