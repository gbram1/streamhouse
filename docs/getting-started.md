# Getting Started

This guide walks you through installing, running, and using StreamHouse for the first time.

## Prerequisites

- **Rust toolchain** (`cargo` 1.70+) — [Install Rust](https://rustup.rs/)
- **Python 3** — For quickstart script
- **Docker** (optional) — For full stack setup with PostgreSQL, MinIO, Grafana

## Quick Start (2 minutes)

The fastest way to try StreamHouse:

```bash
git clone https://github.com/gbram1/streamhouse
cd streamhouse
./quickstart.sh
```

This single script:
1. Builds the server
2. Starts it with local storage (no cloud services)
3. Creates a `demo` topic
4. Produces sample messages
5. Consumes them back

Press **Ctrl+C** to stop the server when you're done.

**What you'll see:**

```bash
Starting server (local storage, no cloud services needed)
Server ready  REST=http://localhost:8080  gRPC=localhost:50051  Kafka=localhost:9092

Create topic  POST /api/v1/topics
  Created 'demo' with 4 partitions

Produce messages  POST /api/v1/produce
  + partition=0 offset=0  user-0
  + partition=1 offset=0  user-1
  + partition=2 offset=0  user-2
  ...

Consume messages  GET /api/v1/consume
  Partition 0: 2 records
    offset=0  key=user-0  {"event":"signup","user":"alice","plan":"pro"}
    offset=1  key=user-0  {"event":"purchase","user":"alice","item":"gadget","amount":49.99}
  ...
```

## Local Development

### Manual Setup

If you prefer to run each step yourself:

```bash
# Build the server
cargo build --release -p streamhouse-server --bin unified-server

# Start the server (SQLite + local filesystem — no cloud services needed)
USE_LOCAL_STORAGE=1 ./target/release/unified-server
```

The server starts three listeners:
- **REST API** — `http://localhost:8080`
- **gRPC** — `localhost:50051`
- **Kafka protocol** — `localhost:9092`

### Create Your First Topic

Open a new terminal and create a topic:

```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 4}'
```

**Response:**
```json
{
  "name": "events",
  "partitions": 4,
  "replication_factor": 1
}
```

### Produce a Message

```bash
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "key": "user-1",
    "value": "{\"event\": \"signup\", \"user\": \"alice\"}"
  }'
```

**Response:**
```json
{
  "topic": "events",
  "partition": 2,
  "offset": 0
}
```

### Consume Messages

```bash
curl "http://localhost:8080/api/v1/consume?topic=events&partition=2&offset=0"
```

**Response:**
```json
{
  "topic": "events",
  "partition": 2,
  "records": [
    {
      "offset": 0,
      "key": "user-1",
      "value": "{\"event\": \"signup\", \"user\": \"alice\"}",
      "timestamp": 1678886400000
    }
  ]
}
```

## CLI Tool (`streamctl`)

The `streamctl` CLI provides an interactive REPL and direct commands for all StreamHouse operations.

### Build

```bash
cargo build --release -p streamhouse-cli
```

### Interactive REPL

```bash
./target/release/streamctl
```

**Inside the REPL:**

```
> topic create orders --partitions 4
Created topic 'orders' with 4 partitions

> produce orders --partition 0 --value {"event":"signup","user":"alice"}
Produced to partition=0 offset=0

> consume orders --partition 0 --offset 0 --limit 10
offset=0 key= value={"event":"signup","user":"alice"} timestamp=1678886400000

> sql query SELECT * FROM orders LIMIT 5
[
  {"offset": 0, "key": "", "value": "{\"event\":\"signup\",\"user\":\"alice\"}", "timestamp": 1678886400000}
]

> topic list
- orders (4 partitions)

> help
Available commands:
  topic create <name> --partitions <N>
  topic list
  topic get <name>
  topic delete <name>
  produce <topic> --partition <N> --value <json>
  consume <topic> --partition <N> --offset <N> --limit <N>
  sql query <query>
  schema list
  schema register <subject> <path> --schema-type JSON|AVRO
  schema get <subject>
  consumer list
  offset commit --group <group> --topic <topic> --partition <N> --offset <N>
  help
  exit

> exit
```

**REPL tip:** The REPL uses whitespace splitting, so avoid spaces inside JSON values. Use `{"key":"value"}` not `{"key": "value"}`.

### Direct Commands

You can also run commands directly without entering the REPL:

```bash
# Create topic
./target/release/streamctl topic create orders --partitions 4

# Produce
./target/release/streamctl produce orders --partition 0 --value '{"event":"signup"}'

# Consume
./target/release/streamctl consume orders --partition 0 --offset 0 --limit 10

# SQL query
./target/release/streamctl sql query 'SELECT * FROM orders LIMIT 10'

# List topics
./target/release/streamctl topic list
```

### Connection Configuration

By default, `streamctl` connects to:
- **gRPC:** `http://localhost:9090`
- **REST:** `http://localhost:8080`
- **Schema Registry:** `http://localhost:8081`

For local development (server running directly), set:

```bash
export STREAMHOUSE_ADDR=http://localhost:50051
export STREAMHOUSE_API_URL=http://localhost:8080
export SCHEMA_REGISTRY_URL=http://localhost:8080/schemas
```

For Docker Compose (see below), the defaults work as-is.

## Docker Compose (Full Stack)

For a production-like environment with PostgreSQL, MinIO (S3-compatible), monitoring, and multi-agent setup:

```bash
docker compose up -d
```

This starts:
- **PostgreSQL** — Metadata store
- **MinIO** — S3-compatible object storage
- **StreamHouse server** — Main API server
- **3x standalone agents** — Partition ownership and coordination
- **Prometheus** — Metrics collection
- **Grafana** — Dashboard (pre-configured)

### Service URLs

| Service | URL | Credentials |
|---------|-----|-------------|
| REST API | http://localhost:8080 | — |
| gRPC | localhost:50051 | — |
| Kafka protocol | localhost:9092 | — |
| Schema Registry | http://localhost:8080/schemas | — |
| Grafana | http://localhost:3001 | admin / admin |
| Prometheus | http://localhost:9091 | — |
| MinIO Console | http://localhost:9001 | minioadmin / minioadmin |

### Using streamctl with Docker Compose

When running via `docker compose`, ports are different. Set these before using the CLI:

```bash
export STREAMHOUSE_ADDR=http://localhost:50051
export STREAMHOUSE_API_URL=http://localhost:8080
export SCHEMA_REGISTRY_URL=http://localhost:8080/schemas
```

Then use `streamctl` as normal:

```bash
./target/release/streamctl topic list
./target/release/streamctl topic create orders --partitions 4
./target/release/streamctl produce orders --partition 0 --value '{"event":"signup"}'
```

### Monitoring

Open Grafana at http://localhost:3001 (admin / admin) to see:
- Produce/consume throughput
- Agent partition ownership
- Segment upload latency
- Storage utilization

### Stopping Services

```bash
docker compose down

# Clean up data volumes
docker compose down -v
```

## Schema Registry

StreamHouse includes a built-in schema registry for JSON Schema and Avro validation.

### Subject Naming Convention

Schema subjects follow the pattern: `{topic}-value` (e.g., `orders-value`).

### Register a JSON Schema

Create a schema file:

```bash
cat > /tmp/orders-schema.json << 'EOF'
{
  "type": "object",
  "properties": {
    "event": { "type": "string" },
    "user": { "type": "string" },
    "timestamp": { "type": "number" }
  },
  "required": ["event", "user"]
}
EOF
```

Register it:

```bash
# Via streamctl
./target/release/streamctl schema register orders-value /tmp/orders-schema.json --schema-type JSON

# Via curl
curl -X POST http://localhost:8080/schemas/subjects/orders-value/versions \
  -H 'Content-Type: application/json' \
  -d '{
    "schema": "{\"type\":\"object\",\"properties\":{\"event\":{\"type\":\"string\"},\"user\":{\"type\":\"string\"}},\"required\":[\"event\",\"user\"]}",
    "schemaType": "JSON"
  }'
```

### Produce with Schema Validation

Once a schema is registered, StreamHouse validates all messages against it:

```bash
# Valid message (accepted)
./target/release/streamctl produce orders --partition 0 --value '{"event":"signup","user":"alice"}'

# Invalid message (rejected — missing required field "user")
./target/release/streamctl produce orders --partition 0 --value '{"event":"signup"}'
# Error: Schema validation failed: missing required property "user"
```

### Query Schemas

```bash
# List all subjects
./target/release/streamctl schema list

# Get latest version
./target/release/streamctl schema get orders-value

# Via curl
curl http://localhost:8080/schemas/subjects
curl http://localhost:8080/schemas/subjects/orders-value/versions/latest
```

### Delete Schema

```bash
./target/release/streamctl schema delete orders-value
```

## Working with Kafka Clients

StreamHouse implements the Kafka wire protocol, so you can use standard Kafka clients.

### kafka-python

```python
from kafka import KafkaProducer, KafkaConsumer

# Produce
producer = KafkaProducer(bootstrap_servers='localhost:9092')
producer.send('events', key=b'user-1', value=b'{"event":"signup"}')
producer.flush()

# Consume
consumer = KafkaConsumer(
    'events',
    bootstrap_servers='localhost:9092',
    auto_offset_reset='earliest',
    group_id='my-group'
)

for message in consumer:
    print(f"offset={message.offset} key={message.key} value={message.value}")
```

### kafkacat / kcat

```bash
# Produce
echo '{"event":"signup","user":"alice"}' | kcat -P -b localhost:9092 -t events -k user-1

# Consume
kcat -C -b localhost:9092 -t events -o beginning
```

### Kafka CLI Tools

```bash
# Produce
kafka-console-producer.sh --broker-list localhost:9092 --topic events

# Consume
kafka-console-consumer.sh --bootstrap-server localhost:9092 --topic events --from-beginning
```

## SQL Queries

StreamHouse supports SQL queries over streams:

```bash
# Via streamctl
./target/release/streamctl sql query "SELECT * FROM events WHERE value->>'event' = 'signup' LIMIT 5"

# Via curl
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM events WHERE value->>\"event\" = \"signup\" LIMIT 5"}'
```

**Supported SQL features:**
- `SELECT`, `WHERE`, `LIMIT`, `OFFSET`
- JSON operators: `->`, `->>`
- Aggregations: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`
- `GROUP BY`, `ORDER BY`

## Consumer Groups

Consumer groups track offsets for coordinated consumption:

```bash
# Commit offset
./target/release/streamctl offset commit --group my-group --topic events --partition 0 --offset 10

# Get committed offset
./target/release/streamctl offset get --group my-group --topic events --partition 0

# List consumer groups
./target/release/streamctl consumer list

# Get consumer group details
./target/release/streamctl consumer get my-group

# Check consumer lag
./target/release/streamctl consumer lag my-group
```

## Next Steps

Now that you have StreamHouse running:

- **[API Reference →](api-reference.md)** — REST, gRPC, and Kafka protocol details
- **[Configuration →](configuration.md)** — All environment variables and tuning options
- **[Testing →](testing.md)** — E2E tests, benchmarks, validation scripts
- **[Architecture →](overview.md)** — How StreamHouse works internally

## Troubleshooting

### Server won't start

Check logs:

```bash
# Local mode
tail -f /tmp/streamhouse-quickstart.log

# Docker Compose
docker compose logs streamhouse-server
```

Common issues:
- Port already in use (8080, 50051, or 9092) — kill existing process or change ports
- Missing `data/` directory — create with `mkdir -p data/storage data/cache data/wal`

### Messages not appearing when consuming

StreamHouse buffers writes by default. For testing with small payloads:
- Wait 5-10 seconds after producing
- Or set `SEGMENT_MAX_AGE_MS=1000` to flush more frequently
- Or use `ACK_DURABLE` mode in gRPC for immediate S3 flush

### Schema validation not working

Ensure:
1. Schema is registered: `./target/release/streamctl schema get <subject>`
2. Subject name follows convention: `{topic}-value`
3. Schema is valid JSON Schema or Avro format

### Docker Compose services unhealthy

```bash
# Check all services
docker compose ps

# Check specific service
docker compose logs postgres
docker compose logs minio
docker compose logs agent-1

# Restart everything
docker compose down
docker compose up -d
```
