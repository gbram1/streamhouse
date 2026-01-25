# StreamHouse Client

High-level Producer and Consumer APIs for interacting with StreamHouse.

## Features

### Producer API (Phase 5.1 - Complete)

- ✅ Key-based partitioning (consistent hashing)
- ✅ Explicit partition selection
- ✅ Agent discovery and health monitoring
- ✅ Topic metadata caching
- ✅ Builder pattern configuration
- ✅ Direct storage writes (Phase 5.1)
- ⏳ gRPC communication with agents (Phase 5.2)
- ⏳ Batching and compression (Phase 5.2)
- ⏳ Retries and error handling (Phase 5.2)

### Consumer API (Phase 5.2 - Planned)

- ⏳ Consumer groups with offset management
- ⏳ Partition assignment and rebalancing
- ⏳ Exactly-once semantics
- ⏳ Poll-based consumption

## Quick Start

### Producer

```rust
use streamhouse_client::Producer;
use streamhouse_metadata::{SqliteMetadataStore, MetadataStore, TopicConfig};
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup metadata store
    let metadata: Arc<dyn MetadataStore> = Arc::new(
        SqliteMetadataStore::new("metadata.db").await?
    );

    // Create topic
    metadata.create_topic(TopicConfig {
        name: "orders".to_string(),
        partition_count: 3,
        retention_ms: Some(86400000), // 1 day
        config: HashMap::new(),
    }).await?;

    // Create producer
    let producer = Producer::builder()
        .metadata_store(metadata)
        .agent_group("prod")
        .compression_enabled(true)
        .build()
        .await?;

    // Send with key-based partitioning
    let result = producer.send(
        "orders",
        Some(b"user123"),    // Key determines partition
        b"order data",       // Value
        None,                // Auto-select partition
    ).await?;

    println!("Sent to partition {} at offset {}",
        result.partition, result.offset);

    // Send to explicit partition
    let result = producer.send(
        "orders",
        Some(b"user456"),
        b"order data 2",
        Some(1),             // Explicit partition
    ).await?;

    // Close producer
    producer.close().await?;

    Ok(())
}
```

## Examples

Run the included examples to see the Producer in action:

```bash
# Simple producer example
cargo run --package streamhouse-client --example simple_producer
```

## Configuration

### ProducerConfig

| Field | Default | Description |
|-------|---------|-------------|
| `metadata_store` | (required) | Metadata store for topic/agent discovery |
| `agent_group` | `"default"` | Agent group to send requests to |
| `batch_size` | `100` | Records to batch before flushing |
| `batch_timeout` | `100ms` | Max time to wait before flushing |
| `compression_enabled` | `true` | Enable LZ4 compression |
| `request_timeout` | `30s` | Timeout for agent requests |
| `agent_refresh_interval` | `30s` | How often to refresh agent list |
| `max_retries` | `3` | Max retries on failure |
| `retry_backoff` | `100ms` | Base retry backoff duration |

## Architecture

### Phase 5.1: Direct Storage Writes

```text
┌──────────┐
│ Producer │
└────┬─────┘
     │ send(topic, key, value)
     ▼
┌─────────────────┐
│ PartitionWriter │
└────┬────────────┘
     │ append()
     ▼
┌─────────────┐
│   MinIO     │ (S3-compatible)
└─────────────┘
```

In Phase 5.1, the Producer writes directly to storage:
- Creates a new `PartitionWriter` for each send
- Writes segment to MinIO via S3 API
- Updates metadata in PostgreSQL/SQLite

**Limitations:**
- Each send creates a new writer (inefficient)
- No batching or pooling
- No agent coordination

### Phase 5.2: Agent Communication (Planned)

```text
┌──────────┐
│ Producer │
└────┬─────┘
     │ send(topic, key, value)
     ▼
┌─────────────────┐
│ Agent Discovery │ (list_agents via metadata)
└────┬────────────┘
     │ gRPC ProduceRequest
     ▼
┌──────────────┐
│ StreamHouse  │
│    Agent     │
└────┬─────────┘
     │ PartitionWriter (pooled)
     ▼
┌─────────────┐
│   MinIO     │
└─────────────┘
```

In Phase 5.2, the Producer will:
- Maintain connection pool to agents
- Send batched records via gRPC
- Let agents handle write pooling and coordination
- Implement retries and backpressure

## Testing

```bash
# Run unit tests
cargo test --package streamhouse-client --lib

# Run integration tests
cargo test --package streamhouse-client --test producer_integration_test

# All tests
cargo test --package streamhouse-client
```

## Performance

### Phase 5.1 (Direct Writes)

- Throughput: Limited by S3 API overhead (creates new writer per send)
- Latency: ~200ms per send (S3 connection + write + upload)
- Use case: Testing and development

### Phase 5.2 (Agent Communication) - Expected

- Throughput: 50K+ records/sec (with batching)
- Latency: <10ms p99 (gRPC + batching)
- Use case: Production workloads

## API Documentation

See the [module documentation](src/lib.rs) for detailed API docs.

## Next Steps

After completing Phase 5.1 (Producer API):
1. **Phase 5.2**: Add gRPC communication with agents
2. **Phase 5.3**: Implement Consumer API
3. **Phase 5.4**: Consumer groups and offset management

## License

See the root LICENSE file for details.
