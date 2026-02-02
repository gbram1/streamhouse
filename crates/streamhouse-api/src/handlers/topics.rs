//! Topic management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};

use crate::{models::*, AppState};
use streamhouse_metadata::TopicConfig;

fn format_timestamp(timestamp_millis: i64) -> String {
    // Convert milliseconds to seconds
    let timestamp_secs = timestamp_millis / 1000;
    let nanos = ((timestamp_millis % 1000) * 1_000_000) as u32;

    DateTime::from_timestamp(timestamp_secs, nanos)
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

    let mut response = Vec::new();
    for t in topics {
        // Calculate message count and size from segments across all partitions
        let mut total_messages: u64 = 0;
        let mut total_size: u64 = 0;

        for partition_id in 0..t.partition_count {
            if let Ok(segments) = state.metadata.get_segments(&t.name, partition_id).await {
                for seg in segments {
                    total_messages += seg.record_count as u64;
                    total_size += seg.size_bytes;
                }
            }
        }

        response.push(Topic {
            name: t.name,
            partitions: t.partition_count,
            replication_factor: 1, // Not stored in metadata yet
            created_at: format_timestamp(t.created_at),
            message_count: total_messages,
            size_bytes: total_size,
        });
    }

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
        message_count: 0,
        size_bytes: 0,
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

    // Calculate message count and size from segments across all partitions
    let mut total_messages: u64 = 0;
    let mut total_size: u64 = 0;

    for partition_id in 0..topic.partition_count {
        if let Ok(segments) = state.metadata.get_segments(&topic.name, partition_id).await {
            for seg in segments {
                total_messages += seg.record_count as u64;
                total_size += seg.size_bytes;
            }
        }
    }

    Ok(Json(Topic {
        name: topic.name,
        partitions: topic.partition_count,
        replication_factor: 1, // Not stored in metadata yet
        created_at: format_timestamp(topic.created_at),
        message_count: total_messages,
        size_bytes: total_size,
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

#[utoipa::path(
    get,
    path = "/api/v1/topics/{name}/messages",
    params(
        ("name" = String, Path, description = "Topic name"),
        ("partition" = Option<u32>, Query, description = "Partition ID (optional)"),
        ("offset" = Option<u64>, Query, description = "Starting offset"),
        ("limit" = Option<usize>, Query, description = "Max messages to return (default: 50, max: 1000)")
    ),
    responses(
        (status = 200, description = "Messages retrieved", body = Vec<ConsumedRecord>),
        (status = 404, description = "Topic not found")
    ),
    tag = "topics"
)]
pub async fn get_topic_messages(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<MessageQueryParams>,
) -> Result<Json<Vec<ConsumedRecord>>, StatusCode> {
    use bytes::Bytes;
    use object_store::path::Path as ObjectPath;
    use streamhouse_storage::segment::SegmentReader;

    // Validate topic exists
    state
        .metadata
        .get_topic(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Get partition (default to 0 if not specified)
    let partition_id = params.partition.unwrap_or(0);

    // Get starting offset (default to 0)
    let start_offset = params.offset.unwrap_or(0);

    // Limit messages (default: 50, max: 1000)
    let limit = params.limit.unwrap_or(50).min(1000);

    // Get segments for this partition
    let segments = state
        .metadata
        .get_segments(&name, partition_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut messages = Vec::new();

    // Read messages from segments
    for segment in segments {
        // Skip segments that are entirely before our start offset
        if segment.end_offset < start_offset {
            continue;
        }

        // Try to read segment from object store
        let path = ObjectPath::from(segment.s3_key.clone());
        let get_result = state
            .object_store
            .get(&path)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get segment from S3: {} - {}", segment.s3_key, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let segment_bytes = get_result
            .bytes()
            .await
            .map_err(|e| {
                tracing::error!("Failed to read segment bytes: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        // Parse segment and read records
        let reader = SegmentReader::new(Bytes::from(segment_bytes.to_vec()))
            .map_err(|e| {
                tracing::error!("Failed to parse segment: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let records = if start_offset > segment.base_offset {
            reader.read_from_offset(start_offset)
        } else {
            reader.read_all()
        }
        .map_err(|e| {
            tracing::error!("Failed to read segment records: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Convert records to API format
        for record in records {
            if messages.len() >= limit {
                break;
            }

            messages.push(ConsumedRecord {
                partition: partition_id,
                offset: record.offset,
                key: record.key.map(|k| String::from_utf8_lossy(&k).to_string()),
                value: String::from_utf8_lossy(&record.value).to_string(),
                timestamp: record.timestamp as i64,
            });
        }

        if messages.len() >= limit {
            break;
        }
    }

    Ok(Json(messages))
}
