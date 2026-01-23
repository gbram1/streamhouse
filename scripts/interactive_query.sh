#!/bin/bash
#
# Interactive Query Tool - Query any offset
#

query_offset() {
  local topic=$1
  local partition=$2
  local offset=$3

  echo "🔍 Querying: topic=$topic, partition=$partition, offset=$offset"
  echo "================================================================"
  echo ""

  # Find the segment
  RESULT=$(docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -A -F'|' -c "
    SELECT base_offset, end_offset, record_count, s3_bucket, s3_key, size_bytes
    FROM segments
    WHERE topic = '$topic' AND partition_id = $partition
      AND base_offset <= $offset AND end_offset >= $offset;
  ")

  if [ -z "$RESULT" ]; then
    echo "❌ Offset $offset not found in $topic/partition-$partition"
    echo ""
    echo "Available segments:"
    docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
      SELECT base_offset, end_offset, record_count
      FROM segments
      WHERE topic = '$topic' AND partition_id = $partition
      ORDER BY base_offset;
    "
    return 1
  fi

  # Parse result
  IFS='|' read -r base_offset end_offset record_count s3_bucket s3_key size_bytes <<< "$RESULT"

  echo "✅ Found segment:"
  echo "   Offset range: $base_offset - $end_offset"
  echo "   Record count: $record_count"
  echo "   Size: $(echo "scale=2; $size_bytes / 1024 / 1024" | bc) MB"
  echo "   S3 location: s3://$s3_bucket/$s3_key"
  echo ""

  # Show BTreeMap simulation
  echo "📊 Phase 3.4 BTreeMap Lookup Simulation:"
  echo "   Query: index.range(..=$offset).next_back()"
  echo "   Result: Found segment with base_offset=$base_offset"
  echo "   Complexity: O(log n) < 1µs"
  echo ""

  # Verify file exists in MinIO
  echo "🪣 Verifying MinIO storage..."
  if docker exec streamhouse-minio mc stat local/$s3_bucket/$s3_key &>/dev/null; then
    SIZE=$(docker exec streamhouse-minio mc stat local/$s3_bucket/$s3_key 2>/dev/null | grep "Size" | awk '{print $3, $4}')
    DATE=$(docker exec streamhouse-minio mc stat local/$s3_bucket/$s3_key 2>/dev/null | grep "Date" | awk '{print $3, $4, $5, $6}')
    echo "   ✅ File exists: $SIZE, created $DATE"
  else
    echo "   ❌ File not found in MinIO"
  fi
  echo ""

  # Show read flow
  echo "📖 Read Flow:"
  echo "   1. PartitionReader.read(offset=$offset, limit=100)"
  echo "   2. SegmentIndex.find_segment_for_offset($offset) → $s3_key"
  echo "   3. SegmentCache.get_segment(...) → Load from cache or MinIO"
  echo "   4. SegmentReader.read_range($offset, $((offset + 100)))"
  echo "   5. Return 100 records (offsets $offset-$((offset + 99)))"
  echo ""
}

# Main
case "${1:-}" in
  "")
    echo "Usage: $0 <topic> <partition> <offset>"
    echo ""
    echo "Examples:"
    echo "  $0 orders 0 5000     # Query offset 5000 in orders/partition-0"
    echo "  $0 orders 0 25000    # Query offset 25000 in orders/partition-0"
    echo "  $0 user-events 0 75000  # Query offset 75000 in user-events/partition-0"
    echo "  $0 metrics 1 50000   # Query offset 50000 in metrics/partition-1"
    echo ""
    echo "Available topics:"
    docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
      "SELECT name, partition_count FROM topics ORDER BY name;" 2>/dev/null
    echo ""
    echo "Show all segments:"
    docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c \
      "SELECT topic, partition_id, base_offset, end_offset, record_count
       FROM segments ORDER BY topic, partition_id, base_offset;" 2>/dev/null
    ;;
  *)
    query_offset "$1" "$2" "$3"
    ;;
esac
