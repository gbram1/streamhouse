#!/bin/bash
# StreamHouse Kafka Protocol - Quick Test Commands
#
# Usage: ./quick_test.sh

set -e

BOOTSTRAP="127.0.0.1:9092"

echo "StreamHouse Kafka Protocol - Quick Tests"
echo "========================================="
echo ""

# Check if server is running
if ! nc -z 127.0.0.1 9092 2>/dev/null; then
    echo "ERROR: StreamHouse not running on port 9092"
    echo ""
    echo "Start the server with:"
    echo "  USE_LOCAL_STORAGE=1 ./target/release/unified-server"
    exit 1
fi

echo "Server is running on $BOOTSTRAP"
echo ""

# Check for kafka-python
if ! python3 -c "import kafka" 2>/dev/null; then
    echo "Installing kafka-python..."
    pip3 install kafka-python
fi

# Quick produce/consume test
echo "Running quick produce/consume test..."
echo "--------------------------------------"

python3 << 'EOF'
import time
from kafka import KafkaProducer, KafkaConsumer

BOOTSTRAP = '127.0.0.1:9092'
TOPIC = 'quick-test'

# Produce
print("1. Producing 3 messages...")
producer = KafkaProducer(bootstrap_servers=BOOTSTRAP, api_version=(2, 3))
for i in range(3):
    producer.send(TOPIC, f'Test message {i}'.encode(), key=f'key{i}'.encode())
producer.flush()
producer.close()
print("   Done!")

# Wait for flush (server flushes every 5s, plus some margin)
print("2. Waiting for flush (8s)...")
time.sleep(8)

# Consume
print("3. Consuming messages...")
consumer = KafkaConsumer(
    TOPIC,
    bootstrap_servers=BOOTSTRAP,
    api_version=(2, 3),
    auto_offset_reset='earliest',
    consumer_timeout_ms=5000
)

count = 0
for msg in consumer:
    count += 1
    print(f"   Received: key={msg.key.decode()}, value={msg.value.decode()}")

consumer.close()

if count > 0:
    print(f"\n✓ SUCCESS: Produced and consumed {count} messages")
else:
    print("\n✗ FAILED: No messages consumed")
EOF

echo ""
echo "========================================="
echo "Quick test complete!"
echo ""
echo "For more examples, see:"
echo "  - python_example.py"
echo "  - nodejs_example.js"
echo "  - go_example.go"
echo "  - JavaExample.java"
