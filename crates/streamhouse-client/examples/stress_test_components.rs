//! Write Path Stress Test with Full Observability
//!
//! Writes 1M records through the full write path (WAL -> SegmentWriter -> S3),
//! then reads every segment back to verify zero data loss. Collects comprehensive
//! stats on every layer: latency percentiles, segment inventory, WAL throughput,
//! compression ratios, and data integrity verification.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p streamhouse-client --example stress_test_components --release
//! ```

use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use streamhouse_core::segment::Compression;
use streamhouse_core::Record;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};
use streamhouse_storage::wal::{SyncPolicy, WALConfig, WAL};
use streamhouse_storage::{PartitionWriter, SegmentReader, SegmentWriter, WriteConfig};

// =============================================================================
// Configuration
// =============================================================================

const TOTAL_RECORDS: u64 = 1_000_000;
const VALUE_SIZE: usize = 128;
const PARTITION_COUNT: u32 = 8;
const SEGMENT_MAX_SIZE: usize = 4 * 1024 * 1024; // 4MB segments -> many rolls
const LATENCY_SAMPLE_RATE: u64 = 50; // sample every Nth append

// =============================================================================
// Stats structs
// =============================================================================

#[derive(Default)]
struct SegmentWriterStats {
    records: u64,
    elapsed: Duration,
    segment_bytes: usize,
    raw_bytes: usize,
    block_count: u32,
}

#[derive(Default)]
struct WalStats {
    records: u64,
    elapsed: Duration,
    wal_bytes: u64,
}

struct PartitionStats {
    partition_id: u32,
    records_written: u64,
    first_offset: u64,
    last_offset: u64,
    segments_written: u32,
    total_segment_bytes: u64,
    total_raw_bytes: u64,
}

struct VerificationResult {
    partition_id: u32,
    segments_read: u32,
    records_verified: u64,
    offset_gaps: Vec<(u64, u64)>, // (expected, got)
    first_offset: Option<u64>,
    last_offset: Option<u64>,
}

struct FullRunStats {
    // Overall
    total_records_written: u64,
    total_elapsed: Duration,
    // Per-second throughput snapshots
    throughput_timeline: Vec<(u64, u64)>, // (second, records_this_second)
    // Append latency (sampled)
    append_latencies_us: Vec<u64>,
    // Per-partition write stats
    partition_stats: Vec<PartitionStats>,
    // Segment inventory
    segment_inventory: Vec<SegmentDetail>,
    // Verification
    verification: Vec<VerificationResult>,
    // WAL
    wal_stats: Option<WalStats>,
    // SegmentWriter micro-bench
    seg_writer_stats: SegmentWriterStats,
}

struct SegmentDetail {
    partition_id: u32,
    base_offset: u64,
    end_offset: u64,
    record_count: u32,
    size_bytes: u64,
    s3_key: String,
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_target(false)
        .init();

    println!();
    println!("================================================================");
    println!("  StreamHouse Full Write Path Stress Test");
    println!("================================================================");
    println!();
    println!("  Records:         {}", fmt(TOTAL_RECORDS));
    println!("  Value size:      {} bytes", VALUE_SIZE);
    println!("  Partitions:      {}", PARTITION_COUNT);
    println!("  Segment limit:   {} MB", SEGMENT_MAX_SIZE / (1024 * 1024));
    println!("  Latency sample:  1/{}", LATENCY_SAMPLE_RATE);
    println!();

    let value = Bytes::from(vec![b'x'; VALUE_SIZE]);

    // -----------------------------------------------------------------
    // Phase 1: SegmentWriter micro-benchmark (baseline)
    // -----------------------------------------------------------------
    println!("[1/5] SegmentWriter baseline ...");
    let seg_stats = bench_segment_writer(&value);
    println!(
        "       {} rec/s | segment {:.1} MB | {:.1}% compression",
        fmt(per_sec(seg_stats.records, seg_stats.elapsed)),
        seg_stats.segment_bytes as f64 / MB,
        seg_stats.segment_bytes as f64 / seg_stats.raw_bytes as f64 * 100.0,
    );

    // -----------------------------------------------------------------
    // Phase 2: WAL micro-benchmark
    // -----------------------------------------------------------------
    println!("[2/5] WAL baseline ...");
    let wal_stats = bench_wal(&value).await?;
    println!(
        "       {} rec/s | WAL {:.1} MB | {:.1} MB/s disk",
        fmt(per_sec(wal_stats.records, wal_stats.elapsed)),
        wal_stats.wal_bytes as f64 / MB,
        wal_stats.wal_bytes as f64 / wal_stats.elapsed.as_secs_f64() / MB,
    );

    // -----------------------------------------------------------------
    // Phase 3: Full write path (multi-partition with WAL)
    // -----------------------------------------------------------------
    println!("[3/5] Writing {} records across {} partitions (with WAL) ...", fmt(TOTAL_RECORDS), PARTITION_COUNT);
    let mut run = run_full_write_path(&value).await?;
    run.seg_writer_stats = seg_stats;
    run.wal_stats = Some(wal_stats);
    println!(
        "       {} rec/s | {:.2}s elapsed",
        fmt(per_sec(run.total_records_written, run.total_elapsed)),
        run.total_elapsed.as_secs_f64(),
    );

    // -----------------------------------------------------------------
    // Phase 4: Verification (read everything back)
    // -----------------------------------------------------------------
    println!("[4/5] Verifying all segments (reading back from S3) ...");
    // (verification is already done inside run_full_write_path)
    let total_verified: u64 = run.verification.iter().map(|v| v.records_verified).sum();
    let total_gaps: usize = run.verification.iter().map(|v| v.offset_gaps.len()).sum();
    println!(
        "       {} records verified | {} offset gaps",
        fmt(total_verified),
        total_gaps,
    );

    // -----------------------------------------------------------------
    // Phase 5: Full report
    // -----------------------------------------------------------------
    println!("[5/5] Generating report ...");
    println!();
    print_full_report(&run);

    Ok(())
}

// =============================================================================
// Phase 1: SegmentWriter micro-benchmark
// =============================================================================

fn bench_segment_writer(value: &Bytes) -> SegmentWriterStats {
    let mut writer = SegmentWriter::new(Compression::Lz4);
    let ts_base: u64 = 1_700_000_000_000;
    let records = TOTAL_RECORDS;

    let start = Instant::now();
    for i in 0..records {
        let record = Record::new(
            i,
            ts_base + i,
            Some(Bytes::from(format!("k{}", i % 10000))),
            value.clone(),
        );
        writer.append(&record).unwrap();
    }
    let segment_bytes = writer.finish().unwrap();
    let elapsed = start.elapsed();

    // Parse block count from header (bytes 28..32, big-endian u32)
    let block_count = u32::from_be_bytes(segment_bytes[28..32].try_into().unwrap());

    SegmentWriterStats {
        records,
        elapsed,
        segment_bytes: segment_bytes.len(),
        raw_bytes: records as usize * (VALUE_SIZE + 16 + 6), // ~overhead for key
        block_count,
    }
}

// =============================================================================
// Phase 2: WAL micro-benchmark
// =============================================================================

async fn bench_wal(value: &Bytes) -> anyhow::Result<WalStats> {
    let temp_dir = tempfile::tempdir()?;
    let config = WALConfig {
        directory: temp_dir.path().to_path_buf(),
        sync_policy: SyncPolicy::Interval {
            interval: Duration::from_millis(10),
        },
        max_size_bytes: 4 * 1024 * 1024 * 1024,
        batch_enabled: true,
        batch_max_records: 5000,
        batch_max_bytes: 4 * 1024 * 1024,
        batch_max_age_ms: 5,
        agent_id: None,
    };

    let wal = WAL::open("wal-bench", 0, config).await?;
    let key = b"bench-key";

    let start = Instant::now();
    for _ in 0..TOTAL_RECORDS {
        wal.append(Some(key.as_slice()), value.as_ref()).await?;
    }
    wal.flush_batch().await?;
    let elapsed = start.elapsed();

    Ok(WalStats {
        records: TOTAL_RECORDS,
        elapsed,
        wal_bytes: wal.size().await,
    })
}

// =============================================================================
// Phase 3: Full write path
// =============================================================================

async fn run_full_write_path(value: &Bytes) -> anyhow::Result<FullRunStats> {
    // --- Setup ---
    let metadata = Arc::new(SqliteMetadataStore::new_in_memory().await?);
    let topic = "stress-full";

    metadata
        .create_topic(TopicConfig {
            name: topic.to_string(),
            partition_count: PARTITION_COUNT,
            retention_ms: None,
            cleanup_policy: Default::default(),
            config: Default::default(),
        })
        .await?;

    let wal_dir = tempfile::tempdir()?;
    let object_store: Arc<dyn object_store::ObjectStore> =
        Arc::new(object_store::memory::InMemory::new());

    let write_config = WriteConfig {
        segment_max_size: SEGMENT_MAX_SIZE,
        segment_max_age_ms: 600_000,
        s3_bucket: "stress-bucket".to_string(),
        s3_region: "us-east-1".to_string(),
        wal_config: Some(WALConfig {
            directory: wal_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Interval {
                interval: Duration::from_millis(10),
            },
            max_size_bytes: 4 * 1024 * 1024 * 1024,
            batch_enabled: true,
            batch_max_records: 5000,
            batch_max_bytes: 4 * 1024 * 1024,
            batch_max_age_ms: 5,
            agent_id: None,
        }),
        ..Default::default()
    };

    // Create per-partition writers
    let mut writers: Vec<PartitionWriter> = Vec::new();
    for pid in 0..PARTITION_COUNT {
        writers.push(
            PartitionWriter::new(
                topic.to_string(),
                pid,
                object_store.clone(),
                metadata.clone(),
                write_config.clone(),
            )
            .await?,
        );
    }

    // --- Write phase ---
    let ts_base: u64 = 1_700_000_000_000;
    let key = Bytes::from("stress-key");
    let mut append_latencies_us: Vec<u64> = Vec::with_capacity((TOTAL_RECORDS / LATENCY_SAMPLE_RATE) as usize + 1);
    let mut throughput_timeline: Vec<(u64, u64)> = Vec::new();
    let mut records_this_second = 0u64;
    let mut current_second = 0u64;

    let run_start = Instant::now();
    let mut second_start = Instant::now();

    for i in 0..TOTAL_RECORDS {
        let pid = (i % PARTITION_COUNT as u64) as usize;

        let sample = (i % LATENCY_SAMPLE_RATE) == 0;
        let t0 = if sample { Instant::now() } else { run_start }; // reuse to avoid syscall

        writers[pid]
            .append(Some(key.clone()), value.clone(), ts_base + i)
            .await?;

        if sample {
            append_latencies_us.push(t0.elapsed().as_micros() as u64);
        }

        records_this_second += 1;

        // Per-second snapshot
        if second_start.elapsed() >= Duration::from_secs(1) {
            current_second += 1;
            throughput_timeline.push((current_second, records_this_second));
            records_this_second = 0;
            second_start = Instant::now();
        }
    }
    // final partial second
    if records_this_second > 0 {
        current_second += 1;
        throughput_timeline.push((current_second, records_this_second));
    }

    // Flush all partitions
    for w in &mut writers {
        w.flush_durable().await?;
    }
    let total_elapsed = run_start.elapsed();

    // --- Collect partition & segment stats ---
    let mut partition_stats: Vec<PartitionStats> = Vec::new();
    let mut segment_inventory: Vec<SegmentDetail> = Vec::new();

    for pid in 0..PARTITION_COUNT {
        let segments = metadata.get_segments(topic, pid).await?;
        let total_seg_bytes: u64 = segments.iter().map(|s| s.size_bytes).sum();
        let total_records_in_segs: u64 = segments.iter().map(|s| s.record_count as u64).sum();

        let first_offset = segments.first().map(|s| s.base_offset).unwrap_or(0);
        let last_offset = segments.last().map(|s| s.end_offset).unwrap_or(0);

        let raw_bytes = total_records_in_segs * (VALUE_SIZE as u64 + 16);

        partition_stats.push(PartitionStats {
            partition_id: pid,
            records_written: total_records_in_segs,
            first_offset,
            last_offset,
            segments_written: segments.len() as u32,
            total_segment_bytes: total_seg_bytes,
            total_raw_bytes: raw_bytes,
        });

        for seg in &segments {
            segment_inventory.push(SegmentDetail {
                partition_id: pid,
                base_offset: seg.base_offset,
                end_offset: seg.end_offset,
                record_count: seg.record_count,
                size_bytes: seg.size_bytes,
                s3_key: seg.s3_key.clone(),
            });
        }
    }

    // --- Verification: read back every segment ---
    let mut verification: Vec<VerificationResult> = Vec::new();

    for pid in 0..PARTITION_COUNT {
        let segments = metadata.get_segments(topic, pid).await?;
        let mut records_verified = 0u64;
        let mut offset_gaps: Vec<(u64, u64)> = Vec::new();
        let mut expected_offset: Option<u64> = None;
        let mut first_offset: Option<u64> = None;
        let mut last_offset: Option<u64> = None;

        for seg in &segments {
            let path = object_store::path::Path::from(seg.s3_key.as_str());
            let data = object_store.get(&path).await?.bytes().await?;
            let reader = SegmentReader::new(data)?;
            let records = reader.read_all()?;

            if first_offset.is_none() && !records.is_empty() {
                first_offset = Some(records[0].offset);
            }
            for rec in &records {
                if let Some(exp) = expected_offset {
                    if rec.offset != exp {
                        offset_gaps.push((exp, rec.offset));
                    }
                }
                expected_offset = Some(rec.offset + 1);
                last_offset = Some(rec.offset);
                records_verified += 1;
            }
        }

        verification.push(VerificationResult {
            partition_id: pid,
            segments_read: segments.len() as u32,
            records_verified,
            offset_gaps,
            first_offset,
            last_offset,
        });
    }

    Ok(FullRunStats {
        total_records_written: TOTAL_RECORDS,
        total_elapsed,
        throughput_timeline,
        append_latencies_us,
        partition_stats,
        segment_inventory,
        verification,
        wal_stats: None,       // filled by caller
        seg_writer_stats: Default::default(), // filled by caller
    })
}

// =============================================================================
// Report printer
// =============================================================================

const MB: f64 = 1024.0 * 1024.0;

fn print_full_report(run: &FullRunStats) {
    let total = run.total_records_written;
    let elapsed = run.total_elapsed.as_secs_f64();
    let throughput = total as f64 / elapsed;
    let raw_data_bytes = total as f64 * (VALUE_SIZE + 16) as f64;

    // =====================================================================
    println!("================================================================");
    println!("  WRITE PERFORMANCE");
    println!("================================================================");
    println!("  Total records:     {}", fmt(total));
    println!("  Elapsed:           {:.3}s", elapsed);
    println!("  Throughput:        {} rec/s", fmt(throughput as u64));
    println!("  Raw data rate:     {:.1} MB/s", raw_data_bytes / elapsed / MB);
    println!();

    // Per-second throughput timeline
    if !run.throughput_timeline.is_empty() {
        println!("  Per-second throughput:");
        for (sec, count) in &run.throughput_timeline {
            let bar_len = (*count as f64 / 100_000.0).min(50.0) as usize;
            let bar: String = "|".repeat(bar_len);
            println!("    {:>3}s  {:>8} rec/s  {}", sec, fmt(*count), bar);
        }
        println!();
    }

    // =====================================================================
    println!("================================================================");
    println!("  APPEND LATENCY (sampled 1/{})", LATENCY_SAMPLE_RATE);
    println!("================================================================");
    if !run.append_latencies_us.is_empty() {
        let mut sorted = run.append_latencies_us.clone();
        sorted.sort();
        let len = sorted.len();
        let p = |pct: f64| -> u64 {
            let idx = ((pct / 100.0) * len as f64).ceil() as usize;
            sorted[idx.min(len - 1)]
        };
        let avg = sorted.iter().sum::<u64>() as f64 / len as f64;

        println!("  Samples:   {}", fmt(len as u64));
        println!("  avg:       {:.1} us", avg);
        println!("  p50:       {} us", p(50.0));
        println!("  p90:       {} us", p(90.0));
        println!("  p95:       {} us", p(95.0));
        println!("  p99:       {} us", p(99.0));
        println!("  p99.9:     {} us", p(99.9));
        println!("  max:       {} us", sorted[len - 1]);
    }
    println!();

    // =====================================================================
    println!("================================================================");
    println!("  SEGMENT WRITER BASELINE (pure CPU, LZ4)");
    println!("================================================================");
    let sw = &run.seg_writer_stats;
    if sw.records > 0 {
        println!("  Records:         {}", fmt(sw.records));
        println!("  Throughput:      {} rec/s", fmt(per_sec(sw.records, sw.elapsed)));
        println!("  Blocks:          {}", sw.block_count);
        println!("  Raw size:        {:.1} MB", sw.raw_bytes as f64 / MB);
        println!("  Segment size:    {:.1} MB", sw.segment_bytes as f64 / MB);
        println!(
            "  Compression:     {:.1}% ({:.1}x ratio)",
            sw.segment_bytes as f64 / sw.raw_bytes as f64 * 100.0,
            sw.raw_bytes as f64 / sw.segment_bytes as f64,
        );
    }
    println!();

    // =====================================================================
    println!("================================================================");
    println!("  WAL BASELINE (Interval 10ms, batched)");
    println!("================================================================");
    if let Some(ref ws) = run.wal_stats {
        println!("  Records:         {}", fmt(ws.records));
        println!("  Throughput:      {} rec/s", fmt(per_sec(ws.records, ws.elapsed)));
        println!("  WAL size:        {:.1} MB", ws.wal_bytes as f64 / MB);
        println!(
            "  Disk rate:       {:.1} MB/s",
            ws.wal_bytes as f64 / ws.elapsed.as_secs_f64() / MB
        );
        println!("  Elapsed:         {:.3}s", ws.elapsed.as_secs_f64());
        // overhead per record
        let overhead_ns = ws.elapsed.as_nanos() as f64 / ws.records as f64;
        println!("  Per-record cost: {:.0} ns", overhead_ns);
    }
    println!();

    // =====================================================================
    println!("================================================================");
    println!("  PARTITION STATS");
    println!("================================================================");
    println!(
        "  {:>4}  {:>10}  {:>8}  {:>14}  {:>10}  {:>10}  {:>8}",
        "P#", "Records", "Segments", "Offsets", "Seg MB", "Raw MB", "Ratio"
    );
    println!("  {}", "-".repeat(74));
    let mut total_segments = 0u32;
    let mut total_seg_bytes = 0u64;
    let mut total_raw_bytes = 0u64;
    for ps in &run.partition_stats {
        total_segments += ps.segments_written;
        total_seg_bytes += ps.total_segment_bytes;
        total_raw_bytes += ps.total_raw_bytes;
        let ratio = if ps.total_raw_bytes > 0 {
            ps.total_segment_bytes as f64 / ps.total_raw_bytes as f64
        } else {
            0.0
        };
        println!(
            "  {:>4}  {:>10}  {:>8}  {:>6}-{:<6}  {:>10.2}  {:>10.2}  {:>7.1}%",
            ps.partition_id,
            fmt(ps.records_written),
            ps.segments_written,
            ps.first_offset,
            ps.last_offset,
            ps.total_segment_bytes as f64 / MB,
            ps.total_raw_bytes as f64 / MB,
            ratio * 100.0,
        );
    }
    println!("  {}", "-".repeat(74));
    let total_ratio = if total_raw_bytes > 0 {
        total_seg_bytes as f64 / total_raw_bytes as f64
    } else {
        0.0
    };
    let total_recs: u64 = run.partition_stats.iter().map(|p| p.records_written).sum();
    println!(
        "  {:>4}  {:>10}  {:>8}  {:>14}  {:>10.2}  {:>10.2}  {:>7.1}%",
        "SUM",
        fmt(total_recs),
        total_segments,
        "",
        total_seg_bytes as f64 / MB,
        total_raw_bytes as f64 / MB,
        total_ratio * 100.0,
    );
    println!();

    // =====================================================================
    println!("================================================================");
    println!("  SEGMENT INVENTORY (all {} segments)", run.segment_inventory.len());
    println!("================================================================");
    if run.segment_inventory.len() <= 40 {
        println!(
            "  {:>4}  {:>10}  {:>10}  {:>8}  {:>10}  {}",
            "P#", "Base", "End", "Records", "Size KB", "S3 Key"
        );
        println!("  {}", "-".repeat(80));
        for sd in &run.segment_inventory {
            println!(
                "  {:>4}  {:>10}  {:>10}  {:>8}  {:>10.1}  {}",
                sd.partition_id,
                sd.base_offset,
                sd.end_offset,
                sd.record_count,
                sd.size_bytes as f64 / 1024.0,
                sd.s3_key,
            );
        }
    } else {
        // Summarize when there are too many segments
        let mut by_partition: HashMap<u32, Vec<&SegmentDetail>> = HashMap::new();
        for sd in &run.segment_inventory {
            by_partition.entry(sd.partition_id).or_default().push(sd);
        }
        println!(
            "  {:>4}  {:>8}  {:>12}  {:>12}  {:>12}",
            "P#", "Segments", "Min Size KB", "Max Size KB", "Avg Size KB"
        );
        println!("  {}", "-".repeat(60));
        let mut pids: Vec<u32> = by_partition.keys().copied().collect();
        pids.sort();
        for pid in pids {
            let segs = &by_partition[&pid];
            let sizes: Vec<f64> = segs.iter().map(|s| s.size_bytes as f64 / 1024.0).collect();
            let min = sizes.iter().cloned().fold(f64::MAX, f64::min);
            let max = sizes.iter().cloned().fold(0.0f64, f64::max);
            let avg = sizes.iter().sum::<f64>() / sizes.len() as f64;
            println!(
                "  {:>4}  {:>8}  {:>12.1}  {:>12.1}  {:>12.1}",
                pid,
                segs.len(),
                min,
                max,
                avg,
            );
        }
    }

    // Segment size distribution
    let sizes: Vec<u64> = run.segment_inventory.iter().map(|s| s.size_bytes).collect();
    if !sizes.is_empty() {
        let min = *sizes.iter().min().unwrap();
        let max = *sizes.iter().max().unwrap();
        let avg = sizes.iter().sum::<u64>() as f64 / sizes.len() as f64;
        let records_per: Vec<u32> = run.segment_inventory.iter().map(|s| s.record_count).collect();
        let avg_recs = records_per.iter().map(|r| *r as f64).sum::<f64>() / records_per.len() as f64;
        println!();
        println!("  Size distribution:");
        println!("    min:  {:.1} KB", min as f64 / 1024.0);
        println!("    avg:  {:.1} KB", avg / 1024.0);
        println!("    max:  {:.1} KB", max as f64 / 1024.0);
        println!("    avg records/segment: {:.0}", avg_recs);
    }
    println!();

    // =====================================================================
    println!("================================================================");
    println!("  DATA INTEGRITY VERIFICATION");
    println!("================================================================");
    let mut all_ok = true;
    let mut total_verified = 0u64;
    for v in &run.verification {
        total_verified += v.records_verified;
        let status = if v.offset_gaps.is_empty() {
            "OK"
        } else {
            all_ok = false;
            "GAPS"
        };
        println!(
            "  P{:<3}  {} segments | {} records | offsets {}-{} | {}",
            v.partition_id,
            v.segments_read,
            fmt(v.records_verified),
            v.first_offset.unwrap_or(0),
            v.last_offset.unwrap_or(0),
            status,
        );
        for (expected, got) in &v.offset_gaps {
            println!("         GAP: expected offset {} but got {}", expected, got);
        }
    }
    println!();
    println!("  Records written:   {}", fmt(total));
    println!("  Records verified:  {}", fmt(total_verified));
    let lost = if total > total_verified {
        total - total_verified
    } else {
        0
    };
    println!("  Records lost:      {}", lost);
    println!(
        "  Integrity:         {}",
        if all_ok && lost == 0 {
            "PASS - zero data loss, zero offset gaps"
        } else {
            "FAIL - DATA LOSS OR GAPS DETECTED"
        }
    );
    println!();

    // =====================================================================
    println!("================================================================");
    println!("  SUMMARY");
    println!("================================================================");
    println!();
    println!(
        "  Write throughput:     {} rec/s",
        fmt(throughput as u64)
    );
    println!(
        "  Data rate:            {:.1} MB/s",
        raw_data_bytes / elapsed / MB
    );
    println!("  Total segments:       {}", total_segments);
    println!(
        "  Compression ratio:    {:.1}x ({:.1} MB -> {:.1} MB)",
        total_raw_bytes as f64 / total_seg_bytes as f64,
        total_raw_bytes as f64 / MB,
        total_seg_bytes as f64 / MB,
    );
    let total_gaps: usize = run.verification.iter().map(|v| v.offset_gaps.len()).sum();
    println!("  Records lost:         {}", lost);
    println!("  Offset gaps:          {}", total_gaps);
    println!();

    if throughput >= 1_000_000.0 {
        println!("  PASS: {:.1}x target (1M rec/s)", throughput / 1_000_000.0);
    } else {
        println!(
            "  MISS: {:.1}% of target (1M rec/s)",
            throughput / 1_000_000.0 * 100.0
        );
    }
    if all_ok && lost == 0 {
        println!("  PASS: zero data loss");
    } else {
        println!("  FAIL: data loss detected");
    }
    println!();
}

// =============================================================================
// Helpers
// =============================================================================

fn fmt(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn per_sec(n: u64, d: Duration) -> u64 {
    (n as f64 / d.as_secs_f64()) as u64
}
