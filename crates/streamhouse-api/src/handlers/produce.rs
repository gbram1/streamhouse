//! Message produce endpoint

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use bytes::Bytes;
use std::collections::HashMap;

use crate::{models::*, AppState};
use streamhouse_metadata::DEFAULT_ORGANIZATION_ID;

/// Validate a value against the schema registered for `{topic}-value`.
/// Returns Ok(()) if no schema is registered or validation passes.
async fn validate_value_against_schema(
    registry: &streamhouse_schema_registry::SchemaRegistry,
    topic: &str,
    value: &str,
) -> Result<(), (StatusCode, String)> {
    let subject = format!("{}-value", topic);
    let schema = match registry.get_latest_schema(&subject).await {
        Ok(s) => s,
        Err(_) => return Ok(()), // No schema registered → skip validation
    };

    match schema.schema_type {
        streamhouse_schema_registry::SchemaFormat::Json => {
            validate_json_schema(&schema.schema, value)
        }
        streamhouse_schema_registry::SchemaFormat::Avro => {
            validate_avro_schema(&schema.schema, value)
        }
        streamhouse_schema_registry::SchemaFormat::Protobuf => Ok(()), // Not implemented
    }
}

fn validate_json_schema(schema_str: &str, value: &str) -> Result<(), (StatusCode, String)> {
    let schema_value: serde_json::Value = serde_json::from_str(schema_str).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid schema definition: {}", e),
        )
    })?;

    let instance: serde_json::Value = serde_json::from_str(value).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Value is not valid JSON: {}", e),
        )
    })?;

    let validator = jsonschema::validator_for(&schema_value).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to compile JSON schema: {}", e),
        )
    })?;

    let errors: Vec<String> = validator.iter_errors(&instance).map(|e| e.to_string()).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            format!("Schema validation failed: {}", errors.join("; ")),
        ))
    }
}

fn validate_avro_schema(schema_str: &str, value: &str) -> Result<(), (StatusCode, String)> {
    let schema = apache_avro::Schema::parse_str(schema_str).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid Avro schema: {}", e),
        )
    })?;

    let json_value: serde_json::Value = serde_json::from_str(value).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Value is not valid JSON: {}", e),
        )
    })?;

    let avro_value = apache_avro::types::Value::from(json_value);
    if avro_value.resolve(&schema).is_ok() {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            format!("Value does not conform to Avro schema"),
        ))
    }
}

/// Extract organization ID from request headers, defaulting to DEFAULT_ORGANIZATION_ID.
fn extract_org_id(headers: &HeaderMap) -> String {
    headers
        .get("x-organization-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_ORGANIZATION_ID.to_string())
}

#[utoipa::path(
    post,
    path = "/api/v1/produce",
    request_body = ProduceRequest,
    responses(
        (status = 200, description = "Message produced", body = ProduceResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Topic not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "produce"
)]
pub async fn produce(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ProduceRequest>,
) -> Result<Json<ProduceResponse>, StatusCode> {
    let org_id = extract_org_id(&headers);

    // Verify topic belongs to org
    if let Err(e) = state.metadata.get_topic_for_org(&org_id, &req.topic).await {
        tracing::error!("get_topic_for_org failed: org={}, topic={}, err={:?}", org_id, req.topic, e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Validate topic exists
    let topic = match state.metadata.get_topic(&req.topic).await {
        Ok(Some(t)) => t,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("get_topic failed: topic={}, err={:?}", req.topic, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Validate value against schema (if schema registry is available)
    if let Some(registry) = &state.schema_registry {
        if let Err((status, msg)) =
            validate_value_against_schema(registry, &req.topic, &req.value).await
        {
            tracing::warn!("Schema validation failed: topic={}, err={}", req.topic, msg);
            return Err(status);
        }
    }

    // If WriterPool is available, write directly (unified server mode)
    if let Some(writer_pool) = &state.writer_pool {
        // Determine partition
        let partition = req.partition.unwrap_or(0);

        // Validate partition exists
        if partition >= topic.partition_count {
            return Err(StatusCode::BAD_REQUEST);
        }

        // Get writer for partition
        let writer = match writer_pool.get_writer(&req.topic, partition).await {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("get_writer failed: topic={}, partition={}, err={:?}", req.topic, partition, e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        // Prepare record
        let key = req.key.as_deref().map(|k| Bytes::from(k.to_string()));
        let value = Bytes::from(req.value);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Write record
        let offset = {
            let mut writer_guard = writer.lock().await;
            match writer_guard.append(key, value, timestamp).await {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!("append failed: topic={}, partition={}, err={:?}", req.topic, partition, e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        };

        return Ok(Json(ProduceResponse { offset, partition }));
    }

    // Otherwise, use Producer client (distributed mode)
    if let Some(producer) = &state.producer {
        let mut result = producer
            .send(
                &req.topic,
                req.key.as_deref().map(|k| k.as_bytes()),
                req.value.as_bytes(),
                req.partition,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Immediately flush to send the batch and get the offset
        producer
            .flush()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Wait for the offset to be assigned (should be immediate after flush)
        let offset = result
            .wait_offset()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        return Ok(Json(ProduceResponse {
            offset,
            partition: result.partition,
        }));
    }

    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

/// Batch produce - send many messages in a single request for high throughput
#[utoipa::path(
    post,
    path = "/api/v1/produce/batch",
    request_body = BatchProduceRequest,
    responses(
        (status = 200, description = "Messages produced", body = BatchProduceResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Topic not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "produce"
)]
pub async fn produce_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BatchProduceRequest>,
) -> Result<Json<BatchProduceResponse>, StatusCode> {
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

    // Validate all record values against schema (if schema registry is available)
    if let Some(registry) = &state.schema_registry {
        for (idx, record) in req.records.iter().enumerate() {
            if let Err((status, msg)) =
                validate_value_against_schema(registry, &req.topic, &record.value).await
            {
                tracing::warn!(
                    "Schema validation failed: topic={}, record={}, err={}",
                    req.topic,
                    idx,
                    msg
                );
                return Err(status);
            }
        }
    }

    // If WriterPool is available, write directly (unified server mode)
    if let Some(writer_pool) = &state.writer_pool {
        let mut results = Vec::with_capacity(req.records.len());

        // Group records by partition for efficient batching
        let mut by_partition: HashMap<u32, Vec<(usize, &BatchRecord)>> = HashMap::new();
        for (idx, record) in req.records.iter().enumerate() {
            let partition = record
                .partition
                .unwrap_or(idx as u32 % topic.partition_count);
            if partition >= topic.partition_count {
                return Err(StatusCode::BAD_REQUEST);
            }
            by_partition
                .entry(partition)
                .or_default()
                .push((idx, record));
        }

        // Write to each partition
        for (partition, records) in by_partition {
            let writer = writer_pool
                .get_writer(&req.topic, partition)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let mut writer_guard = writer.lock().await;
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            for (_idx, record) in records {
                let key = record.key.as_deref().map(|k| Bytes::from(k.to_string()));
                let value = Bytes::from(record.value.clone());

                let offset = writer_guard
                    .append(key, value, timestamp)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                results.push(BatchRecordResult { partition, offset });
            }
        }

        return Ok(Json(BatchProduceResponse {
            count: results.len(),
            offsets: results,
        }));
    }

    Err(StatusCode::INTERNAL_SERVER_ERROR)
}
