//! JWT Token Authentication for StreamHouse API
//!
//! Provides JWT-based authentication as an alternative to API keys.
//! Supports both symmetric (HMAC) and asymmetric (RSA, ECDSA) algorithms.
//!
//! ## Usage
//!
//! ### Generate a JWT Token
//! ```ignore
//! use streamhouse_api::jwt::{JwtConfig, JwtService};
//!
//! let config = JwtConfig::from_env()?;
//! let service = JwtService::new(config)?;
//!
//! let token = service.generate_token(
//!     "user_123",
//!     "org_456",
//!     vec!["read", "write"],
//!     None, // Use default expiry
//! )?;
//! ```
//!
//! ### Use JWT Middleware
//! ```ignore
//! use streamhouse_api::jwt::JwtLayer;
//!
//! let protected_routes = Router::new()
//!     .route("/produce", post(produce))
//!     .layer(JwtLayer::new(jwt_config)?);
//! ```
//!
//! ## Authorization Header Format
//!
//! JWT tokens should be provided in the Authorization header:
//! - `Authorization: Bearer eyJhbGciOiJIUzI1NiIs...`
//!
//! ## Environment Variables
//!
//! - `JWT_SECRET`: Secret key for HMAC algorithms (HS256, HS384, HS512)
//! - `JWT_ALGORITHM`: Algorithm to use (default: HS256)
//! - `JWT_ISSUER`: Token issuer (default: streamhouse)
//! - `JWT_AUDIENCE`: Token audience (default: streamhouse-api)
//! - `JWT_EXPIRY_SECS`: Token expiry in seconds (default: 3600 = 1 hour)
//! - `JWT_PUBLIC_KEY_PATH`: Path to public key for RSA/ECDSA verification
//! - `JWT_PRIVATE_KEY_PATH`: Path to private key for RSA/ECDSA signing

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use futures::future::BoxFuture;
use jsonwebtoken::{
    decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tower::{Layer, Service};

use crate::auth::{AuthenticatedKey, RequiredPermission};

/// JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID or service account ID)
    pub sub: String,
    /// Organization ID
    pub org: String,
    /// Permissions (read, write, admin)
    pub permissions: Vec<String>,
    /// Topic scopes (optional, empty = all topics)
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Issued at (Unix timestamp)
    pub iat: u64,
    /// Expiration time (Unix timestamp)
    pub exp: u64,
    /// Token issuer
    pub iss: String,
    /// Token audience
    pub aud: String,
    /// JWT ID (unique identifier for this token)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
}

impl Claims {
    /// Create new claims with the given parameters
    pub fn new(
        subject: impl Into<String>,
        organization: impl Into<String>,
        permissions: Vec<String>,
        scopes: Vec<String>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        expiry_secs: u64,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            sub: subject.into(),
            org: organization.into(),
            permissions,
            scopes,
            iat: now,
            exp: now + expiry_secs,
            iss: issuer.into(),
            aud: audience.into(),
            jti: Some(uuid::Uuid::new_v4().to_string()),
        }
    }

    /// Check if the token has expired
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.exp < now
    }

    /// Convert claims to AuthenticatedKey for compatibility with existing auth system
    pub fn to_authenticated_key(&self) -> AuthenticatedKey {
        AuthenticatedKey {
            key_id: self.jti.clone().unwrap_or_else(|| self.sub.clone()),
            organization_id: self.org.clone(),
            permissions: self.permissions.clone(),
            scopes: self.scopes.clone(),
        }
    }
}

/// JWT configuration
#[derive(Clone)]
pub struct JwtConfig {
    /// Algorithm to use for signing/verification
    pub algorithm: Algorithm,
    /// Encoding key (for signing)
    pub encoding_key: Option<EncodingKey>,
    /// Decoding key (for verification)
    pub decoding_key: DecodingKey,
    /// Token issuer
    pub issuer: String,
    /// Token audience
    pub audience: String,
    /// Default token expiry
    pub default_expiry: Duration,
}

impl std::fmt::Debug for JwtConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtConfig")
            .field("algorithm", &self.algorithm)
            .field(
                "encoding_key",
                &self.encoding_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("decoding_key", &"[REDACTED]")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("default_expiry", &self.default_expiry)
            .finish()
    }
}

impl JwtConfig {
    /// Create configuration from a secret (for HMAC algorithms)
    pub fn from_secret(secret: &[u8]) -> Result<Self, JwtError> {
        if secret.len() < 32 {
            return Err(JwtError::ConfigError(
                "JWT secret must be at least 32 bytes".to_string(),
            ));
        }

        Ok(Self {
            algorithm: Algorithm::HS256,
            encoding_key: Some(EncodingKey::from_secret(secret)),
            decoding_key: DecodingKey::from_secret(secret),
            issuer: "streamhouse".to_string(),
            audience: "streamhouse-api".to_string(),
            default_expiry: Duration::from_secs(3600),
        })
    }

    /// Create configuration from RSA keys
    pub fn from_rsa(
        private_key_pem: Option<&[u8]>,
        public_key_pem: &[u8],
    ) -> Result<Self, JwtError> {
        let encoding_key = private_key_pem
            .map(EncodingKey::from_rsa_pem)
            .transpose()
            .map_err(|e| JwtError::ConfigError(format!("Invalid RSA private key: {}", e)))?;

        let decoding_key = DecodingKey::from_rsa_pem(public_key_pem)
            .map_err(|e| JwtError::ConfigError(format!("Invalid RSA public key: {}", e)))?;

        Ok(Self {
            algorithm: Algorithm::RS256,
            encoding_key,
            decoding_key,
            issuer: "streamhouse".to_string(),
            audience: "streamhouse-api".to_string(),
            default_expiry: Duration::from_secs(3600),
        })
    }

    /// Create configuration from ECDSA keys
    pub fn from_ecdsa(
        private_key_pem: Option<&[u8]>,
        public_key_pem: &[u8],
    ) -> Result<Self, JwtError> {
        let encoding_key = private_key_pem
            .map(EncodingKey::from_ec_pem)
            .transpose()
            .map_err(|e| JwtError::ConfigError(format!("Invalid ECDSA private key: {}", e)))?;

        let decoding_key = DecodingKey::from_ec_pem(public_key_pem)
            .map_err(|e| JwtError::ConfigError(format!("Invalid ECDSA public key: {}", e)))?;

        Ok(Self {
            algorithm: Algorithm::ES256,
            encoding_key,
            decoding_key,
            issuer: "streamhouse".to_string(),
            audience: "streamhouse-api".to_string(),
            default_expiry: Duration::from_secs(3600),
        })
    }

    /// Create configuration from environment variables
    pub fn from_env() -> Result<Option<Self>, JwtError> {
        // Check if JWT is enabled
        let enabled = std::env::var("JWT_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        if !enabled {
            return Ok(None);
        }

        let algorithm = std::env::var("JWT_ALGORITHM").unwrap_or_else(|_| "HS256".to_string());

        let config = match algorithm.to_uppercase().as_str() {
            "HS256" | "HS384" | "HS512" => {
                let secret = std::env::var("JWT_SECRET")
                    .map_err(|_| JwtError::ConfigError("JWT_SECRET not set".to_string()))?;
                Self::from_secret(secret.as_bytes())?
            }
            "RS256" | "RS384" | "RS512" => {
                let public_key_path = std::env::var("JWT_PUBLIC_KEY_PATH").map_err(|_| {
                    JwtError::ConfigError("JWT_PUBLIC_KEY_PATH not set".to_string())
                })?;
                let private_key_path = std::env::var("JWT_PRIVATE_KEY_PATH").ok();

                let public_key = std::fs::read(&public_key_path).map_err(|e| {
                    JwtError::ConfigError(format!("Failed to read public key: {}", e))
                })?;
                let private_key = private_key_path
                    .map(|path| std::fs::read(&path))
                    .transpose()
                    .map_err(|e| {
                        JwtError::ConfigError(format!("Failed to read private key: {}", e))
                    })?;

                let mut cfg = Self::from_rsa(private_key.as_deref(), &public_key)?;
                cfg.algorithm = match algorithm.to_uppercase().as_str() {
                    "RS384" => Algorithm::RS384,
                    "RS512" => Algorithm::RS512,
                    _ => Algorithm::RS256,
                };
                cfg
            }
            "ES256" | "ES384" => {
                let public_key_path = std::env::var("JWT_PUBLIC_KEY_PATH").map_err(|_| {
                    JwtError::ConfigError("JWT_PUBLIC_KEY_PATH not set".to_string())
                })?;
                let private_key_path = std::env::var("JWT_PRIVATE_KEY_PATH").ok();

                let public_key = std::fs::read(&public_key_path).map_err(|e| {
                    JwtError::ConfigError(format!("Failed to read public key: {}", e))
                })?;
                let private_key = private_key_path
                    .map(|path| std::fs::read(&path))
                    .transpose()
                    .map_err(|e| {
                        JwtError::ConfigError(format!("Failed to read private key: {}", e))
                    })?;

                let mut cfg = Self::from_ecdsa(private_key.as_deref(), &public_key)?;
                cfg.algorithm = if algorithm.to_uppercase() == "ES384" {
                    Algorithm::ES384
                } else {
                    Algorithm::ES256
                };
                cfg
            }
            _ => {
                return Err(JwtError::ConfigError(format!(
                    "Unsupported algorithm: {}",
                    algorithm
                )))
            }
        };

        // Override with environment variables
        let issuer = std::env::var("JWT_ISSUER").unwrap_or_else(|_| config.issuer.clone());
        let audience = std::env::var("JWT_AUDIENCE").unwrap_or_else(|_| config.audience.clone());
        let expiry_secs = std::env::var("JWT_EXPIRY_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(config.default_expiry.as_secs());

        Ok(Some(Self {
            issuer,
            audience,
            default_expiry: Duration::from_secs(expiry_secs),
            ..config
        }))
    }

    /// Set the issuer
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = issuer.into();
        self
    }

    /// Set the audience
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = audience.into();
        self
    }

    /// Set the default token expiry
    pub fn with_expiry(mut self, expiry: Duration) -> Self {
        self.default_expiry = expiry;
        self
    }

    /// Check if this config can sign tokens
    pub fn can_sign(&self) -> bool {
        self.encoding_key.is_some()
    }
}

/// JWT service for token generation and validation
#[derive(Clone)]
pub struct JwtService {
    config: Arc<JwtConfig>,
}

impl JwtService {
    /// Create a new JWT service
    pub fn new(config: JwtConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Generate a JWT token
    pub fn generate_token(
        &self,
        subject: impl Into<String>,
        organization: impl Into<String>,
        permissions: Vec<String>,
        expiry: Option<Duration>,
    ) -> Result<String, JwtError> {
        self.generate_token_with_scopes(subject, organization, permissions, vec![], expiry)
    }

    /// Generate a JWT token with topic scopes
    pub fn generate_token_with_scopes(
        &self,
        subject: impl Into<String>,
        organization: impl Into<String>,
        permissions: Vec<String>,
        scopes: Vec<String>,
        expiry: Option<Duration>,
    ) -> Result<String, JwtError> {
        let encoding_key = self
            .config
            .encoding_key
            .as_ref()
            .ok_or(JwtError::ConfigError(
                "No encoding key configured".to_string(),
            ))?;

        let expiry_secs = expiry.unwrap_or(self.config.default_expiry).as_secs();

        let claims = Claims::new(
            subject,
            organization,
            permissions,
            scopes,
            &self.config.issuer,
            &self.config.audience,
            expiry_secs,
        );

        let header = Header::new(self.config.algorithm);
        encode(&header, &claims, encoding_key).map_err(|e| JwtError::TokenGeneration(e.to_string()))
    }

    /// Validate and decode a JWT token
    pub fn validate_token(&self, token: &str) -> Result<Claims, JwtError> {
        let mut validation = Validation::new(self.config.algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.validate_exp = true;
        validation.leeway = 0; // No leeway for expiry checking

        let token_data: TokenData<Claims> = decode(token, &self.config.decoding_key, &validation)
            .map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => JwtError::TokenExpired,
            jsonwebtoken::errors::ErrorKind::InvalidToken => {
                JwtError::InvalidToken("Malformed token".to_string())
            }
            jsonwebtoken::errors::ErrorKind::InvalidSignature => JwtError::InvalidSignature,
            jsonwebtoken::errors::ErrorKind::InvalidIssuer => JwtError::InvalidIssuer,
            jsonwebtoken::errors::ErrorKind::InvalidAudience => JwtError::InvalidAudience,
            _ => JwtError::InvalidToken(e.to_string()),
        })?;

        Ok(token_data.claims)
    }

    /// Refresh a token (generate a new token with the same claims but new expiry)
    pub fn refresh_token(&self, token: &str, expiry: Option<Duration>) -> Result<String, JwtError> {
        let claims = self.validate_token(token)?;

        self.generate_token_with_scopes(
            claims.sub,
            claims.org,
            claims.permissions,
            claims.scopes,
            expiry,
        )
    }
}

/// JWT authentication errors
#[derive(Debug, Error)]
pub enum JwtError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Token generation failed: {0}")]
    TokenGeneration(String),

    #[error("Token has expired")]
    TokenExpired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Invalid token signature")]
    InvalidSignature,

    #[error("Invalid token issuer")]
    InvalidIssuer,

    #[error("Invalid token audience")]
    InvalidAudience,

    #[error("Insufficient permissions")]
    InsufficientPermissions,
}

impl IntoResponse for JwtError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            JwtError::ConfigError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            JwtError::TokenGeneration(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            JwtError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token has expired".to_string()),
            JwtError::InvalidToken(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            JwtError::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "Invalid token signature".to_string(),
            ),
            JwtError::InvalidIssuer => {
                (StatusCode::UNAUTHORIZED, "Invalid token issuer".to_string())
            }
            JwtError::InvalidAudience => (
                StatusCode::UNAUTHORIZED,
                "Invalid token audience".to_string(),
            ),
            JwtError::InsufficientPermissions => (
                StatusCode::FORBIDDEN,
                "Insufficient permissions".to_string(),
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

/// JWT authentication layer for Axum routes
#[derive(Clone)]
pub struct JwtLayer {
    service: Arc<JwtService>,
    required_permission: RequiredPermission,
}

impl JwtLayer {
    /// Create a new JWT authentication layer
    pub fn new(config: JwtConfig, required_permission: RequiredPermission) -> Self {
        Self {
            service: Arc::new(JwtService::new(config)),
            required_permission,
        }
    }

    /// Create a layer that requires read permission
    pub fn read(config: JwtConfig) -> Self {
        Self::new(config, RequiredPermission::Read)
    }

    /// Create a layer that requires write permission
    pub fn write(config: JwtConfig) -> Self {
        Self::new(config, RequiredPermission::Write)
    }

    /// Create a layer that requires admin permission
    pub fn admin(config: JwtConfig) -> Self {
        Self::new(config, RequiredPermission::Admin)
    }
}

impl<S> Layer<S> for JwtLayer {
    type Service = JwtMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        JwtMiddleware {
            inner,
            service: self.service.clone(),
            required_permission: self.required_permission,
        }
    }
}

/// JWT authentication middleware
#[derive(Clone)]
pub struct JwtMiddleware<S> {
    inner: S,
    service: Arc<JwtService>,
    required_permission: RequiredPermission,
}

impl<S> Service<Request> for JwtMiddleware<S>
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

        let service = self.service.clone();
        let required_permission = self.required_permission;
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract Authorization header
            let auth_header = request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok());

            let token = match auth_header {
                Some(header) if header.starts_with("Bearer ") => {
                    &header[7..] // Skip "Bearer "
                }
                _ => {
                    return Ok(
                        JwtError::InvalidToken("Missing authorization token".to_string())
                            .into_response(),
                    );
                }
            };

            // Validate the token
            let claims = match service.validate_token(token) {
                Ok(claims) => claims,
                Err(e) => {
                    tracing::warn!(error = %e, "JWT validation failed");
                    return Ok(e.into_response());
                }
            };

            // Check permissions
            if !required_permission.is_satisfied_by(&claims.permissions) {
                tracing::warn!(
                    subject = %claims.sub,
                    required = ?required_permission,
                    actual = ?claims.permissions,
                    "Insufficient JWT permissions"
                );
                return Ok(JwtError::InsufficientPermissions.into_response());
            }

            // Store authenticated key in request extensions for compatibility
            let authenticated_key = claims.to_authenticated_key();
            request.extensions_mut().insert(authenticated_key);

            // Continue to the inner service
            inner.call(request).await
        })
    }
}

/// Smart JWT layer that determines permission based on HTTP method
#[derive(Clone)]
pub struct SmartJwtLayer {
    service: Arc<JwtService>,
}

impl SmartJwtLayer {
    pub fn new(config: JwtConfig) -> Self {
        Self {
            service: Arc::new(JwtService::new(config)),
        }
    }
}

impl<S> Layer<S> for SmartJwtLayer {
    type Service = SmartJwtMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SmartJwtMiddleware {
            inner,
            service: self.service.clone(),
        }
    }
}

/// Smart JWT middleware that checks permissions based on HTTP method
#[derive(Clone)]
pub struct SmartJwtMiddleware<S> {
    inner: S,
    service: Arc<JwtService>,
}

impl<S> Service<Request> for SmartJwtMiddleware<S>
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
        let service = self.service.clone();
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

            let token = match auth_header {
                Some(header) if header.starts_with("Bearer ") => {
                    &header[7..] // Skip "Bearer "
                }
                _ => {
                    return Ok(
                        JwtError::InvalidToken("Missing authorization token".to_string())
                            .into_response(),
                    );
                }
            };

            // Validate the token
            let claims = match service.validate_token(token) {
                Ok(claims) => claims,
                Err(e) => {
                    tracing::warn!(error = %e, "JWT validation failed");
                    return Ok(e.into_response());
                }
            };

            // Check permissions
            if !required_permission.is_satisfied_by(&claims.permissions) {
                tracing::warn!(
                    subject = %claims.sub,
                    method = %request.method(),
                    required = ?required_permission,
                    actual = ?claims.permissions,
                    "Insufficient JWT permissions"
                );
                return Ok(JwtError::InsufficientPermissions.into_response());
            }

            // Store authenticated key in request extensions
            let authenticated_key = claims.to_authenticated_key();
            request.extensions_mut().insert(authenticated_key);

            // Continue to the inner service
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        JwtConfig::from_secret(b"super-secret-key-for-testing-at-least-32-bytes-long")
            .unwrap()
            .with_issuer("test")
            .with_audience("test-api")
    }

    #[test]
    fn test_generate_and_validate_token() {
        let service = JwtService::new(test_config());

        let token = service
            .generate_token(
                "user_123",
                "org_456",
                vec!["read".to_string(), "write".to_string()],
                None,
            )
            .unwrap();

        let claims = service.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "user_123");
        assert_eq!(claims.org, "org_456");
        assert_eq!(claims.permissions, vec!["read", "write"]);
        assert_eq!(claims.iss, "test");
        assert_eq!(claims.aud, "test-api");
    }

    #[test]
    fn test_token_with_scopes() {
        let service = JwtService::new(test_config());

        let token = service
            .generate_token_with_scopes(
                "user_123",
                "org_456",
                vec!["read".to_string()],
                vec!["orders-*".to_string(), "users".to_string()],
                None,
            )
            .unwrap();

        let claims = service.validate_token(&token).unwrap();
        assert_eq!(claims.scopes, vec!["orders-*", "users"]);

        let auth_key = claims.to_authenticated_key();
        assert!(auth_key.can_access_topic("orders-eu"));
        assert!(auth_key.can_access_topic("users"));
        assert!(!auth_key.can_access_topic("payments"));
    }

    #[test]
    fn test_expired_token() {
        let service = JwtService::new(test_config());

        // Manually create an expired token by setting exp in the past
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: "user_123".to_string(),
            org: "org_456".to_string(),
            permissions: vec!["read".to_string()],
            scopes: vec![],
            iat: now - 3600, // Issued an hour ago
            exp: now - 10,   // Expired 10 seconds ago (more margin)
            iss: "test".to_string(),
            aud: "test-api".to_string(),
            jti: Some(uuid::Uuid::new_v4().to_string()),
        };

        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
        let encoding_key = jsonwebtoken::EncodingKey::from_secret(
            b"super-secret-key-for-testing-at-least-32-bytes-long",
        );
        let token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

        let result = service.validate_token(&token);
        match result {
            Err(JwtError::TokenExpired) => {} // Expected
            Err(e) => panic!("Expected TokenExpired, got: {:?}", e),
            Ok(_) => panic!("Expected error, but token was valid"),
        }
    }

    #[test]
    fn test_invalid_signature() {
        let service1 = JwtService::new(
            JwtConfig::from_secret(b"super-secret-key-for-testing-at-least-32-bytes-long1")
                .unwrap(),
        );
        let service2 = JwtService::new(
            JwtConfig::from_secret(b"different-secret-key-for-testing-at-least-32-bytes").unwrap(),
        );

        let token = service1
            .generate_token("user_123", "org_456", vec!["read".to_string()], None)
            .unwrap();

        let result = service2.validate_token(&token);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_claims_is_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: "user".to_string(),
            org: "org".to_string(),
            permissions: vec!["read".to_string()],
            scopes: vec![],
            iat: now - 3600, // Issued an hour ago
            exp: now - 1,    // Expired 1 second ago
            iss: "test".to_string(),
            aud: "test".to_string(),
            jti: None,
        };

        assert!(claims.is_expired());
    }

    #[test]
    fn test_claims_not_expired() {
        let claims = Claims::new(
            "user",
            "org",
            vec!["read".to_string()],
            vec![],
            "test",
            "test",
            3600, // 1 hour
        );

        assert!(!claims.is_expired());
    }

    #[test]
    fn test_secret_too_short() {
        let result = JwtConfig::from_secret(b"short");
        assert!(matches!(result, Err(JwtError::ConfigError(_))));
    }

    #[test]
    fn test_refresh_token() {
        let service = JwtService::new(test_config());

        let original_token = service
            .generate_token(
                "user_123",
                "org_456",
                vec!["read".to_string()],
                Some(Duration::from_secs(60)),
            )
            .unwrap();

        let new_token = service
            .refresh_token(&original_token, Some(Duration::from_secs(3600)))
            .unwrap();

        // Tokens should be different (different jti and exp)
        assert_ne!(original_token, new_token);

        // But claims should have same subject and org
        let new_claims = service.validate_token(&new_token).unwrap();
        assert_eq!(new_claims.sub, "user_123");
        assert_eq!(new_claims.org, "org_456");
    }
}
