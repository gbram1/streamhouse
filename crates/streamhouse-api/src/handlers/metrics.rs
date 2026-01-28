//! Metrics and health endpoints

use axum::{extract::State, http::StatusCode, Json};

use crate::{models::*, AppState};

#[utoipa::path(
    get,
    path = "/api/v1/metrics",
    responses(
        (status = 200, description = "Cluster metrics", body = MetricsSnapshot)
    ),
    tag = "metrics"
)]
pub async fn get_metrics(
    State(state): State<AppState>,
) -> Result<Json<MetricsSnapshot>, StatusCode> {
    // Get topics count
    let topics = state
        .metadata
        .list_topics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get agents count
    let agents = state
        .metadata
        .list_agents(None, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Count total partitions
    let mut partitions_count = 0u64;
    for topic in &topics {
        let partitions = state
            .metadata
            .list_partitions(&topic.name)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        partitions_count += partitions.len() as u64;
    }

    // Calculate total messages (sum of high watermarks)
    let mut total_messages = 0u64;
    for topic in &topics {
        let partitions = state
            .metadata
            .list_partitions(&topic.name)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for partition in partitions {
            total_messages += partition.high_watermark;
        }
    }

    Ok(Json(MetricsSnapshot {
        topics_count: topics.len() as u64,
        agents_count: agents.len() as u64,
        partitions_count,
        total_messages,
    }))
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    ),
    tag = "health"
)]
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}
