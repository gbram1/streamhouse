//! Message consume endpoint

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;

use crate::models::{ConsumeResponse, ConsumedRecord};
use crate::AppState;
use streamhouse_metadata::DEFAULT_ORGANIZATION_ID;

/// Extract organization ID from request headers, defaulting to DEFAULT_ORGANIZATION_ID.
fn extract_org_id(headers: &HeaderMap) -> String {
    headers
        .get("x-organization-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_ORGANIZATION_ID.to_string())
}

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
    axum::extract::Query(req): axum::extract::Query<ConsumeRequest>,
) -> Result<Json<ConsumeResponse>, StatusCode> {
    let org_id = extract_org_id(&headers);

    // Verify topic belongs to org
    if state
        .metadata
        .get_topic_for_org(&org_id, &req.topic)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }

    // Validate topic exists
    let topic = state
        .metadata
        .get_topic(&req.topic)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Validate partition exists
    if req.partition >= topic.partition_count {
        return Err(StatusCode::NOT_FOUND);
    }

    // Create a partition reader
    use streamhouse_storage::PartitionReader;

    let reader = PartitionReader::new(
        req.topic.clone(),
        req.partition,
        state.metadata.clone(),
        state.object_store.clone(),
        state.segment_cache.clone(),
    );

    // Read records
    let result = reader
        .read(req.offset, req.max_records)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

    Ok(Json(ConsumeResponse {
        records,
        next_offset,
    }))
}
