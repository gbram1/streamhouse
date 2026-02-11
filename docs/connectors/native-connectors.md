# Native StreamHouse Connectors

StreamHouse includes built-in sink connectors for common data destinations. These run as part of the StreamHouse process and are managed via the REST API.

## S3 Sink

Streams records from StreamHouse topics to S3 in Parquet, Avro, or JSON format.

### Configuration

```json
{
  "name": "orders-to-s3",
  "connectorType": "sink",
  "connectorClass": "s3-sink",
  "topics": ["orders", "payments"],
  "config": {
    "s3.bucket": "my-data-lake",
    "s3.prefix": "raw/streaming",
    "s3.region": "us-east-1",
    "format": "parquet",
    "batch.size": "10000",
    "batch.interval_ms": "60000",
    "partitioning": "hourly"
  }
}
```

### Create via REST API

```bash
curl -X POST http://localhost:8080/api/v1/connectors \
  -H "Content-Type: application/json" \
  -d @connector-config.json
```

### Config Options

| Key | Default | Description |
|-----|---------|-------------|
| `s3.bucket` | (required) | S3 bucket name |
| `s3.prefix` | `""` | Key prefix for all files |
| `s3.region` | `us-east-1` | AWS region |
| `format` | `json` | Output format: `json`, `parquet`, `avro` |
| `batch.size` | `10000` | Records per output file |
| `batch.interval_ms` | `60000` | Max ms before flushing partial batch |
| `partitioning` | `none` | S3 key partitioning: `none`, `hourly`, `daily` |

### Output Structure

With `partitioning=hourly`:
```
s3://my-data-lake/raw/streaming/orders/year=2026/month=02/day=10/hour=14/batch-uuid.parquet
```

With `partitioning=none`:
```
s3://my-data-lake/raw/streaming/orders/batch-uuid.parquet
```

## PostgreSQL Sink

Streams records from StreamHouse topics to a PostgreSQL table.

### Configuration

```json
{
  "name": "orders-to-postgres",
  "connectorType": "sink",
  "connectorClass": "postgres-sink",
  "topics": ["orders"],
  "config": {
    "connection.url": "postgres://user:pass@localhost:5432/analytics",
    "table.name": "orders",
    "insert.mode": "upsert",
    "pk.fields": "order_id",
    "batch.size": "500"
  }
}
```

### Config Options

| Key | Default | Description |
|-----|---------|-------------|
| `connection.url` | (required) | PostgreSQL connection string |
| `table.name` | (required) | Target table name |
| `insert.mode` | `upsert` | `insert` or `upsert` |
| `pk.fields` | `""` | Comma-separated PK fields (required for upsert) |
| `batch.size` | `500` | Records per batch INSERT |
| `value.format` | `json` | Record value format (currently only `json`) |

### How It Works

1. Consumes records from the configured topic(s)
2. Parses each record's value as JSON
3. Maps JSON fields to table columns
4. Executes batch INSERT or upsert (INSERT ... ON CONFLICT DO UPDATE)
5. Commits consumer offsets after successful write

## Elasticsearch Sink

Streams records from StreamHouse topics to Elasticsearch for full-text search and analytics.

### Configuration

```json
{
  "name": "logs-to-elasticsearch",
  "connectorType": "sink",
  "connectorClass": "elasticsearch-sink",
  "topics": ["app-logs", "access-logs"],
  "config": {
    "connection.url": "http://localhost:9200",
    "index.name": "logs-{topic}",
    "batch.size": "1000",
    "batch.interval_ms": "5000"
  }
}
```

### Config Options

| Key | Default | Description |
|-----|---------|-------------|
| `connection.url` | `http://localhost:9200` | Elasticsearch URL |
| `index.name` | (required) | Target index (supports `{topic}` template) |
| `document.id` | (auto) | JSON field to use as `_id` |
| `batch.size` | `1000` | Documents per bulk request |
| `batch.interval_ms` | `5000` | Max ms before flushing partial batch |

### Index Templates

The `{topic}` placeholder in `index.name` is replaced with the source topic name. For example, with topics `["app-logs", "access-logs"]` and `index.name=logs-{topic}`:
- Records from `app-logs` go to index `logs-app-logs`
- Records from `access-logs` go to index `logs-access-logs`

## Managing Connectors

### List all connectors

```bash
curl http://localhost:8080/api/v1/connectors
```

### Get connector status

```bash
curl http://localhost:8080/api/v1/connectors/orders-to-s3
```

### Pause a connector

```bash
curl -X POST http://localhost:8080/api/v1/connectors/orders-to-s3/pause
```

### Resume a connector

```bash
curl -X POST http://localhost:8080/api/v1/connectors/orders-to-s3/resume
```

### Delete a connector

```bash
curl -X DELETE http://localhost:8080/api/v1/connectors/orders-to-s3
```

## Monitoring

Connector metrics are available via the StreamHouse metrics endpoint:

```bash
curl http://localhost:8080/api/v1/metrics
```

Key metrics:
- `connector_records_processed` — total records processed per connector
- `connector_state` — current state (running/paused/stopped/failed)
- `connector_errors_total` — error count per connector
