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

pub mod handlers;
pub mod models;
pub mod prometheus;

pub use prometheus::PrometheusClient;

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
}

/// Create the API router with all endpoints
pub fn create_router(state: AppState) -> Router {
    // API v1 routes
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
        .route("/agents/:id/metrics", get(handlers::metrics::get_agent_metrics))
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
        .route("/metrics/throughput", get(handlers::metrics::get_throughput_metrics))
        .route("/metrics/latency", get(handlers::metrics::get_latency_metrics))
        .route("/metrics/errors", get(handlers::metrics::get_error_metrics))
        .route("/metrics/storage", get(handlers::metrics::get_storage_metrics))
        // SQL
        .route("/sql", post(handlers::sql::execute_sql))
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
            get(handlers::api_keys::list_api_keys)
                .post(handlers::api_keys::create_api_key),
        )
        // API Keys (by ID)
        .route(
            "/api-keys/:id",
            get(handlers::api_keys::get_api_key)
                .delete(handlers::api_keys::revoke_api_key),
        )
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
        .route("/query/history/:id/refine", post(handlers::ai::refine_query))
        .route("/ai/health", get(handlers::ai::ai_health))
        .with_state(state.clone());

    // OpenAPI documentation
    let swagger = SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi());

    // WebSocket routes
    let ws_routes = Router::new()
        .route("/metrics", get(handlers::websocket::metrics_websocket))
        .route("/topics/:name", get(handlers::websocket::topic_websocket))
        .route("/consumers/:id", get(handlers::websocket::consumer_websocket))
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

/// Start the API server
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
