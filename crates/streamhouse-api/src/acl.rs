//! Access Control List (ACL) System for StreamHouse
//!
//! Provides fine-grained access control for resources like topics, consumer groups,
//! and organizations. Works alongside the permission system (read/write/admin) to
//! provide granular control.
//!
//! ## Concepts
//!
//! - **Principal**: Who is making the request (user, service account, API key)
//! - **Resource**: What is being accessed (topic, consumer group, organization)
//! - **Action**: What operation is being performed (read, write, describe, alter, delete)
//! - **ACL Entry**: A rule that allows or denies an action on a resource for a principal
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::acl::{AclChecker, AclEntry, AclAction, AclResource, AclPermission};
//!
//! let checker = AclChecker::new();
//!
//! // Allow user to read from "orders" topic
//! checker.add_entry(AclEntry {
//!     principal: "user:alice".to_string(),
//!     resource: AclResource::Topic("orders".to_string()),
//!     action: AclAction::Read,
//!     permission: AclPermission::Allow,
//! });
//!
//! // Check if allowed
//! let allowed = checker.is_allowed("user:alice", &AclResource::Topic("orders".to_string()), AclAction::Read);
//! ```
//!
//! ## Wildcard Patterns
//!
//! - Principal: `user:*` matches all users, `service:payment-*` matches all payment services
//! - Resource: `Topic("orders-*")` matches orders-eu, orders-us, etc.
//!
//! ## Default Behavior
//!
//! If no ACL entry matches, the default is to deny unless the principal has admin permission.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Resource types that can be protected by ACLs
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AclResource {
    /// A topic (supports wildcards like "orders-*")
    Topic(String),
    /// A consumer group (supports wildcards)
    ConsumerGroup(String),
    /// An organization
    Organization(String),
    /// Schema subjects
    Schema(String),
    /// Cluster-level operations
    Cluster,
    /// All resources
    All,
}

impl AclResource {
    /// Check if this resource pattern matches a specific resource
    pub fn matches(&self, other: &AclResource) -> bool {
        match (self, other) {
            (AclResource::All, _) => true,
            (AclResource::Cluster, AclResource::Cluster) => true,
            (AclResource::Topic(pattern), AclResource::Topic(name)) => {
                pattern_matches(pattern, name)
            }
            (AclResource::ConsumerGroup(pattern), AclResource::ConsumerGroup(name)) => {
                pattern_matches(pattern, name)
            }
            (AclResource::Organization(pattern), AclResource::Organization(name)) => {
                pattern_matches(pattern, name)
            }
            (AclResource::Schema(pattern), AclResource::Schema(name)) => {
                pattern_matches(pattern, name)
            }
            _ => false,
        }
    }

    /// Get the resource type name
    pub fn resource_type(&self) -> &'static str {
        match self {
            AclResource::Topic(_) => "topic",
            AclResource::ConsumerGroup(_) => "consumer_group",
            AclResource::Organization(_) => "organization",
            AclResource::Schema(_) => "schema",
            AclResource::Cluster => "cluster",
            AclResource::All => "all",
        }
    }

    /// Get the resource name/pattern
    pub fn name(&self) -> &str {
        match self {
            AclResource::Topic(name) => name,
            AclResource::ConsumerGroup(name) => name,
            AclResource::Organization(name) => name,
            AclResource::Schema(name) => name,
            AclResource::Cluster => "*",
            AclResource::All => "*",
        }
    }
}

impl std::fmt::Display for AclResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.resource_type(), self.name())
    }
}

/// Actions that can be performed on resources
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AclAction {
    /// Read data (consume messages, get topic info)
    Read,
    /// Write data (produce messages)
    Write,
    /// Create resources (topics, consumer groups)
    Create,
    /// Delete resources
    Delete,
    /// Alter configuration
    Alter,
    /// Describe metadata
    Describe,
    /// Administrative operations
    Admin,
    /// All actions
    All,
}

impl AclAction {
    /// Check if this action pattern matches a specific action
    pub fn matches(&self, other: AclAction) -> bool {
        if *self == AclAction::All {
            return true;
        }
        *self == other
    }

    /// Convert from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "read" => Some(AclAction::Read),
            "write" => Some(AclAction::Write),
            "create" => Some(AclAction::Create),
            "delete" => Some(AclAction::Delete),
            "alter" => Some(AclAction::Alter),
            "describe" => Some(AclAction::Describe),
            "admin" => Some(AclAction::Admin),
            "all" | "*" => Some(AclAction::All),
            _ => None,
        }
    }
}

impl std::fmt::Display for AclAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AclAction::Read => "read",
            AclAction::Write => "write",
            AclAction::Create => "create",
            AclAction::Delete => "delete",
            AclAction::Alter => "alter",
            AclAction::Describe => "describe",
            AclAction::Admin => "admin",
            AclAction::All => "all",
        };
        write!(f, "{}", s)
    }
}

/// Permission type (allow or deny)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AclPermission {
    Allow,
    Deny,
}

/// An ACL entry that grants or denies access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEntry {
    /// Unique ID for this entry
    pub id: String,
    /// Principal (e.g., "user:alice", "service:payment", "apikey:key_123", "*")
    pub principal: String,
    /// Resource being protected
    pub resource: AclResource,
    /// Action being controlled
    pub action: AclAction,
    /// Allow or deny
    pub permission: AclPermission,
    /// When this entry was created
    pub created_at: i64,
    /// Who created this entry
    pub created_by: Option<String>,
}

impl AclEntry {
    /// Create a new allow entry
    pub fn allow(principal: impl Into<String>, resource: AclResource, action: AclAction) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            principal: principal.into(),
            resource,
            action,
            permission: AclPermission::Allow,
            created_at: chrono::Utc::now().timestamp_millis(),
            created_by: None,
        }
    }

    /// Create a new deny entry
    pub fn deny(principal: impl Into<String>, resource: AclResource, action: AclAction) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            principal: principal.into(),
            resource,
            action,
            permission: AclPermission::Deny,
            created_at: chrono::Utc::now().timestamp_millis(),
            created_by: None,
        }
    }

    /// Check if this entry matches the given principal, resource, and action
    pub fn matches(&self, principal: &str, resource: &AclResource, action: AclAction) -> bool {
        pattern_matches(&self.principal, principal)
            && self.resource.matches(resource)
            && self.action.matches(action)
    }
}

/// Check if a pattern matches a value (supports * wildcard)
fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        return value.starts_with(prefix);
    }

    if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        return value.ends_with(suffix);
    }

    if pattern.contains('*') {
        // Handle middle wildcards by splitting and checking prefix/suffix
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return value.starts_with(parts[0]) && value.ends_with(parts[1]);
        }
    }

    pattern == value
}

/// Result of an ACL check
#[derive(Debug, Clone)]
pub struct AclCheckResult {
    /// Whether access is allowed
    pub allowed: bool,
    /// The matching entry (if any)
    pub matched_entry: Option<AclEntry>,
    /// Reason for the decision
    pub reason: String,
}

impl AclCheckResult {
    pub fn allowed(entry: AclEntry) -> Self {
        Self {
            allowed: true,
            reason: format!("Allowed by ACL entry {}", entry.id),
            matched_entry: Some(entry),
        }
    }

    pub fn denied(entry: AclEntry) -> Self {
        Self {
            allowed: false,
            reason: format!("Denied by ACL entry {}", entry.id),
            matched_entry: Some(entry),
        }
    }

    pub fn default_deny() -> Self {
        Self {
            allowed: false,
            matched_entry: None,
            reason: "No matching ACL entry (default deny)".to_string(),
        }
    }

    pub fn admin_bypass() -> Self {
        Self {
            allowed: true,
            matched_entry: None,
            reason: "Admin bypass".to_string(),
        }
    }
}

/// In-memory ACL checker for authorization decisions
///
/// This provides a thread-safe, in-memory ACL store. For production use,
/// ACLs should be persisted in the metadata store and loaded on startup.
#[derive(Default)]
pub struct AclChecker {
    entries: RwLock<Vec<AclEntry>>,
    /// Cache of (principal, resource_str, action) -> allowed
    cache: RwLock<HashMap<(String, String, String), bool>>,
}

impl AclChecker {
    /// Create a new ACL checker
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create an ACL checker with initial entries
    pub fn with_entries(entries: Vec<AclEntry>) -> Self {
        Self {
            entries: RwLock::new(entries),
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Add an ACL entry
    pub fn add_entry(&self, entry: AclEntry) {
        if let Ok(mut entries) = self.entries.write() {
            entries.push(entry);
        }
        // Clear cache when entries change
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Remove an ACL entry by ID
    pub fn remove_entry(&self, entry_id: &str) -> bool {
        let removed = if let Ok(mut entries) = self.entries.write() {
            let len_before = entries.len();
            entries.retain(|e| e.id != entry_id);
            entries.len() < len_before
        } else {
            false
        };

        if removed {
            if let Ok(mut cache) = self.cache.write() {
                cache.clear();
            }
        }
        removed
    }

    /// List all entries
    pub fn list_entries(&self) -> Vec<AclEntry> {
        self.entries.read().map(|e| e.clone()).unwrap_or_default()
    }

    /// List entries matching a filter
    pub fn list_entries_filtered(
        &self,
        principal_filter: Option<&str>,
        resource_type_filter: Option<&str>,
    ) -> Vec<AclEntry> {
        self.entries
            .read()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| {
                        let principal_matches = principal_filter
                            .map(|p| {
                                pattern_matches(p, &e.principal) || pattern_matches(&e.principal, p)
                            })
                            .unwrap_or(true);
                        let resource_matches = resource_type_filter
                            .map(|r| e.resource.resource_type() == r)
                            .unwrap_or(true);
                        principal_matches && resource_matches
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if an action is allowed
    pub fn check(
        &self,
        principal: &str,
        resource: &AclResource,
        action: AclAction,
    ) -> AclCheckResult {
        // Check cache first
        let cache_key = (
            principal.to_string(),
            resource.to_string(),
            action.to_string(),
        );

        if let Ok(cache) = self.cache.read() {
            if let Some(&allowed) = cache.get(&cache_key) {
                return if allowed {
                    AclCheckResult {
                        allowed: true,
                        matched_entry: None,
                        reason: "Cached allow".to_string(),
                    }
                } else {
                    AclCheckResult {
                        allowed: false,
                        matched_entry: None,
                        reason: "Cached deny".to_string(),
                    }
                };
            }
        }

        // Find matching entries
        let result = if let Ok(entries) = self.entries.read() {
            // Deny entries take precedence - check them first
            for entry in entries.iter() {
                if entry.permission == AclPermission::Deny
                    && entry.matches(principal, resource, action)
                {
                    return AclCheckResult::denied(entry.clone());
                }
            }

            // Then check allow entries
            for entry in entries.iter() {
                if entry.permission == AclPermission::Allow
                    && entry.matches(principal, resource, action)
                {
                    return AclCheckResult::allowed(entry.clone());
                }
            }

            AclCheckResult::default_deny()
        } else {
            AclCheckResult::default_deny()
        };

        // Update cache
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(cache_key, result.allowed);
        }

        result
    }

    /// Check if allowed (simple boolean result)
    pub fn is_allowed(&self, principal: &str, resource: &AclResource, action: AclAction) -> bool {
        self.check(principal, resource, action).allowed
    }

    /// Clear all entries
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Get entry count
    pub fn entry_count(&self) -> usize {
        self.entries.read().map(|e| e.len()).unwrap_or(0)
    }
}

/// Configuration for ACL checking
#[derive(Debug, Clone)]
pub struct AclConfig {
    /// Whether ACL checking is enabled
    pub enabled: bool,
    /// Allow admin principals to bypass ACL checks
    pub admin_bypass: bool,
    /// Default permission when no entry matches
    pub default_permission: AclPermission,
    /// Principal pattern for admins (e.g., "admin:*")
    pub admin_pattern: String,
}

impl Default for AclConfig {
    fn default() -> Self {
        Self {
            enabled: std::env::var("ACL_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            admin_bypass: true,
            default_permission: AclPermission::Deny,
            admin_pattern: "admin:*".to_string(),
        }
    }
}

/// ACL authorization layer for Axum routes
#[derive(Clone)]
pub struct AclLayer {
    checker: std::sync::Arc<AclChecker>,
    config: AclConfig,
    resource_extractor: ResourceExtractor,
}

/// Function type for extracting resources from requests
type ResourceExtractor =
    std::sync::Arc<dyn Fn(&str, &str) -> Option<(AclResource, AclAction)> + Send + Sync>;

impl AclLayer {
    /// Create a new ACL layer with default resource extraction
    pub fn new(checker: std::sync::Arc<AclChecker>, config: AclConfig) -> Self {
        Self {
            checker,
            config,
            resource_extractor: std::sync::Arc::new(default_resource_extractor),
        }
    }

    /// Create with custom resource extractor
    pub fn with_extractor<F>(
        checker: std::sync::Arc<AclChecker>,
        config: AclConfig,
        extractor: F,
    ) -> Self
    where
        F: Fn(&str, &str) -> Option<(AclResource, AclAction)> + Send + Sync + 'static,
    {
        Self {
            checker,
            config,
            resource_extractor: std::sync::Arc::new(extractor),
        }
    }
}

/// Default resource extractor that maps HTTP paths to ACL resources
fn default_resource_extractor(method: &str, path: &str) -> Option<(AclResource, AclAction)> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match parts.as_slice() {
        // Topic operations
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

        // Produce
        ["api", "v1", "produce"] | ["api", "v1", "produce", "batch"] => {
            // Topic is in the request body, can't determine here
            Some((AclResource::All, AclAction::Write))
        }

        // Consume
        ["api", "v1", "consume"] => {
            // Topic is in query params, can't determine here
            Some((AclResource::All, AclAction::Read))
        }

        // Consumer groups
        ["api", "v1", "consumer-groups"] => Some((AclResource::All, AclAction::Describe)),
        // commit must come before the wildcard pattern
        ["api", "v1", "consumer-groups", "commit"] => Some((AclResource::All, AclAction::Write)),
        ["api", "v1", "consumer-groups", group_id] => {
            let action = match method {
                "GET" => AclAction::Describe,
                "DELETE" => AclAction::Delete,
                _ => return None,
            };
            Some((AclResource::ConsumerGroup(group_id.to_string()), action))
        }
        ["api", "v1", "consumer-groups", group_id, "reset"]
        | ["api", "v1", "consumer-groups", group_id, "seek"] => Some((
            AclResource::ConsumerGroup(group_id.to_string()),
            AclAction::Alter,
        )),

        // Organizations (admin)
        ["api", "v1", "organizations"] | ["api", "v1", "organizations", ..] => {
            Some((AclResource::Cluster, AclAction::Admin))
        }

        // API keys (admin)
        ["api", "v1", "api-keys", ..] => Some((AclResource::Cluster, AclAction::Admin)),

        // SQL
        ["api", "v1", "sql"] => Some((AclResource::All, AclAction::Read)),

        // Health/metrics - no ACL needed
        ["health"] | ["live"] | ["ready"] | ["metrics"] => None,

        _ => None,
    }
}

impl<S> tower::Layer<S> for AclLayer {
    type Service = AclMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AclMiddleware {
            inner,
            checker: self.checker.clone(),
            config: self.config.clone(),
            resource_extractor: self.resource_extractor.clone(),
        }
    }
}

/// ACL middleware service
#[derive(Clone)]
pub struct AclMiddleware<S> {
    inner: S,
    checker: std::sync::Arc<AclChecker>,
    config: AclConfig,
    resource_extractor: ResourceExtractor,
}

impl<S> tower::Service<axum::extract::Request> for AclMiddleware<S>
where
    S: tower::Service<axum::extract::Request, Response = axum::response::Response>
        + Send
        + Clone
        + 'static,
    S::Future: Send + 'static,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: axum::extract::Request) -> Self::Future {
        // Skip if ACL not enabled
        if !self.config.enabled {
            let future = self.inner.call(request);
            return Box::pin(async move { future.await });
        }

        let checker = self.checker.clone();
        let config = self.config.clone();
        let extractor = self.resource_extractor.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let method = request.method().as_str();
            let path = request.uri().path();

            // Extract resource and action from request
            let Some((resource, action)) = extractor(method, path) else {
                // No ACL needed for this path
                return inner.call(request).await;
            };

            // Get principal from authenticated key extension
            let principal = request
                .extensions()
                .get::<crate::auth::AuthenticatedKey>()
                .map(|key| format!("apikey:{}", key.key_id))
                .unwrap_or_else(|| "anonymous".to_string());

            // Check for admin bypass
            if config.admin_bypass && pattern_matches(&config.admin_pattern, &principal) {
                return inner.call(request).await;
            }

            // Also check if authenticated key has admin permission
            if config.admin_bypass {
                if let Some(auth_key) = request.extensions().get::<crate::auth::AuthenticatedKey>()
                {
                    if auth_key.permissions.iter().any(|p| p == "admin") {
                        return inner.call(request).await;
                    }
                }
            }

            // Perform ACL check
            let result = checker.check(&principal, &resource, action);

            if !result.allowed {
                tracing::warn!(
                    principal = %principal,
                    resource = %resource,
                    action = %action,
                    reason = %result.reason,
                    "ACL denied"
                );

                let body = serde_json::json!({
                    "error": format!("Access denied: {} on {} for {}", action, resource, principal),
                    "code": 403
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        assert!(pattern_matches("*", "anything"));
        assert!(pattern_matches("user:*", "user:alice"));
        assert!(pattern_matches("*-service", "payment-service"));
        assert!(pattern_matches("orders-*", "orders-eu"));
        assert!(pattern_matches("orders", "orders"));

        assert!(!pattern_matches("user:*", "service:alice"));
        assert!(!pattern_matches("orders-*", "payments-eu"));
    }

    #[test]
    fn test_resource_matching() {
        let pattern = AclResource::Topic("orders-*".to_string());
        let specific = AclResource::Topic("orders-eu".to_string());

        assert!(pattern.matches(&specific));
        assert!(!specific.matches(&pattern));

        let all = AclResource::All;
        assert!(all.matches(&specific));
    }

    #[test]
    fn test_acl_entry_matching() {
        let entry = AclEntry::allow(
            "user:alice",
            AclResource::Topic("orders-*".to_string()),
            AclAction::Read,
        );

        assert!(entry.matches(
            "user:alice",
            &AclResource::Topic("orders-eu".to_string()),
            AclAction::Read
        ));
        assert!(!entry.matches(
            "user:bob",
            &AclResource::Topic("orders-eu".to_string()),
            AclAction::Read
        ));
        assert!(!entry.matches(
            "user:alice",
            &AclResource::Topic("payments".to_string()),
            AclAction::Read
        ));
        assert!(!entry.matches(
            "user:alice",
            &AclResource::Topic("orders-eu".to_string()),
            AclAction::Write
        ));
    }

    #[test]
    fn test_acl_checker() {
        let checker = AclChecker::new();

        // Add some entries
        checker.add_entry(AclEntry::allow(
            "user:alice",
            AclResource::Topic("orders-*".to_string()),
            AclAction::Read,
        ));
        checker.add_entry(AclEntry::allow(
            "user:bob",
            AclResource::Topic("*".to_string()),
            AclAction::All,
        ));
        checker.add_entry(AclEntry::deny(
            "user:charlie",
            AclResource::Topic("secret".to_string()),
            AclAction::All,
        ));

        // Alice can read orders-*
        assert!(checker.is_allowed(
            "user:alice",
            &AclResource::Topic("orders-eu".to_string()),
            AclAction::Read
        ));

        // Alice cannot write to orders-*
        assert!(!checker.is_allowed(
            "user:alice",
            &AclResource::Topic("orders-eu".to_string()),
            AclAction::Write
        ));

        // Bob can do anything
        assert!(checker.is_allowed(
            "user:bob",
            &AclResource::Topic("anything".to_string()),
            AclAction::Write
        ));

        // Charlie is denied access to secret
        assert!(!checker.is_allowed(
            "user:charlie",
            &AclResource::Topic("secret".to_string()),
            AclAction::Read
        ));

        // Unknown user is denied by default
        assert!(!checker.is_allowed(
            "user:unknown",
            &AclResource::Topic("orders".to_string()),
            AclAction::Read
        ));
    }

    #[test]
    fn test_deny_takes_precedence() {
        let checker = AclChecker::new();

        // Allow all for everyone
        checker.add_entry(AclEntry::allow("*", AclResource::All, AclAction::All));

        // But deny specific user
        checker.add_entry(AclEntry::deny(
            "user:blocked",
            AclResource::All,
            AclAction::All,
        ));

        // Normal user is allowed
        assert!(checker.is_allowed(
            "user:normal",
            &AclResource::Topic("orders".to_string()),
            AclAction::Read
        ));

        // Blocked user is denied
        assert!(!checker.is_allowed(
            "user:blocked",
            &AclResource::Topic("orders".to_string()),
            AclAction::Read
        ));
    }

    #[test]
    fn test_remove_entry() {
        let checker = AclChecker::new();

        let entry = AclEntry::allow(
            "user:test",
            AclResource::Topic("*".to_string()),
            AclAction::Read,
        );
        let entry_id = entry.id.clone();

        checker.add_entry(entry);
        assert_eq!(checker.entry_count(), 1);

        let removed = checker.remove_entry(&entry_id);
        assert!(removed);
        assert_eq!(checker.entry_count(), 0);

        // Remove non-existent
        let removed = checker.remove_entry("non-existent");
        assert!(!removed);
    }

    #[test]
    fn test_default_resource_extractor() {
        // Topic list
        let result = default_resource_extractor("GET", "/api/v1/topics");
        assert!(matches!(
            result,
            Some((AclResource::All, AclAction::Describe))
        ));

        // Topic get
        let result = default_resource_extractor("GET", "/api/v1/topics/orders");
        assert!(
            matches!(result, Some((AclResource::Topic(name), AclAction::Describe)) if name == "orders")
        );

        // Topic delete
        let result = default_resource_extractor("DELETE", "/api/v1/topics/orders");
        assert!(
            matches!(result, Some((AclResource::Topic(name), AclAction::Delete)) if name == "orders")
        );

        // Consumer group
        let result = default_resource_extractor("GET", "/api/v1/consumer-groups/my-group");
        assert!(
            matches!(result, Some((AclResource::ConsumerGroup(name), AclAction::Describe)) if name == "my-group")
        );

        // Health endpoint - no ACL
        let result = default_resource_extractor("GET", "/health");
        assert!(result.is_none());
    }
}
