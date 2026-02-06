#!/bin/bash
# Send 100 messages to StreamHouse via gRPC and watch flush to MinIO

set -e

echo ""
echo "🚀 StreamHouse Pipeline Test - 100 Messages"
echo "============================================="
echo ""

TOPIC="test-topic"
PARTITIONS=3
MESSAGES_PER_PARTITION=$((100 / PARTITIONS))

echo "📤 Sending 100 messages to topic '$TOPIC'"
echo "   Distributing across $PARTITIONS partitions"
echo ""

total_sent=0

for partition in $(seq 0 $((PARTITIONS - 1))); do
    echo -n "   Partition $partition: "

    for i in $(seq 0 $((MESSAGES_PER_PARTITION - 1))); do
        msg_id=$((partition * MESSAGES_PER_PARTITION + i))
        timestamp=$(date +%s%3N)

        # Create JSON payload
        key="key_${msg_id}"
        value=$(cat <<EOF
{"message_id": ${msg_id}, "partition": ${partition}, "content": "Test message ${msg_id}", "timestamp": ${timestamp}}
EOF
)

        # Encode key and value as base64 for gRPC bytes field
        key_b64=$(echo -n "$key" | base64)
        value_b64=$(echo -n "$value" | base64)

        # Send via gRPC (suppress output)
        grpcurl -plaintext \
            -d "{\"topic\": \"${TOPIC}\", \"partition\": ${partition}, \"key\": \"${key_b64}\", \"value\": \"${value_b64}\"}" \
            localhost:50051 \
            streamhouse.StreamHouse.Produce \
            > /dev/null 2>&1

        total_sent=$((total_sent + 1))

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
echo ""

# Use curl to check MinIO API (list objects in bucket)
# Note: This requires aws CLI or mc (MinIO Client)
if command -v mc &> /dev/null; then
    echo "   Using MinIO Client (mc):"
    mc alias set local http://localhost:9000 minioadmin minioadmin > /dev/null 2>&1 || true
    mc ls local/streamhouse/${TOPIC}/ 2>/dev/null || echo "   (No segments yet or mc not configured)"
else
    echo "   (Install MinIO Client 'mc' to list segments programmatically)"
fi
echo ""

# Check metrics
echo "📊 Checking metrics..."
segment_writes=$(curl -s http://localhost:8080/metrics 2>/dev/null | grep 'streamhouse_segment_writes_total' | tail -1)
segment_flushes=$(curl -s http://localhost:8080/metrics 2>/dev/null | grep 'streamhouse_segment_flushes_total' | tail -1)

if [ -n "$segment_writes" ]; then
    echo "   ${segment_writes}"
fi
if [ -n "$segment_flushes" ]; then
    echo "   ${segment_flushes}"
fi
echo ""

echo "════════════════════════════════════════"
echo "✅ Pipeline Test Complete!"
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
echo "     docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \"SELECT * FROM segments WHERE topic='${TOPIC}' LIMIT 5;\""
echo ""
