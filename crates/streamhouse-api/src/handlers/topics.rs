//! Topic management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};

use crate::{models::*, AppState};
use streamhouse_metadata::TopicConfig;

/// Decode message value, handling Avro Object Container format
/// Returns JSON string if Avro, otherwise returns raw string
fn decode_message_value(data: &[u8]) -> String {
    use apache_avro::Reader;

    // Check for Confluent wire format: [0x00][4-byte schema ID][payload]
    let payload = if data.len() >= 5 && data[0] == 0 {
        &data[5..] // Skip Confluent header
    } else {
        data
    };

    // Check for Avro Object Container magic bytes "Obj"
    if payload.len() >= 4 && &payload[0..3] == b"Obj" {
        if let Ok(reader) = Reader::new(payload) {
            let mut records: Vec<serde_json::Value> = Vec::new();
            for value in reader {
                if let Ok(avro_value) = value {
                    if let Some(json) = avro_to_json(&avro_value) {
                        records.push(json);
                    }
                }
            }
            // Return first record if single, otherwise array
            if records.len() == 1 {
                return serde_json::to_string_pretty(&records[0]).unwrap_or_default();
            } else if !records.is_empty() {
                return serde_json::to_string_pretty(&records).unwrap_or_default();
            }
        }
    }

    // Try parsing as JSON
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(data) {
        return serde_json::to_string_pretty(&json).unwrap_or_default();
    }

    // Fallback to lossy UTF-8
    String::from_utf8_lossy(data).to_string()
}

/// Convert Avro Value to JSON Value
fn avro_to_json(value: &apache_avro::types::Value) -> Option<serde_json::Value> {
    use apache_avro::types::Value;

    match value {
        Value::Null => Some(serde_json::Value::Null),
        Value::Boolean(b) => Some(serde_json::Value::Bool(*b)),
        Value::Int(i) => Some(serde_json::Value::Number((*i).into())),
        Value::Long(l) => Some(serde_json::Value::Number((*l).into())),
        Value::Float(f) => serde_json::Number::from_f64(*f as f64).map(serde_json::Value::Number),
        Value::Double(d) => serde_json::Number::from_f64(*d).map(serde_json::Value::Number),
        Value::String(s) => Some(serde_json::Value::String(s.clone())),
        Value::Bytes(b) => Some(serde_json::Value::String(hex_encode(b))),
        Value::Fixed(_, b) => Some(serde_json::Value::String(hex_encode(b))),
        Value::Enum(_, s) => Some(serde_json::Value::String(s.clone())),
        Value::Union(_, inner) => avro_to_json(inner),
        Value::Array(arr) => {
            let items: Vec<_> = arr.iter().filter_map(avro_to_json).collect();
            Some(serde_json::Value::Array(items))
        }
        Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .filter_map(|(k, v)| avro_to_json(v).map(|jv| (k.clone(), jv)))
                .collect();
            Some(serde_json::Value::Object(obj))
        }
        Value::Record(fields) => {
            let obj: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .filter_map(|(k, v)| avro_to_json(v).map(|jv| (k.clone(), jv)))
                .collect();
            Some(serde_json::Value::Object(obj))
        }
        Value::Date(d) => Some(serde_json::Value::Number((*d).into())),
        Value::TimeMillis(t) => Some(serde_json::Value::Number((*t).into())),
        Value::TimeMicros(t) => Some(serde_json::Value::Number((*t).into())),
        Value::TimestampMillis(t) => Some(serde_json::Value::Number((*t).into())),
        Value::TimestampMicros(t) => Some(serde_json::Value::Number((*t).into())),
        Value::Decimal(d) => Some(serde_json::Value::String(format!("{:?}", d))),
        Value::Uuid(u) => Some(serde_json::Value::String(u.to_string())),
        _ => None,
    }
}

fn hex_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(data.len() * 2);
    for byte in data {
        write!(s, "{:02x}", byte).unwrap();
    }
    s
}

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
        cleanup_policy: Default::default(),
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

    // Get partition leases to find leaders
    let leases = state
        .metadata
        .list_partition_leases(Some(&name), None)
        .await
        .unwrap_or_default();

    // Create a map of partition_id -> leader_agent_id
    let lease_map: std::collections::HashMap<u32, String> = leases
        .into_iter()
        .map(|l| (l.partition_id, l.leader_agent_id))
        .collect();

    let response = partitions
        .into_iter()
        .map(|p| Partition {
            topic: p.topic.clone(),
            partition_id: p.partition_id,
            leader_agent_id: lease_map.get(&p.partition_id).cloned(),
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
        let get_result = state.object_store.get(&path).await.map_err(|e| {
            tracing::error!("Failed to get segment from S3: {} - {}", segment.s3_key, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let segment_bytes = get_result.bytes().await.map_err(|e| {
            tracing::error!("Failed to read segment bytes: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Parse segment and read records
        let reader = SegmentReader::new(Bytes::from(segment_bytes.to_vec())).map_err(|e| {
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
                value: decode_message_value(&record.value),
                timestamp: record.timestamp as i64,
            });
        }

        if messages.len() >= limit {
            break;
        }
    }

    Ok(Json(messages))
}
