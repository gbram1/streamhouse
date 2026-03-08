//! StreamHouse Production Load Test
//!
//! Long-running e2e load test that exercises all subsystems:
//! - Multi-org tenancy (3 organizations)
//! - 24 topics across orgs (204 total partitions)
//! - REST, Kafka, and gRPC producers
//! - Consumers with offset tracking
//! - SQL queries, schema evolution
//! - Data integrity validation
//! - S3 storage verification
//!
//! Exposes Prometheus metrics on port 9100 for Grafana dashboards.
//!
//! Run: cargo run -p streamhouse-loadtest --release

mod config;
mod http_client;
mod kafka_wire;
mod metrics;
mod setup;
mod validation;
mod workloads;

use clap::Parser;
use config::Config;
use http_client::HttpClient;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info,reqwest=warn,hyper=warn,tonic=warn".to_string()),
        )
        .init();

    let config = Config::parse();

    println!("=== StreamHouse Production Load Test ===");
    println!("  HTTP:  {}", config.http_addr);
    println!("  gRPC:  {}", config.grpc_addr);
    println!("  Kafka: {}", config.kafka_addr);
    println!("  Metrics: http://0.0.0.0:{}/metrics", config.metrics_port);
    if let Some(d) = config.duration {
        println!("  Duration: {}s", d);
    } else {
        println!("  Duration: infinite (ctrl+c to stop)");
    }
    println!();

    // ── 1. Initialize metrics server ────────────────────────────────────
    metrics::init();
    metrics::start_metrics_server(config.metrics_port).await?;

    // ── 2. Wait for server health ───────────────────────────────────────
    let client = HttpClient::new(&config.http_addr);
    print!("Waiting for server...");
    for i in 0..60 {
        if client.health_check().await {
            println!(" ready! (after {}s)", i * 2);
            break;
        }
        if i == 59 {
            anyhow::bail!("Server not healthy after 120s");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        print!(".");
    }

    // ── 3. Setup phase ──────────────────────────────────────────────────
    println!("\n--- Setup Phase ---");
    let orgs = setup::run_setup(&client).await?;

    // Build produced/consumed counters per topic
    let mut all_topics: Vec<(String, String, setup::TopicSpec)> = Vec::new();
    for org in &orgs {
        for topic in &org.topics {
            all_topics.push((org.id.clone(), org.slug.clone(), topic.clone()));
        }
    }

    let produced_counts: Arc<HashMap<String, AtomicU64>> = Arc::new(
        all_topics
            .iter()
            .map(|(_, _, t)| (t.name.clone(), AtomicU64::new(0)))
            .chain(
                // Kafka-specific topics in default org
                ["kafka-orders", "kafka-market-data"]
                    .iter()
                    .map(|name| (name.to_string(), AtomicU64::new(0))),
            )
            .collect(),
    );
    let consumed_counts: Arc<HashMap<String, AtomicU64>> = Arc::new(
        all_topics
            .iter()
            .map(|(_, _, t)| (t.name.clone(), AtomicU64::new(0)))
            .collect(),
    );
    let topic_org_map: HashMap<String, String> = all_topics
        .iter()
        .map(|(_, slug, t)| (t.name.clone(), slug.clone()))
        .collect();

    // ── 4. Shutdown signal ──────────────────────────────────────────────
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // ── 5. Spawn workloads ──────────────────────────────────────────────
    println!("\n--- Starting Workloads ---");
    let mut tasks = JoinSet::new();

    // REST producers: 2 per org
    for org in &orgs {
        for _ in 0..2 {
            let org_client = client.with_org(&org.id);
            let slug = org.slug.clone();
            let topics = org.topics.clone();
            let pc = Arc::clone(&produced_counts);
            let rx = shutdown_rx.clone();
            let bs = config.batch_size;
            let rate = config.produce_rate;
            tasks.spawn(async move {
                workloads::rest_producer::run_rest_producer(
                    org_client, slug, topics, bs, rate, pc, rx,
                )
                .await;
            });
        }
    }
    println!("  Started {} REST producers", orgs.len() * 2);

    // Kafka producers: use default-org topics (unauthenticated Kafka connections
    // resolve to the default organization)
    for (topic_name, partitions) in &[("kafka-orders", 8u32), ("kafka-market-data", 16u32)] {
        let addr = config.kafka_addr.clone();
        let slug = "default".to_string();
        let topic = topic_name.to_string();
        let parts = *partitions;
        let pc = Arc::clone(&produced_counts);
        let rx = shutdown_rx.clone();
        let rate = config.produce_rate * 5; // Higher rate for Kafka
        let bs = 500;
        // Add to produced_counts tracking (these topics aren't in all_topics)
        tasks.spawn(async move {
            workloads::kafka_producer::run_kafka_producer(
                addr, slug, topic, parts, bs, rate, None, pc, rx,
            )
            .await;
        });
    }
    println!("  Started 2 Kafka producers");

    // gRPC producers: ft-transactions and an-clickstream
    for topic_name in &["ft-transactions", "an-clickstream"] {
        if let Some((org_id, slug, spec)) = all_topics.iter().find(|(_, _, t)| t.name == *topic_name) {
            let addr = config.grpc_addr.clone();
            let org_id = org_id.clone();
            let slug = slug.clone();
            let topic = spec.name.clone();
            let parts = spec.partitions;
            let pc = Arc::clone(&produced_counts);
            let rx = shutdown_rx.clone();
            let rate = config.produce_rate * 2;
            let bs = 200;
            tasks.spawn(async move {
                workloads::grpc_producer::run_grpc_producer(
                    addr, org_id, slug, topic, parts, bs, rate, pc, rx,
                )
                .await;
            });
        }
    }
    println!("  Started 2 gRPC producers");

    // Consumers: 1 per topic
    for (org_id, slug, spec) in &all_topics {
        let org_client = client.with_org(org_id);
        let slug = slug.clone();
        let topic = spec.name.clone();
        let parts = spec.partitions;
        let cc = Arc::clone(&consumed_counts);
        let rx = shutdown_rx.clone();
        tasks.spawn(async move {
            workloads::consumer::run_consumer(org_client, slug, topic, parts, cc, rx).await;
        });
    }
    println!("  Started {} consumers", all_topics.len());

    // SQL queries
    {
        let org_clients: Vec<_> = orgs.iter().map(|o| client.with_org(&o.id)).collect();
        let rx = shutdown_rx.clone();
        let sql_interval = config.sql_interval;
        tasks.spawn(async move {
            workloads::sql_queries::run_sql_queries(org_clients, sql_interval, rx).await;
        });
        println!("  Started SQL query workload (every {}s)", config.sql_interval);
    }

    // Schema evolution
    {
        let c = client.clone();
        let rx = shutdown_rx.clone();
        let schema_interval = config.schema_evolution_interval;
        tasks.spawn(async move {
            workloads::schema_evolution::run_schema_evolution(c, schema_interval, rx).await;
        });
        println!(
            "  Started schema evolution (every {}s)",
            config.schema_evolution_interval
        );
    }

    // ── 6. Spawn validation tasks ───────────────────────────────────────
    {
        let pc = Arc::clone(&produced_counts);
        let cc = Arc::clone(&consumed_counts);
        let tom = topic_org_map.clone();
        let rx = shutdown_rx.clone();
        let integrity_interval = config.integrity_interval;
        tasks.spawn(async move {
            validation::integrity::run_integrity_checker(pc, cc, tom, integrity_interval, rx).await;
        });
    }

    {
        let rx = shutdown_rx.clone();
        let storage_interval = config.storage_check_interval;
        tasks.spawn(async move {
            validation::storage::run_storage_checker(
                "http://localhost:9000".to_string(),
                "streamhouse".to_string(),
                storage_interval,
                rx,
            )
            .await;
        });
    }

    // ── 7. Stats reporter ───────────────────────────────────────────────
    let start_time = Instant::now();
    let pc_stats = Arc::clone(&produced_counts);
    let cc_stats = Arc::clone(&consumed_counts);
    let stats_interval = config.stats_interval;
    let stats_rx = shutdown_rx.clone();

    tasks.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(stats_interval));
        let mut last_produced: u64 = 0;
        let mut last_consumed: u64 = 0;

        loop {
            let mut rx_clone = stats_rx.clone();
            tokio::select! {
                _ = interval.tick() => {},
                _ = rx_clone.changed() => break,
            }

            if *stats_rx.borrow() {
                break;
            }

            let elapsed = start_time.elapsed().as_secs();
            metrics::RUNNING_SECONDS.set(elapsed as i64);

            let total_produced: u64 = pc_stats.values().map(|c| c.load(Ordering::Relaxed)).sum();
            let total_consumed: u64 = cc_stats.values().map(|c| c.load(Ordering::Relaxed)).sum();

            let produce_rate = (total_produced - last_produced) / stats_interval;
            let consume_rate = (total_consumed - last_consumed) / stats_interval;
            last_produced = total_produced;
            last_consumed = total_consumed;

            println!(
                "[{:>4}s] Produced: {:>8} ({:>5}/s) | Consumed: {:>8} ({:>5}/s) | Gap: {}",
                elapsed,
                format_count(total_produced),
                produce_rate,
                format_count(total_consumed),
                consume_rate,
                total_produced.saturating_sub(total_consumed),
            );
        }
    });

    println!("\n--- Load Test Running ---\n");

    // ── 8. Wait for shutdown ────────────────────────────────────────────
    if let Some(duration) = config.duration {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutting down (ctrl+c)...");
            }
            _ = tokio::time::sleep(Duration::from_secs(duration)) => {
                println!("\nDuration reached ({}s), shutting down...", duration);
            }
        }
    } else {
        tokio::signal::ctrl_c().await?;
        println!("\nShutting down (ctrl+c)...");
    }

    // Signal all tasks to stop
    let _ = shutdown_tx.send(true);

    // Give tasks a few seconds to finish
    tokio::time::sleep(Duration::from_secs(2)).await;
    tasks.shutdown().await;

    // ── 9. Final report ─────────────────────────────────────────────────
    let elapsed = start_time.elapsed();
    let total_produced: u64 = produced_counts
        .values()
        .map(|c| c.load(Ordering::Relaxed))
        .sum();
    let total_consumed: u64 = consumed_counts
        .values()
        .map(|c| c.load(Ordering::Relaxed))
        .sum();

    println!("\n=== Final Report ===");
    println!("  Duration:  {:.1}s", elapsed.as_secs_f64());
    println!("  Produced:  {}", format_count(total_produced));
    println!("  Consumed:  {}", format_count(total_consumed));
    println!(
        "  Avg Rate:  {:.0} msg/s produced, {:.0} msg/s consumed",
        total_produced as f64 / elapsed.as_secs_f64(),
        total_consumed as f64 / elapsed.as_secs_f64(),
    );
    println!(
        "  Gap:       {} messages",
        total_produced.saturating_sub(total_consumed)
    );

    if total_produced > total_consumed + 10_000 {
        println!("  Status:    DATA LOSS DETECTED");
        std::process::exit(1);
    } else {
        println!("  Status:    OK");
    }

    Ok(())
}

fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}
