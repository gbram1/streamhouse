# StreamHouse CLI (`streamctl`)

Command-line tool for interacting with StreamHouse servers.

## Installation

```bash
# Build and install
cargo install --path crates/streamhouse-cli

# Or run directly
cargo run -p streamhouse-cli -- <command>
```

## Usage

### Connection

By default, `streamctl` connects to `http://localhost:9090`. Override with:

```bash
# Via flag
streamctl --server http://production:9090 topic list

# Via environment variable
export STREAMHOUSE_ADDR=http://production:9090
streamctl topic list
```

### Topic Management

#### Create a Topic

```bash
# Create with default 1 partition
streamctl topic create orders

# Create with multiple partitions
streamctl topic create orders --partitions 3

# Create with retention period (1 day = 86400000ms)
streamctl topic create orders --partitions 3 --retention-ms 86400000
```

#### List Topics

```bash
streamctl topic list
```

Output:
```
Topics (2):
  - orders (3 partitions)
  - events (1 partitions)
```

#### Get Topic Info

```bash
streamctl topic get orders
```

Output:
```
Topic: orders
  Partitions: 3
  Retention: 86400000ms
```

#### Delete a Topic

```bash
streamctl topic delete orders
```

### Producing Records

#### Basic Produce

```bash
# Produce to partition 0
streamctl produce orders --partition 0 --value '{"amount": 99.99, "item": "widget"}'
```

#### Produce with Key

```bash
streamctl produce orders --partition 0 --key "user-123" --value '{"amount": 99.99}'
```

Output:
```
✅ Record produced:
  Topic: orders
  Partition: 0
  Offset: 42
  Timestamp: 1737524856277
```

### Consuming Records

#### Consume from Beginning

```bash
streamctl consume orders --partition 0 --offset 0
```

#### Consume with Limit

```bash
# Get only first 10 records
streamctl consume orders --partition 0 --offset 0 --limit 10
```

Output:
```
📥 Consuming from orders:0 starting at offset 0

Record 1 (offset: 0, timestamp: 1737524856277)
  Key: user-123
  Value: {
  "amount": 99.99,
  "item": "widget"
}

Record 2 (offset: 1, timestamp: 1737524860123)
  Value: {
  "user": "jane",
  "action": "purchase"
}

✅ Consumed 2 records
```

### Consumer Offsets

#### Commit an Offset

```bash
streamctl offset commit \
  --group analytics \
  --topic orders \
  --partition 0 \
  --offset 42
```

Output:
```
✅ Offset committed:
  Consumer group: analytics
  Topic: orders
  Partition: 0
  Offset: 42
```

#### Get Committed Offset

```bash
streamctl offset get \
  --group analytics \
  --topic orders \
  --partition 0
```

Output:
```
Consumer group: analytics
  Topic: orders
  Partition: 0
  Committed offset: 42
```

## Examples

### Complete Workflow

```bash
# 1. Create a topic
streamctl topic create clicks --partitions 2

# 2. Produce some events
streamctl produce clicks --partition 0 --value '{"user_id": 1, "url": "/home"}'
streamctl produce clicks --partition 0 --value '{"user_id": 2, "url": "/products"}'
streamctl produce clicks --partition 1 --value '{"user_id": 3, "url": "/checkout"}'

# 3. Consume from partition 0
streamctl consume clicks --partition 0 --offset 0

# 4. Track consumer progress
streamctl offset commit --group processor --topic clicks --partition 0 --offset 2

# 5. Check committed offset
streamctl offset get --group processor --topic clicks --partition 0

# 6. List all topics
streamctl topic list

# 7. Clean up
streamctl topic delete clicks
```

### Batch Production Script

```bash
#!/bin/bash
# produce-batch.sh

TOPIC="events"
PARTITION=0

for i in {1..100}; do
  VALUE="{\"event_id\": $i, \"timestamp\": $(date +%s)}"
  streamctl produce $TOPIC --partition $PARTITION --value "$VALUE"
  echo "Produced event $i"
done
```

## Help

```bash
# General help
streamctl --help

# Topic command help
streamctl topic --help

# Create topic help
streamctl topic create --help
```

## Configuration

All configuration is via flags or environment variables:

| Flag | Environment Variable | Default | Description |
|------|---------------------|---------|-------------|
| `--server` | `STREAMHOUSE_ADDR` | `http://localhost:9090` | Server address |

## Development

### Building

```bash
cd crates/streamhouse-cli
cargo build --release
```

### Testing

```bash
# Start server first
./start-dev.sh

# Test CLI commands
cargo run -p streamhouse-cli -- topic create test --partitions 1
cargo run -p streamhouse-cli -- produce test --partition 0 --value '{"test": "data"}'
cargo run -p streamhouse-cli -- consume test --partition 0 --offset 0
```

## Comparison with grpcurl

The CLI is more ergonomic than raw grpcurl:

**grpcurl:**
```bash
# Produce (requires base64 encoding)
VALUE=$(echo -n '{"amount": 99.99}' | base64)
grpcurl -plaintext \
  -d "{\"topic\": \"orders\", \"partition\": 0, \"value\": \"$VALUE\"}" \
  localhost:9090 streamhouse.StreamHouse/Produce
```

**streamctl:**
```bash
# Produce (no encoding needed)
streamctl produce orders --partition 0 --value '{"amount": 99.99}'
```

## Troubleshooting

### Connection Refused

```
Error: Failed to connect to server

Caused by:
    connection error: Connection refused (os error 61)
```

**Solution**: Start the server first with `./start-dev.sh`

### Topic Not Found

```
Error: Failed to produce record

Caused by:
    status: NotFound, message: "Topic not found: orders"
```

**Solution**: Create the topic first with `streamctl topic create orders`

### Invalid Partition

```
Error: Failed to produce record

Caused by:
    status: InvalidArgument, message: "Invalid partition: 5"
```

**Solution**: Use a valid partition number (0 to partition_count-1)
