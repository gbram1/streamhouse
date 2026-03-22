//! Message produce endpoint

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use std::collections::HashMap;

use crate::auth::AuthenticatedKey;
use crate::handlers::topics::extract_org_id;
use crate::{models::*, AppState};
use std::sync::atomic::{AtomicU32, Ordering};
use streamhouse_observability::metrics;

/// Round-robin counter for keyless partition assignment
static ROUND_ROBIN_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Select partition: explicit > key hash > round-robin
fn select_partition(partition: Option<u32>, key: Option<&str>, partition_count: u32) -> u32 {
    if let Some(p) = partition {
        return p;
    }
    if let Some(k) = key {
        // Murmur2-style hash (matches Kafka's default partitioner)
        let hash = murmur2_hash(k.as_bytes());
        return hash % partition_count;
    }
    // Round-robin for keyless messages
    ROUND_ROBIN_COUNTER.fetch_add(1, Ordering::Relaxed) % partition_count
}

/// Murmur2 hash (compatible with Kafka's default partitioner)
fn murmur2_hash(data: &[u8]) -> u32 {
    let seed: u32 = 0x9747b28c;
    let m: u32 = 0x5bd1e995;
    let r: u32 = 24;
    let len = data.len() as u32;
    let mut h: u32 = seed ^ len;
    let chunks = data.chunks_exact(4);
    let remainder = chunks.remainder();
    for chunk in chunks {
        let mut k = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        k = k.wrapping_mul(m);
        k ^= k >> r;
        k = k.wrapping_mul(m);
        h = h.wrapping_mul(m);
        h ^= k;
    }
    match remainder.len() {
        3 => {
            h ^= (remainder[2] as u32) << 16;
            h ^= (remainder[1] as u32) << 8;
            h ^= remainder[0] as u32;
            h = h.wrapping_mul(m);
        }
        2 => {
            h ^= (remainder[1] as u32) << 8;
            h ^= remainder[0] as u32;
            h = h.wrapping_mul(m);
        }
        1 => {
            h ^= remainder[0] as u32;
            h = h.wrapping_mul(m);
        }
        _ => {}
    }
    h ^= h >> 13;
    h = h.wrapping_mul(m);
    h ^= h >> 15;
    h
}

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
        streamhouse_schema_registry::SchemaFormat::Protobuf => {
            // Protobuf messages sent via the REST API are expected to be
            // base64-encoded binary or JSON representations. We validate
            // that the value is non-empty; full wire-format validation
            // requires the compiled descriptor which is handled at the
            // schema registry level during registration.
            if value.trim().is_empty() {
                Err((
                    StatusCode::BAD_REQUEST,
                    "Protobuf value must not be empty".to_string(),
                ))
            } else {
                Ok(())
            }
        }
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

    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| e.to_string())
        .collect();
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
            "Value does not conform to Avro schema".to_string(),
        ))
    }
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
    auth_key: Option<Extension<AuthenticatedKey>>,
    Json(req): Json<ProduceRequest>,
) -> Result<Json<ProduceResponse>, StatusCode> {
    let start = std::time::Instant::now();
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;
    let topic_name = req.topic.clone();
    let value_bytes = req.value.len() as u64;

    // Check produce byte-rate quota and storage quota
    if let Some(ref enforcer) = state.quota_enforcer {
        let tenant_ctx = crate::rate_limit::build_tenant_context(&org_id, enforcer).await;
        if let Some(ctx) = tenant_ctx {
            let check = enforcer.check_produce(&ctx, value_bytes as i64, None).await;
            if let Ok(streamhouse_metadata::QuotaCheck::Denied(reason)) = check {
                metrics::RATE_LIMIT_TOTAL
                    .with_label_values(&[&org_id, "denied", "rest"])
                    .inc();
                tracing::warn!(
                    "Produce rate limit denied: org={}, reason={}",
                    org_id,
                    reason
                );
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }

            let storage_check = enforcer.check_storage(&ctx).await;
            if let Ok(streamhouse_metadata::QuotaCheck::Denied(reason)) = storage_check {
                tracing::warn!("Storage limit denied: org={}, reason={}", org_id, reason);
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
        }
    }

    // Verify topic belongs to org and get its metadata
    let topic = match state.metadata.get_topic_for_org(&org_id, &req.topic).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            metrics::PRODUCER_ERRORS_TOTAL
                .with_label_values(&[&org_id, &topic_name, "topic_not_found"])
                .inc();
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!(
                "get_topic_for_org failed: org={}, topic={}, err={:?}",
                org_id,
                req.topic,
                e
            );
            metrics::PRODUCER_ERRORS_TOTAL
                .with_label_values(&[&org_id, &topic_name, "metadata_error"])
                .inc();
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Validate value against schema (if schema registry is available)
    if let Some(registry) = &state.schema_registry {
        if let Err((status, msg)) =
            validate_value_against_schema(registry, &req.topic, &req.value).await
        {
            tracing::warn!("Schema validation failed: topic={}, err={}", req.topic, msg);
            metrics::PRODUCER_ERRORS_TOTAL
                .with_label_values(&[&org_id, &topic_name, "schema_validation"])
                .inc();
            return Err(status);
        }
    }

    // Forward produce to partition-leader agent via AgentRouter
    if let Some(agent_router) = &state.agent_router {
        // Determine partition: explicit > key hash > round-robin
        let partition = select_partition(req.partition, req.key.as_deref(), topic.partition_count);

        // Validate partition exists
        if partition >= topic.partition_count {
            metrics::PRODUCER_ERRORS_TOTAL
                .with_label_values(&[&org_id, &topic_name, "invalid_partition"])
                .inc();
            return Err(StatusCode::BAD_REQUEST);
        }

        let key = req.key.as_deref().map(|k| k.as_bytes().to_vec());
        let value = req.value.into_bytes();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let resp = agent_router
            .produce_single(
                &org_id,
                &req.topic,
                partition,
                key,
                value,
                timestamp,
                req.ack_mode.unwrap_or(0),
            )
            .await
            .map_err(|e| {
                tracing::error!(
                    "AgentRouter produce failed: topic={}, partition={}, err={:?}",
                    topic_name,
                    partition,
                    e
                );
                metrics::PRODUCER_ERRORS_TOTAL
                    .with_label_values(&[&org_id, &topic_name, "agent_router_error"])
                    .inc();
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        metrics::PRODUCER_RECORDS_TOTAL
            .with_label_values(&[&org_id, &topic_name])
            .inc();
        metrics::PRODUCER_BYTES_TOTAL
            .with_label_values(&[&org_id, &topic_name])
            .inc_by(value_bytes);
        metrics::PRODUCER_BATCH_SIZE
            .with_label_values(&[&org_id, &topic_name])
            .observe(1.0);
        metrics::PRODUCER_LATENCY
            .with_label_values(&[&org_id, &topic_name])
            .observe(start.elapsed().as_secs_f64());

        return Ok(Json(ProduceResponse {
            offset: resp.base_offset,
            partition,
        }));
    }

    // Fallback: write directly via WriterPool (used in e2e tests without agents)
    if let Some(writer_pool) = &state.writer_pool {
        let partition = select_partition(req.partition, req.key.as_deref(), topic.partition_count);
        if partition >= topic.partition_count {
            return Err(StatusCode::BAD_REQUEST);
        }
        let key = req
            .key
            .as_deref()
            .map(|k| bytes::Bytes::from(k.as_bytes().to_vec()));
        let value = bytes::Bytes::from(req.value.into_bytes());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let writer = writer_pool
            .get_writer(&org_id, &req.topic, partition)
            .await
            .map_err(|e| {
                tracing::error!("WriterPool get_writer failed: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        let offset = writer
            .lock()
            .await
            .append(key, value, timestamp)
            .await
            .map_err(|e| {
                tracing::error!("WriterPool append failed: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        metrics::PRODUCER_RECORDS_TOTAL
            .with_label_values(&[&org_id, &topic_name])
            .inc();
        return Ok(Json(ProduceResponse { offset, partition }));
    }

    metrics::PRODUCER_ERRORS_TOTAL
        .with_label_values(&[&org_id, &topic_name, "no_router"])
        .inc();
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
    auth_key: Option<Extension<AuthenticatedKey>>,
    Json(req): Json<BatchProduceRequest>,
) -> Result<Json<BatchProduceResponse>, StatusCode> {
    let start = std::time::Instant::now();
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;
    let topic_name = req.topic.clone();
    let total_bytes: u64 = req.records.iter().map(|r| r.value.len() as u64).sum();
    let record_count = req.records.len();

    // Check produce byte-rate quota and storage quota
    if let Some(ref enforcer) = state.quota_enforcer {
        let tenant_ctx = crate::rate_limit::build_tenant_context(&org_id, enforcer).await;
        if let Some(ctx) = tenant_ctx {
            let check = enforcer.check_produce(&ctx, total_bytes as i64, None).await;
            if let Ok(streamhouse_metadata::QuotaCheck::Denied(reason)) = check {
                metrics::RATE_LIMIT_TOTAL
                    .with_label_values(&[&org_id, "denied", "rest"])
                    .inc();
                tracing::warn!(
                    "Produce batch rate limit denied: org={}, reason={}",
                    org_id,
                    reason
                );
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }

            let storage_check = enforcer.check_storage(&ctx).await;
            if let Ok(streamhouse_metadata::QuotaCheck::Denied(reason)) = storage_check {
                tracing::warn!("Storage limit denied: org={}, reason={}", org_id, reason);
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
        }
    }

    // Verify topic belongs to org and get its metadata
    let topic = state
        .metadata
        .get_topic_for_org(&org_id, &req.topic)
        .await
        .map_err(|_| {
            metrics::PRODUCER_ERRORS_TOTAL
                .with_label_values(&[&org_id, &topic_name, "metadata_error"])
                .inc();
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            metrics::PRODUCER_ERRORS_TOTAL
                .with_label_values(&[&org_id, &topic_name, "topic_not_found"])
                .inc();
            StatusCode::NOT_FOUND
        })?;

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
                metrics::PRODUCER_ERRORS_TOTAL
                    .with_label_values(&[&org_id, &topic_name, "schema_validation"])
                    .inc();
                return Err(status);
            }
        }
    }

    // Forward produce to partition-leader agents via AgentRouter
    if let Some(agent_router) = &state.agent_router {
        let mut results = Vec::with_capacity(req.records.len());

        // Group records by partition for efficient batching
        let mut by_partition: HashMap<u32, Vec<(usize, &BatchRecord)>> = HashMap::new();
        for (idx, record) in req.records.iter().enumerate() {
            let partition = select_partition(
                record.partition,
                record.key.as_deref(),
                topic.partition_count,
            );
            if partition >= topic.partition_count {
                metrics::PRODUCER_ERRORS_TOTAL
                    .with_label_values(&[&org_id, &topic_name, "invalid_partition"])
                    .inc();
                return Err(StatusCode::BAD_REQUEST);
            }
            by_partition
                .entry(partition)
                .or_default()
                .push((idx, record));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Forward each partition batch to its lease-holder agent
        for (partition, records) in by_partition {
            use streamhouse_proto::producer::produce_request::Record;

            let proto_records: Vec<Record> = records
                .iter()
                .map(|(_, record)| Record {
                    key: record.key.as_ref().map(|k| k.as_bytes().to_vec()),
                    value: record.value.as_bytes().to_vec(),
                    timestamp,
                    headers: HashMap::new(),
                })
                .collect();

            let batch_size = proto_records.len();

            let resp = agent_router
                .produce(
                    &org_id,
                    &req.topic,
                    partition,
                    proto_records,
                    req.ack_mode.unwrap_or(0),
                    None,
                    None,
                    None,
                    None,
                )
                .await
                .map_err(|e| {
                    tracing::error!(
                        "AgentRouter batch produce failed: topic={}, partition={}, err={:?}",
                        topic_name,
                        partition,
                        e
                    );
                    metrics::PRODUCER_ERRORS_TOTAL
                        .with_label_values(&[&org_id, &topic_name, "agent_router_error"])
                        .inc();
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            // Generate results based on base_offset
            for i in 0..batch_size {
                results.push(BatchRecordResult {
                    partition,
                    offset: resp.base_offset + i as u64,
                });
            }
        }

        metrics::PRODUCER_RECORDS_TOTAL
            .with_label_values(&[&org_id, &topic_name])
            .inc_by(record_count as u64);
        metrics::PRODUCER_BYTES_TOTAL
            .with_label_values(&[&org_id, &topic_name])
            .inc_by(total_bytes);
        metrics::PRODUCER_BATCH_SIZE
            .with_label_values(&[&org_id, &topic_name])
            .observe(record_count as f64);
        metrics::PRODUCER_LATENCY
            .with_label_values(&[&org_id, &topic_name])
            .observe(start.elapsed().as_secs_f64());

        return Ok(Json(BatchProduceResponse {
            count: results.len(),
            offsets: results,
        }));
    }

    // Fallback: write directly via WriterPool (used in e2e tests without agents)
    if let Some(writer_pool) = &state.writer_pool {
        let mut results = Vec::with_capacity(req.records.len());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for record in &req.records {
            let partition = select_partition(
                record.partition,
                record.key.as_deref(),
                topic.partition_count,
            );
            let key = record
                .key
                .as_ref()
                .map(|k| bytes::Bytes::from(k.as_bytes().to_vec()));
            let value = bytes::Bytes::from(record.value.as_bytes().to_vec());
            let writer = writer_pool
                .get_writer(&org_id, &req.topic, partition)
                .await
                .map_err(|e| {
                    tracing::error!("WriterPool get_writer failed: {:?}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            let offset = writer
                .lock()
                .await
                .append(key, value, timestamp)
                .await
                .map_err(|e| {
                    tracing::error!("WriterPool append failed: {:?}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            results.push(BatchRecordResult { partition, offset });
        }

        metrics::PRODUCER_RECORDS_TOTAL
            .with_label_values(&[&org_id, &topic_name])
            .inc_by(record_count as u64);
        return Ok(Json(BatchProduceResponse {
            count: results.len(),
            offsets: results,
        }));
    }

    metrics::PRODUCER_ERRORS_TOTAL
        .with_label_values(&[&org_id, &topic_name, "no_router"])
        .inc();
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}
