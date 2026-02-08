//! End-to-End Write Path Stress Test
//!
//! Tests the full production write path at 1M+ messages/second:
//!   Producer -> gRPC -> Agent -> SegmentWriter -> S3
//!
//! Uses multiple concurrent producer tasks across multiple partitions
//! to saturate the server.
//!
//! ## Prerequisites
//!
//! 1. Start the server:
//!    ```bash
//!    ./start-server.sh
//!    ```
//!
//! 2. The test will auto-create the topic via HTTP API.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p streamhouse-client --example stress_test_e2e --release
//! ```
//!
//! ## Tuning for 1M msg/s
//!
//! - Increase `PRODUCER_TASKS` to add more concurrent writers
//! - Increase `BATCH_SIZE` to reduce per-batch overhead
//! - Increase `PARTITIONS` to reduce lock contention
//! - Run with `--release` (debug mode is 10-50x slower)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use streamhouse_proto::streamhouse::{
    stream_house_client::StreamHouseClient, ProduceBatchRequest, Record,
};
use tokio::sync::Mutex;
use tonic::transport::Channel;

// =============================================================================
// Configuration
// =============================================================================

/// gRPC server address
const GRPC_ADDR: &str = "http://localhost:50051";

/// HTTP API address (for topic creation)
const HTTP_ADDR: &str = "http://localhost:8080";

/// Topic name for stress testing
const TOPIC: &str = "stress-test-1m";

/// Number of partitions
const PARTITIONS: u32 = 16;

/// Number of concurrent producer tasks
const PRODUCER_TASKS: usize = 8;

/// Records per gRPC batch
const BATCH_SIZE: usize = 5000;

/// Value payload size in bytes
const VALUE_SIZE: usize = 128;

/// Test duration in seconds
const DURATION_SECS: u64 = 30;

/// Warmup batches per producer
const WARMUP_BATCHES: usize = 5;

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_target(false)
        .init();

    let target_msg_sec = 1_000_000u64;

    println!();
    println!("================================================================");
    println!("  StreamHouse End-to-End Write Path Stress Test");
    println!("================================================================");
    println!();
    println!("  Target:          {} msg/s", format_number(target_msg_sec));
    println!("  gRPC endpoint:   {}", GRPC_ADDR);
    println!("  Topic:           {} ({} partitions)", TOPIC, PARTITIONS);
    println!("  Producers:       {} concurrent tasks", PRODUCER_TASKS);
    println!("  Batch size:      {} records", BATCH_SIZE);
    println!("  Value size:      {} bytes", VALUE_SIZE);
    println!("  Duration:        {}s", DURATION_SECS);
    println!(
        "  Theoretical max: {} msg/s ({} producers x {} batch/rpc)",
        format_number((PRODUCER_TASKS * BATCH_SIZE) as u64 * 1000), // assuming 1ms per batch
        PRODUCER_TASKS,
        BATCH_SIZE
    );
    println!();

    // Step 1: Create topic
    println!("--- Setup ---");
    print!("  Creating topic... ");
    match create_topic().await {
        Ok(()) => println!("OK"),
        Err(e) => println!("skipped ({})", e),
    }

    // Step 2: Connect to gRPC
    print!("  Connecting to gRPC... ");
    let channel = Channel::from_static(GRPC_ADDR)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .http2_keep_alive_interval(Duration::from_secs(20))
        .keep_alive_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true)
        .connect()
        .await?;
    println!("OK");

    // Step 3: Warmup
    println!();
    println!("--- Warmup ({} batches per producer) ---", WARMUP_BATCHES);
    warmup(&channel).await?;
    println!("  Done");

    // Step 4: Stress test
    println!();
    println!("--- Stress Test ({} seconds) ---", DURATION_SECS);
    println!();

    let stats = Arc::new(Stats::new());
    let running = Arc::new(AtomicBool::new(true));

    // Spawn producer tasks
    let mut handles = Vec::new();
    for task_id in 0..PRODUCER_TASKS {
        let channel = channel.clone();
        let stats = stats.clone();
        let running = running.clone();
        handles.push(tokio::spawn(
            producer_task(task_id, channel, stats, running),
        ));
    }

    // Spawn reporter task
    let stats_reporter = stats.clone();
    let running_reporter = running.clone();
    let reporter = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_total = 0u64;
        let mut second = 0u64;

        loop {
            interval.tick().await;
            if !running_reporter.load(Ordering::Relaxed) {
                break;
            }

            let total = stats_reporter.records_sent.load(Ordering::Relaxed);
            let errors = stats_reporter.errors.load(Ordering::Relaxed);
            let delta = total - last_total;
            last_total = total;
            second += 1;

            let indicator = if delta >= 1_000_000 {
                ">>>"
            } else if delta >= 500_000 {
                ">>"
            } else {
                ">"
            };

            println!(
                "  [{:>3}s] {} {} msg/s  (total: {}, errors: {})",
                second,
                indicator,
                format_number(delta),
                format_number(total),
                errors,
            );
        }
    });

    // Wait for test duration
    tokio::time::sleep(Duration::from_secs(DURATION_SECS)).await;
    running.store(false, Ordering::Relaxed);

    // Wait for all producers to finish
    for handle in handles {
        let _ = handle.await;
    }
    reporter.abort();

    // Step 5: Report results
    let total_records = stats.records_sent.load(Ordering::Relaxed);
    let total_batches = stats.batches_sent.load(Ordering::Relaxed);
    let total_errors = stats.errors.load(Ordering::Relaxed);
    let total_secs = DURATION_SECS as f64;
    let throughput = total_records as f64 / total_secs;

    // Calculate latency percentiles
    let latencies = stats.batch_latencies.lock().await;
    let (p50, p95, p99, p999, max) = percentiles(&latencies);

    println!();
    println!("================================================================");
    println!("  Results");
    println!("================================================================");
    println!();
    println!("  Total records:   {}", format_number(total_records));
    println!("  Total batches:   {}", format_number(total_batches));
    println!("  Total errors:    {}", total_errors);
    println!("  Duration:        {:.1}s", total_secs);
    println!("  Throughput:      {} msg/s", format_number(throughput as u64));
    println!(
        "  Data rate:       {:.1} MB/s",
        total_records as f64 * VALUE_SIZE as f64 / total_secs / (1024.0 * 1024.0)
    );
    println!();
    println!("  Batch latency percentiles (ms):");
    println!("    p50:  {:.2}", p50);
    println!("    p95:  {:.2}", p95);
    println!("    p99:  {:.2}", p99);
    println!("    p999: {:.2}", p999);
    println!("    max:  {:.2}", max);
    println!();

    // Verdict
    if throughput >= target_msg_sec as f64 {
        println!(
            "  PASS: {:.1}x target ({} msg/s)",
            throughput / target_msg_sec as f64,
            format_number(target_msg_sec)
        );
    } else {
        println!(
            "  MISS: {:.1}% of target ({} msg/s)",
            throughput / target_msg_sec as f64 * 100.0,
            format_number(target_msg_sec)
        );
        println!();
        println!("  Tuning suggestions:");
        if p99 > 50.0 {
            println!("    - High p99 latency ({:.0}ms). Server may be overloaded.", p99);
        }
        if total_errors > 0 {
            println!(
                "    - {} errors detected. Check server logs.",
                total_errors
            );
        }
        println!("    - Try increasing PRODUCER_TASKS (currently {})", PRODUCER_TASKS);
        println!("    - Try increasing BATCH_SIZE (currently {})", BATCH_SIZE);
        println!("    - Try increasing PARTITIONS (currently {})", PARTITIONS);
        println!("    - Ensure running with: --release");
    }

    println!();
    Ok(())
}

// =============================================================================
// Topic creation via HTTP
// =============================================================================

async fn create_topic() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/topics", HTTP_ADDR))
        .json(&serde_json::json!({
            "name": TOPIC,
            "partitions": PARTITIONS,
            "replication_factor": 1
        }))
        .send()
        .await?;

    match response.status().as_u16() {
        200 | 201 => Ok(()),
        409 => Ok(()), // already exists
        status => Err(format!("HTTP {}", status).into()),
    }
}

// =============================================================================
// Warmup
// =============================================================================

async fn warmup(channel: &Channel) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = StreamHouseClient::new(channel.clone())
        .max_decoding_message_size(64 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024);

    for i in 0..WARMUP_BATCHES {
        let records = make_batch(BATCH_SIZE, i * BATCH_SIZE);
        let request = ProduceBatchRequest {
            topic: TOPIC.to_string(),
            partition: (i as u32) % PARTITIONS,
            records,
            producer_id: None,
            producer_epoch: None,
            base_sequence: None,
            transaction_id: None,
        };
        client.produce_batch(request).await?;
    }
    Ok(())
}

// =============================================================================
// Producer task
// =============================================================================

async fn producer_task(
    task_id: usize,
    channel: Channel,
    stats: Arc<Stats>,
    running: Arc<AtomicBool>,
) {
    let mut client = StreamHouseClient::new(channel)
        .max_decoding_message_size(64 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024);

    let mut batch_num = 0usize;

    while running.load(Ordering::Relaxed) {
        let partition = ((task_id * 1000 + batch_num) as u32) % PARTITIONS;
        let records = make_batch(BATCH_SIZE, task_id * 1_000_000 + batch_num * BATCH_SIZE);

        let request = ProduceBatchRequest {
            topic: TOPIC.to_string(),
            partition,
            records,
            producer_id: None,
            producer_epoch: None,
            base_sequence: None,
            transaction_id: None,
        };

        let start = Instant::now();
        match client.produce_batch(request).await {
            Ok(response) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                let count = response.into_inner().count as u64;

                stats.records_sent.fetch_add(count, Ordering::Relaxed);
                stats.batches_sent.fetch_add(1, Ordering::Relaxed);
                stats.batch_latencies.lock().await.push(latency_ms);
            }
            Err(_) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                // Brief backoff on error to avoid tight error loop
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        batch_num += 1;
    }
}

// =============================================================================
// Record generation
// =============================================================================

fn make_batch(size: usize, offset: usize) -> Vec<Record> {
    // Pre-generate a shared value payload (all records use the same payload
    // to minimize allocation overhead - we're testing the write path, not allocation)
    let value = vec![b'S'; VALUE_SIZE];

    (0..size)
        .map(|i| {
            let idx = offset + i;
            Record {
                key: format!("k{}", idx % 100_000).into_bytes(),
                value: value.clone(),
                headers: Default::default(),
            }
        })
        .collect()
}

// =============================================================================
// Stats collection
// =============================================================================

struct Stats {
    records_sent: AtomicU64,
    batches_sent: AtomicU64,
    errors: AtomicU64,
    batch_latencies: Mutex<Vec<f64>>,
}

impl Stats {
    fn new() -> Self {
        Self {
            records_sent: AtomicU64::new(0),
            batches_sent: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            batch_latencies: Mutex::new(Vec::with_capacity(100_000)),
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn percentiles(latencies: &[f64]) -> (f64, f64, f64, f64, f64) {
    if latencies.is_empty() {
        return (0.0, 0.0, 0.0, 0.0, 0.0);
    }

    let mut sorted = latencies.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let len = sorted.len();
    let p = |pct: f64| -> f64 {
        let idx = ((pct / 100.0) * len as f64).ceil() as usize;
        sorted[idx.min(len - 1)]
    };

    (p(50.0), p(95.0), p(99.0), p(99.9), sorted[len - 1])
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
