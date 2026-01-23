#!/bin/bash
#
# Inspect StreamHouse Data in PostgreSQL and MinIO
#

set -e

export DATABASE_URL=postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata

echo "🔍 StreamHouse Data Inspector"
echo "=============================="
echo ""

# Check PostgreSQL
echo "📊 PostgreSQL Metadata:"
echo "----------------------"
echo ""

echo "1. Topics:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT name, partition_count, retention_ms, created_at FROM topics ORDER BY created_at DESC LIMIT 5;" 2>/dev/null || echo "No topics found"
echo ""

echo "2. Partitions:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT topic, partition_id, high_watermark FROM partitions ORDER BY topic, partition_id LIMIT 10;" 2>/dev/null || echo "No partitions found"
echo ""

echo "3. Segments:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT id, topic, partition_id, base_offset, end_offset, record_count, size_bytes FROM segments ORDER BY created_at DESC LIMIT 5;" 2>/dev/null || echo "No segments found"
echo ""

echo "4. Consumer Groups:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT group_id, topic, partition_id, committed_offset FROM consumer_offsets LIMIT 5;" 2>/dev/null || echo "No consumer offsets found"
echo ""

# Check MinIO
echo "🪣 MinIO Objects:"
echo "----------------"
echo ""

echo "Buckets:"
docker exec streamhouse-minio mc ls local/ 2>/dev/null || echo "MinIO mc not configured"
echo ""

echo "Objects in streamhouse bucket:"
docker exec streamhouse-minio mc ls local/streamhouse/ --recursive 2>/dev/null || echo "No objects found or bucket doesn't exist"
echo ""

echo "=============================="
echo "💡 Tips:"
echo "  - Topics are created via create_topic() API"
echo "  - Segments are written to MinIO when producer flushes"
echo "  - Partition watermarks track latest offset"
echo "  - Consumer offsets track consumption progress"
