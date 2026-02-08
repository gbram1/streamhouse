# WAL Performance Tradeoffs

**Date**: Feb 8, 2026
**Context**: Phase 12.4.5 (Shared WAL) reduced failover data loss from 0.02% to 0.0032%,
but throughput dropped from ~1.5M msg/s (WAL disabled) to ~310K msg/s (WAL enabled).

This document describes the options for recovering throughput while preserving durability.

---

## Current Architecture

```
produce_batch(2000 records)
  └─ for each record:
       ├─ wal.append()          ← channel send (~50ns each, fire-and-forget)
       └─ segment.append()      ← in-memory buffer

WAL writer task (background):
  └─ drain channel → accumulate batch → write_all → fdatasync
     triggered by: batch_max_records(1000) OR batch_max_bytes(1MB) OR batch_max_age_ms(10ms)
```

### Why WAL Costs 5x Throughput

Three costs stack up:

1. **Per-record serialization + channel send**: `wal.append()` is called 2000 times per
   produce_batch (once per record). Each call allocates a `Vec<u8>`, serializes the record,
   and sends through the mpsc channel. That's 2000 allocations + 2000 channel sends per batch.

2. **fdatasync frequency**: With `batch_max_records=1000`, a 2000-record produce_batch triggers
   ~2 fdatasyncs in the writer task. Each fdatasync on macOS APFS takes ~200-500us.
   On NFS/EFS it's 1-5ms.

3. **Channel contention**: 4 producer tasks x 8 partitions = up to 32 concurrent WAL channel
   senders competing for writer task drain cycles.

---

## Options

### Option 1: Batch WAL Writes in produce_batch (Code Change)

**What**: Instead of calling `wal.append()` 2000 times per produce_batch, collect all records
and call `wal.append_batch()` once. This sends a single pre-serialized buffer through the
channel and does one fdatasync for the entire gRPC batch.

**Change**: `crates/streamhouse-server/src/services/mod.rs` — accumulate WAL records, then call
`wal.append_batch()` after the loop. Requires exposing the WAL through PartitionWriter or adding
an `append_batch()` method to PartitionWriter itself.

**Expected throughput**: ~800K-1.2M msg/s (based on WAL standalone benchmark of 2.21M rec/s
with group commit). The single fdatasync per 2000-record batch amortizes the sync cost.

**Durability**: Same as current. All records in the batch are durable after the fdatasync.

**Loss window on kill -9**: Same as current (~1 batch worth of records in the channel).

| Metric | Before | After |
|--------|--------|-------|
| Throughput | 310K msg/s | ~800K-1.2M msg/s |
| fdatasyncs per batch | ~2 | 1 |
| Channel sends per batch | 2000 | 1 |
| Vec allocations per batch | 2000 | 1 |
| Durability | Full | Full |

**Verdict**: Best option. Highest throughput recovery with zero durability compromise.
This is a bug-level inefficiency — the batch WAL path already exists but isn't used.

---

### Option 2: Increase Batch Thresholds (Config Change)

**What**: Increase `batch_max_records` and `batch_max_bytes` so the writer task does fewer
fdatasyncs. Larger batches = fewer syncs = higher throughput.

**Config changes** (env vars or WALConfig):
```
batch_max_records: 1000 → 5000
batch_max_bytes:   1MB  → 4MB
batch_max_age_ms:  10   → 50
```

**Expected throughput**: ~400-500K msg/s. Fewer fdatasyncs but still 2000 channel sends per
produce_batch (Option 1 addresses this).

**Durability**: Same — records are still written to WAL before ack.

**Loss window on kill -9**: Larger. Up to 5000 records (vs 1000) could be in the unflushed
batch when the process dies. On shared WAL, the surviving agent recovers everything that
was fdatasynced — the loss is only the in-flight batch.

| batch_max_records | fdatasyncs/batch | Est. throughput | Kill -9 loss window |
|-------------------|------------------|-----------------|---------------------|
| 1000 (current) | ~2 | 310K msg/s | ~1000 records |
| 2000 | ~1 | ~400K msg/s | ~2000 records |
| 5000 | ~0.4 | ~500K msg/s | ~5000 records |
| 10000 | ~0.2 | ~550K msg/s | ~10000 records |

**Verdict**: Easy config-only change. Moderate throughput gain. Combines well with Option 1.

---

### Option 3: SyncPolicy::Never (No fdatasync)

**What**: Set `sync_policy: Never` to skip fdatasync entirely. The WAL still writes to the
file, but relies on the OS page cache to flush to disk asynchronously.

**Config**: `WAL_SYNC_POLICY=never` (would need env var support, currently only configurable
via WALConfig struct).

**Expected throughput**: ~1.0-1.3M msg/s. Eliminates the fdatasync bottleneck entirely.
Remaining cost is serialization + channel sends (addressed by Option 1).

**Durability**: Weaker. On a clean `kill -9`, the OS usually flushes dirty pages within
~30 seconds (Linux default `dirty_expire_centisecs`). But there's no guarantee — a kernel
panic or power loss could lose the entire page cache.

**Loss window**:
- `kill -9` (process crash): Usually zero loss (OS flushes pages), but up to ~30s of writes
  could be lost if the OS hasn't flushed yet
- Kernel panic / power loss: Up to 30 seconds of writes lost
- On shared WAL (EFS/NFS): EFS has its own durability guarantees; `close()` semantics on NFS
  force a flush, so process crash recovery works, but in-flight writes at crash time may be lost

**Verdict**: Good for development/testing. Not recommended for production unless combined
with shared WAL on a storage system that has its own durability guarantees (like EFS).

---

### Option 4: Deferred WAL Ack (Ack Before Durable)

**What**: Acknowledge the produce request immediately after writing to the in-memory segment
buffer. The WAL write happens in the background (fire-and-forget channel send). Don't wait
for the WAL batch to flush before returning to the producer.

**This is actually what's already happening** in batched mode (`batch_enabled=true`). The
`wal.append()` call sends to the channel and returns immediately — it does NOT wait for
fdatasync. The throughput bottleneck is the 2000 individual channel sends + serializations,
not waiting for fdatasync.

**Verdict**: Already implemented. The channel send overhead (not fdatasync) is the bottleneck.
Option 1 directly solves this.

---

### Option 5: Two-Tier Durability (Producer-Selectable)

**What**: Let producers choose their durability guarantee per request:

- `acks=0` (fire-and-forget): Don't wait for anything. Max throughput, risk of loss.
- `acks=1` (default): Write to segment buffer + WAL channel send. Current behavior.
- `acks=all` (durable): Write to WAL + wait for fdatasync before ack.

This mirrors Kafka's `acks` setting. The proto already has placeholder fields for this
(`producer_id`, `producer_epoch`), and adding an `acks` field to `ProduceBatchRequest`
is straightforward.

**Expected throughput**:
- `acks=0`: ~1.5M msg/s (same as no WAL)
- `acks=1`: ~800K-1.2M msg/s (with Option 1)
- `acks=all`: ~200-400K msg/s (fdatasync in hot path)

**Verdict**: Good long-term feature. Lets users choose their own throughput/durability
tradeoff per topic or per producer. Moderate implementation effort (~1-2 days).

---

### Option 6: WAL on tmpfs/ramfs (RAM-Backed WAL)

**What**: Mount the WAL directory on tmpfs (Linux) or a RAM disk. Writes go to RAM only —
no disk I/O at all. The shared WAL on EFS/NFS provides the cross-node durability; the local
WAL is only for same-node restart recovery.

```bash
# Linux
mount -t tmpfs -o size=2G tmpfs /data/wal

# macOS (development)
diskutil erasevolume HFS+ 'RAMDisk' `hdiutil attach -nomount ram://4194304`
```

**Expected throughput**: ~1.3-1.5M msg/s (near WAL-disabled levels). write_all to RAM is
effectively free; fdatasync on tmpfs is a no-op.

**Durability**:
- Same-node restart (process crash): Full recovery (RAM persists across process restarts)
- Same-node reboot: Complete WAL loss (RAM is volatile)
- Cross-node failover: No protection (tmpfs is local)

**This defeats the purpose of shared WAL** — it's only useful if you have a separate
replication mechanism or don't care about same-node reboot durability (process crashes are
the common case, not server reboots).

**Verdict**: Niche. Only useful for development or if you accept that server reboots lose
unflushed data (process crashes are still handled).

---

## Recommended Approach

Combine **Option 1 + Option 2** for the best result:

| Step | Change | Effort | Throughput gain |
|------|--------|--------|-----------------|
| 1 | **Batch WAL writes in produce_batch** (Option 1) | ~2-3 hours | 310K → ~800K-1.2M msg/s |
| 2 | **Increase batch thresholds** (Option 2) | Config only | +10-20% on top |
| 3 | **Two-tier acks** (Option 5) | ~1-2 days | Lets users choose |

### Expected Result After Option 1 + 2

```
WAL disabled:  ~1.5M msg/s  (no durability)
WAL enabled:   ~310K msg/s  (current, per-record channel sends)
After fix:     ~1.0M msg/s  (batch WAL, larger thresholds)
```

That's ~67% of WAL-disabled throughput with full shared WAL durability — a reasonable
tradeoff for zero-loss failover.

### Implementation Sketch for Option 1

Add `append_batch` to `PartitionWriter`:

```rust
// In PartitionWriter
pub async fn append_batch(
    &mut self,
    records: Vec<(Option<Bytes>, Bytes, u64)>,
) -> Result<(u64, u64)> {  // returns (first_offset, last_offset)
    let first_offset = self.current_offset;

    // Step 1: Write all records to WAL as single batch
    if let Some(ref wal) = self.wal {
        let wal_records: Vec<(Option<&[u8]>, &[u8])> = records.iter()
            .map(|(k, v, _)| (k.as_deref(), v.as_ref()))
            .collect();
        wal.append_batch(&wal_records).await?;
    }

    // Step 2: Append all records to segment buffer
    for (key, value, timestamp) in records {
        let offset = self.current_offset;
        self.current_offset += 1;
        let record = Record::new(offset, timestamp, key, value);
        self.current_segment.append(&record)
            .map_err(|e| Error::SegmentError(e.to_string()))?;
    }

    let last_offset = self.current_offset - 1;
    Ok((first_offset, last_offset))
}
```

Then update `produce_batch` in the gRPC service to call `writer_guard.append_batch()`
instead of looping `writer_guard.append()`.
