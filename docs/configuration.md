# Configuration Reference

All configuration is via environment variables. Values are logged at startup.

## Server Ports

| Env Var | Default | Description |
|---------|---------|-------------|
| `GRPC_ADDR` | `0.0.0.0:50051` | gRPC bind address |
| `HTTP_ADDR` | `0.0.0.0:8080` | REST API bind address |
| `KAFKA_ADDR` | `0.0.0.0:9092` | Kafka protocol bind address |

## Storage Backends

StreamHouse supports local filesystem (development) or S3-compatible storage (production).

### Local Filesystem

| Env Var | Default | Description |
|---------|---------|-------------|
| `USE_LOCAL_STORAGE` | unset | Set to any value (e.g., `1`) to use local filesystem instead of S3 |
| `LOCAL_STORAGE_PATH` | `./data/storage` | Path for local storage |

**Example:**

```bash
USE_LOCAL_STORAGE=1 ./target/release/unified-server
```

### S3 (AWS or MinIO)

| Env Var | Default | Description |
|---------|---------|-------------|
| `STREAMHOUSE_BUCKET` | `streamhouse` | S3 bucket name |
| `AWS_REGION` | `us-east-1` | S3 region |
| `AWS_ACCESS_KEY_ID` | — | AWS access key |
| `AWS_SECRET_ACCESS_KEY` | — | AWS secret key |
| `S3_ENDPOINT` | _(AWS S3)_ | Custom S3 endpoint (for MinIO) |

**Example (MinIO):**

```bash
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export S3_ENDPOINT=http://localhost:9000
export STREAMHOUSE_BUCKET=streamhouse
./target/release/unified-server
```

**Example (AWS S3):**

```bash
export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
export AWS_REGION=us-west-2
export STREAMHOUSE_BUCKET=my-streamhouse-bucket
./target/release/unified-server
```

## Metadata Backends

StreamHouse stores topic/partition metadata, consumer group offsets, and agent leases in a metadata store.

| Backend | Use Case | Config |
|---------|----------|--------|
| SQLite | Development (default) | Automatic — stores at `./data/metadata.db` |
| PostgreSQL | Production | Build with `--features postgres`, set `DATABASE_URL` |

### SQLite (Default)

No configuration needed. The metadata database is automatically created at `./data/metadata.db`.

```bash
# Runs with SQLite
./target/release/unified-server
```

### PostgreSQL

```bash
# Build with PostgreSQL support
cargo build --release --features postgres -p streamhouse-server --bin unified-server

# Set database URL
export DATABASE_URL=postgres://user:password@localhost/streamhouse

# Run
./target/release/unified-server
```

You can also set the metadata store type explicitly:

```bash
export STREAMHOUSE_METADATA=sqlite   # default
export STREAMHOUSE_METADATA=postgres # requires --features postgres
```

## Agent Configuration

StreamHouse uses agents to own partitions and coordinate writes. Agents can run embedded (same process as server) or standalone (separate processes).

| Env Var | Default | Description |
|---------|---------|-------------|
| `AGENT_ID` | auto-generated UUID | Agent identifier (must be unique per agent) |
| `AGENT_ZONE` | `default` | Agent zone for rack-aware partition assignment |
| `DISABLE_EMBEDDED_AGENT` | `false` | Set to `true` to disable the embedded agent (use standalone agents only) |

**Example (embedded agent, default):**

```bash
./target/release/unified-server
# Embedded agent auto-starts with a random agent ID
```

**Example (standalone agents):**

```bash
# Server with embedded agent disabled
DISABLE_EMBEDDED_AGENT=true ./target/release/unified-server &

# Standalone agent 1
AGENT_ID=agent-1 AGENT_ZONE=us-east-1a ./target/release/standalone-agent &

# Standalone agent 2
AGENT_ID=agent-2 AGENT_ZONE=us-east-1b ./target/release/standalone-agent &
```

## Performance Tuning

### Segment Flushing

Controls when segments are rolled and uploaded to S3.

| Env Var | Default | Description |
|---------|---------|-------------|
| `FLUSH_INTERVAL_SECS` | `5` | Background flush interval for ACK_BUFFERED path (seconds) |
| `SEGMENT_MAX_SIZE` | `1048576` (1MB) | Roll segment after this many bytes |
| `SEGMENT_MAX_AGE_MS` | `10000` (10s) | Roll segment after this much time (milliseconds) |

**Tuning guidance:**
- Lower `SEGMENT_MAX_AGE_MS` = more frequent uploads, lower end-to-end latency
- Higher `SEGMENT_MAX_SIZE` = fewer S3 PUTs, better cost efficiency
- `FLUSH_INTERVAL_SECS` only affects ACK_BUFFERED (async) writes, not ACK_DURABLE (sync)

### ACK_DURABLE Batching

When using gRPC `ProduceBatch` with `ack_mode: ACK_DURABLE`, StreamHouse batches multiple produce requests into a single segment upload (group-commit pattern).

| Env Var | Default | Description |
|---------|---------|-------------|
| `DURABLE_BATCH_MAX_AGE_MS` | `200` (200ms) | Max wait time before flushing batch. Lower = lower latency, more S3 PUTs. Higher = more batching, better throughput. |
| `DURABLE_BATCH_MAX_RECORDS` | `10000` | Force flush after N records |
| `DURABLE_BATCH_MAX_BYTES` | `16777216` (16MB) | Force flush after N bytes |

**Tuning guidance:**
- `DURABLE_BATCH_MAX_AGE_MS=50` for low-latency use cases (50ms p99 durability)
- `DURABLE_BATCH_MAX_AGE_MS=500` for high-throughput use cases (better batching, more efficient S3 uploads)
- Increase `DURABLE_BATCH_MAX_BYTES` if you have very large messages

### S3 Upload

| Env Var | Default | Description |
|---------|---------|-------------|
| `S3_UPLOAD_RETRIES` | `3` | Retry count for S3 uploads (with exponential backoff) |
| `MULTIPART_THRESHOLD` | `8388608` (8MB) | Switch to multipart upload above this size |
| `MULTIPART_PART_SIZE` | `8388608` (8MB) | Size of each multipart chunk |
| `PARALLEL_UPLOAD_PARTS` | `4` | Concurrent upload threads per segment |
| `THROTTLE_ENABLED` | `true` | S3 rate limiting circuit breaker (backs off on 503/429 errors) |

**Tuning guidance:**
- Lower `MULTIPART_THRESHOLD` if you frequently produce large batches
- Increase `PARALLEL_UPLOAD_PARTS` for better upload throughput on large segments (uses more network/CPU)
- Disable `THROTTLE_ENABLED` if you have reserved capacity with AWS

### gRPC Server

| Env Var | Default | Description |
|---------|---------|-------------|
| `GRPC_MAX_CONCURRENT_STREAMS` | unlimited | Max concurrent RPCs per connection (backpressure) |
| `GRPC_KEEPALIVE_INTERVAL_SECS` | _(none)_ | HTTP/2 keepalive ping interval |
| `GRPC_KEEPALIVE_TIMEOUT_SECS` | _(none)_ | Keepalive ping timeout |
| `GRPC_INITIAL_STREAM_WINDOW_SIZE` | `65535` | HTTP/2 flow control per stream (bytes) |
| `GRPC_INITIAL_CONNECTION_WINDOW_SIZE` | `65535` | HTTP/2 flow control per connection (bytes) |

**Tuning guidance:**
- Set `GRPC_MAX_CONCURRENT_STREAMS=1000` to limit concurrent requests per connection
- Set `GRPC_KEEPALIVE_INTERVAL_SECS=30` for long-lived connections over flaky networks
- Increase window sizes for high-throughput streaming consumers

### Write-Ahead Log (WAL)

The WAL provides crash recovery for in-flight writes. If the server crashes before a segment is uploaded to S3, the WAL replays on restart.

| Env Var | Default | Description |
|---------|---------|-------------|
| `WAL_ENABLED` | `false` | Enable WAL for crash recovery |
| `WAL_DIR` | `./data/wal` | WAL directory |
| `WAL_SYNC_INTERVAL_MS` | `100` | WAL fsync interval (milliseconds) |
| `WAL_MAX_SIZE` | `1073741824` (1GB) | WAL rotation size |

**Tuning guidance:**
- Enable WAL in production for durability: `WAL_ENABLED=true`
- Lower `WAL_SYNC_INTERVAL_MS` for stricter durability guarantees (more fsync calls, lower throughput)
- Increase `WAL_MAX_SIZE` if you have high write rates

### Segment Cache

The segment cache stores recently read segments on local disk for faster consume performance.

| Env Var | Default | Description |
|---------|---------|-------------|
| `STREAMHOUSE_CACHE` | `./data/cache` | Segment cache directory |
| `STREAMHOUSE_CACHE_SIZE` | `1073741824` (1GB) | Max cache size in bytes (LRU eviction) |

**Tuning guidance:**
- Increase `STREAMHOUSE_CACHE_SIZE` if you have many consumers re-reading recent data
- Set to a smaller value (e.g., `104857600` = 100MB) on memory-constrained environments

## Example Configurations

### Development (local, no cloud dependencies)

```bash
export USE_LOCAL_STORAGE=1
export WAL_ENABLED=false
export RUST_LOG=info

./target/release/unified-server
```

### Production (AWS S3, PostgreSQL, high throughput)

```bash
# Storage
export AWS_REGION=us-east-1
export AWS_ACCESS_KEY_ID=<key>
export AWS_SECRET_ACCESS_KEY=<secret>
export STREAMHOUSE_BUCKET=my-streamhouse-prod

# Metadata
export DATABASE_URL=postgres://user:pass@db.example.com/streamhouse

# Performance
export DURABLE_BATCH_MAX_AGE_MS=200
export SEGMENT_MAX_SIZE=5242880  # 5MB segments
export WAL_ENABLED=true
export STREAMHOUSE_CACHE_SIZE=10737418240  # 10GB cache

# Observability
export RUST_LOG=info

./target/release/unified-server --features postgres
```

### Docker Compose (full stack with MinIO)

See `docker-compose.yml` for a complete example with PostgreSQL, MinIO, Prometheus, and Grafana.

```bash
docker compose up -d
```

## .env File Support

You can create a `.env` file in the project root (not tracked by git):

```bash
# .env
USE_LOCAL_STORAGE=1
WAL_ENABLED=true
RUST_LOG=debug
SEGMENT_MAX_AGE_MS=5000
```

Then source it before running:

```bash
set -a; source .env; set +a
./target/release/unified-server
```

## Logging

Set `RUST_LOG` to control log verbosity:

```bash
export RUST_LOG=error   # errors only
export RUST_LOG=warn    # warnings and errors
export RUST_LOG=info    # informational (default for production)
export RUST_LOG=debug   # verbose debugging
export RUST_LOG=trace   # very verbose (includes gRPC frames)

# Per-module filtering
export RUST_LOG=streamhouse=debug,tower=warn
```
