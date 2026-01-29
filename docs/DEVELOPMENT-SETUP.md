# Development Setup Guide

This guide explains how to run StreamHouse for local development.

## Quick Start (Development)

### Option 1: Separate Services (Recommended for Frontend Development)

This gives you hot reload on the frontend:

```bash
# Terminal 1: Start backend API
cd streamhouse
USE_LOCAL_STORAGE=1 cargo run -p streamhouse-api --bin api
# API will be on http://localhost:3001

# Terminal 2: Start Next.js dev server
cd streamhouse/web
npm run dev
# Frontend will be on http://localhost:3000 with hot reload
```

### Option 2: Unified Server (Backend Only)

For backend development or full-stack testing:

```bash
# Start unified server (gRPC + REST API + Schema Registry)
cd streamhouse
USE_LOCAL_STORAGE=1 cargo run -p streamhouse-server --bin unified-server
# Services available at:
# - gRPC: localhost:50051
# - REST API: http://localhost:8080/api/v1
# - Schema Registry: http://localhost:8080/schemas

# In another terminal: Start Next.js dev server
cd streamhouse/web
npm run dev
# Visit http://localhost:3000
```

## Connecting to MinIO and PostgreSQL

### MinIO Setup (S3-Compatible Storage)

1. Start MinIO:
```bash
docker run -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data --console-address ":9001"
```

2. Create a bucket at http://localhost:9001 (login: minioadmin/minioadmin)

3. Configure StreamHouse to use MinIO:
```bash
export S3_ENDPOINT=http://localhost:9000
export STREAMHOUSE_BUCKET=streamhouse
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_REGION=us-east-1
unset USE_LOCAL_STORAGE  # Important: disable local storage

# Now run the server
cargo run -p streamhouse-server --bin unified-server
```

### PostgreSQL Setup (Metadata Store)

1. Start PostgreSQL:
```bash
docker run -d -p 5432:5432 \
  -e POSTGRES_DB=streamhouse \
  -e POSTGRES_USER=streamhouse \
  -e POSTGRES_PASSWORD=streamhouse \
  postgres:15
```

2. Build with PostgreSQL feature:
```bash
cargo build -p streamhouse-server --bin unified-server --features postgres
```

3. Configure connection:
```bash
export DATABASE_URL=postgresql://streamhouse:streamhouse@localhost/streamhouse

# Run the server
cargo run -p streamhouse-server --bin unified-server --features postgres
```

### Full Setup (MinIO + PostgreSQL)

```bash
# Start infrastructure
docker-compose up -d  # If you have a docker-compose.yml

# Or start manually:
# Terminal 1: MinIO
docker run -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data --console-address ":9001"

# Terminal 2: PostgreSQL
docker run -d -p 5432:5432 \
  -e POSTGRES_DB=streamhouse \
  -e POSTGRES_USER=streamhouse \
  -e POSTGRES_PASSWORD=streamhouse \
  postgres:15

# Terminal 3: StreamHouse with PostgreSQL + MinIO
export DATABASE_URL=postgresql://streamhouse:streamhouse@localhost/streamhouse
export S3_ENDPOINT=http://localhost:9000
export STREAMHOUSE_BUCKET=streamhouse
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_REGION=us-east-1

cargo run -p streamhouse-server --bin unified-server --features postgres

# Terminal 4: Frontend
cd web && npm run dev
# Visit http://localhost:3000
```

## Environment Variables Reference

### Storage Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `USE_LOCAL_STORAGE` | (unset) | Use local filesystem instead of S3 |
| `LOCAL_STORAGE_PATH` | `./data/storage` | Path for local storage |
| `STREAMHOUSE_BUCKET` | `streamhouse` | S3 bucket name |
| `S3_ENDPOINT` | (none) | MinIO/LocalStack endpoint (e.g., `http://localhost:9000`) |
| `AWS_REGION` | `us-east-1` | AWS region |
| `AWS_ACCESS_KEY_ID` | (from env) | AWS/MinIO access key |
| `AWS_SECRET_ACCESS_KEY` | (from env) | AWS/MinIO secret key |

### Database Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `STREAMHOUSE_METADATA` | `./data/metadata.db` | SQLite database path (when not using PostgreSQL) |
| `DATABASE_URL` | (none) | PostgreSQL connection string (requires `--features postgres`) |

### Server Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `GRPC_ADDR` | `0.0.0.0:50051` | gRPC bind address |
| `HTTP_ADDR` | `0.0.0.0:8080` | HTTP bind address (unified server) |
| `RUST_LOG` | `info` | Log level (trace/debug/info/warn/error) |

## Troubleshooting

### "Connection refused" when accessing API

Make sure the backend is running:
```bash
# Check if API server is running
curl http://localhost:3001/health  # Separate API server
# OR
curl http://localhost:8080/health  # Unified server
```

### "Cannot connect to MinIO"

1. Check MinIO is running: `docker ps | grep minio`
2. Test access: `curl http://localhost:9000`
3. Verify credentials in environment variables
4. Make sure `USE_LOCAL_STORAGE` is unset

### "Database connection failed"

1. Check PostgreSQL is running: `docker ps | grep postgres`
2. Test connection: `psql postgresql://streamhouse:streamhouse@localhost/streamhouse`
3. Verify `DATABASE_URL` is correct
4. Make sure you built with `--features postgres`

### Frontend shows blank page

The web console is a Next.js app that requires a dev server. You cannot run it as static files at localhost:8080.

**Solution:**
```bash
cd web && npm run dev
# Visit http://localhost:3000 (not 8080)
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Development Setup                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐         ┌──────────────────────────┐    │
│  │   Next.js    │  HTTP   │   Unified Server         │    │
│  │   Dev Server ├────────>│                          │    │
│  │ (localhost:  │         │  • gRPC (:50051)         │    │
│  │    3000)     │         │  • REST API (:8080/api)  │    │
│  └──────────────┘         │  • Schema Reg (:8080/    │    │
│                           │    schemas)               │    │
│                           └───────────┬──────────────┘    │
│                                       │                    │
│                           ┌───────────┴───────────┐        │
│                           │   Infrastructure      │        │
│                           │   • SQLite/PostgreSQL │        │
│                           │   • Local FS / MinIO  │        │
│                           │   • Segment Cache     │        │
│                           └───────────────────────┘        │
└─────────────────────────────────────────────────────────────┘
```

## Recommended Development Workflow

1. **Start with SQLite + Local Storage** (simplest):
   ```bash
   USE_LOCAL_STORAGE=1 cargo run -p streamhouse-server --bin unified-server
   cd web && npm run dev
   ```

2. **Upgrade to MinIO** (test S3 integration):
   ```bash
   docker run -p 9000:9000 minio/minio server /data
   # Configure MinIO env vars
   cargo run -p streamhouse-server --bin unified-server
   ```

3. **Upgrade to PostgreSQL** (test distributed setup):
   ```bash
   docker run -p 5432:5432 postgres:15
   # Configure PostgreSQL env vars
   cargo build --features postgres
   cargo run -p streamhouse-server --bin unified-server --features postgres
   ```

## Production Deployment

For production, you would:
1. Deploy unified server with PostgreSQL + S3 (real AWS S3, not MinIO)
2. Deploy Next.js separately (Vercel, or `npm run build && npm run start`)
3. Configure CORS and proper authentication

See [PRODUCTION.md](./PRODUCTION.md) for production deployment guide.
