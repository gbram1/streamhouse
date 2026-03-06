//! Storage validation: verify S3 segments exist in MinIO.

use crate::metrics;
use std::time::Duration;
use tokio::sync::watch;

pub async fn run_storage_checker(
    minio_url: String,
    bucket: String,
    interval_secs: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        // Check MinIO health
        let health_url = format!("{}/minio/health/live", minio_url);
        match client.get(&health_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::debug!("MinIO health OK");
            }
            Ok(resp) => {
                tracing::warn!(status = %resp.status(), "MinIO health check returned non-200");
                continue;
            }
            Err(e) => {
                tracing::warn!(err = %e, "MinIO health check failed");
                continue;
            }
        }

        // List objects in bucket to count .seg files
        // MinIO S3 API: GET /?list-type=2&prefix=data/
        let list_url = format!(
            "{}/{bucket}?list-type=2&prefix=data/&max-keys=1000",
            minio_url
        );
        match client.get(&list_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                // Count .seg occurrences in the XML listing
                let seg_count = body.matches(".seg").count();
                if seg_count > 0 {
                    metrics::S3_SEGMENTS_VERIFIED.inc_by(seg_count as u64);
                    tracing::info!(segments = seg_count, "S3 segments verified");
                } else {
                    tracing::debug!("No segments found in S3 yet");
                }
            }
            Ok(resp) => {
                tracing::debug!(status = %resp.status(), "S3 list returned non-200 (may need auth)");
            }
            Err(e) => {
                tracing::debug!(err = %e, "S3 list request failed");
            }
        }
    }
}
