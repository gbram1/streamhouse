# Using Kafka Connect with StreamHouse

StreamHouse implements the Kafka binary protocol (14 APIs), which means **standard Kafka Connect workers connect directly** without any custom code.

## How It Works

StreamHouse's Kafka protocol layer (port 9092) is fully compatible with:
- Kafka Connect workers (standalone and distributed)
- All standard Kafka Connect connectors (Debezium, Confluent, community)
- Kafka Connect REST API (managed by the Connect worker, not StreamHouse)

## Quick Start

### 1. Start StreamHouse

```bash
streamhouse-server --kafka-port 9092 --http-port 8080
```

### 2. Configure Kafka Connect Worker

Create `connect-standalone.properties`:

```properties
bootstrap.servers=localhost:9092
key.converter=org.apache.kafka.connect.json.JsonConverter
value.converter=org.apache.kafka.connect.json.JsonConverter
key.converter.schemas.enable=false
value.converter.schemas.enable=false
offset.storage.file.filename=/tmp/connect.offsets
offset.flush.interval.ms=10000
```

### 3. Run a Connector

Example: FileStream source connector (ships with Kafka Connect):

Create `file-source.properties`:

```properties
name=local-file-source
connector.class=org.apache.kafka.connect.file.FileStreamSourceConnector
tasks.max=1
file=/var/log/app.log
topic=app-logs
```

Start the worker:

```bash
connect-standalone.sh connect-standalone.properties file-source.properties
```

Records from `/var/log/app.log` will appear in the `app-logs` topic in StreamHouse.

### 4. Verify

```bash
# Via StreamHouse CLI
streamhouse consume app-logs --from-beginning

# Via Kafka consumer (since StreamHouse speaks Kafka protocol)
kafka-console-consumer.sh --bootstrap-server localhost:9092 --topic app-logs --from-beginning
```

## Distributed Mode

For production, use distributed mode with multiple workers:

```properties
bootstrap.servers=localhost:9092
group.id=connect-cluster
config.storage.topic=connect-configs
offset.storage.topic=connect-offsets
status.storage.topic=connect-status
config.storage.replication.factor=1
offset.storage.replication.factor=1
status.storage.replication.factor=1
```

StreamHouse will automatically create the internal topics (`connect-configs`, `connect-offsets`, `connect-status`).

## Supported Kafka APIs

StreamHouse implements all APIs needed for Kafka Connect:

| API | Name | Used By Connect |
|-----|------|----------------|
| 18 | ApiVersions | Version negotiation |
| 3 | Metadata | Topic discovery |
| 0 | Produce | Writing records |
| 1 | Fetch | Reading records |
| 2 | ListOffsets | Offset management |
| 10 | FindCoordinator | Group coordination |
| 11 | JoinGroup | Worker coordination |
| 14 | SyncGroup | Worker coordination |
| 12 | Heartbeat | Liveness |
| 13 | LeaveGroup | Graceful shutdown |
| 8 | OffsetCommit | Offset tracking |
| 9 | OffsetFetch | Offset tracking |
| 19 | CreateTopics | Auto-create topics |

## Limitations

- StreamHouse currently runs as a single node, so `replication.factor` should be set to `1`
- Exactly-once semantics for Connect require transaction support (Phase 13.1)
- Schema Registry integration uses StreamHouse's built-in registry (port 8081), not Confluent's
