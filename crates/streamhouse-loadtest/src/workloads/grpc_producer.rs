//! gRPC producer workload using tonic StreamHouseClient.

use crate::metrics;
use rand::Rng;
use rand::SeedableRng;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use streamhouse_proto::streamhouse::{
    stream_house_client::StreamHouseClient, ProduceBatchRequest, Record,
};
use tokio::sync::watch;

pub async fn run_grpc_producer(
    grpc_addr: String,
    org_slug: String,
    topic: String,
    partitions: u32,
    batch_size: usize,
    produce_rate: usize,
    produced_counts: Arc<std::collections::HashMap<String, AtomicU64>>,
    mut shutdown: watch::Receiver<bool>,
) {
    metrics::ACTIVE_PRODUCERS.inc();

    let interval_ms = if produce_rate > 0 {
        (batch_size as u64 * 1000) / produce_rate as u64
    } else {
        1000
    };
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms.max(10)));

    // Connect with retry
    let channel = loop {
        match tonic::transport::Channel::from_shared(grpc_addr.clone())
            .unwrap()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .connect()
            .await
        {
            Ok(ch) => break ch,
            Err(e) => {
                tracing::warn!(addr = %grpc_addr, err = %e, "gRPC connect failed, retrying...");
                tokio::time::sleep(Duration::from_secs(2)).await;
                if *shutdown.borrow() {
                    metrics::ACTIVE_PRODUCERS.dec();
                    return;
                }
            }
        }
    };

    let mut client = StreamHouseClient::new(channel)
        .max_decoding_message_size(64 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024);

    let mut rng = rand::rngs::StdRng::from_entropy();
    let mut seq: u64 = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        let partition: u32 = rng.gen_range(0..partitions);

        let records: Vec<Record> = (0..batch_size)
            .map(|_| {
                seq += 1;
                let key = format!("key-{}", rng.gen_range(0u32..1000));
                let value = format!(
                    r#"{{"seq":{},"ts":{},"topic":"{}","org":"{}"}}"#,
                    seq,
                    chrono::Utc::now().timestamp_millis(),
                    topic,
                    org_slug
                );
                Record {
                    key: key.into_bytes(),
                    value: value.into_bytes(),
                    headers: Default::default(),
                }
            })
            .collect();

        let request = ProduceBatchRequest {
            topic: topic.clone(),
            partition,
            records,
            ..Default::default()
        };

        let start = Instant::now();
        match client.produce_batch(request).await {
            Ok(_) => {
                let elapsed = start.elapsed().as_secs_f64();
                metrics::PRODUCE_LATENCY
                    .with_label_values(&["grpc"])
                    .observe(elapsed);
                metrics::PRODUCED_TOTAL
                    .with_label_values(&[&org_slug, &topic, "grpc"])
                    .inc_by(batch_size as u64);
                if let Some(counter) = produced_counts.get(&topic) {
                    counter.fetch_add(batch_size as u64, Ordering::Relaxed);
                }
            }
            Err(e) => {
                metrics::PRODUCE_ERRORS
                    .with_label_values(&[&org_slug, &topic, "grpc"])
                    .inc();
                tracing::warn!(topic = %topic, err = %e, "gRPC produce failed");
            }
        }
    }

    metrics::ACTIVE_PRODUCERS.dec();
}
