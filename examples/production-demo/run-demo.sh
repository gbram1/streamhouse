#!/bin/bash
# StreamHouse Production Demo
#
# This script demonstrates a complete e-commerce order pipeline:
# 1. Order Service: REST API → StreamHouse (Producer)
# 2. Analytics Service: StreamHouse → Real-time metrics (Consumer)
# 3. Inventory Service: StreamHouse → Inventory updates (Consumer + Producer)
#
# Usage:
#   ./run-demo.sh          # Run the full demo
#   ./run-demo.sh setup    # Just setup topics
#   ./run-demo.sh orders   # Just send test orders
#   ./run-demo.sh clean    # Clean up

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

API_URL="http://localhost:8080/api/v1"
ORDER_SERVICE_URL="http://localhost:3001"

print_header() {
    echo ""
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

check_server() {
    if ! curl -s "$API_URL/topics" > /dev/null 2>&1; then
        echo -e "${RED}Error: StreamHouse server is not running${NC}"
        echo "Start the server with: ./start-server.sh"
        exit 1
    fi
    echo -e "${GREEN}✓ StreamHouse server is running${NC}"
}

setup_topics() {
    print_header "Setting up Topics"

    # Create orders topic
    echo "Creating topic 'orders' (6 partitions)..."
    curl -s -X POST "$API_URL/topics" \
        -H "Content-Type: application/json" \
        -d '{"name": "orders", "partitions": 6, "replication_factor": 1}' \
        > /dev/null 2>&1 || true
    echo -e "${GREEN}✓ orders topic ready${NC}"

    # Create inventory-updates topic
    echo "Creating topic 'inventory-updates' (4 partitions)..."
    curl -s -X POST "$API_URL/topics" \
        -H "Content-Type: application/json" \
        -d '{"name": "inventory-updates", "partitions": 4, "replication_factor": 1}' \
        > /dev/null 2>&1 || true
    echo -e "${GREEN}✓ inventory-updates topic ready${NC}"

    # List topics
    echo ""
    echo "Current topics:"
    curl -s "$API_URL/topics" | jq -r '.[] | "  - \(.name) (\(.partitions) partitions)"' 2>/dev/null || \
        curl -s "$API_URL/topics"
}

build_services() {
    print_header "Building Services"

    cd "$SCRIPT_DIR"

    echo "Building order-service..."
    cargo build -p order-service --release 2>&1 | tail -1

    echo "Building analytics-service..."
    cargo build -p analytics-service --release 2>&1 | tail -1

    echo "Building inventory-service..."
    cargo build -p inventory-service --release 2>&1 | tail -1

    echo -e "${GREEN}✓ All services built${NC}"
}

start_services() {
    print_header "Starting Services"

    # Kill any existing services
    pkill -f "order-service" 2>/dev/null || true
    pkill -f "analytics-service" 2>/dev/null || true
    pkill -f "inventory-service" 2>/dev/null || true
    sleep 1

    # Start Order Service
    echo "Starting Order Service on port 3001..."
    cd "$PROJECT_ROOT"
    ./target/release/order-service > /tmp/order-service.log 2>&1 &
    ORDER_PID=$!
    sleep 2

    if ps -p $ORDER_PID > /dev/null 2>&1; then
        echo -e "${GREEN}✓ Order Service started (PID: $ORDER_PID)${NC}"
    else
        echo -e "${RED}✗ Order Service failed to start${NC}"
        cat /tmp/order-service.log
        exit 1
    fi

    # Start Inventory Service
    echo "Starting Inventory Service..."
    ./target/release/inventory-service > /tmp/inventory-service.log 2>&1 &
    INVENTORY_PID=$!
    sleep 2

    if ps -p $INVENTORY_PID > /dev/null 2>&1; then
        echo -e "${GREEN}✓ Inventory Service started (PID: $INVENTORY_PID)${NC}"
    else
        echo -e "${RED}✗ Inventory Service failed to start${NC}"
        cat /tmp/inventory-service.log
        exit 1
    fi

    echo ""
    echo "Services running:"
    echo "  - Order Service:     http://localhost:3001 (PID: $ORDER_PID)"
    echo "  - Inventory Service: (background, PID: $INVENTORY_PID)"
    echo ""
    echo "Logs:"
    echo "  - tail -f /tmp/order-service.log"
    echo "  - tail -f /tmp/inventory-service.log"
}

send_test_orders() {
    print_header "Sending Test Orders"

    if ! curl -s "$ORDER_SERVICE_URL/health" > /dev/null 2>&1; then
        echo -e "${RED}Error: Order Service is not running${NC}"
        echo "Start it with: cargo run -p order-service --release"
        exit 1
    fi

    # Sample products
    PRODUCTS=(
        '{"sku": "WIDGET-1", "quantity": 2, "price": 29.99}'
        '{"sku": "WIDGET-2", "quantity": 1, "price": 49.99}'
        '{"sku": "GADGET-A", "quantity": 3, "price": 19.99}'
        '{"sku": "GADGET-B", "quantity": 1, "price": 99.99}'
        '{"sku": "TOOL-X", "quantity": 2, "price": 14.99}'
        '{"sku": "TOOL-Y", "quantity": 1, "price": 39.99}'
    )

    # Sample customers
    CUSTOMERS=("cust-alice" "cust-bob" "cust-charlie" "cust-diana" "cust-eve")

    echo "Sending 20 test orders..."
    echo ""

    for i in {1..20}; do
        # Random customer
        CUSTOMER=${CUSTOMERS[$((RANDOM % ${#CUSTOMERS[@]}))]}

        # Random 1-3 items per order
        NUM_ITEMS=$((RANDOM % 3 + 1))
        ITEMS=""
        for j in $(seq 1 $NUM_ITEMS); do
            PRODUCT=${PRODUCTS[$((RANDOM % ${#PRODUCTS[@]}))]}
            if [ -n "$ITEMS" ]; then
                ITEMS="$ITEMS, $PRODUCT"
            else
                ITEMS="$PRODUCT"
            fi
        done

        # Send order
        RESPONSE=$(curl -s -X POST "$ORDER_SERVICE_URL/orders" \
            -H "Content-Type: application/json" \
            -d "{\"customer_id\": \"$CUSTOMER\", \"items\": [$ITEMS]}")

        ORDER_ID=$(echo "$RESPONSE" | jq -r '.order_id' 2>/dev/null || echo "unknown")
        TOTAL=$(echo "$RESPONSE" | jq -r '.total_amount' 2>/dev/null || echo "0")

        printf "  Order %2d: %s | Customer: %-12s | Total: \$%s\n" \
            "$i" "$ORDER_ID" "$CUSTOMER" "$TOTAL"

        # Small delay between orders
        sleep 0.1
    done

    echo ""
    echo -e "${GREEN}✓ Sent 20 orders${NC}"
    echo ""

    # Show stats
    echo "Order Service stats:"
    curl -s "$ORDER_SERVICE_URL/stats" | jq . 2>/dev/null || curl -s "$ORDER_SERVICE_URL/stats"
}

run_analytics() {
    print_header "Starting Analytics Dashboard"

    echo "Starting Analytics Service..."
    echo "Press Ctrl+C to exit"
    echo ""

    cd "$PROJECT_ROOT"
    ./target/release/analytics-service
}

stop_services() {
    print_header "Stopping Services"

    pkill -f "order-service" 2>/dev/null && echo "Stopped order-service" || true
    pkill -f "analytics-service" 2>/dev/null && echo "Stopped analytics-service" || true
    pkill -f "inventory-service" 2>/dev/null && echo "Stopped inventory-service" || true

    echo -e "${GREEN}✓ All services stopped${NC}"
}

show_help() {
    echo "StreamHouse Production Demo"
    echo ""
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  (none)     Run the full demo (setup + build + start + orders)"
    echo "  setup      Create topics only"
    echo "  build      Build all services"
    echo "  start      Start all services"
    echo "  orders     Send test orders"
    echo "  analytics  Run analytics dashboard (interactive)"
    echo "  stop       Stop all services"
    echo "  help       Show this help"
    echo ""
    echo "Quick Start:"
    echo "  1. Start StreamHouse: ./start-server.sh"
    echo "  2. Run demo: cd examples/production-demo && ./run-demo.sh"
    echo "  3. Watch analytics: ./run-demo.sh analytics"
}

# Main
case "${1:-}" in
    setup)
        check_server
        setup_topics
        ;;
    build)
        build_services
        ;;
    start)
        check_server
        start_services
        ;;
    orders)
        send_test_orders
        ;;
    analytics)
        check_server
        run_analytics
        ;;
    stop)
        stop_services
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        print_header "StreamHouse Production Demo"
        echo "This demo shows a complete e-commerce order pipeline:"
        echo ""
        echo "  ┌──────────────────┐       ┌─────────────────┐"
        echo "  │  Order Service   │──────▶│   StreamHouse   │"
        echo "  │  (REST → gRPC)   │       │    (orders)     │"
        echo "  └──────────────────┘       └────────┬────────┘"
        echo "                                      │"
        echo "                        ┌─────────────┼─────────────┐"
        echo "                        │             │             │"
        echo "                        ▼             ▼             ▼"
        echo "               ┌────────────┐  ┌────────────┐  ┌────────────┐"
        echo "               │ Analytics  │  │ Inventory  │──▶│ inventory- │"
        echo "               │  Service   │  │  Service   │  │  updates   │"
        echo "               └────────────┘  └────────────┘  └────────────┘"
        echo ""

        check_server
        setup_topics
        build_services
        start_services

        echo ""
        echo -e "${YELLOW}Demo is ready!${NC}"
        echo ""
        echo "Next steps:"
        echo "  1. Send orders:     ./run-demo.sh orders"
        echo "  2. Watch analytics: ./run-demo.sh analytics (in another terminal)"
        echo "  3. Stop services:   ./run-demo.sh stop"
        echo ""
        echo "Or send orders manually:"
        echo "  curl -X POST http://localhost:3001/orders \\"
        echo "    -H \"Content-Type: application/json\" \\"
        echo "    -d '{\"customer_id\": \"test-user\", \"items\": [{\"sku\": \"WIDGET-1\", \"quantity\": 2, \"price\": 29.99}]}'"
        ;;
esac
