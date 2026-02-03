//! Metrics Aggregator Example
//!
//! This example shows how to consume events from StreamHouse and compute
//! real-time aggregations like counts, rates, and top events.
//!
//! ## Usage
//!
//! ```bash
//! # Start StreamHouse
//! docker compose up -d
//!
//! # Create topics and produce some events first
//! curl -X POST http://localhost:8080/api/v1/topics \
//!   -H "Content-Type: application/json" \
//!   -d '{"name": "events", "partitions": 3}'
//!
//! # Produce some test events
//! for i in {1..100}; do
//!   curl -X POST http://localhost:8080/api/v1/produce \
//!     -H "Content-Type: application/json" \
//!     -d "{\"topic\": \"events\", \"key\": \"user-$((i % 10))\", \"value\": \"{\\\"event\\\": \\\"page_view\\\", \\\"url\\\": \\\"/home\\\"}\"}"
//! done
//!
//! # Start the metrics aggregator
//! cargo run --release
//! ```

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info};

const STREAMHOUSE_URL: &str = "http://localhost:8080";
const TOPIC: &str = "events";
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Deserialize)]
struct ConsumeResponse {
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
struct Message {
    offset: u64,
    partition: u32,
    value: Option<String>,
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Event {
    event: Option<String>,
    #[serde(rename = "type")]
    event_type: Option<String>,
}

struct Metrics {
    total_events: AtomicU64,
    event_counts: RwLock<HashMap<String, u64>>,
    unique_keys: RwLock<std::collections::HashSet<String>>,
    start_time: Instant,
}

impl Metrics {
    fn new() -> Self {
        Self {
            total_events: AtomicU64::new(0),
            event_counts: RwLock::new(HashMap::new()),
            unique_keys: RwLock::new(std::collections::HashSet::new()),
            start_time: Instant::now(),
        }
    }

    async fn record_event(&self, event_type: &str, key: Option<&str>) {
        self.total_events.fetch_add(1, Ordering::Relaxed);

        {
            let mut counts = self.event_counts.write().await;
            *counts.entry(event_type.to_string()).or_insert(0) += 1;
        }

        if let Some(k) = key {
            let mut keys = self.unique_keys.write().await;
            keys.insert(k.to_string());
        }
    }

    async fn print_summary(&self) {
        let total = self.total_events.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 {
            total as f64 / elapsed
        } else {
            0.0
        };

        let counts = self.event_counts.read().await;
        let unique_count = self.unique_keys.read().await.len();

        println!("\n========================================");
        println!("         METRICS SUMMARY");
        println!("========================================");
        println!("Total events:    {:>10}", total);
        println!("Events/sec:      {:>10.1}", rate);
        println!("Unique keys:     {:>10}", unique_count);
        println!("Elapsed time:    {:>10.1}s", elapsed);
        println!("----------------------------------------");
        println!("Event Type Breakdown:");

        // Sort by count descending
        let mut sorted: Vec<_> = counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        for (event_type, count) in sorted.iter().take(10) {
            let pct = if total > 0 {
                (*count as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            println!("  {:<20} {:>6} ({:>5.1}%)", event_type, count, pct);
        }
        println!("========================================\n");
    }
}

async fn consume_partition(
    client: &Client,
    topic: &str,
    partition: u32,
    offset: u64,
    limit: u32,
) -> Result<(Vec<Message>, u64), reqwest::Error> {
    let url = format!(
        "{}/api/v1/consume?topic={}&partition={}&offset={}&limit={}",
        STREAMHOUSE_URL, topic, partition, offset, limit
    );

    let response: ConsumeResponse = client.get(&url).send().await?.json().await?;

    let next_offset = response
        .messages
        .last()
        .map(|m| m.offset + 1)
        .unwrap_or(offset);

    Ok((response.messages, next_offset))
}

async fn get_partition_count(client: &Client, topic: &str) -> Result<u32, reqwest::Error> {
    #[derive(Deserialize)]
    struct TopicInfo {
        partition_count: Option<u32>,
        partitions: Option<u32>,
    }

    let url = format!("{}/api/v1/topics/{}", STREAMHOUSE_URL, topic);
    let info: TopicInfo = client.get(&url).send().await?.json().await?;

    Ok(info.partition_count.or(info.partitions).unwrap_or(1))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let client = Client::builder()
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to create HTTP client");

    let metrics = Arc::new(Metrics::new());

    info!("Starting metrics aggregator");
    info!("Consuming from topic: {}", TOPIC);
    info!("StreamHouse URL: {}", STREAMHOUSE_URL);

    // Get partition count
    let partition_count = match get_partition_count(&client, TOPIC).await {
        Ok(count) => {
            info!("Topic has {} partitions", count);
            count
        }
        Err(e) => {
            error!("Failed to get topic info: {}. Using 1 partition.", e);
            1
        }
    };

    // Track offsets per partition
    let mut offsets: Vec<u64> = vec![0; partition_count as usize];

    // Print summary every 5 seconds
    let metrics_clone = metrics.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            metrics_clone.print_summary().await;
        }
    });

    // Main consumer loop
    loop {
        for partition in 0..partition_count {
            let offset = offsets[partition as usize];

            match consume_partition(&client, TOPIC, partition, offset, 100).await {
                Ok((messages, next_offset)) => {
                    for msg in messages {
                        // Parse event type from value
                        let event_type = msg
                            .value
                            .as_ref()
                            .and_then(|v| serde_json::from_str::<Event>(v).ok())
                            .and_then(|e| e.event.or(e.event_type))
                            .unwrap_or_else(|| "unknown".to_string());

                        metrics.record_event(&event_type, msg.key.as_deref()).await;
                    }

                    offsets[partition as usize] = next_offset;
                }
                Err(e) => {
                    error!(partition = partition, error = %e, "Failed to consume");
                }
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
