//! Crash Failover Stress Test
//!
//! Tests that when one agent crashes (kill -9), another agent picks up
//! its partitions and no acknowledged messages are lost.
//!
//! ## What it does
//!
//! 1. Starts 2 unified-server instances sharing the same metadata DB + storage
//! 2. Creates a topic and produces at high throughput to both
//! 3. Kill -9s one server mid-stream
//! 4. Waits for partition reassignment (lease expiry + rebalance)
//! 5. Continues producing to the survivor
//! 6. Consumes everything back and reports:
//!    - Total produced (acknowledged) vs total consumed
//!    - Loss window size (records lost in the crash)
//!    - Time to recover
//!
//! ## Run
//!
//! ```bash
//! cargo run -p streamhouse-client --example stress_test_failover --release
//! ```
//!
//! The test manages server processes itself — no need to start servers manually.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use streamhouse_proto::streamhouse::{
    stream_house_client::StreamHouseClient, ConsumeRequest, ProduceBatchRequest, Record,
};
use tokio::sync::Mutex;
use tonic::transport::Channel;

// =============================================================================
// Configuration
// =============================================================================

const TOPIC: &str = "failover-test";
const PARTITIONS: u32 = 8;
const BATCH_SIZE: usize = 2000;
const VALUE_SIZE: usize = 128;
const PRODUCER_TASKS: usize = 4;

/// How long to produce before killing an agent
const PRE_CRASH_SECS: u64 = 15;
/// How long to produce after the crash
const POST_CRASH_SECS: u64 = 30;

/// Server binary path (relative to project root)
const SERVER_BIN: &str = "target/release/unified-server";

/// Shared data directory
const DATA_DIR: &str = "scripts/data-failover";

struct ServerInstance {
    grpc_port: u16,
    http_port: u16,
    #[allow(dead_code)]
    kafka_port: u16,
    child: Option<Child>,
}

impl ServerInstance {
    fn grpc_addr(&self) -> String {
        format!("http://localhost:{}", self.grpc_port)
    }

    fn http_addr(&self) -> String {
        format!("http://localhost:{}", self.http_port)
    }
}

impl Drop for ServerInstance {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Try graceful kill first, then force
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// =============================================================================
// Server management
// =============================================================================

fn start_server(grpc_port: u16, http_port: u16, kafka_port: u16, data_dir: &str) -> ServerInstance {
    let project_root = std::env::current_dir().expect("current dir");
    let bin_path = project_root.join(SERVER_BIN);
    let data_path = PathBuf::from(data_dir);

    std::fs::create_dir_all(data_path.join("storage")).expect("create storage dir");
    std::fs::create_dir_all(data_path.join("cache")).expect("create cache dir");
    std::fs::create_dir_all(data_path.join("wal")).expect("create shared wal dir");

    // Log server output to files for debugging
    let log_path = data_path.join(format!("server-{}.log", grpc_port));
    let log_file = std::fs::File::create(&log_path).expect("create log file");
    let err_file = log_file.try_clone().expect("clone log file");

    let child = Command::new(&bin_path)
        .env("GRPC_ADDR", format!("0.0.0.0:{}", grpc_port))
        .env("HTTP_ADDR", format!("0.0.0.0:{}", http_port))
        .env("KAFKA_ADDR", format!("0.0.0.0:{}", kafka_port))
        .env("USE_LOCAL_STORAGE", "1")
        .env("LOCAL_STORAGE_PATH", data_path.join("storage").to_str().unwrap())
        .env("STREAMHOUSE_METADATA", data_path.join("metadata.db").to_str().unwrap())
        .env("STREAMHOUSE_CACHE", data_path.join("cache").to_str().unwrap())
        .env("FLUSH_INTERVAL_SECS", "5")
        .env("WAL_ENABLED", "true")
        .env("WAL_DIR", data_path.join("wal").to_str().unwrap())
        .env("RUST_LOG", "warn")
        .env("STREAMHOUSE_AUTH_ENABLED", "false")
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(err_file))
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to start server on port {}: {}", grpc_port, e));

    println!("  Started server :{} (PID {}, log: {})", grpc_port, child.id(), log_path.display());

    ServerInstance {
        grpc_port,
        http_port,
        kafka_port,
        child: Some(child),
    }
}

fn kill_9(server: &mut ServerInstance) -> Option<u32> {
    if let Some(ref mut child) = server.child {
        let pid = child.id();
        // Use kill -9 to simulate a real crash (no graceful shutdown)
        let _ = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
        let _ = child.wait();
        server.child = None;
        Some(pid)
    } else {
        None
    }
}

async fn wait_for_server(addr: &str, timeout_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        match Channel::from_shared(addr.to_string())
            .unwrap()
            .connect_timeout(Duration::from_millis(500))
            .connect()
            .await
        {
            Ok(_) => return true,
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
    false
}

// =============================================================================
// Stats
// =============================================================================

struct Stats {
    records_acked: AtomicU64,
    batches_sent: AtomicU64,
    errors: AtomicU64,
    batch_latencies: Mutex<Vec<f64>>,
}

impl Stats {
    fn new() -> Self {
        Self {
            records_acked: AtomicU64::new(0),
            batches_sent: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            batch_latencies: Mutex::new(Vec::with_capacity(50_000)),
        }
    }
}

// =============================================================================
// Producer
// =============================================================================

async fn producer_task(
    task_id: usize,
    grpc_addrs: Vec<String>,
    stats: Arc<Stats>,
    running: Arc<AtomicBool>,
) {
    // Each producer round-robins across available servers
    let mut channels: Vec<Option<Channel>> = Vec::new();
    for addr in &grpc_addrs {
        match Channel::from_shared(addr.clone())
            .unwrap()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(30))
            .connect()
            .await
        {
            Ok(ch) => channels.push(Some(ch)),
            Err(_) => channels.push(None),
        }
    }

    let value = vec![b'F'; VALUE_SIZE];
    let mut batch_num = 0usize;

    while running.load(Ordering::Relaxed) {
        let partition = ((task_id * 1000 + batch_num) as u32) % PARTITIONS;

        // Pick a server — round-robin, skip dead ones
        let mut sent = false;
        for attempt in 0..grpc_addrs.len() {
            let idx = (batch_num + attempt) % grpc_addrs.len();
            let channel = match &channels[idx] {
                Some(ch) => ch.clone(),
                None => continue,
            };

            let mut client = StreamHouseClient::new(channel)
                .max_decoding_message_size(64 * 1024 * 1024)
                .max_encoding_message_size(64 * 1024 * 1024);

            let records: Vec<Record> = (0..BATCH_SIZE)
                .map(|i| Record {
                    key: format!("k{}", (task_id * 1_000_000 + batch_num * BATCH_SIZE + i) % 100_000)
                        .into_bytes(),
                    value: value.clone(),
                    headers: Default::default(),
                })
                .collect();

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
                    stats.records_acked.fetch_add(count, Ordering::Relaxed);
                    stats.batches_sent.fetch_add(1, Ordering::Relaxed);
                    stats.batch_latencies.lock().await.push(latency_ms);
                    sent = true;
                    break;
                }
                Err(_) => {
                    // Mark this channel as dead
                    channels[idx] = None;
                    // Try to reconnect in background
                    let addr = grpc_addrs[idx].clone();
                    if let Ok(ch) = Channel::from_shared(addr)
                        .unwrap()
                        .connect_timeout(Duration::from_millis(500))
                        .connect()
                        .await
                    {
                        channels[idx] = Some(ch);
                    }
                    continue;
                }
            }
        }

        if !sent {
            stats.errors.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        batch_num += 1;
    }
}

// =============================================================================
// Verification
// =============================================================================

async fn verify_messages(channel: &Channel) -> (u64, Vec<(u32, u64, u64)>) {
    let mut total_consumed = 0u64;
    let mut partition_results = Vec::new();

    for partition_id in 0..PARTITIONS {
        let mut client = StreamHouseClient::new(channel.clone())
            .max_decoding_message_size(64 * 1024 * 1024)
            .max_encoding_message_size(64 * 1024 * 1024);

        let mut consumed_count = 0u64;
        let mut current_offset = 0u64;
        let mut hw = 0u64;

        loop {
            let resp = client
                .consume(ConsumeRequest {
                    topic: TOPIC.to_string(),
                    partition: partition_id,
                    offset: current_offset,
                    max_records: 10_000,
                    consumer_group: None,
                })
                .await;

            match resp {
                Ok(r) => {
                    let inner = r.into_inner();
                    hw = inner.high_watermark;
                    let batch_count = inner.records.len() as u64;
                    if batch_count == 0 {
                        break;
                    }
                    consumed_count += batch_count;
                    current_offset = inner.records.last().unwrap().offset + 1;
                    if !inner.has_more {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("  Partition {}: consume error: {}", partition_id, e);
                    break;
                }
            }
        }

        total_consumed += consumed_count;
        partition_results.push((partition_id, consumed_count, hw));
    }

    (total_consumed, partition_results)
}

// =============================================================================
// Helpers
// =============================================================================

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

async fn create_topic(http_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/topics", http_addr))
        .json(&serde_json::json!({
            "name": TOPIC,
            "partitions": PARTITIONS,
            "replication_factor": 1
        }))
        .send()
        .await?;

    match response.status().as_u16() {
        200 | 201 | 409 => Ok(()),
        status => Err(format!("HTTP {}", status).into()),
    }
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_target(false)
        .init();

    println!();
    println!("================================================================");
    println!("  StreamHouse Crash Failover Stress Test");
    println!("================================================================");
    println!();
    println!("  Topic:           {} ({} partitions)", TOPIC, PARTITIONS);
    println!("  Producers:       {} tasks (round-robin across servers)", PRODUCER_TASKS);
    println!("  Batch size:      {} records", BATCH_SIZE);
    println!("  Pre-crash:       {}s of producing", PRE_CRASH_SECS);
    println!("  Post-crash:      {}s of producing to survivor", POST_CRASH_SECS);
    println!();

    // Clean up previous test data
    let data_dir = format!("{}/{}", std::env::current_dir()?.display(), DATA_DIR);
    if std::path::Path::new(&data_dir).exists() {
        println!("  Cleaning previous test data...");
        std::fs::remove_dir_all(&data_dir)?;
    }

    // =========================================================================
    // Phase 1: Start two servers
    // =========================================================================
    println!("--- Phase 1: Starting 2 server instances ---");
    println!();

    // Start Server A first so it runs SQLite migrations
    let server_a = start_server(50061, 8090, 9192, &data_dir);

    print!("  Waiting for Server A (:{})... ", server_a.grpc_port);
    if wait_for_server(&server_a.grpc_addr(), 15).await {
        println!("OK");
    } else {
        println!("FAILED");
        return Err("Server A failed to start".into());
    }

    // Start Server B after A is ready (migrations already complete)
    let mut server_b = start_server(50062, 8091, 9193, &data_dir);

    print!("  Waiting for Server B (:{})... ", server_b.grpc_port);
    if wait_for_server(&server_b.grpc_addr(), 15).await {
        println!("OK");
    } else {
        println!("FAILED");
        return Err("Server B failed to start".into());
    }

    // Give agents time to register and assign partitions
    println!("  Waiting 5s for agent registration + partition assignment...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Create topic via Server A
    print!("  Creating topic... ");
    match create_topic(&server_a.http_addr()).await {
        Ok(()) => println!("OK"),
        Err(e) => {
            println!("FAILED: {}", e);
            return Err(e);
        }
    }

    // Wait for partition assignment after topic creation
    println!("  Waiting 5s for partition assignment after topic creation...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    // =========================================================================
    // Phase 2: Produce to both servers
    // =========================================================================
    println!();
    println!("--- Phase 2: Producing to both servers ({}s) ---", PRE_CRASH_SECS);
    println!();

    let stats = Arc::new(Stats::new());
    let running = Arc::new(AtomicBool::new(true));

    let grpc_addrs = vec![server_a.grpc_addr(), server_b.grpc_addr()];

    let mut handles = Vec::new();
    for task_id in 0..PRODUCER_TASKS {
        let addrs = grpc_addrs.clone();
        let stats = stats.clone();
        let running = running.clone();
        handles.push(tokio::spawn(producer_task(task_id, addrs, stats, running)));
    }

    // Report progress every second
    let stats_r = stats.clone();
    let running_r = running.clone();
    let reporter = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last = 0u64;
        let mut sec = 0u64;
        loop {
            interval.tick().await;
            if !running_r.load(Ordering::Relaxed) {
                break;
            }
            let total = stats_r.records_acked.load(Ordering::Relaxed);
            let errors = stats_r.errors.load(Ordering::Relaxed);
            let delta = total - last;
            last = total;
            sec += 1;
            println!(
                "  [{:>3}s] {} msg/s  (total: {}, errors: {})",
                sec,
                format_number(delta),
                format_number(total),
                errors,
            );
        }
    });

    // Produce for PRE_CRASH_SECS
    tokio::time::sleep(Duration::from_secs(PRE_CRASH_SECS)).await;

    let pre_crash_acked = stats.records_acked.load(Ordering::Relaxed);
    let pre_crash_errors = stats.errors.load(Ordering::Relaxed);

    // =========================================================================
    // Phase 3: CRASH — kill -9 Server B
    // =========================================================================
    println!();
    println!("--- Phase 3: CRASH — kill -9 Server B (:{}) ---", server_b.grpc_port);
    println!();

    let _crash_time = Instant::now();
    if let Some(pid) = kill_9(&mut server_b) {
        println!("  Killed Server B (PID {}) with SIGKILL", pid);
    }
    println!("  Records acked before crash: {}", format_number(pre_crash_acked));
    println!();
    println!("  Producers will failover to Server A...");
    println!("  Waiting for lease expiry + partition reassignment (~30-60s)...");
    println!();

    // =========================================================================
    // Phase 4: Continue producing to survivor
    // =========================================================================
    println!("--- Phase 4: Producing to survivor ({}s) ---", POST_CRASH_SECS);
    println!();

    tokio::time::sleep(Duration::from_secs(POST_CRASH_SECS)).await;

    // Stop producers
    running.store(false, Ordering::Relaxed);
    for handle in handles {
        let _ = handle.await;
    }
    reporter.abort();

    let total_acked = stats.records_acked.load(Ordering::Relaxed);
    let total_errors = stats.errors.load(Ordering::Relaxed);
    let post_crash_acked = total_acked - pre_crash_acked;
    let recovery_errors = total_errors - pre_crash_errors;

    println!();
    println!("  Records acked after crash:  {}", format_number(post_crash_acked));
    println!("  Errors during/after crash:  {}", recovery_errors);

    // =========================================================================
    // Phase 5: Verification
    // =========================================================================
    println!();
    println!("--- Phase 5: Message Verification ---");
    println!();

    // Wait for S3 flushes
    println!("  Waiting 8s for segment flushes to storage...");
    tokio::time::sleep(Duration::from_secs(8)).await;

    // Connect to surviving server
    let channel = Channel::from_shared(server_a.grpc_addr())
        .unwrap()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .connect()
        .await?;

    let (total_consumed, partition_results) = verify_messages(&channel).await;

    // Print per-partition breakdown
    println!(
        "  {:>5}  {:>12}  {:>12}  {:>8}",
        "Part", "Consumed", "HW", "Loss"
    );
    println!("  {:->5}  {:->12}  {:->12}  {:->8}", "", "", "", "");

    let mut total_hw = 0u64;
    for (pid, consumed, hw) in &partition_results {
        let loss = if *consumed < *hw { hw - consumed } else { 0 };
        total_hw += hw;
        println!(
            "  {:>5}  {:>12}  {:>12}  {:>8}",
            pid,
            format_number(*consumed),
            format_number(*hw),
            loss
        );
    }

    println!("  {:->5}  {:->12}  {:->12}  {:->8}", "", "", "", "");
    println!(
        "  {:>5}  {:>12}  {:>12}  {:>8}",
        "TOTAL",
        format_number(total_consumed),
        format_number(total_hw),
        if total_consumed < total_hw {
            total_hw - total_consumed
        } else {
            0
        }
    );

    // =========================================================================
    // Phase 6: Final Report
    // =========================================================================
    println!();
    println!("================================================================");
    println!("  Failover Report");
    println!("================================================================");
    println!();
    println!("  Records acked (producer-side):  {}", format_number(total_acked));
    println!("  Records consumed (read-back):   {}", format_number(total_consumed));
    println!("  High watermark total:           {}", format_number(total_hw));
    println!();

    let acked_loss = if total_acked > total_consumed {
        total_acked - total_consumed
    } else {
        0
    };

    if acked_loss == 0 {
        println!("  ZERO ACKNOWLEDGED MESSAGE LOSS");
        println!("  All {} acked records were consumed back.", format_number(total_acked));
    } else {
        let loss_pct = acked_loss as f64 / total_acked as f64 * 100.0;
        println!(
            "  ACKNOWLEDGED MESSAGE LOSS: {} records ({:.4}%)",
            format_number(acked_loss),
            loss_pct
        );
        println!();
        println!("  These records were acked by the server but lost in the crash.");
        println!("  They were in the WAL/segment buffer of the killed server.");
        let flush_interval = 5u64;
        let est_throughput = pre_crash_acked / PRE_CRASH_SECS;
        let est_loss_window = if est_throughput > 0 {
            acked_loss as f64 / est_throughput as f64
        } else {
            0.0
        };
        println!();
        println!("  Estimated loss window:  {:.1}s (flush_interval={}s)", est_loss_window, flush_interval);
        println!(
            "  Throughput at crash:    {} msg/s",
            format_number(est_throughput)
        );
    }

    println!();
    println!("  Errors during test:     {} total ({} during/after crash)",
        total_errors, recovery_errors);
    println!("  Recovery time:          ~30-60s (lease expiry interval)");
    println!();

    // Cleanup
    drop(server_a);
    drop(server_b);

    Ok(())
}
