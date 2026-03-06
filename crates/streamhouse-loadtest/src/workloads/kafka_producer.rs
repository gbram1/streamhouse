//! Kafka protocol producer workload (raw TCP, LZ4 compressed batches).

use crate::kafka_wire;
use crate::metrics;
use rand::Rng;
use rand::SeedableRng;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::watch;

pub async fn run_kafka_producer(
    kafka_addr: String,
    org_slug: String,
    topic: String,
    partitions: u32,
    batch_size: usize,
    produce_rate: usize,
    api_key: Option<String>,
    produced_counts: Arc<std::collections::HashMap<String, AtomicU64>>,
    mut shutdown: watch::Receiver<bool>,
) {
    metrics::ACTIVE_PRODUCERS.inc();
    let client_id = format!("lt-kafka-{}", &topic);

    let interval_ms = if produce_rate > 0 {
        (batch_size as u64 * 1000) / produce_rate as u64
    } else {
        1000
    };
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms.max(10)));

    // Connect with retry
    let mut stream = loop {
        match TcpStream::connect(&kafka_addr).await {
            Ok(s) => break s,
            Err(e) => {
                tracing::warn!(addr = %kafka_addr, err = %e, "Kafka connect failed, retrying...");
                tokio::time::sleep(Duration::from_secs(2)).await;
                if *shutdown.borrow() {
                    metrics::ACTIVE_PRODUCERS.dec();
                    return;
                }
            }
        }
    };

    // Handshake
    let req = kafka_wire::build_api_versions_request(0, &client_id);
    if let Err(e) = kafka_wire::send_request(&mut stream, &req).await {
        tracing::error!(err = %e, "Kafka handshake send failed");
        metrics::ACTIVE_PRODUCERS.dec();
        return;
    }
    if let Err(e) = kafka_wire::recv_response(&mut stream).await {
        tracing::error!(err = %e, "Kafka handshake recv failed");
        metrics::ACTIVE_PRODUCERS.dec();
        return;
    }

    // SASL authentication (if API key available)
    if let Some(ref key) = api_key {
        let sasl_handshake = kafka_wire::build_sasl_handshake_request(1, &client_id, "PLAIN");
        if let Err(e) = kafka_wire::send_request(&mut stream, &sasl_handshake).await {
            tracing::error!(err = %e, "SASL handshake send failed");
            metrics::ACTIVE_PRODUCERS.dec();
            return;
        }
        match kafka_wire::recv_response(&mut stream).await {
            Ok(resp) => {
                let err_code = kafka_wire::parse_sasl_handshake_error(&resp);
                if err_code != 0 {
                    tracing::error!(error_code = err_code, "SASL handshake error");
                    metrics::ACTIVE_PRODUCERS.dec();
                    return;
                }
            }
            Err(e) => {
                tracing::error!(err = %e, "SASL handshake recv failed");
                metrics::ACTIVE_PRODUCERS.dec();
                return;
            }
        }

        let sasl_auth = kafka_wire::build_sasl_authenticate_request(2, &client_id, key, key);
        if let Err(e) = kafka_wire::send_request(&mut stream, &sasl_auth).await {
            tracing::error!(err = %e, "SASL authenticate send failed");
            metrics::ACTIVE_PRODUCERS.dec();
            return;
        }
        match kafka_wire::recv_response(&mut stream).await {
            Ok(resp) => {
                let err_code = kafka_wire::parse_sasl_authenticate_error(&resp);
                if err_code != 0 {
                    tracing::error!(error_code = err_code, "SASL authenticate error");
                    metrics::ACTIVE_PRODUCERS.dec();
                    return;
                }
                tracing::info!(topic = %topic, "Kafka SASL auth successful");
            }
            Err(e) => {
                tracing::error!(err = %e, "SASL authenticate recv failed");
                metrics::ACTIVE_PRODUCERS.dec();
                return;
            }
        }
    }

    let mut rng = rand::rngs::StdRng::from_entropy();
    let mut seq: u64 = 0;
    let mut corr_id: i32 = 1;

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        let partition: i32 = rng.gen_range(0..partitions as i32);

        let records: Vec<(Vec<u8>, Vec<u8>)> = (0..batch_size)
            .map(|_| {
                seq += 1;
                let key = format!("key-{}", rng.gen_range(0u32..1000)).into_bytes();
                let value = format!(
                    r#"{{"seq":{},"ts":{},"topic":"{}","org":"{}"}}"#,
                    seq,
                    chrono::Utc::now().timestamp_millis(),
                    topic,
                    org_slug
                )
                .into_bytes();
                (key, value)
            })
            .collect();

        let req =
            kafka_wire::build_batched_produce_request(corr_id, &client_id, &topic, partition, &records);
        corr_id += 1;

        let start = Instant::now();
        if let Err(e) = kafka_wire::send_request(&mut stream, &req).await {
            tracing::warn!(topic = %topic, err = %e, "Kafka send failed, reconnecting");
            metrics::PRODUCE_ERRORS
                .with_label_values(&[&org_slug, &topic, "kafka"])
                .inc();
            // Try to reconnect
            if let Ok(new_stream) = TcpStream::connect(&kafka_addr).await {
                stream = new_stream;
                let req = kafka_wire::build_api_versions_request(0, &client_id);
                let _ = kafka_wire::send_request(&mut stream, &req).await;
                let _ = kafka_wire::recv_response(&mut stream).await;
            }
            continue;
        }

        match kafka_wire::recv_response(&mut stream).await {
            Ok(resp) => {
                let elapsed = start.elapsed().as_secs_f64();
                let error_code = kafka_wire::parse_produce_error(&resp);
                if error_code == 0 {
                    metrics::PRODUCE_LATENCY
                        .with_label_values(&["kafka"])
                        .observe(elapsed);
                    metrics::PRODUCED_TOTAL
                        .with_label_values(&[&org_slug, &topic, "kafka"])
                        .inc_by(batch_size as u64);
                    if let Some(counter) = produced_counts.get(&topic) {
                        counter.fetch_add(batch_size as u64, Ordering::Relaxed);
                    }
                } else {
                    metrics::PRODUCE_ERRORS
                        .with_label_values(&[&org_slug, &topic, "kafka"])
                        .inc();
                    tracing::warn!(topic = %topic, error_code, "Kafka produce error");
                }
            }
            Err(e) => {
                metrics::PRODUCE_ERRORS
                    .with_label_values(&[&org_slug, &topic, "kafka"])
                    .inc();
                tracing::warn!(topic = %topic, err = %e, "Kafka recv failed");
            }
        }
    }

    metrics::ACTIVE_PRODUCERS.dec();
}
