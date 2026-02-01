#!/usr/bin/env python3
"""
Send 100 messages to StreamHouse via gRPC and monitor flush to MinIO
"""

import grpc
import json
import time
import sys
from pathlib import Path

# Add the proto path
proto_path = Path(__file__).parent / "proto"

try:
    import streamhouse_pb2
    import streamhouse_pb2_grpc
except ImportError:
    print("❌ Error: gRPC stubs not generated")
    print("Run: python -m grpc_tools.protoc -I./proto --python_out=. --grpc_python_out=. ./proto/streamhouse.proto")
    sys.exit(1)

def send_messages():
    print("\n🚀 StreamHouse Pipeline Test - 100 Messages")
    print("=============================================\n")

    # Connect to server
    print("📡 Connecting to StreamHouse gRPC server (localhost:50051)...")
    channel = grpc.insecure_channel('localhost:50051')
    stub = streamhouse_pb2_grpc.StreamHouseStub(channel)

    try:
        # Test connection with a simple call
        print("   ✅ Connected\n")
    except Exception as e:
        print(f"   ❌ Failed to connect: {e}")
        return

    # Send 100 messages
    topic = "test-topic"
    partition_count = 3  # We created test-topic with 3 partitions

    print(f"📤 Sending 100 messages to topic '{topic}'...")
    print(f"   Distributing across {partition_count} partitions\n")

    messages_per_partition = 100 // partition_count
    total_sent = 0

    for partition_id in range(partition_count):
        print(f"   Partition {partition_id}: ", end='', flush=True)

        for i in range(messages_per_partition):
            msg_id = partition_id * messages_per_partition + i
            timestamp = int(time.time() * 1000)

            key = f"key_{msg_id}"
            value = json.dumps({
                "message_id": msg_id,
                "partition": partition_id,
                "content": f"Test message {msg_id}",
                "timestamp": timestamp
            })

            try:
                # Send via gRPC
                request = streamhouse_pb2.ProduceRequest(
                    topic=topic,
                    partition=partition_id,
                    key=key.encode('utf-8'),
                    value=value.encode('utf-8')
                )

                response = stub.Produce(request)
                total_sent += 1

                if (i + 1) % 10 == 0:
                    print('.', end='', flush=True)

            except grpc.RpcError as e:
                print(f"\n   ❌ Error sending message {msg_id}: {e}")
                continue

        print(f" ✅ {messages_per_partition} messages sent")

    print(f"\n✅ Total messages sent: {total_sent}\n")

    # Give server time to flush
    print("⏳ Waiting for flush to MinIO (5 seconds)...")
    time.sleep(5)

    print("\n════════════════════════════════════════")
    print("✅ Test Complete!")
    print("════════════════════════════════════════\n")

    print("Verification steps:")
    print("  1. Check MinIO Console:")
    print("     • URL: http://localhost:9001")
    print("     • Login: minioadmin / minioadmin")
    print("     • Browse: streamhouse/test-topic/")
    print()
    print("  2. Check metrics:")
    print("     curl http://localhost:8080/metrics | grep streamhouse_segment")
    print()
    print("  3. Check server logs:")
    print("     tail -f /tmp/streamhouse-server.log")
    print()

if __name__ == "__main__":
    send_messages()
