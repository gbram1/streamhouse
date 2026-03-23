# StreamHouse

S3-native event streaming. One binary replaces Kafka.

StreamHouse stores everything in S3 — no broker fleets, no disk replication, no JVM. Kafka protocol compatible. 1 TB of retention costs ~$23/month.

## Get Started

### Cloud (fastest)

Sign up at [streamhouse.app](https://streamhouse.app), create an org, grab your API key.

### CLI

```bash
brew install gbram1/tap/stm
stm auth login
```

```bash
# Create a topic
stm topic create events --partitions 3

# Produce
stm produce events --key user-1 --value '{"event":"signup","user":"alice"}'

# Consume
stm consume events --partition 0

# SQL
stm sql query "SELECT * FROM events LIMIT 10"

# Metrics
stm metrics overview
```

### Python SDK

```bash
pip install streamhouse
# On macOS, use a venv if pip is blocked:
# python3 -m venv .venv && source .venv/bin/activate && pip install streamhouse
```

```python
from streamhouse import StreamHouse

sh = StreamHouse(api_key="sk_live_...")

# Create a topic
sh.admin.create_topic("events", partitions=3)

# Produce
producer = sh.producer()
producer.send("events", key="user-1", value='{"event":"signup"}')

# Consume
consumer = sh.consumer()
records = consumer.poll("events", partition=0, offset=0)
for record in records:
    print(record.value_str)
```

### TypeScript SDK

```bash
npm install streamhouse
```

```typescript
import { StreamHouse } from "streamhouse";

const sh = new StreamHouse({ apiKey: "sk_live_..." });

// Create a topic
await sh.admin.createTopic("events", { partitions: 3 });

// Produce
const producer = sh.producer();
await producer.send("events", { event: "signup", user: "alice" }, { key: "user-1" });

// Consume
const consumer = sh.consumer();
for await (const record of consumer.subscribe("events")) {
  console.log(record.value);
}
```

### SQL Queries

Query your streams with SQL — no external engine needed.

```bash
stm sql query "SELECT value->>'event' as event, COUNT(*) as cnt FROM events GROUP BY 1"
```

```python
result = sh.admin.get_metrics()  # or use the SQL endpoint
```

```typescript
const result = await sh.query('SELECT * FROM events WHERE value->>\'event\' = \'signup\' LIMIT 10');
console.log(result.rows);
```

### Pipelines & Sink Connectors

Stream data from topics to external systems with optional SQL transforms.

```bash
# Create a Postgres sink target
stm pipeline target create my-postgres \
  --target-type postgres \
  --url "postgres://user:pass@host:5432/mydb" \
  --table events_sink

# Create a pipeline with a SQL transform
stm pipeline create signup-pipeline \
  --source-topic events \
  --target my-postgres \
  --transform "SELECT value->>'user' as user_id, value->>'event' as event_type, timestamp FROM events WHERE value->>'event' = 'signup'"

# Start it
stm pipeline start signup-pipeline

# Check status
stm pipeline list
```

Supported sinks: **PostgreSQL**, **S3** (Parquet/JSON/CSV), **Elasticsearch**.

### Kafka Protocol

Works with any Kafka client — kcat, kafka-python, confluent-kafka, etc.

```bash
# kcat
echo '{"event":"signup"}' | kcat -P -b localhost:9092 -t events -k user-1
kcat -C -b localhost:9092 -t events -o beginning
```

### Self-Hosted

```bash
docker compose up -d
```

| Service | URL |
|---------|-----|
| REST API | http://localhost:8080 |
| gRPC | localhost:50051 |
| Kafka | localhost:9092 |
| Swagger UI | http://localhost:8080/swagger-ui/ |
| Grafana | http://localhost:3001 |

## Why StreamHouse?

**Cost.** Kafka replicates data 3x on broker disks. StreamHouse stores data in S3 — retention is nearly free.

**Simplicity.** No JVM, no ZooKeeper, no KRaft, no broker fleet. One Rust binary. SQLite for dev, PostgreSQL for prod.

**Flexibility.** Choose durability per-write:

| Mode | Latency | Best for |
|------|---------|----------|
| `acks=buffered` | ~1ms | High-throughput ingestion |
| `acks=durable` | ~150ms | Production workloads needing S3 durability |

## Features

- **Kafka protocol** — 23 APIs, consumer groups, transactions, SASL auth
- **REST + gRPC APIs** — OpenAPI docs at `/swagger-ui/`
- **SQL engine** — Query streams with SQL, window aggregations, JSON operators
- **Schema Registry** — JSON Schema, Avro, Protobuf with compatibility checking
- **Pipelines** — Stream processing with SQL transforms, sink to Postgres/S3/Elasticsearch
- **Multi-tenancy** — Org-scoped isolation, API keys with permissions and topic scopes
- **Log compaction** — Tombstone handling, background compaction
- **Observability** — Prometheus metrics, Grafana dashboards, real-time WebSocket metrics
- **Disaster recovery** — S3 metadata snapshots, self-healing reconciliation

## Performance

| Metric | Throughput |
|--------|-----------|
| WAL writes | 2.21M records/sec |
| Full path (WAL → S3) | 769K records/sec |
| gRPC ProduceBatch | 100K+ messages/sec |
| Segment read (LZ4) | 3.10M records/sec |

## Docs

| | |
|---|---|
| [Getting Started](docs/getting-started.md) | Installation, first topic, Docker |
| [Architecture](docs/overview.md) | Segments, WAL, leases, rebalancing |
| [API Reference](docs/api-reference.md) | REST, gRPC, Kafka endpoints |
| [Configuration](docs/configuration.md) | Env vars, tuning, durability |
| [Authentication](docs/authentication.md) | API keys, SASL, multi-tenancy |

## Compared to Alternatives

| | StreamHouse | Kafka | WarpStream |
|---|---|---|---|
| Storage | S3 | Broker disks (3x) | S3 |
| Runtime | Single Rust binary | JVM + ZK/KRaft | Go agents |
| Retention (1 TB) | ~$23/mo | ~$3K+/mo | ~$23/mo |
| Built-in SQL | Yes | No | No |
| Schema Registry | Built-in | Separate | No |

## Contributing

Issues and PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[Apache License, Version 2.0](LICENSE)
