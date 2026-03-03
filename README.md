# StreamHouse

**S3-native event streaming with Kafka compatibility**

StreamHouse is an Apache Kafka alternative that stores events directly in S3. No broker fleet, no disk management, no replication — just S3's durability and StreamHouse's coordination.

## Quick Start

```bash
# Start StreamHouse (local dev mode)
./quickstart.sh

# Or manually:
cargo build --release
USE_LOCAL_STORAGE=1 ./target/release/unified-server

# In another terminal, use the CLI:
cargo run --bin streamctl

# Inside the REPL:
> topic create events --partitions 4
> produce events --partition 0 --value {"type":"signup","user":"alice"}
> consume events --partition 0 --offset 0 --limit 10
> sql query SELECT * FROM events WHERE value->>'type' = 'signup' LIMIT 5
```

That's it. You're running an event streaming platform.

## What You Get

- **Kafka wire protocol** — Use existing Kafka clients (kafkacat, kafka-python, etc.)
- **gRPC + REST APIs** — Modern APIs with OpenAPI docs at `/swagger-ui/`
- **SQL queries** — Query streams with SQL (`SELECT * FROM orders WHERE value->>'status' = 'paid'`)
- **Schema Registry** — JSON Schema and Avro validation built-in
- **S3-native storage** — Events stored directly in S3 (or local filesystem for dev)
- **Multi-agent coordination** — Automatic partition load balancing across agents
- **No JVM, no Zookeeper** — Single Rust binary, SQLite or PostgreSQL metadata store

## Docker Compose (Full Stack)

```bash
docker compose up -d
```

Starts PostgreSQL, MinIO (S3-compatible), StreamHouse server, 3 agents, Prometheus, and Grafana.

| Service | URL | Credentials |
|---------|-----|-------------|
| REST API | http://localhost:8080 | — |
| gRPC | localhost:50051 | — |
| Kafka protocol | localhost:9092 | — |
| Grafana | http://localhost:3001 | admin / admin |
| MinIO Console | http://localhost:9001 | minioadmin / minioadmin |

## Documentation

- **[Getting Started →](docs/getting-started.md)** — Installation, first topic, CLI usage, schema registry
- **[API Reference →](docs/api-reference.md)** — REST, gRPC, and Kafka protocol endpoints
- **[Configuration →](docs/configuration.md)** — Environment variables and performance tuning
- **[Testing →](docs/testing.md)** — E2E tests, benchmarks, validation scripts
- **[Architecture →](docs/overview.md)** — How StreamHouse works internally

## Examples

### REST API

```bash
# Create topic
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 4}'

# Produce
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic": "events", "key": "user-1", "value": "{\"event\": \"signup\"}"}'

# Consume
curl "http://localhost:8080/api/v1/consume?topic=events&partition=0&offset=0"

# SQL query
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM events LIMIT 10"}'
```

### Kafka Protocol

```bash
# kafkacat / kcat
echo '{"event":"signup"}' | kcat -P -b localhost:9092 -t events
kcat -C -b localhost:9092 -t events -o beginning

# kafka-python
from kafka import KafkaProducer
producer = KafkaProducer(bootstrap_servers='localhost:9092')
producer.send('events', value=b'{"event":"signup"}')
```

### Schema Registry

```bash
# Register schema
./target/release/streamctl schema register events-value /path/to/schema.json --schema-type JSON

# Produce with validation
./target/release/streamctl produce events --partition 0 --value {"event":"signup","user":"alice"}
# Validation enforced automatically
```

## Architecture

```
Clients (REST, gRPC, Kafka)
    ↓
StreamHouse Server
    ↓
    ├── Partition Writers (one per partition)
    │   ├── WAL (optional crash recovery)
    │   ├── Segment Builder (batching, compression)
    │   └── S3 Upload (group-commit)
    │
    ├── Metadata Store (SQLite or PostgreSQL)
    │   Topics, partitions, offsets, leases
    │
    └── Segment Cache (LRU, local disk)
```

See **[Architecture docs →](docs/overview.md)** for full details.

## Testing

```bash
# Unit tests
cargo test --workspace

# Quick validation (local dev)
./quickstart.sh

# Full e2e test (Docker Compose)
./tests/e2e_full.sh

# Benchmarks
./tests/benchmarks/grpc-bench.sh
./tests/benchmarks/rest-bench.sh
```

See **[Testing docs →](docs/testing.md)** for all test suites.

## Performance

From recent benchmarks:
- **WAL throughput**: 2.21M records/sec
- **Full path (WAL → S3)**: 769K records/sec
- **gRPC ProduceBatch**: 100K+ messages/sec sustained

Tune with environment variables — see **[Configuration docs →](docs/configuration.md)**.

## Development Status

StreamHouse is in active development. Core features are stable:
- ✅ Produce/consume (REST, gRPC, Kafka protocol)
- ✅ Multi-agent coordination
- ✅ Schema registry (JSON Schema)
- ✅ SQL queries
- ✅ Consumer groups

Planned:
- 🚧 Authentication (SASL, API keys)
- 🚧 Avro schema support (partial)
- 🚧 Transactions (in progress)
- 🚧 Compacted topics

## Contributing

Issues and PRs welcome at https://github.com/gbram1/streamhouse

## License

[Apache License, Version 2.0](LICENSE)
