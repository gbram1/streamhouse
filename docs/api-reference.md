# API Reference

StreamHouse exposes three protocols: REST, Kafka, and gRPC. All three access the same data.

When authentication is enabled, include your API key:
- **REST**: `Authorization: Bearer sk_live_...`
- **Kafka**: SASL/PLAIN with API key as username + password
- **gRPC**: `authorization` metadata header

---

## REST API

Base URL: `http://localhost:8080`

### Topics

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/topics` | List all topics |
| `POST` | `/api/v1/topics` | Create a topic |
| `GET` | `/api/v1/topics/{name}` | Get topic details |
| `DELETE` | `/api/v1/topics/{name}` | Delete a topic |
| `GET` | `/api/v1/topics/{name}/partitions` | List partitions with offsets |

**Create topic:**
```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partition_count": 4}'
```

**Create topic with config:**
```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{
    "name": "events",
    "partition_count": 4,
    "config": {
      "retention_ms": 604800000,
      "cleanup_policy": "delete"
    }
  }'
```

### Produce

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/v1/produce` | Produce a single message |
| `POST` | `/api/v1/produce/batch` | Produce a batch of messages |

```bash
# Single
curl -X POST http://localhost:8080/api/v1/produce \
  -d '{"topic": "events", "key": "user-1", "value": "{\"action\":\"signup\"}"}'

# Batch
curl -X POST http://localhost:8080/api/v1/produce/batch \
  -d '{
    "topic": "events",
    "records": [
      {"key": "user-1", "value": "{\"action\":\"login\"}"},
      {"key": "user-2", "value": "{\"action\":\"purchase\"}"}
    ]
  }'
```

### Consume

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/consume` | Consume messages from a partition |

Query params: `topic`, `partition`, `offset`, `maxRecords` (default 100).

```bash
curl "http://localhost:8080/api/v1/consume?topic=events&partition=0&offset=0&maxRecords=50"
```

### Consumer Groups

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/consumer-groups` | List consumer groups |
| `POST` | `/api/v1/consumer-groups` | Create consumer group |
| `GET` | `/api/v1/consumer-groups/{id}` | Get group details |
| `POST` | `/api/v1/consumer-groups/commit` | Commit offsets |
| `GET` | `/api/v1/consumer-groups/{id}/offsets` | Get committed offsets |

```bash
# Commit offset
curl -X POST http://localhost:8080/api/v1/consumer-groups/commit \
  -d '{"group_id": "my-group", "topic": "events", "partition": 0, "offset": 42}'
```

### SQL

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/v1/sql` | Execute SQL query |

```bash
curl -X POST http://localhost:8080/api/v1/sql \
  -d '{"query": "SELECT * FROM events WHERE value->>\"action\" = \"signup\" LIMIT 10"}'
```

Supported: `SELECT`, `WHERE`, `GROUP BY`, `ORDER BY`, `LIMIT`, `OFFSET`, `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `SHOW TOPICS`, `DESCRIBE <topic>`, JSON operators (`->`, `->>`), and window functions (`TUMBLE()`, `HOP()`, `SESSION()`) for time-based aggregations.

### WebSocket

| Endpoint | Description |
|----------|-------------|
| `ws://localhost:8080/ws/metrics` | Real-time cluster metrics stream |
| `ws://localhost:8080/ws/topics/{name}` | Live message stream for a topic |

### Schema Registry

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/schemas/subjects` | List all subjects |
| `GET` | `/schemas/subjects/{subject}/versions` | List versions |
| `POST` | `/schemas/subjects/{subject}/versions` | Register schema |
| `GET` | `/schemas/subjects/{subject}/versions/latest` | Get latest |
| `GET` | `/schemas/subjects/{subject}/versions/{version}` | Get specific version |
| `DELETE` | `/schemas/subjects/{subject}` | Delete subject |

Supported schema types: `JSON`, `AVRO`, `PROTOBUF`.

Compatibility modes: `BACKWARD`, `FORWARD`, `FULL`, `NONE`. Set per-subject to control schema evolution.

```bash
# Register a JSON Schema
curl -X POST http://localhost:8080/schemas/subjects/events-value/versions \
  -H 'Content-Type: application/json' \
  -d '{
    "schema": "{\"type\":\"object\",\"properties\":{\"action\":{\"type\":\"string\"}},\"required\":[\"action\"]}",
    "schemaType": "JSON"
  }'

# Register an Avro schema
curl -X POST http://localhost:8080/schemas/subjects/orders-value/versions \
  -H 'Content-Type: application/json' \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"id\",\"type\":\"string\"},{\"name\":\"amount\",\"type\":\"double\"}]}",
    "schemaType": "AVRO"
  }'
```

### Metrics

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/metrics` | General metrics |
| `GET` | `/api/v1/metrics/storage` | Storage metrics per topic |
| `GET` | `/api/v1/metrics/throughput` | Produce/consume rates |
| `GET` | `/api/v1/metrics/latency` | p50/p95/p99 latency |
| `GET` | `/api/v1/metrics/errors` | Error counts |
| `GET` | `/metrics` | Prometheus scrape endpoint |

All metrics endpoints are org-scoped when auth is enabled.

### Organizations & API Keys

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/organizations` | List organizations |
| `POST` | `/api/v1/organizations` | Create organization |
| `POST` | `/api/v1/organizations/{id}/api-keys` | Create API key |
| `GET` | `/api/v1/organizations/{id}/api-keys` | List API keys |
| `DELETE` | `/api/v1/organizations/{id}/api-keys/{key_id}` | Revoke key |

### Connectors

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/connectors` | List connectors |
| `POST` | `/api/v1/connectors` | Create connector |
| `GET` | `/api/v1/connectors/{name}` | Get connector |
| `PUT` | `/api/v1/connectors/{name}` | Update connector |
| `DELETE` | `/api/v1/connectors/{name}` | Delete connector |
| `POST` | `/api/v1/connectors/{name}/restart` | Restart connector |
| `POST` | `/api/v1/connectors/{name}/pause` | Pause connector |
| `POST` | `/api/v1/connectors/{name}/resume` | Resume connector |

**Connector types:**

| Type | Direction | Description |
|------|-----------|-------------|
| `debezium` | Source | CDC from PostgreSQL/MySQL via Debezium |
| `kafka` | Source | Read from an external Kafka cluster |
| `s3` | Sink | Write to S3 (Parquet, JSON, or CSV format) |
| `postgres` | Sink | Write to PostgreSQL tables |
| `elasticsearch` | Sink | Index into Elasticsearch |

### Agents (Admin)

Requires `X-Admin-Key` header matching `STREAMHOUSE_ADMIN_KEY` env var.

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/agents` | List agents |
| `GET` | `/api/v1/agents/{id}` | Get agent details |
| `GET` | `/api/v1/agents/{id}/metrics` | Agent metrics |

### Other

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/api/v1/query/ask` | AI-powered natural language query |
| `POST` | `/api/v1/query/estimate` | Query cost estimation |

### Error Codes

| Status | Meaning |
|--------|---------|
| `200` | Success |
| `201` | Created |
| `400` | Invalid input |
| `401` | Unauthorized (missing or invalid API key) |
| `403` | Forbidden (insufficient permissions) |
| `404` | Not found |
| `409` | Already exists |
| `422` | Schema validation failure |
| `500` | Server error |

---

## Kafka Protocol

Port: `9092` (default)

StreamHouse implements the Kafka wire protocol — use any Kafka client without code changes.

### 23 Supported APIs

| API Key | Name | Description |
|---------|------|-------------|
| 0 | Produce | Produce messages (v3, v7, v9) |
| 1 | Fetch | Consume messages (v4, v11) |
| 2 | ListOffsets | Get earliest/latest offsets |
| 3 | Metadata | Cluster and topic metadata |
| 8 | OffsetCommit | Commit consumer group offsets |
| 9 | OffsetFetch | Fetch committed offsets |
| 10 | FindCoordinator | Find group coordinator |
| 11 | JoinGroup | Join consumer group |
| 12 | Heartbeat | Consumer group heartbeat |
| 13 | LeaveGroup | Leave consumer group |
| 14 | SyncGroup | Sync consumer group state |
| 15 | DescribeGroups | Get group details |
| 16 | ListGroups | List all groups |
| 17 | SaslHandshake | SASL mechanism negotiation |
| 18 | ApiVersions | List supported API versions |
| 19 | CreateTopics | Create topics |
| 20 | DeleteTopics | Delete topics |
| 22 | InitProducerId | Initialize idempotent/transactional producer |
| 24 | AddPartitionsToTxn | Add partitions to transaction |
| 25 | AddOffsetsToTxn | Add offsets to transaction |
| 26 | EndTxn | Commit or abort transaction |
| 28 | TxnOffsetCommit | Commit offsets within transaction |
| 36 | SaslAuthenticate | SASL/PLAIN authentication |

### Authentication

```python
# confluent-kafka
producer = Producer({
    'bootstrap.servers': 'localhost:9092',
    'security.protocol': 'SASL_PLAINTEXT',
    'sasl.mechanism': 'PLAIN',
    'sasl.username': 'sk_live_abc123...',
    'sasl.password': 'sk_live_abc123...',
})
```

### Transactions

```python
producer = Producer({
    'bootstrap.servers': 'localhost:9092',
    'transactional.id': 'my-txn-producer',
})
producer.init_transactions()
producer.begin_transaction()
producer.produce('events', value=b'...')
producer.commit_transaction()
```

### Consumer Groups

Full consumer group rebalancing is supported:

```python
consumer = Consumer({
    'bootstrap.servers': 'localhost:9092',
    'group.id': 'my-group',
    'auto.offset.reset': 'earliest',
})
consumer.subscribe(['events'])
```

### Compatibility Notes

- **Compression**: None, GZIP, Snappy, LZ4 on the wire (internally stores as LZ4 or Zstd)
- **Record format**: Translates Kafka RecordBatch on the wire to StreamHouse's STRM segment format internally
- **Consumer rebalancing**: Fully implemented
- **Idempotent producer**: Supported via InitProducerId
- **Transactions**: Full exactly-once semantics (InitProducerId, AddPartitionsToTxn, AddOffsetsToTxn, EndTxn, TxnOffsetCommit)

### Not Yet Supported

| API Key | Name | Notes |
|---------|------|-------|
| 32/33/44 | DescribeConfigs / AlterConfigs | Needed for admin tools |
| 37 | CreatePartitions | Partition scaling |
| 42 | DeleteGroups | Group cleanup |
| 29-31 | ACL APIs | Blocked on ACL implementation |

---

## gRPC API

Service: `streamhouse.StreamHouse`
Port: `50051` (default)

### Methods

```protobuf
// Topics
rpc CreateTopic(CreateTopicRequest) returns (CreateTopicResponse);
rpc GetTopic(GetTopicRequest) returns (GetTopicResponse);
rpc ListTopics(ListTopicsRequest) returns (ListTopicsResponse);
rpc DeleteTopic(DeleteTopicRequest) returns (DeleteTopicResponse);

// Produce
rpc Produce(ProduceRequest) returns (ProduceResponse);
rpc ProduceBatch(ProduceBatchRequest) returns (ProduceBatchResponse);

// Consume
rpc Consume(ConsumeRequest) returns (ConsumeResponse);
rpc CommitOffset(CommitOffsetRequest) returns (CommitOffsetResponse);
rpc GetOffset(GetOffsetRequest) returns (GetOffsetResponse);

// Producer lifecycle & transactions
rpc InitProducer(InitProducerRequest) returns (InitProducerResponse);
rpc BeginTransaction(BeginTransactionRequest) returns (BeginTransactionResponse);
rpc CommitTransaction(CommitTransactionRequest) returns (CommitTransactionResponse);
rpc AbortTransaction(AbortTransactionRequest) returns (AbortTransactionResponse);
rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
```

### AckMode

| Mode | Behavior | Use case |
|------|----------|----------|
| `ACK_NONE` | Fire-and-forget | Metrics, logs where loss is acceptable |
| `ACK_BUFFERED` | Acked when buffered in RAM (default) | General use |
| `ACK_DURABLE` | Acked when flushed to S3 | Financial data, critical events |

### Example (Rust)

```rust
let mut client = StreamHouseClient::connect("http://localhost:50051").await?;

let request = ProduceBatchRequest {
    topic: "events".to_string(),
    records: vec![ProduceRecord {
        key: Some("user-1".to_string()),
        value: b"{\"action\":\"signup\"}".to_vec(),
        ..Default::default()
    }],
    ack_mode: AckMode::AckDurable as i32,
    ..Default::default()
};

let response = client.produce_batch(request).await?;
```

### Example (Python)

```python
import grpc
from streamhouse_pb2 import ProduceBatchRequest, ProduceRecord, ACK_DURABLE
from streamhouse_pb2_grpc import StreamHouseStub

channel = grpc.insecure_channel('localhost:50051')
client = StreamHouseStub(channel)

request = ProduceBatchRequest(
    topic='events',
    records=[ProduceRecord(key='user-1', value=b'{"action":"signup"}')],
    ack_mode=ACK_DURABLE,
)
response = client.ProduceBatch(request)
```
