# StreamHouse CLI Quick Start

Get up and running with `streamctl` in 5 minutes.

## Step 1: Build the CLI

```bash
# Build in release mode for best performance
cargo build --release -p streamhouse-cli

# The binary will be at ./target/release/streamctl
```

## Step 2: Start the Server

```bash
# In one terminal, start the server
./start-dev.sh
```

You should see:
```
StreamHouse server starting on 0.0.0.0:9090
```

## Step 3: Test the CLI

```bash
# In another terminal, run the test script
./test-cli.sh
```

This will run through all CLI commands and verify everything works.

## Step 4: Try It Yourself

### Create a Topic

```bash
./target/release/streamctl topic create my-topic --partitions 2
```

Expected output:
```
✅ Topic created:
  Name: my-topic
  Partitions: 2
```

### List Topics

```bash
./target/release/streamctl topic list
```

Expected output:
```
Topics (1):
  - my-topic (2 partitions)
```

### Produce a Record

```bash
./target/release/streamctl produce my-topic \
  --partition 0 \
  --value '{"user": "alice", "action": "login", "timestamp": 1234567890}'
```

Expected output:
```
✅ Record produced:
  Topic: my-topic
  Partition: 0
  Offset: 0
  Timestamp: 1737530000123
```

### Produce with a Key

```bash
./target/release/streamctl produce my-topic \
  --partition 0 \
  --key "user-123" \
  --value '{"event": "purchase", "amount": 99.99}'
```

### Consume Records

```bash
./target/release/streamctl consume my-topic --partition 0 --offset 0
```

Expected output:
```
📥 Consuming from my-topic:0 starting at offset 0

Record 1 (offset: 0, timestamp: 1737530000123)
  Value: {
  "user": "alice",
  "action": "login",
  "timestamp": 1234567890
}

Record 2 (offset: 1, timestamp: 1737530005456)
  Key: user-123
  Value: {
  "event": "purchase",
  "amount": 99.99
}

✅ Consumed 2 records
```

### Commit Consumer Offset

```bash
./target/release/streamctl offset commit \
  --group my-app \
  --topic my-topic \
  --partition 0 \
  --offset 2
```

Expected output:
```
✅ Offset committed:
  Consumer group: my-app
  Topic: my-topic
  Partition: 0
  Offset: 2
```

### Get Committed Offset

```bash
./target/release/streamctl offset get \
  --group my-app \
  --topic my-topic \
  --partition 0
```

Expected output:
```
Consumer group: my-app
  Topic: my-topic
  Partition: 0
  Committed offset: 2
```

### Delete Topic

```bash
./target/release/streamctl topic delete my-topic
```

Expected output:
```
✅ Topic deleted: my-topic
```

## Common Patterns

### Stream Processing Workflow

```bash
# 1. Create topic for events
streamctl topic create events --partitions 3

# 2. Producer sends events
for i in {1..10}; do
  streamctl produce events -p 0 -v "{\"id\": $i, \"data\": \"event $i\"}"
done

# 3. Consumer reads events
streamctl consume events -p 0 -o 0

# 4. Consumer commits progress
streamctl offset commit -g processor -t events -p 0 -o 10
```

### Multiple Partitions

```bash
# Create topic with 3 partitions
streamctl topic create logs --partitions 3

# Write to different partitions
streamctl produce logs -p 0 -v '{"level": "info", "msg": "started"}'
streamctl produce logs -p 1 -v '{"level": "warn", "msg": "slow query"}'
streamctl produce logs -p 2 -v '{"level": "error", "msg": "failed"}'

# Read from each partition
streamctl consume logs -p 0 -o 0
streamctl consume logs -p 1 -o 0
streamctl consume logs -p 2 -o 0
```

### JSON Formatting

The CLI automatically pretty-prints JSON values:

```bash
# This JSON will be formatted nicely when consumed
streamctl produce events -p 0 -v '{"user":{"id":123,"name":"alice"},"action":"login"}'

# Output when consumed:
# {
#   "user": {
#     "id": 123,
#     "name": "alice"
#   },
#   "action": "login"
# }
```

## Configuration

### Server Address

```bash
# Set via environment variable
export STREAMHOUSE_ADDR=http://production-server:9090
streamctl topic list

# Or via flag
streamctl --server http://production-server:9090 topic list
```

### Create Alias (Optional)

```bash
# Add to ~/.bashrc or ~/.zshrc
alias sc='./target/release/streamctl'

# Now you can use:
sc topic list
sc produce orders -p 0 -v '{"test": "data"}'
```

## Help

Every command has detailed help:

```bash
# General help
streamctl --help

# Command-specific help
streamctl topic --help
streamctl topic create --help
streamctl produce --help
streamctl consume --help
streamctl offset --help
```

## Troubleshooting

### Error: Connection refused

```
Error: Failed to connect to server

Caused by:
    connection error: Connection refused (os error 61)
```

**Solution**: Start the server first with `./start-dev.sh`

### Error: Topic not found

```
Error: Failed to produce record

Caused by:
    status: NotFound, message: "Topic not found: my-topic"
```

**Solution**: Create the topic first with `streamctl topic create my-topic`

### Error: Invalid partition

```
Error: Failed to produce record

Caused by:
    status: InvalidArgument, message: "Invalid partition: 5"
```

**Solution**: Use a valid partition number (0 to partition_count-1)

## Next Steps

- Read the full CLI documentation: [crates/streamhouse-cli/README.md](crates/streamhouse-cli/README.md)
- Run the automated test suite: `./test-cli.sh`
- Check server logs: `RUST_LOG=debug ./start-dev.sh`
- Try performance benchmarks: `./bench-server.sh`

## Comparison: grpcurl vs streamctl

**grpcurl** (raw gRPC):
```bash
VALUE=$(echo -n '{"test": "data"}' | base64)
grpcurl -plaintext \
  -d "{\"topic\": \"orders\", \"partition\": 0, \"value\": \"$VALUE\"}" \
  localhost:9090 streamhouse.StreamHouse/Produce
```

**streamctl** (ergonomic):
```bash
streamctl produce orders --partition 0 --value '{"test": "data"}'
```

The CLI eliminates:
- Base64 encoding
- JSON escaping
- Proto file management
- Verbose syntax

Much easier for daily use!
