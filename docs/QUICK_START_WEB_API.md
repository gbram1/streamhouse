# Quick Start: Web Console + REST API

Complete guide to running StreamHouse with the web console and REST API.

## Prerequisites

- Rust 1.75+ installed
- Node.js 20+ installed
- Docker (for MinIO)

## Step 1: Start MinIO

```bash
docker-compose up -d minio
```

## Step 2: Start an Agent

The agent handles data storage and partition management.

```bash
# Terminal 1
export METADATA_STORE=./data/metadata.db
export AWS_ENDPOINT_URL=http://localhost:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AGENT_ID=agent-001
export AGENT_ADDRESS=127.0.0.1:9090
export METRICS_PORT=8080

cargo run --release --bin agent --features metrics
```

Expected output:
```
✓ Metadata store connected
✓ Object store connected (bucket: streamhouse-data)
✓ Agent started successfully
✓ gRPC server started on 127.0.0.1:9090
✓ Metrics server started on 0.0.0.0:8080

Agent agent-001 is now running
  gRPC:    127.0.0.1:9090
  Metrics: http://0.0.0.0:8080
```

## Step 3: Start REST API

The REST API provides HTTP/JSON access for the web console.

```bash
# Terminal 2
export METADATA_STORE=./data/metadata.db
export AWS_ENDPOINT_URL=http://localhost:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export API_PORT=3001

cargo run --release --bin api
```

Expected output:
```
✓ Metadata store connected
✓ Producer initialized

REST API Server Ready
  API: http://localhost:3001/api/v1
  Swagger UI: http://localhost:3001/swagger-ui
  Health: http://localhost:3001/health

🚀 REST API server listening on 0.0.0.0:3001
```

## Step 4: Start Web Console

The Next.js web application for managing StreamHouse.

```bash
# Terminal 3
cd web
echo "NEXT_PUBLIC_API_URL=http://localhost:3001" > .env.local
npm install
npm run dev
```

Expected output:
```
▲ Next.js 16.1.6
- Local:        http://localhost:3000
- Ready in 1.5s
```

## Step 5: Open Web Console

Open your browser to http://localhost:3000

You should see:
- **Landing page** with StreamHouse branding
- Click **"Get Started"** or **"Dashboard"**
- **Dashboard** shows:
  - Stats (topics, agents, throughput)
  - Topics table
  - Agents table
  - Consumer groups table

## Step 6: Test the Stack

### Create a Topic via Web Console

1. Go to http://localhost:3000/dashboard
2. Click **"Topics"** tab
3. Click **"Create Topic"** button
4. Fill in:
   - Name: `orders`
   - Partitions: `3`
5. Click **"Create"**

### Or via REST API

```bash
curl -X POST http://localhost:3001/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name":"orders","partitions":3,"replication_factor":1}'
```

### Produce a Message

```bash
curl -X POST http://localhost:3001/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{
    "topic":"orders",
    "key":"user123",
    "value":"{\"order_id\":42,\"amount\":99.99}"
  }'
```

Response:
```json
{
  "offset": 0,
  "partition": 0
}
```

### Check Metrics

```bash
curl http://localhost:3001/api/v1/metrics
```

Response:
```json
{
  "topics_count": 1,
  "agents_count": 1,
  "partitions_count": 3,
  "total_messages": 1
}
```

### View in Dashboard

Refresh http://localhost:3000/dashboard - you should now see:
- 1 topic ("orders")
- 1 agent (agent-001)
- Your produced message in the metrics

## Useful URLs

### Web Console
- Landing: http://localhost:3000
- Dashboard: http://localhost:3000/dashboard

### REST API
- Swagger UI: http://localhost:3001/swagger-ui
- Health: http://localhost:3001/health
- List Topics: http://localhost:3001/api/v1/topics
- List Agents: http://localhost:3001/api/v1/agents
- Metrics: http://localhost:3001/api/v1/metrics

### Agent Metrics
- Health: http://localhost:8080/health
- Ready: http://localhost:8080/ready
- Prometheus: http://localhost:8080/metrics

## Common Issues

### Port Already in Use

If you see "Address already in use":
```bash
# Kill processes on specific ports
lsof -ti:3000 | xargs kill  # Web console
lsof -ti:3001 | xargs kill  # REST API
lsof -ti:8080 | xargs kill  # Agent metrics
lsof -ti:9090 | xargs kill  # Agent gRPC
```

### MinIO Not Running

```bash
docker ps | grep minio
# If not running:
docker-compose up -d minio
```

### Database Issues

If you see SQLite errors, reset the database:
```bash
rm -f ./data/metadata.db*
# Restart agent - it will recreate the database
```

### Web Console Shows Empty Data

1. Check REST API is running: `curl http://localhost:3001/health`
2. Check `.env.local` in web folder has correct API URL
3. Check browser console for CORS errors
4. Restart web console: `cd web && npm run dev`

## Architecture Overview

```
┌──────────────────┐     HTTP/JSON     ┌──────────────────┐
│  Web Console     │◄─────────────────►│   REST API       │
│  (Next.js :3000) │                   │  (Axum :3001)    │
└──────────────────┘                   └────────┬─────────┘
                                                │
                                         ┌──────▼─────────┐
                                         │  MetadataStore │
                                         │   (SQLite)     │
                                         └────────────────┘
                                                │
┌──────────────────┐       gRPC         ┌──────▼─────────┐
│    Producer      │◄───────────────────┤     Agent      │
│    (Client)      │                    │   (:9090)      │
└──────────────────┘                    └────────┬───────┘
                                                 │
                                        ┌────────▼────────┐
                                        │   S3/MinIO      │
                                        │  (:9000)        │
                                        └─────────────────┘
```

## Next Steps

### Explore the API

Open Swagger UI: http://localhost:3001/swagger-ui

Try these operations:
1. List topics
2. Create a topic
3. List partitions
4. Produce messages
5. View metrics

### Build More Features

Now that the foundation is ready, you can:

1. **Add Topic Management UI** (Week 14)
   - Create topic dialog
   - Delete confirmation
   - Configure retention

2. **Agent Monitoring** (Week 15)
   - Agent detail view
   - Lease visualization
   - Performance charts

3. **Consumer Groups** (Week 16)
   - Lag monitoring
   - Offset management
   - Member list

4. **Web Console** (Week 17)
   - Browser-based producer
   - Browser-based consumer
   - Message viewer

5. **Authentication** (Week 18)
   - User login
   - JWT tokens
   - Protected routes

## Clean Up

When done testing:

```bash
# Stop services (Ctrl+C in each terminal)
# Or kill all at once:
pkill -f "cargo run.*agent"
pkill -f "cargo run.*api"
pkill -f "next-server"

# Stop MinIO
docker-compose down
```

## Summary

You now have a fully functional StreamHouse stack:
- ✅ Agent handling data storage
- ✅ REST API serving HTTP/JSON
- ✅ Web console for management
- ✅ OpenAPI documentation
- ✅ Production-ready foundation

**Total startup time**: ~30 seconds
**Services**: 4 (MinIO, Agent, API, Web)
**Ports**: 4 (9000, 9090, 3001, 3000)

Ready for Phase 10 Week 13+ features! 🚀
