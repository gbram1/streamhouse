//! Kafka protocol server
//!
//! TCP server that handles Kafka binary protocol connections.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bytes::BytesMut;
use futures::SinkExt;
use object_store::ObjectStore;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::Framed;
use tracing::{debug, error, info, instrument, warn};

use streamhouse_metadata::MetadataStore;
use streamhouse_storage::{SegmentCache, WriterPool};

use crate::codec::{KafkaCodec, RequestHeader, ResponseHeader};
use crate::coordinator::GroupCoordinator;
use crate::error::{KafkaError, KafkaResult};
use crate::handlers;
use crate::tenant::KafkaTenantResolver;
use crate::types::ApiKey;

/// Kafka server configuration
#[derive(Debug, Clone)]
pub struct KafkaServerConfig {
    /// Address to bind to
    pub bind_addr: String,
    /// Node ID for this broker
    pub node_id: i32,
    /// Advertised host for clients
    pub advertised_host: String,
    /// Advertised port for clients
    pub advertised_port: i32,
}

impl Default for KafkaServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:9092".to_string(),
            node_id: 0,
            advertised_host: "localhost".to_string(),
            advertised_port: 9092,
        }
    }
}

/// Shared state for all Kafka connections
pub struct KafkaServerState {
    /// Configuration
    pub config: KafkaServerConfig,
    /// Metadata store for topic/partition information
    pub metadata: Arc<dyn MetadataStore>,
    /// Writer pool for producing messages
    pub writer_pool: Arc<WriterPool>,
    /// Segment cache for consuming messages
    pub segment_cache: Arc<SegmentCache>,
    /// Object store for segment data
    pub object_store: Arc<dyn ObjectStore>,
    /// Consumer group coordinator
    pub group_coordinator: Arc<GroupCoordinator>,
    /// Tenant resolver for SASL and client_id-based authentication
    pub tenant_resolver: Option<KafkaTenantResolver<dyn MetadataStore>>,
    /// Quota enforcer for rate limiting (None = rate limiting disabled)
    pub quota_enforcer: Option<Arc<streamhouse_metadata::QuotaEnforcer<dyn MetadataStore>>>,
    /// Active connections per organization for connection quota enforcement
    pub active_connections: Arc<dashmap::DashMap<String, AtomicU32>>,
}

impl KafkaServerState {
    pub fn new(
        config: KafkaServerConfig,
        metadata: Arc<dyn MetadataStore>,
        writer_pool: Arc<WriterPool>,
        segment_cache: Arc<SegmentCache>,
        object_store: Arc<dyn ObjectStore>,
    ) -> Self {
        let group_coordinator = Arc::new(GroupCoordinator::new(metadata.clone()));
        let tenant_resolver = Some(KafkaTenantResolver::new(metadata.clone()));
        Self {
            config,
            metadata,
            writer_pool,
            segment_cache,
            object_store,
            group_coordinator,
            tenant_resolver,
            quota_enforcer: None,
            active_connections: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Build a TenantContext for quota checking by looking up the real org from metadata.
    /// Falls back to Free plan defaults if the org is not found.
    pub async fn resolve_tenant_context(
        &self,
        org_id: &str,
    ) -> streamhouse_metadata::tenant::TenantContext {
        if let Some(ref enforcer) = self.quota_enforcer {
            enforcer.resolve_tenant_context(org_id).await
        } else {
            // No enforcer — build minimal context with Free defaults
            streamhouse_metadata::tenant::TenantContext {
                organization: streamhouse_metadata::Organization {
                    id: org_id.to_string(),
                    name: String::new(),
                    slug: String::new(),
                    plan: streamhouse_metadata::OrganizationPlan::Free,
                    status: streamhouse_metadata::OrganizationStatus::Active,
                    created_at: 0,
                    settings: std::collections::HashMap::new(),
                    clerk_id: None,
                },
                api_key: None,
                quota: streamhouse_metadata::QuotaEnforcer::<dyn MetadataStore>::default_quotas_for_plan(
                    streamhouse_metadata::OrganizationPlan::Free,
                    org_id,
                ),
                is_default: org_id == streamhouse_metadata::DEFAULT_ORGANIZATION_ID,
            }
        }
    }
}

/// Kafka protocol server
pub struct KafkaServer {
    state: Arc<KafkaServerState>,
}

impl KafkaServer {
    /// Create a new Kafka server with the given state
    pub fn new(state: Arc<KafkaServerState>) -> Self {
        Self { state }
    }

    /// Create and bind a new Kafka server (convenience method)
    pub async fn bind(
        config: KafkaServerConfig,
        metadata: Arc<dyn MetadataStore>,
        writer_pool: Arc<WriterPool>,
        segment_cache: Arc<SegmentCache>,
        object_store: Arc<dyn ObjectStore>,
    ) -> KafkaResult<BoundKafkaServer> {
        let listener = TcpListener::bind(&config.bind_addr).await?;
        let local_addr = listener.local_addr()?;
        info!("Kafka server listening on {}", local_addr);

        let state = Arc::new(KafkaServerState::new(
            config,
            metadata,
            writer_pool,
            segment_cache,
            object_store,
        ));

        Ok(BoundKafkaServer { listener, state })
    }

    /// Run the server until the shutdown signal is received
    pub async fn run_until(self, shutdown: tokio::sync::oneshot::Receiver<()>) -> KafkaResult<()> {
        let listener = TcpListener::bind(&self.state.config.bind_addr).await?;
        let local_addr = listener.local_addr()?;
        info!("Kafka server listening on {}", local_addr);

        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let state = self.state.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, addr, state).await {
                                    match e {
                                        KafkaError::ConnectionClosed => {
                                            debug!("Connection closed: {}", addr);
                                        }
                                        _ => {
                                            warn!("Connection error from {}: {}", addr, e);
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = &mut shutdown => {
                    info!("Kafka server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Kafka server that has been bound to a port
pub struct BoundKafkaServer {
    listener: TcpListener,
    state: Arc<KafkaServerState>,
}

impl BoundKafkaServer {
    /// Run the server, accepting connections
    pub async fn run(self) -> KafkaResult<()> {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, addr, state).await {
                            match e {
                                KafkaError::ConnectionClosed => {
                                    debug!("Connection closed: {}", addr);
                                }
                                _ => {
                                    warn!("Connection error from {}: {}", addr, e);
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Get the local address the server is bound to
    pub fn local_addr(&self) -> KafkaResult<SocketAddr> {
        self.listener.local_addr().map_err(KafkaError::from)
    }
}

/// Handle a single Kafka client connection (public API for embedding/testing).
pub async fn handle_connection_public(
    stream: TcpStream,
    addr: SocketAddr,
    state: Arc<KafkaServerState>,
) -> KafkaResult<()> {
    handle_connection(stream, addr, state).await
}

/// Drop guard that decrements the org connection counter when the connection closes.
struct ConnectionGuard {
    org_id: String,
    active_connections: Arc<dashmap::DashMap<String, AtomicU32>>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(counter) = self.active_connections.get(&self.org_id) {
            counter.value().fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Per-connection state
pub(crate) struct ConnectionState {
    _addr: SocketAddr,
    client_id: Option<String>,
    /// Resolved tenant context after SASL authentication
    pub(crate) tenant: Option<crate::tenant::KafkaTenantContext>,
    /// Guard that decrements connection count on drop
    _connection_guard: Option<ConnectionGuard>,
}

/// Handle a single client connection
#[instrument(skip(stream, state), fields(client = %addr))]
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    state: Arc<KafkaServerState>,
) -> KafkaResult<()> {
    debug!("New connection from {}", addr);

    let mut framed = Framed::new(stream, KafkaCodec::new());
    let mut conn_state = ConnectionState {
        _addr: addr,
        client_id: None,
        tenant: None,
        _connection_guard: None,
    };

    use futures::StreamExt;
    while let Some(result) = framed.next().await {
        let mut frame = match result {
            Ok(frame) => frame,
            Err(e) => {
                warn!("Frame decode error: {}", e);
                return Err(e);
            }
        };

        // Parse request header
        let header = RequestHeader::parse(&mut frame)?;

        // Update client_id if provided
        if let Some(ref client_id) = header.client_id {
            conn_state.client_id = Some(client_id.clone());
        }

        // Try client_id-based tenant resolution on first request with a client_id
        if conn_state.tenant.is_none() {
            if let Some(ref client_id) = conn_state.client_id {
                if let Some(ref resolver) = state.tenant_resolver {
                    if let Ok(Some(ctx)) = resolver.try_resolve_from_client_id(client_id).await {
                        debug!("Resolved tenant from client_id: org={}", ctx.tenant.organization.id);
                        conn_state.tenant = Some(ctx);
                    }
                }
            }
        }

        // Track connection and check quota once tenant is resolved (first time only)
        if conn_state._connection_guard.is_none() {
            if let Some(ref tenant) = conn_state.tenant {
                let org_id = tenant.tenant.organization.id.clone();

                // Increment connection count
                let count = {
                    let entry = state
                        .active_connections
                        .entry(org_id.clone())
                        .or_insert_with(|| AtomicU32::new(0));
                    entry.value().fetch_add(1, Ordering::Relaxed) + 1
                };

                // Set up drop guard to decrement on disconnect
                conn_state._connection_guard = Some(ConnectionGuard {
                    org_id: org_id.clone(),
                    active_connections: state.active_connections.clone(),
                });

                // Check connection quota
                if let Some(ref enforcer) = state.quota_enforcer {
                    let tenant_ctx = &tenant.tenant;
                    if let Ok(streamhouse_metadata::QuotaCheck::Denied(reason)) =
                        enforcer.check_connection(tenant_ctx, count as i32).await
                    {
                        warn!(
                            "Connection limit exceeded: org={}, count={}, reason={}",
                            org_id, count, reason
                        );
                        return Err(KafkaError::ConnectionClosed);
                    }
                }
            }
        }

        debug!(
            "Request: api_key={}, version={}, correlation_id={}, client_id={:?}",
            header.api_key, header.api_version, header.correlation_id, header.client_id
        );

        // Handle the request
        let (response, throttle_time_ms) = handle_request(&state, &mut conn_state, &header, &mut frame).await?;

        // Enforce throttle delay — hold the response so the client physically
        // cannot send the next request until the delay expires (same as Apache Kafka).
        if throttle_time_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(throttle_time_ms as u64)).await;
        }

        // Send response
        framed.send(response).await?;
    }

    Err(KafkaError::ConnectionClosed)
}

/// Route request to appropriate handler.
/// Returns (response_bytes, throttle_time_ms) so the connection loop can delay the response.
async fn handle_request(
    state: &Arc<KafkaServerState>,
    conn_state: &mut ConnectionState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<(BytesMut, i32)> {
    let api_key = ApiKey::from_i16(header.api_key);

    // Handle SASL APIs first (they can mutate connection state)
    if let Some(ApiKey::SaslHandshake) = api_key {
        let response_body = handlers::handle_sasl_handshake(state, header, body).await?;
        return Ok((build_response(header, response_body)?, 0));
    }
    if let Some(ApiKey::SaslAuthenticate) = api_key {
        let (response_body, tenant_ctx) =
            handlers::handle_sasl_authenticate(state, header, body).await?;
        if let Some(ctx) = tenant_ctx {
            debug!("SASL auth success: org={}", ctx.tenant.organization.id);
            conn_state.tenant = Some(ctx);
        }
        return Ok((build_response(header, response_body)?, 0));
    }

    // Extract org_id from tenant context (default for unauthenticated connections)
    let org_id = conn_state
        .tenant
        .as_ref()
        .map(|t| t.tenant.organization.id.as_str())
        .unwrap_or(streamhouse_metadata::DEFAULT_ORGANIZATION_ID);

    // Compute throttle_time_ms for Kafka backpressure
    let throttle_time_ms = if let Some(ref enforcer) = state.quota_enforcer {
        // Use real org plan from metadata (or SASL-resolved tenant context)
        let tenant_ctx = match conn_state.tenant {
            Some(ref t) => t.tenant.clone(),
            None => state.resolve_tenant_context(org_id).await,
        };

        // Check request rate and get throttle time
        let _ = enforcer.check_request(&tenant_ctx, None).await;
        let ms = enforcer.throttle_time_ms(&tenant_ctx, None).await;
        if ms > 0 {
            streamhouse_observability::metrics::RATE_LIMIT_TOTAL
                .with_label_values(&[org_id, "denied", "kafka"])
                .inc();
            streamhouse_observability::metrics::THROTTLE_TIME_MS
                .with_label_values(&[org_id, "kafka"])
                .observe(ms as f64);
        } else {
            streamhouse_observability::metrics::RATE_LIMIT_TOTAL
                .with_label_values(&[org_id, "allowed", "kafka"])
                .inc();
        }
        ms
    } else {
        0
    };

    let response_body = match api_key {
        Some(ApiKey::ApiVersions) => handlers::handle_api_versions(state, header, body).await?,
        Some(ApiKey::Metadata) => handlers::handle_metadata(state, org_id, header, body).await?,
        Some(ApiKey::Produce) => handlers::handle_produce(state, org_id, header, body, throttle_time_ms).await?,
        Some(ApiKey::Fetch) => handlers::handle_fetch(state, org_id, header, body, throttle_time_ms).await?,
        Some(ApiKey::ListOffsets) => {
            handlers::handle_list_offsets(state, org_id, header, body).await?
        }
        Some(ApiKey::FindCoordinator) => {
            handlers::handle_find_coordinator(state, header, body).await?
        }
        Some(ApiKey::JoinGroup) => {
            handlers::handle_join_group(state, org_id, header, body).await?
        }
        Some(ApiKey::SyncGroup) => {
            handlers::handle_sync_group(state, org_id, header, body).await?
        }
        Some(ApiKey::Heartbeat) => {
            handlers::handle_heartbeat(state, org_id, header, body).await?
        }
        Some(ApiKey::LeaveGroup) => {
            handlers::handle_leave_group(state, org_id, header, body).await?
        }
        Some(ApiKey::OffsetCommit) => {
            handlers::handle_offset_commit(state, org_id, header, body).await?
        }
        Some(ApiKey::OffsetFetch) => {
            handlers::handle_offset_fetch(state, org_id, header, body).await?
        }
        Some(ApiKey::CreateTopics) => {
            handlers::handle_create_topics(state, org_id, header, body).await?
        }
        Some(ApiKey::DeleteTopics) => {
            handlers::handle_delete_topics(state, org_id, header, body).await?
        }
        Some(ApiKey::DescribeGroups) => {
            handlers::handle_describe_groups(state, org_id, header, body).await?
        }
        Some(ApiKey::ListGroups) => {
            handlers::handle_list_groups(state, org_id, header, body).await?
        }
        Some(ApiKey::InitProducerId) => {
            handlers::handle_init_producer_id(state, org_id, header, body).await?
        }
        Some(ApiKey::AddPartitionsToTxn) => {
            handlers::handle_add_partitions_to_txn(state, org_id, header, body).await?
        }
        Some(ApiKey::AddOffsetsToTxn) => {
            handlers::handle_add_offsets_to_txn(state, org_id, header, body).await?
        }
        Some(ApiKey::EndTxn) => handlers::handle_end_txn(state, org_id, header, body).await?,
        Some(ApiKey::TxnOffsetCommit) => {
            handlers::handle_txn_offset_commit(state, org_id, header, body).await?
        }
        Some(ApiKey::SaslHandshake) | Some(ApiKey::SaslAuthenticate) => {
            unreachable!("SASL APIs handled above")
        }
        None => {
            warn!("Unsupported API key: {}", header.api_key);
            return Err(KafkaError::UnsupportedApi(
                header.api_key,
                header.api_version,
            ));
        }
    };

    Ok((build_response(header, response_body)?, throttle_time_ms))
}

/// Build a full Kafka response with the appropriate header format.
fn build_response(header: &RequestHeader, response_body: BytesMut) -> KafkaResult<BytesMut> {
    // ApiVersions always uses response header v0 (just correlation_id).
    // Other flexible-version APIs use response header v1 (correlation_id + tagged fields).
    let mut response = BytesMut::new();
    let resp_header = ResponseHeader::new(header.correlation_id);
    let flexible = crate::types::is_flexible_version(header.api_key, header.api_version);
    if flexible && header.api_key != ApiKey::ApiVersions.as_i16() {
        resp_header.encode_flexible(&mut response);
    } else {
        resp_header.encode(&mut response);
    }
    response.extend_from_slice(&response_body);

    Ok(response)
}
