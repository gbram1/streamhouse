# Kafka Protocol Compatibility

StreamHouse supports the native Kafka binary protocol, allowing you to use any standard Kafka client to connect directly on port **9092**.

## Quick Start

### Python (kafka-python)

```bash
pip install kafka-python
```

```python
from kafka import KafkaProducer, KafkaConsumer

# Produce messages
producer = KafkaProducer(bootstrap_servers='localhost:9092')
producer.send('my-topic', b'Hello StreamHouse!')
producer.send('my-topic', key=b'user-123', value=b'{"event": "click"}')
producer.close()

# Consume messages (note: Fetch is currently stubbed)
consumer = KafkaConsumer(
    'my-topic',
    bootstrap_servers='localhost:9092',
    auto_offset_reset='earliest',
    consumer_timeout_ms=10000
)
for msg in consumer:
    print(f'{msg.key}: {msg.value}')
```

### Node.js (kafkajs)

```bash
npm install kafkajs
```

```javascript
const { Kafka } = require('kafkajs');

const kafka = new Kafka({
  clientId: 'my-app',
  brokers: ['localhost:9092']
});

// Produce
const producer = kafka.producer();
await producer.connect();
await producer.send({
  topic: 'my-topic',
  messages: [
    { key: 'user-123', value: JSON.stringify({ event: 'click' }) }
  ]
});
await producer.disconnect();

// Consume
const consumer = kafka.consumer({ groupId: 'my-group' });
await consumer.connect();
await consumer.subscribe({ topic: 'my-topic', fromBeginning: true });
await consumer.run({
  eachMessage: async ({ topic, partition, message }) => {
    console.log(`${message.key}: ${message.value}`);
  }
});
```

### Go (confluent-kafka-go)

```bash
go get github.com/confluentinc/confluent-kafka-go/kafka
```

```go
package main

import (
    "fmt"
    "github.com/confluentinc/confluent-kafka-go/kafka"
)

func main() {
    // Produce
    p, _ := kafka.NewProducer(&kafka.ConfigMap{
        "bootstrap.servers": "localhost:9092",
    })

    topic := "my-topic"
    p.Produce(&kafka.Message{
        TopicPartition: kafka.TopicPartition{Topic: &topic, Partition: kafka.PartitionAny},
        Key:            []byte("user-123"),
        Value:          []byte(`{"event": "click"}`),
    }, nil)
    p.Flush(5000)
    p.Close()

    // Consume
    c, _ := kafka.NewConsumer(&kafka.ConfigMap{
        "bootstrap.servers": "localhost:9092",
        "group.id":          "my-group",
        "auto.offset.reset": "earliest",
    })
    c.Subscribe("my-topic", nil)

    for {
        msg, err := c.ReadMessage(-1)
        if err == nil {
            fmt.Printf("%s: %s\n", string(msg.Key), string(msg.Value))
        }
    }
}
```

### Java (kafka-clients)

```xml
<dependency>
    <groupId>org.apache.kafka</groupId>
    <artifactId>kafka-clients</artifactId>
    <version>3.6.0</version>
</dependency>
```

```java
import org.apache.kafka.clients.producer.*;
import org.apache.kafka.clients.consumer.*;
import java.util.*;

public class Example {
    public static void main(String[] args) {
        // Produce
        Properties producerProps = new Properties();
        producerProps.put("bootstrap.servers", "localhost:9092");
        producerProps.put("key.serializer", "org.apache.kafka.common.serialization.StringSerializer");
        producerProps.put("value.serializer", "org.apache.kafka.common.serialization.StringSerializer");

        KafkaProducer<String, String> producer = new KafkaProducer<>(producerProps);
        producer.send(new ProducerRecord<>("my-topic", "user-123", "{\"event\": \"click\"}"));
        producer.close();

        // Consume
        Properties consumerProps = new Properties();
        consumerProps.put("bootstrap.servers", "localhost:9092");
        consumerProps.put("group.id", "my-group");
        consumerProps.put("auto.offset.reset", "earliest");
        consumerProps.put("key.deserializer", "org.apache.kafka.common.serialization.StringDeserializer");
        consumerProps.put("value.deserializer", "org.apache.kafka.common.serialization.StringDeserializer");

        KafkaConsumer<String, String> consumer = new KafkaConsumer<>(consumerProps);
        consumer.subscribe(Arrays.asList("my-topic"));

        while (true) {
            ConsumerRecords<String, String> records = consumer.poll(Duration.ofMillis(100));
            for (ConsumerRecord<String, String> record : records) {
                System.out.printf("%s: %s%n", record.key(), record.value());
            }
        }
    }
}
```

## Server Configuration

```bash
# Default configuration (Kafka on port 9092)
cargo run -p streamhouse-server --bin unified-server

# Custom Kafka port
export KAFKA_ADDR=0.0.0.0:9094
export KAFKA_ADVERTISED_HOST=my-server.example.com
export KAFKA_ADVERTISED_PORT=9094

# Start with local storage (development)
export USE_LOCAL_STORAGE=1
cargo run -p streamhouse-server --bin unified-server
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KAFKA_ADDR` | `0.0.0.0:9092` | Kafka protocol bind address |
| `KAFKA_ADVERTISED_HOST` | `localhost` | Hostname returned to clients |
| `KAFKA_ADVERTISED_PORT` | `9092` | Port returned to clients |

## Supported APIs

| API Key | Name | Status | Notes |
|---------|------|--------|-------|
| 0 | Produce | ✅ Working | RecordBatch v2 format |
| 1 | Fetch | ⚠️ Stubbed | Returns empty (needs storage integration) |
| 2 | ListOffsets | ✅ Working | |
| 3 | Metadata | ✅ Working | v0-v12 |
| 8 | OffsetCommit | ✅ Working | |
| 9 | OffsetFetch | ✅ Working | |
| 10 | FindCoordinator | ✅ Working | |
| 11 | JoinGroup | ✅ Working | |
| 12 | Heartbeat | ✅ Working | |
| 13 | LeaveGroup | ✅ Working | |
| 14 | SyncGroup | ✅ Working | |
| 15 | DescribeGroups | ✅ Working | |
| 16 | ListGroups | ✅ Working | |
| 18 | ApiVersions | ✅ Working | v0-v3 |
| 19 | CreateTopics | ✅ Working | |
| 20 | DeleteTopics | ✅ Working | |

## Known Limitations

### 1. Fetch API (Consuming)
The Fetch API currently returns empty record batches. Messages are stored but cannot be consumed via Kafka protocol yet. This will be fixed when PartitionReader integration is complete.

**Workaround**: Use the REST API or gRPC to consume messages.

### 2. Message Format
Only **RecordBatch v2** format (Kafka 0.11+) is supported. Older Message Set formats are not supported.

**Solution**: Use clients configured for Kafka 0.11+ (most modern clients default to this).

### 3. kcat/librdkafka
Some versions of `kcat` (formerly `kafkacat`) may crash due to compact protocol encoding issues.

**Workaround**: Use `kafka-python` or `kafkajs` instead, which work correctly.

## Migrating from Kafka

StreamHouse is designed as a drop-in replacement. Most applications only need to change the bootstrap server:

```diff
- bootstrap.servers=kafka-broker-1:9092,kafka-broker-2:9092
+ bootstrap.servers=streamhouse.example.com:9092
```

### What Works Identically
- Topic creation and deletion
- Producing messages
- Consumer groups
- Offset management
- Partition assignment

### What's Different
- **Storage**: S3-native instead of local disks
- **Architecture**: Stateless agents instead of stateful brokers
- **Metadata**: PostgreSQL/SQLite instead of ZooKeeper

## Troubleshooting

### Connection Refused
```
Error: Connection to localhost:9092 refused
```
Ensure the server is running and check the `KAFKA_ADDR` setting.

### Unsupported API Version
```
Error: Unsupported API version
```
StreamHouse supports modern Kafka clients (0.11+). Update your client library.

### Empty Consumer
```
Consumer receives no messages
```
The Fetch API is currently stubbed. Use REST API or gRPC for consuming until this is fixed.

## Next Steps

- For a simpler API without Kafka complexity, see the [Native SDK documentation](./NATIVE_SDK.md) (coming soon)
- For REST API access, see the [REST API documentation](./REST_API.md)
- For gRPC access, see the [gRPC documentation](./GRPC_API.md)
