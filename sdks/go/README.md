# StreamHouse Go Client

Go client for the StreamHouse streaming data platform.

## Installation

```bash
go get github.com/streamhouse/streamhouse-go
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "log"

    streamhouse "github.com/streamhouse/streamhouse-go"
)

func main() {
    ctx := context.Background()
    client := streamhouse.NewClient("http://localhost:8080")

    // Create a topic
    topic, err := client.CreateTopic(ctx, "events", 3)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Created topic: %s\n", topic.Name)

    // Produce a message
    result, err := client.Produce(ctx, "events", `{"event": "click", "user": "123"}`)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Produced to partition %d at offset %d\n", result.Partition, result.Offset)

    // Produce with key
    result, err = client.Produce(ctx, "events", `{"event": "purchase"}`,
        streamhouse.WithKey("user-123"))
    if err != nil {
        log.Fatal(err)
    }

    // Batch produce
    batchResult, err := client.ProduceBatch(ctx, "events", []streamhouse.BatchRecord{
        streamhouse.NewBatchRecord(`{"event": "click"}`),
        streamhouse.NewBatchRecordWithKey(`{"event": "scroll"}`, "user-456"),
    })
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Produced %d messages\n", batchResult.Count)

    // Consume messages
    consumeResult, err := client.Consume(ctx, "events", 0, streamhouse.ConsumeOptions{
        Offset:     0,
        MaxRecords: 10,
    })
    if err != nil {
        log.Fatal(err)
    }
    for _, record := range consumeResult.Records {
        fmt.Printf("[%d:%d] %s\n", record.Partition, record.Offset, record.Value)
    }

    // Execute SQL query
    sqlResult, err := client.Query(ctx, "SELECT * FROM events LIMIT 10", streamhouse.QueryOptions{})
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Query returned %d rows in %dms\n", sqlResult.RowCount, sqlResult.ExecutionTimeMs)

    // List topics
    topics, err := client.ListTopics(ctx)
    if err != nil {
        log.Fatal(err)
    }
    for _, t := range topics {
        fmt.Printf("%s: %d partitions, %d messages\n", t.Name, t.Partitions, t.MessageCount)
    }
}
```

## With Authentication

```go
client := streamhouse.NewClient("http://localhost:8080",
    streamhouse.WithAPIKey("sk_live_your_api_key_here"))

// All operations will use the API key
topics, err := client.ListTopics(ctx)
```

## Custom HTTP Client

```go
httpClient := &http.Client{
    Timeout: 60 * time.Second,
    Transport: &http.Transport{
        MaxIdleConns:        100,
        MaxIdleConnsPerHost: 10,
    },
}

client := streamhouse.NewClient("http://localhost:8080",
    streamhouse.WithHTTPClient(httpClient))
```

## API Reference

### Topic Operations

```go
// List all topics
topics, err := client.ListTopics(ctx)

// Create a new topic
topic, err := client.CreateTopic(ctx, "events", 3)

// Create with replication factor
topic, err := client.CreateTopic(ctx, "events", 3,
    streamhouse.WithReplicationFactor(2))

// Get topic details
topic, err := client.GetTopic(ctx, "events")

// Delete a topic
err := client.DeleteTopic(ctx, "events")

// List partitions
partitions, err := client.ListPartitions(ctx, "events")
```

### Producer Operations

```go
// Produce a single message
result, err := client.Produce(ctx, "events", `{"data": "value"}`)

// Produce with key and partition
result, err := client.Produce(ctx, "events", `{"data": "value"}`,
    streamhouse.WithKey("user-123"),
    streamhouse.WithPartition(0))

// Batch produce
result, err := client.ProduceBatch(ctx, "events", []streamhouse.BatchRecord{
    streamhouse.NewBatchRecord(`{"event": "a"}`),
    streamhouse.NewBatchRecordWithKey(`{"event": "b"}`, "key1"),
})
```

### Consumer Operations

```go
// Consume messages
result, err := client.Consume(ctx, "events", 0, streamhouse.ConsumeOptions{})

// Consume with offset and limit
result, err := client.Consume(ctx, "events", 0, streamhouse.ConsumeOptions{
    Offset:     100,
    MaxRecords: 50,
})

// Access consumed records
for _, record := range result.Records {
    fmt.Println(record.Partition, record.Offset, record.Key, record.Value)
}
```

### Consumer Group Operations

```go
// List consumer groups
groups, err := client.ListConsumerGroups(ctx)

// Get consumer group details
group, err := client.GetConsumerGroup(ctx, "my-group")

// Commit offset
err := client.CommitOffset(ctx, "my-group", "events", 0, 100)

// Delete consumer group
err := client.DeleteConsumerGroup(ctx, "my-group")
```

### SQL Operations

```go
// Execute SQL query
result, err := client.Query(ctx, "SELECT * FROM events WHERE key = 'user-123'",
    streamhouse.QueryOptions{})

// With timeout
result, err := client.Query(ctx, "SELECT COUNT(*) FROM events",
    streamhouse.QueryOptions{TimeoutMs: 60000})

// Access results
fmt.Println(result.Columns)         // Column info
fmt.Println(result.RowCount)        // Number of rows
fmt.Println(result.ExecutionTimeMs) // Query time

// Get a specific row
row, err := result.GetRow(0)
```

### Cluster Operations

```go
// List agents
agents, err := client.ListAgents(ctx)

// Get metrics
metrics, err := client.GetMetrics(ctx)

// Health check
isHealthy := client.HealthCheck(ctx)
```

## Error Handling

```go
import "errors"

topic, err := client.GetTopic(ctx, "nonexistent")
if err != nil {
    if streamhouse.IsNotFound(err) {
        fmt.Println("Topic not found")
    } else if streamhouse.IsValidation(err) {
        fmt.Println("Validation error:", err)
    } else if streamhouse.IsConflict(err) {
        fmt.Println("Topic already exists")
    } else if streamhouse.IsTimeout(err) {
        fmt.Println("Request timed out")
    } else if streamhouse.IsConnection(err) {
        fmt.Println("Connection failed")
    } else if streamhouse.IsServer(err) {
        fmt.Println("Server error:", err)
    } else {
        fmt.Println("Unknown error:", err)
    }
}
```

## Types

The client provides strongly-typed responses:

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
- `SQLResult` - SQL query results
- `MetricsSnapshot` - Cluster metrics

## License

Apache 2.0
