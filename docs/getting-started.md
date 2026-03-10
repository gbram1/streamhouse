# Getting Started

StreamHouse is an S3-native event streaming platform. Think Kafka, but your data lives in S3 — infinite retention, fraction of the cost, zero broker storage to manage.

You can connect via **REST**, **Kafka protocol**, or **gRPC**. Same data, same topics, pick your protocol.

---

## 30-Second Start (Local)

```bash
git clone https://github.com/gbram1/streamhouse
cd streamhouse
./quickstart.sh
```

This builds the server, starts it locally (no cloud services), creates a demo topic, produces messages, and consumes them back. Ctrl+C to stop.

---

## Manual Setup

### Build

```bash
cargo build --release -p streamhouse-server --bin unified-server
```

### Run

```bash
# Local mode — SQLite metadata, local filesystem storage
USE_LOCAL_STORAGE=1 ./target/release/unified-server
```

Three protocols start automatically:

| Protocol | Address | Use case |
|----------|---------|----------|
| REST | `http://localhost:8080` | Quick scripts, Web UI, universal |
| Kafka | `localhost:9092` | Production pipelines, existing Kafka clients |
| gRPC | `localhost:50051` | High-throughput programmatic access |

---

## Your First Topic

### Create

```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partition_count": 4}'
```

### Produce

```bash
# Single message
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "key": "user-1",
    "value": "{\"action\": \"signup\", \"user\": \"alice\"}"
  }'

# Batch (faster for multiple messages)
curl -X POST http://localhost:8080/api/v1/produce/batch \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "records": [
      {"key": "user-1", "value": "{\"action\": \"login\"}"},
      {"key": "user-2", "value": "{\"action\": \"purchase\", \"amount\": 49.99}"}
    ]
  }'
```

### Consume

```bash
curl "http://localhost:8080/api/v1/consume?topic=events&partition=0&offset=0&maxRecords=10"
```

### Query with SQL

```bash
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM events LIMIT 10"}'
```

SQL supports `SELECT`, `WHERE`, `GROUP BY`, `ORDER BY`, `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, JSON operators (`->`, `->>`), and window functions (`TUMBLE`, `HOP`, `SESSION`) for time-based aggregations.

```bash
# Window aggregation — count events per 5-minute window
curl -X POST http://localhost:8080/api/v1/sql \
  -d '{"query": "SELECT TUMBLE(timestamp, '\''5 minutes'\'') as window, COUNT(*) FROM events GROUP BY window"}'
```

---

## Using gRPC

gRPC is the highest-throughput protocol, supporting batched produce and configurable ack modes.

```python
import grpc
from streamhouse_pb2 import ProduceBatchRequest, ProduceRecord, ACK_DURABLE
from streamhouse_pb2_grpc import StreamHouseStub

channel = grpc.insecure_channel('localhost:50051')
client = StreamHouseStub(channel)

request = ProduceBatchRequest(
    topic='events',
    records=[ProduceRecord(key='user-1', value=b'{"action":"signup"}')],
    ack_mode=ACK_DURABLE,  # Wait for S3 flush before ack
)
response = client.ProduceBatch(request)
```

### Ack Modes

| Mode | Latency | Durability |
|------|---------|-----------|
| `ACK_NONE` | Fire-and-forget | No guarantee |
| `ACK_BUFFERED` (default) | ~1ms | Survives process crash (WAL) |
| `ACK_DURABLE` | ~150ms | Survives disk loss (flushed to S3) |

Choose the mode that matches your workload. Most use cases work well with `ACK_BUFFERED` + WAL enabled.

---

## Using Kafka Clients

StreamHouse speaks the Kafka wire protocol. Any Kafka client works — no code changes needed.

### Python (confluent-kafka)

```python
from confluent_kafka import Producer, Consumer

producer = Producer({'bootstrap.servers': 'localhost:9092'})
producer.produce('events', key='user-1', value=b'{"action": "signup"}')
producer.flush()

consumer = Consumer({
    'bootstrap.servers': 'localhost:9092',
    'group.id': 'my-group',
    'auto.offset.reset': 'earliest',
})
consumer.subscribe(['events'])

while True:
    msg = consumer.poll(1.0)
    if msg and not msg.error():
        print(f"{msg.key()}: {msg.value()}")
```

### With Authentication (SASL/PLAIN)

When auth is enabled (`STREAMHOUSE_AUTH_ENABLED=true`), use your API key as both username and password:

```python
producer = Producer({
    'bootstrap.servers': 'localhost:9092',
    'security.protocol': 'SASL_PLAINTEXT',
    'sasl.mechanism': 'PLAIN',
    'sasl.username': 'sk_live_abc123...',
    'sasl.password': 'sk_live_abc123...',
})
```

For REST, use a Bearer token:

```bash
curl -H "Authorization: Bearer sk_live_abc123..." \
  http://localhost:8080/api/v1/topics
```

See [Authentication](authentication.md) for full details.

### kcat

```bash
# Produce
echo '{"action":"signup"}' | kcat -P -b localhost:9092 -t events -k user-1

# Consume
kcat -C -b localhost:9092 -t events -o beginning

# With auth
kcat -C -b localhost:9092 -t events -o beginning \
  -X security.protocol=SASL_PLAINTEXT \
  -X sasl.mechanism=PLAIN \
  -X sasl.username=sk_live_abc123... \
  -X sasl.password=sk_live_abc123...
```

---

## Docker Compose (Full Stack)

For a production-like setup with Postgres, MinIO (S3), monitoring, and 3 agents:

```bash
docker compose up -d
```

### Services

| Service | URL | Credentials |
|---------|-----|-------------|
| REST API | http://localhost:8080 | — |
| Kafka | localhost:9092 | — |
| gRPC | localhost:50051 | — |
| Schema Registry | http://localhost:8080/schemas | — |
| Grafana | http://localhost:3001 | admin / admin |
| Prometheus | http://localhost:9091 | — |
| MinIO Console | http://localhost:9001 | minioadmin / minioadmin |

### Architecture

```
┌───────────────────────────────────────────────────────────┐
│                     Docker Compose                         │
│                                                            │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────────────┐│
│  │ Postgres │  │  MinIO   │  │   Unified Server          ││
│  │  :5432   │  │  :9000   │  │   :8080 REST              ││
│  │ metadata │  │ segments │  │   :50051 gRPC             ││
│  └──────────┘  └──────────┘  │   :9092 Kafka             ││
│                               │   DISABLE_EMBEDDED_AGENT  ││
│                               └───────────────────────────┘│
│  ┌─────────┐  ┌─────────┐  ┌─────────┐                    │
│  │ agent-1 │  │ agent-2 │  │ agent-3 │  (partition owners) │
│  └─────────┘  └─────────┘  └─────────┘                    │
│                                                            │
│  ┌────────────┐  ┌─────────┐                               │
│  │ Prometheus │  │ Grafana │  (monitoring)                  │
│  │   :9091    │  │  :3001  │                                │
│  └────────────┘  └─────────┘                                │
└───────────────────────────────────────────────────────────┘
```

---

## CLI (`streamctl`)

```bash
cargo build --release -p streamhouse-cli
```

### Commands

```bash
# Topics
streamctl topic create orders --partitions 4
streamctl topic list
streamctl topic get orders
streamctl topic delete orders

# Produce & consume
streamctl produce orders --partition 0 --value '{"event":"signup"}'
streamctl consume orders --partition 0 --offset 0 --limit 10

# SQL
streamctl sql query 'SELECT * FROM orders LIMIT 10'

# Schemas
streamctl schema list
streamctl schema register orders-value /tmp/schema.json --schema-type JSON
streamctl schema get orders-value

# Consumer groups
streamctl consumer list
streamctl consumer lag my-group
streamctl offset commit --group my-group --topic orders --partition 0 --offset 10

# Interactive REPL
streamctl   # (no arguments)
```

### Connection Config

```bash
export STREAMHOUSE_ADDR=http://localhost:50051     # gRPC
export STREAMHOUSE_API_URL=http://localhost:8080    # REST
export SCHEMA_REGISTRY_URL=http://localhost:8080/schemas
```

---

## Schema Registry

Built-in schema validation for JSON Schema, Avro, and Protobuf.

```bash
# Register a JSON Schema
curl -X POST http://localhost:8080/schemas/subjects/events-value/versions \
  -H 'Content-Type: application/json' \
  -d '{
    "schema": "{\"type\":\"object\",\"properties\":{\"action\":{\"type\":\"string\"},\"user\":{\"type\":\"string\"}},\"required\":[\"action\",\"user\"]}",
    "schemaType": "JSON"
  }'

# Now produces are validated — invalid messages are rejected
# Missing required "user" field → 422 error
curl -X POST http://localhost:8080/api/v1/produce \
  -d '{"topic": "events", "value": "{\"action\":\"signup\"}"}'
# → Schema validation failed: missing required property "user"
```

Subject naming convention: `{topic}-value` (e.g., `orders-value`).

Avro and Protobuf schemas are also supported — use `"schemaType": "AVRO"` or `"schemaType": "PROTOBUF"` when registering.

Compatibility modes (BACKWARD, FORWARD, FULL, NONE) can be configured per subject to control schema evolution. See [API Reference](api-reference.md) for details.

---

## What's Next

- **[Authentication](authentication.md)** — API keys, SASL/PLAIN, org-scoped access
- **[Configuration](configuration.md)** — All env vars and tuning options
- **[API Reference](api-reference.md)** — REST, gRPC, and Kafka protocol details
- **[Architecture](overview.md)** — How StreamHouse works under the hood
- **[Testing](testing.md)** — E2E tests, benchmarks, validation

---

## Troubleshooting

**Server won't start** — Check port conflicts (8080, 50051, 9092). Check logs: `docker compose logs streamhouse-server` or `tail -f /tmp/streamhouse-quickstart.log`.

**Messages not appearing** — StreamHouse buffers writes before uploading to S3. Wait 5-10 seconds, or set `SEGMENT_MAX_AGE_MS=1000` for faster flushes. For immediate durability, use gRPC with `ACK_DURABLE`.

**Production setup** — Enable the WAL (`WAL_ENABLED=true`) for crash recovery. See [Configuration](configuration.md) for all production settings.

**Docker services unhealthy** — `docker compose ps` to check status, `docker compose down && docker compose up -d` to restart.
