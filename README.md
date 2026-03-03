# StreamHouse

**S3-native event streaming with Kafka compatibility**

StreamHouse is an Apache Kafka alternative that stores events directly in S3. No broker fleet, no disk management, no replication — just S3's durability and StreamHouse's coordination.

## Quick Start

```console
$ ./quickstart.sh
Building StreamHouse...
   Compiling streamhouse v0.1.0
    Finished release [optimized] in 45s

Starting server (local storage, no cloud services needed)
Server ready  REST=http://localhost:8080  gRPC=localhost:50051  Kafka=localhost:9092

Create topic  POST /api/v1/topics
  ✓ Created 'demo' with 4 partitions

Produce messages  POST /api/v1/produce
  ✓ partition=0 offset=0  user-0
  ✓ partition=1 offset=0  user-1
  ✓ partition=2 offset=0  user-2

Consume messages  GET /api/v1/consume
  Partition 0: 2 records
    offset=0  key=user-0  {"event":"signup","user":"alice"}
    offset=1  key=user-0  {"event":"purchase","user":"alice","item":"gadget"}
```

Or use the interactive CLI:

```console
$ cargo run --bin streamctl
    Finished dev [unoptimized + debuginfo] in 0.5s
     Running `target/debug/streamctl`

StreamHouse CLI v0.1.0
Connected to http://localhost:50051

> topic create events --partitions 4
✓ Created topic 'events' with 4 partitions

> produce events --partition 0 --value {"type":"signup","user":"alice"}
✓ Produced to partition=0 offset=0

> consume events --partition 0 --offset 0 --limit 10
offset=0  value={"type":"signup","user":"alice"}  timestamp=2024-03-03T12:34:56Z

> sql query SELECT * FROM events WHERE value->>'type' = 'signup' LIMIT 5
┌────────┬──────────────────────────────────┬─────────────────────────┐
│ offset │ value                            │ timestamp               │
├────────┼──────────────────────────────────┼─────────────────────────┤
│ 0      │ {"type":"signup","user":"alice"} │ 2024-03-03T12:34:56Z    │
└────────┴──────────────────────────────────┴─────────────────────────┘
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

## Contributing

Issues and PRs welcome at https://github.com/gbram1/streamhouse

## License

[Apache License, Version 2.0](LICENSE)
