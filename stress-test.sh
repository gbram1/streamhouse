#!/bin/bash
# StreamHouse Stress Test - 10k messages per topic

API_URL="${API_URL:-http://localhost:8080}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║         StreamHouse Stress Test (10k per topic)           ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Function to send messages to a topic
send_messages() {
    local topic=$1
    local count=$2
    local partitions=$3
    
    echo -e "${YELLOW}Sending ${count} messages to ${topic}...${NC}"
    
    local start_time=$(date +%s.%N)
    local success=0
    local failed=0
    
    for i in $(seq 1 $count); do
        partition=$((i % partitions))
        value="{\\\"id\\\": ${i}, \\\"topic\\\": \\\"${topic}\\\", \\\"timestamp\\\": $(date +%s)}"
        
        response=$(curl -s -w "%{http_code}" -o /dev/null -X POST "${API_URL}/api/v1/produce" \
          -H "Content-Type: application/json" \
          -d "{
            \"topic\": \"${topic}\",
            \"partition\": ${partition},
            \"key\": \"key-${i}\",
            \"value\": \"${value}\"
          }" 2>/dev/null)
        
        if [ "$response" = "200" ]; then
            ((success++))
        else
            ((failed++))
        fi
        
        # Progress indicator every 1000 messages
        if [ $((i % 1000)) -eq 0 ]; then
            echo -ne "  Progress: ${i}/${count} (${success} ok, ${failed} failed)\r"
        fi
    done
    
    local end_time=$(date +%s.%N)
    local duration=$(echo "$end_time - $start_time" | bc)
    local rate=$(echo "scale=2; $success / $duration" | bc)
    
    echo -e "\n  ${GREEN}✓ ${topic}: ${success} messages in ${duration}s (${rate} msg/s)${NC}"
    if [ $failed -gt 0 ]; then
        echo -e "  ${RED}  Failed: ${failed}${NC}"
    fi
}

# Check server health
echo -e "${YELLOW}Checking server health...${NC}"
if ! curl -s "${API_URL}/health" > /dev/null 2>&1; then
    echo -e "${RED}Server not responding at ${API_URL}${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Server healthy${NC}"
echo ""

# Get start time
TOTAL_START=$(date +%s.%N)

# Send 10k messages to each topic (running sequentially for accurate timing)
send_messages "orders" 10000 6
send_messages "users" 10000 3
send_messages "inventory" 10000 4
send_messages "events" 10000 2

# Calculate totals
TOTAL_END=$(date +%s.%N)
TOTAL_DURATION=$(echo "$TOTAL_END - $TOTAL_START" | bc)
TOTAL_RATE=$(echo "scale=2; 40000 / $TOTAL_DURATION" | bc)

echo ""
echo -e "${BLUE}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                    Stress Test Complete                    ║${NC}"
echo -e "${BLUE}╠═══════════════════════════════════════════════════════════╣${NC}"
echo -e "${BLUE}║  Total Messages:    40,000                                ║${NC}"
echo -e "${BLUE}║  Total Duration:    ${TOTAL_DURATION}s                              ${NC}"
echo -e "${BLUE}║  Average Rate:      ${TOTAL_RATE} msg/s                          ${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${YELLOW}Waiting 15s for flush to complete...${NC}"
sleep 15

# Check final counts
echo ""
echo -e "${BLUE}Final partition watermarks:${NC}"
for topic in orders users inventory events; do
    total=$(curl -s "${API_URL}/api/v1/topics/${topic}/partitions" | python3 -c "
import json, sys
data = json.load(sys.stdin)
total = sum(p.get('high_watermark', 0) for p in data)
print(total)
" 2>/dev/null || echo "?")
    echo "  ${topic}: ${total} messages"
done
