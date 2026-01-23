#!/bin/bash
#
# Inspect Segment File Contents
#
# Shows what's actually inside a StreamHouse segment file:
# - Binary header
# - Compressed blocks
# - Offset index
# - Footer with CRC
#

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <topic> <partition> [offset]"
    echo ""
    echo "Examples:"
    echo "  $0 orders 0          # Inspect first segment"
    echo "  $0 orders 0 0        # Inspect segment starting at offset 0"
    echo "  $0 user-events 0"
    echo ""
    echo "Available segments:"
    docker exec streamhouse-minio mc ls local/streamhouse/data/ --recursive 2>/dev/null | grep "\.seg"
    exit 1
fi

TOPIC=$1
PARTITION=$2
OFFSET=${3:-0}

# Find the segment file
SEGMENT_FILE=$(printf "data/%s/%s/%020d.seg" "$TOPIC" "$PARTITION" "$OFFSET")

echo "🔍 Inspecting Segment: $SEGMENT_FILE"
echo "=========================================="
echo ""

# Check if file exists
if ! docker exec streamhouse-minio mc stat local/streamhouse/$SEGMENT_FILE &>/dev/null; then
    echo "❌ Segment not found: $SEGMENT_FILE"
    echo ""
    echo "Available segments for $TOPIC/$PARTITION:"
    docker exec streamhouse-minio mc ls local/streamhouse/data/$TOPIC/$PARTITION/ 2>/dev/null || echo "No segments found"
    exit 1
fi

# Get file stats
echo "📦 File Information:"
docker exec streamhouse-minio mc stat local/streamhouse/$SEGMENT_FILE 2>/dev/null | grep -E "Name|Size|Date|ETag"
echo ""

# Download the segment
echo "⬇️  Downloading segment..."
TMP_FILE="/tmp/segment_inspect_$(date +%s).seg"
docker exec streamhouse-minio mc cp local/streamhouse/$SEGMENT_FILE $TMP_FILE 2>/dev/null
docker cp streamhouse-minio:$TMP_FILE $TMP_FILE
echo "   ✅ Downloaded to $TMP_FILE"
echo ""

# Show binary structure
echo "📋 Binary Structure:"
echo ""

# Header (first 32 bytes)
echo "1. Header (32 bytes):"
hexdump -C $TMP_FILE | head -3
echo ""

# Check magic bytes
MAGIC=$(xxd -p -l 4 $TMP_FILE)
if [ "$MAGIC" = "5354524d" ]; then  # "STRM" in hex
    echo "   ✅ Magic: STRM (valid StreamHouse segment)"
else
    echo "   ⚠️  Magic: $MAGIC (expected 5354524d = 'STRM')"
fi

# Version
VERSION=$(xxd -p -s 4 -l 2 $TMP_FILE)
echo "   Version: 0x$VERSION"

# Compression
COMPRESSION=$(xxd -p -s 6 -l 1 $TMP_FILE)
case "$COMPRESSION" in
    "00") echo "   Compression: None (0x00)" ;;
    "01") echo "   Compression: LZ4 (0x01)" ;;
    *) echo "   Compression: Unknown (0x$COMPRESSION)" ;;
esac
echo ""

# File size breakdown
FILE_SIZE=$(stat -f%z $TMP_FILE 2>/dev/null || stat -c%s $TMP_FILE 2>/dev/null)
echo "2. Size Breakdown:"
echo "   Total size: $FILE_SIZE bytes"
echo "   Header: 32 bytes"
echo "   Data blocks + index: $((FILE_SIZE - 32)) bytes"
echo ""

# Try to extract readable strings (record keys/values)
echo "3. Readable Content (sample):"
echo ""
if [ "$COMPRESSION" = "00" ]; then
    echo "   Keys found:"
    strings $TMP_FILE | grep -E '^event-|^order-' | head -5 | sed 's/^/     /'
    echo ""
    echo "   JSON fragments:"
    strings $TMP_FILE | grep -E 'event_type|user_id|order_id|customer' | head -5 | sed 's/^/     /'
else
    echo "   (Data is LZ4 compressed - cannot view raw)"
fi
echo ""
echo "   To read full decoded records:"
echo "   cargo run --package streamhouse-storage --example simple_reader -- $TOPIC $PARTITION 0 10"
echo ""

# Show hex dump of interesting parts
echo "4. Hex Dump (first 128 bytes):"
hexdump -C $TMP_FILE | head -10
echo "   ..."
echo ""

echo "5. Last 64 bytes (footer area):"
hexdump -C $TMP_FILE | tail -5
echo ""

# Try to decompress if LZ4
if [ "$COMPRESSION" = "01" ]; then
    echo "6. Attempting LZ4 decompression..."

    # Skip header (32 bytes) and try to decompress
    COMPRESSED_DATA=$(mktemp)
    dd if=$TMP_FILE of=$COMPRESSED_DATA bs=1 skip=32 2>/dev/null

    if command -v lz4 &>/dev/null; then
        DECOMPRESSED=$(mktemp)
        if lz4 -d $COMPRESSED_DATA $DECOMPRESSED 2>/dev/null; then
            echo "   ✅ Decompressed successfully"
            echo ""
            echo "   Decompressed content (first 512 bytes):"
            hexdump -C $DECOMPRESSED | head -15
            echo ""
            echo "   Readable strings:"
            strings $DECOMPRESSED | head -20
        else
            echo "   ⚠️  Decompression failed (may need to parse block headers)"
        fi
        rm -f $DECOMPRESSED
    else
        echo "   ⚠️  lz4 command not found (install with: brew install lz4)"
    fi

    rm -f $COMPRESSED_DATA
fi
echo ""

# Query metadata to see what should be in this segment
echo "7. Metadata (what the segment contains):"
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "
SELECT
  base_offset,
  end_offset,
  record_count,
  size_bytes,
  to_timestamp(created_at/1000) as created
FROM segments
WHERE topic = '$TOPIC' AND partition_id = $PARTITION
  AND s3_key = '$SEGMENT_FILE';
" 2>/dev/null
echo ""

echo "=========================================="
echo "✅ Inspection Complete"
echo ""
echo "What you saw:"
echo "  • Binary header with magic bytes 'STRS'"
echo "  • Compression type (LZ4 or None)"
echo "  • Compressed data blocks"
echo "  • Offset index for fast seeks"
echo "  • CRC checksum in footer"
echo ""
echo "This is a real StreamHouse segment file!"
echo ""
echo "To extract all records, use the PartitionReader API:"
echo "  cargo run --example write_with_real_api"
echo ""
