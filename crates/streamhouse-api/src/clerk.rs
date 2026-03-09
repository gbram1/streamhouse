//! Clerk JWT Authentication
//!
//! Validates Clerk-issued JWTs using JWKS (JSON Web Key Sets) fetched from the
//! Clerk issuer. Enabled when the `CLERK_ISSUER_URL` environment variable is set.
//!
//! JWKS keys are cached for 1 hour and automatically refetched when an unknown
//! `kid` is encountered (handles key rotation).

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const JWKS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// A single JWK from the Clerk JWKS endpoint.
#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    kid: String,
    kty: String,
    n: String,
    e: String,
}

/// JWKS response from Clerk.
#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

/// Cached JWKS with expiry time.
#[derive(Debug, Clone)]
struct CachedJwks {
    keys: Vec<Jwk>,
    fetched_at: Instant,
}

impl CachedJwks {
    fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > JWKS_CACHE_TTL
    }

    fn find_key(&self, kid: &str) -> Option<&Jwk> {
        self.keys.iter().find(|k| k.kid == kid)
    }
}

/// Claims extracted from a Clerk JWT.
#[derive(Debug, Clone, Deserialize)]
pub struct ClerkClaims {
    /// Clerk user ID (e.g., "user_2abc...")
    pub sub: String,
    /// Organization ID from Clerk (if user selected an org)
    #[serde(default)]
    pub org_id: Option<String>,
    /// Organization slug
    #[serde(default)]
    pub org_slug: Option<String>,
    /// Organization role
    #[serde(default)]
    pub org_role: Option<String>,
    /// Issuer URL
    pub iss: String,
    /// Expiry (unix timestamp)
    pub exp: u64,
}

/// Clerk authentication handler.
///
/// Fetches and caches JWKS from the Clerk issuer URL, then validates
/// incoming JWTs against those keys.
pub struct ClerkAuth {
    issuer_url: String,
    jwks_url: String,
    client: reqwest::Client,
    cache: RwLock<Option<CachedJwks>>,
}

impl ClerkAuth {
    /// Create a new ClerkAuth from an issuer URL.
    pub fn new(issuer_url: String) -> Self {
        let jwks_url = format!("{}/.well-known/jwks.json", issuer_url.trim_end_matches('/'));
        Self {
            issuer_url,
            jwks_url,
            client: reqwest::Client::new(),
            cache: RwLock::new(None),
        }
    }

    /// Create from the `CLERK_ISSUER_URL` environment variable.
    /// Returns `None` if the variable is not set.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("CLERK_ISSUER_URL").ok()?;
        if url.is_empty() {
            return None;
        }
        Some(Self::new(url))
    }

    /// Validate a JWT token and return the claims.
    pub async fn validate_token(&self, token: &str) -> Result<ClerkClaims, ClerkAuthError> {
        // Decode the header to get the key ID
        let header = decode_header(token).map_err(|e| {
            tracing::debug!(error = %e, "Failed to decode JWT header");
            ClerkAuthError::InvalidToken
        })?;

        let kid = header.kid.ok_or_else(|| {
            tracing::debug!("JWT header missing kid");
            ClerkAuthError::InvalidToken
        })?;

        // Try to find the key in cache first
        let decoding_key = self.get_decoding_key(&kid).await?;

        // Validate the token
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer_url]);
        // Clerk JWTs don't always have an audience claim
        validation.validate_aud = false;

        let token_data = decode::<ClerkClaims>(token, &decoding_key, &validation).map_err(|e| {
            tracing::debug!(error = %e, "JWT validation failed");
            ClerkAuthError::InvalidToken
        })?;

        Ok(token_data.claims)
    }

    /// Get the decoding key for a given kid, fetching JWKS if needed.
    async fn get_decoding_key(&self, kid: &str) -> Result<DecodingKey, ClerkAuthError> {
        // Try cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if !cached.is_expired() {
                    if let Some(jwk) = cached.find_key(kid) {
                        return jwk_to_decoding_key(jwk);
                    }
                }
            }
        }

        // Cache miss or expired or unknown kid — refetch
        let jwks = self.fetch_jwks().await?;

        let key = jwks
            .find_key(kid)
            .ok_or_else(|| {
                tracing::warn!(kid = %kid, "Unknown kid after JWKS refresh");
                ClerkAuthError::UnknownKey
            })?
            .clone();

        // Update cache
        {
            let mut cache = self.cache.write().await;
            *cache = Some(jwks);
        }

        jwk_to_decoding_key(&key)
    }

    /// Fetch JWKS from the Clerk issuer.
    async fn fetch_jwks(&self) -> Result<CachedJwks, ClerkAuthError> {
        tracing::debug!(url = %self.jwks_url, "Fetching Clerk JWKS");

        let resp = self
            .client
            .get(&self.jwks_url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, url = %self.jwks_url, "Failed to fetch JWKS");
                ClerkAuthError::JwksFetchFailed
            })?;

        if !resp.status().is_success() {
            tracing::error!(status = %resp.status(), "JWKS endpoint returned error");
            return Err(ClerkAuthError::JwksFetchFailed);
        }

        let jwks: JwksResponse = resp.json().await.map_err(|e| {
            tracing::error!(error = %e, "Failed to parse JWKS response");
            ClerkAuthError::JwksFetchFailed
        })?;

        Ok(CachedJwks {
            keys: jwks.keys,
            fetched_at: Instant::now(),
        })
    }
}

/// Convert a JWK to a DecodingKey.
fn jwk_to_decoding_key(jwk: &Jwk) -> Result<DecodingKey, ClerkAuthError> {
    if jwk.kty != "RSA" {
        tracing::warn!(kty = %jwk.kty, "Unsupported key type");
        return Err(ClerkAuthError::UnsupportedKeyType);
    }
    DecodingKey::from_rsa_components(&jwk.n, &jwk.e).map_err(|e| {
        tracing::error!(error = %e, kid = %jwk.kid, "Failed to create decoding key from JWK");
        ClerkAuthError::InvalidKey
    })
}

/// Errors from Clerk JWT authentication.
#[derive(Debug)]
pub enum ClerkAuthError {
    InvalidToken,
    UnknownKey,
    JwksFetchFailed,
    UnsupportedKeyType,
    InvalidKey,
}

impl std::fmt::Display for ClerkAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidToken => write!(f, "Invalid JWT token"),
            Self::UnknownKey => write!(f, "Unknown signing key"),
            Self::JwksFetchFailed => write!(f, "Failed to fetch JWKS"),
            Self::UnsupportedKeyType => write!(f, "Unsupported key type"),
            Self::InvalidKey => write!(f, "Invalid key data"),
        }
    }
}

/// Check if a bearer token looks like a JWT (3 dot-separated base64 segments).
pub fn looks_like_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty())
}
