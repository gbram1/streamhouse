# StreamHouse Testing Guide

Complete guide for testing StreamHouse locally.

## Quick Start

### 1. Run Automated Tests

```bash
# All tests (29 tests)
cargo test --workspace

# Just server integration tests
cargo test -p streamhouse-server --test integration_test

# With output
cargo test --workspace -- --nocapture
```

### 2. Start the Server

```bash
# Option 1: Use the convenience script
./start-dev.sh

# Option 2: Manual setup
mkdir -p ./data/{metadata,storage,cache}
export USE_LOCAL_STORAGE=1
export LOCAL_STORAGE_PATH=./data/storage
cargo run -p streamhouse-server
```

The server will start on `localhost:9090`.

**Note**: The server includes gRPC reflection, which allows tools like `grpcurl` to discover services automatically.

### 3. Manual Testing with grpcurl

First, install grpcurl if you don't have it:
```bash
brew install grpcurl  # macOS
```

Then try these commands:

#### Create a Topic
```bash
grpcurl -plaintext \
  -d '{"name": "orders", "partition_count": 3}' \
  localhost:9090 streamhouse.StreamHouse/CreateTopic
```

Expected output:
```json
{
  "topicId": "orders",
  "partitionCount": 3
}
```

#### List Topics
```bash
grpcurl -plaintext \
  localhost:9090 streamhouse.StreamHouse/ListTopics
```

#### Produce Records
First, encode your data (grpcurl requires base64 for bytes):
```bash
# Encode key and value
echo -n "user-123" | base64
# Output: dXNlci0xMjM=

echo -n '{"amount": 99.99, "item": "widget"}' | base64
# Output: eyJhbW91bnQiOiA5OS45OSwgIml0ZW0iOiAid2lkZ2V0In0=
```

Then produce:
```bash
grpcurl -plaintext \
  -d '{
    "topic": "orders",
    "partition": 0,
    "key": "dXNlci0xMjM=",
    "value": "eyJhbW91bnQiOiA5OS45OSwgIml0ZW0iOiAid2lkZ2V0In0="
  }' \
  localhost:9090 streamhouse.StreamHouse/Produce
```

Expected output:
```json
{
  "offset": "0",
  "timestamp": "1737524856277"
}
```

#### Produce Multiple Records (Batch)
```bash
grpcurl -plaintext \
  -d '{
    "topic": "orders",
    "partition": 0,
    "records": [
      {
        "key": "dXNlci0xMjM=",
        "value": "eyJhbW91bnQiOiA1MC4wMH0="
      },
      {
        "key": "dXNlci00NTY=",
        "value": "eyJhbW91bnQiOiAxMjAuNTB9"
      }
    ]
  }' \
  localhost:9090 streamhouse.StreamHouse/ProduceBatch
```

Expected output:
```json
{
  "firstOffset": "0",
  "lastOffset": "1",
  "count": 2
}
```

#### Get Topic Info
```bash
grpcurl -plaintext \
  -d '{"name": "orders"}' \
  localhost:9090 streamhouse.StreamHouse/GetTopic
```

#### Commit Consumer Offset
```bash
grpcurl -plaintext \
  -d '{
    "consumer_group": "analytics-group",
    "topic": "orders",
    "partition": 0,
    "offset": 42
  }' \
  localhost:9090 streamhouse.StreamHouse/CommitOffset
```

#### Get Committed Offset
```bash
grpcurl -plaintext \
  -d '{
    "consumer_group": "analytics-group",
    "topic": "orders",
    "partition": 0
  }' \
  localhost:9090 streamhouse.StreamHouse/GetOffset
```

Expected output:
```json
{
  "offset": "42"
}
```

#### Delete a Topic
```bash
grpcurl -plaintext \
  -d '{"name": "orders"}' \
  localhost:9090 streamhouse.StreamHouse/DeleteTopic
```

## Complete Test Scenario

Here's a complete workflow from start to finish:

```bash
# 1. Start server
./start-dev.sh

# In another terminal:

# 2. Create a topic
grpcurl -plaintext \
  -d '{"name": "clicks", "partition_count": 2}' \
  localhost:9090 streamhouse.StreamHouse/CreateTopic

# 3. Produce some events
for i in {1..5}; do
  VALUE=$(echo -n "{\"user_id\": $i, \"timestamp\": $(date +%s)}" | base64)
  grpcurl -plaintext \
    -d "{\"topic\": \"clicks\", \"partition\": 0, \"value\": \"$VALUE\"}" \
    localhost:9090 streamhouse.StreamHouse/Produce
  echo "Produced record $i"
done

# 4. Verify topic exists
grpcurl -plaintext \
  -d '{"name": "clicks"}' \
  localhost:9090 streamhouse.StreamHouse/GetTopic

# 5. Commit offset for a consumer group
grpcurl -plaintext \
  -d '{
    "consumer_group": "processor-1",
    "topic": "clicks",
    "partition": 0,
    "offset": 5
  }' \
  localhost:9090 streamhouse.StreamHouse/CommitOffset

# 6. Retrieve committed offset
grpcurl -plaintext \
  -d '{
    "consumer_group": "processor-1",
    "topic": "clicks",
    "partition": 0
  }' \
  localhost:9090 streamhouse.StreamHouse/GetOffset
```

## Test Coverage

### Automated Tests (29 total)

**streamhouse-core (7 tests)**
- Varint encoding correctness
- ZigZag signed integer mapping
- Byte efficiency

**streamhouse-metadata (4 tests)**
- Topic CRUD operations
- Segment tracking
- Consumer offset management

**streamhouse-storage (9 tests)**
- Binary segment format
- LZ4 compression
- Delta encoding
- Offset lookups

**streamhouse-server (7 integration tests)**
- Topic creation and retrieval
- Batch production
- Consumer offset commits
- Error handling (invalid partition, topic not found)

### What's NOT Tested Yet

These will be added in future phases:
- End-to-end consume flow (requires writer flush management)
- Concurrent writes
- Large file uploads
- Cache eviction behavior
- Network failures and retries
- S3 upload failures

## Troubleshooting

### Server won't start

**Error**: `unable to open database file`
```bash
# Solution: Create data directories
mkdir -p ./data/{metadata,storage,cache}
```

**Error**: `Address already in use`
```bash
# Solution: Change port or kill existing process
export STREAMHOUSE_ADDR=0.0.0.0:9091
# or
lsof -ti:9090 | xargs kill -9
```

### grpcurl not found

```bash
# macOS
brew install grpcurl

# Or with Go
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest
```

### Can't produce records

Make sure to base64 encode your data:
```bash
echo -n "your-data" | base64
```

### Check server logs

```bash
# Run with debug logging
RUST_LOG=debug cargo run -p streamhouse-server
```

## Next Steps

After testing locally, see:
- [Phase 1 Documentation](docs/phases/phase1/) for architecture details
- [Server README](crates/streamhouse-server/README.md) for deployment options
- Phase 1.7 for CLI tool development
