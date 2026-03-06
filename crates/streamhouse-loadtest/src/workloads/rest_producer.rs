//! REST API batch producer workload.

use crate::http_client::OrgClient;
use crate::metrics;
use crate::setup::TopicSpec;
use rand::Rng;
use rand::SeedableRng;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

pub async fn run_rest_producer(
    client: OrgClient,
    org_slug: String,
    topics: Vec<TopicSpec>,
    batch_size: usize,
    produce_rate: usize,
    produced_counts: Arc<std::collections::HashMap<String, AtomicU64>>,
    mut shutdown: watch::Receiver<bool>,
) {
    metrics::ACTIVE_PRODUCERS.inc();
    let mut rng = rand::rngs::StdRng::from_entropy();
    let mut seq: u64 = 0;
    let producer_id = format!("rest-{}-{}", org_slug, uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Interval for rate limiting: batch_size records every (batch_size / produce_rate) seconds
    let interval_ms = if produce_rate > 0 {
        (batch_size as u64 * 1000) / produce_rate as u64
    } else {
        1000
    };
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms.max(10)));

    let mut topic_idx = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        let topic = &topics[topic_idx % topics.len()];
        topic_idx += 1;

        let partition: u32 = rng.gen_range(0..topic.partitions);

        let records: Vec<serde_json::Value> = (0..batch_size)
            .map(|_| {
                seq += 1;
                let value_size: usize = rng.gen_range(100..2000);
                let payload: String = (0..value_size).map(|_| 'x').collect();

                let value = json!({
                    "seq": seq,
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "producer_id": producer_id,
                    "org": org_slug,
                    "payload": payload
                });

                let mut record = json!({
                    "value": value.to_string(),
                    "partition": partition,
                });

                if topic.keyed {
                    record["key"] = json!(format!("key-{}", rng.gen_range(0u32..1000)));
                }

                record
            })
            .collect();

        let body = json!({
            "topic": topic.name,
            "records": records
        });

        let start = Instant::now();
        match client
            .post_json::<serde_json::Value>("/api/v1/produce/batch", &body)
            .await
        {
            Ok(_) => {
                let elapsed = start.elapsed().as_secs_f64();
                metrics::PRODUCE_LATENCY
                    .with_label_values(&["rest"])
                    .observe(elapsed);
                metrics::PRODUCED_TOTAL
                    .with_label_values(&[&org_slug, &topic.name, "rest"])
                    .inc_by(batch_size as u64);
                if let Some(counter) = produced_counts.get(&topic.name) {
                    counter.fetch_add(batch_size as u64, Ordering::Relaxed);
                }
            }
            Err(e) => {
                metrics::PRODUCE_ERRORS
                    .with_label_values(&[&org_slug, &topic.name, "rest"])
                    .inc();
                tracing::warn!(topic = %topic.name, err = %e, "REST produce failed");
            }
        }
    }

    metrics::ACTIVE_PRODUCERS.dec();
}
