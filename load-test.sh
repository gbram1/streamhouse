#!/usr/bin/env bash
# StreamHouse Load Test Script
# Creates 3 topics, 2 schemas each, 10k messages per schema = 60k total

set -e

API_URL="http://localhost:8080"
SCHEMA_URL="http://localhost:8080/schemas"

echo "=== StreamHouse Load Test ==="
echo "Creating 3 topics, 2 schemas each, 10k messages per schema"
echo ""

# Step 1: Create topics
echo "Step 1: Creating topics..."
for topic in orders users events; do
    echo -n "  Creating topic: $topic... "
    response=$(curl -s -w "%{http_code}" -o /tmp/topic_response.txt -X POST "$API_URL/api/v1/topics" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"$topic\", \"partitions\": 6, \"replication_factor\": 1}")

    if [ "$response" = "200" ] || [ "$response" = "201" ]; then
        echo "OK"
    elif [ "$response" = "409" ]; then
        echo "Already exists"
    else
        echo "Failed ($response)"
    fi
done
echo ""

# Step 2: Register schemas (v1)
echo "Step 2: Registering schemas (v1)..."

# Orders v1
echo -n "  Registering orders-value v1... "
curl -s -X POST "$SCHEMA_URL/subjects/orders-value/versions" \
    -H "Content-Type: application/vnd.schemaregistry.v1+json" \
    -d '{"schema": "{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"order_id\",\"type\":\"string\"},{\"name\":\"customer_id\",\"type\":\"string\"},{\"name\":\"amount\",\"type\":\"double\"},{\"name\":\"status\",\"type\":\"string\"}]}"}' > /dev/null && echo "OK" || echo "Failed"

# Users v1
echo -n "  Registering users-value v1... "
curl -s -X POST "$SCHEMA_URL/subjects/users-value/versions" \
    -H "Content-Type: application/vnd.schemaregistry.v1+json" \
    -d '{"schema": "{\"type\":\"record\",\"name\":\"User\",\"fields\":[{\"name\":\"user_id\",\"type\":\"string\"},{\"name\":\"email\",\"type\":\"string\"},{\"name\":\"name\",\"type\":\"string\"},{\"name\":\"created_at\",\"type\":\"long\"}]}"}' > /dev/null && echo "OK" || echo "Failed"

# Events v1
echo -n "  Registering events-value v1... "
curl -s -X POST "$SCHEMA_URL/subjects/events-value/versions" \
    -H "Content-Type: application/vnd.schemaregistry.v1+json" \
    -d '{"schema": "{\"type\":\"record\",\"name\":\"Event\",\"fields\":[{\"name\":\"event_id\",\"type\":\"string\"},{\"name\":\"event_type\",\"type\":\"string\"},{\"name\":\"payload\",\"type\":\"string\"},{\"name\":\"timestamp\",\"type\":\"long\"}]}"}' > /dev/null && echo "OK" || echo "Failed"
echo ""

# Step 3: Register schemas (v2)
echo "Step 3: Registering schemas (v2)..."

# Orders v2 (added currency)
echo -n "  Registering orders-value v2... "
curl -s -X POST "$SCHEMA_URL/subjects/orders-value/versions" \
    -H "Content-Type: application/vnd.schemaregistry.v1+json" \
    -d '{"schema": "{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"order_id\",\"type\":\"string\"},{\"name\":\"customer_id\",\"type\":\"string\"},{\"name\":\"amount\",\"type\":\"double\"},{\"name\":\"status\",\"type\":\"string\"},{\"name\":\"currency\",\"type\":\"string\",\"default\":\"USD\"}]}"}' > /dev/null && echo "OK" || echo "Failed"

# Users v2 (added verified)
echo -n "  Registering users-value v2... "
curl -s -X POST "$SCHEMA_URL/subjects/users-value/versions" \
    -H "Content-Type: application/vnd.schemaregistry.v1+json" \
    -d '{"schema": "{\"type\":\"record\",\"name\":\"User\",\"fields\":[{\"name\":\"user_id\",\"type\":\"string\"},{\"name\":\"email\",\"type\":\"string\"},{\"name\":\"name\",\"type\":\"string\"},{\"name\":\"created_at\",\"type\":\"long\"},{\"name\":\"verified\",\"type\":\"boolean\",\"default\":false}]}"}' > /dev/null && echo "OK" || echo "Failed"

# Events v2 (added source)
echo -n "  Registering events-value v2... "
curl -s -X POST "$SCHEMA_URL/subjects/events-value/versions" \
    -H "Content-Type: application/vnd.schemaregistry.v1+json" \
    -d '{"schema": "{\"type\":\"record\",\"name\":\"Event\",\"fields\":[{\"name\":\"event_id\",\"type\":\"string\"},{\"name\":\"event_type\",\"type\":\"string\"},{\"name\":\"payload\",\"type\":\"string\"},{\"name\":\"timestamp\",\"type\":\"long\"},{\"name\":\"source\",\"type\":\"string\",\"default\":\"unknown\"}]}"}' > /dev/null && echo "OK" || echo "Failed"
echo ""

# Step 4: Send messages in batches
echo "Step 4: Sending 60,000 messages..."
echo ""

BATCH_SIZE=50
MESSAGES_PER_TOPIC_VERSION=10000
TOTAL=0

send_orders_batch() {
    local start=$1
    local count=$2
    local version=$3

    records=""
    for i in $(seq 0 $((count - 1))); do
        idx=$((start + i))
        amount=$((RANDOM % 10000))
        if [ -n "$records" ]; then records+=","; fi
        if [ "$version" = "v1" ]; then
            records+="{\"key\":\"order-$idx\",\"value\":\"{\\\"order_id\\\":\\\"ord-$idx\\\",\\\"customer_id\\\":\\\"cust-$((idx % 1000))\\\",\\\"amount\\\":$amount.99,\\\"status\\\":\\\"pending\\\"}\"}"
        else
            records+="{\"key\":\"order-$idx\",\"value\":\"{\\\"order_id\\\":\\\"ord-$idx\\\",\\\"customer_id\\\":\\\"cust-$((idx % 1000))\\\",\\\"amount\\\":$amount.99,\\\"status\\\":\\\"pending\\\",\\\"currency\\\":\\\"USD\\\"}\"}"
        fi
    done

    curl -s -X POST "$API_URL/api/v1/produce/batch" \
        -H "Content-Type: application/json" \
        -d "{\"topic\":\"orders\",\"records\":[$records]}" > /dev/null
}

send_users_batch() {
    local start=$1
    local count=$2
    local version=$3
    local ts=$(date +%s)

    records=""
    for i in $(seq 0 $((count - 1))); do
        idx=$((start + i))
        if [ -n "$records" ]; then records+=","; fi
        if [ "$version" = "v1" ]; then
            records+="{\"key\":\"user-$idx\",\"value\":\"{\\\"user_id\\\":\\\"user-$idx\\\",\\\"email\\\":\\\"user$idx@test.com\\\",\\\"name\\\":\\\"User $idx\\\",\\\"created_at\\\":$ts}\"}"
        else
            records+="{\"key\":\"user-$idx\",\"value\":\"{\\\"user_id\\\":\\\"user-$idx\\\",\\\"email\\\":\\\"user$idx@test.com\\\",\\\"name\\\":\\\"User $idx\\\",\\\"created_at\\\":$ts,\\\"verified\\\":true}\"}"
        fi
    done

    curl -s -X POST "$API_URL/api/v1/produce/batch" \
        -H "Content-Type: application/json" \
        -d "{\"topic\":\"users\",\"records\":[$records]}" > /dev/null
}

send_events_batch() {
    local start=$1
    local count=$2
    local version=$3
    local ts=$(date +%s)

    records=""
    for i in $(seq 0 $((count - 1))); do
        idx=$((start + i))
        if [ -n "$records" ]; then records+=","; fi
        if [ "$version" = "v1" ]; then
            records+="{\"key\":\"event-$idx\",\"value\":\"{\\\"event_id\\\":\\\"evt-$idx\\\",\\\"event_type\\\":\\\"click\\\",\\\"payload\\\":\\\"{}\\\",\\\"timestamp\\\":$ts}\"}"
        else
            records+="{\"key\":\"event-$idx\",\"value\":\"{\\\"event_id\\\":\\\"evt-$idx\\\",\\\"event_type\\\":\\\"click\\\",\\\"payload\\\":\\\"{}\\\",\\\"timestamp\\\":$ts,\\\"source\\\":\\\"web\\\"}\"}"
        fi
    done

    curl -s -X POST "$API_URL/api/v1/produce/batch" \
        -H "Content-Type: application/json" \
        -d "{\"topic\":\"events\",\"records\":[$records]}" > /dev/null
}

# Send to each topic with each schema version
for topic in orders users events; do
    for version in v1 v2; do
        echo -n "  Sending to $topic ($version): "
        sent=0

        while [ $sent -lt $MESSAGES_PER_TOPIC_VERSION ]; do
            remaining=$((MESSAGES_PER_TOPIC_VERSION - sent))
            batch=$((remaining < BATCH_SIZE ? remaining : BATCH_SIZE))

            case $topic in
                orders) send_orders_batch $sent $batch $version ;;
                users) send_users_batch $sent $batch $version ;;
                events) send_events_batch $sent $batch $version ;;
            esac

            sent=$((sent + batch))
            TOTAL=$((TOTAL + batch))

            # Progress every 1000 messages
            if [ $((sent % 1000)) -eq 0 ]; then
                echo -n "."
            fi
        done

        echo " $sent messages"
    done
done

echo ""
echo "=== Load Test Complete ==="
echo "Total messages sent: $TOTAL"
echo ""
echo "Check results:"
echo "  - Grafana:  http://localhost:3001"
echo "  - UI:       http://localhost:3000"
echo "  - Topics:   curl $API_URL/api/v1/topics"
echo "  - Schemas:  curl $SCHEMA_URL/subjects"
echo "  - Metrics:  curl $API_URL/metrics | grep streamhouse"
