//! High-Performance Producer Example
//!
//! This example demonstrates Phase 18.5 - the native Rust client's
//! high-performance mode using persistent gRPC connections with batching.
//!
//! ## Prerequisites
//!
//! 1. Start the unified server:
//!    ```bash
//!    ./start-server.sh
//!    ```
//!
//! 2. Create a test topic via HTTP API:
//!    ```bash
//!    curl -X POST http://localhost:8080/api/v1/topics \
//!      -H "Content-Type: application/json" \
//!      -d '{"name": "high-perf-test", "partitions": 6, "replication_factor": 1}'
//!    ```
//!
//! ## Run
//!
//! ```bash
//! cargo run -p streamhouse-client --example high_perf_producer
//! ```
//!
//! ## Expected Performance
//!
//! With proper batching and persistent connections:
//! - Target: 50,000+ msg/s
//! - Latency: < 10ms p99
//!
//! This is achieved by:
//! 1. Maintaining persistent gRPC connections (no TCP handshake per request)
//! 2. Batching records (100+ records per RPC call)
//! 3. HTTP/2 multiplexing (multiple streams on one connection)

use std::time::{Duration, Instant};
use streamhouse_proto::streamhouse::{
    stream_house_client::StreamHouseClient, ProduceBatchRequest, Record,
};
use tonic::transport::Channel;

const GRPC_ADDR: &str = "http://localhost:50051";
const TOPIC_NAME: &str = "high-perf-test";
const TOTAL_MESSAGES: usize = 100_000;
const BATCH_SIZE: usize = 1000;
const PARTITIONS: u32 = 6;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    println!();
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║  StreamHouse High-Performance Client (Phase 18.5)        ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();
    println!("Configuration:");
    println!("  gRPC endpoint:   {}", GRPC_ADDR);
    println!("  Topic:           {}", TOPIC_NAME);
    println!("  Total messages:  {}", TOTAL_MESSAGES);
    println!("  Batch size:      {}", BATCH_SIZE);
    println!("  Partitions:      {}", PARTITIONS);
    println!();

    // Create persistent gRPC connection
    println!("🔌 Establishing persistent gRPC connection...");
    let channel = Channel::from_static(GRPC_ADDR)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .http2_keep_alive_interval(Duration::from_secs(20))
        .keep_alive_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true)
        .connect()
        .await?;

    let mut client = StreamHouseClient::new(channel)
        .max_decoding_message_size(64 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024);

    println!("   ✅ Connected to StreamHouse server");
    println!();

    // Warm up
    println!("🔥 Warming up (sending 10 batches)...");
    for i in 0..10 {
        let partition = (i % PARTITIONS as usize) as u32;
        let records = create_batch(100, i * 100);
        let request = ProduceBatchRequest {
            topic: TOPIC_NAME.to_string(),
            partition,
            records,
        };
        client.produce_batch(request).await?;
    }
    println!("   ✅ Warm-up complete");
    println!();

    // Main benchmark
    println!("═══════════════════════════════════════════════════════════");
    println!("Starting benchmark: {} messages in {} batches", TOTAL_MESSAGES, TOTAL_MESSAGES / BATCH_SIZE);
    println!("═══════════════════════════════════════════════════════════");
    println!();

    let num_batches = TOTAL_MESSAGES / BATCH_SIZE;
    let start = Instant::now();
    let mut total_sent: u64 = 0;

    for batch_num in 0..num_batches {
        let partition = (batch_num % PARTITIONS as usize) as u32;
        let records = create_batch(BATCH_SIZE, batch_num * BATCH_SIZE);

        let request = ProduceBatchRequest {
            topic: TOPIC_NAME.to_string(),
            partition,
            records,
        };

        let response = client.produce_batch(request).await?;
        let count = response.into_inner().count;
        total_sent += count as u64;

        // Progress update every 10 batches
        if (batch_num + 1) % 10 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let current_rate = total_sent as f64 / elapsed;
            println!(
                "  Batch {}/{}: {} messages, {:.0} msg/s cumulative",
                batch_num + 1,
                num_batches,
                count,
                current_rate
            );
        }
    }

    let elapsed = start.elapsed();
    let total_secs = elapsed.as_secs_f64();
    let throughput = total_sent as f64 / total_secs;

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("Results");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("  Total messages sent:  {}", total_sent);
    println!("  Total time:           {:.3}s", total_secs);
    println!("  Throughput:           {:.0} msg/s", throughput);
    println!("  Avg latency/batch:    {:.2}ms", (total_secs * 1000.0) / num_batches as f64);
    println!();

    // Performance assessment
    if throughput >= 50000.0 {
        println!("🎉 EXCELLENT: Achieved target throughput (50K+ msg/s)!");
    } else if throughput >= 10000.0 {
        println!("✅ GOOD: Solid performance, consider tuning batch size");
    } else if throughput >= 1000.0 {
        println!("⚠️  MODERATE: Check network latency and server load");
    } else {
        println!("❌ LOW: Check for connection issues or server errors");
    }

    println!();
    println!("Protocol stack used:");
    println!("  Application → gRPC → Protobuf → HTTP/2 → TCP (persistent)");
    println!();

    Ok(())
}

/// Create a batch of records for testing
fn create_batch(size: usize, offset: usize) -> Vec<Record> {
    (0..size)
        .map(|i| {
            let key = format!("key-{}", offset + i).into_bytes();
            let value = format!(
                r#"{{"id":{},"data":"test message {}","timestamp":{}}}"#,
                offset + i,
                offset + i,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            )
            .into_bytes();

            Record {
                key,
                value,
                headers: Default::default(),
            }
        })
        .collect()
}
