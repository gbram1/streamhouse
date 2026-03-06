//! REST API consumer workload with offset tracking.

use crate::http_client::OrgClient;
use crate::metrics;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

const CONSUMER_GROUP: &str = "loadtest-primary";

pub async fn run_consumer(
    client: OrgClient,
    org_slug: String,
    topic: String,
    partitions: u32,
    consumed_counts: Arc<HashMap<String, AtomicU64>>,
    mut shutdown: watch::Receiver<bool>,
) {
    metrics::ACTIVE_CONSUMERS.inc();

    // Track current offset per partition
    let mut offsets: HashMap<u32, u64> = HashMap::new();
    for p in 0..partitions {
        offsets.insert(p, 0);
    }

    let mut interval = tokio::time::interval(Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        for partition in 0..partitions {
            let offset = offsets.get(&partition).copied().unwrap_or(0);

            let path = format!(
                "/api/v1/consume?topic={}&partition={}&offset={}&maxRecords=100",
                topic, partition, offset
            );

            let start = Instant::now();
            match client.get_json::<serde_json::Value>(&path).await {
                Ok(resp) => {
                    let elapsed = start.elapsed().as_secs_f64();
                    metrics::CONSUME_LATENCY.observe(elapsed);

                    let records = resp
                        .get("records")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);

                    if records > 0 {
                        let next_offset = resp
                            .get("next_offset")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(offset + records as u64);

                        offsets.insert(partition, next_offset);

                        metrics::CONSUMED_TOTAL
                            .with_label_values(&[&org_slug, &topic, CONSUMER_GROUP])
                            .inc_by(records as u64);

                        if let Some(counter) = consumed_counts.get(&topic) {
                            counter.fetch_add(records as u64, Ordering::Relaxed);
                        }

                        // Commit offset
                        let commit_body = serde_json::json!({
                            "groupId": CONSUMER_GROUP,
                            "topic": topic,
                            "partition": partition,
                            "offset": next_offset
                        });
                        if let Err(e) = client
                            .post_json::<serde_json::Value>(
                                "/api/v1/consumer-groups/commit",
                                &commit_body,
                            )
                            .await
                        {
                            tracing::debug!(topic = %topic, partition, err = %e, "Offset commit failed");
                        }
                    }
                }
                Err(e) => {
                    metrics::CONSUME_ERRORS
                        .with_label_values(&[&org_slug, &topic])
                        .inc();
                    tracing::debug!(topic = %topic, partition, err = %e, "Consume failed");
                }
            }
        }
    }

    metrics::ACTIVE_CONSUMERS.dec();
}
