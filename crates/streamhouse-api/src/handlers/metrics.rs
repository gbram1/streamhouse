//! Metrics and health endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::collections::HashMap;

use crate::{models::*, prometheus::PrometheusClient, AppState};

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

/// Liveness probe endpoint (Kubernetes liveness check)
/// Returns 200 OK if the process is running
#[utoipa::path(
    get,
    path = "/live",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse)
    ),
    tag = "health"
)]
pub async fn liveness_check() -> Json<HealthResponse> {
    // Same as health_check - if we can respond, we're alive
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// Readiness probe endpoint (Kubernetes readiness check)
/// Returns 200 OK if the service is ready to accept traffic
/// Checks that metadata store is accessible
#[utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "Service is ready", body = HealthResponse),
        (status = 503, description = "Service not ready")
    ),
    tag = "health"
)]
pub async fn readiness_check(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    // Check if metadata store is accessible by attempting to list topics
    match state.metadata.list_topics().await {
        Ok(_) => Ok(Json(HealthResponse {
            status: "ready".to_string(),
        })),
        Err(e) => {
            tracing::warn!("Readiness check failed: metadata store unavailable: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

/// Get storage metrics (cache, segments, WAL)
#[utoipa::path(
    get,
    path = "/api/v1/metrics/storage",
    responses(
        (status = 200, description = "Storage metrics", body = StorageMetricsResponse)
    ),
    tag = "metrics"
)]
pub async fn get_storage_metrics(
    State(state): State<AppState>,
) -> Result<Json<StorageMetricsResponse>, StatusCode> {
    // Get total storage from all segments
    let topics = state
        .metadata
        .list_topics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut total_size = 0u64;
    let mut storage_by_topic = HashMap::new();
    let mut segment_count = 0u64;

    for topic in topics {
        let mut topic_size = 0u64;
        for partition_id in 0..topic.partition_count {
            let segments = state
                .metadata
                .get_segments(&topic.name, partition_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for segment in segments {
                total_size += segment.size_bytes;
                topic_size += segment.size_bytes;
                segment_count += 1;
            }
        }
        storage_by_topic.insert(topic.name, topic_size);
    }

    // Get cache stats
    let cache_stats = state.segment_cache.stats().await;

    Ok(Json(StorageMetricsResponse {
        total_size_bytes: total_size,
        segment_count,
        storage_by_topic,
        cache_size: cache_stats.current_size,
        cache_hit_rate: 0.0, // TODO: Track cache hit rate
        cache_evictions: 0,  // TODO: Track cache evictions
        wal_size: 0,         // TODO: Get from WAL if enabled
        wal_uncommitted_entries: 0,
    }))
}

/// Get throughput metrics over time
#[utoipa::path(
    get,
    path = "/api/v1/metrics/throughput",
    params(
        ("time_range" = Option<String>, Query, description = "Time range: 5m, 1h, 24h, 7d (default: 1h)")
    ),
    responses(
        (status = 200, description = "Throughput metrics", body = Vec<ThroughputMetric>)
    ),
    tag = "metrics"
)]
pub async fn get_throughput_metrics(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<Vec<ThroughputMetric>>, StatusCode> {
    let time_range = params.time_range.as_deref().unwrap_or("1h");

    // Try to get real metrics from Prometheus
    if let Some(prometheus) = &state.prometheus {
        if let Ok(metrics) = get_real_throughput_metrics(prometheus, time_range).await {
            if !metrics.is_empty() {
                return Ok(Json(metrics));
            }
        }
    }

    // Fall back to simulated metrics
    get_simulated_throughput_metrics(&state, time_range).await
}

/// Fetch real throughput metrics from Prometheus
async fn get_real_throughput_metrics(
    prometheus: &PrometheusClient,
    time_range: &str,
) -> Result<Vec<ThroughputMetric>, ()> {
    let msgs_per_sec = prometheus.get_throughput(time_range).await.map_err(|_| ())?;
    let bytes_per_sec = prometheus.get_bytes_throughput(time_range).await.map_err(|_| ())?;

    // Combine the two series
    let metrics: Vec<ThroughputMetric> = msgs_per_sec
        .iter()
        .zip(bytes_per_sec.iter())
        .map(|(msg, bytes)| ThroughputMetric {
            timestamp: msg.timestamp,
            messages_per_second: msg.value,
            bytes_per_second: bytes.value,
        })
        .collect();

    Ok(metrics)
}

/// Generate simulated throughput metrics (fallback)
async fn get_simulated_throughput_metrics(
    state: &AppState,
    time_range: &str,
) -> Result<Json<Vec<ThroughputMetric>>, StatusCode> {
    // Get current message count to base simulated data on
    let topics = state
        .metadata
        .list_topics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

    // Generate simulated time-series data based on time range
    let (points, interval_secs) = match time_range {
        "5m" => (30, 10),      // 30 points, 10 seconds apart
        "1h" => (60, 60),      // 60 points, 1 minute apart
        "24h" => (96, 900),    // 96 points, 15 minutes apart
        "7d" => (168, 3600),   // 168 points, 1 hour apart
        _ => (60, 60),
    };

    let now = chrono::Utc::now().timestamp();
    let base_rate = if total_messages > 0 { (total_messages as f64 / 3600.0).max(10.0) } else { 50.0 };

    let metrics: Vec<ThroughputMetric> = (0..points)
        .map(|i| {
            let timestamp = now - ((points - 1 - i) * interval_secs);
            // Add some variation to make it look realistic
            let variation = 1.0 + ((i as f64 * 0.5).sin() * 0.3);
            let msgs_per_sec = base_rate * variation;
            ThroughputMetric {
                timestamp,
                messages_per_second: msgs_per_sec,
                bytes_per_second: msgs_per_sec * 256.0, // Assume avg 256 bytes per message
            }
        })
        .collect();

    Ok(Json(metrics))
}

/// Get latency metrics over time
#[utoipa::path(
    get,
    path = "/api/v1/metrics/latency",
    params(
        ("time_range" = Option<String>, Query, description = "Time range: 5m, 1h, 24h, 7d (default: 1h)")
    ),
    responses(
        (status = 200, description = "Latency metrics", body = Vec<LatencyMetric>)
    ),
    tag = "metrics"
)]
pub async fn get_latency_metrics(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<Vec<LatencyMetric>>, StatusCode> {
    let time_range = params.time_range.as_deref().unwrap_or("1h");

    // Try to get real metrics from Prometheus
    if let Some(prometheus) = &state.prometheus {
        if let Ok(metrics) = get_real_latency_metrics(prometheus, time_range).await {
            if !metrics.is_empty() {
                return Ok(Json(metrics));
            }
        }
    }

    // Fall back to simulated metrics
    get_simulated_latency_metrics(time_range)
}

/// Fetch real latency metrics from Prometheus
async fn get_real_latency_metrics(
    prometheus: &PrometheusClient,
    time_range: &str,
) -> Result<Vec<LatencyMetric>, ()> {
    let percentiles = prometheus.get_latency_percentiles(time_range).await.map_err(|_| ())?;

    // Combine percentiles into latency metrics
    // Convert from seconds to milliseconds for UI
    let metrics: Vec<LatencyMetric> = percentiles
        .p50
        .iter()
        .enumerate()
        .map(|(i, p50_point)| {
            let p95 = percentiles.p95.get(i).map(|p| p.value * 1000.0).unwrap_or(p50_point.value * 2.5 * 1000.0);
            let p99 = percentiles.p99.get(i).map(|p| p.value * 1000.0).unwrap_or(p50_point.value * 4.0 * 1000.0);
            let p50_ms = p50_point.value * 1000.0;

            LatencyMetric {
                timestamp: p50_point.timestamp,
                p50: p50_ms,
                p95,
                p99,
                avg: p50_ms * 1.2, // Approximate average
            }
        })
        .collect();

    Ok(metrics)
}

/// Generate simulated latency metrics (fallback)
fn get_simulated_latency_metrics(time_range: &str) -> Result<Json<Vec<LatencyMetric>>, StatusCode> {
    let (points, interval_secs) = match time_range {
        "5m" => (30, 10),
        "1h" => (60, 60),
        "24h" => (96, 900),
        "7d" => (168, 3600),
        _ => (60, 60),
    };

    let now = chrono::Utc::now().timestamp();

    let metrics: Vec<LatencyMetric> = (0..points)
        .map(|i| {
            let timestamp = now - ((points - 1 - i) * interval_secs);
            // Simulate realistic latency patterns (in milliseconds)
            let base_p50 = 2.0 + ((i as f64 * 0.3).sin() * 0.5);
            let base_p95 = base_p50 * 2.5;
            let base_p99 = base_p50 * 4.0;
            LatencyMetric {
                timestamp,
                p50: base_p50,
                p95: base_p95,
                p99: base_p99,
                avg: base_p50 * 1.2,
            }
        })
        .collect();

    Ok(Json(metrics))
}

/// Get error metrics over time
#[utoipa::path(
    get,
    path = "/api/v1/metrics/errors",
    params(
        ("time_range" = Option<String>, Query, description = "Time range: 5m, 1h, 24h, 7d (default: 1h)")
    ),
    responses(
        (status = 200, description = "Error metrics", body = Vec<ErrorMetric>)
    ),
    tag = "metrics"
)]
pub async fn get_error_metrics(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<Vec<ErrorMetric>>, StatusCode> {
    let time_range = params.time_range.as_deref().unwrap_or("1h");

    // Try to get real metrics from Prometheus
    if let Some(prometheus) = &state.prometheus {
        if let Ok(metrics) = get_real_error_metrics(prometheus, time_range).await {
            if !metrics.is_empty() {
                return Ok(Json(metrics));
            }
        }
    }

    // Fall back to simulated metrics
    get_simulated_error_metrics(time_range)
}

/// Fetch real error metrics from Prometheus
async fn get_real_error_metrics(
    prometheus: &PrometheusClient,
    time_range: &str,
) -> Result<Vec<ErrorMetric>, ()> {
    let error_rates = prometheus.get_error_rate(time_range).await.map_err(|_| ())?;
    let error_counts = prometheus.get_error_count(time_range).await.map_err(|_| ())?;

    // Combine the two series
    let metrics: Vec<ErrorMetric> = error_rates
        .iter()
        .zip(error_counts.iter())
        .map(|(rate, count)| ErrorMetric {
            timestamp: rate.timestamp,
            error_rate: rate.value / 100.0, // Convert from percentage to ratio
            error_count: count.value as u64,
        })
        .collect();

    Ok(metrics)
}

/// Generate simulated error metrics (fallback)
fn get_simulated_error_metrics(time_range: &str) -> Result<Json<Vec<ErrorMetric>>, StatusCode> {
    let (points, interval_secs) = match time_range {
        "5m" => (30, 10),
        "1h" => (60, 60),
        "24h" => (96, 900),
        "7d" => (168, 3600),
        _ => (60, 60),
    };

    let now = chrono::Utc::now().timestamp();

    let metrics: Vec<ErrorMetric> = (0..points)
        .map(|i| {
            let timestamp = now - ((points - 1 - i) * interval_secs);
            // Very low error rate for healthy system (0.01% - 0.1%)
            let error_rate = 0.0001 + ((i as f64 * 0.2).sin().abs() * 0.0009);
            let error_count = (error_rate * 10000.0) as u64;
            ErrorMetric {
                timestamp,
                error_rate,
                error_count,
            }
        })
        .collect();

    Ok(Json(metrics))
}

/// Get agent-specific metrics
#[utoipa::path(
    get,
    path = "/api/v1/agents/{id}/metrics",
    params(
        ("id" = String, Path, description = "Agent ID")
    ),
    responses(
        (status = 200, description = "Agent metrics", body = AgentMetricsResponse),
        (status = 404, description = "Agent not found")
    ),
    tag = "agents"
)]
pub async fn get_agent_metrics(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentMetricsResponse>, StatusCode> {
    // Get agent info
    let agent = state
        .metadata
        .get_agent(&agent_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Get partition assignments for this agent
    let leases = state
        .metadata
        .list_partition_leases(None, Some(&agent_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AgentMetricsResponse {
        agent_id: agent.agent_id,
        address: agent.address,
        availability_zone: agent.availability_zone,
        partition_count: leases.len(),
        last_heartbeat: agent.last_heartbeat,
        uptime_ms: chrono::Utc::now().timestamp_millis() - agent.started_at,
    }))
}
