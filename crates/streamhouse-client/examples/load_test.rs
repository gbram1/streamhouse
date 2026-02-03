//! High-performance load test for StreamHouse
//!
//! Creates 3 topics, 2 schemas each, 10k messages per schema = 60k total
//!
//! Run with:
//! ```bash
//! cargo run --example load_test --release
//! ```

use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const API_URL: &str = "http://localhost:8080";
const SCHEMA_URL: &str = "http://localhost:8080/schemas";
const MESSAGES_PER_TOPIC_VERSION: usize = 10000;
const BATCH_SIZE: usize = 100;
const CONCURRENCY: usize = 20; // Parallel requests

#[derive(Serialize)]
struct CreateTopicRequest {
    name: String,
    partitions: u32,
    replication_factor: u32,
}

#[derive(Serialize)]
struct SchemaRequest {
    schema: String,
}

#[derive(Serialize)]
struct BatchProduceRequest {
    topic: String,
    records: Vec<BatchRecord>,
}

#[derive(Serialize)]
struct BatchRecord {
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct BatchProduceResponse {
    #[allow(dead_code)]
    offsets: Vec<i64>,
}

struct LoadTestStats {
    messages_sent: AtomicU64,
    batches_sent: AtomicU64,
    errors: AtomicU64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║     StreamHouse High-Performance Load Test                 ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  Topics: 3 (orders, users, events)                         ║");
    println!("║  Schemas: 2 per topic (v1 + v2)                            ║");
    println!("║  Messages: 10,000 per topic/version                        ║");
    println!("║  Total: 60,000 messages                                    ║");
    println!("║  Concurrency: {} parallel requests                          ║", CONCURRENCY);
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    let client = Client::builder()
        .pool_max_idle_per_host(CONCURRENCY)
        .timeout(Duration::from_secs(30))
        .build()?;

    let stats = Arc::new(LoadTestStats {
        messages_sent: AtomicU64::new(0),
        batches_sent: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });

    // Step 1: Create topics
    println!("Step 1: Creating topics...");
    let topics = vec!["orders", "users", "events"];
    for topic in &topics {
        print!("  Creating topic: {}... ", topic);
        let response = client
            .post(format!("{}/api/v1/topics", API_URL))
            .json(&CreateTopicRequest {
                name: topic.to_string(),
                partitions: 6,
                replication_factor: 1,
            })
            .send()
            .await?;

        match response.status().as_u16() {
            200 | 201 => println!("OK"),
            409 => println!("Already exists"),
            status => println!("Failed ({})", status),
        }
    }
    println!();

    // Step 2: Register schemas (v1)
    println!("Step 2: Registering schemas (v1)...");
    register_schema(&client, "orders-value", r#"{"type":"record","name":"Order","fields":[{"name":"order_id","type":"string"},{"name":"customer_id","type":"string"},{"name":"amount","type":"double"},{"name":"status","type":"string"}]}"#).await?;
    register_schema(&client, "users-value", r#"{"type":"record","name":"User","fields":[{"name":"user_id","type":"string"},{"name":"email","type":"string"},{"name":"name","type":"string"},{"name":"created_at","type":"long"}]}"#).await?;
    register_schema(&client, "events-value", r#"{"type":"record","name":"Event","fields":[{"name":"event_id","type":"string"},{"name":"event_type","type":"string"},{"name":"payload","type":"string"},{"name":"timestamp","type":"long"}]}"#).await?;
    println!();

    // Step 3: Register schemas (v2)
    println!("Step 3: Registering schemas (v2)...");
    register_schema(&client, "orders-value", r#"{"type":"record","name":"Order","fields":[{"name":"order_id","type":"string"},{"name":"customer_id","type":"string"},{"name":"amount","type":"double"},{"name":"status","type":"string"},{"name":"currency","type":"string","default":"USD"}]}"#).await?;
    register_schema(&client, "users-value", r#"{"type":"record","name":"User","fields":[{"name":"user_id","type":"string"},{"name":"email","type":"string"},{"name":"name","type":"string"},{"name":"created_at","type":"long"},{"name":"verified","type":"boolean","default":false}]}"#).await?;
    register_schema(&client, "events-value", r#"{"type":"record","name":"Event","fields":[{"name":"event_id","type":"string"},{"name":"event_type","type":"string"},{"name":"payload","type":"string"},{"name":"timestamp","type":"long"},{"name":"source","type":"string","default":"unknown"}]}"#).await?;
    println!();

    // Step 4: Send messages
    println!("Step 4: Sending 60,000 messages with {} parallel workers...", CONCURRENCY);
    println!();

    let start = Instant::now();

    for topic in &topics {
        for version in &["v1", "v2"] {
            print!("  {} ({}): ", topic, version);
            std::io::Write::flush(&mut std::io::stdout())?;

            let topic_start = Instant::now();
            send_messages_parallel(&client, topic, version, MESSAGES_PER_TOPIC_VERSION, stats.clone()).await;
            let topic_duration = topic_start.elapsed();

            let rate = MESSAGES_PER_TOPIC_VERSION as f64 / topic_duration.as_secs_f64();
            println!(" {} msgs in {:.2}s ({:.0} msg/s)",
                MESSAGES_PER_TOPIC_VERSION,
                topic_duration.as_secs_f64(),
                rate
            );
        }
    }

    let duration = start.elapsed();
    let total_messages = stats.messages_sent.load(Ordering::Relaxed);
    let total_batches = stats.batches_sent.load(Ordering::Relaxed);
    let errors = stats.errors.load(Ordering::Relaxed);
    let rate = total_messages as f64 / duration.as_secs_f64();

    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                    Load Test Complete                      ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  Total messages:  {:>10}                               ║", total_messages);
    println!("║  Total batches:   {:>10}                               ║", total_batches);
    println!("║  Errors:          {:>10}                               ║", errors);
    println!("║  Duration:        {:>10.2}s                              ║", duration.as_secs_f64());
    println!("║  Throughput:      {:>10.0} msg/s                         ║", rate);
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("Check results:");
    println!("  - Grafana:  http://localhost:3001");
    println!("  - UI:       http://localhost:3000");
    println!("  - Topics:   curl {}/api/v1/topics", API_URL);
    println!("  - Metrics:  curl {}/metrics | grep streamhouse", API_URL);

    Ok(())
}

async fn register_schema(client: &Client, subject: &str, schema: &str) -> anyhow::Result<()> {
    print!("  Registering {}... ", subject);

    let response = client
        .post(format!("{}/subjects/{}/versions", SCHEMA_URL, subject))
        .header("Content-Type", "application/vnd.schemaregistry.v1+json")
        .json(&SchemaRequest {
            schema: schema.to_string(),
        })
        .send()
        .await?;

    match response.status().as_u16() {
        200 => println!("OK"),
        409 => println!("Already exists"),
        status => println!("Failed ({})", status),
    }

    Ok(())
}

async fn send_messages_parallel(
    client: &Client,
    topic: &str,
    version: &str,
    total_messages: usize,
    stats: Arc<LoadTestStats>,
) {
    let num_batches = (total_messages + BATCH_SIZE - 1) / BATCH_SIZE;

    // Generate all batches
    let batches: Vec<_> = (0..num_batches)
        .map(|batch_idx| {
            let start_idx = batch_idx * BATCH_SIZE;
            let end_idx = std::cmp::min(start_idx + BATCH_SIZE, total_messages);
            generate_batch(topic, version, start_idx, end_idx)
        })
        .collect();

    // Send batches in parallel
    let client = client.clone();
    let topic = topic.to_string();

    stream::iter(batches)
        .map(|batch| {
            let client = client.clone();
            let topic = topic.clone();
            let stats = stats.clone();

            async move {
                let batch_size = batch.records.len();
                let result = client
                    .post(format!("{}/api/v1/produce/batch", API_URL))
                    .json(&batch)
                    .send()
                    .await;

                match result {
                    Ok(response) if response.status().is_success() => {
                        stats.messages_sent.fetch_add(batch_size as u64, Ordering::Relaxed);
                        stats.batches_sent.fetch_add(1, Ordering::Relaxed);

                        // Progress indicator
                        let sent = stats.messages_sent.load(Ordering::Relaxed);
                        if sent % 1000 == 0 {
                            print!(".");
                            let _ = std::io::Write::flush(&mut std::io::stdout());
                        }
                    }
                    Ok(response) => {
                        stats.errors.fetch_add(1, Ordering::Relaxed);
                        eprintln!("Error: {} - {}", response.status(), topic);
                    }
                    Err(e) => {
                        stats.errors.fetch_add(1, Ordering::Relaxed);
                        eprintln!("Error: {} - {}", e, topic);
                    }
                }
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
}

fn generate_batch(topic: &str, version: &str, start_idx: usize, end_idx: usize) -> BatchProduceRequest {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let records: Vec<BatchRecord> = (start_idx..end_idx)
        .map(|idx| {
            let (key, value) = match (topic, version) {
                ("orders", "v1") => (
                    format!("order-{}", idx),
                    format!(r#"{{"order_id":"ord-{}","customer_id":"cust-{}","amount":{}.99,"status":"pending"}}"#,
                        idx, idx % 1000, idx % 10000)
                ),
                ("orders", "v2") => (
                    format!("order-{}", idx),
                    format!(r#"{{"order_id":"ord-{}","customer_id":"cust-{}","amount":{}.99,"status":"pending","currency":"USD"}}"#,
                        idx, idx % 1000, idx % 10000)
                ),
                ("users", "v1") => (
                    format!("user-{}", idx),
                    format!(r#"{{"user_id":"user-{}","email":"user{}@test.com","name":"User {}","created_at":{}}}"#,
                        idx, idx, idx, ts)
                ),
                ("users", "v2") => (
                    format!("user-{}", idx),
                    format!(r#"{{"user_id":"user-{}","email":"user{}@test.com","name":"User {}","created_at":{},"verified":true}}"#,
                        idx, idx, idx, ts)
                ),
                ("events", "v1") => (
                    format!("event-{}", idx),
                    format!(r#"{{"event_id":"evt-{}","event_type":"click","payload":"{{}}","timestamp":{}}}"#,
                        idx, ts)
                ),
                ("events", "v2") => (
                    format!("event-{}", idx),
                    format!(r#"{{"event_id":"evt-{}","event_type":"click","payload":"{{}}","timestamp":{},"source":"web"}}"#,
                        idx, ts)
                ),
                _ => (format!("key-{}", idx), format!(r#"{{"idx":{}}}"#, idx)),
            };

            BatchRecord { key, value }
        })
        .collect();

    BatchProduceRequest {
        topic: topic.to_string(),
        records,
    }
}
