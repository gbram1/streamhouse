#!/usr/bin/env python3
"""
StreamHouse Kafka Protocol Compatibility Test Suite

Tests all major Kafka protocol features:
- Produce/Consume messages
- Topic auto-creation
- Consumer groups
- Offset management
- Multiple partitions

Usage:
    # Start server first:
    USE_LOCAL_STORAGE=1 ./target/release/unified-server

    # Run tests:
    python3 scripts/test_kafka_protocol.py
"""

import logging
import time
import sys
import json
from datetime import datetime

# Configure logging
logging.basicConfig(
    level=logging.WARNING,
    format='%(levelname)s - %(name)s - %(message)s'
)

try:
    from kafka import KafkaProducer, KafkaConsumer, KafkaAdminClient
    from kafka.admin import NewTopic
    from kafka.errors import TopicAlreadyExistsError
except ImportError:
    print("ERROR: kafka-python not installed. Run: pip install kafka-python")
    sys.exit(1)

BOOTSTRAP_SERVERS = '127.0.0.1:9092'
API_VERSION = (2, 3)

class Colors:
    GREEN = '\033[92m'
    RED = '\033[91m'
    YELLOW = '\033[93m'
    BLUE = '\033[94m'
    RESET = '\033[0m'
    BOLD = '\033[1m'

def log_test(name):
    print(f"\n{Colors.BLUE}{Colors.BOLD}=== {name} ==={Colors.RESET}")

def log_success(msg):
    print(f"{Colors.GREEN}  ✓ {msg}{Colors.RESET}")

def log_fail(msg):
    print(f"{Colors.RED}  ✗ {msg}{Colors.RESET}")

def log_info(msg):
    print(f"  {msg}")


def test_connection():
    """Test basic connection to the server."""
    log_test("Test 1: Connection")
    try:
        producer = KafkaProducer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            request_timeout_ms=5000,
            max_block_ms=5000
        )
        producer.close()
        log_success("Connected to StreamHouse Kafka server")
        return True
    except Exception as e:
        log_fail(f"Failed to connect: {e}")
        return False


def test_produce_single():
    """Test producing a single message."""
    log_test("Test 2: Produce Single Message")
    try:
        producer = KafkaProducer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            request_timeout_ms=10000,
            max_block_ms=10000
        )

        topic = 'test-produce-single'
        future = producer.send(topic, b'Hello StreamHouse!', key=b'key1')
        result = future.get(timeout=10)

        log_success(f"Produced message to '{topic}' at offset {result.offset}")
        producer.close()
        return True
    except Exception as e:
        log_fail(f"Failed to produce: {e}")
        return False


def test_produce_batch():
    """Test producing multiple messages."""
    log_test("Test 3: Produce Batch Messages")
    try:
        producer = KafkaProducer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            request_timeout_ms=10000,
            max_block_ms=10000
        )

        topic = 'test-produce-batch'
        count = 10

        for i in range(count):
            msg = json.dumps({'id': i, 'timestamp': datetime.now().isoformat()}).encode()
            producer.send(topic, msg, key=f'key-{i}'.encode())

        producer.flush()
        log_success(f"Produced {count} messages to '{topic}'")
        producer.close()
        return True
    except Exception as e:
        log_fail(f"Failed to produce batch: {e}")
        return False


def test_consume():
    """Test consuming messages (requires prior produce + flush)."""
    log_test("Test 4: Consume Messages")

    topic = 'test-consume'

    # First, produce some messages
    log_info("Producing messages...")
    try:
        producer = KafkaProducer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            request_timeout_ms=10000,
            max_block_ms=10000
        )

        for i in range(5):
            producer.send(topic, f'Message {i}'.encode(), key=f'key{i}'.encode())
        producer.flush()
        producer.close()
        log_info(f"Produced 5 messages to '{topic}'")
    except Exception as e:
        log_fail(f"Failed to produce: {e}")
        return False

    # Wait for background flush to storage
    log_info("Waiting 8 seconds for flush to storage...")
    time.sleep(8)

    # Now consume
    log_info("Consuming messages...")
    try:
        consumer = KafkaConsumer(
            topic,
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            auto_offset_reset='earliest',
            consumer_timeout_ms=10000,
            request_timeout_ms=15000,
            session_timeout_ms=30000
        )

        messages = []
        for msg in consumer:
            messages.append({
                'key': msg.key.decode() if msg.key else None,
                'value': msg.value.decode() if msg.value else None,
                'offset': msg.offset
            })
            if len(messages) >= 5:
                break

        consumer.close()

        if len(messages) > 0:
            log_success(f"Consumed {len(messages)} messages:")
            for m in messages:
                log_info(f"  offset={m['offset']}, key={m['key']}, value={m['value']}")
            return True
        else:
            log_fail("No messages consumed (timeout)")
            return False

    except Exception as e:
        log_fail(f"Failed to consume: {e}")
        return False


def test_consumer_group():
    """Test consumer group functionality."""
    log_test("Test 5: Consumer Group")

    topic = 'test-consumer-group'
    group_id = 'test-group-1'

    # Produce messages
    log_info("Producing messages...")
    try:
        producer = KafkaProducer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            request_timeout_ms=10000,
            max_block_ms=10000
        )

        for i in range(3):
            producer.send(topic, f'Group message {i}'.encode())
        producer.flush()
        producer.close()
    except Exception as e:
        log_fail(f"Failed to produce: {e}")
        return False

    log_info("Waiting 8 seconds for flush...")
    time.sleep(8)

    # Consume with group
    log_info(f"Consuming with group_id='{group_id}'...")
    try:
        consumer = KafkaConsumer(
            topic,
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            group_id=group_id,
            auto_offset_reset='earliest',
            consumer_timeout_ms=10000,
            enable_auto_commit=True
        )

        count = 0
        for msg in consumer:
            count += 1
            log_info(f"  Received: {msg.value.decode()}")
            if count >= 3:
                break

        consumer.close()

        if count > 0:
            log_success(f"Consumer group '{group_id}' consumed {count} messages")
            return True
        else:
            log_fail("No messages consumed")
            return False

    except Exception as e:
        log_fail(f"Consumer group failed: {e}")
        return False


def test_topic_metadata():
    """Test fetching topic metadata."""
    log_test("Test 6: Topic Metadata")
    try:
        consumer = KafkaConsumer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            request_timeout_ms=10000
        )

        # This triggers metadata fetch
        topics = consumer.topics()
        partitions = consumer.partitions_for_topic('test-consume')

        log_success(f"Found {len(topics)} topics: {list(topics)[:5]}...")
        if partitions:
            log_success(f"Topic 'test-consume' has partitions: {partitions}")

        consumer.close()
        return True
    except Exception as e:
        log_fail(f"Failed to get metadata: {e}")
        return False


def test_json_messages():
    """Test producing and consuming JSON messages."""
    log_test("Test 7: JSON Messages")

    topic = 'test-json'

    # Produce JSON
    log_info("Producing JSON messages...")
    try:
        producer = KafkaProducer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            value_serializer=lambda v: json.dumps(v).encode('utf-8'),
            key_serializer=lambda k: k.encode('utf-8') if k else None
        )

        messages = [
            {'order_id': 1001, 'product': 'Widget', 'quantity': 5},
            {'order_id': 1002, 'product': 'Gadget', 'quantity': 2},
            {'order_id': 1003, 'product': 'Gizmo', 'quantity': 10},
        ]

        for msg in messages:
            producer.send(topic, msg, key=str(msg['order_id']))
        producer.flush()
        producer.close()
        log_success(f"Produced {len(messages)} JSON messages")
    except Exception as e:
        log_fail(f"Failed to produce JSON: {e}")
        return False

    log_info("Waiting 8 seconds for flush...")
    time.sleep(8)

    # Consume JSON
    log_info("Consuming JSON messages...")
    try:
        consumer = KafkaConsumer(
            topic,
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            auto_offset_reset='earliest',
            consumer_timeout_ms=10000,
            value_deserializer=lambda v: json.loads(v.decode('utf-8'))
        )

        count = 0
        for msg in consumer:
            count += 1
            log_info(f"  Order: {msg.value}")
            if count >= 3:
                break

        consumer.close()

        if count > 0:
            log_success(f"Consumed {count} JSON messages")
            return True
        else:
            log_fail("No JSON messages consumed")
            return False

    except Exception as e:
        log_fail(f"Failed to consume JSON: {e}")
        return False


def test_large_messages():
    """Test producing and consuming larger messages."""
    log_test("Test 8: Large Messages")

    topic = 'test-large'

    # Produce large message (1KB)
    log_info("Producing 1KB message...")
    try:
        producer = KafkaProducer(
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            max_request_size=10485760  # 10MB
        )

        large_value = 'X' * 1024  # 1KB
        future = producer.send(topic, large_value.encode(), key=b'large-key')
        result = future.get(timeout=10)

        producer.close()
        log_success(f"Produced 1KB message at offset {result.offset}")
    except Exception as e:
        log_fail(f"Failed to produce large message: {e}")
        return False

    log_info("Waiting 8 seconds for flush...")
    time.sleep(8)

    # Consume
    try:
        consumer = KafkaConsumer(
            topic,
            bootstrap_servers=BOOTSTRAP_SERVERS,
            api_version=API_VERSION,
            auto_offset_reset='earliest',
            consumer_timeout_ms=10000
        )

        for msg in consumer:
            size = len(msg.value)
            log_success(f"Consumed message of {size} bytes")
            consumer.close()
            return True

        consumer.close()
        log_fail("No large message consumed")
        return False

    except Exception as e:
        log_fail(f"Failed to consume large message: {e}")
        return False


def run_all_tests():
    """Run all tests and report results."""
    print(f"\n{Colors.BOLD}StreamHouse Kafka Protocol Test Suite{Colors.RESET}")
    print(f"Server: {BOOTSTRAP_SERVERS}")
    print(f"Time: {datetime.now().isoformat()}")
    print("=" * 50)

    results = {}

    # Run tests
    tests = [
        ("Connection", test_connection),
        ("Produce Single", test_produce_single),
        ("Produce Batch", test_produce_batch),
        ("Consume", test_consume),
        ("Consumer Group", test_consumer_group),
        ("Topic Metadata", test_topic_metadata),
        ("JSON Messages", test_json_messages),
        ("Large Messages", test_large_messages),
    ]

    for name, test_fn in tests:
        try:
            results[name] = test_fn()
        except Exception as e:
            log_fail(f"Test crashed: {e}")
            results[name] = False

    # Summary
    print(f"\n{Colors.BOLD}{'=' * 50}{Colors.RESET}")
    print(f"{Colors.BOLD}Test Summary{Colors.RESET}")
    print("=" * 50)

    passed = sum(1 for v in results.values() if v)
    failed = len(results) - passed

    for name, result in results.items():
        status = f"{Colors.GREEN}PASS{Colors.RESET}" if result else f"{Colors.RED}FAIL{Colors.RESET}"
        print(f"  {name}: {status}")

    print("=" * 50)
    if failed == 0:
        print(f"{Colors.GREEN}{Colors.BOLD}All {passed} tests passed!{Colors.RESET}")
    else:
        print(f"{Colors.YELLOW}Passed: {passed}, Failed: {failed}{Colors.RESET}")

    return failed == 0


if __name__ == '__main__':
    success = run_all_tests()
    sys.exit(0 if success else 1)
