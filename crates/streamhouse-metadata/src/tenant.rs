//! Tenant Context Module
//!
//! This module provides utilities for managing tenant context in multi-tenant deployments.
//! It handles:
//! - Extracting tenant information from API keys
//! - Validating API key permissions
//! - Managing tenant context in requests
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_metadata::tenant::{TenantContext, ApiKeyValidator};
//!
//! // Create validator with metadata store
//! let validator = ApiKeyValidator::new(metadata_store.clone());
//!
//! // Validate API key and get tenant context
//! let context = validator.validate("sk_live_abc123...").await?;
//!
//! // Use context to authorize operations
//! if context.can_write() && context.can_access_topic("orders") {
//!     // Proceed with write operation
//! }
//! ```

use crate::{ApiKey, MetadataStore, Organization, OrganizationQuota, Result, DEFAULT_ORGANIZATION_ID};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Tenant context containing validated organization and permissions.
///
/// This struct is created after successfully validating an API key
/// and contains all information needed to authorize operations.
#[derive(Debug, Clone)]
pub struct TenantContext {
    /// The organization making the request
    pub organization: Organization,
    /// The API key used for authentication (if any)
    pub api_key: Option<ApiKey>,
    /// Organization quotas
    pub quota: OrganizationQuota,
    /// Whether this is the default (single-tenant) context
    pub is_default: bool,
}

impl TenantContext {
    /// Create a default tenant context for single-tenant deployments
    pub fn default_context() -> Self {
        Self {
            organization: Organization {
                id: DEFAULT_ORGANIZATION_ID.to_string(),
                name: "Default Organization".to_string(),
                slug: "default".to_string(),
                plan: crate::OrganizationPlan::Enterprise,
                status: crate::OrganizationStatus::Active,
                created_at: 0,
                settings: Default::default(),
            },
            api_key: None,
            quota: OrganizationQuota::default(),
            is_default: true,
        }
    }

    /// Check if the tenant can read data
    pub fn can_read(&self) -> bool {
        if self.is_default {
            return true;
        }
        self.api_key
            .as_ref()
            .map(|k| k.permissions.iter().any(|p| p == "read" || p == "admin"))
            .unwrap_or(false)
    }

    /// Check if the tenant can write data
    pub fn can_write(&self) -> bool {
        if self.is_default {
            return true;
        }
        self.api_key
            .as_ref()
            .map(|k| k.permissions.iter().any(|p| p == "write" || p == "admin"))
            .unwrap_or(false)
    }

    /// Check if the tenant has admin permissions
    pub fn can_admin(&self) -> bool {
        if self.is_default {
            return true;
        }
        self.api_key
            .as_ref()
            .map(|k| k.permissions.iter().any(|p| p == "admin"))
            .unwrap_or(false)
    }

    /// Check if the tenant can access a specific topic
    ///
    /// Returns true if:
    /// - This is the default context
    /// - API key has no scope restrictions (empty scopes)
    /// - Topic matches one of the scoped patterns
    pub fn can_access_topic(&self, topic: &str) -> bool {
        if self.is_default {
            return true;
        }

        match &self.api_key {
            None => false,
            Some(key) => {
                // Empty scopes means access to all topics
                if key.scopes.is_empty() {
                    return true;
                }

                // Check if topic matches any scope pattern
                key.scopes.iter().any(|scope| {
                    if scope.ends_with('*') {
                        // Prefix match
                        let prefix = &scope[..scope.len() - 1];
                        topic.starts_with(prefix)
                    } else {
                        // Exact match
                        topic == scope
                    }
                })
            }
        }
    }

    /// Get the S3 prefix for this organization's data
    ///
    /// Data is stored with organization isolation:
    /// `org-{org_id}/data/{topic}/{partition}/`
    pub fn s3_prefix(&self) -> String {
        if self.is_default {
            "data".to_string()
        } else {
            format!("org-{}/data", self.organization.id)
        }
    }

    /// Get the full S3 key for a topic partition
    pub fn s3_key_for_partition(&self, topic: &str, partition: u32, segment_id: &str) -> String {
        format!(
            "{}/{}/{}/{}.seg",
            self.s3_prefix(),
            topic,
            partition,
            segment_id
        )
    }
}

/// API key validator for authenticating requests
pub struct ApiKeyValidator<S: MetadataStore> {
    store: Arc<S>,
}

impl<S: MetadataStore> Clone for ApiKeyValidator<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S: MetadataStore> ApiKeyValidator<S> {
    /// Create a new API key validator
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Hash an API key using SHA-256
    pub fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Validate an API key and return the tenant context
    ///
    /// # Arguments
    ///
    /// * `key` - The raw API key (e.g., "sk_live_abc123...")
    ///
    /// # Returns
    ///
    /// A `TenantContext` if the key is valid, or an error if:
    /// - Key is invalid/not found
    /// - Key is expired
    /// - Organization is suspended/deleted
    pub async fn validate(&self, key: &str) -> Result<TenantContext> {
        // Hash the key
        let key_hash = Self::hash_key(key);

        // Look up the key
        let api_key = self
            .store
            .validate_api_key(&key_hash)
            .await?
            .ok_or_else(|| {
                crate::error::MetadataError::AuthenticationError("Invalid API key".to_string())
            })?;

        // Update last_used_at
        let _ = self.store.touch_api_key(&api_key.id).await;

        // Get the organization
        let organization = self
            .store
            .get_organization(&api_key.organization_id)
            .await?
            .ok_or_else(|| {
                crate::error::MetadataError::NotFoundError(format!(
                    "Organization {} not found",
                    api_key.organization_id
                ))
            })?;

        // Check organization status
        if organization.status != crate::OrganizationStatus::Active {
            return Err(crate::error::MetadataError::AuthenticationError(format!(
                "Organization {} is {}",
                organization.slug, organization.status
            )));
        }

        // Get organization quotas
        let quota = self
            .store
            .get_organization_quota(&organization.id)
            .await?;

        Ok(TenantContext {
            organization,
            api_key: Some(api_key),
            quota,
            is_default: false,
        })
    }

    /// Get tenant context for the default organization (single-tenant mode)
    pub async fn default_context(&self) -> Result<TenantContext> {
        // Try to get the default organization
        if let Some(org) = self.store.get_organization(DEFAULT_ORGANIZATION_ID).await? {
            let quota = self.store.get_organization_quota(&org.id).await?;
            Ok(TenantContext {
                organization: org,
                api_key: None,
                quota,
                is_default: true,
            })
        } else {
            // Return a synthetic default context
            Ok(TenantContext::default_context())
        }
    }
}

/// Generate a new API key
///
/// Returns a tuple of (raw_key, key_hash, key_prefix)
/// The raw_key should be given to the user once, the hash stored in the database
pub fn generate_api_key(prefix: &str) -> (String, String, String) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // Generate 32 random bytes
    let random_bytes: [u8; 32] = rng.gen();
    let random_part: String = random_bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    // Format: prefix_randomhex
    let raw_key = format!("{}_{}", prefix, random_part);
    let key_hash = ApiKeyValidator::<crate::SqliteMetadataStore>::hash_key(&raw_key);
    let key_prefix = raw_key.chars().take(16).collect();

    (raw_key, key_hash, key_prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_key() {
        let key = "sk_live_abc123";
        let hash = ApiKeyValidator::<crate::SqliteMetadataStore>::hash_key(key);

        // Hash should be consistent
        let hash2 = ApiKeyValidator::<crate::SqliteMetadataStore>::hash_key(key);
        assert_eq!(hash, hash2);

        // Hash should be 64 hex characters (256 bits)
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_generate_api_key() {
        let (raw_key, hash, prefix) = generate_api_key("sk_live");

        // Raw key should start with prefix
        assert!(raw_key.starts_with("sk_live_"));

        // Hash should be 64 characters
        assert_eq!(hash.len(), 64);

        // Prefix should be 16 characters
        assert_eq!(prefix.len(), 16);

        // Generated key should hash to the same value
        let computed_hash = ApiKeyValidator::<crate::SqliteMetadataStore>::hash_key(&raw_key);
        assert_eq!(hash, computed_hash);
    }

    #[test]
    fn test_default_context_permissions() {
        let ctx = TenantContext::default_context();

        assert!(ctx.can_read());
        assert!(ctx.can_write());
        assert!(ctx.can_admin());
        assert!(ctx.can_access_topic("any-topic"));
        assert!(ctx.is_default);
    }

    #[test]
    fn test_s3_prefix() {
        // Default context
        let default_ctx = TenantContext::default_context();
        assert_eq!(default_ctx.s3_prefix(), "data");

        // Multi-tenant context
        let tenant_ctx = TenantContext {
            organization: Organization {
                id: "org-123".to_string(),
                name: "Test Org".to_string(),
                slug: "test-org".to_string(),
                plan: crate::OrganizationPlan::Pro,
                status: crate::OrganizationStatus::Active,
                created_at: 0,
                settings: Default::default(),
            },
            api_key: None,
            quota: OrganizationQuota::default(),
            is_default: false,
        };
        assert_eq!(tenant_ctx.s3_prefix(), "org-org-123/data");
    }

    #[test]
    fn test_topic_access_with_scopes() {
        let api_key = ApiKey {
            id: "key-1".to_string(),
            organization_id: "org-1".to_string(),
            name: "Test Key".to_string(),
            key_prefix: "sk_live_abc".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
            scopes: vec!["orders".to_string(), "events-*".to_string()],
            expires_at: None,
            last_used_at: None,
            created_at: 0,
            created_by: None,
        };

        let ctx = TenantContext {
            organization: Organization {
                id: "org-1".to_string(),
                name: "Test Org".to_string(),
                slug: "test-org".to_string(),
                plan: crate::OrganizationPlan::Pro,
                status: crate::OrganizationStatus::Active,
                created_at: 0,
                settings: Default::default(),
            },
            api_key: Some(api_key),
            quota: OrganizationQuota::default(),
            is_default: false,
        };

        // Exact match
        assert!(ctx.can_access_topic("orders"));

        // Prefix match
        assert!(ctx.can_access_topic("events-1"));
        assert!(ctx.can_access_topic("events-user-activity"));

        // No match
        assert!(!ctx.can_access_topic("users"));
        assert!(!ctx.can_access_topic("eventsx"));
    }
}
