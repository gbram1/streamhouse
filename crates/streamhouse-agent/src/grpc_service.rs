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
use streamhouse_metadata::MetadataStore;
use streamhouse_proto::producer::{
    producer_service_server::ProducerService, ProduceRequest, ProduceResponse,
};
use streamhouse_storage::writer_pool::WriterPool;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{debug, error, warn};

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
            prometheus_client::metrics::histogram::exponential_buckets(0.001, 2.0, 15)
        );

        let active_connections = prometheus_client::metrics::gauge::Gauge::<i64>::default();
        let grpc_requests = prometheus_client::metrics::counter::Counter::<u64>::default();

        let grpc_duration = prometheus_client::metrics::histogram::Histogram::new(
            prometheus_client::metrics::histogram::exponential_buckets(0.001, 2.0, 15)
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
    async fn produce(
        &self,
        request: Request<ProduceRequest>,
    ) -> std::result::Result<Response<ProduceResponse>, Status> {
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

        Ok(Response::new(ProduceResponse {
            base_offset,
            record_count,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Add unit tests for ProducerServiceImpl
    // - test_produce_success
    // - test_produce_no_lease
    // - test_produce_expired_lease
    // - test_produce_shutting_down
    // - test_produce_empty_batch
}
