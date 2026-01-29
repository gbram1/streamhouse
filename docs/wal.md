# Write-Ahead Log (WAL) for Durability

## Overview

The Write-Ahead Log (WAL) provides local disk durability for StreamHouse data before it's uploaded to S3. This prevents data loss if an agent crashes before flushing its in-memory buffer to S3.

## Problem Statement

Without WAL, unflushed data in the SegmentBuffer (in-memory) is **LOST** if an agent crashes before the segment is flushed to S3. This is the same fundamental issue as the "Glacier Kafka" multi-part PUT approach.

**Example scenario without WAL:**
```
Producer → Agent → SegmentBuffer (RAM) → [CRASH] → Data Lost!
```

## Solution Architecture

With WAL, records are first written to a local sequential log on disk before being added to the in-memory buffer:

```
Producer → Agent → WAL (disk) → SegmentBuffer (RAM) → S3
                    ↓
                (durable!)
```

On agent restart, the WAL is replayed to recover any unflushed records.

## File Format

Each WAL file is a sequence of records with CRC32 checksums for integrity verification:

```
[Record Entry 1][Record Entry 2]...[Record Entry N]

Record Entry:
┌─────────────┬──────────┬───────────┬──────────┬─────────┐
│ Record Size │ CRC32    │ Timestamp │ Key Size │ Key     │
│ (4 bytes)   │(4 bytes) │(8 bytes)  │(4 bytes) │(N bytes)│
└─────────────┴──────────┴───────────┴──────────┴─────────┘
┌────────────┬─────────┐
│ Value Size │ Value   │
│ (4 bytes)  │(M bytes)│
└────────────┴─────────┘
```

### CRC Calculation

The CRC32 is calculated over:
- Timestamp (8 bytes)
- Key size (4 bytes)
- Key data (N bytes)
- Value size (4 bytes)
- Value data (M bytes)

Corrupted records (CRC mismatch) are skipped during recovery with a warning.

## Configuration

### Environment Variables (Agent)

```bash
# Enable WAL
export WAL_ENABLED=true

# WAL directory (default: ./data/wal)
export WAL_DIR=/var/streamhouse/wal

# Fsync interval in milliseconds (default: 100ms)
export WAL_SYNC_INTERVAL_MS=100

# Max WAL file size in bytes (default: 1GB)
export WAL_MAX_SIZE=1073741824
```

### Programmatic Configuration

```rust
use streamhouse_storage::{WriteConfig, WALConfig, SyncPolicy};
use std::time::Duration;

let write_config = WriteConfig {
    // ... other config ...
    wal_config: Some(WALConfig {
        directory: "./data/wal".into(),
        sync_policy: SyncPolicy::Interval {
            interval: Duration::from_millis(100),
        },
        max_size_bytes: 1024 * 1024 * 1024, // 1GB
    }),
};
```

## Sync Policies

The sync policy controls when `fsync()` is called to ensure data is written to disk.

### 1. Always (Ultra-Safe)

```rust
sync_policy: SyncPolicy::Always
```

- **Fsync**: After every write
- **Durability**: Zero data loss (assuming disk doesn't lie)
- **Throughput**: ~50-100K records/sec (depends on disk)
- **Use case**: Financial transactions, critical logs

### 2. Interval (Balanced) - **Recommended**

```rust
sync_policy: SyncPolicy::Interval {
    interval: Duration::from_millis(100),
}
```

- **Fsync**: Every N milliseconds
- **Durability**: At most N milliseconds of data at risk
- **Throughput**: ~1M+ records/sec
- **Use case**: Most production workloads

**Common intervals:**
- 1ms: ~1-10 records at risk, ~500K records/sec
- 100ms: ~100-1000 records at risk, ~2M records/sec
- 1000ms: ~10K+ records at risk, ~2M+ records/sec

### 3. Never (Testing Only)

```rust
sync_policy: SyncPolicy::Never
```

- **Fsync**: Never
- **Durability**: All unflushed data at risk
- **Throughput**: ~2M+ records/sec
- **Use case**: Testing, development, non-critical data

**⚠️ WARNING**: Do not use in production unless you can tolerate complete data loss on crash.

## Recovery Process

### Automatic Recovery on Startup

When a PartitionWriter is created, it automatically:

1. Opens the WAL file for the topic/partition
2. Reads all records from the WAL
3. Validates CRC checksums
4. Replays valid records into the SegmentBuffer
5. Continues normal operations

```rust
// This happens automatically in PartitionWriter::new()
let writer = PartitionWriter::new(
    "orders".to_string(),
    0,  // partition_id
    object_store,
    metadata,
    write_config,  // Contains wal_config
).await?;

// If WAL exists, records are recovered and logged:
// "Recovering unflushed records from WAL: recovered=1523"
```

### WAL Lifecycle

1. **Append**: Each record is written to WAL before SegmentBuffer
2. **Fsync**: Based on sync policy (Always, Interval, Never)
3. **Segment Roll**: When segment is flushed to S3, WAL is truncated
4. **Recovery**: On restart, WAL is replayed to SegmentBuffer

```
Agent Start → WAL Recovery → Normal Operations → Segment Flush → WAL Truncate
```

## Performance Impact

### Overhead

- **Atomic operations**: ~5-10ns per write
- **Disk write**: ~10-50µs per record (buffered)
- **Fsync (SSD)**: ~100-500µs
- **Fsync (HDD)**: ~5-10ms

### Throughput with Different Sync Policies

| Sync Policy | Fsync Frequency | Throughput (SSD) | Data at Risk |
|-------------|-----------------|------------------|--------------|
| Always | Every write | 50-100K/sec | 0 records |
| Interval(1ms) | Every 1ms | 500K-1M/sec | ~1-10 records |
| Interval(100ms) | Every 100ms | 1-2M/sec | ~100-1K records |
| Interval(1000ms) | Every 1 second | 2M+/sec | ~10K+ records |
| Never | Never | 2M+/sec | All unflushed |

### Memory Usage

- **WAL struct**: ~200 bytes
- **Write buffer**: Minimal (records written immediately)
- **Recovery buffer**: Proportional to WAL size (typically < 100MB)

## Failure Scenarios

### 1. Agent Crash Before Segment Flush

**Without WAL:**
```
Producer sends 10K records → Agent buffers in RAM → CRASH → Lost all 10K records
```

**With WAL:**
```
Producer sends 10K records → WAL writes to disk → Agent buffers in RAM → CRASH
→ Restart → WAL recovery → All 10K records recovered ✓
```

### 2. Agent Crash During S3 Upload

**Without WAL:**
```
Agent flushes 1M records to S3 → Upload fails → CRASH → Lost all 1M records
```

**With WAL:**
```
Agent flushes 1M records to S3 → Upload fails → CRASH
→ Restart → WAL recovery → All 1M records recovered → Retry upload ✓
```

### 3. Partial WAL Record

If a WAL write is interrupted (e.g., power loss mid-write), the partial record is detected during recovery via CRC mismatch and skipped:

```
WAL: [Record 1][Record 2][Partial Record (corrupted)]
Recovery: ✓ Record 1, ✓ Record 2, ✗ Partial (CRC fail) → Skipped
```

### 4. Disk Full

If the WAL disk fills up, writes will fail with an I/O error. The agent should handle this gracefully by:

1. Logging the error
2. Attempting to flush the current segment to S3
3. Truncating the WAL to free space
4. Retrying the write

## Monitoring

### Log Messages

```
# WAL enabled
INFO WAL enabled:
INFO   Directory: /var/streamhouse/wal
INFO   Sync interval: 100ms
INFO   Max size: 1024MB

# Recovery on startup
INFO Recovering unflushed records from WAL: topic=orders, partition=0, recovered=1523
INFO WAL recovery complete: topic=orders, partition=0

# Segment flush
DEBUG WAL truncated after successful S3 upload: topic=orders, partition=0
INFO Segment rolled and uploaded to S3: base_offset=0, end_offset=9999
```

### Metrics to Monitor

- **WAL size**: Monitor disk usage in WAL directory
- **Recovery count**: Number of records recovered on startup
- **Fsync latency**: Time taken for fsync operations (if Always policy)
- **Throughput**: Records/sec with WAL enabled vs disabled

## Configuration Recommendations

### Production (Ultra-Safe)

For critical data where zero data loss is required:

```bash
export WAL_ENABLED=true
export WAL_DIR=/var/streamhouse/wal  # Separate disk recommended
export WAL_SYNC_INTERVAL_MS=1  # Fsync every 1ms
export WAL_MAX_SIZE=10737418240  # 10GB
```

**Trade-off**: Lower throughput (~500K records/sec) for maximum durability.

### Production (Balanced) - **Recommended**

For most production workloads:

```bash
export WAL_ENABLED=true
export WAL_DIR=/var/streamhouse/wal
export WAL_SYNC_INTERVAL_MS=100  # Fsync every 100ms
export WAL_MAX_SIZE=1073741824  # 1GB
```

**Trade-off**: At most ~100-1000 records at risk for high throughput (~2M records/sec).

### Development/Testing

For local development:

```bash
export WAL_ENABLED=false
# OR
export WAL_ENABLED=true
export WAL_SYNC_INTERVAL_MS=1000  # Fsync every 1 second
```

**Trade-off**: Faster local development, but data loss on crash is acceptable.

## Best Practices

### 1. Separate Disk for WAL

For maximum performance and reliability, use a separate disk for WAL:

```bash
# Mount dedicated SSD for WAL
mount /dev/nvme1n1 /var/streamhouse/wal
export WAL_DIR=/var/streamhouse/wal
```

**Benefits:**
- No contention with other disk I/O
- Easier to monitor WAL-specific metrics
- Can use faster (but smaller) SSD for WAL

### 2. Choose Sync Policy Based on SLA

| Use Case | Acceptable Data Loss | Recommended Policy |
|----------|---------------------|-------------------|
| Financial transactions | None | Always |
| User analytics | < 1 second | Interval(100ms) |
| Application logs | < 10 seconds | Interval(1000ms) |
| Development/testing | Acceptable | Never |

### 3. Monitor WAL Size

Set up monitoring for WAL directory size:

```bash
# Alert if WAL directory exceeds 5GB
du -sh /var/streamhouse/wal
```

Large WAL files indicate:
- Segments not flushing to S3 (check S3 connectivity)
- Very high write throughput
- Potential disk space issues

### 4. Test Recovery

Periodically test WAL recovery by:

1. Killing an agent process
2. Restarting and verifying recovery logs
3. Confirming no data loss

```bash
# Kill agent
pkill -9 agent

# Restart and check logs
./agent 2>&1 | grep "Recovering unflushed records"
```

## Disabling WAL

To disable WAL (not recommended for production):

```bash
export WAL_ENABLED=false
```

or programmatically:

```rust
let write_config = WriteConfig {
    // ... other config ...
    wal_config: None,  // WAL disabled
};
```

**⚠️ WARNING**: Without WAL, unflushed data is lost on agent crash!

## Comparison with Kafka

| Feature | Kafka | StreamHouse WAL |
|---------|-------|-----------------|
| Durability mechanism | Log segments on disk | WAL + S3 segments |
| Fsync control | `log.flush.interval.messages` | `SyncPolicy` |
| Recovery | Read from disk log | Replay WAL → S3 |
| Long-term storage | Disk (with retention) | S3 (infinite retention) |
| Throughput | ~1M msgs/sec/partition | ~2M records/sec/partition |

StreamHouse WAL provides similar durability guarantees to Kafka while using S3 for long-term storage.

## Troubleshooting

### Issue: "Corrupted WAL record (CRC mismatch)"

**Cause**: Partial write during crash or disk corruption

**Solution**: The record is automatically skipped during recovery. Check disk health if this happens frequently.

```bash
# Check disk health (Linux)
smartctl -a /dev/sda
```

### Issue: "WAL directory not found"

**Cause**: WAL directory doesn't exist and can't be created

**Solution**: Ensure the parent directory exists and has correct permissions:

```bash
mkdir -p /var/streamhouse/wal
chown streamhouse:streamhouse /var/streamhouse/wal
chmod 755 /var/streamhouse/wal
```

### Issue: High recovery time on startup

**Cause**: Large WAL file with many unflushed records

**Solution**:
1. Reduce `SEGMENT_MAX_AGE_MS` to flush segments more frequently
2. Increase `SEGMENT_MAX_SIZE` to reduce number of segments
3. Check S3 connectivity if segments aren't uploading

### Issue: Disk space running out

**Cause**: WAL not being truncated after S3 uploads

**Solution**:
1. Check S3 upload errors in logs
2. Verify S3 credentials and connectivity
3. Manually flush and truncate WAL:

```rust
writer.flush().await?;
// WAL is automatically truncated after successful flush
```

## Example: Complete Setup

```bash
#!/bin/bash
# Production StreamHouse Agent with WAL

# Create WAL directory
mkdir -p /var/streamhouse/wal
chown streamhouse:streamhouse /var/streamhouse/wal

# Agent configuration
export AGENT_ID=agent-001
export AGENT_ADDRESS=0.0.0.0:9090
export METADATA_STORE=postgresql://user:pass@localhost/streamhouse
export AWS_REGION=us-east-1
export STREAMHOUSE_BUCKET=my-streamhouse-bucket

# Segment settings
export SEGMENT_MAX_SIZE=67108864  # 64MB
export SEGMENT_MAX_AGE_MS=600000  # 10 minutes

# WAL settings (Balanced mode)
export WAL_ENABLED=true
export WAL_DIR=/var/streamhouse/wal
export WAL_SYNC_INTERVAL_MS=100  # 100ms
export WAL_MAX_SIZE=1073741824   # 1GB

# Start agent
./streamhouse-agent
```

## Future Enhancements

Potential future improvements to the WAL implementation:

1. **WAL rotation**: Rotate WAL files based on size/time instead of truncating
2. **Compression**: Compress WAL entries to reduce disk usage
3. **Replication**: Replicate WAL to remote disk for disaster recovery
4. **Batch fsync**: Fsync multiple writes in a single syscall
5. **Direct I/O**: Bypass page cache for lower latency

---

For more information, see:
- [Architecture Documentation](./architecture.md)
- [Storage Layer Design](./storage.md)
- [Agent Configuration](./agent-config.md)
