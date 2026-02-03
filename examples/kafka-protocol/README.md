# StreamHouse Kafka Protocol Examples

StreamHouse supports the Kafka wire protocol, allowing any Kafka client to connect directly without code changes.

## Quick Start

### 1. Start StreamHouse

```bash
# Development mode (local filesystem storage)
USE_LOCAL_STORAGE=1 ./target/release/unified-server

# Production mode (S3 storage)
export AWS_REGION=us-east-1
export S3_BUCKET=my-streamhouse-bucket
./target/release/unified-server
```

Server endpoints:
- **Kafka**: `localhost:9092`
- **gRPC**: `localhost:50051`
- **REST**: `http://localhost:8080`

### 2. Connect with Any Kafka Client

```python
from kafka import KafkaProducer
producer = KafkaProducer(bootstrap_servers='localhost:9092')
producer.send('my-topic', b'Hello StreamHouse!')
```

---

## Examples by Language

| Language | File | Dependencies |
|----------|------|--------------|
| Python | [python_example.py](python_example.py) | `kafka-python` |
| Node.js | [nodejs_example.js](nodejs_example.js) | `kafkajs` |
| Go | [go_example.go](go_example.go) | `confluent-kafka-go` |
| Java | [JavaExample.java](JavaExample.java) | `kafka-clients` |
| Rust | [rust_example.rs](rust_example.rs) | `rdkafka` |

---

## Common Patterns

### Producer with Acknowledgments

```python
from kafka import KafkaProducer

producer = KafkaProducer(
    bootstrap_servers='localhost:9092',
    acks='all',  # Wait for all replicas
    retries=3,
    api_version=(2, 3)
)

# Synchronous send with confirmation
future = producer.send('orders', b'{"order_id": 123}')
result = future.get(timeout=10)
print(f"Sent to partition {result.partition} at offset {result.offset}")
```

### Consumer with Auto-Commit

```python
from kafka import KafkaConsumer

consumer = KafkaConsumer(
    'orders',
    bootstrap_servers='localhost:9092',
    group_id='order-processor',
    auto_offset_reset='earliest',
    enable_auto_commit=True,
    api_version=(2, 3)
)

for message in consumer:
    print(f"Received: {message.value}")
```

### Consumer with Manual Commit

```python
from kafka import KafkaConsumer

consumer = KafkaConsumer(
    'orders',
    bootstrap_servers='localhost:9092',
    group_id='order-processor',
    enable_auto_commit=False,
    api_version=(2, 3)
)

for message in consumer:
    process(message)
    consumer.commit()  # Commit after successful processing
```

### JSON Serialization

```python
import json
from kafka import KafkaProducer, KafkaConsumer

# Producer with JSON serialization
producer = KafkaProducer(
    bootstrap_servers='localhost:9092',
    value_serializer=lambda v: json.dumps(v).encode('utf-8'),
    key_serializer=lambda k: k.encode('utf-8') if k else None
)

producer.send('events', {'type': 'click', 'user_id': 42}, key='user-42')

# Consumer with JSON deserialization
consumer = KafkaConsumer(
    'events',
    bootstrap_servers='localhost:9092',
    value_deserializer=lambda v: json.loads(v.decode('utf-8'))
)

for msg in consumer:
    print(f"Event: {msg.value}")  # Already a dict
```

---

## Configuration Reference

### Producer Options

| Option | Description | Default |
|--------|-------------|---------|
| `bootstrap_servers` | Broker addresses | Required |
| `acks` | Acknowledgment level (`0`, `1`, `all`) | `1` |
| `retries` | Number of retries | `0` |
| `batch_size` | Batch size in bytes | `16384` |
| `linger_ms` | Time to wait for batch | `0` |
| `compression_type` | `none`, `gzip`, `lz4`, `zstd` | `none` |

### Consumer Options

| Option | Description | Default |
|--------|-------------|---------|
| `bootstrap_servers` | Broker addresses | Required |
| `group_id` | Consumer group ID | None |
| `auto_offset_reset` | `earliest`, `latest` | `latest` |
| `enable_auto_commit` | Auto-commit offsets | `True` |
| `auto_commit_interval_ms` | Commit interval | `5000` |
| `session_timeout_ms` | Session timeout | `10000` |
| `max_poll_records` | Max records per poll | `500` |

---

## Supported Kafka APIs

| API | Name | Status |
|-----|------|--------|
| 0 | Produce | Supported |
| 1 | Fetch | Supported |
| 2 | ListOffsets | Supported |
| 3 | Metadata | Supported |
| 8 | OffsetCommit | Supported |
| 9 | OffsetFetch | Supported |
| 10 | FindCoordinator | Supported |
| 11 | JoinGroup | Supported |
| 12 | Heartbeat | Supported |
| 13 | LeaveGroup | Supported |
| 14 | SyncGroup | Supported |
| 18 | ApiVersions | Supported |
| 19 | CreateTopics | Supported |
| 20 | DeleteTopics | Supported |

---

## Troubleshooting

### Connection Issues

```python
# Use 127.0.0.1 instead of localhost to avoid IPv6 issues
producer = KafkaProducer(bootstrap_servers='127.0.0.1:9092')
```

### Version Compatibility

```python
# Specify API version to avoid negotiation issues
producer = KafkaProducer(
    bootstrap_servers='localhost:9092',
    api_version=(2, 3)
)
```

### Timeout on Consume

Messages are flushed to storage every 5 seconds. If consuming immediately after producing, wait for the flush:

```python
import time

producer.send('topic', b'message')
producer.flush()
time.sleep(6)  # Wait for background flush

# Now consume
for msg in consumer:
    print(msg.value)
```

---

## Performance Tips

1. **Batch messages**: Use `batch_size` and `linger_ms` to batch multiple messages
2. **Compression**: Enable `compression_type='lz4'` for better throughput
3. **Parallel consumers**: Use multiple consumers in the same group for parallelism
4. **Avoid small messages**: Batch small events into larger payloads

---

## See Also

- [Test Suite](../../scripts/test_kafka_protocol.py) - Comprehensive test script
- [Kafka Protocol Docs](../../docs/KAFKA_PROTOCOL.md) - Protocol implementation details
