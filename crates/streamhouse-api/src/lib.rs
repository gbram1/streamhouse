//! StreamHouse REST API Server
//!
//! HTTP/JSON API for managing StreamHouse clusters via web console and other HTTP clients.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use streamhouse_client::Producer;
use streamhouse_metadata::MetadataStore;
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod acl;
pub mod active_active;
pub mod audit;
pub mod audit_store;
pub mod auth;
pub mod compliance;
pub mod failover;
pub mod handlers;
pub mod jwt;
pub mod leader;
pub mod models;
pub mod oauth;
pub mod pitr;
pub mod opa;
pub mod rbac;
pub mod replication;
pub mod prometheus;
pub mod sasl;
pub mod shutdown;
pub mod tls;
pub mod cluster;
pub mod discovery;
pub mod health_monitor;

pub use cluster::{
    ClusterCoordinator, ClusterNode, ClusterTopology, NodeStatus,
};
pub use discovery::{
    DiscoveredNode, DiscoveryConfig, DiscoveryEvent, DnsDiscovery, MetadataStoreDiscovery,
    ServiceDiscovery, StaticDiscovery,
};
pub use health_monitor::{
    ClusterHealth, ClusterHealthMonitor, HealthThresholds, NodeHealth, NodeHealthStatus,
};

pub use acl::{
    AclAction, AclCheckResult, AclChecker, AclConfig, AclEntry, AclLayer, AclPermission,
    AclResource,
};
pub use active_active::{
    ActiveActiveConfig, ActiveActiveCoordinator, ActiveActiveError, ActiveActiveStats,
    ConflictRecord, ConflictResolution, ConflictResolver, ConflictStrategy, RegionState,
    SyncResult,
};
pub use audit::{log_audit_event, AuditConfig, AuditEntry, AuditEventType, AuditLayer};
pub use audit_store::{
    AuditBackend, AuditQuery, AuditStore, AuditStoreConfig, AuditStoreError, AuditStoreStats,
    StoredAuditEntry, StoredAuditRecord, VerificationResult,
};
pub use auth::{
    AuthConfig, AuthError, AuthLayer, AuthenticatedKey, RequiredPermission, SmartAuthLayer,
};
pub use compliance::{
    ComplianceError, ComplianceReport, ComplianceReporter, Finding, FindingSeverity, Framework,
    ReportConfig, ReportFormat, ReportMetadata,
};
pub use failover::{
    FailoverConfig, FailoverError, FailoverEvent, FailoverManager, FailoverPolicy, FailoverStats,
    HealthCheckResult, HealthChecker, HealthStatus, HttpHealthChecker, InMemoryHealthChecker,
    NodeInfo,
};
pub use jwt::{Claims, JwtConfig, JwtError, JwtLayer, JwtService, SmartJwtLayer};
pub use leader::{
    LeaderBackend, LeaderConfig, LeaderElection, LeaderError, LeaderEvent, LeaderGuard,
    LeaderState, LeadershipRecord, LostReason,
};
pub use oauth::{
    oauth_router, IdTokenClaims, OAuthConfig, OAuthError, OAuthProvider, OAuthService,
    OAuthSession, OAuthTokens, PkceChallenge, UserInfo,
};
pub use pitr::{
    BackupMetadata, BackupStatus, BaseBackup, PitrConfig, PitrConfigBuilder, PitrError,
    PitrManager, RecoveryResult, RecoveryTarget, SystemSnapshot,
    VerificationResult as PitrVerificationResult, WalEntry, WalOperation, WalSegment, WalStats,
};
pub use opa::{
    OpaClient, OpaConfig, OpaDecision, OpaDecisionSource, OpaError, OpaInput, OpaLayer,
    OpaRbacManager, OpaResult, OpaResponse, OpaService,
};
pub use rbac::{
    DataMasker, DataMaskingPolicy, MaskType, Permission, RbacError, RbacLayer, RbacManager,
    RbacMiddleware, Role, RoleAssignment, ADMIN_ROLE_ID, DEVELOPER_ROLE_ID, OPERATOR_ROLE_ID,
    VIEWER_ROLE_ID,
};
pub use replication::{
    RegionEndpoint, RegionReplicationState, ReplicatedSegment, ReplicationConfig, ReplicationError,
    ReplicationLag, ReplicationManager, ReplicationMode, ReplicationStats, ReplicationStatus,
};
pub use prometheus::PrometheusClient;
pub use sasl::{
    ScramClient, ScramConfig, ScramError, ScramMechanism, ScramServer, ScramState,
    StoredCredentials,
};
pub use shutdown::{
    serve_with_custom_shutdown, serve_with_shutdown, shutdown_signal, GracefulShutdown,
    ShutdownBuilder, ShutdownHandle, ShutdownSignal,
};
pub use tls::{
    serve_with_tls, serve_with_tls_and_shutdown, serve_with_tls_custom_shutdown, TlsConfig,
    TlsError,
};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub metadata: Arc<dyn MetadataStore>,
    pub producer: Option<Arc<Producer>>,
    pub writer_pool: Option<Arc<streamhouse_storage::writer_pool::WriterPool>>,
    pub object_store: Arc<dyn object_store::ObjectStore>,
    pub segment_cache: Arc<streamhouse_storage::SegmentCache>,
    /// Optional Prometheus client for real metrics (falls back to simulated if None)
    pub prometheus: Option<Arc<PrometheusClient>>,
    /// Authentication configuration
    pub auth_config: AuthConfig,
}

/// Create the API router with all endpoints
pub fn create_router(state: AppState) -> Router {
    let auth_enabled = state.auth_config.enabled;
    let metadata = state.metadata.clone();

    // API v1 routes (original structure)
    let api_routes = Router::new()
        // Topics
        .route(
            "/topics",
            get(handlers::topics::list_topics).post(handlers::topics::create_topic),
        )
        .route(
            "/topics/:name",
            get(handlers::topics::get_topic).delete(handlers::topics::delete_topic),
        )
        .route(
            "/topics/:name/partitions",
            get(handlers::topics::list_partitions),
        )
        .route(
            "/topics/:name/messages",
            get(handlers::topics::get_topic_messages),
        )
        // Agents
        .route("/agents", get(handlers::agents::list_agents))
        .route("/agents/:id", get(handlers::agents::get_agent))
        .route(
            "/agents/:id/metrics",
            get(handlers::metrics::get_agent_metrics),
        )
        // Produce
        .route("/produce", post(handlers::produce::produce))
        .route("/produce/batch", post(handlers::produce::produce_batch))
        // Consume
        .route("/consume", get(handlers::consume::consume))
        // Consumer Groups
        .route(
            "/consumer-groups",
            get(handlers::consumer_groups::list_consumer_groups),
        )
        .route(
            "/consumer-groups/:group_id",
            get(handlers::consumer_groups::get_consumer_group)
                .delete(handlers::consumer_groups::delete_consumer_group),
        )
        .route(
            "/consumer-groups/:group_id/lag",
            get(handlers::consumer_groups::get_consumer_group_lag),
        )
        .route(
            "/consumer-groups/:group_id/reset",
            post(handlers::consumer_groups::reset_offsets),
        )
        .route(
            "/consumer-groups/:group_id/seek",
            post(handlers::consumer_groups::seek_to_timestamp),
        )
        .route(
            "/consumer-groups/commit",
            post(handlers::consumer_groups::commit_offset),
        )
        // Metrics
        .route("/metrics", get(handlers::metrics::get_metrics))
        .route(
            "/metrics/throughput",
            get(handlers::metrics::get_throughput_metrics),
        )
        .route(
            "/metrics/latency",
            get(handlers::metrics::get_latency_metrics),
        )
        .route("/metrics/errors", get(handlers::metrics::get_error_metrics))
        .route(
            "/metrics/storage",
            get(handlers::metrics::get_storage_metrics),
        )
        // SQL
        .route("/sql", post(handlers::sql::execute_sql))
        // AI-powered queries
        .route("/query/ask", post(handlers::ai::ask_query))
        .route("/query/estimate", post(handlers::ai::estimate_cost))
        .route(
            "/query/history",
            get(handlers::ai::get_query_history).delete(handlers::ai::clear_query_history),
        )
        .route(
            "/query/history/:id",
            get(handlers::ai::get_query_by_id).delete(handlers::ai::delete_query),
        )
        .route(
            "/query/history/:id/refine",
            post(handlers::ai::refine_query),
        )
        .route("/ai/health", get(handlers::ai::ai_health))
        // Schema inference
        .route("/schema/infer", post(handlers::ai::infer_schema))
        // Connectors
        .route(
            "/connectors",
            get(handlers::connectors::list_connectors).post(handlers::connectors::create_connector),
        )
        .route(
            "/connectors/:name",
            get(handlers::connectors::get_connector).delete(handlers::connectors::delete_connector),
        )
        .route(
            "/connectors/:name/pause",
            post(handlers::connectors::pause_connector),
        )
        .route(
            "/connectors/:name/resume",
            post(handlers::connectors::resume_connector),
        )
        .with_state(state.clone());

    // Admin routes (always require admin permission)
    let admin_routes = Router::new()
        // Organizations
        .route(
            "/organizations",
            get(handlers::organizations::list_organizations)
                .post(handlers::organizations::create_organization),
        )
        .route(
            "/organizations/:id",
            get(handlers::organizations::get_organization)
                .patch(handlers::organizations::update_organization)
                .delete(handlers::organizations::delete_organization),
        )
        .route(
            "/organizations/:id/quota",
            get(handlers::organizations::get_organization_quota),
        )
        .route(
            "/organizations/:id/usage",
            get(handlers::organizations::get_organization_usage),
        )
        // API Keys (under organizations)
        .route(
            "/organizations/:org_id/api-keys",
            get(handlers::api_keys::list_api_keys).post(handlers::api_keys::create_api_key),
        )
        // API Keys (by ID)
        .route(
            "/api-keys/:id",
            get(handlers::api_keys::get_api_key).delete(handlers::api_keys::revoke_api_key),
        )
        .with_state(state.clone());

    // Apply authentication layers based on configuration
    let api_routes = if auth_enabled {
        tracing::info!("🔐 API authentication enabled");
        // Use SmartAuthLayer that determines permissions based on method + path
        Router::new()
            .merge(api_routes.layer(auth::SmartAuthLayer::new(metadata.clone())))
            .merge(admin_routes.layer(AuthLayer::admin(metadata.clone())))
    } else {
        tracing::warn!("⚠️  API authentication DISABLED - all endpoints are public");
        Router::new().merge(api_routes).merge(admin_routes)
    };

    // OpenAPI documentation
    let swagger = SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi());

    // WebSocket routes
    let ws_routes = Router::new()
        .route("/metrics", get(handlers::websocket::metrics_websocket))
        .route("/topics/:name", get(handlers::websocket::topic_websocket))
        .route(
            "/consumers/:id",
            get(handlers::websocket::consumer_websocket),
        )
        .with_state(state.clone());

    // Main router with CORS
    Router::new()
        .nest("/api/v1", api_routes)
        .nest("/ws", ws_routes)
        .merge(swagger)
        .route("/health", get(handlers::metrics::health_check))
        .route("/live", get(handlers::metrics::liveness_check))
        .route("/ready", get(handlers::metrics::readiness_check))
        .with_state(state.clone())
        .layer(CorsLayer::permissive())
}

/// Start the API server (HTTP)
pub async fn serve(router: Router, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("🚀 REST API server listening on {}", addr);
    tracing::info!("   Swagger UI: http://localhost:{}/swagger-ui", port);
    tracing::info!("   Health: http://localhost:{}/health", port);
    tracing::info!("   Liveness: http://localhost:{}/live", port);
    tracing::info!("   Readiness: http://localhost:{}/ready", port);

    axum::serve(listener, router).await?;
    Ok(())
}

/// Start the API server with optional TLS
///
/// If TLS is configured via environment variables, starts an HTTPS server.
/// Otherwise falls back to HTTP.
///
/// # Environment Variables
///
/// - `TLS_ENABLED`: Set to "true" to enable TLS
/// - `TLS_CERT_PATH`: Path to server certificate (required if TLS enabled)
/// - `TLS_KEY_PATH`: Path to server private key (required if TLS enabled)
/// - `TLS_CA_CERT_PATH`: Path to CA cert for mTLS (optional)
pub async fn serve_auto_tls(router: Router, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    match tls::TlsConfig::from_env()? {
        Some(tls_config) => tls::serve_with_tls(router, port, tls_config).await,
        None => serve(router, port).await,
    }
}

/// Start the API server with optional TLS and graceful shutdown
///
/// Combines auto-TLS detection with graceful shutdown support.
/// Handles SIGINT/SIGTERM signals and waits for in-flight requests.
///
/// # Environment Variables
///
/// - `TLS_ENABLED`: Set to "true" to enable TLS
/// - `TLS_CERT_PATH`: Path to server certificate (required if TLS enabled)
/// - `TLS_KEY_PATH`: Path to server private key (required if TLS enabled)
/// - `TLS_CA_CERT_PATH`: Path to CA cert for mTLS (optional)
/// - `SHUTDOWN_TIMEOUT_SECS`: Graceful shutdown timeout (default: 30)
///
/// # Example
///
/// ```ignore
/// let router = create_router(state);
/// serve_auto_tls_with_shutdown(router, 8080).await?;
/// ```
pub async fn serve_auto_tls_with_shutdown(
    router: Router,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let shutdown_config = GracefulShutdown::default();

    match tls::TlsConfig::from_env()? {
        Some(tls_config) => {
            tls::serve_with_tls_and_shutdown(router, port, tls_config, shutdown_config).await
        }
        None => shutdown::serve_with_shutdown(router, port, shutdown_config).await,
    }
}

/// Start the API server with graceful shutdown (HTTP only)
///
/// Handles SIGINT/SIGTERM signals and waits for in-flight requests to complete.
///
/// # Environment Variables
///
/// - `SHUTDOWN_TIMEOUT_SECS`: Maximum time to wait for in-flight requests (default: 30)
///
/// # Example
///
/// ```ignore
/// let router = create_router(state);
/// serve_graceful(router, 8080).await?;
/// ```
pub async fn serve_graceful(router: Router, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    shutdown::serve_with_shutdown(router, port, GracefulShutdown::default()).await
}

/// OpenAPI specification
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::topics::list_topics,
        handlers::topics::create_topic,
        handlers::topics::get_topic,
        handlers::topics::delete_topic,
        handlers::topics::list_partitions,
        handlers::topics::get_topic_messages,
        handlers::agents::list_agents,
        handlers::agents::get_agent,
        handlers::produce::produce,
        handlers::produce::produce_batch,
        handlers::consume::consume,
        handlers::consumer_groups::list_consumer_groups,
        handlers::consumer_groups::get_consumer_group,
        handlers::consumer_groups::get_consumer_group_lag,
        handlers::consumer_groups::commit_offset,
        handlers::consumer_groups::reset_offsets,
        handlers::consumer_groups::seek_to_timestamp,
        handlers::consumer_groups::delete_consumer_group,
        handlers::metrics::get_metrics,
        handlers::metrics::get_storage_metrics,
        handlers::metrics::get_throughput_metrics,
        handlers::metrics::get_latency_metrics,
        handlers::metrics::get_error_metrics,
        handlers::metrics::get_agent_metrics,
        handlers::metrics::health_check,
        handlers::sql::execute_sql,
        handlers::organizations::list_organizations,
        handlers::organizations::get_organization,
        handlers::organizations::create_organization,
        handlers::organizations::update_organization,
        handlers::organizations::delete_organization,
        handlers::organizations::get_organization_quota,
        handlers::organizations::get_organization_usage,
        handlers::api_keys::list_api_keys,
        handlers::api_keys::get_api_key,
        handlers::api_keys::create_api_key,
        handlers::api_keys::revoke_api_key,
        handlers::ai::ask_query,
        handlers::ai::ai_health,
        handlers::ai::get_query_history,
        handlers::ai::get_query_by_id,
        handlers::ai::refine_query,
        handlers::ai::estimate_cost,
        handlers::ai::delete_query,
        handlers::ai::clear_query_history,
        handlers::ai::infer_schema,
    ),
    components(schemas(
        models::Topic,
        models::CreateTopicRequest,
        models::Agent,
        models::Partition,
        models::ProduceRequest,
        models::ProduceResponse,
        models::BatchProduceRequest,
        models::BatchRecord,
        models::BatchProduceResponse,
        models::BatchRecordResult,
        models::ConsumeResponse,
        models::ConsumedRecord,
        models::ConsumerGroupInfo,
        models::ConsumerGroupDetail,
        models::ConsumerGroupLag,
        models::ConsumerOffsetInfo,
        models::MetricsSnapshot,
        models::HealthResponse,
        models::StorageMetricsResponse,
        models::ThroughputMetric,
        models::LatencyMetric,
        models::ErrorMetric,
        models::AgentMetricsResponse,
        models::TimeRangeParams,
        models::MessageQueryParams,
        models::CommitOffsetRequest,
        models::CommitOffsetResponse,
        models::ResetStrategy,
        models::ResetOffsetsRequest,
        models::ResetOffsetsResponse,
        models::ResetOffsetDetail,
        models::SeekToTimestampRequest,
        models::SeekToTimestampResponse,
        models::SeekOffsetDetail,
        models::DeleteConsumerGroupResponse,
        handlers::sql::SqlQueryRequest,
        handlers::sql::SqlQueryResponse,
        handlers::sql::SqlErrorResponse,
        handlers::sql::ColumnInfo,
        handlers::organizations::OrganizationResponse,
        handlers::organizations::CreateOrganizationRequest,
        handlers::organizations::UpdateOrganizationRequest,
        handlers::organizations::OrganizationQuotaResponse,
        handlers::organizations::OrganizationUsageResponse,
        handlers::api_keys::ApiKeyResponse,
        handlers::api_keys::ApiKeyCreatedResponse,
        handlers::api_keys::CreateApiKeyRequest,
        handlers::ai::AskQueryRequest,
        handlers::ai::AskQueryResponse,
        handlers::ai::QueryResults,
        handlers::ai::AskQueryError,
        handlers::ai::RefineQueryRequest,
        handlers::ai::EstimateCostRequest,
        handlers::ai::CostEstimate,
        handlers::ai::QueryHistoryEntry,
        handlers::ai::QueryHistoryResponse,
        handlers::ai::InferSchemaRequest,
        handlers::ai::InferredSchema,
        handlers::ai::InferredField,
        handlers::ai::IndexRecommendation,
    )),
    tags(
        (name = "topics", description = "Topic management"),
        (name = "agents", description = "Agent monitoring"),
        (name = "produce", description = "Message production"),
        (name = "consume", description = "Message consumption"),
        (name = "consumer-groups", description = "Consumer group monitoring"),
        (name = "metrics", description = "Cluster metrics"),
        (name = "health", description = "Health checks"),
        (name = "sql", description = "SQL query engine"),
        (name = "organizations", description = "Organization management for multi-tenancy"),
        (name = "api-keys", description = "API key management for authentication"),
        (name = "ai", description = "AI-powered natural language query generation"),
    ),
    info(
        title = "StreamHouse API",
        version = "0.1.0",
        description = "REST API for StreamHouse - S3-Native Event Streaming",
        contact(
            name = "StreamHouse",
            url = "https://github.com/yourusername/streamhouse"
        )
    )
)]
struct ApiDoc;
