# StreamHouse Setup Guide

## Quick Start - Development with Real Services

StreamHouse is now running with **PostgreSQL** and **MinIO** instead of SQLite and local files!

### Current Setup

✅ **PostgreSQL**: Running on port 5432 (database: `streamhouse_metadata`)
✅ **MinIO**: Running on ports 9000 (API) and 9001 (Console)
✅ **Unified Server**: Running on ports 50051 (gRPC) and 8080 (HTTP)

### Access Your Services

| Service | URL | Credentials |
|---------|-----|-------------|
| **Web Frontend** | http://localhost:3000 | N/A |
| **REST API** | http://localhost:8080/api/v1 | N/A |
| **Schema Registry** | http://localhost:8080/schemas | N/A |
| **MinIO Console** | http://localhost:9001 | minioadmin / minioadmin |
| **PostgreSQL** | localhost:5432 | streamhouse / streamhouse_dev |

## Starting Everything

### 1. Start Infrastructure (Docker)

```bash
docker-compose up -d
```

This starts PostgreSQL and MinIO in the background.

### 2. Start Unified Server (Backend)

```bash
./start-with-postgres-minio.sh
```

This starts the StreamHouse unified server with:
- PostgreSQL for metadata storage
- MinIO for S3-compatible object storage
- All APIs (gRPC, REST, Schema Registry)

### 3. Start Web Frontend (UI)

In a **separate terminal**:

```bash
cd web
npm run dev
```

Then visit **http://localhost:3000** in your browser.

## Verify Everything is Working

Run the verification script:

```bash
./verify-setup.sh
```

This checks:
- ✅ Docker containers are running
- ✅ PostgreSQL is accepting connections
- ✅ MinIO is healthy and bucket exists
- ✅ Environment variables are set

## What's Different from Before?

| Component | Old (SQLite + Local) | New (PostgreSQL + MinIO) |
|-----------|---------------------|--------------------------|
| **Metadata** | SQLite file (`./data/metadata.db`) | PostgreSQL database |
| **Storage** | Local filesystem (`./data/storage/`) | MinIO S3 buckets |
| **Scalability** | Single machine only | Can scale horizontally |
| **Production-ready** | No | Yes |

## Common Commands

### View Data

**PostgreSQL:**
```bash
# Connect to database
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata

# List tables
\dt

# Query topics
SELECT * FROM topics;

# Exit
\q
```

**MinIO:**
```bash
# Open MinIO Console
open http://localhost:9001
# Login: minioadmin / minioadmin

# Or use CLI
docker exec streamhouse-minio mc ls local/streamhouse
```

### Manage Infrastructure

```bash
# Stop all services
docker-compose down

# Stop and remove all data (WARNING: Deletes everything!)
docker-compose down -v

# View logs
docker-compose logs -f postgres
docker-compose logs -f minio

# Restart services
docker-compose restart
```

### Reset Everything

If you want to start fresh:

```bash
# Stop everything
docker-compose down -v

# Remove local data
rm -rf data/

# Start fresh
docker-compose up -d
./start-with-postgres-minio.sh
```

## Testing the API

### Create a Topic

```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name":"events","partitions":3,"retentionMs":86400000}'
```

### List Topics

```bash
curl http://localhost:8080/api/v1/topics | jq '.'
```

### Produce Messages (via gRPC)

```bash
grpcurl -plaintext -d '{"topic":"events","partition":0,"key":"user123","value":"{"event":"login"}"}' \
  localhost:50051 streamhouse.StreamHouse/Produce
```

### Register a Schema

```bash
curl -X POST http://localhost:8080/schemas/subjects/events-value/versions \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Event\",\"fields\":[{\"name\":\"event\",\"type\":\"string\"}]}",
    "schemaType": "AVRO"
  }'
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│              StreamHouse Architecture                    │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────┐         ┌─────────────────────────┐  │
│  │   Next.js    │  HTTP   │   Unified Server        │  │
│  │   :3000      ├────────>│                         │  │
│  └──────────────┘         │  • gRPC (:50051)        │  │
│                           │  • REST API (:8080)     │  │
│                           │  • Schema Registry      │  │
│                           └──────────┬──────────────┘  │
│                                      │                  │
│                           ┌──────────┴──────────┐      │
│                           │  Infrastructure     │      │
│                           │  • PostgreSQL :5432 │      │
│                           │  • MinIO :9000      │      │
│                           └─────────────────────┘      │
└─────────────────────────────────────────────────────────┘
```

## Development Workflow

For active development:

```bash
# Terminal 1: Infrastructure (runs in background)
docker-compose up -d

# Terminal 2: Backend
./start-with-postgres-minio.sh

# Terminal 3: Frontend (with hot reload)
cd web && npm run dev
```

Now you can:
- Edit frontend code → Instant hot reload at http://localhost:3000
- Edit backend code → Restart `./start-with-postgres-minio.sh`
- Data persists in PostgreSQL and MinIO (survives restarts)

## Troubleshooting

### "Port already in use"

```bash
# Find process using port 8080
lsof -i :8080
kill -9 <PID>

# Or use different port
export HTTP_ADDR=0.0.0.0:8888
./start-with-postgres-minio.sh
```

### "Cannot connect to PostgreSQL"

```bash
# Check if running
docker ps | grep postgres

# Check logs
docker-compose logs postgres

# Restart
docker-compose restart postgres
```

### "MinIO bucket not found"

```bash
# Create bucket
docker exec streamhouse-minio sh -c "mc alias set local http://localhost:9000 minioadmin minioadmin && mc mb local/streamhouse"
```

### Frontend shows blank page

The web console is a Next.js app that must run on its own dev server:

```bash
cd web
npm run dev
# Visit http://localhost:3000 (NOT 8080)
```

## Next Steps

- **Create Topics**: Use the web UI at http://localhost:3000/topics
- **Produce/Consume**: Use the console at http://localhost:3000/console
- **Manage Schemas**: Visit http://localhost:3000/schemas
- **View Data**: Check PostgreSQL and MinIO consoles

## Files Reference

| File | Purpose |
|------|---------|
| `docker-compose.yml` | Defines PostgreSQL and MinIO services |
| `start-with-postgres-minio.sh` | Starts unified server with production config |
| `verify-setup.sh` | Verifies infrastructure is running correctly |
| `docs/DEVELOPMENT-SETUP.md` | Detailed development guide |
| `docs/UNIFIED-SERVER.md` | Unified server documentation |

## Need Help?

- **Development Setup**: See [docs/DEVELOPMENT-SETUP.md](docs/DEVELOPMENT-SETUP.md)
- **Unified Server**: See [docs/UNIFIED-SERVER.md](docs/UNIFIED-SERVER.md)
- **Schema Registry**: See [docs/schema-registry.md](docs/schema-registry.md)
- **Architecture**: See [docs/ARCHITECTURE_OVERVIEW.md](docs/ARCHITECTURE_OVERVIEW.md)

---

**You're all set!** 🚀

StreamHouse is now running with production-grade infrastructure (PostgreSQL + MinIO).
