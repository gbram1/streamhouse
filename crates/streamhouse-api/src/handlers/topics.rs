//! Topic management endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};

use crate::{models::*, AppState};
use streamhouse_metadata::TopicConfig;

fn format_timestamp(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

#[utoipa::path(
    get,
    path = "/api/v1/topics",
    responses(
        (status = 200, description = "List all topics", body = Vec<Topic>)
    ),
    tag = "topics"
)]
pub async fn list_topics(State(state): State<AppState>) -> Result<Json<Vec<Topic>>, StatusCode> {
    let topics = state
        .metadata
        .list_topics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = topics
        .into_iter()
        .map(|t| Topic {
            name: t.name,
            partitions: t.partition_count,
            replication_factor: 1, // Not stored in metadata yet
            created_at: format_timestamp(t.created_at),
        })
        .collect();

    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/v1/topics",
    request_body = CreateTopicRequest,
    responses(
        (status = 201, description = "Topic created", body = Topic),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Topic already exists")
    ),
    tag = "topics"
)]
pub async fn create_topic(
    State(state): State<AppState>,
    Json(req): Json<CreateTopicRequest>,
) -> Result<(StatusCode, Json<Topic>), StatusCode> {
    // Validate input
    if req.name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.partitions == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check if topic already exists
    if state
        .metadata
        .get_topic(&req.name)
        .await
        .unwrap_or(None)
        .is_some()
    {
        return Err(StatusCode::CONFLICT);
    }

    // Create topic
    let config = TopicConfig {
        name: req.name.clone(),
        partition_count: req.partitions,
        retention_ms: None,
        config: std::collections::HashMap::new(),
    };

    state
        .metadata
        .create_topic(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let topic = Topic {
        name: req.name,
        partitions: req.partitions,
        replication_factor: req.replication_factor,
        created_at: Utc::now().to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(topic)))
}

#[utoipa::path(
    get,
    path = "/api/v1/topics/{name}",
    params(
        ("name" = String, Path, description = "Topic name")
    ),
    responses(
        (status = 200, description = "Topic details", body = Topic),
        (status = 404, description = "Topic not found")
    ),
    tag = "topics"
)]
pub async fn get_topic(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Topic>, StatusCode> {
    let topic = state
        .metadata
        .get_topic(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(Topic {
        name: topic.name,
        partitions: topic.partition_count,
        replication_factor: 1, // Not stored in metadata yet
        created_at: format_timestamp(topic.created_at),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/topics/{name}",
    params(
        ("name" = String, Path, description = "Topic name")
    ),
    responses(
        (status = 204, description = "Topic deleted"),
        (status = 404, description = "Topic not found")
    ),
    tag = "topics"
)]
pub async fn delete_topic(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Check if topic exists
    state
        .metadata
        .get_topic(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Delete topic
    state
        .metadata
        .delete_topic(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/topics/{name}/partitions",
    params(
        ("name" = String, Path, description = "Topic name")
    ),
    responses(
        (status = 200, description = "List partitions", body = Vec<Partition>),
        (status = 404, description = "Topic not found")
    ),
    tag = "topics"
)]
pub async fn list_partitions(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<Partition>>, StatusCode> {
    // Check if topic exists
    state
        .metadata
        .get_topic(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Get partitions
    let partitions = state
        .metadata
        .list_partitions(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = partitions
        .into_iter()
        .map(|p| Partition {
            topic: p.topic,
            partition_id: p.partition_id,
            leader_agent_id: None, // Will query from partition_leases
            high_watermark: p.high_watermark,
            low_watermark: 0, // Not tracked yet
        })
        .collect();

    Ok(Json(response))
}
