# StreamHouse Quick Start Guide

## Prerequisites

1. **Docker services running**:
   ```bash
   docker-compose up -d
   ```

2. **Environment configured**:
   ```bash
   export AWS_ENDPOINT_URL=http://localhost:9000
   export AWS_ACCESS_KEY_ID=minioadmin
   export AWS_SECRET_ACCESS_KEY=minioadmin
   export AWS_ALLOW_HTTP=true
   export STREAMHOUSE_BUCKET=streamhouse-data
   export AWS_REGION=us-east-1
   ```

3. **MinIO bucket setup**:
   ```bash
   ./scripts/setup-minio.sh
   ```

## Run Complete Pipeline Demo

**One-command demo** of the entire system:

```bash
./scripts/demo-complete.sh
```

This will:
- Start StreamHouse server
- Create 3 topics (my-orders, user-events, metrics)
- Produce 90 messages
- Consume and verify data
- Show data in MinIO (60+ segments)

**Duration**: ~3 minutes

## Run Distributed Multi-Agent Demo

**See real agent coordination** with partition leases:

```bash
./scripts/demo-distributed-real.sh
```

This shows:
- 3 agents coordinating via leases
- Partition assignment across agents
- Real-time heartbeats and lease renewal
- Data in MinIO

**Duration**: ~1 minute

## Manual Testing

### 1. Start Server
```bash
# Terminal 1
cargo run --release --bin streamhouse-server
```

### 2. Create Topic
```bash
# Terminal 2
export STREAMHOUSE_ADDR=http://localhost:50051
cargo run --bin streamctl -- topic create my-topic --partitions 4
```

### 3. Produce Messages
```bash
for i in {1..100}; do
  cargo run --bin streamctl -- produce my-topic \
    --partition $((i % 4)) \
    --value "{\"id\": $i, \"data\": \"message-$i\"}"
done
```

### 4. Consume Messages
```bash
cargo run --bin streamctl -- consume my-topic --partition 0 --limit 10
```

### 5. View in MinIO
- Open http://localhost:9001
- Login: minioadmin / minioadmin
- Browse: streamhouse-data → data → my-topic

## View System Status

### Topics
```bash
cargo run --bin streamctl -- topic list
```

### Metadata Database
```bash
sqlite3 ./data/metadata.db "SELECT * FROM topics;"
sqlite3 ./data/metadata.db "SELECT * FROM partitions;"
```

### MinIO Segments
```bash
docker exec streamhouse-minio mc ls myminio/streamhouse-data/data/ --recursive
```

### Server Logs
```bash
tail -f ./data/demo-server.log
```

## URLs

| Service | URL | Credentials |
|---------|-----|-------------|
| MinIO Console | http://localhost:9001 | minioadmin / minioadmin |
| Prometheus | http://localhost:9090 | - |
| Grafana | http://localhost:3000 | admin / admin |
| PostgreSQL | localhost:5432 | streamhouse / streamhouse_dev |

## Troubleshooting

### Error: "403 Forbidden" from MinIO
```bash
./scripts/setup-minio.sh
```

### Error: Server won't start
Check logs:
```bash
tail -50 ./data/demo-server.log
```

### Error: "Offset not found"
Server using wrong storage backend. Restart with correct env vars.

### Data not in MinIO
1. Check `AWS_ENDPOINT_URL` is set
2. Restart server
3. Verify bucket permissions

## What's Next?

- Read [DISTRIBUTED_SYSTEM_WORKING.md](./DISTRIBUTED_SYSTEM_WORKING.md) for architecture details
- Check [docs/](./docs/) for phase documentation
- Run multi-agent demo for distributed coordination
- Explore Grafana dashboards
