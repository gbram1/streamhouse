# StreamHouse Unified Server

The unified server consolidates all StreamHouse services into a single binary for simplified deployment and management.

## Overview

Previously, StreamHouse required running three separate server processes:
- `streamhouse-server` (gRPC on port 9090)
- `streamhouse-api` (REST API on port 3001)
- `streamhouse-schema-registry` (Schema Registry on port 8081)

The unified server combines all three into a single process with shared infrastructure:

**Unified Server Architecture:**
- **gRPC API**: Port 50051 (producers/consumers)
- **REST API**: Port 8080/api/v1/* (management)
- **Schema Registry**: Port 8080/schemas/* (schema management)
- **Web Console**: Port 8080/* (UI)
- **Health Check**: Port 8080/health

## Quick Start

### 1. Build the Web Console

```bash
cd web
npm install
npm run build
cd ..
```

### 2. Start the Unified Server

**Development (local storage):**
```bash
export USE_LOCAL_STORAGE=1
cargo run -p streamhouse-server --bin unified-server
```

**Production (S3):**
```bash
export STREAMHOUSE_BUCKET=my-bucket
export AWS_REGION=us-west-2
cargo run -p streamhouse-server --bin unified-server --release
```

### 3. Access Services

- **Web Console**: http://localhost:8080
- **REST API**: http://localhost:8080/api/v1
- **Schema Registry**: http://localhost:8080/schemas
- **gRPC**: localhost:50051
- **Health Check**: http://localhost:8080/health

## Configuration

All configuration is done via environment variables:

### Server Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `GRPC_ADDR` | `0.0.0.0:50051` | gRPC server bind address |
| `HTTP_ADDR` | `0.0.0.0:8080` | HTTP server bind address |
| `WEB_CONSOLE_PATH` | `./web/out` | Path to web console static files |

### Storage Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `STREAMHOUSE_METADATA` | `./data/metadata.db` | SQLite database path |
| `STREAMHOUSE_BUCKET` | `streamhouse` | S3 bucket name |
| `STREAMHOUSE_PREFIX` | `data` | S3 key prefix |
| `AWS_REGION` | `us-east-1` | AWS region |
| `S3_ENDPOINT` | (none) | Custom S3 endpoint (MinIO/LocalStack) |

### Local Development

| Variable | Default | Description |
|----------|---------|-------------|
| `USE_LOCAL_STORAGE` | (disabled) | Use local filesystem instead of S3 |
| `LOCAL_STORAGE_PATH` | `./data/storage` | Local storage directory |

### Cache Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `STREAMHOUSE_CACHE` | `./data/cache` | Cache directory |
| `STREAMHOUSE_CACHE_SIZE` | `1073741824` | Cache size in bytes (1GB) |

### Other Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `FLUSH_INTERVAL_SECS` | `5` | Background flush interval |
| `RUST_LOG` | `info` | Log level (debug/info/warn/error) |

## Example Configurations

### Development Setup

```bash
#!/bin/bash
export USE_LOCAL_STORAGE=1
export RUST_LOG=debug
export STREAMHOUSE_CACHE_SIZE=536870912  # 512MB

cargo run -p streamhouse-server --bin unified-server
```

### Production Setup with S3

```bash
#!/bin/bash
export STREAMHOUSE_BUCKET=streamhouse-prod
export AWS_REGION=us-west-2
export STREAMHOUSE_CACHE_SIZE=5368709120  # 5GB
export FLUSH_INTERVAL_SECS=10
export RUST_LOG=info

cargo run -p streamhouse-server --bin unified-server --release
```

### Docker Setup

```bash
docker run -p 50051:50051 -p 8080:8080 \
  -e USE_LOCAL_STORAGE=1 \
  -e WEB_CONSOLE_PATH=/app/web/out \
  -v $(pwd)/data:/app/data \
  streamhouse/unified-server:latest
```

## Testing the Server

### Test Health Endpoint

```bash
curl http://localhost:8080/health
# Expected: OK
```

### Test REST API

```bash
# List topics
curl http://localhost:8080/api/v1/topics | jq '.'

# Create a topic
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "test-topic", "partitions": 3}'
```

### Test Schema Registry

```bash
# List subjects
curl http://localhost:8080/schemas/subjects | jq '.'

# Register a schema
curl -X POST http://localhost:8080/schemas/subjects/test-value/versions \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"User\",\"fields\":[{\"name\":\"id\",\"type\":\"long\"}]}",
    "schemaType": "AVRO"
  }' | jq '.'
```

### Test gRPC

```bash
# List topics via gRPC
grpcurl -plaintext localhost:50051 streamhouse.StreamHouse/ListTopics

# Create topic via gRPC
grpcurl -plaintext -d '{"name":"grpc-topic","partition_count":3}' \
  localhost:50051 streamhouse.StreamHouse/CreateTopic
```

### Test Web Console

Open http://localhost:8080 in your browser and navigate through:
- Dashboard
- Topics
- Schemas
- Agents
- Console (produce/consume)

## Migrating from Separate Servers

If you're currently running separate servers, follow these steps to migrate:

### 1. Stop All Separate Servers

```bash
# Stop existing servers
pkill streamhouse-server  # gRPC server
pkill api                 # REST API server
pkill schema-registry     # Schema Registry server
```

### 2. Verify Data Directory

Ensure your `./data` directory contains:
- `metadata.db` - SQLite database
- `storage/` - Local storage (if using local storage)
- `cache/` - Segment cache

### 3. Start Unified Server

```bash
export USE_LOCAL_STORAGE=1  # If you were using local storage
cargo run -p streamhouse-server --bin unified-server
```

### 4. Update Client Connections

**Before:**
```yaml
grpc_endpoint: localhost:9090
rest_api_endpoint: http://localhost:3001
schema_registry_endpoint: http://localhost:8081
```

**After:**
```yaml
grpc_endpoint: localhost:50051
rest_api_endpoint: http://localhost:8080/api/v1
schema_registry_endpoint: http://localhost:8080/schemas
```

## Graceful Shutdown

The unified server supports graceful shutdown on SIGINT (Ctrl+C) or SIGTERM:

1. Stops accepting new connections
2. Flushes all pending writes to S3
3. Closes all active connections
4. Shuts down cleanly

```bash
# Send SIGTERM for graceful shutdown
kill -TERM <pid>

# Or use Ctrl+C
```

## Monitoring

### Logs

The unified server uses structured logging with the `tracing` crate:

```bash
# Set log level
export RUST_LOG=debug
cargo run -p streamhouse-server --bin unified-server

# Example output:
# 2026-01-29T04:37:37.491Z INFO unified_server: 🚀 Starting StreamHouse Unified Server
# 2026-01-29T04:37:37.492Z INFO unified_server: 📦 Initializing metadata store at ./data/metadata.db
# 2026-01-29T04:37:37.501Z INFO unified_server: ☁️  Initializing object store (bucket: streamhouse)
```

### Health Checks

Use the `/health` endpoint for liveness probes:

```bash
# Kubernetes liveness probe
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 10
```

## Benefits of Unified Server

### Simplified Deployment
- Single binary instead of three separate processes
- Reduced complexity in orchestration (Docker, Kubernetes)
- Easier to manage in production

### Shared Infrastructure
- One metadata store connection pool
- One object store connection pool
- Shared segment cache (more efficient memory usage)
- Single configuration file

### Better Resource Utilization
- Lower memory overhead (shared allocations)
- Reduced network overhead (internal communication)
- Simplified monitoring (one process to track)

### Easier Development
- Start one server instead of three
- Simpler local development setup
- Faster iteration cycle

## Backwards Compatibility

The separate server binaries still exist and work:

```bash
# Still available:
cargo run -p streamhouse-server              # gRPC only
cargo run -p streamhouse-api --bin api       # REST API only
cargo run -p streamhouse-schema-registry --bin schema-registry  # Schema Registry only
```

This allows gradual migration or use cases where you need services separated.

## Troubleshooting

### Port Already in Use

```bash
# Check what's using port 8080
lsof -i :8080

# Kill the process
kill -9 <pid>

# Or use a different port
export HTTP_ADDR=0.0.0.0:8888
cargo run -p streamhouse-server --bin unified-server
```

### Web Console Not Found

```bash
# Rebuild web console
cd web && npm run build && cd ..

# Or specify custom path
export WEB_CONSOLE_PATH=/path/to/web/out
cargo run -p streamhouse-server --bin unified-server
```

### Database Locked

```bash
# Remove stale lock files
rm ./data/metadata.db-wal
rm ./data/metadata.db-shm
```

### Permission Denied on Cache Directory

```bash
# Fix permissions
chmod 755 ./data/cache
```

## Production Deployment

### Systemd Service

Create `/etc/systemd/system/streamhouse.service`:

```ini
[Unit]
Description=StreamHouse Unified Server
After=network.target

[Service]
Type=simple
User=streamhouse
Group=streamhouse
WorkingDirectory=/opt/streamhouse
Environment="STREAMHOUSE_BUCKET=streamhouse-prod"
Environment="AWS_REGION=us-west-2"
Environment="RUST_LOG=info"
ExecStart=/opt/streamhouse/target/release/unified-server
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

### Docker Compose

```yaml
version: '3.8'

services:
  streamhouse:
    image: streamhouse/unified-server:latest
    ports:
      - "50051:50051"
      - "8080:8080"
    environment:
      - USE_LOCAL_STORAGE=1
      - RUST_LOG=info
      - STREAMHOUSE_CACHE_SIZE=1073741824
    volumes:
      - ./data:/app/data
      - ./web/out:/app/web/out
    restart: unless-stopped
```

## Performance

The unified server has minimal overhead compared to running separate servers:

| Metric | Separate Servers | Unified Server |
|--------|------------------|----------------|
| Memory | ~300MB | ~200MB |
| Startup time | ~5s total | ~2s |
| Connection pools | 3x | 1x |
| Cache efficiency | Lower | Higher |

## Next Steps

- [API Documentation](./API.md)
- [Schema Registry Guide](./schema-registry.md)
- [Production Deployment Guide](./PRODUCTION.md)
- [Architecture Overview](./ARCHITECTURE_OVERVIEW.md)
