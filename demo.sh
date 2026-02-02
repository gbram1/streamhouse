#!/bin/bash
# StreamHouse Full System Demo
# This script demonstrates the entire StreamHouse system:
# - Multiple topics with different partition counts
# - Schema registration and validation
# - Message production across partitions
# - Consumer groups with offsets and lag

set -e

API_URL="${API_URL:-http://localhost:8080}"
SCHEMA_URL="${SCHEMA_URL:-http://localhost:8080/schemas}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║         StreamHouse Full System Demo                      ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if server is running
echo -e "${YELLOW}[1/8] Checking server health...${NC}"
if ! curl -s "${API_URL}/health" > /dev/null 2>&1; then
    echo -e "${RED}Error: Server is not running at ${API_URL}${NC}"
    echo "Please start the server first with: ./start-server.sh"
    exit 1
fi
echo -e "${GREEN}✓ Server is healthy${NC}"
echo ""

# Create Topics
echo -e "${YELLOW}[2/8] Creating topics...${NC}"

# Orders topic - 6 partitions for high throughput
echo -n "  Creating 'orders' topic (6 partitions)... "
curl -s -X POST "${API_URL}/api/v1/topics" \
  -H "Content-Type: application/json" \
  -d '{"name": "orders", "partitions": 6, "replication_factor": 1, "retention_hours": 168}' > /dev/null 2>&1 || true
echo -e "${GREEN}done${NC}"

# Users topic - 3 partitions
echo -n "  Creating 'users' topic (3 partitions)... "
curl -s -X POST "${API_URL}/api/v1/topics" \
  -H "Content-Type: application/json" \
  -d '{"name": "users", "partitions": 3, "replication_factor": 1, "retention_hours": 168}' > /dev/null 2>&1 || true
echo -e "${GREEN}done${NC}"

# Inventory topic - 4 partitions
echo -n "  Creating 'inventory' topic (4 partitions)... "
curl -s -X POST "${API_URL}/api/v1/topics" \
  -H "Content-Type: application/json" \
  -d '{"name": "inventory", "partitions": 4, "replication_factor": 1, "retention_hours": 168}' > /dev/null 2>&1 || true
echo -e "${GREEN}done${NC}"

# Events topic - 2 partitions for low volume
echo -n "  Creating 'events' topic (2 partitions)... "
curl -s -X POST "${API_URL}/api/v1/topics" \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 2, "replication_factor": 1, "retention_hours": 168}' > /dev/null 2>&1 || true
echo -e "${GREEN}done${NC}"
echo ""

# Register Schemas
echo -e "${YELLOW}[3/8] Registering schemas...${NC}"

# Orders schema (Avro)
echo -n "  Registering 'orders-value' schema... "
curl -s -X POST "${SCHEMA_URL}/subjects/orders-value/versions" \
  -H "Content-Type: application/vnd.schemaregistry.v1+json" \
  -d '{
    "schemaType": "AVRO",
    "schema": "{\"type\": \"record\", \"name\": \"Order\", \"namespace\": \"com.streamhouse\", \"fields\": [{\"name\": \"order_id\", \"type\": \"string\"}, {\"name\": \"customer_id\", \"type\": \"string\"}, {\"name\": \"amount\", \"type\": \"double\"}, {\"name\": \"status\", \"type\": {\"type\": \"enum\", \"name\": \"OrderStatus\", \"symbols\": [\"PENDING\", \"PROCESSING\", \"SHIPPED\", \"DELIVERED\", \"CANCELLED\"]}}, {\"name\": \"created_at\", \"type\": \"long\"}]}"
  }' > /dev/null 2>&1 || true
echo -e "${GREEN}done${NC}"

# Users schema (Avro)
echo -n "  Registering 'users-value' schema... "
curl -s -X POST "${SCHEMA_URL}/subjects/users-value/versions" \
  -H "Content-Type: application/vnd.schemaregistry.v1+json" \
  -d '{
    "schemaType": "AVRO",
    "schema": "{\"type\": \"record\", \"name\": \"User\", \"namespace\": \"com.streamhouse\", \"fields\": [{\"name\": \"user_id\", \"type\": \"string\"}, {\"name\": \"email\", \"type\": \"string\"}, {\"name\": \"name\", \"type\": \"string\"}, {\"name\": \"created_at\", \"type\": \"long\"}]}"
  }' > /dev/null 2>&1 || true
echo -e "${GREEN}done${NC}"

# Inventory schema (JSON Schema)
echo -n "  Registering 'inventory-value' schema... "
curl -s -X POST "${SCHEMA_URL}/subjects/inventory-value/versions" \
  -H "Content-Type: application/vnd.schemaregistry.v1+json" \
  -d '{
    "schemaType": "JSON",
    "schema": "{\"$schema\": \"http://json-schema.org/draft-07/schema#\", \"type\": \"object\", \"properties\": {\"sku\": {\"type\": \"string\"}, \"quantity\": {\"type\": \"integer\"}, \"warehouse\": {\"type\": \"string\"}, \"updated_at\": {\"type\": \"integer\"}}, \"required\": [\"sku\", \"quantity\", \"warehouse\"]}"
  }' > /dev/null 2>&1 || true
echo -e "${GREEN}done${NC}"
echo ""

# Produce Messages
echo -e "${YELLOW}[4/8] Producing messages...${NC}"

# Produce 50 orders distributed across 6 partitions
echo -n "  Producing 50 orders... "
for i in $(seq 1 50); do
    customer_id="customer-$((i % 10))"
    partition=$((i % 6))
    status_idx=$((i % 5))
    statuses=("PENDING" "PROCESSING" "SHIPPED" "DELIVERED" "CANCELLED")
    status=${statuses[$status_idx]}
    # Value must be a JSON string (escaped JSON)
    value="{\\\"order_id\\\": \\\"order-${i}\\\", \\\"customer_id\\\": \\\"${customer_id}\\\", \\\"amount\\\": $((100 + i * 10)).50, \\\"status\\\": \\\"${status}\\\", \\\"created_at\\\": $(date +%s)000}"

    curl -s -X POST "${API_URL}/api/v1/produce" \
      -H "Content-Type: application/json" \
      -d "{
        \"topic\": \"orders\",
        \"partition\": ${partition},
        \"key\": \"order-${i}\",
        \"value\": \"${value}\"
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done (50 messages)${NC}"

# Produce 20 users distributed across 3 partitions
echo -n "  Producing 20 users... "
for i in $(seq 1 20); do
    partition=$((i % 3))
    value="{\\\"user_id\\\": \\\"user-${i}\\\", \\\"email\\\": \\\"user${i}@example.com\\\", \\\"name\\\": \\\"User Number ${i}\\\", \\\"created_at\\\": $(date +%s)000}"

    curl -s -X POST "${API_URL}/api/v1/produce" \
      -H "Content-Type: application/json" \
      -d "{
        \"topic\": \"users\",
        \"partition\": ${partition},
        \"key\": \"user-${i}\",
        \"value\": \"${value}\"
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done (20 messages)${NC}"

# Produce 30 inventory updates distributed across 4 partitions
echo -n "  Producing 30 inventory updates... "
for i in $(seq 1 30); do
    partition=$((i % 4))
    warehouse_idx=$((i % 3))
    warehouses=("NYC" "LAX" "CHI")
    warehouse=${warehouses[$warehouse_idx]}
    value="{\\\"sku\\\": \\\"SKU-${i}\\\", \\\"quantity\\\": $((50 + i * 5)), \\\"warehouse\\\": \\\"${warehouse}\\\", \\\"updated_at\\\": $(date +%s)}"

    curl -s -X POST "${API_URL}/api/v1/produce" \
      -H "Content-Type: application/json" \
      -d "{
        \"topic\": \"inventory\",
        \"partition\": ${partition},
        \"key\": \"SKU-${i}\",
        \"value\": \"${value}\"
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done (30 messages)${NC}"

# Produce 15 events distributed across 2 partitions
echo -n "  Producing 15 events... "
for i in $(seq 1 15); do
    partition=$((i % 2))
    event_types=("PAGE_VIEW" "CLICK" "PURCHASE" "SIGNUP" "LOGIN")
    event_type=${event_types[$((i % 5))]}
    value="{\\\"event_type\\\": \\\"${event_type}\\\", \\\"user_id\\\": \\\"user-$((i % 10))\\\", \\\"timestamp\\\": $(date +%s)000}"

    curl -s -X POST "${API_URL}/api/v1/produce" \
      -H "Content-Type: application/json" \
      -d "{
        \"topic\": \"events\",
        \"partition\": ${partition},
        \"key\": \"event-${i}\",
        \"value\": \"${value}\"
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done (15 messages)${NC}"
echo ""

# Wait for messages to be flushed to storage
echo -e "${YELLOW}[5/8] Waiting for messages to be flushed to storage (10s)...${NC}"
sleep 10
echo -e "${GREEN}✓ Flush complete${NC}"
echo ""

# Create Consumer Groups with Different Offsets
echo -e "${YELLOW}[6/8] Creating consumer groups with offsets...${NC}"

# order-processor: fully caught up on orders
echo -n "  Creating 'order-processor' group (caught up)... "
for partition in $(seq 0 5); do
    curl -s -X POST "${API_URL}/api/v1/consumer-groups/commit" \
      -H "Content-Type: application/json" \
      -d "{
        \"groupId\": \"order-processor\",
        \"topic\": \"orders\",
        \"partition\": ${partition},
        \"offset\": 10
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done${NC}"

# analytics-pipeline: behind on orders (shows lag)
echo -n "  Creating 'analytics-pipeline' group (with lag)... "
for partition in $(seq 0 5); do
    curl -s -X POST "${API_URL}/api/v1/consumer-groups/commit" \
      -H "Content-Type: application/json" \
      -d "{
        \"groupId\": \"analytics-pipeline\",
        \"topic\": \"orders\",
        \"partition\": ${partition},
        \"offset\": 3
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done${NC}"

# user-service: reading users
echo -n "  Creating 'user-service' group... "
for partition in $(seq 0 2); do
    curl -s -X POST "${API_URL}/api/v1/consumer-groups/commit" \
      -H "Content-Type: application/json" \
      -d "{
        \"groupId\": \"user-service\",
        \"topic\": \"users\",
        \"partition\": ${partition},
        \"offset\": 5
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done${NC}"

# inventory-tracker: reading inventory with some lag
echo -n "  Creating 'inventory-tracker' group (with lag)... "
for partition in $(seq 0 3); do
    curl -s -X POST "${API_URL}/api/v1/consumer-groups/commit" \
      -H "Content-Type: application/json" \
      -d "{
        \"groupId\": \"inventory-tracker\",
        \"topic\": \"inventory\",
        \"partition\": ${partition},
        \"offset\": 2
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done${NC}"

# event-processor: reading events
echo -n "  Creating 'event-processor' group... "
for partition in $(seq 0 1); do
    curl -s -X POST "${API_URL}/api/v1/consumer-groups/commit" \
      -H "Content-Type: application/json" \
      -d "{
        \"groupId\": \"event-processor\",
        \"topic\": \"events\",
        \"partition\": ${partition},
        \"offset\": 7
      }" > /dev/null 2>&1 || true
done
echo -e "${GREEN}done${NC}"
echo ""

# Display Summary
echo -e "${YELLOW}[7/8] Fetching system summary...${NC}"
echo ""

# Topics
echo -e "${BLUE}Topics:${NC}"
curl -s "${API_URL}/api/v1/topics" | python3 -c "
import json, sys
topics = json.load(sys.stdin)
for t in topics:
    print(f\"  - {t['name']}: {t['partition_count']} partitions\")
" 2>/dev/null || echo "  (unable to parse response)"
echo ""

# Consumer Groups
echo -e "${BLUE}Consumer Groups:${NC}"
curl -s "${API_URL}/api/v1/consumer-groups" | python3 -c "
import json, sys
groups = json.load(sys.stdin)
for g in groups:
    print(f\"  - {g.get('groupId', g.get('group_id', 'unknown'))}: {len(g.get('topics', []))} topic(s)\")
" 2>/dev/null || echo "  (unable to parse response)"
echo ""

# Agents
echo -e "${BLUE}Agents:${NC}"
curl -s "${API_URL}/api/v1/agents" | python3 -c "
import json, sys
agents = json.load(sys.stdin)
for a in agents:
    leases = a.get('active_leases', 0)
    print(f\"  - {a['agent_id']}: {leases} active leases\")
" 2>/dev/null || echo "  (unable to parse response)"
echo ""

# Schemas
echo -e "${BLUE}Schemas:${NC}"
curl -s "${SCHEMA_URL}/subjects" | python3 -c "
import json, sys
subjects = json.load(sys.stdin)
for s in subjects:
    print(f\"  - {s}\")
" 2>/dev/null || echo "  (unable to parse response)"
echo ""

echo -e "${YELLOW}[8/8] Demo complete!${NC}"
echo ""
echo -e "${BLUE}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                    Summary                                 ║${NC}"
echo -e "${BLUE}╠═══════════════════════════════════════════════════════════╣${NC}"
echo -e "${BLUE}║  Topics Created:      4 (orders, users, inventory, events)║${NC}"
echo -e "${BLUE}║  Total Partitions:    15 (6 + 3 + 4 + 2)                  ║${NC}"
echo -e "${BLUE}║  Messages Produced:   115 (50 + 20 + 30 + 15)             ║${NC}"
echo -e "${BLUE}║  Consumer Groups:     5                                   ║${NC}"
echo -e "${BLUE}║  Schemas Registered:  3                                   ║${NC}"
echo -e "${BLUE}╠═══════════════════════════════════════════════════════════╣${NC}"
echo -e "${BLUE}║  Open the Web Console: http://localhost:3000              ║${NC}"
echo -e "${BLUE}║  API Documentation:    http://localhost:8080/swagger-ui   ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${GREEN}The StreamHouse system is now populated with demo data!${NC}"
echo ""
echo "Try these in the UI:"
echo "  - Dashboard: See real metrics and counts"
echo "  - Topics: Browse messages in each topic"
echo "  - Consumers: View consumer groups and their lag"
echo "  - Agents: See partition assignments (may take ~30s to populate)"
echo ""
