//! Audit Logging for StreamHouse API
//!
//! Provides comprehensive audit logging for all API operations, including:
//! - Who performed the action (API key, organization)
//! - What action was performed (method, path, operation type)
//! - When it happened (timestamps)
//! - Whether it succeeded or failed
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::audit::AuditLayer;
//!
//! let router = Router::new()
//!     .route("/topics", get(list_topics))
//!     .layer(AuditLayer::new());
//! ```
//!
//! ## Log Format
//!
//! Audit logs are emitted as structured tracing events at INFO level:
//!
//! ```text
//! level=INFO target=streamhouse_api::audit
//!   action=API_REQUEST
//!   method=POST
//!   path=/api/v1/topics
//!   api_key_id=key_abc123
//!   org_id=org_xyz789
//!   status=201
//!   duration_ms=45
//!   operation=create_topic
//! ```
//!
//! ## Environment Variables
//!
//! - `AUDIT_LOG_ENABLED`: Enable/disable audit logging (default: true)
//! - `AUDIT_LOG_LEVEL`: Log level for audit events (default: info)
//! - `AUDIT_LOG_INCLUDE_BODY`: Include request body in logs (default: false, security risk)

use axum::{extract::Request, http::Method, response::Response};
use futures::future::BoxFuture;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Layer, Service};

use crate::auth::AuthenticatedKey;

/// Audit log entry for a single API request
#[derive(Debug, Clone)]
pub struct AuditEntry {
    /// Request ID (unique per request)
    pub request_id: u64,
    /// HTTP method
    pub method: String,
    /// Request path
    pub path: String,
    /// Query string (if any)
    pub query: Option<String>,
    /// API key ID (if authenticated)
    pub api_key_id: Option<String>,
    /// Organization ID (if authenticated)
    pub organization_id: Option<String>,
    /// Response status code
    pub status_code: u16,
    /// Request duration in milliseconds
    pub duration_ms: u64,
    /// Operation type (derived from path + method)
    pub operation: String,
    /// Client IP address
    pub client_ip: Option<String>,
    /// User agent
    pub user_agent: Option<String>,
    /// Whether the request was successful
    pub success: bool,
    /// Error message (if any)
    pub error: Option<String>,
    /// Request timestamp (Unix ms)
    pub timestamp: i64,
}

impl AuditEntry {
    /// Log this audit entry
    pub fn log(&self) {
        tracing::info!(
            target: "streamhouse_api::audit",
            request_id = %self.request_id,
            method = %self.method,
            path = %self.path,
            query = ?self.query,
            api_key_id = ?self.api_key_id,
            org_id = ?self.organization_id,
            status = %self.status_code,
            duration_ms = %self.duration_ms,
            operation = %self.operation,
            client_ip = ?self.client_ip,
            user_agent = ?self.user_agent,
            success = %self.success,
            error = ?self.error,
            "AUDIT"
        );
    }
}

/// Derive operation name from HTTP method and path
fn derive_operation(method: &Method, path: &str) -> String {
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // Match common patterns
    match (method, path_parts.as_slice()) {
        // Topics
        (&Method::GET, ["api", "v1", "topics"]) => "list_topics".to_string(),
        (&Method::POST, ["api", "v1", "topics"]) => "create_topic".to_string(),
        (&Method::GET, ["api", "v1", "topics", _name]) => "get_topic".to_string(),
        (&Method::DELETE, ["api", "v1", "topics", _name]) => "delete_topic".to_string(),
        (&Method::GET, ["api", "v1", "topics", _name, "partitions"]) => {
            "list_partitions".to_string()
        }
        (&Method::GET, ["api", "v1", "topics", _name, "messages"]) => "get_messages".to_string(),

        // Produce/Consume
        (&Method::POST, ["api", "v1", "produce"]) => "produce".to_string(),
        (&Method::POST, ["api", "v1", "produce", "batch"]) => "produce_batch".to_string(),
        (&Method::GET, ["api", "v1", "consume"]) => "consume".to_string(),

        // Consumer Groups
        (&Method::GET, ["api", "v1", "consumer-groups"]) => "list_consumer_groups".to_string(),
        (&Method::GET, ["api", "v1", "consumer-groups", _id]) => "get_consumer_group".to_string(),
        (&Method::DELETE, ["api", "v1", "consumer-groups", _id]) => {
            "delete_consumer_group".to_string()
        }
        (&Method::POST, ["api", "v1", "consumer-groups", "commit"]) => "commit_offset".to_string(),
        (&Method::POST, ["api", "v1", "consumer-groups", _id, "reset"]) => {
            "reset_offsets".to_string()
        }
        (&Method::POST, ["api", "v1", "consumer-groups", _id, "seek"]) => {
            "seek_to_timestamp".to_string()
        }

        // Organizations
        (&Method::GET, ["api", "v1", "organizations"]) => "list_organizations".to_string(),
        (&Method::POST, ["api", "v1", "organizations"]) => "create_organization".to_string(),
        (&Method::GET, ["api", "v1", "organizations", _id]) => "get_organization".to_string(),
        (&Method::PATCH, ["api", "v1", "organizations", _id]) => "update_organization".to_string(),
        (&Method::DELETE, ["api", "v1", "organizations", _id]) => "delete_organization".to_string(),

        // API Keys
        (&Method::GET, ["api", "v1", "organizations", _org, "api-keys"]) => {
            "list_api_keys".to_string()
        }
        (&Method::POST, ["api", "v1", "organizations", _org, "api-keys"]) => {
            "create_api_key".to_string()
        }
        (&Method::GET, ["api", "v1", "api-keys", _id]) => "get_api_key".to_string(),
        (&Method::DELETE, ["api", "v1", "api-keys", _id]) => "revoke_api_key".to_string(),

        // SQL
        (&Method::POST, ["api", "v1", "sql"]) => "execute_sql".to_string(),

        // Metrics
        (&Method::GET, ["api", "v1", "metrics"]) => "get_metrics".to_string(),
        (&Method::GET, ["api", "v1", "metrics", metric_type]) => {
            format!("get_{}_metrics", metric_type)
        }

        // Health
        (&Method::GET, ["health"]) => "health_check".to_string(),
        (&Method::GET, ["live"]) => "liveness_check".to_string(),
        (&Method::GET, ["ready"]) => "readiness_check".to_string(),

        // Default
        _ => format!(
            "{}_{}",
            method.as_str().to_lowercase(),
            path_parts.join("_")
        ),
    }
}

/// Request ID generator
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> u64 {
    REQUEST_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Audit logging configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Whether audit logging is enabled
    pub enabled: bool,
    /// Include request body in audit logs (security risk!)
    pub include_body: bool,
    /// Skip logging for these paths (e.g., health checks)
    pub skip_paths: Vec<String>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: std::env::var("AUDIT_LOG_ENABLED")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
            include_body: std::env::var("AUDIT_LOG_INCLUDE_BODY")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            skip_paths: vec![
                "/health".to_string(),
                "/live".to_string(),
                "/ready".to_string(),
            ],
        }
    }
}

/// Audit logging layer for Axum
#[derive(Clone)]
pub struct AuditLayer {
    config: Arc<AuditConfig>,
}

impl AuditLayer {
    /// Create a new audit layer with default configuration
    pub fn new() -> Self {
        Self {
            config: Arc::new(AuditConfig::default()),
        }
    }

    /// Create a new audit layer with custom configuration
    pub fn with_config(config: AuditConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

impl Default for AuditLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for AuditLayer {
    type Service = AuditMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuditMiddleware {
            inner,
            config: self.config.clone(),
        }
    }
}

/// Audit logging middleware
#[derive(Clone)]
pub struct AuditMiddleware<S> {
    inner: S,
    config: Arc<AuditConfig>,
}

impl<S> Service<Request> for AuditMiddleware<S>
where
    S: Service<Request, Response = Response> + Send + Clone + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let config = self.config.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let start = Instant::now();
            let request_id = next_request_id();
            let timestamp = chrono::Utc::now().timestamp_millis();

            // Extract request details
            let method = request.method().clone();
            let path = request.uri().path().to_string();
            let query = request.uri().query().map(|s| s.to_string());

            // Skip logging for certain paths
            if config.skip_paths.iter().any(|p| path.starts_with(p)) {
                return inner.call(request).await;
            }

            // Extract authentication info from extensions (if available)
            let auth_info = request.extensions().get::<AuthenticatedKey>().cloned();

            // Extract client info
            let client_ip = request
                .headers()
                .get("x-forwarded-for")
                .or_else(|| request.headers().get("x-real-ip"))
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let user_agent = request
                .headers()
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // Derive operation name
            let operation = derive_operation(&method, &path);

            // Call the inner service
            let response = inner.call(request).await?;

            // Only log if enabled
            if config.enabled {
                let duration_ms = start.elapsed().as_millis() as u64;
                let status_code = response.status().as_u16();
                let success = response.status().is_success();

                let entry = AuditEntry {
                    request_id,
                    method: method.to_string(),
                    path,
                    query,
                    api_key_id: auth_info.as_ref().map(|a| a.key_id.clone()),
                    organization_id: auth_info.as_ref().map(|a| a.organization_id.clone()),
                    status_code,
                    duration_ms,
                    operation,
                    client_ip,
                    user_agent,
                    success,
                    error: if success {
                        None
                    } else {
                        Some(format!("HTTP {}", status_code))
                    },
                    timestamp,
                };

                entry.log();
            }

            Ok(response)
        })
    }
}

/// Audit event types for specific operations (for more detailed logging)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEventType {
    /// Topic created
    TopicCreated,
    /// Topic deleted
    TopicDeleted,
    /// Messages produced
    MessagesProduced,
    /// Messages consumed
    MessagesConsumed,
    /// API key created
    ApiKeyCreated,
    /// API key revoked
    ApiKeyRevoked,
    /// Organization created
    OrganizationCreated,
    /// Organization deleted
    OrganizationDeleted,
    /// SQL query executed
    SqlExecuted,
    /// Consumer group modified
    ConsumerGroupModified,
    /// Authentication failed
    AuthenticationFailed,
    /// Authorization failed
    AuthorizationFailed,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TopicCreated => "TOPIC_CREATED",
            Self::TopicDeleted => "TOPIC_DELETED",
            Self::MessagesProduced => "MESSAGES_PRODUCED",
            Self::MessagesConsumed => "MESSAGES_CONSUMED",
            Self::ApiKeyCreated => "API_KEY_CREATED",
            Self::ApiKeyRevoked => "API_KEY_REVOKED",
            Self::OrganizationCreated => "ORGANIZATION_CREATED",
            Self::OrganizationDeleted => "ORGANIZATION_DELETED",
            Self::SqlExecuted => "SQL_EXECUTED",
            Self::ConsumerGroupModified => "CONSUMER_GROUP_MODIFIED",
            Self::AuthenticationFailed => "AUTHENTICATION_FAILED",
            Self::AuthorizationFailed => "AUTHORIZATION_FAILED",
        }
    }
}

/// Log a specific audit event with custom details
pub fn log_audit_event(
    event_type: AuditEventType,
    api_key_id: Option<&str>,
    organization_id: Option<&str>,
    details: &str,
) {
    tracing::info!(
        target: "streamhouse_api::audit",
        event = %event_type.as_str(),
        api_key_id = ?api_key_id,
        org_id = ?organization_id,
        details = %details,
        "AUDIT_EVENT"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_operation() {
        assert_eq!(
            derive_operation(&Method::GET, "/api/v1/topics"),
            "list_topics"
        );
        assert_eq!(
            derive_operation(&Method::POST, "/api/v1/topics"),
            "create_topic"
        );
        assert_eq!(
            derive_operation(&Method::GET, "/api/v1/topics/my-topic"),
            "get_topic"
        );
        assert_eq!(
            derive_operation(&Method::DELETE, "/api/v1/topics/my-topic"),
            "delete_topic"
        );
        assert_eq!(
            derive_operation(&Method::POST, "/api/v1/produce"),
            "produce"
        );
        assert_eq!(derive_operation(&Method::GET, "/api/v1/consume"), "consume");
        assert_eq!(
            derive_operation(&Method::POST, "/api/v1/sql"),
            "execute_sql"
        );
        assert_eq!(derive_operation(&Method::GET, "/health"), "health_check");
    }

    #[test]
    fn test_audit_config_default() {
        let config = AuditConfig::default();
        assert!(config.enabled);
        assert!(!config.include_body);
        assert!(config.skip_paths.contains(&"/health".to_string()));
    }

    #[test]
    fn test_request_id_generation() {
        let id1 = next_request_id();
        let id2 = next_request_id();
        assert!(id2 > id1);
    }

    #[test]
    fn test_audit_event_type_as_str() {
        assert_eq!(AuditEventType::TopicCreated.as_str(), "TOPIC_CREATED");
        assert_eq!(
            AuditEventType::AuthenticationFailed.as_str(),
            "AUTHENTICATION_FAILED"
        );
    }
}
