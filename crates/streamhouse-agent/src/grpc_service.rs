//! gRPC Producer Service Implementation
//!
//! This module implements the ProducerService gRPC server for StreamHouse agents.
//! The service accepts ProduceRequests from Producer clients, validates partition
//! leases, and appends records to the appropriate partition writers.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐
//! │   Producer   │ (client)
//! └──────┬───────┘
//!        │ gRPC ProduceRequest
//!        ▼
//! ┌──────────────┐
//! │ ProducerService│ (this module)
//! └──────┬───────┘
//!        │
//!        ├─→ Validate partition lease
//!        ├─→ Get writer from pool
//!        ├─→ Append records
//!        └─→ Return ProduceResponse
//! ```
//!
//! ## Error Handling
//!
//! The service returns gRPC Status codes for different failure modes:
//! - `NOT_FOUND`: Agent doesn't hold lease for partition
//! - `FAILED_PRECONDITION`: Lease expired
//! - `UNAVAILABLE`: Agent shutting down
//! - `INTERNAL`: Storage write failed or other unexpected error
//!
//! ## Performance
//!
//! This service is designed for high throughput:
//! - Batching: Accepts multiple records per request (amortizes RPC overhead)
//! - Writer pooling: Reuses writers across requests (no per-request initialization)
//! - Async: Non-blocking I/O for concurrent request handling
//!
//! Target: 50K+ records/sec per agent with p99 latency < 10ms

use std::sync::Arc;
use streamhouse_metadata::{LeaderChangeReason, LeaseTransfer, MetadataStore, ProducerState};
use streamhouse_proto::producer::{
    producer_service_server::ProducerService, AbortTransactionRequest, AbortTransactionResponse,
    BeginTransactionRequest, BeginTransactionResponse, CommitTransactionRequest,
    CommitTransactionResponse, HeartbeatRequest, HeartbeatResponse, InitProducerRequest,
    InitProducerResponse, ProduceRequest, ProduceResponse,
};
use streamhouse_proto::streamhouse::{
    agent_coordination_server::AgentCoordination, AcceptLeaseRequest, AcceptLeaseResponse,
    CompleteTransferRequest, CompleteTransferResponse, HealthCheckRequest, HealthCheckResponse,
    TransferLeaseRequest, TransferLeaseResponse, TransferReason,
};
use streamhouse_storage::writer_pool::WriterPool;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

use crate::LeaseManager;

/// Prometheus metrics for the Agent.
///
/// Tracks agent-specific metrics like active partitions, lease renewals,
/// write throughput, latency, and gRPC request statistics.
/// All metrics use atomic operations for lock-free updates.
///
/// ## Metrics
///
/// - `active_partitions`: Current number of active partitions (gauge)
/// - `lease_renewals_total`: Total lease renewal attempts (counter)
/// - `records_written_total`: Total records written (counter)
/// - `write_latency_seconds`: Write operation latency (histogram)
/// - `active_connections`: Active gRPC connections (gauge)
/// - `grpc_requests_total`: Total gRPC requests by method and status (counter)
/// - `grpc_request_duration_seconds`: gRPC request duration (histogram)
#[cfg(feature = "metrics")]
pub struct AgentMetrics {
    active_partitions: prometheus_client::metrics::gauge::Gauge<i64>,
    lease_renewals_total: prometheus_client::metrics::counter::Counter<u64>,
    records_written_total: prometheus_client::metrics::counter::Counter<u64>,
    write_latency_seconds: prometheus_client::metrics::histogram::Histogram,
    active_connections: prometheus_client::metrics::gauge::Gauge<i64>,
    grpc_requests_total: prometheus_client::metrics::counter::Counter<u64>,
    grpc_request_duration_seconds: prometheus_client::metrics::histogram::Histogram,
}

#[cfg(feature = "metrics")]
impl AgentMetrics {
    /// Create new AgentMetrics and register with the given registry.
    pub fn new(registry: &mut prometheus_client::registry::Registry) -> Self {
        let active_partitions = prometheus_client::metrics::gauge::Gauge::<i64>::default();
        let lease_renewals = prometheus_client::metrics::counter::Counter::<u64>::default();
        let records_written = prometheus_client::metrics::counter::Counter::<u64>::default();

        let write_latency = prometheus_client::metrics::histogram::Histogram::new(
            prometheus_client::metrics::histogram::exponential_buckets(0.001, 2.0, 15),
        );

        let active_connections = prometheus_client::metrics::gauge::Gauge::<i64>::default();
        let grpc_requests = prometheus_client::metrics::counter::Counter::<u64>::default();

        let grpc_duration = prometheus_client::metrics::histogram::Histogram::new(
            prometheus_client::metrics::histogram::exponential_buckets(0.001, 2.0, 15),
        );

        registry.register(
            "streamhouse_agent_active_partitions",
            "Number of active partitions",
            active_partitions.clone(),
        );

        registry.register(
            "streamhouse_agent_lease_renewals_total",
            "Total lease renewal attempts",
            lease_renewals.clone(),
        );

        registry.register(
            "streamhouse_agent_records_written_total",
            "Total records written",
            records_written.clone(),
        );

        registry.register(
            "streamhouse_agent_write_latency_seconds",
            "Write operation latency in seconds",
            write_latency.clone(),
        );

        registry.register(
            "streamhouse_agent_active_connections",
            "Active gRPC connections",
            active_connections.clone(),
        );

        registry.register(
            "streamhouse_agent_grpc_requests_total",
            "Total gRPC requests",
            grpc_requests.clone(),
        );

        registry.register(
            "streamhouse_agent_grpc_request_duration_seconds",
            "gRPC request duration in seconds",
            grpc_duration.clone(),
        );

        Self {
            active_partitions,
            lease_renewals_total: lease_renewals,
            records_written_total: records_written,
            write_latency_seconds: write_latency,
            active_connections,
            grpc_requests_total: grpc_requests,
            grpc_request_duration_seconds: grpc_duration,
        }
    }

    /// Record a write operation.
    pub fn record_write(&self, record_count: u64, duration_secs: f64) {
        self.records_written_total.inc_by(record_count);
        self.write_latency_seconds.observe(duration_secs);
    }

    /// Record a gRPC request.
    pub fn record_grpc_request(&self, duration_secs: f64) {
        self.grpc_requests_total.inc();
        self.grpc_request_duration_seconds.observe(duration_secs);
    }

    /// Update active partition count.
    pub fn set_active_partitions(&self, count: i64) {
        self.active_partitions.set(count);
    }

    /// Record a lease renewal attempt.
    pub fn record_lease_renewal(&self) {
        self.lease_renewals_total.inc();
    }

    /// Update active connection count.
    pub fn set_active_connections(&self, count: i64) {
        self.active_connections.set(count);
    }
}

/// Producer service implementation for StreamHouse agents.
///
/// This service handles ProduceRequests from Producer clients. It validates
/// partition leases, appends records to partition writers, and returns assigned offsets.
///
/// # Thread Safety
///
/// ProducerServiceImpl is Send + Sync and can safely handle concurrent requests.
/// Internal state (writer_pool, lease_manager) is protected by Arc and RwLock.
///
/// # Lifecycle
///
/// 1. **Construction**: Created when agent starts, holds references to writer pool and lease manager
/// 2. **Serving**: Handles produce requests until agent stops
/// 3. **Shutdown**: Gracefully stops when agent stops (releases leases, flushes writers)
///
/// # Examples
///
/// ```ignore
/// use streamhouse_agent::grpc_service::ProducerServiceImpl;
/// use streamhouse_proto::producer::producer_service_server::ProducerServiceServer;
///
/// let service = ProducerServiceImpl::new(writer_pool, lease_manager);
/// let server = ProducerServiceServer::new(service);
///
/// // Start gRPC server
/// Server::builder()
///     .add_service(server)
///     .serve(addr)
///     .await?;
/// ```
pub struct ProducerServiceImpl {
    /// Writer pool for managing partition writers
    writer_pool: Arc<WriterPool>,

    /// Metadata store for querying partition leases
    metadata_store: Arc<dyn MetadataStore>,

    /// Agent ID for validating lease ownership
    agent_id: String,

    /// Agent state for checking if we're shutting down
    shutting_down: Arc<RwLock<bool>>,

    /// Optional Prometheus metrics for observability
    #[cfg(feature = "metrics")]
    metrics: Option<Arc<AgentMetrics>>,
}

impl ProducerServiceImpl {
    /// Create a new ProducerService implementation.
    ///
    /// # Arguments
    ///
    /// * `writer_pool` - Pool of partition writers for appending records
    /// * `metadata_store` - Metadata store for querying partition leases
    /// * `agent_id` - This agent's ID for validating lease ownership
    ///
    /// # Returns
    ///
    /// A new `ProducerServiceImpl` ready to handle requests.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let service = ProducerServiceImpl::new(
    ///     Arc::new(writer_pool),
    ///     Arc::new(metadata_store),
    ///     "agent-001".to_string(),
    /// );
    /// ```
    pub fn new(
        writer_pool: Arc<WriterPool>,
        metadata_store: Arc<dyn MetadataStore>,
        agent_id: String,
        #[cfg(feature = "metrics")] metrics: Option<Arc<AgentMetrics>>,
    ) -> Self {
        Self {
            writer_pool,
            metadata_store,
            agent_id,
            shutting_down: Arc::new(RwLock::new(false)),
            #[cfg(feature = "metrics")]
            metrics,
        }
    }

    /// Signal that the service is shutting down.
    ///
    /// After calling this, new produce requests will be rejected with UNAVAILABLE.
    pub async fn shutdown(&self) {
        let mut shutting_down = self.shutting_down.write().await;
        *shutting_down = true;
    }

    /// Validate idempotent producer request (producer_id, epoch, sequence).
    ///
    /// For idempotent producers, this method:
    /// 1. Validates the producer exists and is active
    /// 2. Validates the epoch matches (rejects zombies)
    /// 3. Checks sequence numbers for duplicates
    ///
    /// # Arguments
    ///
    /// * `producer_id` - Producer ID from the request
    /// * `producer_epoch` - Producer epoch from the request
    /// * `base_sequence` - Base sequence number from the request
    /// * `topic` - Topic being written to
    /// * `partition` - Partition being written to
    /// * `record_count` - Number of records in the batch
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if records should be written (not a duplicate)
    /// - `Ok(false)` if records are duplicates (should be acked but not written)
    /// - `Err(Status)` if validation fails (fenced, sequence error, etc.)
    async fn validate_idempotent(
        &self,
        producer_id: &str,
        producer_epoch: u32,
        base_sequence: i64,
        topic: &str,
        partition: u32,
        record_count: u32,
    ) -> std::result::Result<bool, Status> {
        // 1. Get producer and validate it exists and is active
        let producer = match self.metadata_store.get_producer(producer_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                warn!(
                    producer_id = %producer_id,
                    "Unknown producer ID"
                );
                return Err(Status::failed_precondition(format!(
                    "Unknown producer ID: {}",
                    producer_id
                )));
            }
            Err(e) => {
                error!(
                    producer_id = %producer_id,
                    error = %e,
                    "Failed to get producer"
                );
                return Err(Status::internal(format!("Failed to get producer: {}", e)));
            }
        };

        // 2. Check producer state
        if producer.state == ProducerState::Fenced {
            warn!(
                producer_id = %producer_id,
                "Producer has been fenced"
            );
            return Err(Status::failed_precondition(format!(
                "Producer {} has been fenced",
                producer_id
            )));
        }

        if producer.state == ProducerState::Expired {
            warn!(
                producer_id = %producer_id,
                "Producer has expired"
            );
            return Err(Status::failed_precondition(format!(
                "Producer {} has expired",
                producer_id
            )));
        }

        // 3. Validate epoch (reject zombie producers)
        if producer.epoch != producer_epoch {
            warn!(
                producer_id = %producer_id,
                request_epoch = producer_epoch,
                current_epoch = producer.epoch,
                "Epoch mismatch - producer may be a zombie"
            );
            return Err(Status::failed_precondition(format!(
                "Producer epoch mismatch: expected {}, got {}",
                producer.epoch, producer_epoch
            )));
        }

        // 4. Check and update sequence atomically
        match self
            .metadata_store
            .check_and_update_sequence(producer_id, topic, partition, base_sequence, record_count)
            .await
        {
            Ok(true) => {
                // Valid sequence - proceed with write
                debug!(
                    producer_id = %producer_id,
                    topic = %topic,
                    partition = partition,
                    base_sequence = base_sequence,
                    record_count = record_count,
                    "Sequence validated, proceeding with write"
                );
                Ok(true)
            }
            Ok(false) => {
                // Duplicate detected - ack but don't write
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
                // Sequence gap or other error
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

    /// Check if a partition lease is valid and held by this agent.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    ///
    /// # Returns
    ///
    /// - `Ok(())` if lease is valid and held by this agent
    /// - `Err(Status)` with appropriate error code if lease is invalid
    async fn validate_lease(&self, topic: &str, partition: u32) -> std::result::Result<(), Status> {
        match self
            .metadata_store
            .get_partition_lease(topic, partition)
            .await
        {
            Ok(Some(lease)) => {
                // Check if this agent holds the lease
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

                // Check if lease is expired
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
}

#[tonic::async_trait]
impl ProducerService for ProducerServiceImpl {
    /// Handle a produce request from a client.
    ///
    /// This method:
    /// 1. Checks if the agent is shutting down
    /// 2. Validates the partition lease
    /// 3. Gets the writer for the partition
    /// 4. Appends all records in the batch
    /// 5. Returns the base offset and record count
    ///
    /// # Arguments
    ///
    /// * `request` - ProduceRequest containing topic, partition, and records
    ///
    /// # Returns
    ///
    /// - `Ok(Response<ProduceResponse>)` with base offset and record count on success
    /// - `Err(Status)` with appropriate error code on failure
    ///
    /// # Error Codes
    ///
    /// - `UNAVAILABLE`: Agent is shutting down
    /// - `INVALID_ARGUMENT`: Empty batch or missing fields
    /// - `NOT_FOUND`: Agent doesn't hold lease for partition
    /// - `FAILED_PRECONDITION`: Lease expired
    /// - `INTERNAL`: Storage write failed or unexpected error
    ///
    /// # Performance
    ///
    /// This method is optimized for high throughput:
    /// - Batching amortizes RPC overhead (100 records per request is typical)
    /// - Writer pooling eliminates per-request initialization
    /// - Lock contention minimized (only one lock per batch, not per record)
    ///
    /// Target: ~1ms per request (100 records), 50K+ rec/s per agent
    #[tracing::instrument(skip(self, request), fields(
        topic = %request.get_ref().topic,
        partition = request.get_ref().partition,
        record_count = request.get_ref().records.len()
    ))]
    async fn produce(
        &self,
        request: Request<ProduceRequest>,
    ) -> std::result::Result<Response<ProduceResponse>, Status> {
        #[cfg(feature = "metrics")]
        let start = std::time::Instant::now();

        // Check if shutting down
        if *self.shutting_down.read().await {
            return Err(Status::unavailable("Agent is shutting down"));
        }

        let req = request.into_inner();

        // Validate request
        if req.topic.is_empty() {
            return Err(Status::invalid_argument("Topic name is required"));
        }
        if req.records.is_empty() {
            return Err(Status::invalid_argument("At least one record is required"));
        }

        debug!(
            topic = %req.topic,
            partition = req.partition,
            record_count = req.records.len(),
            "Received produce request"
        );

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
            true // Non-idempotent producers always write
        };

        // If duplicate detected, return success without writing
        if !should_write {
            // For duplicates, we need to return the same base offset that was assigned originally
            // For now, return 0 as the base offset - the producer should use the cached result
            debug!(
                topic = %req.topic,
                partition = req.partition,
                "Duplicate batch - returning success without write"
            );

            #[cfg(feature = "metrics")]
            if let Some(ref metrics) = self.metrics {
                let duration = start.elapsed().as_secs_f64();
                metrics.record_grpc_request(duration);
            }

            // Note: We're returning 0 for base_offset on duplicates
            // The producer should cache the original response
            return Ok(Response::new(ProduceResponse {
                base_offset: 0,
                record_count: req.records.len() as u32,
                log_append_time: None,
                record_errors: vec![],
                duplicates_filtered: true,
            }));
        }

        // Get writer for partition
        let writer = match self.writer_pool.get_writer(&req.topic, req.partition).await {
            Ok(writer) => writer,
            Err(e) => {
                error!(
                    topic = %req.topic,
                    partition = req.partition,
                    error = %e,
                    "Failed to get writer"
                );
                return Err(Status::internal(format!("Failed to get writer: {}", e)));
            }
        };

        // Append records
        let mut base_offset = None;
        let mut record_count = 0;

        {
            let mut writer_guard = writer.lock().await;

            for record in req.records {
                let timestamp = record.timestamp;
                let key = record.key.map(bytes::Bytes::from);
                let value = bytes::Bytes::from(record.value);

                match writer_guard.append(key, value, timestamp).await {
                    Ok(offset) => {
                        if base_offset.is_none() {
                            base_offset = Some(offset);
                        }
                        record_count += 1;
                    }
                    Err(e) => {
                        // Check for throttle errors and propagate backpressure (Phase 12.4.2)
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
                            "Failed to append record"
                        );
                        return Err(Status::internal(format!("Failed to append record: {}", e)));
                    }
                }
            }
        }

        let base_offset = base_offset.ok_or_else(|| {
            Status::internal("No base offset assigned (should not happen with non-empty batch)")
        })?;

        debug!(
            topic = %req.topic,
            partition = req.partition,
            base_offset = base_offset,
            record_count = record_count,
            "Successfully produced records"
        );

        // Handle ACK_DURABLE mode - wait for S3 persistence before acknowledging
        // AckMode: 0 = ACK_BUFFERED (default), 1 = ACK_DURABLE, 2 = ACK_NONE
        if req.ack_mode == 1 {
            debug!(
                topic = %req.topic,
                partition = req.partition,
                "ACK_DURABLE mode - flushing to S3 before acknowledgment"
            );

            let writer = match self.writer_pool.get_writer(&req.topic, req.partition).await {
                Ok(w) => w,
                Err(e) => {
                    error!(
                        topic = %req.topic,
                        partition = req.partition,
                        error = %e,
                        "Failed to get writer for durable flush"
                    );
                    return Err(Status::internal(format!(
                        "Failed to get writer for durable flush: {}",
                        e
                    )));
                }
            };

            {
                let mut writer_guard = writer.lock().await;
                if let Err(e) = writer_guard.flush_durable().await {
                    error!(
                        topic = %req.topic,
                        partition = req.partition,
                        error = %e,
                        "Failed to flush durable - S3 upload failed"
                    );
                    return Err(Status::internal(format!("Failed to flush durable: {}", e)));
                }
            }

            debug!(
                topic = %req.topic,
                partition = req.partition,
                "ACK_DURABLE flush completed - data persisted to S3"
            );
        }

        // Track transaction partition if this is a transactional write
        if let Some(ref transaction_id) = req.transaction_id {
            let last_offset = base_offset + record_count as u64 - 1;

            // Add partition to transaction (first time we write to this partition)
            if let Err(e) = self
                .metadata_store
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

            // Update the last offset for this partition in the transaction
            if let Err(e) = self
                .metadata_store
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
                // Don't fail the produce - the records are written
                // The transaction commit will use whatever offsets are tracked
            }
        }

        // Record metrics (Phase 7)
        #[cfg(feature = "metrics")]
        if let Some(ref metrics) = self.metrics {
            let duration = start.elapsed().as_secs_f64();
            metrics.record_write(record_count as u64, duration);
            metrics.record_grpc_request(duration);
        }

        Ok(Response::new(ProduceResponse {
            base_offset,
            record_count,
            log_append_time: None,
            record_errors: vec![],
            duplicates_filtered: false,
        }))
    }

    /// Initialize a producer and get a producer ID.
    async fn init_producer(
        &self,
        request: Request<InitProducerRequest>,
    ) -> std::result::Result<Response<InitProducerResponse>, Status> {
        let req = request.into_inner();

        // Build init config
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

        // Initialize producer in metadata store
        match self.metadata_store.init_producer(config).await {
            Ok(producer) => {
                info!(
                    producer_id = %producer.id,
                    epoch = producer.epoch,
                    transactional_id = ?producer.transactional_id,
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

    /// Begin a new transaction.
    async fn begin_transaction(
        &self,
        request: Request<BeginTransactionRequest>,
    ) -> std::result::Result<Response<BeginTransactionResponse>, Status> {
        let req = request.into_inner();

        // Validate producer exists and epoch matches
        let producer = match self.metadata_store.get_producer(&req.producer_id).await {
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

        // Begin transaction
        match self
            .metadata_store
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

    /// Commit a transaction.
    async fn commit_transaction(
        &self,
        request: Request<CommitTransactionRequest>,
    ) -> std::result::Result<Response<CommitTransactionResponse>, Status> {
        let req = request.into_inner();

        // Validate producer
        let producer = match self.metadata_store.get_producer(&req.producer_id).await {
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

        // Commit transaction
        match self
            .metadata_store
            .commit_transaction(&req.transaction_id)
            .await
        {
            Ok(commit_timestamp) => {
                info!(
                    producer_id = %req.producer_id,
                    transaction_id = %req.transaction_id,
                    commit_timestamp = commit_timestamp,
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

    /// Abort a transaction.
    async fn abort_transaction(
        &self,
        request: Request<AbortTransactionRequest>,
    ) -> std::result::Result<Response<AbortTransactionResponse>, Status> {
        let req = request.into_inner();

        // Validate producer
        let producer = match self.metadata_store.get_producer(&req.producer_id).await {
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

        // Abort transaction
        match self
            .metadata_store
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

    /// Send a heartbeat to keep producer and transaction alive.
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> std::result::Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        // Get producer to validate
        let producer = match self.metadata_store.get_producer(&req.producer_id).await {
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

        // Check epoch
        if producer.epoch != req.producer_epoch {
            return Ok(Response::new(HeartbeatResponse {
                valid: false,
                error: Some(format!(
                    "Producer epoch mismatch: expected {}, got {}",
                    producer.epoch, req.producer_epoch
                )),
            }));
        }

        // Check state
        if producer.state != ProducerState::Active {
            return Ok(Response::new(HeartbeatResponse {
                valid: false,
                error: Some(format!(
                    "Producer {} is {:?}",
                    req.producer_id, producer.state
                )),
            }));
        }

        // Update heartbeat
        if let Err(e) = self
            .metadata_store
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

// ============================================================================
// Agent Coordination Service (Fast Leader Handoff)
// ============================================================================

/// Agent Coordination service for fast leader handoff.
///
/// This service handles lease transfers between agents during rolling deploys,
/// graceful shutdown, and rebalancing.
pub struct AgentCoordinationImpl {
    /// Lease manager for handling transfers
    lease_manager: Arc<LeaseManager>,

    /// Metadata store for querying agent and partition info
    metadata_store: Arc<dyn MetadataStore>,

    /// Writer pool for flushing data before transfer
    writer_pool: Arc<WriterPool>,

    /// Agent ID
    agent_id: String,

    /// Agent state for checking if we're shutting down
    shutting_down: Arc<RwLock<bool>>,

    /// Current partition count (for health check)
    partition_count: Arc<RwLock<u32>>,

    /// Maximum partitions this agent can handle (for capacity reporting)
    max_partitions: u32,
}

impl AgentCoordinationImpl {
    /// Create a new AgentCoordination implementation.
    pub fn new(
        lease_manager: Arc<LeaseManager>,
        metadata_store: Arc<dyn MetadataStore>,
        writer_pool: Arc<WriterPool>,
        agent_id: String,
        max_partitions: u32,
    ) -> Self {
        Self {
            lease_manager,
            metadata_store,
            writer_pool,
            agent_id,
            shutting_down: Arc::new(RwLock::new(false)),
            partition_count: Arc::new(RwLock::new(0)),
            max_partitions,
        }
    }

    /// Signal that the service is shutting down.
    pub async fn shutdown(&self) {
        let mut shutting_down = self.shutting_down.write().await;
        *shutting_down = true;
    }

    /// Update partition count (for capacity reporting).
    pub async fn set_partition_count(&self, count: u32) {
        let mut pc = self.partition_count.write().await;
        *pc = count;
    }

    /// Convert proto TransferReason to LeaderChangeReason.
    fn convert_transfer_reason(reason: i32) -> LeaderChangeReason {
        match TransferReason::try_from(reason) {
            Ok(TransferReason::GracefulShutdown) => LeaderChangeReason::GracefulHandoff,
            Ok(TransferReason::RollingDeploy) => LeaderChangeReason::GracefulHandoff,
            Ok(TransferReason::Rebalance) => LeaderChangeReason::Rebalance,
            Ok(TransferReason::Maintenance) => LeaderChangeReason::GracefulHandoff,
            _ => LeaderChangeReason::GracefulHandoff,
        }
    }
}

#[tonic::async_trait]
impl AgentCoordination for AgentCoordinationImpl {
    /// Initiate a graceful lease transfer to another agent.
    async fn transfer_lease(
        &self,
        request: Request<TransferLeaseRequest>,
    ) -> std::result::Result<Response<TransferLeaseResponse>, Status> {
        let req = request.into_inner();

        // Verify this request is from us (we're the outgoing leader)
        if req.from_agent_id != self.agent_id {
            return Ok(Response::new(TransferLeaseResponse {
                accepted: false,
                transfer_id: String::new(),
                error: Some(format!(
                    "This agent is {}, not {}",
                    self.agent_id, req.from_agent_id
                )),
            }));
        }

        let reason = Self::convert_transfer_reason(req.reason);

        info!(
            agent_id = %self.agent_id,
            topic = %req.topic,
            partition = req.partition,
            to_agent = %req.to_agent_id,
            reason = ?reason,
            "Initiating lease transfer"
        );

        // Record metric
        streamhouse_observability::metrics::LEADER_TRANSFERS_PENDING.inc();

        match self
            .lease_manager
            .initiate_transfer(
                &req.topic,
                req.partition,
                &req.to_agent_id,
                reason,
                req.timeout_ms,
            )
            .await
        {
            Ok(transfer_id) => {
                info!(
                    transfer_id = %transfer_id,
                    topic = %req.topic,
                    partition = req.partition,
                    "Lease transfer initiated"
                );

                Ok(Response::new(TransferLeaseResponse {
                    accepted: true,
                    transfer_id,
                    error: None,
                }))
            }
            Err(e) => {
                warn!(
                    topic = %req.topic,
                    partition = req.partition,
                    error = %e,
                    "Failed to initiate lease transfer"
                );

                streamhouse_observability::metrics::LEADER_TRANSFERS_PENDING.dec();
                streamhouse_observability::metrics::LEADER_TRANSFERS_TOTAL
                    .with_label_values(&["error"])
                    .inc();

                Ok(Response::new(TransferLeaseResponse {
                    accepted: false,
                    transfer_id: String::new(),
                    error: Some(e.to_string()),
                }))
            }
        }
    }

    /// Accept a lease transfer from another agent.
    async fn accept_lease(
        &self,
        request: Request<AcceptLeaseRequest>,
    ) -> std::result::Result<Response<AcceptLeaseResponse>, Status> {
        let req = request.into_inner();

        // Verify we're the target agent
        if req.agent_id != self.agent_id {
            return Ok(Response::new(AcceptLeaseResponse {
                success: false,
                new_epoch: 0,
                lease_expires_at: 0,
                error: Some(format!(
                    "This agent is {}, not {}",
                    self.agent_id, req.agent_id
                )),
            }));
        }

        // Check if we're shutting down
        let shutting_down = *self.shutting_down.read().await;
        if shutting_down {
            return Ok(Response::new(AcceptLeaseResponse {
                success: false,
                new_epoch: 0,
                lease_expires_at: 0,
                error: Some("Agent is shutting down".to_string()),
            }));
        }

        // Check capacity
        let current = *self.partition_count.read().await;
        if current >= self.max_partitions {
            return Ok(Response::new(AcceptLeaseResponse {
                success: false,
                new_epoch: 0,
                lease_expires_at: 0,
                error: Some("Agent at capacity".to_string()),
            }));
        }

        info!(
            agent_id = %self.agent_id,
            transfer_id = %req.transfer_id,
            topic = %req.topic,
            partition = req.partition,
            "Accepting lease transfer"
        );

        match self.lease_manager.accept_transfer(&req.transfer_id).await {
            Ok(()) => {
                info!(
                    transfer_id = %req.transfer_id,
                    "Lease transfer accepted"
                );

                Ok(Response::new(AcceptLeaseResponse {
                    success: true,
                    new_epoch: req.expected_epoch,
                    lease_expires_at: 0, // Will be set on complete
                    error: None,
                }))
            }
            Err(e) => {
                warn!(
                    transfer_id = %req.transfer_id,
                    error = %e,
                    "Failed to accept lease transfer"
                );

                Ok(Response::new(AcceptLeaseResponse {
                    success: false,
                    new_epoch: 0,
                    lease_expires_at: 0,
                    error: Some(e.to_string()),
                }))
            }
        }
    }

    /// Complete a lease transfer after data sync.
    async fn complete_transfer(
        &self,
        request: Request<CompleteTransferRequest>,
    ) -> std::result::Result<Response<CompleteTransferResponse>, Status> {
        let req = request.into_inner();

        // Verify we're the source agent
        if req.from_agent_id != self.agent_id {
            return Ok(Response::new(CompleteTransferResponse {
                acknowledged: false,
                error: Some(format!(
                    "This agent is {}, not {}",
                    self.agent_id, req.from_agent_id
                )),
            }));
        }

        let start_time = std::time::Instant::now();

        info!(
            agent_id = %self.agent_id,
            transfer_id = %req.transfer_id,
            topic = %req.topic,
            partition = req.partition,
            last_flushed_offset = req.last_flushed_offset,
            high_watermark = req.high_watermark,
            "Completing lease transfer"
        );

        // Flush the writer to ensure all data is persisted
        if let Ok(writer) = self.writer_pool.get_writer(&req.topic, req.partition).await {
            let mut writer_guard = writer.lock().await;
            if let Err(e) = writer_guard.flush_durable().await {
                warn!(
                    transfer_id = %req.transfer_id,
                    error = %e,
                    "Failed to flush writer before transfer completion"
                );
            }
        }

        match self
            .lease_manager
            .complete_transfer(
                &req.transfer_id,
                req.last_flushed_offset,
                req.high_watermark,
            )
            .await
        {
            Ok(()) => {
                let duration = start_time.elapsed();

                info!(
                    transfer_id = %req.transfer_id,
                    duration_ms = duration.as_millis(),
                    "Lease transfer completed"
                );

                // Record metrics
                streamhouse_observability::metrics::LEADER_TRANSFERS_PENDING.dec();
                streamhouse_observability::metrics::LEADER_TRANSFERS_TOTAL
                    .with_label_values(&["success"])
                    .inc();
                streamhouse_observability::metrics::LEADER_HANDOFF_LATENCY
                    .with_label_values(&[&req.topic, &req.partition.to_string()])
                    .observe(duration.as_secs_f64());
                streamhouse_observability::metrics::LEADER_CHANGES_TOTAL
                    .with_label_values(&[
                        &req.topic,
                        &req.partition.to_string(),
                        "graceful_handoff",
                    ])
                    .inc();

                Ok(Response::new(CompleteTransferResponse {
                    acknowledged: true,
                    error: None,
                }))
            }
            Err(e) => {
                warn!(
                    transfer_id = %req.transfer_id,
                    error = %e,
                    "Failed to complete lease transfer"
                );

                streamhouse_observability::metrics::LEADER_TRANSFERS_PENDING.dec();
                streamhouse_observability::metrics::LEADER_TRANSFERS_TOTAL
                    .with_label_values(&["error"])
                    .inc();

                Ok(Response::new(CompleteTransferResponse {
                    acknowledged: false,
                    error: Some(e.to_string()),
                }))
            }
        }
    }

    /// Check agent health and readiness.
    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> std::result::Result<Response<HealthCheckResponse>, Status> {
        let req = request.into_inner();

        // Verify this is checking us
        if req.agent_id != self.agent_id {
            return Ok(Response::new(HealthCheckResponse {
                healthy: false,
                ready_for_leases: false,
                partition_count: 0,
                available_capacity: 0,
            }));
        }

        let shutting_down = *self.shutting_down.read().await;
        let current_partitions = *self.partition_count.read().await;
        let available = if shutting_down {
            0
        } else {
            self.max_partitions.saturating_sub(current_partitions)
        };

        Ok(Response::new(HealthCheckResponse {
            healthy: !shutting_down,
            ready_for_leases: !shutting_down && current_partitions < self.max_partitions,
            partition_count: current_partitions,
            available_capacity: available,
        }))
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add unit tests for ProducerServiceImpl
    // - test_produce_success
    // - test_produce_no_lease
    // - test_produce_expired_lease
    // - test_produce_shutting_down
    // - test_produce_empty_batch
}
