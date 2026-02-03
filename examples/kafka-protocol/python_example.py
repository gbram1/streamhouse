#!/usr/bin/env python3
"""
StreamHouse Kafka Protocol - Python Examples

Requirements:
    pip install kafka-python

Usage:
    # Start StreamHouse first:
    USE_LOCAL_STORAGE=1 ./target/release/unified-server

    # Run examples:
    python python_example.py
"""

import json
import time
from kafka import KafkaProducer, KafkaConsumer, KafkaAdminClient
from kafka.admin import NewTopic
from kafka.errors import TopicAlreadyExistsError

BOOTSTRAP_SERVERS = '127.0.0.1:9092'
API_VERSION = (2, 3)


# =============================================================================
# BASIC PRODUCER
# =============================================================================

def basic_producer():
    """Simple producer example."""
    producer = KafkaProducer(
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION
    )

    # Send a message
    future = producer.send('my-topic', b'Hello StreamHouse!')
    result = future.get(timeout=10)

    print(f"Sent message to partition {result.partition} at offset {result.offset}")
    producer.close()


# =============================================================================
# PRODUCER WITH JSON SERIALIZATION
# =============================================================================

def json_producer():
    """Producer with automatic JSON serialization."""
    producer = KafkaProducer(
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None
    )

    # Send structured data
    events = [
        {'event': 'page_view', 'page': '/home', 'user_id': 1},
        {'event': 'click', 'button': 'signup', 'user_id': 1},
        {'event': 'page_view', 'page': '/dashboard', 'user_id': 1},
    ]

    for event in events:
        producer.send('events', event, key=str(event['user_id']))

    producer.flush()
    print(f"Sent {len(events)} JSON events")
    producer.close()


# =============================================================================
# PRODUCER WITH CALLBACKS
# =============================================================================

def producer_with_callbacks():
    """Producer with success/error callbacks."""

    def on_success(metadata):
        print(f"Message sent to {metadata.topic}:{metadata.partition} @ {metadata.offset}")

    def on_error(exc):
        print(f"Error sending message: {exc}")

    producer = KafkaProducer(
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION
    )

    for i in range(5):
        future = producer.send('callback-topic', f'Message {i}'.encode())
        future.add_callback(on_success)
        future.add_errback(on_error)

    producer.flush()
    producer.close()


# =============================================================================
# BATCH PRODUCER
# =============================================================================

def batch_producer():
    """High-throughput batch producer."""
    producer = KafkaProducer(
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        batch_size=32768,      # 32KB batch size
        linger_ms=100,         # Wait up to 100ms to batch
        compression_type='lz4' # Use LZ4 compression
    )

    # Send many messages
    for i in range(1000):
        producer.send('high-throughput', f'Record {i}'.encode())

    producer.flush()
    print("Sent 1000 messages in batches")
    producer.close()


# =============================================================================
# BASIC CONSUMER
# =============================================================================

def basic_consumer():
    """Simple consumer example."""
    consumer = KafkaConsumer(
        'my-topic',
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        auto_offset_reset='earliest',
        consumer_timeout_ms=5000  # Stop after 5s of no messages
    )

    print("Consuming messages (5s timeout)...")
    for message in consumer:
        print(f"  Offset {message.offset}: {message.value.decode()}")

    consumer.close()


# =============================================================================
# CONSUMER WITH JSON DESERIALIZATION
# =============================================================================

def json_consumer():
    """Consumer with automatic JSON deserialization."""
    consumer = KafkaConsumer(
        'events',
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        auto_offset_reset='earliest',
        consumer_timeout_ms=5000,
        value_deserializer=lambda v: json.loads(v.decode('utf-8'))
    )

    print("Consuming JSON events...")
    for message in consumer:
        event = message.value  # Already a dict
        print(f"  Event: {event['event']} - User: {event.get('user_id')}")

    consumer.close()


# =============================================================================
# CONSUMER GROUP
# =============================================================================

def consumer_group():
    """Consumer with group coordination."""
    consumer = KafkaConsumer(
        'orders',
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        group_id='order-processors',
        auto_offset_reset='earliest',
        enable_auto_commit=True,
        auto_commit_interval_ms=1000,
        consumer_timeout_ms=10000
    )

    print(f"Consumer joined group 'order-processors'")
    print(f"Assigned partitions: {consumer.assignment()}")

    for message in consumer:
        print(f"  Processing order at offset {message.offset}")

    consumer.close()


# =============================================================================
# MANUAL OFFSET COMMIT
# =============================================================================

def manual_commit_consumer():
    """Consumer with manual offset commits for exactly-once semantics."""
    consumer = KafkaConsumer(
        'important-events',
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        group_id='careful-processor',
        auto_offset_reset='earliest',
        enable_auto_commit=False,  # Manual commits
        consumer_timeout_ms=5000
    )

    for message in consumer:
        try:
            # Process the message
            print(f"Processing: {message.value.decode()}")

            # Commit only after successful processing
            consumer.commit()
        except Exception as e:
            print(f"Error processing message: {e}")
            # Don't commit - message will be reprocessed

    consumer.close()


# =============================================================================
# SEEK TO SPECIFIC OFFSET
# =============================================================================

def seek_consumer():
    """Consumer that seeks to a specific offset."""
    from kafka import TopicPartition

    consumer = KafkaConsumer(
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        auto_offset_reset='earliest',
        consumer_timeout_ms=5000
    )

    # Manually assign partition
    tp = TopicPartition('my-topic', 0)
    consumer.assign([tp])

    # Seek to beginning
    consumer.seek_to_beginning(tp)
    print(f"Position after seek_to_beginning: {consumer.position(tp)}")

    # Or seek to specific offset
    consumer.seek(tp, 5)  # Start from offset 5
    print(f"Position after seek(5): {consumer.position(tp)}")

    # Or seek to end
    consumer.seek_to_end(tp)
    print(f"Position after seek_to_end: {consumer.position(tp)}")

    consumer.close()


# =============================================================================
# TOPIC MANAGEMENT
# =============================================================================

def manage_topics():
    """Create and delete topics programmatically."""
    admin = KafkaAdminClient(
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION
    )

    # Create a topic
    topic = NewTopic(
        name='programmatic-topic',
        num_partitions=3,
        replication_factor=1
    )

    try:
        admin.create_topics([topic])
        print("Created topic 'programmatic-topic'")
    except TopicAlreadyExistsError:
        print("Topic already exists")

    # List topics
    topics = admin.list_topics()
    print(f"All topics: {topics}")

    # Delete topic
    # admin.delete_topics(['programmatic-topic'])
    # print("Deleted topic")

    admin.close()


# =============================================================================
# PRODUCE AND CONSUME ROUND-TRIP
# =============================================================================

def produce_consume_roundtrip():
    """Complete produce -> flush -> consume cycle."""
    topic = 'roundtrip-test'

    # 1. Produce messages
    print("1. Producing messages...")
    producer = KafkaProducer(
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION
    )

    for i in range(5):
        producer.send(topic, f'Message {i}'.encode(), key=f'key-{i}'.encode())

    producer.flush()
    producer.close()
    print("   Produced 5 messages")

    # 2. Wait for flush to storage
    print("2. Waiting for flush to storage (8s)...")
    time.sleep(8)

    # 3. Consume messages
    print("3. Consuming messages...")
    consumer = KafkaConsumer(
        topic,
        bootstrap_servers=BOOTSTRAP_SERVERS,
        api_version=API_VERSION,
        auto_offset_reset='earliest',
        consumer_timeout_ms=5000
    )

    count = 0
    for message in consumer:
        count += 1
        print(f"   Received: key={message.key}, value={message.value}, offset={message.offset}")

    consumer.close()
    print(f"   Consumed {count} messages")


# =============================================================================
# MAIN
# =============================================================================

if __name__ == '__main__':
    import sys

    examples = {
        'basic-producer': basic_producer,
        'json-producer': json_producer,
        'callback-producer': producer_with_callbacks,
        'batch-producer': batch_producer,
        'basic-consumer': basic_consumer,
        'json-consumer': json_consumer,
        'consumer-group': consumer_group,
        'manual-commit': manual_commit_consumer,
        'seek': seek_consumer,
        'topics': manage_topics,
        'roundtrip': produce_consume_roundtrip,
    }

    if len(sys.argv) < 2:
        print("StreamHouse Kafka Protocol - Python Examples")
        print("=" * 50)
        print("\nUsage: python python_example.py <example>")
        print("\nAvailable examples:")
        for name in examples:
            print(f"  - {name}")
        print("\nExample: python python_example.py roundtrip")
        sys.exit(0)

    example = sys.argv[1]
    if example not in examples:
        print(f"Unknown example: {example}")
        print(f"Available: {', '.join(examples.keys())}")
        sys.exit(1)

    print(f"\nRunning example: {example}")
    print("-" * 50)
    examples[example]()
