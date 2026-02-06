//! Kafka Protocol Tenant Resolution
//!
//! This module provides tenant resolution for Kafka protocol connections.
//! Tenants can authenticate using SASL/PLAIN with their API key.
//!
//! ## Authentication Methods
//!
//! ### 1. SASL/PLAIN (Recommended)
//!
//! Standard Kafka SASL authentication where:
//! - Username: API key (e.g., `sk_live_abc123...`)
//! - Password: API key (same as username)
//!
//! ```text
//! Client                          Server
//!   |                               |
//!   |--- ApiVersions Request ----->|
//!   |<-- ApiVersions Response -----|
//!   |                               |
//!   |--- SaslHandshake Request --->|
//!   |<-- SaslHandshake Response ---|
//!   |                               |
//!   |--- SaslAuthenticate -------->|
//!   |    (PLAIN: username=key,     |
//!   |     password=key)            |
//!   |<-- SaslAuthenticate ---------|
//!   |    (authenticated!)          |
//!   |                               |
//!   |--- Metadata Request -------->|
//!   |    (tenant context applied)  |
//! ```
//!
//! ### 2. Client ID Prefix (Development/Testing)
//!
//! For development without SASL, include API key prefix in client_id:
//! - client_id: `sk_live_abc1234567_myapp`
//!
//! This extracts `sk_live_abc123456` as the API key prefix for lookup.
//!
//! ## Connection State
//!
//! After authentication, the tenant context is stored in the connection state
//! and automatically applied to all subsequent requests.
//!
//! ## Usage
//!
//! ```ignore
//! // Kafka client configuration for SASL/PLAIN
//! let config = ClientConfig::new()
//!     .set("bootstrap.servers", "localhost:9092")
//!     .set("security.protocol", "SASL_PLAINTEXT")
//!     .set("sasl.mechanism", "PLAIN")
//!     .set("sasl.username", "sk_live_abc123...")
//!     .set("sasl.password", "sk_live_abc123...");
//! ```

use std::sync::Arc;

use streamhouse_metadata::tenant::TenantContext;
use streamhouse_metadata::MetadataStore;

use crate::error::{KafkaError, KafkaResult};

/// Kafka connection tenant context.
///
/// Stores the resolved tenant information for a Kafka connection.
#[derive(Clone)]
pub struct KafkaTenantContext {
    /// The resolved tenant context
    pub tenant: TenantContext,
    /// Authentication method used
    pub auth_method: AuthMethod,
}

/// How the tenant was authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    /// No authentication (anonymous/default tenant)
    Anonymous,
    /// SASL/PLAIN authentication with API key
    SaslPlain,
    /// Extracted from client_id prefix
    ClientIdPrefix,
}

impl KafkaTenantContext {
    /// Create a default (anonymous) tenant context.
    pub fn default_context() -> Self {
        Self {
            tenant: TenantContext::default_context(),
            auth_method: AuthMethod::Anonymous,
        }
    }

    /// Check if this is a default/anonymous tenant.
    pub fn is_anonymous(&self) -> bool {
        self.auth_method == AuthMethod::Anonymous
    }
}

/// Kafka tenant resolver.
///
/// Resolves tenant context from various authentication methods.
pub struct KafkaTenantResolver<S: MetadataStore> {
    store: Arc<S>,
    /// Whether to allow anonymous connections
    allow_anonymous: bool,
}

impl<S: MetadataStore> KafkaTenantResolver<S> {
    /// Create a new tenant resolver.
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            allow_anonymous: true, // Default: allow anonymous for backwards compatibility
        }
    }

    /// Configure whether anonymous connections are allowed.
    pub fn with_anonymous_allowed(mut self, allow: bool) -> Self {
        self.allow_anonymous = allow;
        self
    }

    /// Resolve tenant from SASL/PLAIN credentials.
    ///
    /// # Arguments
    ///
    /// * `username` - The SASL username (API key)
    /// * `password` - The SASL password (API key, should match username)
    ///
    /// # Returns
    ///
    /// The resolved tenant context.
    pub async fn resolve_sasl_plain(
        &self,
        username: &str,
        _password: &str, // Not used for now; we validate by key lookup
    ) -> KafkaResult<KafkaTenantContext> {
        // Hash the API key and look it up
        let key_hash = streamhouse_metadata::tenant::ApiKeyValidator::<S>::hash_key(username);

        // Find the API key
        let api_key = self
            .store
            .validate_api_key(&key_hash)
            .await
            .map_err(|e| KafkaError::AuthenticationError(e.to_string()))?
            .ok_or_else(|| KafkaError::AuthenticationError("Invalid API key".to_string()))?;

        // Load the organization
        let organization = self
            .store
            .get_organization(&api_key.organization_id)
            .await
            .map_err(|e| KafkaError::AuthenticationError(e.to_string()))?
            .ok_or_else(|| KafkaError::AuthenticationError("Organization not found".to_string()))?;

        // Check organization status
        if organization.status != streamhouse_metadata::OrganizationStatus::Active {
            return Err(KafkaError::AuthenticationError(format!(
                "Organization is {}",
                organization.status
            )));
        }

        // Load quotas
        let quota = self
            .store
            .get_organization_quota(&api_key.organization_id)
            .await
            .map_err(|e| KafkaError::AuthenticationError(e.to_string()))?;

        // Update last_used_at
        let _ = self.store.touch_api_key(&api_key.id).await;

        Ok(KafkaTenantContext {
            tenant: TenantContext {
                organization,
                api_key: Some(api_key),
                quota,
                is_default: false,
            },
            auth_method: AuthMethod::SaslPlain,
        })
    }

    /// Try to extract tenant from client_id.
    ///
    /// Looks for API key prefix in client_id format: `{key_prefix}_{client_name}`
    ///
    /// # Arguments
    ///
    /// * `client_id` - The Kafka client ID
    ///
    /// # Returns
    ///
    /// The resolved tenant context, or None if no valid prefix found.
    pub async fn try_resolve_from_client_id(
        &self,
        client_id: &str,
    ) -> KafkaResult<Option<KafkaTenantContext>> {
        // Check if client_id starts with a key prefix pattern (sk_live_, sk_test_)
        let _prefix = if client_id.starts_with("sk_live_") || client_id.starts_with("sk_test_") {
            // Extract the first 20 chars (sk_live_ + 12 chars of key prefix)
            if client_id.len() >= 20 {
                &client_id[..20]
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        // Look up API key by prefix
        // Note: This requires a prefix lookup which we don't have yet.
        // For now, we'll use the full client_id as the key if it looks like one.
        if client_id.starts_with("sk_") && client_id.len() >= 32 {
            // This looks like a full API key in client_id
            let parts: Vec<&str> = client_id.splitn(2, '_').collect();
            if !parts.is_empty() {
                // Try to use the full client_id as an API key
                match self.resolve_sasl_plain(client_id, client_id).await {
                    Ok(ctx) => {
                        return Ok(Some(KafkaTenantContext {
                            tenant: ctx.tenant,
                            auth_method: AuthMethod::ClientIdPrefix,
                        }));
                    }
                    Err(_) => return Ok(None),
                }
            }
        }

        Ok(None)
    }

    /// Get default tenant context for anonymous connections.
    ///
    /// # Returns
    ///
    /// The default tenant context, or error if anonymous is not allowed.
    pub async fn default_context(&self) -> KafkaResult<KafkaTenantContext> {
        if !self.allow_anonymous {
            return Err(KafkaError::AuthenticationError(
                "Anonymous connections not allowed".to_string(),
            ));
        }

        Ok(KafkaTenantContext::default_context())
    }

    /// Resolve tenant, trying multiple methods.
    ///
    /// # Arguments
    ///
    /// * `sasl_username` - Optional SASL username
    /// * `sasl_password` - Optional SASL password
    /// * `client_id` - Optional Kafka client ID
    ///
    /// # Returns
    ///
    /// The resolved tenant context.
    pub async fn resolve(
        &self,
        sasl_username: Option<&str>,
        sasl_password: Option<&str>,
        client_id: Option<&str>,
    ) -> KafkaResult<KafkaTenantContext> {
        // Try SASL first if provided
        if let (Some(username), Some(password)) = (sasl_username, sasl_password) {
            return self.resolve_sasl_plain(username, password).await;
        }

        // Try client_id extraction
        if let Some(cid) = client_id {
            if let Ok(Some(ctx)) = self.try_resolve_from_client_id(cid).await {
                return Ok(ctx);
            }
        }

        // Fall back to default
        self.default_context().await
    }
}

impl<S: MetadataStore> Clone for KafkaTenantResolver<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            allow_anonymous: self.allow_anonymous,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_context() {
        let ctx = KafkaTenantContext::default_context();
        assert!(ctx.is_anonymous());
        assert_eq!(ctx.auth_method, AuthMethod::Anonymous);
    }

    #[test]
    fn test_auth_method_equality() {
        assert_eq!(AuthMethod::Anonymous, AuthMethod::Anonymous);
        assert_ne!(AuthMethod::Anonymous, AuthMethod::SaslPlain);
        assert_ne!(AuthMethod::SaslPlain, AuthMethod::ClientIdPrefix);
    }
}
