//! Data integrity validation: produced == consumed per topic.

use crate::metrics;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// Maximum allowed gap between produced and consumed before flagging data loss.
/// This accounts for in-flight messages and WAL buffering.
const DATA_LOSS_THRESHOLD: u64 = 10_000;

pub async fn run_integrity_checker(
    produced_counts: Arc<HashMap<String, AtomicU64>>,
    consumed_counts: Arc<HashMap<String, AtomicU64>>,
    topic_org_map: HashMap<String, String>,
    interval_secs: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        let mut all_ok = true;

        for (topic, produced_counter) in produced_counts.iter() {
            let produced = produced_counter.load(Ordering::Relaxed);
            let consumed = consumed_counts
                .get(topic)
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0);

            let org = topic_org_map.get(topic).map(|s| s.as_str()).unwrap_or("unknown");

            if produced > consumed + DATA_LOSS_THRESHOLD {
                metrics::DATA_LOSS_EVENTS
                    .with_label_values(&[org, topic])
                    .inc();
                tracing::warn!(
                    topic,
                    produced,
                    consumed,
                    gap = produced - consumed,
                    "Possible data loss detected"
                );
                all_ok = false;
            }
        }

        if all_ok {
            metrics::INTEGRITY_CHECKS.with_label_values(&["pass"]).inc();
        } else {
            metrics::INTEGRITY_CHECKS.with_label_values(&["fail"]).inc();
        }
    }
}
