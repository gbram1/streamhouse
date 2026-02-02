# StreamHouse Production Demo

A realistic e-commerce order pipeline demonstrating StreamHouse integration patterns.

## Architecture

```
┌──────────────────┐       ┌─────────────────┐       ┌──────────────────┐
│  Order Service   │──────▶│   StreamHouse   │◀──────│ Analytics Service│
│  (Producer)      │       │    [orders]     │       │  (Consumer)      │
└──────────────────┘       └────────┬────────┘       └──────────────────┘
     REST API                       │                   Real-time metrics
     Port 3001                      │
                                    │
                           ┌────────▼────────┐
                           │ Inventory       │
                           │ Service         │
                           │ (Consumer +     │──────▶ [inventory-updates]
                           │  Producer)      │
                           └─────────────────┘
                              Stock tracking
```

## Services

### 1. Order Service (Producer)

A REST API that receives orders and produces them to StreamHouse.

**Endpoints:**
- `POST /orders` - Create a new order
- `GET /health` - Health check
- `GET /stats` - Production statistics

**Integration Pattern:** REST → gRPC Producer

```rust
// Key code pattern
let producer = StreamHouseClient::new(channel);

async fn create_order(order: CreateOrderRequest) {
    let record = Record {
        key: order.customer_id.into_bytes(),  // Partition by customer
        value: serde_json::to_vec(&order)?,
        headers: Default::default(),
    };

    producer.produce_batch(ProduceBatchRequest {
        topic: "orders".to_string(),
        partition,  // Calculated from customer_id hash
        records: vec![record],
    }).await?;
}
```

### 2. Analytics Service (Consumer)

Consumes orders and calculates real-time metrics.

**Features:**
- Total revenue tracking
- Order count
- Average order value
- Top customers by revenue
- Top products by quantity
- Live dashboard with auto-refresh

**Integration Pattern:** gRPC Consumer

```rust
// Key code pattern
loop {
    let response = client.consume(ConsumeRequest {
        topic: "orders".to_string(),
        partition,
        offset,
        max_records: 100,
        consumer_group: Some("analytics-pipeline".to_string()),
    }).await?;

    for record in response.records {
        let order: OrderEvent = serde_json::from_slice(&record.value)?;
        analytics.process_order(&order);
    }
}
```

### 3. Inventory Service (Consumer + Producer)

Consumes orders, updates inventory, produces inventory events.

**Features:**
- Real-time stock tracking
- Low stock alerts
- Out of stock detection
- Inventory update events

**Integration Pattern:** Consumer-Producer (Event Sourcing)

```rust
// Key code pattern: Consume from one topic, produce to another
let order = consume_from("orders").await?;

for item in order.items {
    let update = inventory.reserve_item(&item.sku, item.quantity);
    produce_to("inventory-updates", &update).await?;
}
```

## Quick Start

### Prerequisites

1. Start StreamHouse server:
   ```bash
   cd /path/to/streamhouse
   ./start-server.sh
   ```

2. Verify server is running:
   ```bash
   curl http://localhost:8080/health
   # {"status":"ok"}
   ```

### Run the Demo

```bash
# Option 1: Full automated demo
cd examples/production-demo
./run-demo.sh

# Option 2: Step by step
./run-demo.sh setup     # Create topics
./run-demo.sh build     # Build services
./run-demo.sh start     # Start order & inventory services
./run-demo.sh orders    # Send 20 test orders
./run-demo.sh analytics # Watch real-time dashboard (new terminal)
./run-demo.sh stop      # Stop all services
```

### Manual Testing

```bash
# Start Order Service
cargo run -p order-service --release

# In another terminal, send an order
curl -X POST http://localhost:3001/orders \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "cust-123",
    "items": [
      {"sku": "WIDGET-1", "quantity": 2, "price": 29.99},
      {"sku": "GADGET-A", "quantity": 1, "price": 19.99}
    ]
  }'

# Response:
# {
#   "order_id": "ORD-A1B2C3D4",
#   "status": "created",
#   "total_amount": 79.97,
#   "created_at": "2026-02-02T12:00:00Z"
# }
```

## Topics

| Topic | Partitions | Purpose |
|-------|------------|---------|
| `orders` | 6 | Order events (partitioned by customer_id) |
| `inventory-updates` | 4 | Inventory changes (partitioned by SKU) |

## Data Flow

1. **Order Created**
   - User sends POST to Order Service
   - Order Service produces to `orders` topic
   - Key = customer_id (ensures ordering per customer)

2. **Analytics Updated**
   - Analytics Service consumes from `orders`
   - Aggregates revenue, counts, top customers
   - Displays real-time dashboard

3. **Inventory Reserved**
   - Inventory Service consumes from `orders`
   - Reserves stock for each item
   - Produces to `inventory-updates` topic
   - Key = SKU (ensures ordering per product)

## Key Patterns Demonstrated

### 1. Key-Based Partitioning

```rust
// All orders from same customer go to same partition
// Guarantees ordering within a customer
let partition = hash(customer_id) % num_partitions;
```

### 2. Consumer Groups

```rust
// Multiple instances can share the load
consumer_group: Some("analytics-pipeline".to_string())
```

### 3. Event Sourcing

```rust
// Inventory service reacts to order events
// Produces new events for downstream consumers
consume("orders") → process → produce("inventory-updates")
```

### 4. Persistent Connections

```rust
// Single connection for all requests (high performance)
let channel = Channel::from_static(GRPC_ADDR)
    .tcp_keepalive(Some(Duration::from_secs(30)))
    .http2_keep_alive_interval(Duration::from_secs(20))
    .connect()
    .await?;
```

## Performance

With default settings:
- Order Service: ~10,000 orders/sec (batched)
- Analytics Service: Processes ~50,000 events/sec
- Inventory Service: ~30,000 updates/sec

## Monitoring

View metrics in StreamHouse Web Console:
- http://localhost:8080 - Dashboard
- http://localhost:8080/topics - Topic details
- http://localhost:8080/consumers - Consumer lag

## Files

```
production-demo/
├── Cargo.toml              # Workspace config
├── README.md               # This file
├── run-demo.sh             # Demo script
├── order-service/
│   ├── Cargo.toml
│   └── src/main.rs         # REST API + Producer
├── analytics-service/
│   ├── Cargo.toml
│   └── src/main.rs         # Consumer + Dashboard
└── inventory-service/
    ├── Cargo.toml
    └── src/main.rs         # Consumer + Producer
```

## Next Steps

After running this demo, you can:

1. **Add more consumers** - Create a shipping service, notification service, etc.
2. **Add schemas** - Use the Schema Registry for Avro validation
3. **Scale horizontally** - Run multiple instances of each service
4. **Add monitoring** - Connect Prometheus/Grafana for metrics
5. **Deploy to Kubernetes** - Use the Helm charts (coming soon)
