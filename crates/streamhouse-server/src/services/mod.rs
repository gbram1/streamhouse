use crate::pb::{stream_house_server::StreamHouse, *};
use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_metadata::{MetadataStore, ProducerState, TopicConfig};
use streamhouse_storage::{
    PartitionReader, PartitionWriter, SegmentCache, WriteConfig, WriterPool,
};
use tokio::sync::{Notify, RwLock};
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

/// StreamHouse gRPC service implementation
///
/// Unified service combining admin, producer (with ack modes, idempotent dedup,
/// transactions), and consumer operations.
pub struct StreamHouseService {
    metadata: Arc<dyn MetadataStore>,
    object_store: Arc<dyn object_store::ObjectStore>,
    cache: Arc<SegmentCache>,
    writer_pool: Arc<WriterPool>,
    #[allow(dead_code)]
    config: WriteConfig,
    agent_id: String,
    shutting_down: Arc<RwLock<bool>>,
    /// Notified when topics are created or deleted so the partition assigner
    /// can immediately discover them instead of waiting for the next poll.
    topic_changed: Arc<Notify>,
}

impl StreamHouseService {
    pub fn new(
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn object_store::ObjectStore>,
        cache: Arc<SegmentCache>,
        writer_pool: Arc<WriterPool>,
        config: WriteConfig,
        agent_id: String,
    ) -> Self {
        Self {
            metadata,
            object_store,
            cache,
            writer_pool,
            config,
            agent_id,
            shutting_down: Arc::new(RwLock::new(false)),
            topic_changed: Arc::new(Notify::new()),
        }
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

    /// Get partition writer from pool
    async fn get_writer(
        &self,
        topic: &str,
        partition_id: u32,
    ) -> Result<Arc<tokio::sync::Mutex<PartitionWriter>>, Status> {
        // Verify topic exists
        let topic_info = self
            .metadata
            .get_topic(topic)
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

        // Get writer from pool
        self.writer_pool
            .get_writer(topic, partition_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get writer: {}", e)))
    }

    /// Get a partition reader
    async fn get_reader(&self, topic: &str, partition_id: u32) -> Result<PartitionReader, Status> {
        // Verify topic exists
        let topic_info = self
            .metadata
            .get_topic(topic)
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
            topic.to_string(),
            partition_id,
            self.metadata.clone(),
            self.object_store.clone(),
            self.cache.clone(),
        );

        Ok(reader)
    }

    /// Check if a partition lease is valid and held by this agent.
    async fn validate_lease(&self, topic: &str, partition: u32) -> Result<(), Status> {
        match self.metadata.get_partition_lease(topic, partition).await {
            Ok(Some(lease)) => {
                if lease.leader_agent_id != self.agent_id {
                    debug!(
                        topic = %topic,
                        partition = partition,
                        leader = %lease.leader_agent_id,
                        agent = %self.agent_id,
                        "Lease held by different agent"
                    );
                    return Err(Status::not_found(format!(
                        "Agent doesn't hold lease for partition {}/{} (held by {})",
                        topic, partition, lease.leader_agent_id
                    )));
                }

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;

                if lease.lease_expires_at <= now_ms {
                    warn!(
                        topic = %topic,
                        partition = partition,
                        expires_at = lease.lease_expires_at,
                        now = now_ms,
                        "Lease expired"
                    );
                    return Err(Status::failed_precondition(format!(
                        "Lease expired for partition {}/{}",
                        topic, partition
                    )));
                }

                Ok(())
            }
            Ok(None) => {
                debug!(
                    topic = %topic,
                    partition = partition,
                    "No lease exists for partition"
                );
                Err(Status::not_found(format!(
                    "No lease exists for partition {}/{}",
                    topic, partition
                )))
            }
            Err(e) => {
                error!(
                    topic = %topic,
                    partition = partition,
                    error = %e,
                    "Failed to check lease"
                );
                Err(Status::internal(format!("Failed to check lease: {}", e)))
            }
        }
    }

    /// Validate idempotent producer sequence numbers for deduplication.
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

        Ok(Response::new(CreateTopicResponse {
            topic_id: req.name,
            partition_count: req.partition_count,
        }))
    }

    async fn get_topic(
        &self,
        request: Request<GetTopicRequest>,
    ) -> Result<Response<GetTopicResponse>, Status> {
        let req = request.into_inner();

        let topic = self
            .metadata
            .get_topic(&req.name)
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

    #[tracing::instrument(skip(self, _request))]
    async fn list_topics(
        &self,
        _request: Request<ListTopicsRequest>,
    ) -> Result<Response<ListTopicsResponse>, Status> {
        let topics = self
            .metadata
            .list_topics()
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
        let req = request.into_inner();

        let writer = self.get_writer(&req.topic, req.partition).await?;
        let mut writer_guard = writer.lock().await;

        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let key = if req.key.is_empty() {
            None
        } else {
            Some(bytes::Bytes::from(req.key))
        };

        let offset = writer_guard
            .append(key, bytes::Bytes::from(req.value), timestamp)
            .await
            .map_err(|e| Status::internal(format!("Write failed: {}", e)))?;

        Ok(Response::new(ProduceResponse { offset, timestamp }))
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

        // Validate partition lease
        self.validate_lease(&req.topic, req.partition).await?;

        // Validate idempotent producer (if producer_id is present)
        let should_write = if let Some(ref producer_id) = req.producer_id {
            let epoch = req.producer_epoch.unwrap_or(0);
            let base_seq = req.base_sequence.unwrap_or(0);

            self.validate_idempotent(
                producer_id,
                epoch,
                base_seq,
                &req.topic,
                req.partition,
                req.records.len() as u32,
            )
            .await?
        } else {
            true
        };

        // If duplicate detected, return success without writing
        if !should_write {
            debug!(
                topic = %req.topic,
                partition = req.partition,
                "Duplicate batch - returning success without write"
            );

            return Ok(Response::new(ProduceBatchResponse {
                first_offset: 0,
                last_offset: 0,
                count: req.records.len() as u32,
                duplicates_filtered: true,
            }));
        }

        // Get writer for partition
        let writer = self.get_writer(&req.topic, req.partition).await?;

        // Batch append — single WAL write for the entire produce request
        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let batch: Vec<(Option<bytes::Bytes>, bytes::Bytes, u64)> = req
            .records
            .iter()
            .map(|r| {
                let key = if r.key.is_empty() {
                    None
                } else {
                    Some(bytes::Bytes::from(r.key.clone()))
                };
                let ts = if r.timestamp > 0 { r.timestamp } else { timestamp };
                (key, bytes::Bytes::from(r.value.clone()), ts)
            })
            .collect();

        let (base_offset, record_count) = {
            let mut writer_guard = writer.lock().await;

            match writer_guard.append_batch(&batch).await {
                Ok(offsets) => {
                    let base = *offsets.first().ok_or_else(|| {
                        Status::internal("No offsets returned from batch append")
                    })?;
                    (base, offsets.len() as u64)
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("S3 operation rate limited") {
                        warn!(
                            topic = %req.topic,
                            partition = req.partition,
                            "S3 rate limited - rejecting produce request with backpressure"
                        );
                        return Err(Status::resource_exhausted(
                            "S3 rate limit exceeded - please slow down",
                        ));
                    } else if error_msg.contains("S3 circuit breaker open") {
                        error!(
                            topic = %req.topic,
                            partition = req.partition,
                            "S3 circuit breaker open - rejecting produce request"
                        );
                        return Err(Status::unavailable(
                            "S3 service is temporarily unavailable - circuit breaker open",
                        ));
                    }

                    error!(
                        topic = %req.topic,
                        partition = req.partition,
                        error = %e,
                        "Failed to append batch"
                    );
                    return Err(Status::internal(format!("Failed to append batch: {}", e)));
                }
            }
        };

        debug!(
            topic = %req.topic,
            partition = req.partition,
            base_offset = base_offset,
            record_count = record_count,
            "Successfully produced records"
        );

        // Handle ACK_DURABLE mode - wait for S3 persistence before acknowledging
        // Uses batched durable flush: writes are collected over a ~200ms window
        // and flushed to S3 in a single upload, then all waiters are notified.
        if req.ack_mode == AckMode::AckDurable as i32 {
            debug!(
                topic = %req.topic,
                partition = req.partition,
                "ACK_DURABLE mode - queuing for batched S3 flush"
            );

            if let Err(e) = self
                .writer_pool
                .request_durable_flush(&req.topic, req.partition)
                .await
            {
                error!(
                    topic = %req.topic,
                    partition = req.partition,
                    error = %e,
                    "Failed batched durable flush - S3 upload failed"
                );
                return Err(Status::internal(format!("Failed durable flush: {}", e)));
            }

            debug!(
                topic = %req.topic,
                partition = req.partition,
                "ACK_DURABLE batched flush completed - data persisted to S3"
            );
        }

        // Track transaction partition if this is a transactional write
        if let Some(ref transaction_id) = req.transaction_id {
            let last_offset = base_offset + record_count - 1;

            if let Err(e) = self
                .metadata
                .add_transaction_partition(transaction_id, &req.topic, req.partition, base_offset)
                .await
            {
                warn!(
                    transaction_id = %transaction_id,
                    topic = %req.topic,
                    partition = req.partition,
                    error = %e,
                    "Failed to add partition to transaction (may already exist)"
                );
            }

            if let Err(e) = self
                .metadata
                .update_transaction_partition_offset(
                    transaction_id,
                    &req.topic,
                    req.partition,
                    last_offset,
                )
                .await
            {
                error!(
                    transaction_id = %transaction_id,
                    topic = %req.topic,
                    partition = req.partition,
                    error = %e,
                    "Failed to update transaction partition offset"
                );
            }
        }

        let last_offset = base_offset + record_count - 1;

        Ok(Response::new(ProduceBatchResponse {
            first_offset: base_offset,
            last_offset,
            count: record_count as u32,
            duplicates_filtered: false,
        }))
    }

    // ========================================================================
    // Consumer Operations
    // ========================================================================

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().topic, partition = request.get_ref().partition, offset = request.get_ref().offset, max_records = request.get_ref().max_records))]
    async fn consume(
        &self,
        request: Request<ConsumeRequest>,
    ) -> Result<Response<ConsumeResponse>, Status> {
        let req = request.into_inner();

        let reader = self.get_reader(&req.topic, req.partition).await?;

        let result = reader
            .read(req.offset, req.max_records as usize)
            .await
            .map_err(|e| Status::internal(format!("Read failed: {}", e)))?;

        let records = result
            .records
            .into_iter()
            .map(|r| ConsumedRecord {
                offset: r.offset,
                timestamp: r.timestamp,
                key: r.key.map(|k| k.to_vec()).unwrap_or_default(),
                value: r.value.to_vec(),
                headers: HashMap::new(),
            })
            .collect::<Vec<_>>();

        let has_more =
            !records.is_empty() && records.last().unwrap().offset + 1 < result.high_watermark;

        Ok(Response::new(ConsumeResponse {
            records,
            high_watermark: result.high_watermark,
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

        match self
            .metadata
            .commit_transaction(&req.transaction_id)
            .await
        {
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

        match self
            .metadata
            .abort_transaction(&req.transaction_id)
            .await
        {
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

        let object_store = Arc::new(
            object_store::local::LocalFileSystem::new_with_prefix(&storage_dir).unwrap(),
        );

        let cache = Arc::new(SegmentCache::new(&cache_dir, 10 * 1024 * 1024).unwrap());

        let config = WriteConfig {
            segment_max_size: 1024,
            segment_max_age_ms: 60_000,
            s3_bucket: "test-bucket".to_string(),
            s3_region: "us-east-1".to_string(),
            ..Default::default()
        };

        let writer_pool = Arc::new(WriterPool::new(
            metadata.clone(),
            object_store.clone(),
            config.clone(),
        ));

        let service = StreamHouseService::new(
            metadata,
            object_store,
            cache,
            writer_pool,
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
