# MinIO Data Visibility - FIXED ✅

## Problem Summary

User reported: "I tested a few examples but I don't see any data in minio?"

## Root Causes Identified

### 1. Segment Buffering by Design
- **Issue**: Messages were buffered in memory, not immediately flushed to S3
- **Default thresholds**: 64MB (client) / 100MB (agent) segment size, 10 minute age
- **Impact**: For low-traffic scenarios, data never reached MinIO

### 2. Missing AWS Credentials
- **Issue**: Agent tried to access EC2 metadata service (169.254.169.254)
- **Cause**: `object_store` crate's credential provider chain falls back to EC2 when env vars missing
- **Impact**: Even if segments rolled, S3 uploads would fail

### 3. HTTP Scheme Not Allowed
- **Issue**: `object_store` crate rejected `http://` URLs for MinIO
- **Error**: "URL scheme is not allowed"
- **Cause**: Security setting requires HTTPS by default
- **Impact**: Segments rolled but failed to upload to MinIO

## Fixes Applied

### Fix 1: Configurable Segment Sizes

**File**: `crates/streamhouse-agent/src/bin/agent.rs`

Added environment variables for development/testing:

```rust
let segment_max_size = std::env::var("SEGMENT_MAX_SIZE")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .unwrap_or(100 * 1024 * 1024); // Default: 100MB

let segment_max_age_ms = std::env::var("SEGMENT_MAX_AGE_MS")
    .ok()
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(10 * 60 * 1000); // Default: 10 minutes
```

**Usage**:
```bash
export SEGMENT_MAX_SIZE=1048576     # 1MB for testing
export SEGMENT_MAX_AGE_MS=30000     # 30 seconds
```

### Fix 2: Added AWS Credentials

Restarted agent with proper MinIO credentials:

```bash
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
AWS_ENDPOINT_URL=http://localhost:9000
```

This prevents the agent from trying to access EC2 metadata service.

### Fix 3: Allow HTTP for MinIO

**File**: `crates/streamhouse-agent/src/bin/agent.rs`

Added `.with_allow_http(true)` to S3 builder:

```rust
let object_store: Arc<dyn ObjectStore> = Arc::new(
    AmazonS3Builder::from_env()
        .with_bucket_name(&s3_bucket)
        .with_allow_http(true) // Allow HTTP for MinIO/local development
        .build()?,
);
```

## Verification

### Test 1: Send Small Messages
```bash
curl -X POST http://localhost:3001/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic":"test-web-integration","partition":0,"value":"Test message"}'
```

**Result**: ✅ Messages accepted, buffered in memory

### Test 2: Send 220 Large Messages (>1MB total)
```bash
# Each message ~5KB, 220 messages = ~1.1MB
PADDING=$(python3 -c "print('X' * 5000)")
for i in {1..220}; do
  curl -s -X POST http://localhost:3001/api/v1/produce \
    -H "Content-Type: application/json" \
    -d "{\"topic\":\"test-web-integration\",\"partition\":0,\"value\":\"Message $i: $PADDING\"}"
done
```

**Result**: ✅ Segment rolled and uploaded to S3

### Test 3: Verify Data in MinIO

**Agent logs**:
```
Segment rolled and uploaded to S3
  topic=test-web-integration
  partition=0
  base_offset=0
  end_offset=166
  size_bytes=4704
  s3_key=data/test-web-integration/0/00000000000000000000.seg
```

**MinIO contents**:
```bash
docker exec streamhouse-minio ls -lh /data/streamhouse-data/data/test-web-integration/0/
# Output: 00000000000000000000.seg/
```

✅ **Data is now visible in MinIO!**

## How to Start System for Testing

### Terminal 1 - Agent
```bash
AGENT_ID=agent-001 \
AGENT_ADDRESS=127.0.0.1:9091 \
METRICS_PORT=8081 \
MANAGED_TOPICS="my-orders,user-events,test-web-integration" \
METADATA_STORE=./data/metadata.db \
SEGMENT_MAX_SIZE=1048576 \
SEGMENT_MAX_AGE_MS=30000 \
AWS_ACCESS_KEY_ID=minioadmin \
AWS_SECRET_ACCESS_KEY=minioadmin \
AWS_ENDPOINT_URL=http://localhost:9000 \
RUST_LOG=info \
./target/release/agent
```

### Terminal 2 - REST API
```bash
METADATA_STORE=./data/metadata.db \
AWS_ACCESS_KEY_ID=minioadmin \
AWS_SECRET_ACCESS_KEY=minioadmin \
AWS_ENDPOINT_URL=http://localhost:9000 \
RUST_LOG=info \
./target/release/api
```

### Terminal 3 - Web Console
```bash
cd web
PORT=3002 npm run dev
```

## Expected Behavior

### Low Traffic (<1MB)
- Messages buffered in memory
- **Data NOT yet in MinIO** (by design for throughput)
- Will flush after:
  - 1MB accumulated (with our test config)
  - 30 seconds pass AND another message arrives (age-based trigger)

### High Traffic (>1MB)
- Segment rolls automatically
- Data immediately visible in MinIO
- Offsets return correctly

## Important Notes

### Segment Buffering is By Design
This is **not a bug** - it's an optimization:
- Batching reduces S3 API calls
- Increases throughput
- Reduces costs

For **production**:
- Keep larger thresholds (64MB-100MB)
- Use age-based flushing (10 minutes)

For **testing/development**:
- Use smaller thresholds (1MB)
- Use shorter age (30 seconds)
- Or send enough data to trigger size threshold

### Age-Based Flushing Limitation
Age threshold only checked when `append()` is called:
- No background task checking age
- If no traffic for 30s, old segments won't flush
- Workaround: Send a dummy message to trigger flush

Could be enhanced with background task in future.

## Summary

**Problem**: Data not appearing in MinIO
**Cause**: Combination of segment buffering, missing credentials, and HTTP scheme restriction
**Fix**: Made segments configurable + added credentials + allowed HTTP
**Status**: ✅ WORKING - Data now successfully uploads to MinIO!

## Files Modified

1. **`crates/streamhouse-agent/src/bin/agent.rs`** (~15 LOC)
   - Added `SEGMENT_MAX_SIZE` environment variable support
   - Added `SEGMENT_MAX_AGE_MS` environment variable support
   - Added `.with_allow_http(true)` to S3 builder
   - Added logging for segment configuration

## Testing Commands

### Send Test Message
```bash
curl -X POST http://localhost:3001/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic":"test-web-integration","partition":0,"value":"Hello MinIO!"}'
```

### Check MinIO Contents
```bash
docker exec streamhouse-minio ls -lh /data/streamhouse-data/data/test-web-integration/0/
```

### Monitor Agent Logs
```bash
tail -f /tmp/agent-http-fixed.log | grep -E "upload|segment|ERROR|WARN"
```

---

**The entire pipeline now works end-to-end with data persisting to MinIO! 🚀**
