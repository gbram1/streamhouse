# Configuration

All configuration is via environment variables.

---

## Server

| Env Var | Default | Description |
|---------|---------|-------------|
| `HTTP_ADDR` | `0.0.0.0:8080` | REST API bind address |
| `GRPC_ADDR` | `0.0.0.0:50051` | gRPC bind address |
| `KAFKA_ADDR` | `0.0.0.0:9092` | Kafka protocol bind address |

---

## Storage

### S3 (Production)

| Env Var | Default | Description |
|---------|---------|-------------|
| `STREAMHOUSE_BUCKET` | `streamhouse` | S3 bucket name |
| `AWS_REGION` | `us-east-1` | S3 region |
| `AWS_ACCESS_KEY_ID` | — | AWS credentials |
| `AWS_SECRET_ACCESS_KEY` | — | AWS credentials |
| `S3_ENDPOINT` | _(AWS S3)_ | Custom endpoint for MinIO or S3-compatible stores |

```bash
# AWS S3
export AWS_REGION=us-west-2
export STREAMHOUSE_BUCKET=my-bucket
./target/release/unified-server

# MinIO
export S3_ENDPOINT=http://localhost:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
./target/release/unified-server
```

### Local Filesystem (Development)

| Env Var | Default | Description |
|---------|---------|-------------|
| `USE_LOCAL_STORAGE` | _(unset)_ | Set to `1` to use local disk instead of S3 |
| `LOCAL_STORAGE_PATH` | `./data/storage` | Local storage directory |

```bash
USE_LOCAL_STORAGE=1 ./target/release/unified-server
```

---

## Metadata Store

| Env Var | Default | Description |
|---------|---------|-------------|
| `STREAMHOUSE_METADATA` | `sqlite` | Backend: `sqlite` or `postgres` |
| `DATABASE_URL` | — | Postgres connection string (required for `postgres`) |

SQLite stores data at `./data/metadata.db` automatically. For Postgres:

```bash
cargo build --release --features postgres -p streamhouse-server
export DATABASE_URL=postgres://user:pass@localhost/streamhouse
./target/release/unified-server
```

---

## Authentication

| Env Var | Default | Description |
|---------|---------|-------------|
| `STREAMHOUSE_AUTH_ENABLED` | `false` | Enable API key authentication on all endpoints |
| `STREAMHOUSE_ADMIN_KEY` | — | Admin key for agent management endpoints (`/api/v1/agents`) |

When auth is enabled:
- REST requires `Authorization: Bearer <api-key>`
- Kafka requires SASL/PLAIN with API key as username + password
- See [Authentication](authentication.md) for details

---

## Agents

| Env Var | Default | Description |
|---------|---------|-------------|
| `AGENT_ID` | _(auto UUID)_ | Unique agent identifier |
| `AGENT_ZONE` | `default` | Availability zone (for rack-aware assignment) |
| `DISABLE_EMBEDDED_AGENT` | `false` | Disable the embedded agent — use with standalone agents |

### Embedded vs Standalone

**Embedded** (default): Server runs an agent in the same process. Simple, good for dev and single-node.

**Standalone**: Set `DISABLE_EMBEDDED_AGENT=true` on the server, run separate agent processes. Use for production multi-node deployments.

```bash
# Server (API only)
DISABLE_EMBEDDED_AGENT=true ./target/release/unified-server &

# Agents
AGENT_ID=agent-1 AGENT_ZONE=us-east-1a ./target/release/standalone-agent &
AGENT_ID=agent-2 AGENT_ZONE=us-east-1b ./target/release/standalone-agent &
```

---

## Write Path Tuning

### Segment Flushing

| Env Var | Default | Description |
|---------|---------|-------------|
| `SEGMENT_MAX_SIZE` | `1048576` (1 MB) | Roll segment after this many bytes |
| `SEGMENT_MAX_AGE_MS` | `10000` (10s) | Roll segment after this much time |
| `FLUSH_INTERVAL_SECS` | `5` | Background flush check interval |

**Lower `SEGMENT_MAX_AGE_MS`** = faster end-to-end latency (data reaches S3 sooner), more S3 PUT requests.

**Higher `SEGMENT_MAX_SIZE`** = fewer S3 PUTs, better cost efficiency, higher latency.

### Durable Writes (ACK_DURABLE)

When using gRPC with `ACK_DURABLE`, multiple produce requests are batched into a single S3 upload:

| Env Var | Default | Description |
|---------|---------|-------------|
| `DURABLE_BATCH_MAX_AGE_MS` | `200` | Max wait before flushing batch |
| `DURABLE_BATCH_MAX_RECORDS` | `10000` | Flush after N records |
| `DURABLE_BATCH_MAX_BYTES` | `16777216` (16 MB) | Flush after N bytes |

Low-latency: `DURABLE_BATCH_MAX_AGE_MS=50` (50ms p99 durability).
High-throughput: `DURABLE_BATCH_MAX_AGE_MS=500` (more batching).

### WAL (Write-Ahead Log)

| Env Var | Default | Description |
|---------|---------|-------------|
| `WAL_ENABLED` | `false` | Enable crash recovery WAL. **Recommended `true` for production.** |
| `WAL_DIR` | `./data/wal` | WAL directory |
| `WAL_SYNC_INTERVAL_MS` | `100` | fsync interval (only applies in interval mode) |
| `WAL_MAX_SIZE` | `1073741824` (1 GB) | WAL rotation size |

The WAL uses channel-based group commit — multiple producers share a single `fdatasync` call for high throughput (2.21M records/sec benchmarked).

**Fsync policy**: The default interval mode (`WAL_SYNC_INTERVAL_MS=100`) fsyncs every 100ms. On a process crash the OS buffer cache survives, so nothing is lost. On a power failure, you lose at most ~100ms of writes. For zero-loss guarantees, use `ACK_DURABLE` mode which flushes to S3 before acking.

Enable WAL in production: if the server crashes before uploading a segment to S3, the WAL replays on restart. Cross-agent WAL recovery is also supported when the disk is shared/accessible.

### S3 Upload

| Env Var | Default | Description |
|---------|---------|-------------|
| `S3_UPLOAD_RETRIES` | `3` | Retry count with exponential backoff |
| `MULTIPART_THRESHOLD` | `8388608` (8 MB) | Use multipart upload above this size |
| `MULTIPART_PART_SIZE` | `8388608` (8 MB) | Multipart chunk size |
| `PARALLEL_UPLOAD_PARTS` | `4` | Concurrent upload threads |
| `THROTTLE_ENABLED` | `true` | Circuit breaker on S3 503/429 errors |

---

## Read Path Tuning

### Segment Cache

| Env Var | Default | Description |
|---------|---------|-------------|
| `STREAMHOUSE_CACHE` | `./data/cache` | Cache directory |
| `STREAMHOUSE_CACHE_SIZE` | `1073741824` (1 GB) | Max cache size (LRU eviction) |

Increase cache size if consumers frequently re-read recent data.

---

## Disaster Recovery

| Env Var | Default | Description |
|---------|---------|-------------|
| `SNAPSHOT_INTERVAL_SECS` | `3600` (1 hour) | Metadata snapshot interval. Set to `0` to disable. |
| `RECONCILE_FROM_S3` | _(unset)_ | Set to `1` to run reverse reconciliation and exit. Admin mode. |
| `RECONCILE_INTERVAL` | `3600` (1 hour) | How often the background orphan cleanup runs |
| `RECONCILE_GRACE` | `3600` (1 hour) | Grace period before orphaned S3 segments are deleted. Prevents deleting in-flight uploads. |

StreamHouse automatically takes metadata snapshots after S3 flushes. On startup with empty metadata, it auto-recovers by discovering orgs from S3, restoring the latest snapshot, and running reconcile-from-s3 to fill any gaps.

The S3 reconciler runs in the background to clean up orphaned segments (S3 upload succeeded but metadata write failed). It uses mark-and-sweep with a grace period to avoid deleting segments that are still in-flight.

---

## gRPC Tuning

| Env Var | Default | Description |
|---------|---------|-------------|
| `GRPC_MAX_CONCURRENT_STREAMS` | _(unlimited)_ | Max concurrent RPCs per connection |
| `GRPC_KEEPALIVE_INTERVAL_SECS` | _(none)_ | HTTP/2 keepalive interval |
| `GRPC_KEEPALIVE_TIMEOUT_SECS` | _(none)_ | Keepalive timeout |
| `GRPC_INITIAL_STREAM_WINDOW_SIZE` | `65535` | HTTP/2 flow control per stream |
| `GRPC_INITIAL_CONNECTION_WINDOW_SIZE` | `65535` | HTTP/2 flow control per connection |

---

## Logging

| Env Var | Default | Description |
|---------|---------|-------------|
| `RUST_LOG` | `info` | Log level: `error`, `warn`, `info`, `debug`, `trace` |
| `LOG_FORMAT` | `text` | Log format: `text` or `json` |

```bash
# Production
export RUST_LOG=info
export LOG_FORMAT=json

# Debug specific crates
export RUST_LOG=streamhouse_storage=debug,streamhouse_kafka=debug,tower=warn
```

---

## Example Configs

### Development

```bash
USE_LOCAL_STORAGE=1
WAL_ENABLED=false
RUST_LOG=info
```

### Production (AWS)

```bash
# Storage
AWS_REGION=us-east-1
STREAMHOUSE_BUCKET=my-streamhouse-prod

# Metadata
STREAMHOUSE_METADATA=postgres
DATABASE_URL=postgres://user:pass@rds-host/streamhouse

# Auth
STREAMHOUSE_AUTH_ENABLED=true

# Write path
WAL_ENABLED=true                    # Crash recovery
SEGMENT_MAX_SIZE=5242880            # 5 MB segments
DURABLE_BATCH_MAX_AGE_MS=200       # Batched durable acks

# Read path
STREAMHOUSE_CACHE_SIZE=10737418240  # 10 GB

# DR
SNAPSHOT_INTERVAL_SECS=3600         # Hourly metadata snapshots
RECONCILE_INTERVAL=3600             # Hourly orphan cleanup
RECONCILE_GRACE=3600                # 1 hour grace before deleting orphans

# Logging
RUST_LOG=info
LOG_FORMAT=json
```

### Docker Compose

See `docker-compose.yml` for a complete setup with Postgres, MinIO, 3 agents, Prometheus, and Grafana.

```bash
docker compose up -d
```
