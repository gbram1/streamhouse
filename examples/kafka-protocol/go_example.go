// StreamHouse Kafka Protocol - Go Examples
//
// Requirements:
//   go get github.com/confluentinc/confluent-kafka-go/v2/kafka
//
// Usage:
//   # Start StreamHouse first:
//   USE_LOCAL_STORAGE=1 ./target/release/unified-server
//
//   # Run examples:
//   go run go_example.go <example>

package main

import (
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/confluentinc/confluent-kafka-go/v2/kafka"
)

const bootstrapServers = "localhost:9092"

// =============================================================================
// BASIC PRODUCER
// =============================================================================

func basicProducer() error {
	p, err := kafka.NewProducer(&kafka.ConfigMap{
		"bootstrap.servers": bootstrapServers,
	})
	if err != nil {
		return err
	}
	defer p.Close()

	topic := "go-topic"
	message := "Hello from Go!"

	// Delivery report handler
	go func() {
		for e := range p.Events() {
			switch ev := e.(type) {
			case *kafka.Message:
				if ev.TopicPartition.Error != nil {
					fmt.Printf("Delivery failed: %v\n", ev.TopicPartition.Error)
				} else {
					fmt.Printf("Delivered to %v [%d] @ %d\n",
						*ev.TopicPartition.Topic,
						ev.TopicPartition.Partition,
						ev.TopicPartition.Offset)
				}
			}
		}
	}()

	err = p.Produce(&kafka.Message{
		TopicPartition: kafka.TopicPartition{Topic: &topic, Partition: kafka.PartitionAny},
		Value:          []byte(message),
	}, nil)
	if err != nil {
		return err
	}

	// Wait for delivery
	p.Flush(5000)
	return nil
}

// =============================================================================
// PRODUCER WITH KEYS AND HEADERS
// =============================================================================

func producerWithKeys() error {
	p, err := kafka.NewProducer(&kafka.ConfigMap{
		"bootstrap.servers": bootstrapServers,
	})
	if err != nil {
		return err
	}
	defer p.Close()

	topic := "keyed-go-topic"

	// Event struct
	type Event struct {
		Action    string `json:"action"`
		UserID    int    `json:"user_id"`
		Timestamp int64  `json:"timestamp"`
	}

	event := Event{
		Action:    "purchase",
		UserID:    42,
		Timestamp: time.Now().UnixMilli(),
	}

	value, _ := json.Marshal(event)

	err = p.Produce(&kafka.Message{
		TopicPartition: kafka.TopicPartition{Topic: &topic, Partition: kafka.PartitionAny},
		Key:            []byte(fmt.Sprintf("user-%d", event.UserID)),
		Value:          value,
		Headers: []kafka.Header{
			{Key: "content-type", Value: []byte("application/json")},
			{Key: "source", Value: []byte("go-app")},
		},
	}, nil)
	if err != nil {
		return err
	}

	p.Flush(5000)
	fmt.Println("Sent message with key and headers")
	return nil
}

// =============================================================================
// BATCH PRODUCER
// =============================================================================

func batchProducer() error {
	p, err := kafka.NewProducer(&kafka.ConfigMap{
		"bootstrap.servers": bootstrapServers,
		"batch.size":        32768,
		"linger.ms":         100,
	})
	if err != nil {
		return err
	}
	defer p.Close()

	topic := "batch-go-topic"

	// Send many messages
	for i := 0; i < 100; i++ {
		p.Produce(&kafka.Message{
			TopicPartition: kafka.TopicPartition{Topic: &topic, Partition: kafka.PartitionAny},
			Key:            []byte(fmt.Sprintf("key-%d", i)),
			Value:          []byte(fmt.Sprintf("Message %d", i)),
		}, nil)
	}

	p.Flush(10000)
	fmt.Println("Sent 100 messages in batch")
	return nil
}

// =============================================================================
// BASIC CONSUMER
// =============================================================================

func basicConsumer() error {
	c, err := kafka.NewConsumer(&kafka.ConfigMap{
		"bootstrap.servers": bootstrapServers,
		"group.id":          "go-consumer-group",
		"auto.offset.reset": "earliest",
	})
	if err != nil {
		return err
	}
	defer c.Close()

	c.Subscribe("go-topic", nil)

	fmt.Println("Consuming messages (10s timeout)...")
	timeout := time.After(10 * time.Second)

	for {
		select {
		case <-timeout:
			fmt.Println("Timeout reached")
			return nil
		default:
			msg, err := c.ReadMessage(1 * time.Second)
			if err == nil {
				fmt.Printf("  Offset %d: key=%s, value=%s\n",
					msg.TopicPartition.Offset,
					string(msg.Key),
					string(msg.Value))
			}
		}
	}
}

// =============================================================================
// CONSUMER WITH MANUAL COMMITS
// =============================================================================

func manualCommitConsumer() error {
	c, err := kafka.NewConsumer(&kafka.ConfigMap{
		"bootstrap.servers":  bootstrapServers,
		"group.id":           "manual-commit-go-group",
		"auto.offset.reset":  "earliest",
		"enable.auto.commit": false,
	})
	if err != nil {
		return err
	}
	defer c.Close()

	c.Subscribe("important-go-topic", nil)

	fmt.Println("Consuming with manual commits (10s timeout)...")
	timeout := time.After(10 * time.Second)

	for {
		select {
		case <-timeout:
			return nil
		default:
			msg, err := c.ReadMessage(1 * time.Second)
			if err == nil {
				// Process the message
				fmt.Printf("Processing: %s\n", string(msg.Value))

				// Commit after successful processing
				c.CommitMessage(msg)
			}
		}
	}
}

// =============================================================================
// ROUND-TRIP TEST
// =============================================================================

func roundtrip() error {
	topic := "roundtrip-go"

	// 1. Produce messages
	fmt.Println("1. Producing messages...")
	p, err := kafka.NewProducer(&kafka.ConfigMap{
		"bootstrap.servers": bootstrapServers,
	})
	if err != nil {
		return err
	}

	for i := 0; i < 5; i++ {
		p.Produce(&kafka.Message{
			TopicPartition: kafka.TopicPartition{Topic: &topic, Partition: kafka.PartitionAny},
			Key:            []byte(fmt.Sprintf("key-%d", i)),
			Value:          []byte(fmt.Sprintf("Message %d", i)),
		}, nil)
	}
	p.Flush(5000)
	p.Close()
	fmt.Println("   Produced 5 messages")

	// 2. Wait for flush
	fmt.Println("2. Waiting for flush to storage (8s)...")
	time.Sleep(8 * time.Second)

	// 3. Consume messages
	fmt.Println("3. Consuming messages...")
	c, err := kafka.NewConsumer(&kafka.ConfigMap{
		"bootstrap.servers": bootstrapServers,
		"group.id":          fmt.Sprintf("roundtrip-go-%d", time.Now().UnixNano()),
		"auto.offset.reset": "earliest",
	})
	if err != nil {
		return err
	}
	defer c.Close()

	c.Subscribe(topic, nil)

	count := 0
	timeout := time.After(10 * time.Second)

	for {
		select {
		case <-timeout:
			fmt.Printf("   Consumed %d messages\n", count)
			return nil
		default:
			msg, err := c.ReadMessage(1 * time.Second)
			if err == nil {
				count++
				fmt.Printf("   Received: key=%s, value=%s, offset=%d\n",
					string(msg.Key),
					string(msg.Value),
					msg.TopicPartition.Offset)
			}
		}
	}
}

// =============================================================================
// ADMIN OPERATIONS
// =============================================================================

func adminOperations() error {
	a, err := kafka.NewAdminClient(&kafka.ConfigMap{
		"bootstrap.servers": bootstrapServers,
	})
	if err != nil {
		return err
	}
	defer a.Close()

	// Get metadata
	metadata, err := a.GetMetadata(nil, true, 5000)
	if err != nil {
		return err
	}

	fmt.Println("Cluster ID:", metadata.OriginatingBroker.ID)
	fmt.Println("Topics:")
	for _, topic := range metadata.Topics {
		fmt.Printf("  - %s (%d partitions)\n", topic.Topic, len(topic.Partitions))
	}

	// Create a topic
	results, err := a.CreateTopics(
		nil,
		[]kafka.TopicSpecification{
			{
				Topic:             "admin-go-topic",
				NumPartitions:     3,
				ReplicationFactor: 1,
			},
		},
		kafka.SetAdminOperationTimeout(5*time.Second),
	)
	if err != nil {
		return err
	}

	for _, result := range results {
		if result.Error.Code() != kafka.ErrNoError {
			fmt.Printf("Failed to create topic %s: %v\n", result.Topic, result.Error)
		} else {
			fmt.Printf("Created topic: %s\n", result.Topic)
		}
	}

	return nil
}

// =============================================================================
// MAIN
// =============================================================================

func main() {
	examples := map[string]func() error{
		"basic-producer": basicProducer,
		"producer-keys":  producerWithKeys,
		"batch-producer": batchProducer,
		"basic-consumer": basicConsumer,
		"manual-commit":  manualCommitConsumer,
		"roundtrip":      roundtrip,
		"admin":          adminOperations,
	}

	if len(os.Args) < 2 {
		fmt.Println("StreamHouse Kafka Protocol - Go Examples")
		fmt.Println("==================================================")
		fmt.Println("\nUsage: go run go_example.go <example>")
		fmt.Println("\nAvailable examples:")
		for name := range examples {
			fmt.Printf("  - %s\n", name)
		}
		fmt.Println("\nExample: go run go_example.go roundtrip")
		return
	}

	example := os.Args[1]
	fn, ok := examples[example]
	if !ok {
		fmt.Printf("Unknown example: %s\n", example)
		fmt.Print("Available: ")
		for name := range examples {
			fmt.Printf("%s ", name)
		}
		fmt.Println()
		os.Exit(1)
	}

	fmt.Printf("\nRunning example: %s\n", example)
	fmt.Println("--------------------------------------------------")

	if err := fn(); err != nil {
		fmt.Printf("Error: %v\n", err)
		os.Exit(1)
	}
}
