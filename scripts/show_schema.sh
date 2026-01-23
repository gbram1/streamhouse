#!/bin/bash
#
# Show StreamHouse Database Schema and Sample Data Structure
#

echo "🗄️  StreamHouse Database Schema"
echo "================================"
echo ""

echo "📊 PostgreSQL Schema:"
echo "--------------------"
echo ""

echo "1. TOPICS table:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "\d topics" 2>/dev/null
echo ""

echo "2. PARTITIONS table:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "\d partitions" 2>/dev/null
echo ""

echo "3. SEGMENTS table:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "\d segments" 2>/dev/null
echo ""

echo "4. CONSUMER_GROUPS table:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "\d consumer_groups" 2>/dev/null
echo ""

echo "5. CONSUMER_OFFSETS table:"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "\d consumer_offsets" 2>/dev/null
echo ""

echo "6. AGENTS table (Phase 4 ready):"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "\d agents" 2>/dev/null
echo ""

echo "7. PARTITION_LEASES table (Phase 4 ready):"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "\d partition_leases" 2>/dev/null
echo ""

echo "================================"
echo ""
echo "💡 Sample Data Structure:"
echo ""
echo "  Topic: 'orders'"
echo "  ├── partition 0: offset 0-999"
echo "  │   └── segment: orders/0/seg_0000000000000000000000.bin"
echo "  ├── partition 1: offset 0-1499"
echo "  │   └── segment: orders/1/seg_0000000000000000000000.bin"
echo "  └── partition 2: offset 0-2999"
echo "      └── segment: orders/2/seg_0000000000000000000000.bin"
echo ""
echo "  Segment file format (LZ4 compressed):"
echo "  ┌─────────────────────────────────┐"
echo "  │ SegmentHeader (magic, version)  │"
echo "  ├─────────────────────────────────┤"
echo "  │ Block 1 (compressed records)    │"
echo "  ├─────────────────────────────────┤"
echo "  │ Block 2 (compressed records)    │"
echo "  ├─────────────────────────────────┤"
echo "  │ ... more blocks ...             │"
echo "  ├─────────────────────────────────┤"
echo "  │ Offset Index (for fast seeks)   │"
echo "  └─────────────────────────────────┘"
echo ""
echo "  Record format (inside blocks):"
echo "  - offset: u64 (varint encoded)"
echo "  - timestamp: i64 (milliseconds)"
echo "  - key_len: u32 (varint)"
echo "  - key: bytes"
echo "  - value_len: u32 (varint)"
echo "  - value: bytes"
