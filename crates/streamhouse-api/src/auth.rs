//! API Key Authentication Middleware
//!
//! Provides Axum middleware for validating API keys on protected routes.
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::auth::{AuthLayer, RequiredPermission};
//!
//! let protected_routes = Router::new()
//!     .route("/produce", post(produce))
//!     .layer(AuthLayer::new(metadata.clone(), RequiredPermission::Write));
//! ```
//!
//! ## Authorization Header Format
//!
//! API keys should be provided in the Authorization header:
//! - `Authorization: Bearer sk_live_abc123...`
//!
//! ## Permissions
//!
//! - `read`: Can consume messages, view topics
//! - `write`: Can produce messages, create topics
//! - `admin`: Full access including API key management

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use futures::future::BoxFuture;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::task::{Context, Poll};
use streamhouse_metadata::MetadataStore;
use tower::{Layer, Service};

use crate::oidc::{looks_like_jwt, OidcJwksAuth};

/// Required permission level for a route
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredPermission {
    /// Read-only access (consume, list, get)
    Read,
    /// Write access (produce, create, delete)
    Write,
    /// Admin access (API keys, organizations)
    Admin,
    /// No authentication required (health checks, metrics)
    None,
}

impl RequiredPermission {
    /// Check if the given permissions satisfy this requirement
    pub fn is_satisfied_by(&self, permissions: &[String]) -> bool {
        match self {
            RequiredPermission::None => true,
            RequiredPermission::Read => permissions
                .iter()
                .any(|p| p == "read" || p == "write" || p == "admin"),
            RequiredPermission::Write => permissions.iter().any(|p| p == "write" || p == "admin"),
            RequiredPermission::Admin => permissions.iter().any(|p| p == "admin"),
        }
    }
}

/// Authenticated API key information stored in request extensions
#[derive(Debug, Clone)]
pub struct AuthenticatedKey {
    pub key_id: String,
    pub organization_id: String,
    pub permissions: Vec<String>,
    pub scopes: Vec<String>,
}

impl AuthenticatedKey {
    /// Check if this key has access to a specific topic based on scopes
    pub fn can_access_topic(&self, topic: &str) -> bool {
        // Empty scopes = access to all topics
        if self.scopes.is_empty() {
            return true;
        }

        self.scopes.iter().any(|scope| {
            if scope.ends_with('*') {
                // Wildcard pattern: "orders-*" matches "orders-eu", "orders-us", etc.
                let prefix = &scope[..scope.len() - 1];
                topic.starts_with(prefix)
            } else {
                // Exact match
                scope == topic
            }
        })
    }

    /// Check if this key has a specific permission
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions
            .iter()
            .any(|p| p == permission || p == "admin")
    }
}

/// Authentication layer for Axum routes
#[derive(Clone)]
pub struct AuthLayer {
    metadata: Arc<dyn MetadataStore>,
    required_permission: RequiredPermission,
    oidc_auth: Option<Arc<OidcJwksAuth>>,
}

impl AuthLayer {
    /// Create a new authentication layer
    pub fn new(metadata: Arc<dyn MetadataStore>, required_permission: RequiredPermission) -> Self {
        Self {
            metadata,
            required_permission,
            oidc_auth: None,
        }
    }

    /// Create a new authentication layer with OIDC JWT support
    pub fn new_with_oidc(
        metadata: Arc<dyn MetadataStore>,
        required_permission: RequiredPermission,
        oidc_auth: Option<Arc<OidcJwksAuth>>,
    ) -> Self {
        Self {
            metadata,
            required_permission,
            oidc_auth,
        }
    }

    /// Create a layer that requires read permission
    pub fn read(metadata: Arc<dyn MetadataStore>) -> Self {
        Self::new(metadata, RequiredPermission::Read)
    }

    /// Create a layer that requires write permission
    pub fn write(metadata: Arc<dyn MetadataStore>) -> Self {
        Self::new(metadata, RequiredPermission::Write)
    }

    /// Create a layer that requires admin permission
    pub fn admin(metadata: Arc<dyn MetadataStore>) -> Self {
        Self::new(metadata, RequiredPermission::Admin)
    }

    /// Create an admin layer with OIDC JWT support
    pub fn admin_with_oidc(
        metadata: Arc<dyn MetadataStore>,
        oidc_auth: Option<Arc<OidcJwksAuth>>,
    ) -> Self {
        Self::new_with_oidc(metadata, RequiredPermission::Admin, oidc_auth)
    }
}

/// Smart authentication layer that determines required permission based on HTTP method
///
/// - GET requests require read permission
/// - POST, PUT, PATCH, DELETE requests require write permission
#[derive(Clone)]
pub struct SmartAuthLayer {
    metadata: Arc<dyn MetadataStore>,
    oidc_auth: Option<Arc<OidcJwksAuth>>,
}

impl SmartAuthLayer {
    pub fn new(metadata: Arc<dyn MetadataStore>) -> Self {
        Self {
            metadata,
            oidc_auth: None,
        }
    }

    pub fn new_with_oidc(
        metadata: Arc<dyn MetadataStore>,
        oidc_auth: Option<Arc<OidcJwksAuth>>,
    ) -> Self {
        Self {
            metadata,
            oidc_auth,
        }
    }
}

impl<S> Layer<S> for SmartAuthLayer {
    type Service = SmartAuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SmartAuthMiddleware {
            inner,
            metadata: self.metadata.clone(),
            oidc_auth: self.oidc_auth.clone(),
        }
    }
}

/// Smart authentication middleware that checks permissions based on HTTP method
#[derive(Clone)]
pub struct SmartAuthMiddleware<S> {
    inner: S,
    metadata: Arc<dyn MetadataStore>,
    oidc_auth: Option<Arc<OidcJwksAuth>>,
}

impl<S> Service<Request> for SmartAuthMiddleware<S>
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

    fn call(&mut self, mut request: Request) -> Self::Future {
        let metadata = self.metadata.clone();
        let oidc_auth = self.oidc_auth.clone();
        let mut inner = self.inner.clone();

        // Determine required permission based on HTTP method
        let required_permission = match *request.method() {
            axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS => {
                RequiredPermission::Read
            }
            _ => RequiredPermission::Write,
        };

        Box::pin(async move {
            // Extract Authorization header
            let auth_header = request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok());

            let bearer_token = match auth_header {
                Some(header) if header.starts_with("Bearer ") => {
                    &header[7..] // Skip "Bearer "
                }
                _ => {
                    return Ok(AuthError::MissingApiKey.into_response());
                }
            };

            if bearer_token.is_empty() {
                return Ok(AuthError::MissingApiKey.into_response());
            }

            // Try OIDC JWT auth if token looks like a JWT and OIDC is configured
            if let Some(ref oidc) = oidc_auth {
                if looks_like_jwt(bearer_token) {
                    let org_header = request
                        .headers()
                        .get("x-organization-id")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    match authenticate_oidc_jwt(oidc, bearer_token, org_header).await {
                        Ok(auth_key) => {
                            if !required_permission.is_satisfied_by(&auth_key.permissions) {
                                return Ok(AuthError::InsufficientPermissions.into_response());
                            }
                            request.extensions_mut().insert(auth_key);
                            return inner.call(request).await;
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "OIDC JWT validation failed");
                            return Ok(AuthError::InvalidApiKey.into_response());
                        }
                    }
                }
            }

            // Fall through to API key authentication
            let key_hash = hash_api_key(bearer_token);

            let api_key_record = match metadata.validate_api_key(&key_hash).await {
                Ok(Some(key)) => key,
                Ok(None) => {
                    tracing::warn!(
                        key_prefix = %bearer_token.chars().take(16).collect::<String>(),
                        "Invalid API key"
                    );
                    return Ok(AuthError::InvalidApiKey.into_response());
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to validate API key");
                    return Ok(AuthError::InternalError.into_response());
                }
            };

            // Check permissions
            if !required_permission.is_satisfied_by(&api_key_record.permissions) {
                tracing::warn!(
                    key_id = %api_key_record.id,
                    method = %request.method(),
                    required = ?required_permission,
                    actual = ?api_key_record.permissions,
                    "Insufficient permissions"
                );
                return Ok(AuthError::InsufficientPermissions.into_response());
            }

            // Store authenticated key in request extensions
            let authenticated_key = AuthenticatedKey {
                key_id: api_key_record.id.clone(),
                organization_id: api_key_record.organization_id.clone(),
                permissions: api_key_record.permissions.clone(),
                scopes: api_key_record.scopes.clone(),
            };
            request.extensions_mut().insert(authenticated_key);

            // Update last_used_at timestamp (fire and forget)
            let key_id = api_key_record.id.clone();
            let metadata_clone = metadata.clone();
            tokio::spawn(async move {
                if let Err(e) = metadata_clone.touch_api_key(&key_id).await {
                    tracing::debug!(error = %e, key_id = %key_id, "Failed to update key last_used_at");
                }
            });

            inner.call(request).await
        })
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            metadata: self.metadata.clone(),
            required_permission: self.required_permission,
            oidc_auth: self.oidc_auth.clone(),
        }
    }
}

/// Authentication middleware service
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    metadata: Arc<dyn MetadataStore>,
    required_permission: RequiredPermission,
    oidc_auth: Option<Arc<OidcJwksAuth>>,
}

impl<S> Service<Request> for AuthMiddleware<S>
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

    fn call(&mut self, mut request: Request) -> Self::Future {
        // Skip auth for routes that don't require it
        if self.required_permission == RequiredPermission::None {
            let future = self.inner.call(request);
            return Box::pin(future);
        }

        let metadata = self.metadata.clone();
        let oidc_auth = self.oidc_auth.clone();
        let required_permission = self.required_permission;
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract Authorization header
            let auth_header = request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok());

            let bearer_token = match auth_header {
                Some(header) if header.starts_with("Bearer ") => {
                    &header[7..] // Skip "Bearer "
                }
                _ => {
                    return Ok(AuthError::MissingApiKey.into_response());
                }
            };

            if bearer_token.is_empty() {
                return Ok(AuthError::MissingApiKey.into_response());
            }

            // Try OIDC JWT auth if token looks like a JWT and OIDC is configured
            if let Some(ref oidc) = oidc_auth {
                if looks_like_jwt(bearer_token) {
                    let org_header = request
                        .headers()
                        .get("x-organization-id")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    match authenticate_oidc_jwt(oidc, bearer_token, org_header).await {
                        Ok(auth_key) => {
                            if !required_permission.is_satisfied_by(&auth_key.permissions) {
                                return Ok(AuthError::InsufficientPermissions.into_response());
                            }
                            request.extensions_mut().insert(auth_key);
                            return inner.call(request).await;
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "OIDC JWT validation failed");
                            return Ok(AuthError::InvalidApiKey.into_response());
                        }
                    }
                }
            }

            // Fall through to API key authentication
            let key_hash = hash_api_key(bearer_token);

            let api_key_record = match metadata.validate_api_key(&key_hash).await {
                Ok(Some(key)) => key,
                Ok(None) => {
                    tracing::warn!(
                        key_prefix = %bearer_token.chars().take(16).collect::<String>(),
                        "Invalid API key"
                    );
                    return Ok(AuthError::InvalidApiKey.into_response());
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to validate API key");
                    return Ok(AuthError::InternalError.into_response());
                }
            };

            // Check permissions
            if !required_permission.is_satisfied_by(&api_key_record.permissions) {
                tracing::warn!(
                    key_id = %api_key_record.id,
                    required = ?required_permission,
                    actual = ?api_key_record.permissions,
                    "Insufficient permissions"
                );
                return Ok(AuthError::InsufficientPermissions.into_response());
            }

            // Store authenticated key in request extensions
            let authenticated_key = AuthenticatedKey {
                key_id: api_key_record.id.clone(),
                organization_id: api_key_record.organization_id.clone(),
                permissions: api_key_record.permissions.clone(),
                scopes: api_key_record.scopes.clone(),
            };
            request.extensions_mut().insert(authenticated_key);

            // Update last_used_at timestamp (fire and forget)
            let key_id = api_key_record.id.clone();
            let metadata_clone = metadata.clone();
            tokio::spawn(async move {
                if let Err(e) = metadata_clone.touch_api_key(&key_id).await {
                    tracing::debug!(error = %e, key_id = %key_id, "Failed to update key last_used_at");
                }
            });

            inner.call(request).await
        })
    }
}

/// Authenticate a bearer token as an OIDC JWT.
///
/// On success, returns an `AuthenticatedKey` with:
/// - `key_id`: `"oidc:<subject>"` (no DB touch needed)
/// - `organization_id`: from `org_id_header` (set by web UI via `X-Organization-Id`)
/// - `permissions`: `["admin"]` (OIDC users are trusted admins)
/// - `scopes`: empty (full access)
async fn authenticate_oidc_jwt(
    oidc: &OidcJwksAuth,
    token: &str,
    org_id_header: Option<String>,
) -> Result<AuthenticatedKey, String> {
    let claims = oidc
        .validate_token(token)
        .await
        .map_err(|e| format!("{}", e))?;

    // Organization ID: prefer X-Organization-Id header (set by the web UI's OrgProvider),
    // then fall back to the org_id claim in the JWT itself.
    let organization_id = org_id_header
        .filter(|s| !s.is_empty())
        .or(claims.org_id.clone())
        .ok_or_else(|| {
            "Missing organization ID: set X-Organization-Id header or include org_id in JWT"
                .to_string()
        })?;

    tracing::debug!(
        user_id = %claims.sub,
        org_id = %organization_id,
        "OIDC JWT authenticated"
    );

    Ok(AuthenticatedKey {
        key_id: format!("oidc:{}", claims.sub),
        organization_id,
        permissions: vec!["admin".to_string()],
        scopes: vec![], // full access
    })
}

/// Hash an API key using SHA-256
fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Authentication errors
#[derive(Debug)]
pub enum AuthError {
    MissingApiKey,
    InvalidApiKey,
    InsufficientPermissions,
    InternalError,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingApiKey => (
                StatusCode::UNAUTHORIZED,
                "Missing API key. Provide 'Authorization: Bearer sk_live_...' header",
            ),
            AuthError::InvalidApiKey => (StatusCode::UNAUTHORIZED, "Invalid or expired API key"),
            AuthError::InsufficientPermissions => (
                StatusCode::FORBIDDEN,
                "Insufficient permissions for this operation",
            ),
            AuthError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal authentication error",
            ),
        };

        let body = serde_json::json!({
            "error": message,
            "code": status.as_u16()
        });

        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }
}

/// Configuration for authentication behavior
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Whether authentication is enabled (default: true NOTE)
    pub enabled: bool,
    /// Paths that bypass authentication (e.g., health checks)
    pub bypass_paths: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bypass_paths: vec![
                "/health".to_string(),
                "/live".to_string(),
                "/ready".to_string(),
                "/swagger-ui".to_string(),
                "/api-docs".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_permission_satisfaction() {
        let read_perms = vec!["read".to_string()];
        let write_perms = vec!["write".to_string()];
        let admin_perms = vec!["admin".to_string()];
        let read_write_perms = vec!["read".to_string(), "write".to_string()];

        // None is always satisfied
        assert!(RequiredPermission::None.is_satisfied_by(&[]));
        assert!(RequiredPermission::None.is_satisfied_by(&read_perms));

        // Read permission
        assert!(RequiredPermission::Read.is_satisfied_by(&read_perms));
        assert!(RequiredPermission::Read.is_satisfied_by(&write_perms));
        assert!(RequiredPermission::Read.is_satisfied_by(&admin_perms));
        assert!(!RequiredPermission::Read.is_satisfied_by(&[]));

        // Write permission
        assert!(!RequiredPermission::Write.is_satisfied_by(&read_perms));
        assert!(RequiredPermission::Write.is_satisfied_by(&write_perms));
        assert!(RequiredPermission::Write.is_satisfied_by(&admin_perms));
        assert!(RequiredPermission::Write.is_satisfied_by(&read_write_perms));

        // Admin permission
        assert!(!RequiredPermission::Admin.is_satisfied_by(&read_perms));
        assert!(!RequiredPermission::Admin.is_satisfied_by(&write_perms));
        assert!(RequiredPermission::Admin.is_satisfied_by(&admin_perms));
        assert!(!RequiredPermission::Admin.is_satisfied_by(&read_write_perms));
    }

    #[test]
    fn test_topic_scope_matching() {
        let key = AuthenticatedKey {
            key_id: "test".to_string(),
            organization_id: "org".to_string(),
            permissions: vec!["read".to_string()],
            scopes: vec!["orders-*".to_string(), "users".to_string()],
        };

        // Wildcard matches
        assert!(key.can_access_topic("orders-eu"));
        assert!(key.can_access_topic("orders-us"));
        assert!(key.can_access_topic("orders-"));

        // Exact match
        assert!(key.can_access_topic("users"));

        // Non-matching
        assert!(!key.can_access_topic("payments"));
        assert!(!key.can_access_topic("user")); // Not "users"
        assert!(!key.can_access_topic("my-orders-eu")); // Doesn't start with "orders-"
    }

    #[test]
    fn test_empty_scopes_allows_all() {
        let key = AuthenticatedKey {
            key_id: "test".to_string(),
            organization_id: "org".to_string(),
            permissions: vec!["read".to_string()],
            scopes: vec![], // Empty = all topics
        };

        assert!(key.can_access_topic("any-topic"));
        assert!(key.can_access_topic("another-topic"));
    }

    #[test]
    fn test_hash_api_key() {
        let key = "sk_live_abc123";
        let hash1 = hash_api_key(key);
        let hash2 = hash_api_key(key);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex chars
    }
}
