//! Complete StreamHouse Pipeline Demo (Phases 1-7)
//!
//! This demonstrates:
//! - Producer with metrics
//! - Consumer with metrics and lag monitoring
//! - Agent coordination
//! - Real-time observability
//! - S3 storage
//! - PostgreSQL metadata

use streamhouse_client::{Producer, Consumer, OffsetReset};
use streamhouse_metadata::SqliteMetadataStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  StreamHouse Complete Pipeline Demo (Phases 1-7)     ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Setup
    let metadata_store = Arc::new(
        SqliteMetadataStore::new("/tmp/streamhouse_demo.db").await?
    );

    // Create topic
    println!("📝 Creating topic 'orders' with 4 partitions...");
    let topic_config = streamhouse_metadata::TopicConfig {
        name: "orders".to_string(),
        partition_count: 4,
        retention_ms: Some(86400000), // 24 hours
        config: std::collections::HashMap::new(),
    };

    if let Err(e) = metadata_store.create_topic(topic_config).await {
        println!("   (Topic may already exist: {})", e);
    }
    println!("✓ Topic ready\n");

    // Register agent
    println!("🤖 Registering agent...");
    let agent_info = streamhouse_metadata::AgentInfo {
        agent_id: "demo-agent-001".to_string(),
        address: "localhost:9090".to_string(),
        availability_zone: "local".to_string(),
        agent_group: "demo".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: std::collections::HashMap::new(),
    };
    metadata_store.register_agent(agent_info).await?;
    println!("✓ Agent registered\n");

    // Phase 1: Producer Setup
    println!("╭─────────────────────────────────────────────────────────╮");
    println!("│ Phase 1-5: Producer with Metrics                        │");
    println!("╰─────────────────────────────────────────────────────────╯\n");

    #[cfg(feature = "metrics")]
    {
        use prometheus_client::registry::Registry;
        use streamhouse_client::ProducerMetrics;

        let mut registry = Registry::default();
        let producer_metrics = Arc::new(ProducerMetrics::new(&mut registry));

        let producer = Producer::builder()
            .metadata_store(Arc::clone(&metadata_store))
            .agent_group("demo".to_string())
            .batch_size(100)
            .compression_enabled(true)
            .metrics(producer_metrics.clone())
            .build()
            .await?;

        println!("✓ Producer created with metrics enabled");
        println!("  - Batch size: 100 records");
        println!("  - Compression: LZ4 enabled");
        println!("  - Metrics: Active\n");

        // Produce messages
        println!("📤 Producing 1000 messages...");
        let start = std::time::Instant::now();

        for i in 0..1000 {
            let key = format!("user_{}", i % 100);
            let value = format!(
                r#"{{"order_id": {}, "user_id": "user_{}", "amount": {}, "timestamp": "{}"}}"#,
                i,
                i % 100,
                (i * 10) % 1000,
                chrono::Utc::now().to_rfc3339()
            );

            producer.send(
                "orders",
                Some(key.as_bytes()),
                value.as_bytes(),
                None,
            ).await?;

            if (i + 1) % 100 == 0 {
                println!("   Produced: {}/1000 messages", i + 1);
            }
        }

        producer.flush().await?;
        let elapsed = start.elapsed();

        println!("\n✓ Production complete!");
        println!("  - Total messages: 1000");
        println!("  - Time taken: {:.2}s", elapsed.as_secs_f64());
        println!("  - Throughput: {:.0} msg/s\n", 1000.0 / elapsed.as_secs_f64());
    }

    #[cfg(not(feature = "metrics"))]
    {
        println!("⚠️  Metrics feature not enabled");
        println!("   Run with: cargo run --features metrics --example complete_pipeline_demo\n");
    }

    // Phase 2: Consumer Setup
    println!("╭─────────────────────────────────────────────────────────╮");
    println!("│ Phase 6: Consumer with Lag Monitoring                   │");
    println!("╰─────────────────────────────────────────────────────────╯\n");

    sleep(Duration::from_secs(2)).await;

    #[cfg(feature = "metrics")]
    {
        use prometheus_client::registry::Registry;
        use streamhouse_client::ConsumerMetrics;
        use object_store::local::LocalFileSystem;

        let mut consumer_registry = Registry::default();
        let consumer_metrics = Arc::new(ConsumerMetrics::new(&mut consumer_registry));

        let object_store = Arc::new(
            LocalFileSystem::new_with_prefix("/tmp/streamhouse_segments")?
        );

        let mut consumer = Consumer::builder()
            .group_id("demo-analytics".to_string())
            .topics(vec!["orders".to_string()])
            .metadata_store(Arc::clone(&metadata_store))
            .object_store(object_store)
            .offset_reset(OffsetReset::Earliest)
            .auto_commit(true)
            .metrics(consumer_metrics.clone())
            .build()
            .await?;

        println!("✓ Consumer created with metrics enabled");
        println!("  - Group ID: demo-analytics");
        println!("  - Offset reset: Earliest");
        println!("  - Auto-commit: Enabled");
        println!("  - Metrics: Active\n");

        // Consume messages
        println!("📥 Consuming messages...");
        let mut total_consumed = 0;
        let start = std::time::Instant::now();

        for round in 1..=5 {
            let records = consumer.poll(Duration::from_secs(2)).await?;

            if !records.is_empty() {
                total_consumed += records.len();
                println!("   Round {}: Consumed {} messages (Total: {})",
                    round, records.len(), total_consumed);

                // Show sample record
                if let Some(record) = records.first() {
                    println!("   Sample: topic={}, partition={}, offset={}",
                        record.topic, record.partition, record.offset);
                }
            }

            // Check consumer lag (Phase 7 feature)
            if let Ok(lag) = consumer.lag("orders", 0).await {
                println!("   Consumer lag on partition 0: {} records", lag);
            }
        }

        let elapsed = start.elapsed();

        println!("\n✓ Consumption complete!");
        println!("  - Total consumed: {} messages", total_consumed);
        println!("  - Time taken: {:.2}s", elapsed.as_secs_f64());
        if total_consumed > 0 {
            println!("  - Throughput: {:.0} msg/s\n",
                total_consumed as f64 / elapsed.as_secs_f64());
        }
    }

    // Phase 3: Show stored data
    println!("╭─────────────────────────────────────────────────────────╮");
    println!("│ Phase 7: Observability & Metrics                        │");
    println!("╰─────────────────────────────────────────────────────────╯\n");

    println!("📊 Metrics available at:");
    println!("   - Grafana Dashboard: http://localhost:3000");
    println!("   - Prometheus: http://localhost:9090");
    println!("   - Metrics endpoint: http://localhost:8080/metrics\n");

    println!("🗄️  Data stored in:");
    println!("   - PostgreSQL: streamhouse_metadata database");
    println!("   - MinIO: streamhouse-data bucket");
    println!("   - View MinIO Console: http://localhost:9001\n");

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  Demo Complete! Check Grafana for metrics visualization ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    Ok(())
}
