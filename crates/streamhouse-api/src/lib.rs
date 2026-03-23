//! StreamHouse REST API Server
//!
//! HTTP/JSON API for managing StreamHouse clusters via web console and other HTTP clients.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use streamhouse_metadata::MetadataStore;
use tokio::sync::Notify;
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod acl;
pub mod active_active;
pub mod audit;
pub mod audit_store;
pub mod auth;
pub mod cluster;
pub mod compliance;
pub mod discovery;
pub mod failover;
pub mod handlers;
pub mod health_monitor;
pub mod jwt;
pub mod leader;
pub mod models;
pub mod oauth;
pub mod oidc;
pub mod opa;
pub mod pitr;
pub mod prometheus;
pub mod rate_limit;
pub mod rbac;
pub mod replication;
pub mod sasl;
pub mod shutdown;
pub mod tls;

pub use cluster::{ClusterCoordinator, ClusterNode, ClusterTopology, NodeStatus};
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
pub use oidc::OidcJwksAuth;
pub use opa::{
    OpaClient, OpaConfig, OpaDecision, OpaDecisionSource, OpaError, OpaInput, OpaLayer,
    OpaRbacManager, OpaResponse, OpaResult, OpaService,
};
pub use pitr::{
    BackupMetadata, BackupStatus, BaseBackup, PitrConfig, PitrConfigBuilder, PitrError,
    PitrManager, RecoveryResult, RecoveryTarget, SystemSnapshot,
    VerificationResult as PitrVerificationResult, WalEntry, WalOperation, WalSegment, WalStats,
};
pub use prometheus::PrometheusClient;
pub use rbac::{
    DataMasker, DataMaskingPolicy, MaskType, Permission, RbacError, RbacLayer, RbacManager,
    RbacMiddleware, Role, RoleAssignment, ADMIN_ROLE_ID, DEVELOPER_ROLE_ID, OPERATOR_ROLE_ID,
    VIEWER_ROLE_ID,
};
pub use replication::{
    RegionEndpoint, RegionReplicationState, ReplicatedSegment, ReplicationConfig, ReplicationError,
    ReplicationLag, ReplicationManager, ReplicationMode, ReplicationStats, ReplicationStatus,
};
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
    pub agent_router: Option<Arc<streamhouse_client::AgentRouter>>,
    /// Optional WriterPool fallback for tests (when no AgentRouter is available)
    pub writer_pool: Option<Arc<streamhouse_storage::WriterPool>>,
    pub object_store: Arc<dyn object_store::ObjectStore>,
    pub segment_cache: Arc<streamhouse_storage::SegmentCache>,
    /// Optional Prometheus client for real metrics (falls back to simulated if None)
    pub prometheus: Option<Arc<PrometheusClient>>,
    /// Authentication configuration
    pub auth_config: AuthConfig,
    /// OIDC JWT authentication (None = OIDC auth disabled)
    pub oidc_auth: Option<Arc<OidcJwksAuth>>,
    /// Notified when topics are created or deleted so the partition assigner
    /// can immediately discover them.
    pub topic_changed: Option<Arc<Notify>>,
    /// Schema registry for produce-time validation (None = validation disabled)
    pub schema_registry: Option<Arc<streamhouse_schema_registry::SchemaRegistry>>,
    /// Quota enforcer for rate limiting (None = rate limiting disabled)
    pub quota_enforcer: Option<Arc<streamhouse_metadata::QuotaEnforcer<dyn MetadataStore>>>,
    /// BYOC S3 client pool (None = BYOC disabled)
    pub byoc_s3: Option<Arc<streamhouse_storage::byoc::ByocS3ClientPool>>,
}

/// Create the API router with all endpoints
pub fn create_router(state: AppState) -> Router {
    let auth_enabled = state.auth_config.enabled;
    let metadata = state.metadata.clone();
    let oidc_auth = state.oidc_auth.clone();

    // API v1 routes (original structure)
    let api_routes = Router::new()
        // Identity
        .route("/whoami", get(handlers::metrics::whoami))
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
        // Pipeline Targets
        .route(
            "/pipeline-targets",
            get(handlers::pipelines::list_pipeline_targets)
                .post(handlers::pipelines::create_pipeline_target),
        )
        .route(
            "/pipeline-targets/:name",
            get(handlers::pipelines::get_pipeline_target)
                .delete(handlers::pipelines::delete_pipeline_target),
        )
        // Pipelines
        .route(
            "/pipelines",
            get(handlers::pipelines::list_pipelines).post(handlers::pipelines::create_pipeline),
        )
        .route(
            "/pipelines/:name",
            get(handlers::pipelines::get_pipeline)
                .patch(handlers::pipelines::update_pipeline_state)
                .delete(handlers::pipelines::delete_pipeline),
        )
        // Transform validation
        .route(
            "/transforms/validate",
            post(handlers::pipelines::validate_transform),
        )
        .with_state(state.clone());

    // Admin routes (always require admin permission)
    let admin_routes = Router::new()
        // Organizations — resolve must be registered before /:id to avoid path conflict
        .route(
            "/organizations/resolve",
            post(handlers::organizations::resolve_organization),
        )
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

    // Apply authentication and rate limiting layers based on configuration
    let api_routes =
        if auth_enabled {
            if oidc_auth.is_some() {
                tracing::info!("API authentication enabled (API keys + OIDC JWT)");
            } else {
                tracing::info!("API authentication enabled (API keys only)");
            }

            // Build api routes with auth. If quota enforcer is available, add rate limiting
            // Layer order: Auth runs first (sets AuthenticatedKey), then RateLimit checks it
            let api_with_auth = if let Some(ref enforcer) = state.quota_enforcer {
                tracing::info!("REST rate limiting enabled");
                api_routes
                    .layer(rate_limit::RateLimitLayer::new(enforcer.clone()))
                    .layer(auth::SmartAuthLayer::new_with_oidc(
                        metadata.clone(),
                        oidc_auth.clone(),
                    ))
            } else {
                api_routes.layer(auth::SmartAuthLayer::new_with_oidc(
                    metadata.clone(),
                    oidc_auth.clone(),
                ))
            };

            Router::new().merge(api_with_auth).merge(admin_routes.layer(
                AuthLayer::admin_with_oidc(metadata.clone(), oidc_auth.clone()),
            ))
        } else {
            tracing::warn!("API authentication DISABLED - all endpoints are public");
            Router::new().merge(api_routes).merge(admin_routes)
        };

    // Agent routes (internal only — separate /admin nest)
    let agent_routes = Router::new()
        .route("/agents", get(handlers::agents::list_agents))
        .route("/agents/:id", get(handlers::agents::get_agent))
        .route(
            "/agents/:id/metrics",
            get(handlers::metrics::get_agent_metrics),
        )
        .with_state(state.clone());

    let agent_routes = if auth_enabled {
        agent_routes.layer(AuthLayer::admin_with_oidc(
            metadata.clone(),
            oidc_auth.clone(),
        ))
    } else {
        agent_routes
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
        .nest("/admin", agent_routes)
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
        handlers::pipelines::list_pipeline_targets,
        handlers::pipelines::create_pipeline_target,
        handlers::pipelines::get_pipeline_target,
        handlers::pipelines::delete_pipeline_target,
        handlers::pipelines::list_pipelines,
        handlers::pipelines::create_pipeline,
        handlers::pipelines::get_pipeline,
        handlers::pipelines::delete_pipeline,
        handlers::pipelines::update_pipeline_state,
        handlers::pipelines::validate_transform,
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
        models::PipelineTargetResponse,
        models::CreatePipelineTargetRequest,
        models::PipelineResponse,
        models::CreatePipelineRequest,
        models::UpdatePipelineRequest,
        models::ValidateTransformRequest,
        models::ValidateTransformResponse,
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
        (name = "pipelines", description = "Pipeline management"),
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
