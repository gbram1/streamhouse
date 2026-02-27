# StreamHouse

**S3-native event streaming. One binary, zero ops.**

StreamHouse is an Apache Kafka alternative that stores events directly in S3. No broker fleet, no disk management, no replication — just S3's 99.999999999% durability out of the box.

## Quick Start

**Prerequisites:** Rust toolchain (`cargo`), Python 3

```bash
git clone https://github.com/gbram1/streamhouse
cd streamhouse
./quickstart.sh
```

This single script builds the server, starts it with local storage, creates a `demo` topic, produces sample messages, and consumes them. Press **Ctrl+C** to stop the server when you're done.

### Manual setup

If you prefer to run each step yourself:

```bash
# Build
cargo build --release -p streamhouse-server --bin unified-server

# Run (SQLite + local filesystem — no cloud services needed)
USE_LOCAL_STORAGE=1 ./target/release/unified-server
```

The server starts three listeners:
- **REST API** — `http://localhost:8080`
- **gRPC** — `localhost:50051`
- **Kafka protocol** — `localhost:9092`

### Create a topic and produce

```bash
# Create a topic
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 4}'

# Produce a message
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic": "events", "key": "user-1", "value": "{\"event\": \"signup\"}"}'

# Consume
curl "http://localhost:8080/api/v1/consume?topic=events&partition=0&offset=0"
```

### Docker Compose (full stack)

```bash
docker compose up -d
```

This starts PostgreSQL, MinIO (S3-compatible), the StreamHouse server, Prometheus, and Grafana.

| Service | URL | Credentials |
|---------|-----|-------------|
| REST API | http://localhost:8080 | — |
| gRPC | localhost:50051 | — |
| Kafka protocol | localhost:9092 | — |
| Schema Registry | http://localhost:8080/schemas | — |
| Grafana | http://localhost:3001 | admin / admin |
| Prometheus | http://localhost:9091 | — |
| MinIO Console | http://localhost:9001 | minioadmin / minioadmin |

---

## CLI (`streamctl`)

```bash
cargo build --release -p streamhouse-cli
```

### Local development (server running directly)

```bash
# Interactive REPL
./target/release/streamctl

# Or direct commands
./target/release/streamctl topic create orders --partitions 4
./target/release/streamctl produce orders --partition 0 --value '{"event":"signup","user":"alice"}'
./target/release/streamctl consume orders --partition 0 --offset 0 --limit 10
```

Default connections: gRPC `http://localhost:9090`, REST `http://localhost:8080`, Schema Registry `http://localhost:8081`.

### Docker Compose

When running via `docker compose up -d`, the ports are different. Set these before using the CLI:

```bash
export STREAMHOUSE_ADDR=http://localhost:50051
export STREAMHOUSE_API_URL=http://localhost:8080
export SCHEMA_REGISTRY_URL=http://localhost:8080/schemas
```

Then use the CLI normally:

```bash
# Direct commands
./target/release/streamctl topic list
./target/release/streamctl topic create orders --partitions 4
./target/release/streamctl produce orders --partition 0 --value '{"event":"signup","user":"alice"}'
./target/release/streamctl consume orders --partition 0 --offset 0 --limit 10
./target/release/streamctl sql query 'SELECT * FROM orders LIMIT 10'

# Interactive REPL
./target/release/streamctl
```

### REPL commands

Launch the REPL with `./target/release/streamctl` (no arguments). Commands inside the REPL drop the `streamctl` prefix:

```
topic list
topic create orders --partitions 4
topic get orders
topic delete orders

produce orders --partition 0 --value {"event":"signup","user":"alice"}
produce orders --partition 0 --key user-1 --value {"event":"click"}
consume orders --partition 0 --offset 0 --limit 10

sql query SELECT * FROM orders LIMIT 10

schema list
schema register orders-value /path/to/schema.json --schema-type JSON
schema get orders-value
schema delete orders-value

consumer list
consumer get my-group
consumer lag my-group

offset commit --group my-group --topic orders --partition 0 --offset 5
offset get --group my-group --topic orders --partition 0

help
exit
```

**Note:** The REPL uses whitespace splitting, so avoid spaces inside JSON values. Use `{"key":"value"}` not `{"key": "value"}`.

### Schema Registry

The schema registry stores and manages schemas for topic data. Subject names follow the convention `{topic}-value` (e.g., `orders-value`).

**Register a JSON schema:**

```bash
# Create a schema file
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

# Register it (CLI)
./target/release/streamctl schema register orders-value /tmp/orders-schema.json --schema-type JSON

# Register it (curl)
curl -X POST http://localhost:8080/schemas/subjects/orders-value/versions \
  -H 'Content-Type: application/json' \
  -d '{"schema": "{\"type\":\"object\",\"properties\":{\"event\":{\"type\":\"string\"},\"user\":{\"type\":\"string\"}},\"required\":[\"event\",\"user\"]}", "schemaType": "JSON"}'
```

**Query schemas:**

```bash
# List all subjects
./target/release/streamctl schema list

# Get latest version for a subject
./target/release/streamctl schema get orders-value

# curl equivalents
curl http://localhost:8080/schemas/subjects
curl http://localhost:8080/schemas/subjects/orders-value/versions/latest
```

---

## REST API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/v1/topics` | GET, POST | List / create topics |
| `/api/v1/topics/{name}` | GET, DELETE | Get / delete topic |
| `/api/v1/produce` | POST | Produce a message |
| `/api/v1/produce/batch` | POST | Produce a batch |
| `/api/v1/consume?topic=X&partition=0&offset=0` | GET | Consume messages |
| `/api/v1/consumer-groups` | GET, POST | Consumer group management |
| `/api/v1/sql` | POST | SQL queries over streams |
| `/schemas/subjects` | GET | List schema subjects |
| `/schemas/subjects/{subject}/versions` | GET, POST | List versions / register schema |
| `/schemas/subjects/{subject}/versions/latest` | GET | Get latest schema |
| `/schemas/subjects/{subject}/versions/{version}` | GET | Get specific version |

---

## gRPC API

The unified gRPC service (`streamhouse.StreamHouse` on port 50051) supports:

- **Topics**: `CreateTopic`, `GetTopic`, `ListTopics`, `DeleteTopic`
- **Produce**: `Produce`, `ProduceBatch` (with `AckMode`: BUFFERED, DURABLE, NONE)
- **Consume**: `Consume`, `CommitOffset`, `GetOffset`
- **Producer Lifecycle**: `InitProducer`, `BeginTransaction`, `CommitTransaction`, `AbortTransaction`, `Heartbeat`

`ProduceBatch` with `ack_mode: ACK_DURABLE` waits for the S3 flush before responding — your data is on S3 when the RPC returns.

---

## Storage Backends

| Backend | Use Case | Config |
|---------|----------|--------|
| Local filesystem | Development | `USE_LOCAL_STORAGE=1` |
| MinIO | Docker dev | `S3_ENDPOINT=http://localhost:9000` + AWS creds |
| AWS S3 | Production | `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` + `AWS_REGION` |

## Metadata Backends

| Backend | Use Case | Config |
|---------|----------|--------|
| SQLite | Development (default) | Automatic — stores at `./data/metadata.db` |
| PostgreSQL | Production | Build with `--features postgres`, set `DATABASE_URL` |

---

## Configuration

All configuration is via environment variables. Values are logged at startup.

### Server

| Env Var | Default | Description |
|---------|---------|-------------|
| `GRPC_ADDR` | `0.0.0.0:50051` | gRPC bind address |
| `HTTP_ADDR` | `0.0.0.0:8080` | REST API bind address |
| `KAFKA_ADDR` | `0.0.0.0:9092` | Kafka protocol bind address |

### Storage

| Env Var | Default | Description |
|---------|---------|-------------|
| `USE_LOCAL_STORAGE` | unset | Set to any value to use local filesystem instead of S3 |
| `LOCAL_STORAGE_PATH` | `./data/storage` | Path for local storage |
| `STREAMHOUSE_BUCKET` | `streamhouse` | S3 bucket name |
| `AWS_REGION` | `us-east-1` | S3 region |
| `S3_ENDPOINT` | _(AWS S3)_ | Custom S3 endpoint (for MinIO) |

### Performance Tuning

**Segment flushing:**

| Env Var | Default | Description |
|---------|---------|-------------|
| `FLUSH_INTERVAL_SECS` | `5` | Background flush interval (ACK_BUFFERED path) |
| `SEGMENT_MAX_SIZE` | `1048576` (1MB) | Segment roll size in bytes |
| `SEGMENT_MAX_AGE_MS` | `10000` | Segment roll time in ms |

**ACK_DURABLE batching** — controls the group-commit window for durable writes:

| Env Var | Default | Description |
|---------|---------|-------------|
| `DURABLE_BATCH_MAX_AGE_MS` | `200` | Batch window in ms. Lower = lower latency, more S3 PUTs. Higher = more batching, better throughput. |
| `DURABLE_BATCH_MAX_RECORDS` | `10000` | Force flush after N records |
| `DURABLE_BATCH_MAX_BYTES` | `16777216` (16MB) | Force flush after N bytes |

**S3 upload:**

| Env Var | Default | Description |
|---------|---------|-------------|
| `S3_UPLOAD_RETRIES` | `3` | Retry count for S3 uploads |
| `MULTIPART_THRESHOLD` | `8388608` (8MB) | Switch to multipart upload above this size |
| `MULTIPART_PART_SIZE` | `8388608` (8MB) | Size of each multipart chunk |
| `PARALLEL_UPLOAD_PARTS` | `4` | Concurrent upload threads per segment |
| `THROTTLE_ENABLED` | `true` | S3 rate limiting circuit breaker |

**gRPC server:**

| Env Var | Default | Description |
|---------|---------|-------------|
| `GRPC_MAX_CONCURRENT_STREAMS` | unlimited | Max concurrent RPCs per connection |
| `GRPC_KEEPALIVE_INTERVAL_SECS` | _(none)_ | HTTP/2 keepalive ping interval |
| `GRPC_KEEPALIVE_TIMEOUT_SECS` | _(none)_ | Keepalive ping timeout |
| `GRPC_INITIAL_STREAM_WINDOW_SIZE` | `65535` | HTTP/2 flow control per stream in bytes |
| `GRPC_INITIAL_CONNECTION_WINDOW_SIZE` | `65535` | HTTP/2 flow control per connection in bytes |

**Write-ahead log:**

| Env Var | Default | Description |
|---------|---------|-------------|
| `WAL_ENABLED` | `false` | Enable WAL for crash recovery |
| `WAL_DIR` | `./data/wal` | WAL directory |
| `WAL_SYNC_INTERVAL_MS` | `100` | WAL fsync interval |
| `WAL_MAX_SIZE` | `1073741824` (1GB) | WAL rotation size |

**Cache:**

| Env Var | Default | Description |
|---------|---------|-------------|
| `STREAMHOUSE_CACHE` | `./data/cache` | Segment cache directory |
| `STREAMHOUSE_CACHE_SIZE` | `1073741824` (1GB) | Max cache size in bytes |

---

## Architecture

```
Clients (REST, gRPC, Kafka protocol)
    |
StreamHouse Unified Server
    |
    +-- Partition Writers (one per partition, concurrent)
    |       |
    |       +-- WAL (optional, crash recovery)
    |       +-- Segment Builder (batching, compression)
    |       +-- S3 Upload (group-commit for ACK_DURABLE)
    |
    +-- Metadata Store (SQLite or PostgreSQL)
    |       Topics, partitions, offsets, consumer groups, leases
    |
    +-- Segment Cache (local disk, LRU)
```

---

## Testing

```bash
cargo test --workspace

# With PostgreSQL
DATABASE_URL=postgres://user:pass@localhost/streamhouse cargo test --features postgres --workspace
```

## License

[Apache License, Version 2.0](LICENSE)
