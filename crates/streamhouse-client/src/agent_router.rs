//! Agent Router for forwarding produce requests to partition-leader agents.
//!
//! Instead of writing directly to a local WriterPool, the unified server now
//! resolves which agent holds the lease for a partition and forwards the produce
//! request over gRPC to that agent's ProducerService.
//!
//! ## Architecture
//!
//! ```text
//! Client --> Unified Server (REST/gRPC/Kafka)
//!                |
//!                v
//!            AgentRouter
//!                |
//!                +-> MetadataStore: lease lookup (cached 5s)
//!                |
//!                +-> gRPC ProducerService on agent
//!                        |
//!                        +-> WAL / S3
//! ```
//!
//! ## Retry Policy
//!
//! On `NOT_FOUND` or `FAILED_PRECONDITION` (lease moved), the router
//! invalidates the cached lease and retries once with a fresh lookup.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use streamhouse_metadata::MetadataStore;
use streamhouse_proto::producer::{
    produce_request::Record, producer_service_client::ProducerServiceClient, ProduceRequest,
    ProduceResponse,
};
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by the AgentRouter.
#[derive(Debug, thiserror::Error)]
pub enum AgentRouterError {
    #[error("No lease holder for {topic}/{partition}")]
    NoLeaseHolder { topic: String, partition: u32 },

    #[error("Agent {agent_id} is unavailable: {reason}")]
    AgentUnavailable { agent_id: String, reason: String },

    #[error("Lease expired for {topic}/{partition}")]
    LeaseExpired { topic: String, partition: u32 },

    #[error("Lease moved for {topic}/{partition} -- retried and still failed: {reason}")]
    LeaseMoved {
        topic: String,
        partition: u32,
        reason: String,
    },

    #[error("Metadata lookup failed: {0}")]
    MetadataError(String),

    #[error("Agent not found in registry: {0}")]
    AgentNotFound(String),

    #[error("gRPC error from agent: {0}")]
    GrpcError(#[from] tonic::Status),

    #[error("Transport error: {0}")]
    TransportError(String),
}

// ---------------------------------------------------------------------------
// Cached lease entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CachedLease {
    agent_id: String,
    agent_address: String,
    fetched_at: Instant,
}

const LEASE_CACHE_TTL: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// AgentRouter
// ---------------------------------------------------------------------------

/// Routes produce requests to the agent that holds the partition lease.
pub struct AgentRouter {
    metadata: Arc<dyn MetadataStore>,
    /// Cached gRPC channels keyed by agent address.
    connections: RwLock<HashMap<String, ProducerServiceClient<Channel>>>,
    /// Lease cache: (topic, partition) -> CachedLease.
    lease_cache: RwLock<HashMap<(String, u32), CachedLease>>,
}

impl AgentRouter {
    /// Create a new AgentRouter.
    pub fn new(metadata: Arc<dyn MetadataStore>) -> Self {
        Self {
            metadata,
            connections: RwLock::new(HashMap::new()),
            lease_cache: RwLock::new(HashMap::new()),
        }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Produce a batch of records to a partition, forwarding to the lease-holder agent.
    ///
    /// On lease-moved errors the cache is invalidated and the request is retried once.
    pub async fn produce(
        &self,
        org_id: &str,
        topic: &str,
        partition: u32,
        records: Vec<Record>,
        ack_mode: i32,
        producer_id: Option<String>,
        producer_epoch: Option<u32>,
        base_sequence: Option<i64>,
        transaction_id: Option<String>,
    ) -> Result<ProduceResponse, AgentRouterError> {
        // First attempt
        match self
            .try_produce(
                org_id,
                topic,
                partition,
                &records,
                ack_mode,
                producer_id.as_deref(),
                producer_epoch,
                base_sequence,
                transaction_id.as_deref(),
            )
            .await
        {
            Ok(resp) => Ok(resp),
            Err(e) if Self::is_lease_moved_error(&e) => {
                warn!(
                    topic = %topic,
                    partition = partition,
                    error = %e,
                    "Lease moved -- invalidating cache and retrying"
                );
                self.invalidate_lease_cache(topic, partition).await;

                // Retry once
                self.try_produce(
                    org_id,
                    topic,
                    partition,
                    &records,
                    ack_mode,
                    producer_id.as_deref(),
                    producer_epoch,
                    base_sequence,
                    transaction_id.as_deref(),
                )
                .await
                .map_err(|retry_err| AgentRouterError::LeaseMoved {
                    topic: topic.to_string(),
                    partition,
                    reason: retry_err.to_string(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Convenience wrapper for producing a single record (REST single-produce).
    pub async fn produce_single(
        &self,
        org_id: &str,
        topic: &str,
        partition: u32,
        key: Option<Vec<u8>>,
        value: Vec<u8>,
        timestamp: u64,
        ack_mode: i32,
    ) -> Result<ProduceResponse, AgentRouterError> {
        let record = Record {
            key,
            value,
            timestamp,
            headers: HashMap::new(),
        };

        self.produce(
            org_id,
            topic,
            partition,
            vec![record],
            ack_mode,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Resolve which agent holds the lease for a given partition.
    ///
    /// Returns `(agent_id, agent_address)`.
    pub async fn resolve_agent_for_partition(
        &self,
        topic: &str,
        partition: u32,
    ) -> Result<(String, String), AgentRouterError> {
        // Check cache first
        {
            let cache = self.lease_cache.read().await;
            if let Some(entry) = cache.get(&(topic.to_string(), partition)) {
                if entry.fetched_at.elapsed() < LEASE_CACHE_TTL {
                    return Ok((entry.agent_id.clone(), entry.agent_address.clone()));
                }
            }
        }

        // Cache miss or stale -- look up from metadata
        let lease = self
            .metadata
            .get_partition_lease(topic, partition)
            .await
            .map_err(|e| AgentRouterError::MetadataError(e.to_string()))?
            .ok_or_else(|| AgentRouterError::NoLeaseHolder {
                topic: topic.to_string(),
                partition,
            })?;

        // Verify lease isn't expired
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        if lease.lease_expires_at <= now_ms {
            return Err(AgentRouterError::LeaseExpired {
                topic: topic.to_string(),
                partition,
            });
        }

        // Look up agent address
        let agent = self
            .metadata
            .get_agent(&lease.leader_agent_id)
            .await
            .map_err(|e| AgentRouterError::MetadataError(e.to_string()))?
            .ok_or_else(|| AgentRouterError::AgentNotFound(lease.leader_agent_id.clone()))?;

        // Ensure the address has a scheme for tonic
        let address =
            if agent.address.starts_with("http://") || agent.address.starts_with("https://") {
                agent.address.clone()
            } else {
                format!("http://{}", agent.address)
            };

        // Update cache
        {
            let mut cache = self.lease_cache.write().await;
            cache.insert(
                (topic.to_string(), partition),
                CachedLease {
                    agent_id: lease.leader_agent_id.clone(),
                    agent_address: address.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok((lease.leader_agent_id, address))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Single attempt to forward a produce request to the lease-holder agent.
    async fn try_produce(
        &self,
        org_id: &str,
        topic: &str,
        partition: u32,
        records: &[Record],
        ack_mode: i32,
        producer_id: Option<&str>,
        producer_epoch: Option<u32>,
        base_sequence: Option<i64>,
        transaction_id: Option<&str>,
    ) -> Result<ProduceResponse, AgentRouterError> {
        let (_, address) = self.resolve_agent_for_partition(topic, partition).await?;

        let mut client = self.get_or_create_client(&address).await?;

        let request = ProduceRequest {
            topic: topic.to_string(),
            partition,
            records: records.to_vec(),
            producer_id: producer_id.map(|s| s.to_string()),
            producer_epoch,
            base_sequence,
            transaction_id: transaction_id.map(|s| s.to_string()),
            ack_mode,
        };

        // Attach organization ID as gRPC metadata so the agent uses the correct org scope
        let mut grpc_request = tonic::Request::new(request);
        grpc_request.metadata_mut().insert(
            "x-organization-id",
            org_id
                .parse()
                .unwrap_or_else(|_| tonic::metadata::MetadataValue::from_static("")),
        );

        let response = client
            .produce(grpc_request)
            .await
            .map_err(|status| {
                if Self::is_transport_error(&status) {
                    let addr = address.clone();
                    debug!(address = %addr, "Transport error — will create fresh connection on next request");
                }
                AgentRouterError::GrpcError(status)
            })?;

        Ok(response.into_inner())
    }

    /// Get an existing ProducerServiceClient or create a new one.
    async fn get_or_create_client(
        &self,
        address: &str,
    ) -> Result<ProducerServiceClient<Channel>, AgentRouterError> {
        // Fast path: read lock
        {
            let conns = self.connections.read().await;
            if let Some(client) = conns.get(address) {
                return Ok(client.clone());
            }
        }

        // Slow path: create connection
        let channel = Endpoint::from_shared(address.to_string())
            .map_err(|e| AgentRouterError::TransportError(e.to_string()))?
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .http2_keep_alive_interval(Duration::from_secs(20))
            .keep_alive_timeout(Duration::from_secs(5))
            .keep_alive_while_idle(true)
            .http2_adaptive_window(true)
            .initial_connection_window_size(Some(1024 * 1024))
            .initial_stream_window_size(Some(1024 * 1024))
            .connect()
            .await
            .map_err(|e| AgentRouterError::TransportError(e.to_string()))?;

        let client = ProducerServiceClient::new(channel)
            .max_decoding_message_size(64 * 1024 * 1024)
            .max_encoding_message_size(64 * 1024 * 1024);

        // Cache the client
        {
            let mut conns = self.connections.write().await;
            conns.insert(address.to_string(), client.clone());
        }

        debug!(address = %address, "Created new ProducerService connection");
        Ok(client)
    }

    /// Invalidate the lease cache for a specific partition.
    async fn invalidate_lease_cache(&self, topic: &str, partition: u32) {
        let mut cache = self.lease_cache.write().await;
        cache.remove(&(topic.to_string(), partition));
    }

    /// Check whether an error indicates the lease has moved (should retry).
    fn is_lease_moved_error(err: &AgentRouterError) -> bool {
        match err {
            AgentRouterError::GrpcError(status) => matches!(
                status.code(),
                tonic::Code::NotFound | tonic::Code::FailedPrecondition
            ),
            AgentRouterError::LeaseExpired { .. } => true,
            AgentRouterError::NoLeaseHolder { .. } => true,
            _ => false,
        }
    }

    /// Check whether a gRPC status is a transport-level error.
    fn is_transport_error(status: &tonic::Status) -> bool {
        matches!(
            status.code(),
            tonic::Code::Unavailable | tonic::Code::Unknown
        )
    }

    /// Shut down the router, closing all cached connections.
    pub async fn shutdown(&self) {
        let mut conns = self.connections.write().await;
        conns.clear();
        let mut cache = self.lease_cache.write().await;
        cache.clear();
        debug!("AgentRouter shut down -- all connections and caches cleared");
    }
}
