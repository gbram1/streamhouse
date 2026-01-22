# StreamHouse Server

gRPC server for StreamHouse - an S3-native streaming platform.

## Running the Server

### Development Mode (Local Storage)

For local development without S3:

```bash
# Set environment to use local filesystem
export USE_LOCAL_STORAGE=1
export LOCAL_STORAGE_PATH=./data/storage
export STREAMHOUSE_METADATA=./data/metadata.db
export STREAMHOUSE_CACHE=./data/cache

# Run the server
cargo run -p streamhouse-server
```

### Production Mode (S3)

Configure AWS credentials and run:

```bash
# AWS credentials (or use IAM role)
export AWS_ACCESS_KEY_ID=your_key
export AWS_SECRET_ACCESS_KEY=your_secret
export AWS_REGION=us-east-1

# StreamHouse config
export STREAMHOUSE_BUCKET=my-streamhouse-bucket
export STREAMHOUSE_ADDR=0.0.0.0:9090

# Run the server
cargo run -p streamhouse-server --release
```

## Configuration

Environment variables:

- `STREAMHOUSE_ADDR` - Server bind address (default: `0.0.0.0:9090`)
- `STREAMHOUSE_METADATA` - Path to SQLite metadata DB (default: `./data/metadata.db`)
- `STREAMHOUSE_CACHE` - Cache directory (default: `./data/cache`)
- `STREAMHOUSE_CACHE_SIZE` - Cache size in bytes (default: `1073741824` = 1GB)
- `STREAMHOUSE_BUCKET` - S3 bucket name (default: `streamhouse`)
- `AWS_REGION` - AWS region (default: `us-east-1`)
- `USE_LOCAL_STORAGE` - Set to use local filesystem instead of S3
- `LOCAL_STORAGE_PATH` - Path for local storage (default: `./data/storage`)

## Testing with grpcurl

Install grpcurl:
```bash
brew install grpcurl  # macOS
# or: go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest
```

### Create a Topic

```bash
grpcurl -plaintext \
  -d '{
    "name": "orders",
    "partition_count": 3,
    "retention_ms": 86400000
  }' \
  localhost:9090 \
  streamhouse.StreamHouse/CreateTopic
```

### List Topics

```bash
grpcurl -plaintext localhost:9090 streamhouse.StreamHouse/ListTopics
```

### Produce a Record

```bash
grpcurl -plaintext \
  -d '{
    "topic": "orders",
    "partition": 0,
    "key": "dXNlci0xMjM=",
    "value": "eyJhbW91bnQiOiA5OS45OX0="
  }' \
  localhost:9090 \
  streamhouse.StreamHouse/Produce
```

Note: key and value must be base64 encoded. Example:
```bash
echo -n "user-123" | base64  # dXNlci0xMjM=
echo -n '{"amount": 99.99}' | base64  # eyJhbW91bnQiOiA5OS45OX0=
```

### Produce a Batch

```bash
grpcurl -plaintext \
  -d '{
    "topic": "orders",
    "partition": 0,
    "records": [
      {"key": "a2V5MQ==", "value": "dmFsdWUx"},
      {"key": "a2V5Mg==", "value": "dmFsdWUy"}
    ]
  }' \
  localhost:9090 \
  streamhouse.StreamHouse/ProduceBatch
```

### Consume Records

```bash
grpcurl -plaintext \
  -d '{
    "topic": "orders",
    "partition": 0,
    "offset": 0,
    "max_records": 10
  }' \
  localhost:9090 \
  streamhouse.StreamHouse/Consume
```

### Commit Consumer Offset

```bash
grpcurl -plaintext \
  -d '{
    "consumer_group": "my-group",
    "topic": "orders",
    "partition": 0,
    "offset": 42
  }' \
  localhost:9090 \
  streamhouse.StreamHouse/CommitOffset
```

### Get Consumer Offset

```bash
grpcurl -plaintext \
  -d '{
    "consumer_group": "my-group",
    "topic": "orders",
    "partition": 0
  }' \
  localhost:9090 \
  streamhouse.StreamHouse/GetOffset
```

## Running Tests

```bash
# Run all tests
cargo test -p streamhouse-server

# Run integration tests
cargo test -p streamhouse-server --test integration_test

# Run with output
cargo test -p streamhouse-server -- --nocapture
```

## API Documentation

See [proto/streamhouse.proto](proto/streamhouse.proto) for the complete API definition.

### Available RPC Methods

**Admin Operations:**
- `CreateTopic` - Create a new topic
- `GetTopic` - Get topic information
- `ListTopics` - List all topics
- `DeleteTopic` - Delete a topic

**Producer Operations:**
- `Produce` - Produce a single record
- `ProduceBatch` - Produce multiple records in one call

**Consumer Operations:**
- `Consume` - Consume records from a partition
- `CommitOffset` - Commit consumer group offset
- `GetOffset` - Get committed offset for a consumer group

## Architecture

The server uses:
- **gRPC** for the API layer
- **SQLite** for metadata (topics, partitions, segments, offsets)
- **S3** (or local filesystem) for segment storage
- **LRU cache** for frequently accessed segments
- **Prefetching** for sequential reads

See [../../docs/phases/phase1/](../../docs/phases/phase1/) for detailed architecture documentation.
