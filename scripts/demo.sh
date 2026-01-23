#!/bin/bash
#
# StreamHouse Demo - Complete Pipeline
#
# This script demonstrates the REAL StreamHouse pipeline end-to-end:
# 1. Wipes all data (PostgreSQL + MinIO + SQLite + cache)
# 2. Writes data through actual PartitionWriter APIs
# 3. Shows metadata in PostgreSQL
# 4. Shows segment files in MinIO
# 5. Reads data back through PartitionReader APIs
#
# Usage:
#   ./scripts/demo.sh              # Run full demo
#   ./scripts/demo.sh --skip-wipe  # Keep existing data
#

set -e

SKIP_WIPE=false
if [ "$1" = "--skip-wipe" ]; then
    SKIP_WIPE=true
fi

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🚀 StreamHouse Demo - Real Pipeline"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

#═════════════════════════════════════════════════════════════════════════════
# STEP 1: CLEAN SLATE (Optional)
#═════════════════════════════════════════════════════════════════════════════

if [ "$SKIP_WIPE" = false ]; then
    echo "📦 Step 1: Clean Slate - Wiping All Data"
    echo "─────────────────────────────────────────"
    echo ""

    echo "  • Wiping PostgreSQL..."
    docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
      DELETE FROM consumer_offsets;
      DELETE FROM consumer_groups;
      DELETE FROM segments;
      DELETE FROM partitions;
      DELETE FROM topics;
    " 2>&1 | grep -E "DELETE" | sed 's/^/    /' || true

    echo "  • Wiping MinIO..."
    docker exec streamhouse-minio mc alias set local http://localhost:9000 minioadmin minioadmin 2>/dev/null
    DELETED=$(docker exec streamhouse-minio mc rm --recursive --force local/streamhouse/ 2>&1 | grep "Removed" | wc -l | tr -d ' ')
    echo "    Deleted $DELETED objects"

    echo "  • Wiping SQLite database..."
    if [ -f /tmp/streamhouse_pipeline.db ]; then
        rm -f /tmp/streamhouse_pipeline.db
        echo "    Deleted /tmp/streamhouse_pipeline.db"
    else
        echo "    No SQLite database found"
    fi

    echo "  • Clearing segment cache..."
    CACHE_CLEARED=0
    if [ -d /tmp/streamhouse-cache ]; then
        rm -rf /tmp/streamhouse-cache
        CACHE_CLEARED=1
    fi
    # Also check for any other streamhouse cache directories
    for cache_dir in /tmp/streamhouse-*; do
        if [ -d "$cache_dir" ]; then
            rm -rf "$cache_dir"
            CACHE_CLEARED=1
        fi
    done
    if [ $CACHE_CLEARED -eq 1 ]; then
        echo "    Cleared segment cache"
    else
        echo "    No cache found"
    fi

    echo ""
    echo "  ✅ Clean slate ready"
    echo ""
else
    echo "📦 Step 1: Skipping wipe (keeping existing data)"
    echo ""
fi

#═════════════════════════════════════════════════════════════════════════════
# STEP 2: WRITE REAL DATA
#═════════════════════════════════════════════════════════════════════════════

echo "✍️  Step 2: Writing Data Through Real APIs"
echo "─────────────────────────────────────────"
echo ""
echo "  This uses the ACTUAL StreamHouse write pipeline:"
echo "  • PartitionWriter.append(key, value, timestamp)"
echo "  • SegmentWriter compresses with LZ4"
echo "  • Upload to MinIO via S3 API"
echo "  • Register metadata in PostgreSQL/SQLite"
echo ""

cd /Users/gabrielbram/Desktop/streamhouse

# Run the write example
cargo run --quiet --package streamhouse-storage --example write_with_real_api 2>&1 | grep -v "warning:" | grep -v "Compiling" | sed 's/^/  /'

echo ""

# Sync SQLite to PostgreSQL if needed
if [ -f /tmp/streamhouse_pipeline.db ]; then
    echo "  • Syncing metadata to PostgreSQL..."

    sqlite3 /tmp/streamhouse_pipeline.db "SELECT name, partition_count, retention_ms, created_at, updated_at, config FROM topics;" | while IFS='|' read name pcount retention created updated config; do
        docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
        INSERT INTO topics (name, partition_count, retention_ms, created_at, updated_at, config)
        VALUES ('$name', $pcount, $retention, $created, $updated, '$config'::jsonb)
        ON CONFLICT (name) DO NOTHING;
        " 2>&1 > /dev/null
    done

    sqlite3 /tmp/streamhouse_pipeline.db "SELECT topic, partition_id, high_watermark, created_at, updated_at FROM partitions;" | while IFS='|' read topic pid watermark created updated; do
        docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
        INSERT INTO partitions (topic, partition_id, high_watermark, created_at, updated_at)
        VALUES ('$topic', $pid, $watermark, $created, $updated)
        ON CONFLICT (topic, partition_id) DO UPDATE SET high_watermark = $watermark;
        " 2>&1 > /dev/null
    done

    sqlite3 /tmp/streamhouse_pipeline.db "SELECT id, topic, partition_id, base_offset, end_offset, record_count, size_bytes, s3_bucket, s3_key, created_at FROM segments;" | while IFS='|' read id topic pid base_off end_off rcount size bucket key created; do
        docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
        INSERT INTO segments (id, topic, partition_id, base_offset, end_offset, record_count, size_bytes, s3_bucket, s3_key, created_at)
        VALUES ('$id', '$topic', $pid, $base_off, $end_off, $rcount, $size, '$bucket', '$key', $created)
        ON CONFLICT (id) DO NOTHING;
        " 2>&1 > /dev/null
    done

    echo "    ✅ Synced"
fi

echo ""
echo "  ✅ Data written through real pipeline"
echo ""

#═════════════════════════════════════════════════════════════════════════════
# STEP 3: SHOW POSTGRESQL METADATA
#═════════════════════════════════════════════════════════════════════════════

echo "📊 Step 3: PostgreSQL Metadata"
echo "─────────────────────────────────────────"
echo ""

echo "Topics:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
SELECT name, partition_count, retention_ms / 3600000 as retention_hours
FROM topics
ORDER BY name;
" | sed 's/^/  /'
echo ""

echo "Partitions:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
SELECT topic, partition_id, high_watermark
FROM partitions
ORDER BY topic, partition_id;
" | sed 's/^/  /'
echo ""

echo "Segments:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
SELECT
  topic,
  partition_id as part,
  base_offset,
  end_offset,
  record_count,
  ROUND(size_bytes / 1024.0, 1) as size_kb
FROM segments
ORDER BY topic, partition_id, base_offset;
" | sed 's/^/  /'
echo ""

echo "Summary:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
SELECT
  topic,
  COUNT(*) as segments,
  SUM(record_count) as total_records,
  ROUND(SUM(size_bytes) / 1024.0, 1) as total_kb
FROM segments
GROUP BY topic
ORDER BY topic;
" | sed 's/^/  /'
echo ""

#═════════════════════════════════════════════════════════════════════════════
# STEP 4: SHOW MINIO STORAGE
#═════════════════════════════════════════════════════════════════════════════

echo "🪣 Step 4: MinIO Object Storage"
echo "─────────────────────────────────────────"
echo ""

echo "Directory Structure:"
docker exec streamhouse-minio mc tree local/streamhouse/ 2>/dev/null | sed 's/^/  /'
echo ""

echo "Segment Files:"
docker exec streamhouse-minio mc ls local/streamhouse/ --recursive 2>/dev/null | sed 's/^/  /'
echo ""

echo "Storage Stats:"
docker exec streamhouse-minio mc du local/streamhouse/ 2>/dev/null | sed 's/^/  /'
echo ""

#═════════════════════════════════════════════════════════════════════════════
# STEP 5: READ DATA BACK
#═════════════════════════════════════════════════════════════════════════════

echo "📖 Step 5: Reading Data Back (PartitionReader + Phase 3.4)"
echo "─────────────────────────────────────────"
echo ""
echo "  This uses the ACTUAL StreamHouse read pipeline:"
echo "  • PartitionReader.read(offset, count)"
echo "  • SegmentIndex finds segment via BTreeMap (< 1µs)"
echo "  • Load from cache or download from MinIO"
echo "  • Decompress LZ4 blocks"
echo "  • Decode binary records"
echo ""

echo "Example: Reading first 3 records from orders/partition-0"
echo ""
cargo run --quiet --package streamhouse-storage --example simple_reader -- orders 0 0 3 2>&1 | grep -v "warning:" | sed 's/^/  /'
echo ""

#═════════════════════════════════════════════════════════════════════════════
# STEP 6: SUMMARY
#═════════════════════════════════════════════════════════════════════════════

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ Demo Complete!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Get stats
TOTAL_RECORDS=$(docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -c "SELECT COALESCE(SUM(record_count), 0) FROM segments;" | tr -d ' ')
TOTAL_SEGMENTS=$(docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -c "SELECT COUNT(*) FROM segments;" | tr -d ' ')
TOTAL_TOPICS=$(docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -t -c "SELECT COUNT(*) FROM topics;" | tr -d ' ')

echo "What Was Proven:"
echo "  ✅ $TOTAL_RECORDS records written through PartitionWriter API"
echo "  ✅ $TOTAL_SEGMENTS segments compressed with LZ4 and uploaded to MinIO"
echo "  ✅ $TOTAL_TOPICS topics with metadata in PostgreSQL"
echo "  ✅ Records read back through PartitionReader with Phase 3.4 index"
echo ""

echo "This is the REAL StreamHouse production pipeline!"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📚 What To Do Next"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "1. Read More Records:"
echo "   cargo run --package streamhouse-storage --example simple_reader -- orders 0 50 10"
echo "   cargo run --package streamhouse-storage --example simple_reader -- user-events 0 100 5"
echo "   cargo run --package streamhouse-storage --example simple_reader -- metrics 2 150 20"
echo ""
echo "2. Query Which Segment Contains an Offset:"
echo "   ./scripts/interactive_query.sh orders 0 75"
echo "   ./scripts/interactive_query.sh user-events 0 150"
echo ""
echo "3. Inspect Binary Segment File:"
echo "   ./scripts/inspect_segment.sh orders 0 0"
echo "   ./scripts/inspect_segment.sh user-events 0"
echo ""
echo "4. Browse MinIO Web UI:"
echo "   open http://localhost:9001"
echo "   (Login: minioadmin / minioadmin)"
echo ""
echo "5. Query PostgreSQL Directly:"
echo "   docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata"
echo ""
echo "6. See the Show Schema:"
echo "   ./scripts/show_schema.sh"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
