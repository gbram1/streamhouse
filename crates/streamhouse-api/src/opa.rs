//! Open Policy Agent (OPA) Integration for StreamHouse
//!
//! Provides OPA-based policy evaluation that wraps and extends the existing RBAC system.
//! When OPA is configured, authorization decisions are delegated to an external OPA server,
//! with RBAC roles included in the input context. When OPA is not configured, falls through
//! to the standard RBAC system.
//!
//! ## Architecture
//!
//! ```text
//! Request → OpaLayer → OpaRbacManager → OPA Server (if configured)
//!                                      → RbacManager (fallback)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use streamhouse_api::opa::{OpaConfig, OpaClient, OpaRbacManager, OpaLayer};
//! use streamhouse_api::rbac::RbacManager;
//! use std::sync::Arc;
//!
//! let rbac = Arc::new(RbacManager::new());
//! let opa_config = OpaConfig::default();
//! let opa_client = OpaClient::new(opa_config);
//! let manager = Arc::new(OpaRbacManager::new(rbac, Some(opa_client)));
//! let layer = OpaLayer::new(manager);
//! ```

use crate::acl::{AclAction, AclResource};
use crate::rbac::RbacManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the OPA policy engine integration
#[derive(Debug, Clone)]
pub struct OpaConfig {
    /// OPA server URL (e.g., "http://localhost:8181")
    pub url: String,
    /// Policy path for authorization decisions (e.g., "v1/data/streamhouse/authz")
    pub policy_path: String,
    /// HTTP request timeout
    pub timeout: Duration,
    /// If true, allow access when OPA is unreachable or returns an error.
    /// If false, deny access on OPA errors.
    pub fail_open: bool,
    /// Whether OPA integration is enabled
    pub enabled: bool,
}

impl Default for OpaConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8181".to_string(),
            policy_path: "v1/data/streamhouse/authz".to_string(),
            timeout: Duration::from_secs(5),
            fail_open: false,
            enabled: false,
        }
    }
}

// ---------------------------------------------------------------------------
// OPA Input / Response types
// ---------------------------------------------------------------------------

/// Input payload sent to OPA for policy evaluation.
///
/// Serialized as `{"input": <OpaInput>}` when POSTed to OPA.
#[derive(Debug, Clone, Serialize)]
pub struct OpaInput {
    /// The principal requesting access (e.g., "apikey:key_123")
    pub principal: String,
    /// The action being performed (e.g., "read", "write")
    pub action: String,
    /// The type of resource (e.g., "topic", "consumer_group")
    pub resource_type: String,
    /// The name/pattern of the resource (e.g., "orders")
    pub resource_name: String,
    /// RBAC roles assigned to the principal
    pub roles: Vec<String>,
    /// Additional context for policy evaluation
    pub context: HashMap<String, serde_json::Value>,
}

/// Wrapper for the OPA request body: `{"input": <OpaInput>}`
#[derive(Debug, Serialize)]
struct OpaRequest<'a> {
    input: &'a OpaInput,
}

/// Top-level OPA response envelope
#[derive(Debug, Clone, Deserialize)]
pub struct OpaResponse {
    /// The policy evaluation result
    pub result: OpaResult,
}

/// The result object returned by OPA inside the response
#[derive(Debug, Clone, Deserialize)]
pub struct OpaResult {
    /// Whether the action is allowed
    pub allow: bool,
    /// Optional human-readable reason for the decision
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// OPA Decision
// ---------------------------------------------------------------------------

/// Where the authorization decision originated
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpaDecisionSource {
    /// Decision came from OPA
    Opa,
    /// Decision came from local RBAC (OPA not configured)
    Rbac,
    /// OPA failed and fail_open was true — access allowed by default
    FailOpen,
    /// OPA failed and fail_open was false — access denied by default
    FailClosed,
}

impl fmt::Display for OpaDecisionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpaDecisionSource::Opa => write!(f, "opa"),
            OpaDecisionSource::Rbac => write!(f, "rbac"),
            OpaDecisionSource::FailOpen => write!(f, "fail_open"),
            OpaDecisionSource::FailClosed => write!(f, "fail_closed"),
        }
    }
}

/// The final authorization decision with provenance information
#[derive(Debug, Clone)]
pub struct OpaDecision {
    /// Whether the action is allowed
    pub allowed: bool,
    /// Optional human-readable reason
    pub reason: Option<String>,
    /// Where this decision originated
    pub source: OpaDecisionSource,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during OPA policy evaluation
#[derive(Debug)]
pub enum OpaError {
    /// HTTP request to OPA failed
    RequestFailed(String),
    /// Request to OPA timed out
    Timeout,
    /// OPA returned a response that could not be parsed
    InvalidResponse(String),
    /// OPA returned a non-2xx HTTP status
    ServerError(u16, String),
}

impl fmt::Display for OpaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpaError::RequestFailed(msg) => write!(f, "OPA request failed: {}", msg),
            OpaError::Timeout => write!(f, "OPA request timed out"),
            OpaError::InvalidResponse(msg) => write!(f, "Invalid OPA response: {}", msg),
            OpaError::ServerError(status, msg) => {
                write!(f, "OPA server error (HTTP {}): {}", status, msg)
            }
        }
    }
}

impl std::error::Error for OpaError {}

// ---------------------------------------------------------------------------
// OPA Transport (for testability)
// ---------------------------------------------------------------------------

/// Trait abstracting HTTP transport to OPA, enabling mock-based testing.
#[async_trait::async_trait]
pub trait OpaTransport: Send + Sync {
    /// Evaluate a policy by posting the given input.
    async fn evaluate(&self, url: &str, input: &OpaInput) -> Result<OpaDecision, OpaError>;
    /// Check if OPA is healthy.
    async fn health_check(&self, url: &str) -> bool;
}

// ---------------------------------------------------------------------------
// OPA Client (HTTP-based transport)
// ---------------------------------------------------------------------------

/// HTTP client for communicating with an OPA server
pub struct OpaClient {
    config: OpaConfig,
    http: reqwest::Client,
}

impl OpaClient {
    /// Create a new OPA client from the given configuration
    pub fn new(config: OpaConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to build reqwest client");
        Self { config, http }
    }

    /// Evaluate a policy against the OPA server
    pub async fn evaluate(&self, input: &OpaInput) -> Result<OpaDecision, OpaError> {
        let url = format!("{}/{}", self.config.url.trim_end_matches('/'), self.config.policy_path);
        let body = OpaRequest { input };

        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    OpaError::Timeout
                } else {
                    OpaError::RequestFailed(e.to_string())
                }
            })?;

        let status = response.status().as_u16();
        if status < 200 || status >= 300 {
            let body_text = response.text().await.unwrap_or_default();
            return Err(OpaError::ServerError(status, body_text));
        }

        let opa_response: OpaResponse = response
            .json()
            .await
            .map_err(|e| OpaError::InvalidResponse(e.to_string()))?;

        Ok(OpaDecision {
            allowed: opa_response.result.allow,
            reason: opa_response.result.reason,
            source: OpaDecisionSource::Opa,
        })
    }

    /// Check if the OPA server is healthy
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.config.url.trim_end_matches('/'));
        match self.http.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get a reference to the underlying configuration
    pub fn config(&self) -> &OpaConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl OpaTransport for OpaClient {
    async fn evaluate(&self, _url: &str, input: &OpaInput) -> Result<OpaDecision, OpaError> {
        OpaClient::evaluate(self, input).await
    }

    async fn health_check(&self, _url: &str) -> bool {
        OpaClient::health_check(self).await
    }
}

// ---------------------------------------------------------------------------
// OPA + RBAC Combined Manager
// ---------------------------------------------------------------------------

/// Combined authorization manager that integrates OPA with RBAC.
///
/// When OPA is configured and enabled, queries the OPA server for decisions
/// (including RBAC role information in the input). Falls back to the local
/// RBAC system when OPA is not configured or when OPA encounters errors
/// (depending on `fail_open` configuration).
pub struct OpaRbacManager {
    rbac: Arc<RbacManager>,
    transport: Option<Box<dyn OpaTransport>>,
    config: OpaConfig,
}

impl OpaRbacManager {
    /// Create a new combined OPA + RBAC manager.
    ///
    /// If `opa` is `None`, all authorization decisions will be made by RBAC alone.
    pub fn new(rbac: Arc<RbacManager>, opa: Option<OpaClient>) -> Self {
        let config = opa
            .as_ref()
            .map(|c| c.config.clone())
            .unwrap_or_default();
        let transport: Option<Box<dyn OpaTransport>> =
            opa.map(|c| Box::new(c) as Box<dyn OpaTransport>);
        Self {
            rbac,
            transport,
            config,
        }
    }

    /// Create a new combined manager with a custom transport (for testing).
    pub fn with_transport(
        rbac: Arc<RbacManager>,
        transport: Box<dyn OpaTransport>,
        config: OpaConfig,
    ) -> Self {
        Self {
            rbac,
            transport: Some(transport),
            config,
        }
    }

    /// Get a reference to the underlying RBAC manager
    pub fn rbac(&self) -> &RbacManager {
        &self.rbac
    }

    /// Evaluate whether a principal is allowed to perform an action on a resource.
    ///
    /// Decision flow:
    /// 1. If OPA is configured and enabled, query OPA with RBAC roles in the input.
    ///    - On success, return OPA's decision.
    ///    - On error, return allow (fail_open) or deny (fail_closed).
    /// 2. If OPA is not configured, fall through to RBAC.
    pub async fn is_allowed(
        &self,
        principal: &str,
        resource: &AclResource,
        action: AclAction,
        context: HashMap<String, serde_json::Value>,
    ) -> OpaDecision {
        // If OPA is not configured or not enabled, fall through to RBAC
        let transport = match &self.transport {
            Some(t) if self.config.enabled => t,
            _ => {
                let allowed = self.rbac.is_allowed(principal, resource, action).await;
                return OpaDecision {
                    allowed,
                    reason: if allowed {
                        Some("Allowed by RBAC".to_string())
                    } else {
                        Some("Denied by RBAC".to_string())
                    },
                    source: OpaDecisionSource::Rbac,
                };
            }
        };

        // Gather RBAC roles for the principal to include in OPA input
        let role_ids = self.rbac.get_effective_role_ids(principal).await;

        let input = OpaInput {
            principal: principal.to_string(),
            action: action.to_string(),
            resource_type: resource.resource_type().to_string(),
            resource_name: resource.name().to_string(),
            roles: role_ids,
            context,
        };

        let url = format!(
            "{}/{}",
            self.config.url.trim_end_matches('/'),
            self.config.policy_path
        );

        match transport.evaluate(&url, &input).await {
            Ok(decision) => decision,
            Err(e) => {
                warn!(
                    error = %e,
                    principal = principal,
                    fail_open = self.config.fail_open,
                    "OPA evaluation failed, applying fail policy"
                );

                if self.config.fail_open {
                    OpaDecision {
                        allowed: true,
                        reason: Some(format!("OPA error (fail_open): {}", e)),
                        source: OpaDecisionSource::FailOpen,
                    }
                } else {
                    OpaDecision {
                        allowed: false,
                        reason: Some(format!("OPA error (fail_closed): {}", e)),
                        source: OpaDecisionSource::FailClosed,
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tower Middleware
// ---------------------------------------------------------------------------

/// Function type for extracting resources from HTTP requests
type RbacResourceExtractor =
    Arc<dyn Fn(&str, &str) -> Option<(AclResource, AclAction)> + Send + Sync>;

/// Tower layer that enforces OPA + RBAC authorization on HTTP routes.
///
/// Follows the same pattern as `RbacLayer` in `rbac.rs`.
#[derive(Clone)]
pub struct OpaLayer {
    manager: Arc<OpaRbacManager>,
    resource_extractor: RbacResourceExtractor,
}

impl OpaLayer {
    /// Create a new OPA layer with the default resource extractor
    pub fn new(manager: Arc<OpaRbacManager>) -> Self {
        Self {
            manager,
            resource_extractor: Arc::new(default_opa_resource_extractor),
        }
    }

    /// Create with a custom resource extractor
    pub fn with_extractor<F>(manager: Arc<OpaRbacManager>, extractor: F) -> Self
    where
        F: Fn(&str, &str) -> Option<(AclResource, AclAction)> + Send + Sync + 'static,
    {
        Self {
            manager,
            resource_extractor: Arc::new(extractor),
        }
    }
}

/// Default resource extractor for OPA layer (same logic as RBAC)
fn default_opa_resource_extractor(
    method: &str,
    path: &str,
) -> Option<(AclResource, AclAction)> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match parts.as_slice() {
        ["api", "v1", "topics"] => {
            let action = match method {
                "GET" => AclAction::Describe,
                "POST" => AclAction::Create,
                _ => return None,
            };
            Some((AclResource::All, action))
        }
        ["api", "v1", "topics", name] => {
            let action = match method {
                "GET" => AclAction::Describe,
                "DELETE" => AclAction::Delete,
                _ => return None,
            };
            Some((AclResource::Topic(name.to_string()), action))
        }
        ["api", "v1", "topics", name, "messages"] => {
            Some((AclResource::Topic(name.to_string()), AclAction::Read))
        }
        ["api", "v1", "produce"] | ["api", "v1", "produce", "batch"] => {
            Some((AclResource::All, AclAction::Write))
        }
        ["api", "v1", "consume"] => Some((AclResource::All, AclAction::Read)),
        ["api", "v1", "consumer-groups"] => Some((AclResource::All, AclAction::Describe)),
        ["api", "v1", "consumer-groups", group_id] => {
            let action = match method {
                "GET" => AclAction::Describe,
                "DELETE" => AclAction::Delete,
                _ => return None,
            };
            Some((AclResource::ConsumerGroup(group_id.to_string()), action))
        }
        ["api", "v1", "organizations", ..] => Some((AclResource::Cluster, AclAction::Admin)),
        ["api", "v1", "api-keys", ..] => Some((AclResource::Cluster, AclAction::Admin)),
        ["health"] | ["live"] | ["ready"] | ["metrics"] => None,
        _ => None,
    }
}

impl<S> tower::Layer<S> for OpaLayer {
    type Service = OpaService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OpaService {
            inner,
            manager: self.manager.clone(),
            resource_extractor: self.resource_extractor.clone(),
        }
    }
}

/// Tower service that enforces OPA + RBAC authorization
#[derive(Clone)]
pub struct OpaService<S> {
    inner: S,
    manager: Arc<OpaRbacManager>,
    resource_extractor: RbacResourceExtractor,
}

impl<S> tower::Service<axum::extract::Request> for OpaService<S>
where
    S: tower::Service<axum::extract::Request, Response = axum::response::Response>
        + Send
        + Clone
        + 'static,
    S::Future: Send + 'static,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future =
        futures::future::BoxFuture<'static, std::result::Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: axum::extract::Request) -> Self::Future {
        let manager = self.manager.clone();
        let extractor = self.resource_extractor.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let method = request.method().as_str().to_string();
            let path = request.uri().path().to_string();

            // Extract resource and action from request
            let Some((resource, action)) = extractor(&method, &path) else {
                // No authorization needed for this path
                return inner.call(request).await;
            };

            // Get principal from authenticated key extension
            let principal = request
                .extensions()
                .get::<crate::auth::AuthenticatedKey>()
                .map(|key| format!("apikey:{}", key.key_id))
                .unwrap_or_else(|| "anonymous".to_string());

            // Evaluate via OPA + RBAC
            let decision = manager
                .is_allowed(&principal, &resource, action, HashMap::new())
                .await;

            if !decision.allowed {
                warn!(
                    principal = %principal,
                    resource = %resource,
                    action = %action,
                    source = %decision.source,
                    reason = ?decision.reason,
                    "OPA denied access"
                );

                let body = serde_json::json!({
                    "error": format!("Access denied: {} on {} for {}", action, resource, principal),
                    "code": 403,
                    "source": decision.source.to_string(),
                });

                return Ok(axum::response::Response::builder()
                    .status(axum::http::StatusCode::FORBIDDEN)
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap());
            }

            inner.call(request).await
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::RbacManager;

    // -----------------------------------------------------------------------
    // Mock transport for testing
    // -----------------------------------------------------------------------

    /// A mock OPA transport that returns configurable responses
    struct MockOpaTransport {
        /// If Some, the evaluate call returns Ok with this decision
        decision: Option<OpaDecision>,
        /// If Some, the evaluate call returns Err with this error
        error: Option<String>,
    }

    impl MockOpaTransport {
        fn allow() -> Self {
            Self {
                decision: Some(OpaDecision {
                    allowed: true,
                    reason: Some("Allowed by OPA policy".to_string()),
                    source: OpaDecisionSource::Opa,
                }),
                error: None,
            }
        }

        fn deny() -> Self {
            Self {
                decision: Some(OpaDecision {
                    allowed: false,
                    reason: Some("Denied by OPA policy".to_string()),
                    source: OpaDecisionSource::Opa,
                }),
                error: None,
            }
        }

        fn error() -> Self {
            Self {
                decision: None,
                error: Some("connection refused".to_string()),
            }
        }
    }

    #[async_trait::async_trait]
    impl OpaTransport for MockOpaTransport {
        async fn evaluate(&self, _url: &str, _input: &OpaInput) -> Result<OpaDecision, OpaError> {
            if let Some(ref err) = self.error {
                return Err(OpaError::RequestFailed(err.clone()));
            }
            Ok(self.decision.clone().unwrap())
        }

        async fn health_check(&self, _url: &str) -> bool {
            self.error.is_none()
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_opa_config_default() {
        let config = OpaConfig::default();
        assert_eq!(config.url, "http://localhost:8181");
        assert_eq!(config.policy_path, "v1/data/streamhouse/authz");
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert!(!config.fail_open);
        assert!(!config.enabled);
    }

    #[test]
    fn test_opa_input_serialization() {
        let input = OpaInput {
            principal: "apikey:key_123".to_string(),
            action: "read".to_string(),
            resource_type: "topic".to_string(),
            resource_name: "orders".to_string(),
            roles: vec!["builtin:developer".to_string()],
            context: HashMap::new(),
        };

        let request = OpaRequest { input: &input };
        let json = serde_json::to_value(&request).unwrap();

        // OPA expects {"input": {...}}
        assert!(json.get("input").is_some());
        let input_val = json.get("input").unwrap();
        assert_eq!(input_val["principal"], "apikey:key_123");
        assert_eq!(input_val["action"], "read");
        assert_eq!(input_val["resource_type"], "topic");
        assert_eq!(input_val["resource_name"], "orders");
        assert_eq!(input_val["roles"][0], "builtin:developer");
        assert!(input_val["context"].is_object());
    }

    #[test]
    fn test_opa_response_deserialization() {
        // Test allow response
        let json = r#"{"result": {"allow": true, "reason": "Policy matched"}}"#;
        let response: OpaResponse = serde_json::from_str(json).unwrap();
        assert!(response.result.allow);
        assert_eq!(response.result.reason.as_deref(), Some("Policy matched"));

        // Test deny response
        let json = r#"{"result": {"allow": false, "reason": "Insufficient permissions"}}"#;
        let response: OpaResponse = serde_json::from_str(json).unwrap();
        assert!(!response.result.allow);
        assert_eq!(
            response.result.reason.as_deref(),
            Some("Insufficient permissions")
        );

        // Test response without reason
        let json = r#"{"result": {"allow": true}}"#;
        let response: OpaResponse = serde_json::from_str(json).unwrap();
        assert!(response.result.allow);
        assert!(response.result.reason.is_none());
    }

    #[test]
    fn test_opa_decision_source() {
        assert_eq!(OpaDecisionSource::Opa.to_string(), "opa");
        assert_eq!(OpaDecisionSource::Rbac.to_string(), "rbac");
        assert_eq!(OpaDecisionSource::FailOpen.to_string(), "fail_open");
        assert_eq!(OpaDecisionSource::FailClosed.to_string(), "fail_closed");

        // Verify PartialEq works correctly
        assert_eq!(OpaDecisionSource::Opa, OpaDecisionSource::Opa);
        assert_ne!(OpaDecisionSource::Opa, OpaDecisionSource::Rbac);
    }

    #[tokio::test]
    async fn test_opa_rbac_manager_no_opa() {
        let rbac = Arc::new(RbacManager::new());

        // Assign developer role to alice
        rbac.assign_role("user:alice", "builtin:developer", "admin:root")
            .await
            .unwrap();

        // Create manager without OPA
        let manager = OpaRbacManager::new(rbac, None);

        // Should fall through to RBAC — developer can read
        let decision = manager
            .is_allowed(
                "user:alice",
                &AclResource::Topic("orders".to_string()),
                AclAction::Read,
                HashMap::new(),
            )
            .await;
        assert!(decision.allowed);
        assert_eq!(decision.source, OpaDecisionSource::Rbac);

        // No roles → denied by RBAC
        let decision = manager
            .is_allowed(
                "user:nobody",
                &AclResource::Topic("orders".to_string()),
                AclAction::Read,
                HashMap::new(),
            )
            .await;
        assert!(!decision.allowed);
        assert_eq!(decision.source, OpaDecisionSource::Rbac);
    }

    #[tokio::test]
    async fn test_opa_rbac_manager_opa_allow() {
        let rbac = Arc::new(RbacManager::new());
        let config = OpaConfig {
            enabled: true,
            ..Default::default()
        };

        let manager = OpaRbacManager::with_transport(
            rbac,
            Box::new(MockOpaTransport::allow()),
            config,
        );

        let decision = manager
            .is_allowed(
                "user:alice",
                &AclResource::Topic("orders".to_string()),
                AclAction::Read,
                HashMap::new(),
            )
            .await;

        assert!(decision.allowed);
        assert_eq!(decision.source, OpaDecisionSource::Opa);
    }

    #[tokio::test]
    async fn test_opa_rbac_manager_opa_deny() {
        let rbac = Arc::new(RbacManager::new());
        let config = OpaConfig {
            enabled: true,
            ..Default::default()
        };

        let manager = OpaRbacManager::with_transport(
            rbac,
            Box::new(MockOpaTransport::deny()),
            config,
        );

        let decision = manager
            .is_allowed(
                "user:alice",
                &AclResource::Topic("orders".to_string()),
                AclAction::Read,
                HashMap::new(),
            )
            .await;

        assert!(!decision.allowed);
        assert_eq!(decision.source, OpaDecisionSource::Opa);
        assert_eq!(
            decision.reason.as_deref(),
            Some("Denied by OPA policy")
        );
    }

    #[tokio::test]
    async fn test_opa_rbac_manager_fail_open() {
        let rbac = Arc::new(RbacManager::new());
        let config = OpaConfig {
            enabled: true,
            fail_open: true,
            ..Default::default()
        };

        let manager = OpaRbacManager::with_transport(
            rbac,
            Box::new(MockOpaTransport::error()),
            config,
        );

        let decision = manager
            .is_allowed(
                "user:alice",
                &AclResource::Topic("orders".to_string()),
                AclAction::Read,
                HashMap::new(),
            )
            .await;

        // fail_open = true → should allow on error
        assert!(decision.allowed);
        assert_eq!(decision.source, OpaDecisionSource::FailOpen);
    }

    #[tokio::test]
    async fn test_opa_rbac_manager_fail_closed() {
        let rbac = Arc::new(RbacManager::new());
        let config = OpaConfig {
            enabled: true,
            fail_open: false,
            ..Default::default()
        };

        let manager = OpaRbacManager::with_transport(
            rbac,
            Box::new(MockOpaTransport::error()),
            config,
        );

        let decision = manager
            .is_allowed(
                "user:alice",
                &AclResource::Topic("orders".to_string()),
                AclAction::Read,
                HashMap::new(),
            )
            .await;

        // fail_open = false → should deny on error
        assert!(!decision.allowed);
        assert_eq!(decision.source, OpaDecisionSource::FailClosed);
    }
}
