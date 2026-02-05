# StreamHouse Java Client

Java client for the StreamHouse streaming data platform.

## Requirements

- Java 11+
- Jackson 2.x (included as dependency)

## Installation

### Maven

```xml
<dependency>
    <groupId>io.streamhouse</groupId>
    <artifactId>streamhouse-client</artifactId>
    <version>0.1.0</version>
</dependency>
```

### Gradle

```groovy
implementation 'io.streamhouse:streamhouse-client:0.1.0'
```

## Quick Start

```java
import io.streamhouse.client.StreamHouseClient;
import io.streamhouse.client.model.*;

public class Example {
    public static void main(String[] args) throws Exception {
        // Create a client
        StreamHouseClient client = StreamHouseClient.builder()
            .baseUrl("http://localhost:8080")
            .build();

        // Create a topic
        Topic topic = client.createTopic("events", 3);
        System.out.println("Created topic: " + topic.getName());

        // Produce a message
        ProduceResult result = client.produce("events", "{\"event\": \"click\", \"user\": \"123\"}");
        System.out.printf("Produced to partition %d at offset %d%n",
            result.getPartition(), result.getOffset());

        // Produce with key
        result = client.produce("events", "{\"event\": \"purchase\"}", "user-123");

        // Batch produce
        BatchProduceResult batchResult = client.produceBatch("events", List.of(
            BatchRecord.of("{\"event\": \"click\"}"),
            BatchRecord.of("{\"event\": \"scroll\"}", "user-456")
        ));
        System.out.printf("Produced %d messages%n", batchResult.getCount());

        // Consume messages
        ConsumeResult consumeResult = client.consume("events", 0);
        for (ConsumedRecord record : consumeResult.getRecords()) {
            System.out.printf("[%d:%d] %s%n",
                record.getPartition(), record.getOffset(), record.getValue());
        }

        // Execute SQL query
        SqlResult sqlResult = client.query("SELECT * FROM events LIMIT 10");
        System.out.printf("Query returned %d rows in %dms%n",
            sqlResult.getRowCount(), sqlResult.getExecutionTimeMs());

        // List topics
        List<Topic> topics = client.listTopics();
        for (Topic t : topics) {
            System.out.printf("%s: %d partitions, %d messages%n",
                t.getName(), t.getPartitions(), t.getMessageCount());
        }
    }
}
```

## With Authentication

```java
StreamHouseClient client = StreamHouseClient.builder()
    .baseUrl("http://localhost:8080")
    .apiKey("sk_live_your_api_key_here")
    .build();

// All operations will use the API key
List<Topic> topics = client.listTopics();
```

## Custom Timeout

```java
import java.time.Duration;

StreamHouseClient client = StreamHouseClient.builder()
    .baseUrl("http://localhost:8080")
    .timeout(Duration.ofSeconds(60))
    .build();
```

## API Reference

### Topic Operations

```java
// List all topics
List<Topic> topics = client.listTopics();

// Create a new topic
Topic topic = client.createTopic("events", 3);

// Create with replication factor
Topic topic = client.createTopic("events", 3, 2);

// Get topic details
Topic topic = client.getTopic("events");

// Delete a topic
client.deleteTopic("events");

// List partitions
List<Partition> partitions = client.listPartitions("events");
```

### Producer Operations

```java
// Produce a single message
ProduceResult result = client.produce("events", "{\"data\": \"value\"}");

// Produce with key
ProduceResult result = client.produce("events", "{\"data\": \"value\"}", "user-123");

// Produce with key and partition
ProduceResult result = client.produce("events", "{\"data\": \"value\"}", "user-123", 0);

// Batch produce
BatchProduceResult result = client.produceBatch("events", List.of(
    BatchRecord.of("{\"event\": \"a\"}"),
    BatchRecord.of("{\"event\": \"b\"}", "key1")
));
```

### Consumer Operations

```java
// Consume messages
ConsumeResult result = client.consume("events", 0);

// Consume with offset and limit
ConsumeResult result = client.consume("events", 0, 100, 50);

// Access consumed records
for (ConsumedRecord record : result.getRecords()) {
    System.out.println(record.getPartition());
    System.out.println(record.getOffset());
    System.out.println(record.getKey());
    System.out.println(record.getValue());
}
```

### Consumer Group Operations

```java
// List consumer groups
List<ConsumerGroup> groups = client.listConsumerGroups();

// Get consumer group details
ConsumerGroupDetail group = client.getConsumerGroup("my-group");

// Commit offset
boolean success = client.commitOffset("my-group", "events", 0, 100);

// Delete consumer group
client.deleteConsumerGroup("my-group");
```

### SQL Operations

```java
// Execute SQL query
SqlResult result = client.query("SELECT * FROM events WHERE key = 'user-123'");

// With timeout
SqlResult result = client.query("SELECT COUNT(*) FROM events", 60000L);

// Access results
System.out.println(result.getColumns());         // Column info
System.out.println(result.getRows());            // Data rows
System.out.println(result.getRowCount());        // Number of rows
System.out.println(result.getExecutionTimeMs()); // Query time
```

### Cluster Operations

```java
// List agents
List<Agent> agents = client.listAgents();

// Get metrics
MetricsSnapshot metrics = client.getMetrics();

// Health check
boolean isHealthy = client.healthCheck();
```

## Error Handling

```java
import io.streamhouse.client.exception.*;

try {
    Topic topic = client.getTopic("nonexistent");
} catch (NotFoundException e) {
    System.out.println("Topic not found");
} catch (ValidationException e) {
    System.out.println("Validation error: " + e.getMessage());
} catch (ConflictException e) {
    System.out.println("Topic already exists");
} catch (TimeoutException e) {
    System.out.println("Request timed out");
} catch (ConnectionException e) {
    System.out.println("Connection failed");
} catch (ServerException e) {
    System.out.println("Server error: " + e.getMessage());
} catch (StreamHouseException e) {
    System.out.println("Unknown error: " + e.getMessage());
}
```

## Models

The client provides type-safe models:

- `Topic` - Topic metadata
- `Partition` - Partition information
- `ProduceResult` - Result of producing a message
- `BatchRecord` - A record for batch produce
- `BatchProduceResult` - Result of batch produce
- `ConsumedRecord` - A consumed message
- `ConsumeResult` - Result of consuming messages
- `ConsumerGroup` - Consumer group summary
- `ConsumerGroupDetail` - Detailed consumer group info
- `Agent` - Server/agent information
- `SqlResult` - SQL query results
- `MetricsSnapshot` - Cluster metrics

## Building

```bash
mvn clean install
```

## License

Apache 2.0
