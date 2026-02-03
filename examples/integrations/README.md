# StreamHouse Integration Examples

Practical examples showing how to integrate StreamHouse with common systems and patterns.

## Examples

| Example | Description | Use Case |
|---------|-------------|----------|
| [http-webhook](http-webhook/) | Receive HTTP webhooks and forward to StreamHouse | Stripe, GitHub, Slack webhooks |
| [metrics-aggregator](metrics-aggregator/) | Consume events and compute real-time metrics | Analytics dashboards |
| [event-router](event-router/) | Route events to different topics based on content | Event-driven microservices |
| [dead-letter-queue](dead-letter-queue/) | Handle failed messages with retry logic | Error handling |
| [schema-evolution](schema-evolution/) | Demonstrate schema evolution with Avro | Data governance |

## Quick Start

All examples can be run against the local Docker Compose stack:

```bash
# Start StreamHouse
cd /path/to/streamhouse
docker compose up -d

# Run an example
cd examples/integrations/http-webhook
cargo run --release
```

## Prerequisites

- StreamHouse running (via Docker Compose)
- Rust 1.70+ (for compiling examples)
- curl (for testing)

## Example: HTTP Webhook

Receive webhooks from external services and publish to StreamHouse:

```bash
# Terminal 1: Start webhook server
cd examples/integrations/http-webhook
cargo run --release

# Terminal 2: Send test webhook
curl -X POST http://localhost:3030/webhook \
  -H "Content-Type: application/json" \
  -d '{"event": "order.created", "data": {"id": "123", "amount": 99.99}}'
```

## Example: Metrics Aggregator

Consume events and compute running metrics:

```bash
cd examples/integrations/metrics-aggregator
cargo run --release
```

Output:
```
[Metrics Update]
  Total events: 1,234
  Events/sec: 125.3
  Unique users: 89
  Top event: page_view (45%)
```

## Example: Event Router

Route events to different topics based on content:

```bash
cd examples/integrations/event-router
cargo run --release
```

Routes:
- `order.*` events → `orders` topic
- `user.*` events → `users` topic
- `error.*` events → `errors` topic

## Example: Dead Letter Queue

Handle failed processing with exponential backoff:

```bash
cd examples/integrations/dead-letter-queue
cargo run --release
```

Features:
- Automatic retry with exponential backoff
- Move to DLQ after max retries
- Metrics on success/failure rates

## Building Your Own Integration

### Basic Producer Pattern

```rust
use reqwest::Client;
use serde_json::json;

async fn produce_to_streamhouse(
    client: &Client,
    topic: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), reqwest::Error> {
    client
        .post("http://localhost:8080/api/v1/produce")
        .json(&json!({
            "topic": topic,
            "key": key,
            "value": value.to_string()
        }))
        .send()
        .await?;
    Ok(())
}
```

### Basic Consumer Pattern

```rust
use reqwest::Client;

async fn consume_from_streamhouse(
    client: &Client,
    topic: &str,
    partition: u32,
    offset: u64,
) -> Result<Vec<Message>, reqwest::Error> {
    let response = client
        .get(format!(
            "http://localhost:8080/api/v1/consume?topic={}&partition={}&offset={}&limit=100",
            topic, partition, offset
        ))
        .send()
        .await?
        .json()
        .await?;
    Ok(response)
}
```

### With Schema Validation

```rust
// Register schema first
let schema = json!({
    "type": "record",
    "name": "Event",
    "fields": [
        {"name": "id", "type": "string"},
        {"name": "timestamp", "type": "long"},
        {"name": "data", "type": "string"}
    ]
});

client
    .post("http://localhost:8080/schemas/subjects/events-value/versions")
    .json(&json!({
        "schema": schema.to_string(),
        "schemaType": "AVRO"
    }))
    .send()
    .await?;
```

## Production Considerations

1. **Connection Pooling** - Reuse HTTP clients for better performance
2. **Batching** - Use `/api/v1/produce/batch` for high throughput
3. **Error Handling** - Implement retry logic with exponential backoff
4. **Monitoring** - Track produce/consume latencies and error rates
5. **Schema Evolution** - Use Avro schemas for forward/backward compatibility

## See Also

- [Production Demo](../production-demo/) - Complete e-commerce pipeline
- [QUICKSTART.md](../../docs/QUICKSTART.md) - Getting started guide
- [BENCHMARKS.md](../../docs/BENCHMARKS.md) - Performance benchmarks
