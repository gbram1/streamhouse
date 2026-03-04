//! Quota Enforcement Module
//!
//! This module provides quota enforcement for multi-tenant operations.
//! It tracks resource usage and enforces limits based on organization plans.
//!
//! ## Quota Categories
//!
//! - **Topics**: Maximum number of topics and partitions
//! - **Storage**: Maximum data stored
//! - **Throughput**: Maximum produce/consume bytes per second
//! - **Requests**: Maximum requests per second
//! - **Connections**: Maximum concurrent connections
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_metadata::{MetadataStore, QuotaEnforcer, QuotaCheck};
//!
//! // Create enforcer with the metadata store
//! let enforcer = QuotaEnforcer::new(store);
//!
//! // Check if operation is allowed
//! match enforcer.check_topic_creation(&tenant_ctx).await {
//!     QuotaCheck::Allowed => create_topic().await,
//!     QuotaCheck::Denied(reason) => return Err(reason),
//!     QuotaCheck::Warning(reason) => {
//!         warn!("{}", reason);
//!         create_topic().await
//!     }
//! }
//!
//! // Track usage after operation
//! enforcer.track_produce_bytes(&tenant_ctx, 10_000).await?;
//! ```
//!
//! ## Plan Limits
//!
//! | Resource              | Free    | Pro      | Enterprise |
//! |-----------------------|---------|----------|------------|
//! | Topics                | 10      | 100      | Unlimited  |
//! | Partitions/topic      | 12      | 48       | Unlimited  |
//! | Storage               | 10 GB   | 1 TB     | Custom     |
//! | Produce throughput    | 10 MB/s | 100 MB/s | Custom     |
//! | Consume throughput    | 50 MB/s | 500 MB/s | Custom     |
//! | Requests/sec          | 1,000   | 10,000   | Custom     |
//! | Consumer groups       | 50      | 500      | Unlimited  |

use crate::tenant::TenantContext;
use crate::{MetadataStore, OrganizationPlan, OrganizationQuota, OrganizationStatus, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Result of a quota check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaCheck {
    /// Operation is allowed within quota limits.
    Allowed,
    /// Operation is denied - quota exceeded.
    Denied(String),
    /// Operation is allowed but quota is near limit (>80%).
    Warning(String),
}

impl QuotaCheck {
    /// Returns true if the operation is allowed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, QuotaCheck::Allowed | QuotaCheck::Warning(_))
    }

    /// Returns true if the operation is denied.
    pub fn is_denied(&self) -> bool {
        matches!(self, QuotaCheck::Denied(_))
    }

    /// Returns the reason if denied or warning.
    pub fn reason(&self) -> Option<&str> {
        match self {
            QuotaCheck::Allowed => None,
            QuotaCheck::Denied(r) | QuotaCheck::Warning(r) => Some(r),
        }
    }
}

/// Rate limiter using sliding window algorithm.
struct RateLimiter {
    /// Window duration.
    window: Duration,
    /// Maximum count per window.
    max_count: i64,
    /// Current count in window.
    count: i64,
    /// Window start time.
    window_start: Instant,
}

impl RateLimiter {
    fn new(window: Duration, max_count: i64) -> Self {
        Self {
            window,
            max_count,
            count: 0,
            window_start: Instant::now(),
        }
    }

    /// Try to acquire a permit for the given amount.
    fn try_acquire(&mut self, amount: i64) -> bool {
        let now = Instant::now();

        // Reset window if expired
        if now.duration_since(self.window_start) >= self.window {
            self.count = 0;
            self.window_start = now;
        }

        // Check if we can add the amount
        if self.count + amount <= self.max_count {
            self.count += amount;
            true
        } else {
            false
        }
    }

    /// Get current usage percentage.
    fn usage_percent(&self) -> f64 {
        if self.max_count == 0 {
            return 100.0;
        }
        (self.count as f64 / self.max_count as f64) * 100.0
    }

    /// Get remaining capacity.
    #[allow(dead_code)]
    fn remaining(&self) -> i64 {
        self.max_count - self.count
    }
}

/// In-memory rate limiter state for an organization.
struct OrgRateLimitState {
    /// Produce bytes per second limiter.
    produce_limiter: RateLimiter,
    /// Consume bytes per second limiter.
    consume_limiter: RateLimiter,
    /// Requests per second limiter.
    request_limiter: RateLimiter,
}

impl OrgRateLimitState {
    fn new(quota: &OrganizationQuota) -> Self {
        Self {
            produce_limiter: RateLimiter::new(
                Duration::from_secs(1),
                quota.max_produce_bytes_per_sec,
            ),
            consume_limiter: RateLimiter::new(
                Duration::from_secs(1),
                quota.max_consume_bytes_per_sec,
            ),
            request_limiter: RateLimiter::new(
                Duration::from_secs(1),
                quota.max_requests_per_sec as i64,
            ),
        }
    }
}

/// Quota enforcer for multi-tenant operations.
///
/// Enforces resource limits based on organization plans and tracks usage.
/// Uses a combination of in-memory rate limiting (for high-frequency checks)
/// and persistent storage (for durable limits like topic count).
pub struct QuotaEnforcer<S: MetadataStore> {
    store: Arc<S>,
    /// In-memory rate limiters per organization.
    rate_limiters: RwLock<HashMap<String, OrgRateLimitState>>,
}

impl<S: MetadataStore> QuotaEnforcer<S> {
    /// Create a new quota enforcer.
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            rate_limiters: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create rate limiter state for an organization.
    async fn get_or_create_limiter(&self, ctx: &TenantContext) -> Result<()> {
        let org_id = &ctx.organization.id;

        // Fast path: check if already exists
        {
            let limiters = self.rate_limiters.read().await;
            if limiters.contains_key(org_id) {
                return Ok(());
            }
        }

        // Slow path: create new limiter state
        let mut limiters = self.rate_limiters.write().await;
        if !limiters.contains_key(org_id) {
            limiters.insert(org_id.clone(), OrgRateLimitState::new(&ctx.quota));
        }

        Ok(())
    }

    /// Check if a topic can be created.
    ///
    /// Verifies:
    /// - Organization is active
    /// - Topic count is within quota
    /// - Partition count is within quota
    pub async fn check_topic_creation(
        &self,
        ctx: &TenantContext,
        partition_count: u32,
    ) -> Result<QuotaCheck> {
        // Check organization status
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        // Get current topic count
        let topics = self.store.list_topics().await?;
        let current_topics = topics.len() as i32;

        // Check topic count
        if current_topics >= ctx.quota.max_topics {
            return Ok(QuotaCheck::Denied(format!(
                "Topic limit reached ({}/{})",
                current_topics, ctx.quota.max_topics
            )));
        }

        // Check partitions per topic
        if partition_count as i32 > ctx.quota.max_partitions_per_topic {
            return Ok(QuotaCheck::Denied(format!(
                "Partition count {} exceeds limit of {}",
                partition_count, ctx.quota.max_partitions_per_topic
            )));
        }

        // Warning if approaching limit
        let usage_percent = (current_topics as f64 / ctx.quota.max_topics as f64) * 100.0;
        if usage_percent > 80.0 {
            return Ok(QuotaCheck::Warning(format!(
                "Topic usage at {:.0}% ({}/{})",
                usage_percent, current_topics, ctx.quota.max_topics
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if produce bytes can be written.
    ///
    /// Uses in-memory rate limiting for fast checks.
    pub async fn check_produce(&self, ctx: &TenantContext, bytes: i64) -> Result<QuotaCheck> {
        // Check organization status
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        // Ensure limiter exists
        self.get_or_create_limiter(ctx).await?;

        let mut limiters = self.rate_limiters.write().await;
        let state = limiters.get_mut(&ctx.organization.id).unwrap();

        // Try to acquire produce capacity
        if !state.produce_limiter.try_acquire(bytes) {
            return Ok(QuotaCheck::Denied(format!(
                "Produce rate limit exceeded ({} bytes/s)",
                ctx.quota.max_produce_bytes_per_sec
            )));
        }

        // Also check request rate
        if !state.request_limiter.try_acquire(1) {
            return Ok(QuotaCheck::Denied(format!(
                "Request rate limit exceeded ({}/s)",
                ctx.quota.max_requests_per_sec
            )));
        }

        // Warning if approaching limit
        let usage_percent = state.produce_limiter.usage_percent();
        if usage_percent > 80.0 {
            return Ok(QuotaCheck::Warning(format!(
                "Produce rate at {:.0}% of limit",
                usage_percent
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if consume bytes can be read.
    pub async fn check_consume(&self, ctx: &TenantContext, bytes: i64) -> Result<QuotaCheck> {
        // Check organization status
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        // Ensure limiter exists
        self.get_or_create_limiter(ctx).await?;

        let mut limiters = self.rate_limiters.write().await;
        let state = limiters.get_mut(&ctx.organization.id).unwrap();

        // Try to acquire consume capacity
        if !state.consume_limiter.try_acquire(bytes) {
            return Ok(QuotaCheck::Denied(format!(
                "Consume rate limit exceeded ({} bytes/s)",
                ctx.quota.max_consume_bytes_per_sec
            )));
        }

        // Also check request rate
        if !state.request_limiter.try_acquire(1) {
            return Ok(QuotaCheck::Denied(format!(
                "Request rate limit exceeded ({}/s)",
                ctx.quota.max_requests_per_sec
            )));
        }

        // Warning if approaching limit
        let usage_percent = state.consume_limiter.usage_percent();
        if usage_percent > 80.0 {
            return Ok(QuotaCheck::Warning(format!(
                "Consume rate at {:.0}% of limit",
                usage_percent
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if a request can be made (generic request rate check).
    pub async fn check_request(&self, ctx: &TenantContext) -> Result<QuotaCheck> {
        // Check organization status
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        // Ensure limiter exists
        self.get_or_create_limiter(ctx).await?;

        let mut limiters = self.rate_limiters.write().await;
        let state = limiters.get_mut(&ctx.organization.id).unwrap();

        if !state.request_limiter.try_acquire(1) {
            return Ok(QuotaCheck::Denied(format!(
                "Request rate limit exceeded ({}/s)",
                ctx.quota.max_requests_per_sec
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check storage usage.
    ///
    /// This queries the persistent storage to check total bytes stored.
    pub async fn check_storage(&self, ctx: &TenantContext) -> Result<QuotaCheck> {
        // Check organization status
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        // Get current storage usage from metadata
        let usage = self
            .store
            .get_organization_usage(&ctx.organization.id)
            .await?;
        let storage_bytes = usage
            .iter()
            .find(|u| u.metric == "storage_bytes")
            .map(|u| u.value)
            .unwrap_or(0);

        if storage_bytes >= ctx.quota.max_storage_bytes {
            return Ok(QuotaCheck::Denied(format!(
                "Storage limit exceeded ({} of {} bytes)",
                storage_bytes, ctx.quota.max_storage_bytes
            )));
        }

        // Warning if approaching limit
        let usage_percent = (storage_bytes as f64 / ctx.quota.max_storage_bytes as f64) * 100.0;
        if usage_percent > 80.0 {
            return Ok(QuotaCheck::Warning(format!(
                "Storage usage at {:.0}% ({} of {} bytes)",
                usage_percent, storage_bytes, ctx.quota.max_storage_bytes
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Track produce bytes for billing and monitoring.
    ///
    /// Updates the persistent usage counters.
    pub async fn track_produce_bytes(&self, ctx: &TenantContext, bytes: i64) -> Result<()> {
        self.store
            .increment_organization_usage(&ctx.organization.id, "produce_bytes", bytes)
            .await
    }

    /// Track consume bytes for billing and monitoring.
    pub async fn track_consume_bytes(&self, ctx: &TenantContext, bytes: i64) -> Result<()> {
        self.store
            .increment_organization_usage(&ctx.organization.id, "consume_bytes", bytes)
            .await
    }

    /// Track storage bytes for billing and monitoring.
    pub async fn track_storage_bytes(&self, ctx: &TenantContext, bytes: i64) -> Result<()> {
        self.store
            .increment_organization_usage(&ctx.organization.id, "storage_bytes", bytes)
            .await
    }

    /// Get quota summary for an organization.
    pub async fn get_quota_summary(&self, ctx: &TenantContext) -> Result<QuotaSummary> {
        let topics = self.store.list_topics().await?;
        let usage = self
            .store
            .get_organization_usage(&ctx.organization.id)
            .await?;

        let storage_bytes = usage
            .iter()
            .find(|u| u.metric == "storage_bytes")
            .map(|u| u.value)
            .unwrap_or(0);

        let produce_bytes_total = usage
            .iter()
            .find(|u| u.metric == "produce_bytes")
            .map(|u| u.value)
            .unwrap_or(0);

        let consume_bytes_total = usage
            .iter()
            .find(|u| u.metric == "consume_bytes")
            .map(|u| u.value)
            .unwrap_or(0);

        Ok(QuotaSummary {
            organization_id: ctx.organization.id.clone(),
            plan: ctx.organization.plan,
            topics_used: topics.len() as i32,
            topics_limit: ctx.quota.max_topics,
            storage_used_bytes: storage_bytes,
            storage_limit_bytes: ctx.quota.max_storage_bytes,
            produce_rate_limit: ctx.quota.max_produce_bytes_per_sec,
            consume_rate_limit: ctx.quota.max_consume_bytes_per_sec,
            request_rate_limit: ctx.quota.max_requests_per_sec,
            produce_bytes_total,
            consume_bytes_total,
        })
    }

    /// Get default quotas for a plan.
    pub fn default_quotas_for_plan(plan: OrganizationPlan, org_id: &str) -> OrganizationQuota {
        match plan {
            OrganizationPlan::Free => OrganizationQuota {
                organization_id: org_id.to_string(),
                max_topics: 10,
                max_partitions_per_topic: 12,
                max_total_partitions: 100,
                max_storage_bytes: 10_737_418_240, // 10 GB
                max_retention_days: 7,
                max_produce_bytes_per_sec: 10_485_760, // 10 MB/s
                max_consume_bytes_per_sec: 52_428_800, // 50 MB/s
                max_requests_per_sec: 1_000,
                max_consumer_groups: 50,
                max_schemas: 100,
                max_schema_versions_per_subject: 100,
                max_connections: 100,
            },
            OrganizationPlan::Pro => OrganizationQuota {
                organization_id: org_id.to_string(),
                max_topics: 100,
                max_partitions_per_topic: 48,
                max_total_partitions: 2_000,
                max_storage_bytes: 1_099_511_627_776, // 1 TB
                max_retention_days: 90,
                max_produce_bytes_per_sec: 104_857_600, // 100 MB/s
                max_consume_bytes_per_sec: 524_288_000, // 500 MB/s
                max_requests_per_sec: 10_000,
                max_consumer_groups: 500,
                max_schemas: 1_000,
                max_schema_versions_per_subject: 500,
                max_connections: 1_000,
            },
            OrganizationPlan::Enterprise => OrganizationQuota {
                organization_id: org_id.to_string(),
                max_topics: 10_000,
                max_partitions_per_topic: 256,
                max_total_partitions: 100_000,
                max_storage_bytes: i64::MAX, // Unlimited
                max_retention_days: 365,
                max_produce_bytes_per_sec: 1_073_741_824, // 1 GB/s
                max_consume_bytes_per_sec: 5_368_709_120, // 5 GB/s
                max_requests_per_sec: 100_000,
                max_consumer_groups: 10_000,
                max_schemas: 10_000,
                max_schema_versions_per_subject: 1_000,
                max_connections: 10_000,
            },
        }
    }
}

/// Summary of quota usage for an organization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuotaSummary {
    /// Organization ID.
    pub organization_id: String,
    /// Organization plan.
    pub plan: OrganizationPlan,
    /// Number of topics currently used.
    pub topics_used: i32,
    /// Maximum topics allowed.
    pub topics_limit: i32,
    /// Current storage usage in bytes.
    pub storage_used_bytes: i64,
    /// Maximum storage allowed in bytes.
    pub storage_limit_bytes: i64,
    /// Maximum produce rate (bytes/second).
    pub produce_rate_limit: i64,
    /// Maximum consume rate (bytes/second).
    pub consume_rate_limit: i64,
    /// Maximum requests per second.
    pub request_rate_limit: i32,
    /// Total bytes produced (for billing).
    pub produce_bytes_total: i64,
    /// Total bytes consumed (for billing).
    pub consume_bytes_total: i64,
}

/// Helper macro for quota enforcement in handlers.
///
/// Usage:
/// ```ignore
/// enforce_quota!(enforcer.check_produce(&ctx, bytes).await)?;
/// ```
#[macro_export]
macro_rules! enforce_quota {
    ($check:expr) => {{
        match $check {
            $crate::quota::QuotaCheck::Allowed => Ok(()),
            $crate::quota::QuotaCheck::Warning(reason) => {
                tracing::warn!("Quota warning: {}", reason);
                Ok(())
            }
            $crate::quota::QuotaCheck::Denied(reason) => {
                Err($crate::error::MetadataError::QuotaExceeded(reason))
            }
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Organization, OrganizationStatus};

    fn test_tenant_context() -> TenantContext {
        TenantContext {
            organization: Organization {
                id: "test-org".to_string(),
                name: "Test Org".to_string(),
                slug: "test-org".to_string(),
                plan: OrganizationPlan::Free,
                status: OrganizationStatus::Active,
                created_at: 0,
                settings: HashMap::new(),
            },
            api_key: None,
            quota: OrganizationQuota {
                organization_id: "test-org".to_string(),
                max_topics: 10,
                max_partitions_per_topic: 12,
                max_total_partitions: 100,
                max_storage_bytes: 10_000_000,
                max_retention_days: 7,
                max_produce_bytes_per_sec: 1_000_000, // 1 MB/s
                max_consume_bytes_per_sec: 5_000_000, // 5 MB/s
                max_requests_per_sec: 100,
                max_consumer_groups: 50,
                max_schemas: 100,
                max_schema_versions_per_subject: 100,
                max_connections: 100,
            },
            is_default: false,
        }
    }

    #[test]
    fn test_rate_limiter_basic() {
        let mut limiter = RateLimiter::new(Duration::from_secs(1), 100);

        // Should allow acquiring within limit
        assert!(limiter.try_acquire(50));
        assert_eq!(limiter.count, 50);

        // Should allow another acquire within limit
        assert!(limiter.try_acquire(30));
        assert_eq!(limiter.count, 80);

        // Should deny when exceeding limit
        assert!(!limiter.try_acquire(30));
        assert_eq!(limiter.count, 80);
    }

    #[test]
    fn test_rate_limiter_usage_percent() {
        let mut limiter = RateLimiter::new(Duration::from_secs(1), 100);

        limiter.try_acquire(80);
        assert!((limiter.usage_percent() - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_quota_check_helpers() {
        let allowed = QuotaCheck::Allowed;
        assert!(allowed.is_allowed());
        assert!(!allowed.is_denied());
        assert!(allowed.reason().is_none());

        let denied = QuotaCheck::Denied("quota exceeded".to_string());
        assert!(!denied.is_allowed());
        assert!(denied.is_denied());
        assert_eq!(denied.reason(), Some("quota exceeded"));

        let warning = QuotaCheck::Warning("approaching limit".to_string());
        assert!(warning.is_allowed());
        assert!(!warning.is_denied());
        assert_eq!(warning.reason(), Some("approaching limit"));
    }

    #[test]
    fn test_default_quotas_for_plan() {
        let free = QuotaEnforcer::<crate::SqliteMetadataStore>::default_quotas_for_plan(
            OrganizationPlan::Free,
            "test",
        );
        assert_eq!(free.max_topics, 10);
        assert_eq!(free.max_produce_bytes_per_sec, 10_485_760);

        let pro = QuotaEnforcer::<crate::SqliteMetadataStore>::default_quotas_for_plan(
            OrganizationPlan::Pro,
            "test",
        );
        assert_eq!(pro.max_topics, 100);
        assert_eq!(pro.max_produce_bytes_per_sec, 104_857_600);

        let enterprise = QuotaEnforcer::<crate::SqliteMetadataStore>::default_quotas_for_plan(
            OrganizationPlan::Enterprise,
            "test",
        );
        assert_eq!(enterprise.max_topics, 10_000);
    }
}
