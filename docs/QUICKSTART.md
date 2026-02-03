# StreamHouse Quickstart Guide

Get StreamHouse running in under 5 minutes with Docker Compose.

## Prerequisites

- Docker and Docker Compose v2+
- 4GB RAM minimum
- curl (for testing)

## Quick Start

```bash
# Clone the repository
git clone https://github.com/streamhouse/streamhouse
cd streamhouse

# Start all services
docker compose up -d

# Wait for services to be healthy (about 30 seconds)
docker compose ps

# Verify StreamHouse is running
curl http://localhost:8080/health
# Expected: {"status":"healthy"}
```

## What's Running

After startup, you'll have:

| Service | URL | Description |
|---------|-----|-------------|
| **StreamHouse API** | http://localhost:8080 | REST API + gRPC |
| **Web UI** | http://localhost:3000 | Dashboard & management |
| **Schema Registry** | http://localhost:8080/schemas | Avro/Protobuf/JSON schemas |
| **Prometheus** | http://localhost:9091 | Metrics collection |
| **Grafana** | http://localhost:3001 | Dashboards (admin/admin) |
| **MinIO Console** | http://localhost:9001 | S3 storage (minioadmin/minioadmin) |

## Create Your First Topic

```bash
# Create a topic with 3 partitions
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 3, "replication_factor": 1}'

# Verify topic was created
curl http://localhost:8080/api/v1/topics
```

## Produce Messages

```bash
# Send a single message
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "key": "user-123",
    "value": "{\"event\": \"page_view\", \"url\": \"/home\"}"
  }'

# Send a batch of messages
curl -X POST http://localhost:8080/api/v1/produce/batch \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "records": [
      {"key": "user-1", "value": "{\"event\": \"click\", \"button\": \"signup\"}"},
      {"key": "user-2", "value": "{\"event\": \"click\", \"button\": \"login\"}"},
      {"key": "user-1", "value": "{\"event\": \"purchase\", \"amount\": 99.99}"}
    ]
  }'
```

## Consume Messages

```bash
# Consume from the beginning of a partition
curl "http://localhost:8080/api/v1/consume?topic=events&partition=0&offset=0&limit=10"
```

## Register a Schema

```bash
# Register an Avro schema for events
curl -X POST http://localhost:8080/schemas/subjects/events-value/versions \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Event\",\"fields\":[{\"name\":\"event\",\"type\":\"string\"},{\"name\":\"timestamp\",\"type\":\"long\"}]}",
    "schemaType": "AVRO"
  }'

# List all schemas
curl http://localhost:8080/schemas/subjects
```

## Create a Consumer Group

```bash
# Create a consumer group and commit offsets
curl -X POST http://localhost:8080/api/v1/consumer-groups \
  -H "Content-Type: application/json" \
  -d '{"groupId": "my-analytics-group"}'

# Commit offsets for the group
curl -X POST "http://localhost:8080/api/v1/consumer-groups/my-analytics-group/offsets" \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "partition": 0,
    "offset": 5
  }'

# Check consumer lag
curl http://localhost:8080/api/v1/consumer-groups/my-analytics-group/lag
```

## High-Performance Rust Client

For production workloads, use the native Rust client for 160K+ msg/sec:

```bash
# Run the load test example
cargo run -p streamhouse-client --example load_test --release
```

See [examples/](../examples/) for more examples.

## Explore the Web UI

Open http://localhost:3000 in your browser to:

- View real-time cluster metrics
- Browse topics and messages
- Monitor consumer group lag
- Manage schemas
- View agent health

## View Metrics in Grafana

1. Open http://localhost:3001
2. Login with `admin` / `admin`
3. Navigate to Dashboards → StreamHouse Overview
4. See real-time metrics for:
   - Message throughput
   - S3 request latency
   - Segment writes
   - Cache hit rates

## Stop Services

```bash
# Stop all services
docker compose down

# Stop and remove all data (clean slate)
docker compose down -v
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | (required) | PostgreSQL connection string |
| `S3_ENDPOINT` | (required) | S3/MinIO endpoint URL |
| `S3_BUCKET` | `streamhouse` | Bucket for segment storage |
| `AWS_ACCESS_KEY_ID` | (required) | S3 access key |
| `AWS_SECRET_ACCESS_KEY` | (required) | S3 secret key |
| `HTTP_PORT` | `8080` | REST API port |
| `GRPC_PORT` | `9090` | gRPC port |
| `WAL_ENABLED` | `true` | Enable write-ahead log |
| `RUST_LOG` | `info` | Log level |

### Production Considerations

For production deployments, see [PRODUCTION_DEPLOYMENT.md](PRODUCTION_DEPLOYMENT.md) for:
- Hardware sizing recommendations
- Security configuration (TLS, authentication)
- Backup and disaster recovery
- Monitoring and alerting setup

## Troubleshooting

### Services not starting?

```bash
# Check service logs
docker compose logs streamhouse
docker compose logs postgres

# Restart services
docker compose restart
```

### Database connection issues?

```bash
# Reset PostgreSQL (loses all data)
docker compose down -v
docker compose up -d
```

### Port conflicts?

Edit `docker-compose.yml` to change port mappings:
```yaml
ports:
  - "8081:8080"  # Change external port
```

## Next Steps

- [Production Deployment Guide](PRODUCTION_DEPLOYMENT.md)
- [Rust Client Documentation](../crates/streamhouse-client/README.md)
- [API Reference](API_REFERENCE.md)
- [Architecture Overview](ARCHITECTURE_OVERVIEW.md)

## Support

- GitHub Issues: https://github.com/streamhouse/streamhouse/issues
- Documentation: https://streamhouse.dev/docs
