//! Kafka protocol server
//!
//! TCP server that handles Kafka binary protocol connections.

use std::net::SocketAddr;
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
        Self {
            config,
            metadata,
            writer_pool,
            segment_cache,
            object_store,
            group_coordinator,
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

/// Per-connection state
struct ConnectionState {
    addr: SocketAddr,
    client_id: Option<String>,
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
        addr,
        client_id: None,
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

        debug!(
            "Request: api_key={}, version={}, correlation_id={}, client_id={:?}",
            header.api_key, header.api_version, header.correlation_id, header.client_id
        );

        // Handle the request
        let response = handle_request(&state, &conn_state, &header, &mut frame).await?;

        // Send response
        framed.send(response).await?;
    }

    Err(KafkaError::ConnectionClosed)
}

/// Route request to appropriate handler
async fn handle_request(
    state: &Arc<KafkaServerState>,
    conn_state: &ConnectionState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    let api_key = ApiKey::from_i16(header.api_key);

    let response_body = match api_key {
        Some(ApiKey::ApiVersions) => handlers::handle_api_versions(state, header, body).await?,
        Some(ApiKey::Metadata) => handlers::handle_metadata(state, header, body).await?,
        Some(ApiKey::Produce) => handlers::handle_produce(state, header, body).await?,
        Some(ApiKey::Fetch) => handlers::handle_fetch(state, header, body).await?,
        Some(ApiKey::ListOffsets) => handlers::handle_list_offsets(state, header, body).await?,
        Some(ApiKey::FindCoordinator) => {
            handlers::handle_find_coordinator(state, header, body).await?
        }
        Some(ApiKey::JoinGroup) => handlers::handle_join_group(state, header, body).await?,
        Some(ApiKey::SyncGroup) => handlers::handle_sync_group(state, header, body).await?,
        Some(ApiKey::Heartbeat) => handlers::handle_heartbeat(state, header, body).await?,
        Some(ApiKey::LeaveGroup) => handlers::handle_leave_group(state, header, body).await?,
        Some(ApiKey::OffsetCommit) => handlers::handle_offset_commit(state, header, body).await?,
        Some(ApiKey::OffsetFetch) => handlers::handle_offset_fetch(state, header, body).await?,
        Some(ApiKey::CreateTopics) => handlers::handle_create_topics(state, header, body).await?,
        Some(ApiKey::DeleteTopics) => handlers::handle_delete_topics(state, header, body).await?,
        Some(ApiKey::DescribeGroups) => {
            handlers::handle_describe_groups(state, header, body).await?
        }
        Some(ApiKey::ListGroups) => handlers::handle_list_groups(state, header, body).await?,
        None => {
            warn!("Unsupported API key: {}", header.api_key);
            return Err(KafkaError::UnsupportedApi(
                header.api_key,
                header.api_version,
            ));
        }
    };

    // Build full response with header
    let mut response = BytesMut::new();
    ResponseHeader::new(header.correlation_id).encode(&mut response);
    response.extend_from_slice(&response_body);

    Ok(response)
}
