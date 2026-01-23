# StreamHouse Testing Guide

This guide covers how to test StreamHouse at different levels: unit tests, integration tests, and manual end-to-end testing.

---

## Quick Start: Run All Tests

```bash
# Run all tests in the workspace
cargo test --workspace

# Run with output visible
cargo test --workspace -- --nocapture

# Run a specific test
cargo test test_produce_and_consume

# Run tests for a specific crate
cargo test -p streamhouse-storage
cargo test -p streamhouse-server
```

---

## 1. Unit Tests

Unit tests are embedded in the source files using `#[cfg(test)]` modules.

### Storage Layer Tests

```bash
# Test segment writer/reader
cargo test -p streamhouse-storage segment::

# Test varint encoding
cargo test -p streamhouse-core varint::

# Test metadata store
cargo test -p streamhouse-metadata
```

**Example: Running segment tests**
```bash
$ cargo test -p streamhouse-storage segment::

running 9 tests
test segment::reader::tests::test_invalid_magic ... ok
test segment::writer::tests::test_writer_finish ... ok
test segment::reader::tests::test_roundtrip_single_record ... ok
test segment::writer::tests::test_writer_single_record ... ok
test segment::writer::tests::test_writer_multiple_records ... ok
test segment::reader::tests::test_roundtrip_multiple_records ... ok
test segment::reader::tests::test_read_from_offset ... ok
test segment::reader::tests::test_roundtrip_with_lz4 ... ok
test segment::writer::tests::test_writer_with_lz4_compression ... ok
```

---

## 2. Integration Tests

Integration tests live in `crates/streamhouse-server/tests/` and test the full gRPC API.

### Available Integration Tests

```bash
# Run all integration tests
cargo test -p streamhouse-server --test integration_test

# Run specific integration test
cargo test -p streamhouse-server test_produce_and_consume
cargo test -p streamhouse-server test_create_and_get_topic
cargo test -p streamhouse-server test_produce_batch
cargo test -p streamhouse-server test_consumer_offset_commit
```

### What Integration Tests Cover

1. **test_create_and_get_topic**: Topic creation and retrieval
2. **test_list_topics**: Listing multiple topics
3. **test_produce_and_consume**: Full write → read flow (CRITICAL!)
4. **test_produce_batch**: Batch writing
5. **test_consumer_offset_commit**: Consumer group offset management
6. **test_topic_not_found**: Error handling for missing topics
7. **test_invalid_partition**: Error handling for invalid partitions

### Key Test: Produce → Consume Workflow

This is the most important test that validates Phase 2.1 is working:

```rust
#[tokio::test]
async fn test_produce_and_consume() {
    let (service, _temp) = setup_test_service().await;

    // Create topic
    service.create_topic(...).await.unwrap();

    // Produce records
    service.produce(...).await.unwrap();

    // Consume records (THIS WOULD FAIL IN PHASE 1!)
    let consume_resp = service.consume(...).await.unwrap();

    // Verify we can read what we wrote
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].value, b"value-1");
}
```

**Before Phase 2.1**: This test would fail because segments weren't flushed to S3.
**After Phase 2.1**: ✅ Test passes! Background flush makes data available.

---

## 3. Manual End-to-End Testing

Test the actual server with real gRPC calls.

### Step 1: Start the Server

```bash
# Start with local storage (for testing)
export USE_LOCAL_STORAGE=1
export RUST_LOG=info
cargo run -p streamhouse-server

# Or with S3 (production-like)
export STREAMHOUSE_BUCKET=my-test-bucket
export AWS_REGION=us-east-1
export RUST_LOG=debug
cargo run -p streamhouse-server
```

You should see:
```
2025-01-22T10:30:00Z INFO  streamhouse_server: Initializing metadata store at ./data/metadata.db
2025-01-22T10:30:00Z INFO  streamhouse_server: Initializing object store (bucket: streamhouse)
2025-01-22T10:30:00Z INFO  streamhouse_server: Using local storage at ./data/storage
2025-01-22T10:30:00Z INFO  streamhouse_server: Initializing cache at ./data/cache (max size: 1073741824 bytes)
2025-01-22T10:30:00Z INFO  streamhouse_server: Initializing writer pool
2025-01-22T10:30:00Z INFO  streamhouse_server: Starting background flush thread (interval: 5s)
2025-01-22T10:30:00Z INFO  streamhouse_server: StreamHouse server starting on 0.0.0.0:9090
```

### Step 2: Test with grpcurl

```bash
# Install grpcurl if needed
brew install grpcurl  # macOS
# or
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest

# List available services
grpcurl -plaintext localhost:9090 list

# Create a topic
grpcurl -plaintext -d '{
  "name": "test-events",
  "partition_count": 3,
  "retention_ms": 86400000
}' localhost:9090 streamhouse.StreamHouse/CreateTopic

# Produce a record
grpcurl -plaintext -d '{
  "topic": "test-events",
  "partition": 0,
  "key": "dXNlci0xMjM=",
  "value": "eyJldmVudCI6ICJ1c2VyX3NpZ251cCJ9"
}' localhost:9090 streamhouse.StreamHouse/Produce

# Wait 5 seconds for background flush...
sleep 5

# Consume records
grpcurl -plaintext -d '{
  "topic": "test-events",
  "partition": 0,
  "offset": 0,
  "max_records": 10
}' localhost:9090 streamhouse.StreamHouse/Consume
```

### Step 3: Test with the CLI (Future)

```bash
# Create topic
streamctl topic create test-events --partitions 3

# Produce
echo '{"event": "user_signup"}' | streamctl produce test-events

# Consume
streamctl consume test-events --partition 0 --offset 0
```

---

## 4. Testing Writer Pooling & Background Flush

### Verify Background Flush is Working

**Test 1: Produce Without Manual Flush**

```bash
# Terminal 1: Start server with debug logging
RUST_LOG=debug cargo run -p streamhouse-server

# Terminal 2: Produce some records
grpcurl -plaintext -d '{
  "topic": "flush-test",
  "partition": 0,
  "value": "dGVzdCBkYXRh"
}' localhost:9090 streamhouse.StreamHouse/Produce

# Wait and watch Terminal 1 logs
# You should see after ~5 seconds:
# INFO streamhouse_storage::writer_pool: Background flush completed flushed=1 errors=0 total_writers=1
```

**Test 2: Verify Consume Works Immediately**

```bash
# Produce
grpcurl -plaintext -d '{"topic": "test", "partition": 0, "value": "aGVsbG8="}' \
  localhost:9090 streamhouse.StreamHouse/Produce

# Wait 6 seconds for flush
sleep 6

# Consume - should work!
grpcurl -plaintext -d '{"topic": "test", "partition": 0, "offset": 0}' \
  localhost:9090 streamhouse.StreamHouse/Consume
```

**Test 3: Graceful Shutdown**

```bash
# Start server
cargo run -p streamhouse-server

# Produce records
grpcurl -plaintext -d '{"topic": "test", "partition": 0, "value": "aGVsbG8="}' \
  localhost:9090 streamhouse.StreamHouse/Produce

# Immediately kill with Ctrl+C
# Watch logs - should see:
# INFO streamhouse_server: Received SIGINT (Ctrl+C), initiating graceful shutdown
# INFO streamhouse_server: Flushing all pending writes...
# INFO streamhouse_storage::writer_pool: Shutting down writer pool, flushing all writers
# INFO streamhouse_storage::writer_pool: Writer pool shutdown complete writer_count=1
# INFO streamhouse_server: StreamHouse server shut down gracefully
```

---

## 5. Performance Testing

### Throughput Test

```bash
# Use the integration test as a benchmark
cargo test -p streamhouse-server test_produce_batch --release -- --nocapture

# Or write a custom benchmark
cargo bench  # (if benchmarks are added)
```

### Expected Performance (Phase 2.1)

- **Write throughput**: ~1,000 records/sec (up from ~150 in Phase 1)
- **Latency**: ~5ms average write latency
- **Flush interval**: 5 seconds (configurable)
- **Segment size**: Auto-rolls at 64MB or 10 minutes

---

## 6. Testing Configuration

### Environment Variables

```bash
# Server address
export STREAMHOUSE_ADDR=0.0.0.0:9090

# Storage
export STREAMHOUSE_METADATA=./data/metadata.db
export STREAMHOUSE_BUCKET=my-bucket
export AWS_REGION=us-east-1

# Local storage (for testing)
export USE_LOCAL_STORAGE=1
export LOCAL_STORAGE_PATH=./data/storage

# Cache
export STREAMHOUSE_CACHE=./data/cache
export STREAMHOUSE_CACHE_SIZE=1073741824  # 1GB

# Phase 2.1: Writer pool
export FLUSH_INTERVAL_SECS=5  # Background flush every 5 seconds

# Logging
export RUST_LOG=debug
export RUST_LOG=streamhouse_storage::writer_pool=trace  # Detailed flush logs
```

---

## 7. Troubleshooting Tests

### Common Issues

**1. Tests fail with "Offset not found"**
- **Cause**: Segments not flushed yet
- **Solution**: In Phase 2.1, wait for background flush (5s) or use small segment size in tests

**2. Tests fail with "Topic not found"**
- **Cause**: Test cleanup issue
- **Solution**: Each test creates a unique topic name

**3. Tests hang indefinitely**
- **Cause**: Async runtime issue
- **Solution**: Check for missing `.await` or deadlocks

**4. S3 upload failures in CI**
- **Cause**: No AWS credentials
- **Solution**: Use `USE_LOCAL_STORAGE=1` in CI

### Debug a Failing Test

```bash
# Run with full output
cargo test test_name -- --nocapture

# Run with trace logging
RUST_LOG=trace cargo test test_name -- --nocapture

# Run single-threaded (easier debugging)
cargo test test_name -- --test-threads=1 --nocapture
```

---

## 8. CI/CD Testing

All tests run automatically on every push to `main` via GitHub Actions.

### CI Jobs

1. **test**: Runs `cargo test --workspace --all-features`
2. **fmt**: Checks code formatting
3. **clippy**: Runs linter
4. **build**: Builds release binaries

### View CI Results

```bash
# Visit GitHub Actions page
https://github.com/YOUR_USERNAME/streamhouse/actions

# Or use GitHub CLI
gh run list
gh run view
gh run watch
```

### Required CI Fixes (Completed)

- ✅ sqlx offline mode: Generated `.sqlx/` metadata files
- ✅ protoc installation: Added `apt-get install protobuf-compiler`

---

## 9. Test Coverage Goals

### Current Coverage (Phase 2.1)

- ✅ Segment read/write roundtrip
- ✅ Topic CRUD operations
- ✅ Produce single record
- ✅ Produce batch
- ✅ Consume records
- ✅ Consumer offset commits
- ✅ Writer pooling
- ✅ Background flush
- ✅ Graceful shutdown

### Future Coverage (Phase 3+)

- ⏳ Concurrent writes to same partition
- ⏳ Consumer group coordination
- ⏳ Partition rebalancing
- ⏳ Metadata backend switching
- ⏳ S3 vs local storage equivalence
- ⏳ Cross-partition batching

---

## 10. Writing New Tests

### Adding a Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        let result = my_function();
        assert_eq!(result, expected_value);
    }

    #[tokio::test]
    async fn test_async_feature() {
        let result = my_async_function().await;
        assert!(result.is_ok());
    }
}
```

### Adding an Integration Test

Add to `crates/streamhouse-server/tests/integration_test.rs`:

```rust
#[tokio::test]
async fn test_new_feature() {
    let (service, _temp) = setup_test_service().await;

    // Test your feature
    let response = service.new_rpc_method(request).await.unwrap();

    // Assert expectations
    assert_eq!(response.field, expected_value);
}
```

---

## Success Criteria Checklist

Phase 2.1 is complete when:

- ✅ All unit tests pass
- ✅ All integration tests pass (especially `test_produce_and_consume`)
- ✅ CI pipeline is green
- ✅ Manual produce → wait 5s → consume workflow works
- ✅ Graceful shutdown flushes pending data
- ✅ No clippy warnings
- ✅ Code formatted correctly

**Current Status**: ✅ ALL CRITERIA MET!

---

## Quick Reference

```bash
# Most common commands
cargo test --workspace                    # Run all tests
cargo test -p streamhouse-server          # Server tests only
cargo test test_produce_and_consume       # Critical test
cargo fmt --all                           # Format code
cargo clippy --workspace -- -D warnings   # Lint code

# Start server for manual testing
USE_LOCAL_STORAGE=1 RUST_LOG=info cargo run -p streamhouse-server

# Test with grpcurl
grpcurl -plaintext localhost:9090 list
grpcurl -plaintext -d '{"name":"test","partition_count":1}' \
  localhost:9090 streamhouse.StreamHouse/CreateTopic
```

---

**Next Steps**: See [Phase 2.2: Kafka Protocol](phases/phase2/2.2-KAFKA-PROTOCOL.md)
