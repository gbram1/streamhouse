# StreamHouse

**A high-performance, S3-native streaming platform**

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](.)
[![Tests](https://img.shields.io/badge/tests-223%20passing-brightgreen)](.)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

StreamHouse is an Apache Kafka alternative that stores events directly in S3, providing infinite scalability, 99.999999999% durability, and 10x cost reduction. Includes complete Producer and Consumer APIs with Kafka-compatible interfaces.

## Features

- **High Performance**: 62K writes/sec, 30K+ reads/sec, < 10ms P99 metadata queries
- **Cost Effective**: $0.023/GB/month (10x cheaper than Kafka)
- **Infinite Scale**: Stateless agents, S3 storage, horizontal scaling
- **Reliable**: 99.999999999% S3 durability, ACID metadata
- **Kafka-Compatible API**: Complete Producer and Consumer with batching, compression, consumer groups

## Quick Start (Docker Compose)

Get StreamHouse running in under 5 minutes:

```bash
# Clone and start
git clone https://github.com/gbram1/streamhouse
cd streamhouse
docker compose up -d

# Verify it's running
curl http://localhost:8080/health

# Create a topic
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 3}'

# Send a message
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic": "events", "key": "user-1", "value": "{\"event\": \"signup\"}"}'

# Open the Web UI
open http://localhost:3000
```

**Services available after startup:**

| Service | URL |
|---------|-----|
| REST API | http://localhost:8080 |
| Web UI | http://localhost:3000 |
| Grafana | http://localhost:3001 (admin/admin) |
| MinIO Console | http://localhost:9001 (minioadmin/minioadmin) |

For detailed instructions, see **[docs/QUICKSTART.md](docs/QUICKSTART.md)**.

---

## Storage Configuration

StreamHouse stores segment data using an S3-compatible object store. The storage backend is configured entirely via environment variables — no code changes needed.

| Mode | Use Case | Key Env Vars |
|------|----------|--------------|
| Local filesystem | Development & testing | `USE_LOCAL_STORAGE=1` |
| MinIO | Docker-based dev | `S3_ENDPOINT=http://localhost:9000` + AWS creds |
| AWS S3 | Production | `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` + `AWS_REGION` |

### Local Development

```bash
USE_LOCAL_STORAGE=1 cargo run --release --bin unified-server
# Segments written to ./data/storage/ (override with LOCAL_STORAGE_PATH)
```

### MinIO (Docker)

```bash
S3_ENDPOINT=http://localhost:9000 \
AWS_ACCESS_KEY_ID=minioadmin \
AWS_SECRET_ACCESS_KEY=minioadmin \
STREAMHOUSE_BUCKET=streamhouse \
cargo run --release --bin unified-server
```

### Production (AWS S3)

```bash
AWS_ACCESS_KEY_ID=AKIA...
AWS_SECRET_ACCESS_KEY=your-secret-key
AWS_REGION=us-east-1
STREAMHOUSE_BUCKET=your-bucket-name   # defaults to "streamhouse"
cargo run --release --bin unified-server
```

The S3 bucket must already exist in your AWS account. No `S3_ENDPOINT` is needed — the server connects to real AWS S3 by default when `USE_LOCAL_STORAGE` is not set.

---

## Rust Client Examples

### Producer API Example

```rust
use streamhouse_client::Producer;
use std::sync::Arc;

// Create producer
let producer = Producer::builder()
    .metadata_store(metadata_store)
    .agent_group("default")
    .batch_size(1000)
    .compression_enabled(true)
    .build()
    .await?;

// Send records (non-blocking)
let mut result = producer.send("orders", Some(b"user123"), b"order data", None).await?;

// Optional: wait for committed offset
let offset = result.wait_offset().await?;
println!("Record written at offset: {}", offset);

// Flush and close
producer.flush().await?;
producer.close().await?;
```

### Consumer API Example

```rust
use streamhouse_client::{Consumer, OffsetReset};
use std::time::Duration;

// Create consumer
let mut consumer = Consumer::builder()
    .group_id("analytics")
    .topics(vec!["orders".to_string()])
    .metadata_store(metadata_store)
    .object_store(object_store)
    .offset_reset(OffsetReset::Earliest)
    .auto_commit(true)
    .build()
    .await?;

// Poll for records
loop {
    let records = consumer.poll(Duration::from_secs(1)).await?;
    for record in records {
        println!("Offset: {}, Value: {:?}", record.offset, record.value);
    }
    consumer.commit().await?;
}
```

### Local Development with CLI

```bash
# 1. Start the server (no Docker required — uses SQLite + local filesystem)
mkdir -p ./data/storage
USE_LOCAL_STORAGE=1 cargo run --release --bin unified-server &

# 2. Use the CLI
STREAMHOUSE_ADDR=http://localhost:50051 cargo run --bin streamctl -- topic create orders --partitions 10
STREAMHOUSE_ADDR=http://localhost:50051 cargo run --bin streamctl -- produce orders --value '{"event": "signup"}'
STREAMHOUSE_ADDR=http://localhost:50051 cargo run --bin streamctl -- consume orders --partition 0 --limit 10

# Or use the REST API
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic": "orders", "key": "user-1", "value": "{\"event\": \"signup\"}"}'
```

## Architecture

```
Producer API (batching, compression)
    ↓
StreamHouse Server (gRPC, REST, Kafka protocol)
    ↓
S3 Storage (segments) + PostgreSQL/SQLite Metadata (topics, offsets, consumer groups)
    ↓
Consumer API (multi-partition, offset management)
```

## Performance

| Benchmark | Result |
|-----------|--------|
| Producer throughput | 62,325 records/sec |
| Consumer throughput | 30,819 records/sec |
| Sequential reads | 3.10M records/sec |
| Producer latency (async) | < 1ms |
| Metadata queries (cached) | < 100µs |
| LZ4 compression | 30-50% size reduction |
| Segment cache hit rate | 80% |
| S3 durability | 99.999999999% |

```bash
# Run benchmarks yourself
cargo test --release -p streamhouse-client test_producer_throughput -- --nocapture
cargo test --release -p streamhouse-client test_consumer_throughput -- --nocapture
```

## Documentation

- **[Architecture Overview](docs/ARCHITECTURE_OVERVIEW.md)** — System design
- **[Production Guide](docs/PRODUCTION_GUIDE.md)** — Deployment and operations
- **[PostgreSQL Backend](docs/POSTGRES_BACKEND.md)** — Production metadata store
- **[CLI Quickstart](docs/CLI-QUICKSTART.md)** — Command-line usage

## Testing

```bash
# Run all tests
cargo test --workspace

# Run with PostgreSQL tests (requires running PostgreSQL)
DATABASE_URL=postgres://... cargo test --features postgres --workspace

# Run web UI tests
cd web && npm test
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for build instructions, code style, and how to submit changes.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
