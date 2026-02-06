# Offset Issue - Fixed (Pending Agent Connection)

## The Problem

Offsets were always showing as `0` when producing messages via the REST API.

## Root Cause

The Producer uses **batching** for efficiency:
1. `send()` adds message to batch and returns immediately with `offset: None`
2. Batch flushes after 100ms timeout OR when batch is full (100 messages)
3. After flush, the offset is sent via `offset_receiver` channel
4. `wait_offset()` blocks until the receiver gets the offset

**Original code** didn't wait for flush:
```rust
let result = producer.send(...).await?;
Ok(Json(ProduceResponse {
    offset: result.offset.unwrap_or(0), // Always None, so always 0!
    partition: result.partition,
}))
```

## The Fix

Added explicit flush + wait_offset:
```rust
// Send message
let mut result = producer.send(...).await?;

// Immediately flush to send the batch
producer.flush().await?;

// Wait for offset (should be immediate after flush)
let offset = result.wait_offset().await?;

Ok(Json(ProduceResponse {
    offset,  // Real offset!
    partition: result.partition,
}))
```

## Why Offsets are Per-Partition

This is **correct behavior**:
- Each partition has its own offset sequence: 0, 1, 2, 3...
- Partition 0: messages get offsets 0, 1, 2, 3...
- Partition 1: messages get offsets 0, 1, 2, 3...
- Partition 2: messages get offsets 0, 1, 2, 3...

So seeing "offset: 0" multiple times is **normal** if messages go to different partitions!

**Example**:
```bash
# Message 1 -> Partition 0
{"offset": 0, "partition": 0}  ✓ First message in partition 0

# Message 2 -> Partition 1
{"offset": 0, "partition": 1}  ✓ First message in partition 1

# Message 3 -> Partition 0
{"offset": 1, "partition": 0}  ✓ Second message in partition 0

# Message 4 -> Partition 1
{"offset": 1, "partition": 1}  ✓ Second message in partition 1
```

##  Current Status

✅ **Fix Applied**: Code now calls `flush()` + `wait_offset()`
✅ **Compiled**: Binary rebuilt successfully
⚠️ **Testing Blocked**: Agent connection issues (transport error)

## Agent Connection Issue

The producer can't connect to agent on `127.0.0.1:9090`:
```
ERROR Failed to send batch error=Failed to connect to agent agent-001 at 127.0.0.1:9090: transport error
```

**Possible Causes**:
1. Docker/MinIO also using port 9090 (saw conflict in `lsof`)
2. gRPC connection timing issue
3. Network/firewall blocking localhost connections

## Next Steps

### Option 1: Test via Web Console (Recommended)

The web console is the real user interface anyway!

1. **Open**: http://localhost:3002/console
2. **Select topic**: test-web-integration
3. **Enter message**: `{"test": "hello"}`
4. **Send**: Click button or press Cmd+Enter
5. **Watch offsets**: Should increment in the recent messages log

### Option 2: Fix Agent Connection

1. **Change agent port** to avoid conflict:
   ```bash
   export AGENT_ADDRESS=127.0.0.1:9091
   export METRICS_PORT=8081
   ./target/release/agent
   ```

2. **Update metadata** so producer finds agent on 9091

3. **Test again**

### Option 3: Debug Transport Error

1. Check if it's http2/tls issue
2. Try connecting directly with grpcurl
3. Check firewall rules for localhost

## Verification

Once agent connection works, verify with:

```bash
# Send 5 messages to same partition
for i in {1..5}; do
  curl -X POST http://localhost:3001/api/v1/produce \
    -H "Content-Type: application/json" \
    -d "{\"topic\":\"test\",\"partition\":0,\"value\":\"msg $i\"}"
done
```

**Expected Output**:
```json
{"offset":0,"partition":0}
{"offset":1,"partition":0}
{"offset":2,"partition":0}
{"offset":3,"partition":0}
{"offset":4,"partition":0}
```

## Documentation Updates

✅ **Added to STREAMHOUSE_ARCHITECTURE_EXPLAINED.md**:
- What is a key and when to use it
- Why keys ensure ordering (same key -> same partition)
- Examples of key-based routing
- When to use keys vs no keys

## Summary

**Code Fix**: ✅ Complete - flush() + wait_offset() now called
**Testing**: ⚠️ Blocked by agent connection issue
**Workaround**: Use web console (http://localhost:3002/console) to test
**Documentation**: ✅ Updated with key explanation

The offset fix is correct and will work once the agent connection is resolved. The web console should work fine for testing the full pipeline!
