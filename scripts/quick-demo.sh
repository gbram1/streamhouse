#!/bin/bash
set -e

# Quick StreamHouse Demo - Shows data flowing through the pipeline
# Run this after server is started

echo "╔════════════════════════════════════════════════════════╗"
echo "║         StreamHouse Quick Demo - Data Pipeline         ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Check if server is running
if ! lsof -i :50051 > /dev/null 2>&1; then
    echo -e "${YELLOW}⚠ Server not running on port 50051${NC}"
    echo "Start the server first:"
    echo "  source .env.dev"
    echo "  cargo run --release --bin unified-server"
    exit 1
fi

# Set environment
export STREAMHOUSE_ADDR=http://localhost:50051

echo -e "${GREEN}✓${NC} Server is running"
echo

# ==============================================================================
# STEP 1: Create Topic
# ==============================================================================

echo -e "${BLUE}${BOLD}Step 1: Creating Topic${NC}"
echo "─────────────────────────────────────────"

TOPIC="demo-$(date +%s)"
echo "Topic name: $TOPIC"
echo

cargo run --bin streamctl -- topic create $TOPIC --partitions 4 2>&1 | grep -v "Compiling\|Finished\|Running"
echo

# ==============================================================================
# STEP 2: Produce Messages
# ==============================================================================

echo -e "${BLUE}${BOLD}Step 2: Producing Messages${NC}"
echo "─────────────────────────────────────────"
echo "Sending 10 messages across partitions..."
echo

for i in {1..10}; do
    PARTITION=$((i % 4))
    VALUE="{\"id\": $i, \"user\": \"customer_$i\", \"amount\": $((50 + RANDOM % 500)), \"timestamp\": $(date +%s)}"

    RESULT=$(cargo run --bin streamctl -- produce $TOPIC --partition $PARTITION --value "$VALUE" 2>&1 | grep "Offset:" | head -1)

    echo -e "  ${GREEN}✓${NC} Message $i → Partition $PARTITION $RESULT"
done

echo
echo -e "${GREEN}✓${NC} All messages produced!"
echo

# ==============================================================================
# STEP 3: View Data in Storage
# ==============================================================================

echo -e "${BLUE}${BOLD}Step 3: Checking Storage${NC}"
echo "─────────────────────────────────────────"

# SQLite metadata
echo "📊 Metadata (SQLite):"
echo
sqlite3 ./data/metadata.db "
SELECT '  Topic: ' || name || ' (' || partition_count || ' partitions)'
FROM topics
WHERE name='$TOPIC';
"

sqlite3 ./data/metadata.db "
SELECT '  Partition ' || p.partition_id || ': ' || p.high_watermark || ' messages'
FROM partitions p
JOIN topics t ON p.topic_id = t.id
WHERE t.name='$TOPIC'
ORDER BY p.partition_id;
"
echo

# Segment files
echo "💾 Data Segments (Local Filesystem):"
echo
SEGMENTS=$(find ./data/storage -path "*/$TOPIC/*" -name "*.seg" 2>/dev/null)
if [ -n "$SEGMENTS" ]; then
    echo "$SEGMENTS" | while read file; do
        SIZE=$(ls -lh "$file" | awk '{print $5}')
        echo "  $file ($SIZE)"
    done
else
    echo "  (No segments found yet - may be in cache)"
fi
echo

# ==============================================================================
# STEP 4: Consume Messages
# ==============================================================================

echo -e "${BLUE}${BOLD}Step 4: Consuming Messages${NC}"
echo "─────────────────────────────────────────"
echo "Reading from Partition 0:"
echo

cargo run --bin streamctl -- consume $TOPIC --partition 0 --limit 5 2>&1 | grep -v "Compiling\|Finished\|Running"
echo

echo "Reading from Partition 1:"
echo

cargo run --bin streamctl -- consume $TOPIC --partition 1 --limit 5 2>&1 | grep -v "Compiling\|Finished\|Running"
echo

# ==============================================================================
# STEP 5: Summary
# ==============================================================================

echo "╔════════════════════════════════════════════════════════╗"
echo "║                    Demo Summary                        ║"
echo "╚════════════════════════════════════════════════════════╝"
echo

echo -e "${CYAN}What Just Happened:${NC}"
echo "  1. Created topic '$TOPIC' with 4 partitions ✓"
echo "  2. Produced 10 messages across partitions ✓"
echo "  3. Messages stored in ./data/storage/ ✓"
echo "  4. Metadata tracked in ./data/metadata.db ✓"
echo "  5. Consumed messages from partitions ✓"
echo

echo -e "${CYAN}Data Locations:${NC}"
echo "  Metadata:  sqlite3 ./data/metadata.db"
echo "  Segments:  ./data/storage/data/$TOPIC/"
echo "  Logs:      ./data/server.log"
echo

echo -e "${CYAN}Next Steps:${NC}"
echo "  # Produce more messages"
echo "  cargo run --bin streamctl -- produce $TOPIC --partition 0 --value '{\"test\": \"data\"}'"
echo
echo "  # Consume from different offset"
echo "  cargo run --bin streamctl -- consume $TOPIC --partition 0 --offset 2 --limit 3"
echo
echo "  # View all topics"
echo "  cargo run --bin streamctl -- topic list"
echo
echo "  # Query metadata"
echo "  sqlite3 ./data/metadata.db 'SELECT * FROM topics;'"
echo

echo -e "${GREEN}${BOLD}✓ Demo Complete!${NC}"
echo
