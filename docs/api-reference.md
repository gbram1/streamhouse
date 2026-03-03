# API Reference

StreamHouse exposes three wire protocols: REST, gRPC, and Kafka protocol.

## REST API

Base URL: `http://localhost:8080`

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/v1/topics` | GET | List all topics |
| `/api/v1/topics` | POST | Create a topic |
| `/api/v1/topics/{name}` | GET | Get topic details |
| `/api/v1/topics/{name}` | DELETE | Delete a topic |
| `/api/v1/produce` | POST | Produce a single message |
| `/api/v1/produce/batch` | POST | Produce a batch of messages |
| `/api/v1/consume` | GET | Consume messages from a partition |
| `/api/v1/consumer-groups` | GET | List consumer groups |
| `/api/v1/consumer-groups` | POST | Create consumer group |
| `/api/v1/sql` | POST | Execute SQL query over streams |
| `/schemas/subjects` | GET | List schema subjects |
| `/schemas/subjects/{subject}/versions` | GET | List schema versions |
| `/schemas/subjects/{subject}/versions` | POST | Register a new schema |
| `/schemas/subjects/{subject}/versions/latest` | GET | Get latest schema |
| `/schemas/subjects/{subject}/versions/{version}` | GET | Get specific schema version |

### Examples

#### Health Check

```bash
curl http://localhost:8080/health
```

**Response:**
```json
{"status": "ok"}
```

#### Create Topic

```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 4}'
```

**Response:**
```json
{
  "name": "events",
  "partitions": 4,
  "replication_factor": 1
}
```

#### Produce Message

```bash
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "key": "user-123",
    "value": "{\"event\": \"signup\", \"user\": \"alice\"}"
  }'
```

**Response:**
```json
{
  "topic": "events",
  "partition": 2,
  "offset": 42
}
```

#### Produce Batch

```bash
curl -X POST http://localhost:8080/api/v1/produce/batch \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "events",
    "records": [
      {"key": "user-1", "value": "{\"event\": \"login\"}"},
      {"key": "user-2", "value": "{\"event\": \"logout\"}"}
    ]
  }'
```

**Response:**
```json
{
  "results": [
    {"partition": 0, "offset": 10},
    {"partition": 1, "offset": 5}
  ]
}
```

#### Consume Messages

```bash
curl "http://localhost:8080/api/v1/consume?topic=events&partition=0&offset=0&limit=10"
```

**Response:**
```json
{
  "topic": "events",
  "partition": 0,
  "records": [
    {
      "offset": 0,
      "key": "user-123",
      "value": "{\"event\": \"signup\", \"user\": \"alice\"}",
      "timestamp": 1678886400000
    }
  ]
}
```

#### List Topics

```bash
curl http://localhost:8080/api/v1/topics
```

**Response:**
```json
{
  "topics": [
    {"name": "events", "partitions": 4},
    {"name": "orders", "partitions": 8}
  ]
}
```

#### Get Topic

```bash
curl http://localhost:8080/api/v1/topics/events
```

**Response:**
```json
{
  "name": "events",
  "partitions": 4,
  "replication_factor": 1,
  "config": {
    "retention_ms": 604800000,
    "segment_max_size": 1048576
  }
}
```

#### Delete Topic

```bash
curl -X DELETE http://localhost:8080/api/v1/topics/events
```

**Response:**
```json
{"status": "deleted"}
```

#### SQL Query

```bash
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM events WHERE value->>\"event\" = \"signup\" LIMIT 5"}'
```

**Response:**
```json
{
  "columns": ["offset", "key", "value", "timestamp"],
  "rows": [
    [0, "user-123", "{\"event\":\"signup\",\"user\":\"alice\"}", 1678886400000]
  ]
}
```

#### Register Schema

```bash
curl -X POST http://localhost:8080/schemas/subjects/events-value/versions \
  -H 'Content-Type: application/json' \
  -d '{
    "schema": "{\"type\":\"object\",\"properties\":{\"event\":{\"type\":\"string\"},\"user\":{\"type\":\"string\"}},\"required\":[\"event\",\"user\"]}",
    "schemaType": "JSON"
  }'
```

**Response:**
```json
{"id": 1}
```

#### Get Latest Schema

```bash
curl http://localhost:8080/schemas/subjects/events-value/versions/latest
```

**Response:**
```json
{
  "subject": "events-value",
  "version": 1,
  "id": 1,
  "schema": "{\"type\":\"object\",\"properties\":{\"event\":{\"type\":\"string\"},\"user\":{\"type\":\"string\"}},\"required\":[\"event\",\"user\"]}",
  "schemaType": "JSON"
}
```

## gRPC API

Service: `streamhouse.StreamHouse`
Port: `50051` (default)

### Service Methods

#### Topics

```protobuf
rpc CreateTopic(CreateTopicRequest) returns (CreateTopicResponse);
rpc GetTopic(GetTopicRequest) returns (GetTopicResponse);
rpc ListTopics(ListTopicsRequest) returns (ListTopicsResponse);
rpc DeleteTopic(DeleteTopicRequest) returns (DeleteTopicResponse);
```

#### Produce

```protobuf
rpc Produce(ProduceRequest) returns (ProduceResponse);
rpc ProduceBatch(ProduceBatchRequest) returns (ProduceBatchResponse);
```

**AckMode options:**
- `ACK_NONE` — Fire-and-forget, no durability guarantee
- `ACK_BUFFERED` — Acknowledged when written to in-memory buffer (default)
- `ACK_DURABLE` — Acknowledged when flushed to S3 (synchronous durability)

When using `ACK_DURABLE`, the RPC blocks until the segment is uploaded to S3. Your data is durable when the RPC returns.

#### Consume

```protobuf
rpc Consume(ConsumeRequest) returns (ConsumeResponse);
rpc CommitOffset(CommitOffsetRequest) returns (CommitOffsetResponse);
rpc GetOffset(GetOffsetRequest) returns (GetOffsetResponse);
```

#### Producer Lifecycle

```protobuf
rpc InitProducer(InitProducerRequest) returns (InitProducerResponse);
rpc BeginTransaction(BeginTransactionRequest) returns (BeginTransactionResponse);
rpc CommitTransaction(CommitTransactionRequest) returns (CommitTransactionResponse);
rpc AbortTransaction(AbortTransactionRequest) returns (AbortTransactionResponse);
rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
```

### Example (Rust gRPC Client)

```rust
use streamhouse_proto::streamhouse::{
    stream_house_client::StreamHouseClient,
    ProduceBatchRequest, ProduceRecord, AckMode,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = StreamHouseClient::connect("http://localhost:50051").await?;

    let request = ProduceBatchRequest {
        topic: "events".to_string(),
        records: vec![
            ProduceRecord {
                key: Some("user-1".to_string()),
                value: r#"{"event":"signup"}"#.as_bytes().to_vec(),
                ..Default::default()
            },
        ],
        ack_mode: AckMode::AckDurable as i32,
        ..Default::default()
    };

    let response = client.produce_batch(request).await?;
    println!("Produced: {:?}", response.into_inner());

    Ok(())
}
```

### Example (Python gRPC Client)

```python
import grpc
from streamhouse_pb2 import ProduceBatchRequest, ProduceRecord, ACK_DURABLE
from streamhouse_pb2_grpc import StreamHouseStub

channel = grpc.insecure_channel('localhost:50051')
client = StreamHouseStub(channel)

request = ProduceBatchRequest(
    topic='events',
    records=[
        ProduceRecord(key='user-1', value=b'{"event":"signup"}')
    ],
    ack_mode=ACK_DURABLE
)

response = client.ProduceBatch(request)
print(f"Produced: {response}")
```

## Kafka Protocol

Port: `9092` (default)

StreamHouse implements the Kafka wire protocol for compatibility with existing Kafka clients and tools.

### Supported APIs

StreamHouse implements **14 Kafka APIs**:

| API | Version | Description |
|-----|---------|-------------|
| `Produce` | v3, v7, v9 | Produce messages |
| `Fetch` | v4, v11 | Fetch messages (consume) |
| `ListOffsets` | v1 | Get earliest/latest offsets |
| `Metadata` | v1, v9 | Cluster and topic metadata |
| `OffsetCommit` | v2 | Commit consumer group offsets |
| `OffsetFetch` | v1 | Fetch consumer group offsets |
| `FindCoordinator` | v0, v1 | Find group coordinator |
| `JoinGroup` | v0, v5 | Join consumer group |
| `Heartbeat` | v0, v3 | Consumer group heartbeat |
| `LeaveGroup` | v0, v3 | Leave consumer group |
| `SyncGroup` | v0, v3 | Sync consumer group state |
| `DescribeGroups` | v0 | Get consumer group details |
| `ListGroups` | v0 | List all consumer groups |
| `ApiVersions` | v0, v3 | List supported API versions |

### Example (kafka-python)

```python
from kafka import KafkaProducer, KafkaConsumer

# Produce
producer = KafkaProducer(bootstrap_servers='localhost:9092')
producer.send('events', key=b'user-1', value=b'{"event":"signup"}')
producer.flush()

# Consume
consumer = KafkaConsumer(
    'events',
    bootstrap_servers='localhost:9092',
    auto_offset_reset='earliest',
    group_id='my-group'
)

for message in consumer:
    print(f"offset={message.offset} key={message.key} value={message.value}")
```

### Example (kafkacat / kcat)

```bash
# Produce
echo '{"event":"signup","user":"alice"}' | kcat -P -b localhost:9092 -t events -k user-1

# Consume
kcat -C -b localhost:9092 -t events -o beginning
```

### Example (Kafka CLI Tools)

```bash
# Produce
kafka-console-producer.sh --broker-list localhost:9092 --topic events

# Consume
kafka-console-consumer.sh --bootstrap-server localhost:9092 --topic events --from-beginning

# List topics
kafka-topics.sh --list --bootstrap-server localhost:9092
```

### Compatibility Notes

- **Idempotent producer** and **transactions** are supported via producer lifecycle RPCs
- **Consumer group rebalancing** is fully implemented (sticky assignor)
- **Compression codecs**: None, GZIP, Snappy, LZ4 (StreamHouse internally uses LZ4 for segment compression)
- **Record batching**: StreamHouse uses custom segment format internally but translates to/from Kafka's RecordBatch format on the wire

## Authentication & Authorization

**Current status:** StreamHouse does not yet implement authentication or ACLs. All endpoints are unauthenticated.

**Planned:**
- SASL/SCRAM for Kafka protocol
- API keys for REST/gRPC
- Organization-scoped access control (currently in progress)

## Rate Limiting

StreamHouse has a built-in circuit breaker for S3 uploads (`THROTTLE_ENABLED=true` by default). On 503/429 errors from S3, it backs off exponentially.

gRPC backpressure can be configured with `GRPC_MAX_CONCURRENT_STREAMS`.

## Error Codes

### REST API

Standard HTTP status codes:

- `200 OK` — Success
- `201 Created` — Resource created (topic, schema)
- `400 Bad Request` — Invalid input (e.g., missing required field)
- `404 Not Found` — Topic or resource not found
- `409 Conflict` — Resource already exists (e.g., topic name collision)
- `422 Unprocessable Entity` — Schema validation failure
- `500 Internal Server Error` — Server-side error (check logs)

### gRPC API

Standard gRPC status codes:

- `OK` — Success
- `INVALID_ARGUMENT` — Invalid input
- `NOT_FOUND` — Topic or partition not found
- `ALREADY_EXISTS` — Resource already exists
- `FAILED_PRECONDITION` — Schema validation failure
- `UNAVAILABLE` — Service unavailable (transient, retry)
- `INTERNAL` — Server-side error (check logs)

### Kafka Protocol

Kafka error codes (returned in protocol responses):

- `NONE (0)` — Success
- `OFFSET_OUT_OF_RANGE (1)` — Requested offset does not exist
- `UNKNOWN_TOPIC_OR_PARTITION (3)` — Topic or partition not found
- `INVALID_REQUEST (42)` — Malformed request
- `UNSUPPORTED_VERSION (35)` — API version not supported
