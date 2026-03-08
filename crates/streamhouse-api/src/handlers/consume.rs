//! Message consume endpoint

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use serde::Deserialize;

use crate::auth::AuthenticatedKey;
use crate::handlers::topics::extract_org_id;
use crate::models::{ConsumeResponse, ConsumedRecord};
use crate::AppState;
use streamhouse_observability::metrics;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeRequest {
    pub topic: String,
    pub partition: u32,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_max_records")]
    pub max_records: usize,
}

fn default_max_records() -> usize {
    100
}

#[utoipa::path(
    get,
    path = "/api/v1/consume",
    params(
        ("topic" = String, Query, description = "Topic name"),
        ("partition" = u32, Query, description = "Partition number"),
        ("offset" = u64, Query, description = "Starting offset (default: 0)"),
        ("maxRecords" = usize, Query, description = "Max records to fetch (default: 100)")
    ),
    responses(
        (status = 200, description = "Messages consumed", body = ConsumeResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Topic or partition not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "consume"
)]
pub async fn consume(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    axum::extract::Query(req): axum::extract::Query<ConsumeRequest>,
) -> Result<Json<ConsumeResponse>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));
    let topic_name = req.topic.clone();

    // Verify topic belongs to org and get its metadata
    let topic = state
        .metadata
        .get_topic_for_org(&org_id, &req.topic)
        .await
        .map_err(|_| {
            metrics::CONSUMER_ERRORS_TOTAL.with_label_values(&[&org_id, &topic_name, "rest-api", "metadata_error"]).inc();
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            metrics::CONSUMER_ERRORS_TOTAL.with_label_values(&[&org_id, &topic_name, "rest-api", "topic_not_found"]).inc();
            StatusCode::NOT_FOUND
        })?;

    // Validate partition exists
    if req.partition >= topic.partition_count {
        metrics::CONSUMER_ERRORS_TOTAL.with_label_values(&[&org_id, &topic_name, "rest-api", "invalid_partition"]).inc();
        return Err(StatusCode::NOT_FOUND);
    }

    // Create a partition reader
    use streamhouse_storage::PartitionReader;

    let reader = PartitionReader::new(
        org_id.clone(),
        req.topic.clone(),
        req.partition,
        state.metadata.clone(),
        state.object_store.clone(),
        state.segment_cache.clone(),
    );

    // Read records — OffsetNotFound means the requested offset isn't in any
    // segment (e.g. data hasn't been flushed yet, or was compacted away).
    // Return an empty response with next_offset pointing to the earliest
    // available segment so consumers can skip ahead automatically.
    let result = match reader.read(req.offset, req.max_records).await {
        Ok(result) => result,
        Err(streamhouse_storage::Error::OffsetNotFound(_)) => {
            // Find earliest available offset so consumer can skip ahead
            let next_offset = state
                .metadata
                .get_segments(&org_id, &req.topic, req.partition)
                .await
                .ok()
                .and_then(|segs| segs.first().map(|s| s.base_offset))
                .unwrap_or(req.offset);
            return Ok(Json(ConsumeResponse {
                records: vec![],
                next_offset,
            }));
        }
        Err(_) => {
            metrics::CONSUMER_ERRORS_TOTAL.with_label_values(&[&org_id, &topic_name, "rest-api", "read_error"]).inc();
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Convert to API records
    let records: Vec<ConsumedRecord> = result
        .records
        .iter()
        .enumerate()
        .map(|(i, record)| ConsumedRecord {
            partition: req.partition,
            offset: req.offset + i as u64,
            key: record
                .key
                .as_ref()
                .map(|k| String::from_utf8_lossy(k).to_string()),
            value: String::from_utf8_lossy(&record.value).to_string(),
            timestamp: record.timestamp as i64,
        })
        .collect();

    let next_offset = req.offset + records.len() as u64;

    if !records.is_empty() {
        metrics::CONSUMER_RECORDS_TOTAL.with_label_values(&[&org_id, &topic_name, "rest-api"]).inc_by(records.len() as u64);
    }

    Ok(Json(ConsumeResponse {
        records,
        next_offset,
    }))
}
