# StreamHouse Testing Guide - Fixed

## Current Status

Your StreamHouse server is **running successfully** on port 50051!

The infrastructure is working:
- ✅ PostgreSQL running (port 5432)
- ✅ MinIO running (ports 9000, 9001)
- ✅ Prometheus running (port 9090)
- ✅ Grafana running (port 3000)
- ✅ StreamHouse server running (port 50051)

## What's Working

The **core StreamHouse pipeline** is operational:
1. Server accepts gRPC connections on port 50051
2. PostgreSQL stores metadata
3. MinIO/local filesystem stores data segments
4. Producer/Consumer examples can run

## Testing Commands

### 1. Verify Server is Running

```bash
./scripts/test-server-connection.sh
```

This checks:
- Server listening on port 50051 ✓
- PostgreSQL connectivity ✓
- Infrastructure status ✓

### 2. Use CLI to Test (Recommended)

The `streamctl` CLI is the best way to test the running server:

```bash
# Create a topic
cargo run --bin streamctl -- topic create test-topic --partitions 4

# Produce messages
echo "Hello StreamHouse" | cargo run --bin streamctl -- produce test-topic

# Consume messages
cargo run --bin streamctl -- consume test-topic --group test-group

# List topics
cargo run --bin streamctl -- topic list
```

### 3. Query PostgreSQL

Check the metadata created:

```bash
# Connect to PostgreSQL
docker exec -it streamhouse-postgres psql -U streamhouse -d streamhouse_metadata

# List topics
SELECT * FROM topics;

# List partitions
SELECT * FROM partitions;

# Exit
\q
```

### 4. Check MinIO/Local Storage

View the data segments:

```bash
# If using local storage (current setup)
ls -la ./data/storage/

# If using MinIO
open http://localhost:9001  # minioadmin / minioadmin
```

### 5. Monitor with Prometheus

```bash
# Open Prometheus
open http://localhost:9090

# Note: Server metrics are not yet exposed on port 8080
# The gRPC server itself doesn't have metrics endpoints
```

## Important Notes

### Metrics Status

The `streamhouse-server` binary (the gRPC server you're running) **does not have HTTP metrics endpoints**.

The Phase 7 metrics implementation was done for:
- `streamhouse-client` (Producer/Consumer libraries)
- `streamhouse-agent` (planned but not the same as streamhouse-server)

So you won't see metrics at `http://localhost:8080` - this is expected!

### What Works

1. **gRPC Server** ✅
   - Accepts producer/consumer connections
   - Stores metadata in PostgreSQL
   - Stores data in local filesystem or MinIO

2. **CLI Tool (`streamctl`)** ✅
   - Create topics
   - Produce messages
   - Consume messages
   - List topics/partitions

3. **Infrastructure** ✅
   - PostgreSQL for metadata
   - MinIO for object storage (or local filesystem)
   - Prometheus ready (no metrics to scrape yet)
   - Grafana ready (no data to display yet)

### What Doesn't Work Yet

1. **HTTP Metrics Endpoints** ❌
   - The server doesn't expose `/metrics`, `/health`, `/ready`
   - These would need to be added to `streamhouse-server`

2. **Agent with Metrics** ❌
   - The Phase 7 plan mentioned an `streamhouse-agent` with metrics
   - This is different from `streamhouse-server`
   - Not yet implemented

## Recommended Testing Flow

### Step 1: Create a Topic

```bash
cargo run --bin streamctl -- topic create orders --partitions 4
```

### Step 2: Produce Messages

```bash
# Produce a single message
echo '{"orderId": 1, "amount": 99.99}' | cargo run --bin streamctl -- produce orders

# Produce multiple messages
for i in {1..10}; do
  echo "{\"orderId\": $i, \"amount\": $((RANDOM % 100)).99}" | cargo run --bin streamctl -- produce orders
done
```

### Step 3: Consume Messages

```bash
cargo run --bin streamctl -- consume orders --group my-consumer-group
```

### Step 4: Verify in PostgreSQL

```bash
docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT * FROM topics;"
```

### Step 5: Check Data Storage

```bash
# If using local storage
find ./data/storage -type f -name "*.dat"

# If using MinIO
docker exec streamhouse-minio mc ls myminio/streamhouse-data --recursive
```

## Troubleshooting

### "Server not found on port 50051"

Make sure the server is running:
```bash
source .env.dev
cargo run --release --bin streamhouse-server
```

### "Cannot connect to PostgreSQL"

Verify PostgreSQL is running:
```bash
docker ps | grep postgres
```

### "No tables found"

Tables are created automatically when you create the first topic:
```bash
cargo run --bin streamctl -- topic create test --partitions 1
```

## Summary

Your StreamHouse server is **working correctly**!

The confusion was around metrics - the `streamhouse-server` binary is a pure gRPC server and doesn't have HTTP metrics endpoints. That's totally fine for the core functionality.

To test the complete pipeline:
1. Use `streamctl` CLI to create topics, produce, and consume
2. Verify data in PostgreSQL and storage
3. Everything is working as expected for Phases 1-5

**Phase 7 metrics would require**:
- Adding HTTP server to `streamhouse-server`
- Or implementing a separate `streamhouse-agent` component
- This can be done as a future enhancement

For now, enjoy using StreamHouse via the CLI! 🚀
