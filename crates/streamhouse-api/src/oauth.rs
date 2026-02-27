//! OAuth2/OIDC Authentication
//!
//! Provides OAuth2 and OpenID Connect authentication for the StreamHouse API.
//!
//! ## Features
//!
//! - Authorization Code Flow with PKCE
//! - Multiple provider support (Google, GitHub, Okta, custom)
//! - ID Token validation (OIDC)
//! - Token refresh
//! - Session management
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::oauth::{OAuthConfig, OAuthProvider, OAuthService};
//!
//! // Configure provider
//! let config = OAuthConfig::google(
//!     "client_id",
//!     "client_secret",
//!     "https://myapp.com/oauth/callback",
//! );
//!
//! let service = OAuthService::new(config);
//!
//! // Generate authorization URL
//! let (url, state, verifier) = service.authorization_url()?;
//!
//! // Exchange code for tokens
//! let tokens = service.exchange_code(code, verifier).await?;
//!
//! // Validate ID token
//! let claims = service.validate_id_token(&tokens.id_token).await?;
//! ```
//!
//! ## Environment Variables
//!
//! - `OAUTH_PROVIDER`: Provider name (google, github, okta, custom)
//! - `OAUTH_CLIENT_ID`: OAuth client ID
//! - `OAUTH_CLIENT_SECRET`: OAuth client secret
//! - `OAUTH_REDIRECT_URI`: Redirect URI after authentication
//! - `OAUTH_ISSUER`: OIDC issuer URL (for custom providers)
//! - `OAUTH_SCOPES`: Comma-separated scopes (default: openid,email,profile)

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Redirect, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::RwLock;

/// OAuth errors
#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Invalid state parameter")]
    InvalidState,

    #[error("Invalid code verifier")]
    InvalidVerifier,

    #[error("Token exchange failed: {0}")]
    TokenExchange(String),

    #[error("Token validation failed: {0}")]
    TokenValidation(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Missing required claim: {0}")]
    MissingClaim(String),

    #[error("Session not found")]
    SessionNotFound,
}

pub type Result<T> = std::result::Result<T, OAuthError>;

/// Supported OAuth providers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthProvider {
    Google,
    GitHub,
    Okta { domain: String },
    Custom { issuer: String },
}

impl OAuthProvider {
    /// Get the authorization endpoint
    pub fn authorization_endpoint(&self) -> String {
        match self {
            OAuthProvider::Google => "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            OAuthProvider::GitHub => "https://github.com/login/oauth/authorize".to_string(),
            OAuthProvider::Okta { domain } => {
                format!("https://{}/oauth2/default/v1/authorize", domain)
            }
            OAuthProvider::Custom { issuer } => {
                format!("{}/authorize", issuer)
            }
        }
    }

    /// Get the token endpoint
    pub fn token_endpoint(&self) -> String {
        match self {
            OAuthProvider::Google => "https://oauth2.googleapis.com/token".to_string(),
            OAuthProvider::GitHub => "https://github.com/login/oauth/access_token".to_string(),
            OAuthProvider::Okta { domain } => {
                format!("https://{}/oauth2/default/v1/token", domain)
            }
            OAuthProvider::Custom { issuer } => {
                format!("{}/token", issuer)
            }
        }
    }

    /// Get the userinfo endpoint
    pub fn userinfo_endpoint(&self) -> String {
        match self {
            OAuthProvider::Google => "https://openidconnect.googleapis.com/v1/userinfo".to_string(),
            OAuthProvider::GitHub => "https://api.github.com/user".to_string(),
            OAuthProvider::Okta { domain } => {
                format!("https://{}/oauth2/default/v1/userinfo", domain)
            }
            OAuthProvider::Custom { issuer } => {
                format!("{}/userinfo", issuer)
            }
        }
    }

    /// Get the JWKS (JSON Web Key Set) endpoint
    pub fn jwks_endpoint(&self) -> Option<String> {
        match self {
            OAuthProvider::Google => Some("https://www.googleapis.com/oauth2/v3/certs".to_string()),
            OAuthProvider::GitHub => None, // GitHub doesn't support OIDC
            OAuthProvider::Okta { domain } => {
                Some(format!("https://{}/oauth2/default/v1/keys", domain))
            }
            OAuthProvider::Custom { issuer } => Some(format!("{}/.well-known/jwks.json", issuer)),
        }
    }

    /// Get the issuer URL for token validation
    pub fn issuer(&self) -> String {
        match self {
            OAuthProvider::Google => "https://accounts.google.com".to_string(),
            OAuthProvider::GitHub => "https://github.com".to_string(),
            OAuthProvider::Okta { domain } => {
                format!("https://{}/oauth2/default", domain)
            }
            OAuthProvider::Custom { issuer } => issuer.clone(),
        }
    }

    /// Whether this provider supports OIDC
    pub fn supports_oidc(&self) -> bool {
        !matches!(self, OAuthProvider::GitHub)
    }

    /// Default scopes for this provider
    pub fn default_scopes(&self) -> Vec<String> {
        match self {
            OAuthProvider::Google => {
                vec![
                    "openid".to_string(),
                    "email".to_string(),
                    "profile".to_string(),
                ]
            }
            OAuthProvider::GitHub => {
                vec!["read:user".to_string(), "user:email".to_string()]
            }
            OAuthProvider::Okta { .. } | OAuthProvider::Custom { .. } => {
                vec![
                    "openid".to_string(),
                    "email".to_string(),
                    "profile".to_string(),
                ]
            }
        }
    }
}

/// OAuth configuration
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// OAuth provider
    pub provider: OAuthProvider,
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Redirect URI
    pub redirect_uri: String,
    /// Scopes to request
    pub scopes: Vec<String>,
    /// State parameter timeout
    pub state_timeout: Duration,
    /// Access token lifetime (for session)
    pub token_lifetime: Duration,
}

impl OAuthConfig {
    /// Create Google OAuth config
    pub fn google(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OAuthProvider::Google,
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scopes: OAuthProvider::Google.default_scopes(),
            state_timeout: Duration::from_secs(600),
            token_lifetime: Duration::from_secs(3600),
        }
    }

    /// Create GitHub OAuth config
    pub fn github(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OAuthProvider::GitHub,
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scopes: OAuthProvider::GitHub.default_scopes(),
            state_timeout: Duration::from_secs(600),
            token_lifetime: Duration::from_secs(3600),
        }
    }

    /// Create Okta OAuth config
    pub fn okta(domain: &str, client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OAuthProvider::Okta {
                domain: domain.to_string(),
            },
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scopes: OAuthProvider::Okta {
                domain: domain.to_string(),
            }
            .default_scopes(),
            state_timeout: Duration::from_secs(600),
            token_lifetime: Duration::from_secs(3600),
        }
    }

    /// Create custom OIDC provider config
    pub fn custom(issuer: &str, client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OAuthProvider::Custom {
                issuer: issuer.to_string(),
            },
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            state_timeout: Duration::from_secs(600),
            token_lifetime: Duration::from_secs(3600),
        }
    }

    /// Create config from environment variables
    pub fn from_env() -> Result<Option<Self>> {
        let enabled = std::env::var("OAUTH_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        if !enabled {
            return Ok(None);
        }

        let provider = std::env::var("OAUTH_PROVIDER")
            .map_err(|_| OAuthError::Config("OAUTH_PROVIDER not set".to_string()))?;

        let client_id = std::env::var("OAUTH_CLIENT_ID")
            .map_err(|_| OAuthError::Config("OAUTH_CLIENT_ID not set".to_string()))?;

        let client_secret = std::env::var("OAUTH_CLIENT_SECRET")
            .map_err(|_| OAuthError::Config("OAUTH_CLIENT_SECRET not set".to_string()))?;

        let redirect_uri = std::env::var("OAUTH_REDIRECT_URI")
            .map_err(|_| OAuthError::Config("OAUTH_REDIRECT_URI not set".to_string()))?;

        let config = match provider.to_lowercase().as_str() {
            "google" => Self::google(&client_id, &client_secret, &redirect_uri),
            "github" => Self::github(&client_id, &client_secret, &redirect_uri),
            "okta" => {
                let domain = std::env::var("OAUTH_OKTA_DOMAIN")
                    .map_err(|_| OAuthError::Config("OAUTH_OKTA_DOMAIN not set".to_string()))?;
                Self::okta(&domain, &client_id, &client_secret, &redirect_uri)
            }
            "custom" | "oidc" => {
                let issuer = std::env::var("OAUTH_ISSUER")
                    .map_err(|_| OAuthError::Config("OAUTH_ISSUER not set".to_string()))?;
                Self::custom(&issuer, &client_id, &client_secret, &redirect_uri)
            }
            _ => {
                return Err(OAuthError::Config(format!(
                    "Unknown provider: {}",
                    provider
                )))
            }
        };

        // Override scopes if specified
        let config = if let Ok(scopes) = std::env::var("OAUTH_SCOPES") {
            Self {
                scopes: scopes.split(',').map(|s| s.trim().to_string()).collect(),
                ..config
            }
        } else {
            config
        };

        Ok(Some(config))
    }

    /// Set custom scopes
    pub fn scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }
}

/// OAuth tokens returned after exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    /// Access token
    pub access_token: String,
    /// Refresh token (if provided)
    pub refresh_token: Option<String>,
    /// ID token (OIDC only)
    pub id_token: Option<String>,
    /// Token type (usually "Bearer")
    pub token_type: String,
    /// Expiry time (seconds from now)
    pub expires_in: Option<u64>,
    /// Scopes granted
    pub scope: Option<String>,
}

/// OIDC ID token claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer
    pub iss: String,
    /// Subject (user ID)
    pub sub: String,
    /// Audience (client ID)
    pub aud: StringOrArray,
    /// Expiration time
    pub exp: u64,
    /// Issued at
    pub iat: u64,
    /// Auth time (optional)
    pub auth_time: Option<u64>,
    /// Nonce (optional)
    pub nonce: Option<String>,
    /// Email (optional)
    pub email: Option<String>,
    /// Email verified (optional)
    pub email_verified: Option<bool>,
    /// Name (optional)
    pub name: Option<String>,
    /// Picture URL (optional)
    pub picture: Option<String>,
    /// Given name (optional)
    pub given_name: Option<String>,
    /// Family name (optional)
    pub family_name: Option<String>,
}

/// String or array of strings (for audience claim)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray {
    String(String),
    Array(Vec<String>),
}

impl StringOrArray {
    pub fn contains(&self, value: &str) -> bool {
        match self {
            StringOrArray::String(s) => s == value,
            StringOrArray::Array(arr) => arr.contains(&value.to_string()),
        }
    }
}

/// User info from OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// User ID (subject)
    pub sub: String,
    /// Email address
    pub email: Option<String>,
    /// Whether email is verified
    pub email_verified: Option<bool>,
    /// Display name
    pub name: Option<String>,
    /// Profile picture URL
    pub picture: Option<String>,
    /// Given/first name
    pub given_name: Option<String>,
    /// Family/last name
    pub family_name: Option<String>,
    /// Preferred username
    pub preferred_username: Option<String>,
    /// Provider-specific extra data
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// PKCE code verifier and challenge
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// Code verifier (random string)
    pub verifier: String,
    /// Code challenge (SHA256 hash of verifier)
    pub challenge: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge
    pub fn generate() -> Self {
        // Generate 32 random bytes
        let mut verifier_bytes = [0u8; 32];
        getrandom::getrandom(&mut verifier_bytes).expect("Failed to generate random bytes");
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // Compute SHA256 hash and base64url encode
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        let challenge = URL_SAFE_NO_PAD.encode(hash);

        Self {
            verifier,
            challenge,
        }
    }
}

/// Authorization state (stored during OAuth flow)
#[derive(Debug, Clone)]
pub struct AuthState {
    /// State parameter
    pub state: String,
    /// PKCE verifier
    pub verifier: String,
    /// Nonce (for OIDC)
    pub nonce: Option<String>,
    /// Created at
    pub created_at: u64,
    /// Original redirect (where to send user after auth)
    pub redirect_to: Option<String>,
}

/// OAuth session (after successful auth)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSession {
    /// Session ID
    pub session_id: String,
    /// User info
    pub user: UserInfo,
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: Option<String>,
    /// Expiry time (Unix timestamp)
    pub expires_at: u64,
    /// Created at (Unix timestamp)
    pub created_at: u64,
}

/// OAuth service for handling authentication flows
pub struct OAuthService {
    config: OAuthConfig,
    http_client: reqwest::Client,
    /// Pending authorization states
    pending_states: RwLock<HashMap<String, AuthState>>,
    /// Active sessions
    sessions: RwLock<HashMap<String, OAuthSession>>,
}

impl OAuthService {
    /// Create a new OAuth service
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            pending_states: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Generate an authorization URL
    ///
    /// Returns the URL to redirect the user to, along with the state parameter
    pub async fn authorization_url(&self, redirect_to: Option<String>) -> Result<(String, String)> {
        let pkce = PkceChallenge::generate();

        // Generate state parameter
        let mut state_bytes = [0u8; 16];
        getrandom::getrandom(&mut state_bytes).expect("Failed to generate random bytes");
        let state = URL_SAFE_NO_PAD.encode(state_bytes);

        // Generate nonce for OIDC
        let nonce = if self.config.provider.supports_oidc() {
            let mut nonce_bytes = [0u8; 16];
            getrandom::getrandom(&mut nonce_bytes).expect("Failed to generate random bytes");
            Some(URL_SAFE_NO_PAD.encode(nonce_bytes))
        } else {
            None
        };

        // Store state
        let auth_state = AuthState {
            state: state.clone(),
            verifier: pkce.verifier.clone(),
            nonce: nonce.clone(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            redirect_to,
        };

        self.pending_states
            .write()
            .await
            .insert(state.clone(), auth_state);

        // Build authorization URL
        let mut url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&state={}&code_challenge={}&code_challenge_method=S256",
            self.config.provider.authorization_endpoint(),
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&state),
            urlencoding::encode(&pkce.challenge),
        );

        // Add scopes
        if !self.config.scopes.is_empty() {
            url.push_str(&format!(
                "&scope={}",
                urlencoding::encode(&self.config.scopes.join(" "))
            ));
        }

        // Add nonce for OIDC
        if let Some(ref nonce) = nonce {
            url.push_str(&format!("&nonce={}", urlencoding::encode(nonce)));
        }

        Ok((url, state))
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str, state: &str) -> Result<OAuthTokens> {
        // Verify and retrieve state
        let auth_state = {
            let mut states = self.pending_states.write().await;
            states.remove(state).ok_or(OAuthError::InvalidState)?
        };

        // Check state timeout
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now - auth_state.created_at > self.config.state_timeout.as_secs() {
            return Err(OAuthError::InvalidState);
        }

        // Exchange code for tokens
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", &self.config.redirect_uri);
        params.insert("client_id", &self.config.client_id);
        params.insert("client_secret", &self.config.client_secret);
        params.insert("code_verifier", &auth_state.verifier);

        let response = self
            .http_client
            .post(self.config.provider.token_endpoint())
            .header(header::ACCEPT, "application/json")
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(OAuthError::TokenExchange(error));
        }

        let tokens: OAuthTokens = response
            .json()
            .await
            .map_err(|e| OAuthError::TokenExchange(e.to_string()))?;

        Ok(tokens)
    }

    /// Validate an ID token (OIDC)
    pub fn validate_id_token(&self, id_token: &str) -> Result<IdTokenClaims> {
        // For simplicity, we'll do basic validation without fetching JWKS
        // NOTE, you'd want to verify the signature using the provider's public keys

        // Split the token
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return Err(OAuthError::InvalidToken("Invalid JWT format".to_string()));
        }

        // Decode the payload (middle part)
        let payload = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| OAuthError::InvalidToken(format!("Base64 decode error: {}", e)))?;

        let claims: IdTokenClaims = serde_json::from_slice(&payload)
            .map_err(|e| OAuthError::InvalidToken(format!("JSON decode error: {}", e)))?;

        // Validate issuer
        if claims.iss != self.config.provider.issuer() {
            return Err(OAuthError::TokenValidation(format!(
                "Invalid issuer: expected {}, got {}",
                self.config.provider.issuer(),
                claims.iss
            )));
        }

        // Validate audience
        if !claims.aud.contains(&self.config.client_id) {
            return Err(OAuthError::TokenValidation("Invalid audience".to_string()));
        }

        // Validate expiration
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if claims.exp < now {
            return Err(OAuthError::TokenExpired);
        }

        Ok(claims)
    }

    /// Get user info using access token
    pub async fn get_user_info(&self, access_token: &str) -> Result<UserInfo> {
        let response = self
            .http_client
            .get(self.config.provider.userinfo_endpoint())
            .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(OAuthError::Provider(error));
        }

        let user_info: UserInfo = response
            .json()
            .await
            .map_err(|e| OAuthError::Provider(e.to_string()))?;

        Ok(user_info)
    }

    /// Refresh an access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokens> {
        let mut params = HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", refresh_token);
        params.insert("client_id", &self.config.client_id);
        params.insert("client_secret", &self.config.client_secret);

        let response = self
            .http_client
            .post(self.config.provider.token_endpoint())
            .header(header::ACCEPT, "application/json")
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(OAuthError::TokenExchange(error));
        }

        let tokens: OAuthTokens = response
            .json()
            .await
            .map_err(|e| OAuthError::TokenExchange(e.to_string()))?;

        Ok(tokens)
    }

    /// Create a session from tokens
    pub async fn create_session(&self, tokens: &OAuthTokens) -> Result<OAuthSession> {
        // Get user info
        let user = self.get_user_info(&tokens.access_token).await?;

        // Generate session ID
        let mut session_bytes = [0u8; 32];
        getrandom::getrandom(&mut session_bytes).expect("Failed to generate random bytes");
        let session_id = URL_SAFE_NO_PAD.encode(session_bytes);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expires_at = now
            + tokens
                .expires_in
                .unwrap_or(self.config.token_lifetime.as_secs());

        let session = OAuthSession {
            session_id: session_id.clone(),
            user,
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
            expires_at,
            created_at: now,
        };

        // Store session
        self.sessions
            .write()
            .await
            .insert(session_id, session.clone());

        Ok(session)
    }

    /// Get a session by ID
    pub async fn get_session(&self, session_id: &str) -> Option<OAuthSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Validate a session (check if not expired)
    pub async fn validate_session(&self, session_id: &str) -> Result<OAuthSession> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or(OAuthError::SessionNotFound)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if session.expires_at < now {
            // Try to refresh if we have a refresh token
            if let Some(ref refresh_token) = session.refresh_token {
                let new_tokens = self.refresh_token(refresh_token).await?;
                let new_session = self.create_session(&new_tokens).await?;

                // Remove old session
                self.sessions.write().await.remove(session_id);

                return Ok(new_session);
            }

            return Err(OAuthError::TokenExpired);
        }

        Ok(session)
    }

    /// Revoke a session
    pub async fn revoke_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }

    /// Clean up expired states and sessions
    pub async fn cleanup(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Clean up expired states
        {
            let mut states = self.pending_states.write().await;
            states.retain(|_, state| now - state.created_at < self.config.state_timeout.as_secs());
        }

        // Clean up expired sessions
        {
            let mut sessions = self.sessions.write().await;
            sessions.retain(|_, session| session.expires_at > now);
        }
    }

    /// Get the provider
    pub fn provider(&self) -> &OAuthProvider {
        &self.config.provider
    }
}

// ============================================================================
// Axum Handlers and Middleware
// ============================================================================

/// Query parameters for OAuth callback
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// OAuth callback response
#[derive(Debug, Serialize)]
pub struct OAuthCallbackResponse {
    pub session_id: String,
    pub user: UserInfo,
    pub expires_at: u64,
}

/// OAuth error response
#[derive(Debug, Serialize)]
pub struct OAuthErrorResponse {
    pub error: String,
    pub error_description: Option<String>,
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let (status, error) = match &self {
            OAuthError::InvalidState | OAuthError::InvalidVerifier => {
                (StatusCode::BAD_REQUEST, "invalid_request")
            }
            OAuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "token_expired"),
            OAuthError::SessionNotFound => (StatusCode::UNAUTHORIZED, "session_not_found"),
            OAuthError::InvalidToken(_) | OAuthError::TokenValidation(_) => {
                (StatusCode::UNAUTHORIZED, "invalid_token")
            }
            OAuthError::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, "configuration_error"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
        };

        let body = Json(OAuthErrorResponse {
            error: error.to_string(),
            error_description: Some(self.to_string()),
        });

        (status, body).into_response()
    }
}

/// Handler to initiate OAuth flow
pub async fn oauth_authorize(
    State(oauth): State<Arc<OAuthService>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Redirect> {
    let redirect_to = params.get("redirect_to").cloned();
    let (url, _state) = oauth.authorization_url(redirect_to).await?;
    Ok(Redirect::temporary(&url))
}

/// Handler for OAuth callback
pub async fn oauth_callback(
    State(oauth): State<Arc<OAuthService>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<OAuthCallbackResponse>> {
    // Check for error from provider
    if let Some(error) = query.error {
        return Err(OAuthError::Provider(format!(
            "{}: {}",
            error,
            query.error_description.unwrap_or_default()
        )));
    }

    let code = query.code.ok_or(OAuthError::InvalidState)?;
    let state = query.state.ok_or(OAuthError::InvalidState)?;

    // Exchange code for tokens
    let tokens = oauth.exchange_code(&code, &state).await?;

    // Create session
    let session = oauth.create_session(&tokens).await?;

    Ok(Json(OAuthCallbackResponse {
        session_id: session.session_id,
        user: session.user,
        expires_at: session.expires_at,
    }))
}

/// Handler to get current session
pub async fn oauth_session(
    State(oauth): State<Arc<OAuthService>>,
    headers: HeaderMap,
) -> Result<Json<OAuthSession>> {
    let session_id = extract_session_id(&headers)?;
    let session = oauth.validate_session(&session_id).await?;
    Ok(Json(session))
}

/// Handler to revoke session (logout)
pub async fn oauth_logout(
    State(oauth): State<Arc<OAuthService>>,
    headers: HeaderMap,
) -> Result<StatusCode> {
    let session_id = extract_session_id(&headers)?;
    oauth.revoke_session(&session_id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Extract session ID from headers
fn extract_session_id(headers: &HeaderMap) -> Result<String> {
    // Try Authorization header first
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        let auth_str = auth
            .to_str()
            .map_err(|_| OAuthError::InvalidToken("Invalid header".to_string()))?;
        if let Some(token) = auth_str.strip_prefix("Bearer ") {
            return Ok(token.to_string());
        }
    }

    // Try X-Session-ID header
    if let Some(session) = headers.get("X-Session-ID") {
        let session_str = session
            .to_str()
            .map_err(|_| OAuthError::InvalidToken("Invalid header".to_string()))?;
        return Ok(session_str.to_string());
    }

    Err(OAuthError::SessionNotFound)
}

/// Middleware to validate OAuth session
pub async fn oauth_middleware(
    State(oauth): State<Arc<OAuthService>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> std::result::Result<Response, OAuthError> {
    let session_id = extract_session_id(request.headers())?;
    let _session = oauth.validate_session(&session_id).await?;

    // Could add session to request extensions here if needed
    Ok(next.run(request).await)
}

/// Create OAuth router
pub fn oauth_router(oauth: Arc<OAuthService>) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/authorize", get(oauth_authorize))
        .route("/callback", get(oauth_callback))
        .route("/session", get(oauth_session))
        .route("/logout", post(oauth_logout))
        .with_state(oauth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_challenge() {
        let pkce = PkceChallenge::generate();
        assert!(!pkce.verifier.is_empty());
        assert!(!pkce.challenge.is_empty());
        assert_ne!(pkce.verifier, pkce.challenge);

        // Verify challenge is SHA256 of verifier
        let mut hasher = Sha256::new();
        hasher.update(pkce.verifier.as_bytes());
        let hash = hasher.finalize();
        let expected_challenge = URL_SAFE_NO_PAD.encode(hash);
        assert_eq!(pkce.challenge, expected_challenge);
    }

    #[test]
    fn test_google_config() {
        let config = OAuthConfig::google("client123", "secret456", "http://localhost/callback");
        assert_eq!(config.client_id, "client123");
        assert_eq!(config.client_secret, "secret456");
        assert_eq!(config.redirect_uri, "http://localhost/callback");
        assert!(config.scopes.contains(&"openid".to_string()));
        assert!(matches!(config.provider, OAuthProvider::Google));
    }

    #[test]
    fn test_github_config() {
        let config = OAuthConfig::github("client123", "secret456", "http://localhost/callback");
        assert!(config.scopes.contains(&"read:user".to_string()));
        assert!(!config.provider.supports_oidc());
    }

    #[test]
    fn test_okta_config() {
        let config = OAuthConfig::okta(
            "myorg.okta.com",
            "client123",
            "secret456",
            "http://localhost/callback",
        );
        assert!(matches!(config.provider, OAuthProvider::Okta { .. }));
        assert_eq!(
            config.provider.authorization_endpoint(),
            "https://myorg.okta.com/oauth2/default/v1/authorize"
        );
    }

    #[test]
    fn test_provider_endpoints() {
        let google = OAuthProvider::Google;
        assert!(google.authorization_endpoint().contains("google.com"));
        assert!(google.token_endpoint().contains("googleapis.com"));
        assert!(google.jwks_endpoint().is_some());

        let github = OAuthProvider::GitHub;
        assert!(github.authorization_endpoint().contains("github.com"));
        assert!(github.jwks_endpoint().is_none()); // GitHub doesn't support OIDC
    }

    #[test]
    fn test_string_or_array() {
        let single = StringOrArray::String("test".to_string());
        assert!(single.contains("test"));
        assert!(!single.contains("other"));

        let array = StringOrArray::Array(vec!["a".to_string(), "b".to_string()]);
        assert!(array.contains("a"));
        assert!(array.contains("b"));
        assert!(!array.contains("c"));
    }

    #[tokio::test]
    async fn test_authorization_url() {
        let config = OAuthConfig::google("client123", "secret456", "http://localhost/callback");
        let service = OAuthService::new(config);

        let (url, state) = service.authorization_url(None).await.unwrap();

        assert!(url.contains("accounts.google.com"));
        assert!(url.contains("client_id=client123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(!state.is_empty());
    }

    #[test]
    fn test_config_from_env_disabled() {
        std::env::remove_var("OAUTH_ENABLED");
        let config = OAuthConfig::from_env().unwrap();
        assert!(config.is_none());
    }

    #[test]
    fn test_id_token_validation_invalid_format() {
        let config = OAuthConfig::google("client123", "secret456", "http://localhost/callback");
        let service = OAuthService::new(config);

        let result = service.validate_id_token("invalid");
        assert!(matches!(result, Err(OAuthError::InvalidToken(_))));
    }
}
