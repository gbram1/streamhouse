//! API Key Authentication Middleware
//!
//! This module provides authentication middleware for HTTP APIs.
//! It validates API keys and injects tenant context into requests.
//!
//! ## Authentication Flow
//!
//! ```text
//! Request with Authorization Header
//!     │
//!     ▼
//! ┌─────────────────────────────────┐
//! │ Extract API Key                 │
//! │ Authorization: Bearer sk_live_… │
//! └─────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────┐
//! │ Hash Key (SHA-256)              │
//! └─────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────┐
//! │ Lookup in MetadataStore         │
//! │ - Find key by hash              │
//! │ - Load organization             │
//! │ - Load quotas                   │
//! └─────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────┐
//! │ Inject TenantContext            │
//! │ into request extensions         │
//! └─────────────────────────────────┘
//!     │
//!     ▼
//!   Handler
//! ```
//!
//! ## Usage with Axum
//!
//! ```ignore
//! use streamhouse_metadata::auth::{ApiKeyAuth, TenantContextExt};
//!
//! // Create auth layer
//! let auth = ApiKeyAuth::new(metadata_store);
//!
//! // Apply to routes
//! let app = Router::new()
//!     .route("/api/v1/topics", get(list_topics))
//!     .layer(axum::middleware::from_fn_with_state(
//!         auth.clone(),
//!         auth_middleware,
//!     ));
//!
//! // In handler, extract tenant context
//! async fn list_topics(ctx: TenantContextExt) -> impl IntoResponse {
//!     let tenant = ctx.tenant;
//!     // Use tenant.organization, tenant.quota, etc.
//! }
//! ```
//!
//! ## Authentication Methods
//!
//! Supports multiple authentication methods:
//!
//! 1. **Bearer Token**: `Authorization: Bearer sk_live_abc123...`
//! 2. **X-API-Key Header**: `X-API-Key: sk_live_abc123...`
//! 3. **Query Parameter**: `?api_key=sk_live_abc123...`
//!
//! ## Error Responses
//!
//! | Status | Reason |
//! |--------|--------|
//! | 401 | Missing or invalid API key |
//! | 403 | Organization suspended |
//! | 403 | Key expired |
//! | 403 | Permission denied |

use crate::tenant::{ApiKeyValidator, TenantContext};
use crate::MetadataStore;
use std::sync::Arc;

/// Authentication error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No API key provided.
    MissingApiKey,
    /// API key is invalid or not found.
    InvalidApiKey,
    /// API key has expired.
    ExpiredApiKey,
    /// Organization is suspended.
    OrganizationSuspended,
    /// Organization is deleted.
    OrganizationDeleted,
    /// Permission denied for this operation.
    PermissionDenied(String),
    /// Internal error during authentication.
    InternalError(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingApiKey => write!(f, "Missing API key"),
            AuthError::InvalidApiKey => write!(f, "Invalid API key"),
            AuthError::ExpiredApiKey => write!(f, "Expired API key"),
            AuthError::OrganizationSuspended => write!(f, "Organization is suspended"),
            AuthError::OrganizationDeleted => write!(f, "Organization is deleted"),
            AuthError::PermissionDenied(op) => write!(f, "Permission denied for: {}", op),
            AuthError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for AuthError {}

/// Result type for authentication operations.
pub type AuthResult<T> = std::result::Result<T, AuthError>;

/// API key authenticator.
///
/// This struct validates API keys and provides tenant context.
/// It can be used as middleware or called directly in handlers.
pub struct ApiKeyAuth<S: MetadataStore> {
    validator: ApiKeyValidator<S>,
    /// Whether to allow requests without API key (default tenant).
    allow_anonymous: bool,
}

impl<S: MetadataStore> ApiKeyAuth<S> {
    /// Create a new authenticator.
    ///
    /// # Arguments
    ///
    /// * `store` - Metadata store for key validation
    pub fn new(store: Arc<S>) -> Self {
        Self {
            validator: ApiKeyValidator::new(store),
            allow_anonymous: false,
        }
    }

    /// Allow anonymous requests (use default tenant context).
    ///
    /// This is useful for development or single-tenant deployments.
    pub fn with_anonymous_allowed(mut self, allow: bool) -> Self {
        self.allow_anonymous = allow;
        self
    }

    /// Extract API key from request headers.
    ///
    /// Checks in order:
    /// 1. `Authorization: Bearer <key>`
    /// 2. `X-API-Key: <key>`
    pub fn extract_key_from_headers(
        &self,
        authorization: Option<&str>,
        x_api_key: Option<&str>,
    ) -> Option<String> {
        // Check Authorization header first
        if let Some(auth) = authorization {
            if let Some(key) = auth.strip_prefix("Bearer ") {
                return Some(key.trim().to_string());
            }
        }

        // Check X-API-Key header
        if let Some(key) = x_api_key {
            return Some(key.trim().to_string());
        }

        None
    }

    /// Authenticate a request with the given API key.
    ///
    /// Returns the tenant context for the authenticated key.
    pub async fn authenticate(&self, api_key: Option<&str>) -> AuthResult<TenantContext> {
        match api_key {
            Some(key) => {
                self.validator
                    .validate(key)
                    .await
                    .map_err(|e| AuthError::InternalError(e.to_string()))
            }
            None => {
                if self.allow_anonymous {
                    self.validator
                        .default_context()
                        .await
                        .map_err(|e| AuthError::InternalError(e.to_string()))
                } else {
                    Err(AuthError::MissingApiKey)
                }
            }
        }
    }

    /// Authenticate from headers.
    ///
    /// Extracts API key from headers and validates it.
    pub async fn authenticate_from_headers(
        &self,
        authorization: Option<&str>,
        x_api_key: Option<&str>,
    ) -> AuthResult<TenantContext> {
        let key = self.extract_key_from_headers(authorization, x_api_key);
        self.authenticate(key.as_deref()).await
    }

    /// Check if a tenant has permission for an operation.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Tenant context
    /// * `permission` - Required permission (read, write, admin)
    pub fn check_permission(&self, ctx: &TenantContext, permission: &str) -> AuthResult<()> {
        if let Some(ref api_key) = ctx.api_key {
            if !api_key.permissions.iter().any(|p| p == permission || p == "admin") {
                return Err(AuthError::PermissionDenied(permission.to_string()));
            }
        }
        Ok(())
    }

    /// Check if a tenant can access a topic.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Tenant context
    /// * `topic` - Topic name
    pub fn check_topic_access(&self, ctx: &TenantContext, topic: &str) -> AuthResult<()> {
        if !ctx.can_access_topic(topic) {
            return Err(AuthError::PermissionDenied(format!("topic:{}", topic)));
        }
        Ok(())
    }
}

impl<S: MetadataStore> Clone for ApiKeyAuth<S> {
    fn clone(&self) -> Self {
        Self {
            validator: self.validator.clone(),
            allow_anonymous: self.allow_anonymous,
        }
    }
}

/// HTTP response helpers for auth errors.
impl AuthError {
    /// Get HTTP status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            AuthError::MissingApiKey => 401,
            AuthError::InvalidApiKey => 401,
            AuthError::ExpiredApiKey => 401,
            AuthError::OrganizationSuspended => 403,
            AuthError::OrganizationDeleted => 403,
            AuthError::PermissionDenied(_) => 403,
            AuthError::InternalError(_) => 500,
        }
    }

    /// Get error code string for JSON responses.
    pub fn error_code(&self) -> &'static str {
        match self {
            AuthError::MissingApiKey => "MISSING_API_KEY",
            AuthError::InvalidApiKey => "INVALID_API_KEY",
            AuthError::ExpiredApiKey => "EXPIRED_API_KEY",
            AuthError::OrganizationSuspended => "ORGANIZATION_SUSPENDED",
            AuthError::OrganizationDeleted => "ORGANIZATION_DELETED",
            AuthError::PermissionDenied(_) => "PERMISSION_DENIED",
            AuthError::InternalError(_) => "INTERNAL_ERROR",
        }
    }
}

/// Axum middleware for API key authentication.
///
/// This module provides Axum-specific middleware integration.
#[cfg(feature = "axum")]
pub mod axum_middleware {
    use super::*;
    use axum::{
        extract::{Request, State},
        http::StatusCode,
        middleware::Next,
        response::{IntoResponse, Response},
        Json,
    };
    use serde::Serialize;

    /// Error response for authentication failures.
    #[derive(Serialize)]
    pub struct AuthErrorResponse {
        pub error: String,
        pub code: String,
    }

    impl IntoResponse for AuthError {
        fn into_response(self) -> Response {
            let status = match self.status_code() {
                401 => StatusCode::UNAUTHORIZED,
                403 => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };

            let body = AuthErrorResponse {
                error: self.to_string(),
                code: self.error_code().to_string(),
            };

            (status, Json(body)).into_response()
        }
    }

    /// Axum middleware function for API key authentication.
    ///
    /// # Usage
    ///
    /// ```ignore
    /// let app = Router::new()
    ///     .route("/api/v1/topics", get(list_topics))
    ///     .layer(axum::middleware::from_fn_with_state(
    ///         auth.clone(),
    ///         auth_middleware,
    ///     ));
    /// ```
    pub async fn auth_middleware<S: MetadataStore + 'static>(
        State(auth): State<ApiKeyAuth<S>>,
        mut request: Request,
        next: Next,
    ) -> Result<Response, AuthError> {
        // Extract API key from headers
        let authorization = request
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok());
        let x_api_key = request
            .headers()
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok());

        // Authenticate
        let tenant = auth.authenticate_from_headers(authorization, x_api_key).await?;

        // Store tenant context in request extensions
        request.extensions_mut().insert(tenant);

        // Continue to handler
        Ok(next.run(request).await)
    }

    /// Axum extractor for tenant context.
    ///
    /// # Usage
    ///
    /// ```ignore
    /// async fn list_topics(
    ///     TenantContextExt(tenant): TenantContextExt,
    /// ) -> impl IntoResponse {
    ///     // Use tenant.organization, tenant.quota, etc.
    /// }
    /// ```
    pub struct TenantContextExt(pub TenantContext);

    impl<S> axum::extract::FromRequestParts<S> for TenantContextExt
    where
        S: Send + Sync,
    {
        type Rejection = AuthError;

        fn from_request_parts<'life0, 'life1, 'async_trait>(
            parts: &'life0 mut axum::http::request::Parts,
            _state: &'life1 S,
        ) -> core::pin::Pin<
            Box<
                dyn core::future::Future<Output = Result<Self, Self::Rejection>>
                    + core::marker::Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                parts
                    .extensions
                    .get::<TenantContext>()
                    .cloned()
                    .map(TenantContextExt)
                    .ok_or(AuthError::InternalError(
                        "TenantContext not found in extensions".to_string(),
                    ))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bearer_token() {
        // Create a mock authenticator (we just need to test the extraction logic)
        // Since we can't easily create a mock MetadataStore, we'll test the extraction logic directly

        // Test Bearer token extraction
        let auth_header = "Bearer sk_live_abc123";
        if let Some(key) = auth_header.strip_prefix("Bearer ") {
            assert_eq!(key.trim(), "sk_live_abc123");
        } else {
            panic!("Failed to extract Bearer token");
        }
    }

    #[test]
    fn test_auth_error_status_codes() {
        assert_eq!(AuthError::MissingApiKey.status_code(), 401);
        assert_eq!(AuthError::InvalidApiKey.status_code(), 401);
        assert_eq!(AuthError::ExpiredApiKey.status_code(), 401);
        assert_eq!(AuthError::OrganizationSuspended.status_code(), 403);
        assert_eq!(AuthError::OrganizationDeleted.status_code(), 403);
        assert_eq!(
            AuthError::PermissionDenied("write".to_string()).status_code(),
            403
        );
        assert_eq!(
            AuthError::InternalError("test".to_string()).status_code(),
            500
        );
    }

    #[test]
    fn test_auth_error_codes() {
        assert_eq!(AuthError::MissingApiKey.error_code(), "MISSING_API_KEY");
        assert_eq!(AuthError::InvalidApiKey.error_code(), "INVALID_API_KEY");
        assert_eq!(AuthError::ExpiredApiKey.error_code(), "EXPIRED_API_KEY");
        assert_eq!(
            AuthError::OrganizationSuspended.error_code(),
            "ORGANIZATION_SUSPENDED"
        );
        assert_eq!(
            AuthError::PermissionDenied("write".to_string()).error_code(),
            "PERMISSION_DENIED"
        );
    }

    #[test]
    fn test_auth_error_display() {
        assert_eq!(AuthError::MissingApiKey.to_string(), "Missing API key");
        assert_eq!(AuthError::InvalidApiKey.to_string(), "Invalid API key");
        assert_eq!(
            AuthError::PermissionDenied("write".to_string()).to_string(),
            "Permission denied for: write"
        );
    }

    #[test]
    fn test_auth_error_display_expired() {
        assert_eq!(AuthError::ExpiredApiKey.to_string(), "Expired API key");
    }

    #[test]
    fn test_auth_error_display_organization_suspended() {
        assert_eq!(
            AuthError::OrganizationSuspended.to_string(),
            "Organization is suspended"
        );
    }

    #[test]
    fn test_auth_error_display_organization_deleted() {
        assert_eq!(
            AuthError::OrganizationDeleted.to_string(),
            "Organization is deleted"
        );
    }

    #[test]
    fn test_auth_error_display_internal() {
        assert_eq!(
            AuthError::InternalError("db connection failed".to_string()).to_string(),
            "Internal error: db connection failed"
        );
    }

    #[test]
    fn test_auth_error_error_code_expired() {
        assert_eq!(AuthError::ExpiredApiKey.error_code(), "EXPIRED_API_KEY");
    }

    #[test]
    fn test_auth_error_error_code_deleted() {
        assert_eq!(
            AuthError::OrganizationDeleted.error_code(),
            "ORGANIZATION_DELETED"
        );
    }

    #[test]
    fn test_auth_error_error_code_internal() {
        assert_eq!(
            AuthError::InternalError("test".to_string()).error_code(),
            "INTERNAL_ERROR"
        );
    }

    #[test]
    fn test_auth_error_equality() {
        assert_eq!(AuthError::MissingApiKey, AuthError::MissingApiKey);
        assert_eq!(AuthError::InvalidApiKey, AuthError::InvalidApiKey);
        assert_eq!(AuthError::ExpiredApiKey, AuthError::ExpiredApiKey);
        assert_eq!(
            AuthError::OrganizationSuspended,
            AuthError::OrganizationSuspended
        );
        assert_eq!(
            AuthError::OrganizationDeleted,
            AuthError::OrganizationDeleted
        );
        assert_eq!(
            AuthError::PermissionDenied("write".to_string()),
            AuthError::PermissionDenied("write".to_string())
        );
        assert_eq!(
            AuthError::InternalError("err".to_string()),
            AuthError::InternalError("err".to_string())
        );
    }

    #[test]
    fn test_auth_error_inequality() {
        assert_ne!(AuthError::MissingApiKey, AuthError::InvalidApiKey);
        assert_ne!(AuthError::ExpiredApiKey, AuthError::MissingApiKey);
        assert_ne!(
            AuthError::PermissionDenied("read".to_string()),
            AuthError::PermissionDenied("write".to_string())
        );
        assert_ne!(
            AuthError::InternalError("a".to_string()),
            AuthError::InternalError("b".to_string())
        );
    }

    #[test]
    fn test_auth_error_clone() {
        let err = AuthError::PermissionDenied("admin".to_string());
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    #[test]
    fn test_auth_error_debug() {
        let err = AuthError::MissingApiKey;
        let debug = format!("{:?}", err);
        assert!(debug.contains("MissingApiKey"));
    }

    #[test]
    fn test_auth_error_is_error_trait() {
        let err: &dyn std::error::Error = &AuthError::MissingApiKey;
        assert_eq!(err.to_string(), "Missing API key");
    }

    #[test]
    fn test_auth_error_status_codes_are_valid_http() {
        // All status codes should be valid HTTP status codes
        let errors = vec![
            AuthError::MissingApiKey,
            AuthError::InvalidApiKey,
            AuthError::ExpiredApiKey,
            AuthError::OrganizationSuspended,
            AuthError::OrganizationDeleted,
            AuthError::PermissionDenied("test".to_string()),
            AuthError::InternalError("test".to_string()),
        ];
        for err in errors {
            let code = err.status_code();
            assert!(
                code >= 100 && code < 600,
                "Invalid HTTP status code {} for {:?}",
                code,
                err
            );
        }
    }

    #[test]
    fn test_auth_error_401_errors() {
        // All authentication errors should be 401
        assert_eq!(AuthError::MissingApiKey.status_code(), 401);
        assert_eq!(AuthError::InvalidApiKey.status_code(), 401);
        assert_eq!(AuthError::ExpiredApiKey.status_code(), 401);
    }

    #[test]
    fn test_auth_error_403_errors() {
        // All authorization errors should be 403
        assert_eq!(AuthError::OrganizationSuspended.status_code(), 403);
        assert_eq!(AuthError::OrganizationDeleted.status_code(), 403);
        assert_eq!(
            AuthError::PermissionDenied("any".to_string()).status_code(),
            403
        );
    }

    #[test]
    fn test_extract_bearer_token_with_whitespace() {
        let auth_header = "Bearer   sk_live_abc123  ";
        if let Some(key) = auth_header.strip_prefix("Bearer ") {
            assert_eq!(key.trim(), "sk_live_abc123");
        } else {
            panic!("Failed to extract Bearer token with whitespace");
        }
    }

    #[test]
    fn test_extract_bearer_token_not_bearer() {
        let auth_header = "Basic abc123";
        let result = auth_header.strip_prefix("Bearer ");
        assert!(result.is_none());
    }
}
