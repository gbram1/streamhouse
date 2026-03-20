use crate::pb::{stream_house_server::StreamHouse, *};
use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_client::AgentRouter;
use streamhouse_metadata::{MetadataStore, ProducerState, TopicConfig, DEFAULT_ORGANIZATION_ID};
use streamhouse_storage::{PartitionReader, SegmentCache, WriteConfig};
use tokio::sync::{Notify, RwLock};
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

/// Extract organization ID from gRPC request metadata.
///
/// Checks for `x-organization-id` metadata key, falling back to
/// `DEFAULT_ORGANIZATION_ID` when not present (backwards-compatible).
fn extract_org_id_from_request<T>(request: &Request<T>) -> String {
    request
        .metadata()
        .get("x-organization-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_ORGANIZATION_ID.to_string())
}

/// StreamHouse gRPC service implementation
///
/// Unified service combining admin, producer (with ack modes, idempotent dedup,
/// transactions), and consumer operations.
///
/// Produce requests are forwarded to the partition-leader agent via the
/// AgentRouter rather than writing directly to a local WriterPool.
pub struct StreamHouseService {
    metadata: Arc<dyn MetadataStore>,
    object_store: Arc<dyn object_store::ObjectStore>,
    cache: Arc<SegmentCache>,
    agent_router: Option<Arc<AgentRouter>>,
    #[allow(dead_code)]
    config: WriteConfig,
    #[allow(dead_code)]
    agent_id: String,
    shutting_down: Arc<RwLock<bool>>,
    /// Notified when topics are created or deleted so the partition assigner
    /// can immediately discover them instead of waiting for the next poll.
    topic_changed: Arc<Notify>,
    /// Schema registry for produce-time validation (None = validation disabled)
    schema_registry: Option<Arc<streamhouse_schema_registry::SchemaRegistry>>,
    /// Optional WriterPool fallback for tests (when no AgentRouter is available)
    writer_pool: Option<Arc<streamhouse_storage::WriterPool>>,
}

impl StreamHouseService {
    pub fn new(
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn object_store::ObjectStore>,
        cache: Arc<SegmentCache>,
        agent_router: Option<Arc<AgentRouter>>,
        config: WriteConfig,
        agent_id: String,
    ) -> Self {
        Self {
            metadata,
            object_store,
            cache,
            agent_router,
            config,
            agent_id,
            shutting_down: Arc::new(RwLock::new(false)),
            topic_changed: Arc::new(Notify::new()),
            schema_registry: None,
            writer_pool: None,
        }
    }

    /// Set a WriterPool fallback for testing (when no agents are available)
    pub fn set_writer_pool(&mut self, pool: Option<Arc<streamhouse_storage::WriterPool>>) {
        self.writer_pool = pool;
    }

    /// Set schema registry for produce-time validation
    pub fn set_schema_registry(
        &mut self,
        registry: Option<Arc<streamhouse_schema_registry::SchemaRegistry>>,
    ) {
        self.schema_registry = registry;
    }

    /// Validate a value against the registered schema for a topic.
    /// Returns Ok(()) if no schema registry, no schema registered, or validation passes.
    async fn validate_schema(&self, topic: &str, value: &[u8]) -> Result<(), Status> {
        let registry = match &self.schema_registry {
            Some(r) => r,
            None => return Ok(()),
        };

        let subject = format!("{}-value", topic);
        let schema = match registry.get_latest_schema(&subject).await {
            Ok(s) => s,
            Err(_) => return Ok(()), // No schema registered → skip
        };

        let value_str = std::str::from_utf8(value)
            .map_err(|_| Status::invalid_argument("Value is not valid UTF-8"))?;

        match schema.schema_type {
            streamhouse_schema_registry::SchemaFormat::Json => {
                let schema_value: serde_json::Value = serde_json::from_str(&schema.schema)
                    .map_err(|e| Status::internal(format!("Invalid schema definition: {}", e)))?;
                let instance: serde_json::Value = serde_json::from_str(value_str)
                    .map_err(|e| Status::invalid_argument(format!("Invalid JSON: {}", e)))?;
                let validator = jsonschema::validator_for(&schema_value)
                    .map_err(|e| Status::internal(format!("Schema compile error: {}", e)))?;
                let errors: Vec<String> = validator
                    .iter_errors(&instance)
                    .map(|e| e.to_string())
                    .collect();
                if !errors.is_empty() {
                    return Err(Status::invalid_argument(format!(
                        "Schema validation failed: {}",
                        errors.join("; ")
                    )));
                }
            }
            streamhouse_schema_registry::SchemaFormat::Avro => {
                let avro_schema = apache_avro::Schema::parse_str(&schema.schema)
                    .map_err(|e| Status::internal(format!("Invalid Avro schema: {}", e)))?;
                let json_value: serde_json::Value = serde_json::from_str(value_str)
                    .map_err(|e| Status::invalid_argument(format!("Invalid JSON: {}", e)))?;
                let avro_value = apache_avro::types::Value::from(json_value);
                if avro_value.resolve(&avro_schema).is_err() {
                    return Err(Status::invalid_argument(
                        "Value does not conform to Avro schema",
                    ));
                }
            }
            streamhouse_schema_registry::SchemaFormat::Protobuf => {} // Not implemented
        }

        Ok(())
    }

    /// Returns a handle the partition assigner can wait on.
    /// Fires whenever a topic is created or deleted.
    pub fn topic_change_notify(&self) -> Arc<Notify> {
        self.topic_changed.clone()
    }

    /// Signal that the service is shutting down.
    pub async fn shutdown(&self) {
        let mut shutting_down = self.shutting_down.write().await;
        *shutting_down = true;
    }

    /// Get a partition reader
    async fn get_reader(
        &self,
        org_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<PartitionReader, Status> {
        // Verify topic exists for this org
        let topic_info = self
            .metadata
            .get_topic_for_org(org_id, topic)
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Topic not found: {}", topic)))?;

        // Verify partition exists
        if partition_id >= topic_info.partition_count {
            return Err(Status::invalid_argument(format!(
                "Invalid partition: {} (topic has {} partitions)",
                partition_id, topic_info.partition_count
            )));
        }

        // Create reader
        let reader = PartitionReader::new(
            org_id.to_string(),
            topic.to_string(),
            partition_id,
            self.metadata.clone(),
            self.object_store.clone(),
            self.cache.clone(),
        );

        Ok(reader)
    }

    /// Validate idempotent producer sequence numbers for deduplication.
    /// NOTE: Currently unused — the agent handles idempotent validation.
    /// Kept for potential future use in server-side dedup.
    #[allow(dead_code)]
    async fn validate_idempotent(
        &self,
        producer_id: &str,
        epoch: u32,
        base_sequence: i64,
        topic: &str,
        partition: u32,
        record_count: u32,
    ) -> Result<bool, Status> {
        // Validate producer exists and epoch matches
        let producer = match self.metadata.get_producer(producer_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Err(Status::not_found(format!(
                    "Unknown producer: {}",
                    producer_id
                )));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get producer: {}", e)));
            }
        };

        if producer.epoch != epoch {
            return Err(Status::failed_precondition(format!(
                "Producer epoch mismatch: expected {}, got {}. Producer may have been fenced.",
                producer.epoch, epoch
            )));
        }

        if producer.state != ProducerState::Active {
            return Err(Status::failed_precondition(format!(
                "Producer {} is not active (state: {:?})",
                producer_id, producer.state
            )));
        }

        // Check and update sequence
        match self
            .metadata
            .check_and_update_sequence(producer_id, topic, partition, base_sequence, record_count)
            .await
        {
            Ok(true) => {
                debug!(
                    producer_id = %producer_id,
                    topic = %topic,
                    partition = partition,
                    base_sequence = base_sequence,
                    "Sequence validated, proceeding with write"
                );
                Ok(true)
            }
            Ok(false) => {
                info!(
                    producer_id = %producer_id,
                    topic = %topic,
                    partition = partition,
                    base_sequence = base_sequence,
                    "Duplicate batch detected, returning success without write"
                );
                Ok(false)
            }
            Err(e) => {
                error!(
                    producer_id = %producer_id,
                    topic = %topic,
                    partition = partition,
                    base_sequence = base_sequence,
                    error = %e,
                    "Sequence validation failed"
                );
                Err(Status::failed_precondition(format!(
                    "Sequence validation failed: {}",
                    e
                )))
            }
        }
    }
}

#[tonic::async_trait]
impl StreamHouse for StreamHouseService {
    // ========================================================================
    // Admin Operations
    // ========================================================================

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().name, partitions = request.get_ref().partition_count))]
    async fn create_topic(
        &self,
        request: Request<CreateTopicRequest>,
    ) -> Result<Response<CreateTopicResponse>, Status> {
        let req = request.into_inner();

        let config = TopicConfig {
            name: req.name.clone(),
            partition_count: req.partition_count,
            retention_ms: req.retention_ms.map(|v| v as i64),
            cleanup_policy: Default::default(),
            config: req.config,
        };

        self.metadata
            .create_topic(config)
            .await
            .map_err(|e| Status::internal(format!("Failed to create topic: {}", e)))?;

        info!(
            topic = %req.name,
            partitions = req.partition_count,
            "Topic created"
        );

        // Wake up partition assigner immediately so leases are acquired
        // without waiting for the 60-second poll interval.
        self.topic_changed.notify_waiters();

        // Pre-assign partition leases to available agents so produce works immediately
        let org_id = streamhouse_metadata::DEFAULT_ORGANIZATION_ID;
        if let Ok(agents) = self.metadata.list_agents(None, None).await {
            let now_ms = chrono::Utc::now().timestamp_millis();
            let active: Vec<_> = agents
                .iter()
                .filter(|a| (now_ms - a.last_heartbeat) < 60_000)
                .collect();
            if !active.is_empty() {
                for p in 0..req.partition_count {
                    let agent = &active[p as usize % active.len()];
                    let _ = self
                        .metadata
                        .acquire_partition_lease(org_id, &req.name, p, &agent.agent_id, 60_000)
                        .await;
                }
            }
        }

        Ok(Response::new(CreateTopicResponse {
            topic_id: req.name,
            partition_count: req.partition_count,
        }))
    }

    async fn get_topic(
        &self,
        request: Request<GetTopicRequest>,
    ) -> Result<Response<GetTopicResponse>, Status> {
        let org_id = extract_org_id_from_request(&request);
        let req = request.into_inner();

        let topic = self
            .metadata
            .get_topic_for_org(&org_id, &req.name)
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Topic not found: {}", req.name)))?;

        Ok(Response::new(GetTopicResponse {
            topic: Some(Topic {
                name: topic.name,
                partition_count: topic.partition_count,
                retention_ms: topic.retention_ms.map(|v| v as u64),
                config: topic.config,
                created_at: topic.created_at as u64,
            }),
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn list_topics(
        &self,
        request: Request<ListTopicsRequest>,
    ) -> Result<Response<ListTopicsResponse>, Status> {
        let org_id = extract_org_id_from_request(&request);
        let topics = self
            .metadata
            .list_topics_for_org(&org_id)
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?;

        let topic_list = topics
            .into_iter()
            .map(|t| Topic {
                name: t.name,
                partition_count: t.partition_count,
                retention_ms: t.retention_ms.map(|v| v as u64),
                config: t.config,
                created_at: t.created_at as u64,
            })
            .collect();

        Ok(Response::new(ListTopicsResponse { topics: topic_list }))
    }

    async fn delete_topic(
        &self,
        request: Request<DeleteTopicRequest>,
    ) -> Result<Response<DeleteTopicResponse>, Status> {
        let req = request.into_inner();

        self.metadata
            .delete_topic(&req.name)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete topic: {}", e)))?;

        info!(topic = %req.name, "Topic deleted");

        // Wake up partition assigner so it stops managing deleted topic.
        self.topic_changed.notify_waiters();

        Ok(Response::new(DeleteTopicResponse { success: true }))
    }

    // ========================================================================
    // Producer Operations
    // ========================================================================

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().topic, partition = request.get_ref().partition, value_len = request.get_ref().value.len()))]
    async fn produce(
        &self,
        request: Request<ProduceRequest>,
    ) -> Result<Response<ProduceResponse>, Status> {
        let org_id = extract_org_id_from_request(&request);
        let req = request.into_inner();

        // Validate topic exists and partition is valid
        let topic = self
            .metadata
            .get_topic_for_org(&org_id, &req.topic)
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Topic not found: {}", req.topic)))?;

        if req.partition >= topic.partition_count {
            return Err(Status::invalid_argument(format!(
                "Invalid partition {} for topic {} (has {} partitions)",
                req.partition, req.topic, topic.partition_count
            )));
        }

        // Validate value against schema
        self.validate_schema(&req.topic, &req.value).await?;

        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let key_bytes = if req.key.is_empty() {
            None
        } else {
            Some(req.key.to_vec())
        };

        if let Some(agent_router) = &self.agent_router {
            let resp = agent_router
                .produce_single(
                    &org_id,
                    &req.topic,
                    req.partition,
                    key_bytes,
                    req.value,
                    timestamp,
                    0,
                )
                .await
                .map_err(|e| Status::internal(format!("AgentRouter produce failed: {}", e)))?;

            return Ok(Response::new(ProduceResponse {
                offset: resp.base_offset,
                timestamp,
            }));
        }

        // Fallback: WriterPool (for tests without agents)
        if let Some(writer_pool) = &self.writer_pool {
            let key = key_bytes.map(bytes::Bytes::from);
            let value = bytes::Bytes::from(req.value);
            let writer = writer_pool
                .get_writer(&org_id, &req.topic, req.partition)
                .await
                .map_err(|e| Status::internal(format!("WriterPool error: {}", e)))?;
            let offset = writer
                .lock()
                .await
                .append(key, value, timestamp)
                .await
                .map_err(|e| Status::internal(format!("WriterPool append error: {}", e)))?;
            return Ok(Response::new(ProduceResponse { offset, timestamp }));
        }

        Err(Status::internal("No produce backend configured"))
    }

    #[tracing::instrument(skip(self, request), fields(
        topic = %request.get_ref().topic,
        partition = request.get_ref().partition,
        record_count = request.get_ref().records.len()
    ))]
    async fn produce_batch(
        &self,
        request: Request<ProduceBatchRequest>,
    ) -> Result<Response<ProduceBatchResponse>, Status> {
        let org_id = extract_org_id_from_request(&request);
        let req = request.into_inner();

        // Check if shutting down
        if *self.shutting_down.read().await {
            return Err(Status::unavailable("Server is shutting down"));
        }

        if req.topic.is_empty() {
            return Err(Status::invalid_argument("Topic name is required"));
        }
        if req.records.is_empty() {
            return Err(Status::invalid_argument("At least one record is required"));
        }

        // Validate all records against schema
        for record in &req.records {
            self.validate_schema(&req.topic, &record.value).await?;
        }

        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let record_count = req.records.len();

        if let Some(agent_router) = &self.agent_router {
            let proto_records: Vec<streamhouse_proto::producer::produce_request::Record> = req
                .records
                .iter()
                .map(|r| {
                    let key = if r.key.is_empty() {
                        None
                    } else {
                        Some(r.key.clone())
                    };
                    let ts = if r.timestamp > 0 {
                        r.timestamp
                    } else {
                        timestamp
                    };
                    streamhouse_proto::producer::produce_request::Record {
                        key,
                        value: r.value.clone(),
                        timestamp: ts,
                        headers: std::collections::HashMap::new(),
                    }
                })
                .collect();

            let resp = agent_router
                .produce(
                    &org_id,
                    &req.topic,
                    req.partition,
                    proto_records,
                    req.ack_mode,
                    req.producer_id,
                    req.producer_epoch,
                    req.base_sequence,
                    req.transaction_id,
                )
                .await
                .map_err(|e| Status::internal(format!("AgentRouter produce failed: {}", e)))?;

            let base_offset = resp.base_offset;
            let last_offset = base_offset + record_count as u64 - 1;

            debug!(
                topic = %req.topic,
                partition = req.partition,
                base_offset = base_offset,
                record_count = record_count,
                "Successfully forwarded produce to agent"
            );

            return Ok(Response::new(ProduceBatchResponse {
                first_offset: base_offset,
                last_offset,
                count: record_count as u32,
                duplicates_filtered: resp.duplicates_filtered,
            }));
        }

        // Fallback: WriterPool (for tests without agents)
        if let Some(writer_pool) = &self.writer_pool {
            let mut first_offset = 0u64;
            for (i, record) in req.records.iter().enumerate() {
                let key = if record.key.is_empty() {
                    None
                } else {
                    Some(bytes::Bytes::from(record.key.clone()))
                };
                let value = bytes::Bytes::from(record.value.clone());
                let ts = if record.timestamp > 0 {
                    record.timestamp
                } else {
                    timestamp
                };
                let writer = writer_pool
                    .get_writer(&org_id, &req.topic, req.partition)
                    .await
                    .map_err(|e| Status::internal(format!("WriterPool error: {}", e)))?;
                let offset = writer
                    .lock()
                    .await
                    .append(key, value, ts)
                    .await
                    .map_err(|e| Status::internal(format!("WriterPool append error: {}", e)))?;
                if i == 0 {
                    first_offset = offset;
                }
            }
            let last_offset = first_offset + record_count as u64 - 1;
            return Ok(Response::new(ProduceBatchResponse {
                first_offset,
                last_offset,
                count: record_count as u32,
                duplicates_filtered: false,
            }));
        }

        Err(Status::internal("No produce backend configured"))
    }

    // ========================================================================
    // Consumer Operations
    // ========================================================================

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().topic, partition = request.get_ref().partition, offset = request.get_ref().offset, max_records = request.get_ref().max_records))]
    async fn consume(
        &self,
        request: Request<ConsumeRequest>,
    ) -> Result<Response<ConsumeResponse>, Status> {
        let org_id = extract_org_id_from_request(&request);
        let req = request.into_inner();
        let max_records = req.max_records as usize;

        let reader = self.get_reader(&org_id, &req.topic, req.partition).await?;

        // Read from S3 segments (agents handle in-memory buffering and flushing)
        let result = match reader.read(req.offset, max_records).await {
            Ok(r) => r,
            Err(streamhouse_storage::Error::OffsetNotFound(_)) => {
                // Offset not yet flushed to S3 — return empty result with
                // the current high watermark so the consumer can poll again.
                let partition = self
                    .metadata
                    .get_partition(&org_id, &req.topic, req.partition)
                    .await
                    .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?
                    .ok_or_else(|| Status::not_found("Partition not found"))?;

                streamhouse_storage::ReadResult {
                    records: vec![],
                    high_watermark: partition.high_watermark,
                }
            }
            Err(e) => return Err(Status::internal(format!("Read failed: {}", e))),
        };

        let high_watermark = result.high_watermark;
        let records: Vec<ConsumedRecord> = result
            .records
            .into_iter()
            .map(|r| ConsumedRecord {
                offset: r.offset,
                timestamp: r.timestamp,
                key: r.key.map(|k| k.to_vec()).unwrap_or_default(),
                value: r.value.to_vec(),
                headers: HashMap::new(),
            })
            .collect();

        let has_more = !records.is_empty() && records.last().unwrap().offset + 1 < high_watermark;

        Ok(Response::new(ConsumeResponse {
            records,
            high_watermark,
            has_more,
        }))
    }

    async fn commit_offset(
        &self,
        request: Request<CommitOffsetRequest>,
    ) -> Result<Response<CommitOffsetResponse>, Status> {
        let req = request.into_inner();

        self.metadata
            .commit_offset(
                &req.consumer_group,
                &req.topic,
                req.partition,
                req.offset,
                req.metadata,
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to commit offset: {}", e)))?;

        Ok(Response::new(CommitOffsetResponse { success: true }))
    }

    async fn get_offset(
        &self,
        request: Request<GetOffsetRequest>,
    ) -> Result<Response<GetOffsetResponse>, Status> {
        let req = request.into_inner();

        let offset = self
            .metadata
            .get_committed_offset(&req.consumer_group, &req.topic, req.partition)
            .await
            .map_err(|e| Status::internal(format!("Failed to get offset: {}", e)))?
            .unwrap_or(0);

        Ok(Response::new(GetOffsetResponse {
            offset,
            metadata: None,
        }))
    }

    // ========================================================================
    // Producer Lifecycle Operations
    // ========================================================================

    async fn init_producer(
        &self,
        request: Request<InitProducerRequest>,
    ) -> Result<Response<InitProducerResponse>, Status> {
        let req = request.into_inner();

        let config = streamhouse_metadata::InitProducerConfig {
            transactional_id: req.transactional_id,
            organization_id: req.organization_id,
            timeout_ms: req.timeout_ms,
            metadata: if req.metadata.is_empty() {
                None
            } else {
                Some(req.metadata.into_iter().collect())
            },
        };

        match self.metadata.init_producer(config).await {
            Ok(producer) => {
                info!(
                    producer_id = %producer.id,
                    epoch = producer.epoch,
                    "Producer initialized"
                );
                Ok(Response::new(InitProducerResponse {
                    producer_id: producer.id,
                    epoch: producer.epoch,
                }))
            }
            Err(e) => {
                error!(error = %e, "Failed to initialize producer");
                Err(Status::internal(format!(
                    "Failed to initialize producer: {}",
                    e
                )))
            }
        }
    }

    async fn begin_transaction(
        &self,
        request: Request<BeginTransactionRequest>,
    ) -> Result<Response<BeginTransactionResponse>, Status> {
        let req = request.into_inner();

        let producer = match self.metadata.get_producer(&req.producer_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Err(Status::not_found(format!(
                    "Unknown producer: {}",
                    req.producer_id
                )));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get producer: {}", e)));
            }
        };

        if producer.epoch != req.producer_epoch {
            return Err(Status::failed_precondition(format!(
                "Producer epoch mismatch: expected {}, got {}",
                producer.epoch, req.producer_epoch
            )));
        }

        if producer.state != ProducerState::Active {
            return Err(Status::failed_precondition(format!(
                "Producer {} is not active",
                req.producer_id
            )));
        }

        match self
            .metadata
            .begin_transaction(&req.producer_id, req.timeout_ms)
            .await
        {
            Ok(txn) => {
                info!(
                    producer_id = %req.producer_id,
                    transaction_id = %txn.transaction_id,
                    "Transaction started"
                );
                Ok(Response::new(BeginTransactionResponse {
                    transaction_id: txn.transaction_id,
                }))
            }
            Err(e) => {
                error!(
                    producer_id = %req.producer_id,
                    error = %e,
                    "Failed to begin transaction"
                );
                Err(Status::internal(format!(
                    "Failed to begin transaction: {}",
                    e
                )))
            }
        }
    }

    async fn commit_transaction(
        &self,
        request: Request<CommitTransactionRequest>,
    ) -> Result<Response<CommitTransactionResponse>, Status> {
        let req = request.into_inner();

        let producer = match self.metadata.get_producer(&req.producer_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Err(Status::not_found(format!(
                    "Unknown producer: {}",
                    req.producer_id
                )));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get producer: {}", e)));
            }
        };

        if producer.epoch != req.producer_epoch {
            return Err(Status::failed_precondition(format!(
                "Producer epoch mismatch: expected {}, got {}",
                producer.epoch, req.producer_epoch
            )));
        }

        match self.metadata.commit_transaction(&req.transaction_id).await {
            Ok(commit_timestamp) => {
                info!(
                    producer_id = %req.producer_id,
                    transaction_id = %req.transaction_id,
                    "Transaction committed"
                );
                Ok(Response::new(CommitTransactionResponse {
                    success: true,
                    commit_timestamp: commit_timestamp as u64,
                }))
            }
            Err(e) => {
                error!(
                    producer_id = %req.producer_id,
                    transaction_id = %req.transaction_id,
                    error = %e,
                    "Failed to commit transaction"
                );
                Err(Status::internal(format!(
                    "Failed to commit transaction: {}",
                    e
                )))
            }
        }
    }

    async fn abort_transaction(
        &self,
        request: Request<AbortTransactionRequest>,
    ) -> Result<Response<AbortTransactionResponse>, Status> {
        let req = request.into_inner();

        let producer = match self.metadata.get_producer(&req.producer_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Err(Status::not_found(format!(
                    "Unknown producer: {}",
                    req.producer_id
                )));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get producer: {}", e)));
            }
        };

        if producer.epoch != req.producer_epoch {
            return Err(Status::failed_precondition(format!(
                "Producer epoch mismatch: expected {}, got {}",
                producer.epoch, req.producer_epoch
            )));
        }

        match self.metadata.abort_transaction(&req.transaction_id).await {
            Ok(()) => {
                info!(
                    producer_id = %req.producer_id,
                    transaction_id = %req.transaction_id,
                    "Transaction aborted"
                );
                Ok(Response::new(AbortTransactionResponse { success: true }))
            }
            Err(e) => {
                error!(
                    producer_id = %req.producer_id,
                    transaction_id = %req.transaction_id,
                    error = %e,
                    "Failed to abort transaction"
                );
                Err(Status::internal(format!(
                    "Failed to abort transaction: {}",
                    e
                )))
            }
        }
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        let producer = match self.metadata.get_producer(&req.producer_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Ok(Response::new(HeartbeatResponse {
                    valid: false,
                    error: Some(format!("Unknown producer: {}", req.producer_id)),
                }));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get producer: {}", e)));
            }
        };

        if producer.epoch != req.producer_epoch {
            return Ok(Response::new(HeartbeatResponse {
                valid: false,
                error: Some(format!(
                    "Producer epoch mismatch: expected {}, got {}",
                    producer.epoch, req.producer_epoch
                )),
            }));
        }

        if producer.state != ProducerState::Active {
            return Ok(Response::new(HeartbeatResponse {
                valid: false,
                error: Some(format!(
                    "Producer {} is {:?}",
                    req.producer_id, producer.state
                )),
            }));
        }

        if let Err(e) = self
            .metadata
            .update_producer_heartbeat(&req.producer_id)
            .await
        {
            warn!(
                producer_id = %req.producer_id,
                error = %e,
                "Failed to update heartbeat"
            );
        }

        Ok(Response::new(HeartbeatResponse {
            valid: true,
            error: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamhouse_metadata::SqliteMetadataStore;
    use streamhouse_storage::WriteConfig;

    /// Helper: create a fully wired StreamHouseService backed by an in-memory
    /// SQLite metadata store and a local-filesystem object store.
    async fn make_test_service() -> (StreamHouseService, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path();

        let metadata_path = data_dir.join("metadata.db");
        let storage_dir = data_dir.join("storage");
        let cache_dir = data_dir.join("cache");

        std::fs::create_dir_all(&storage_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let metadata = Arc::new(
            SqliteMetadataStore::new(metadata_path.to_str().unwrap())
                .await
                .unwrap(),
        );

        let object_store =
            Arc::new(object_store::local::LocalFileSystem::new_with_prefix(&storage_dir).unwrap());

        let cache = Arc::new(SegmentCache::new(&cache_dir, 10 * 1024 * 1024).unwrap());

        let config = WriteConfig {
            segment_max_size: 1024,
            segment_max_age_ms: 60_000,
            s3_bucket: "test-bucket".to_string(),
            s3_region: "us-east-1".to_string(),
            ..Default::default()
        };

        let service = StreamHouseService::new(
            metadata,
            object_store,
            cache,
            None, // No AgentRouter in tests (admin-only tests)
            config,
            "test-agent".to_string(),
        );
        (service, temp_dir)
    }

    #[tokio::test]
    async fn test_create_topic_returns_correct_response() {
        let (service, _temp) = make_test_service().await;

        let req = Request::new(CreateTopicRequest {
            name: "my-topic".to_string(),
            partition_count: 4,
            retention_ms: Some(3_600_000),
            config: HashMap::new(),
        });

        let resp = service.create_topic(req).await.unwrap().into_inner();
        assert_eq!(resp.topic_id, "my-topic");
        assert_eq!(resp.partition_count, 4);
    }

    #[tokio::test]
    async fn test_get_topic_not_found_returns_status_not_found() {
        let (service, _temp) = make_test_service().await;

        let req = Request::new(GetTopicRequest {
            name: "nonexistent".to_string(),
        });

        let err = service.get_topic(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
        assert!(err.message().contains("Topic not found"));
    }

    #[tokio::test]
    async fn test_delete_topic_success() {
        let (service, _temp) = make_test_service().await;

        // Create a topic first
        let create_req = Request::new(CreateTopicRequest {
            name: "to-delete".to_string(),
            partition_count: 1,
            retention_ms: None,
            config: HashMap::new(),
        });
        service.create_topic(create_req).await.unwrap();

        // Delete it
        let delete_req = Request::new(DeleteTopicRequest {
            name: "to-delete".to_string(),
        });
        let resp = service.delete_topic(delete_req).await.unwrap().into_inner();
        assert!(resp.success);

        // Verify it is gone
        let get_req = Request::new(GetTopicRequest {
            name: "to-delete".to_string(),
        });
        let err = service.get_topic(get_req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_produce_to_invalid_partition_returns_error() {
        let (service, _temp) = make_test_service().await;

        // Create topic with 2 partitions
        let create_req = Request::new(CreateTopicRequest {
            name: "partitioned".to_string(),
            partition_count: 2,
            retention_ms: None,
            config: HashMap::new(),
        });
        service.create_topic(create_req).await.unwrap();

        // Try partition 10 (out of range)
        let produce_req = Request::new(ProduceRequest {
            topic: "partitioned".to_string(),
            partition: 10,
            key: vec![],
            value: b"hello".to_vec(),
            headers: HashMap::new(),
        });
        let err = service.produce(produce_req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("Invalid partition"));
    }
}
