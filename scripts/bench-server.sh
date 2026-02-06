#!/bin/bash
# StreamHouse Performance Benchmarking Script
#
# Tests write throughput, read throughput, and latency

set -e

SERVER_ADDR="localhost:9090"
TOPIC="perf-test"
PARTITIONS=3

# Colors for output
GREEN='\033[0.32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "⚡ StreamHouse Performance Benchmark"
echo "===================================="
echo ""

# Check if server is running
if ! nc -z localhost 9090 2>/dev/null; then
    echo "❌ Server not running on port 9090"
    echo "   Start it with: ./start-dev.sh"
    exit 1
fi

echo "✅ Server is running"
echo ""

# Check if streamctl is built
if [ ! -f "target/release/streamctl" ]; then
    echo "📦 Building streamctl in release mode..."
    cargo build --release -p streamhouse-cli
fi

STREAMCTL="./target/release/streamctl"

# Clean up any existing test topic
echo "🧹 Cleaning up old test data..."
$STREAMCTL topic delete $TOPIC 2>/dev/null || true
sleep 1

# Create test topic
echo "📝 Creating test topic: $TOPIC ($PARTITIONS partitions)"
$STREAMCTL topic create $TOPIC --partitions $PARTITIONS
echo ""

# Test 1: Single record write latency
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 1: Single Record Write Latency"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Writing 100 records and measuring latency..."

TOTAL_TIME=0
COUNT=100

for i in $(seq 1 $COUNT); do
    VALUE="{\"id\": $i, \"data\": \"test message number $i\"}"
    START=$(date +%s%N)
    $STREAMCTL produce $TOPIC --partition 0 --value "$VALUE" > /dev/null 2>&1
    END=$(date +%s%N)
    DURATION=$((($END - $START) / 1000000)) # Convert to ms
    TOTAL_TIME=$(($TOTAL_TIME + $DURATION))
done

AVG_LATENCY=$(($TOTAL_TIME / $COUNT))
echo "✅ Average write latency: ${AVG_LATENCY}ms"
echo ""

# Test 2: Batch write throughput
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 2: Batch Write Throughput"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Writing 1000 records as fast as possible..."

START=$(date +%s%N)
for i in $(seq 1 1000); do
    PARTITION=$(($i % $PARTITIONS))
    VALUE="{\"id\": $i, \"timestamp\": $(date +%s), \"payload\": \"benchmark data\"}"
    $STREAMCTL produce $TOPIC --partition $PARTITION --value "$VALUE" > /dev/null 2>&1 &

    # Limit concurrent processes
    if [ $(($i % 50)) -eq 0 ]; then
        wait
    fi
done
wait
END=$(date +%s%N)

DURATION_SEC=$((($END - $START) / 1000000000))
DURATION_MS=$((($END - $START) / 1000000))
THROUGHPUT=$((1000 * 1000 / $DURATION_MS))

echo "✅ Wrote 1000 records in ${DURATION_SEC}s (${DURATION_MS}ms)"
echo "✅ Throughput: ~${THROUGHPUT} records/second"
echo ""

# Test 3: Read throughput
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 3: Read Throughput"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Reading all records from partition 0..."

START=$(date +%s%N)
RESULT=$($STREAMCTL consume $TOPIC --partition 0 --offset 0 --limit 1000 2>&1)
END=$(date +%s%N)

RECORD_COUNT=$(echo "$RESULT" | grep "✅ Consumed" | awk '{print $3}')
DURATION_MS=$((($END - $START) / 1000000))
READ_THROUGHPUT=$(($RECORD_COUNT * 1000 / $DURATION_MS))

echo "✅ Read $RECORD_COUNT records in ${DURATION_MS}ms"
echo "✅ Read throughput: ~${READ_THROUGHPUT} records/second"
echo ""

# Test 4: Offset commit/get performance
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test 4: Offset Operations"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Testing offset commit and retrieval..."

# Commit offset
START=$(date +%s%N)
for i in $(seq 1 100); do
    $STREAMCTL offset commit --group bench-group --topic $TOPIC --partition 0 --offset $i > /dev/null 2>&1
done
END=$(date +%s%N)

COMMIT_AVG=$((($END - $START) / 100 / 1000000))
echo "✅ Average offset commit time: ${COMMIT_AVG}ms (100 commits)"

# Get offset
START=$(date +%s%N)
for i in $(seq 1 100); do
    $STREAMCTL offset get --group bench-group --topic $TOPIC --partition 0 > /dev/null 2>&1
done
END=$(date +%s%N)

GET_AVG=$((($END - $START) / 100 / 1000000))
echo "✅ Average offset get time: ${GET_AVG}ms (100 gets)"
echo ""

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Performance Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Write Latency (avg):     ${AVG_LATENCY}ms"
echo "Write Throughput:        ~${THROUGHPUT} records/sec"
echo "Read Throughput:         ~${READ_THROUGHPUT} records/sec"
echo "Offset Commit (avg):     ${COMMIT_AVG}ms"
echo "Offset Get (avg):        ${GET_AVG}ms"
echo ""

# Cleanup
echo "🧹 Cleaning up test data..."
$STREAMCTL topic delete $TOPIC

echo ""
echo "✅ Benchmark complete!"
echo ""
echo "Note: These are basic benchmarks using the CLI."
echo "For production performance testing, use:"
echo "  - cargo bench (Criterion-based benchmarks)"
echo "  - Direct gRPC calls (eliminates CLI overhead)"
echo "  - Larger payloads (test compression efficiency)"
echo "  - S3 storage (test actual cloud performance)"
