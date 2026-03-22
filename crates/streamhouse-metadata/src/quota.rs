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
use crate::{
    ApiKey, MetadataStore, OrganizationPlan, OrganizationQuota, OrganizationStatus, Result,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
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

// ============================================================================
// Token Bucket Rate Limiter (lock-free, CAS-based)
// ============================================================================

/// Precision multiplier: store tokens as integer with 1000x precision.
const TOKEN_PRECISION: u64 = 1000;

/// Token bucket rate limiter using atomic CAS for lock-free operation.
///
/// Tokens are stored with 1000x precision as u64 to enable atomic operations.
/// Refill happens lazily on each acquire/query call.
struct TokenBucket {
    /// Current tokens (stored as amount * TOKEN_PRECISION)
    tokens: AtomicU64,
    /// Last refill time in microseconds (Instant-based offset)
    last_refill_us: AtomicU64,
    /// Rate in tokens per second
    rate: AtomicI64,
    /// Burst capacity (max tokens)
    burst: AtomicI64,
    /// Reference instant for computing elapsed time
    epoch: Instant,
}

impl TokenBucket {
    /// Create a new token bucket.
    /// `rate` = tokens per second, `burst` = max tokens (defaults to 2x rate).
    fn new(rate: i64) -> Self {
        let burst = rate * 2;
        Self {
            tokens: AtomicU64::new((burst as u64) * TOKEN_PRECISION),
            last_refill_us: AtomicU64::new(0),
            rate: AtomicI64::new(rate),
            burst: AtomicI64::new(burst),
            epoch: Instant::now(),
        }
    }

    #[cfg(test)]
    fn with_burst(rate: i64, burst: i64) -> Self {
        Self {
            tokens: AtomicU64::new((burst as u64) * TOKEN_PRECISION),
            last_refill_us: AtomicU64::new(0),
            rate: AtomicI64::new(rate),
            burst: AtomicI64::new(burst),
            epoch: Instant::now(),
        }
    }

    /// Get current time as microseconds since epoch Instant.
    fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    /// Refill tokens based on elapsed time (CAS loop).
    fn refill(&self) {
        let now = self.now_us();
        let last = self.last_refill_us.load(Ordering::Acquire);
        let elapsed_us = now.saturating_sub(last);
        if elapsed_us == 0 {
            return;
        }

        let rate = self.rate.load(Ordering::Acquire);
        let burst = self.burst.load(Ordering::Acquire);
        if rate <= 0 || burst <= 0 {
            return;
        }

        let tokens_to_add =
            ((rate as f64) * (elapsed_us as f64 / 1_000_000.0) * TOKEN_PRECISION as f64) as u64;
        if tokens_to_add == 0 {
            return;
        }

        let max_tokens = (burst as u64) * TOKEN_PRECISION;

        // CAS loop to add tokens
        loop {
            let current = self.tokens.load(Ordering::Acquire);
            let new_tokens = std::cmp::min(current.saturating_add(tokens_to_add), max_tokens);
            match self.tokens.compare_exchange_weak(
                current,
                new_tokens,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(_) => continue,
            }
        }

        // Update last refill time
        self.last_refill_us.store(now, Ordering::Release);
    }

    /// Try to acquire `amount` tokens. Returns true if acquired.
    fn try_acquire(&self, amount: i64) -> bool {
        if amount <= 0 {
            return true;
        }
        self.refill();

        let required = (amount as u64) * TOKEN_PRECISION;
        loop {
            let current = self.tokens.load(Ordering::Acquire);
            if current < required {
                return false;
            }
            match self.tokens.compare_exchange_weak(
                current,
                current - required,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
    }

    /// Compute how long until `amount` tokens are available.
    /// Returns Duration::ZERO if tokens are already available.
    fn time_until_available(&self, amount: i64) -> Duration {
        if amount <= 0 {
            return Duration::ZERO;
        }
        self.refill();

        let required = (amount as u64) * TOKEN_PRECISION;
        let current = self.tokens.load(Ordering::Acquire);
        if current >= required {
            return Duration::ZERO;
        }

        let deficit = required - current;
        let rate = self.rate.load(Ordering::Acquire);
        if rate <= 0 {
            return Duration::from_secs(3600); // Effectively blocked
        }

        let seconds = (deficit as f64) / ((rate as u64 * TOKEN_PRECISION) as f64);
        Duration::from_secs_f64(seconds)
    }

    /// Get current usage as a percentage of burst capacity.
    fn usage_percent(&self) -> f64 {
        self.refill();
        let burst = self.burst.load(Ordering::Acquire);
        if burst <= 0 {
            return 100.0;
        }
        let max_tokens = (burst as u64) * TOKEN_PRECISION;
        let current = self.tokens.load(Ordering::Acquire);
        // usage = how much of the burst has been consumed
        let used = max_tokens.saturating_sub(current);
        (used as f64 / max_tokens as f64) * 100.0
    }

    /// Update the rate (e.g., when quota changes).
    fn update_rate(&self, new_rate: i64) {
        let new_burst = new_rate * 2;
        self.rate.store(new_rate, Ordering::Release);
        self.burst.store(new_burst, Ordering::Release);
    }
}

// ============================================================================
// Rate Limit State
// ============================================================================

/// In-memory rate limiter state for an organization.
struct OrgRateLimitState {
    /// Produce bytes per second limiter.
    produce_limiter: TokenBucket,
    /// Consume bytes per second limiter.
    consume_limiter: TokenBucket,
    /// Requests per second limiter.
    request_limiter: TokenBucket,
    /// Last time this org had activity (for eviction).
    last_activity: AtomicU64,
}

impl OrgRateLimitState {
    fn new(quota: &OrganizationQuota) -> Self {
        Self {
            produce_limiter: TokenBucket::new(quota.max_produce_bytes_per_sec),
            consume_limiter: TokenBucket::new(quota.max_consume_bytes_per_sec),
            request_limiter: TokenBucket::new(quota.max_requests_per_sec as i64),
            last_activity: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        }
    }

    fn touch(&self) {
        self.last_activity.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Release,
        );
    }

    fn last_activity_secs(&self) -> u64 {
        self.last_activity.load(Ordering::Acquire)
    }

    /// Update rates if quota changed.
    fn update_rates(&self, quota: &OrganizationQuota) {
        self.produce_limiter
            .update_rate(quota.max_produce_bytes_per_sec);
        self.consume_limiter
            .update_rate(quota.max_consume_bytes_per_sec);
        self.request_limiter
            .update_rate(quota.max_requests_per_sec as i64);
    }
}

/// Per-API-key rate limiter state.
struct KeyRateLimitState {
    produce_limiter: Option<TokenBucket>,
    consume_limiter: Option<TokenBucket>,
    request_limiter: Option<TokenBucket>,
}

impl KeyRateLimitState {
    fn from_api_key(key: &ApiKey) -> Option<Self> {
        let has_limits = key.max_requests_per_sec.is_some()
            || key.max_produce_bytes_per_sec.is_some()
            || key.max_consume_bytes_per_sec.is_some();
        if !has_limits {
            return None;
        }
        Some(Self {
            produce_limiter: key.max_produce_bytes_per_sec.map(TokenBucket::new),
            consume_limiter: key.max_consume_bytes_per_sec.map(TokenBucket::new),
            request_limiter: key.max_requests_per_sec.map(|r| TokenBucket::new(r.into())),
        })
    }
}

// ============================================================================
// QuotaEnforcer
// ============================================================================

/// Quota enforcer for multi-tenant operations.
///
/// Enforces resource limits based on organization plans and tracks usage.
/// Uses a combination of in-memory rate limiting (for high-frequency checks)
/// and persistent storage (for durable limits like topic count).
pub struct QuotaEnforcer<S: MetadataStore + ?Sized> {
    store: Arc<S>,
    /// In-memory rate limiters per organization.
    rate_limiters: RwLock<HashMap<String, OrgRateLimitState>>,
    /// Per-API-key rate limiters, keyed by (org_id, key_id).
    key_limiters: RwLock<HashMap<(String, String), KeyRateLimitState>>,
}

impl<S: MetadataStore + ?Sized + 'static> QuotaEnforcer<S> {
    /// Create a new quota enforcer.
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            rate_limiters: RwLock::new(HashMap::new()),
            key_limiters: RwLock::new(HashMap::new()),
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

    /// Ensure per-key limiter exists (if the key has custom limits).
    async fn ensure_key_limiter(&self, org_id: &str, key: &ApiKey) {
        let map_key = (org_id.to_string(), key.id.clone());

        // Fast path
        {
            let limiters = self.key_limiters.read().await;
            if limiters.contains_key(&map_key) {
                return;
            }
        }

        // Slow path
        if let Some(state) = KeyRateLimitState::from_api_key(key) {
            let mut limiters = self.key_limiters.write().await;
            limiters.entry(map_key).or_insert(state);
        }
    }

    /// Check if a topic can be created.
    pub async fn check_topic_creation(
        &self,
        ctx: &TenantContext,
        partition_count: u32,
    ) -> Result<QuotaCheck> {
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        let topics = self.store.list_topics_for_org(&ctx.organization.id).await?;
        let current_topics = topics.len() as i32;

        if current_topics >= ctx.quota.max_topics {
            return Ok(QuotaCheck::Denied(format!(
                "Topic limit reached ({}/{})",
                current_topics, ctx.quota.max_topics
            )));
        }

        if partition_count as i32 > ctx.quota.max_partitions_per_topic {
            return Ok(QuotaCheck::Denied(format!(
                "Partition count {} exceeds limit of {}",
                partition_count, ctx.quota.max_partitions_per_topic
            )));
        }

        let usage_percent = (current_topics as f64 / ctx.quota.max_topics as f64) * 100.0;
        if usage_percent > 80.0 {
            return Ok(QuotaCheck::Warning(format!(
                "Topic usage at {:.0}% ({}/{})",
                usage_percent, current_topics, ctx.quota.max_topics
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if a consumer group can be created.
    pub async fn check_consumer_group_creation(&self, ctx: &TenantContext) -> Result<QuotaCheck> {
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        let groups = self
            .store
            .list_consumer_groups_for_org(&ctx.organization.id)
            .await?;
        let current_groups = groups.len() as i32;

        if current_groups >= ctx.quota.max_consumer_groups {
            return Ok(QuotaCheck::Denied(format!(
                "Consumer group limit reached ({}/{})",
                current_groups, ctx.quota.max_consumer_groups
            )));
        }

        let usage_percent = (current_groups as f64 / ctx.quota.max_consumer_groups as f64) * 100.0;
        if usage_percent > 80.0 {
            return Ok(QuotaCheck::Warning(format!(
                "Consumer group usage at {:.0}% ({}/{})",
                usage_percent, current_groups, ctx.quota.max_consumer_groups
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if a new connection is allowed.
    pub async fn check_connection(
        &self,
        ctx: &TenantContext,
        current_connections: i32,
    ) -> Result<QuotaCheck> {
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        if current_connections >= ctx.quota.max_connections {
            return Ok(QuotaCheck::Denied(format!(
                "Connection limit reached ({}/{})",
                current_connections, ctx.quota.max_connections
            )));
        }

        let usage_percent = (current_connections as f64 / ctx.quota.max_connections as f64) * 100.0;
        if usage_percent > 80.0 {
            return Ok(QuotaCheck::Warning(format!(
                "Connection usage at {:.0}% ({}/{})",
                usage_percent, current_connections, ctx.quota.max_connections
            )));
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if produce bytes can be written.
    ///
    /// Checks org-level limit first, then optional per-key limit.
    pub async fn check_produce(
        &self,
        ctx: &TenantContext,
        bytes: i64,
        api_key: Option<&ApiKey>,
    ) -> Result<QuotaCheck> {
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        self.get_or_create_limiter(ctx).await?;

        // Org-level check
        {
            let limiters = self.rate_limiters.read().await;
            let state = limiters.get(&ctx.organization.id).unwrap();
            state.touch();

            if !state.produce_limiter.try_acquire(bytes) {
                return Ok(QuotaCheck::Denied(format!(
                    "Produce rate limit exceeded ({} bytes/s)",
                    ctx.quota.max_produce_bytes_per_sec
                )));
            }
        }

        // Per-key check
        if let Some(key) = api_key {
            if let Some(max_produce) = key.max_produce_bytes_per_sec {
                self.ensure_key_limiter(&ctx.organization.id, key).await;
                let map_key = (ctx.organization.id.clone(), key.id.clone());
                let limiters = self.key_limiters.read().await;
                if let Some(state) = limiters.get(&map_key) {
                    if let Some(ref limiter) = state.produce_limiter {
                        if !limiter.try_acquire(bytes) {
                            return Ok(QuotaCheck::Denied(format!(
                                "API key produce rate limit exceeded ({} bytes/s)",
                                max_produce
                            )));
                        }
                    }
                }
            }
        }

        // Warning if approaching org limit
        {
            let limiters = self.rate_limiters.read().await;
            let state = limiters.get(&ctx.organization.id).unwrap();
            let usage_percent = state.produce_limiter.usage_percent();
            if usage_percent > 80.0 {
                return Ok(QuotaCheck::Warning(format!(
                    "Produce rate at {:.0}% of limit",
                    usage_percent
                )));
            }
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if consume bytes can be read.
    pub async fn check_consume(
        &self,
        ctx: &TenantContext,
        bytes: i64,
        api_key: Option<&ApiKey>,
    ) -> Result<QuotaCheck> {
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        self.get_or_create_limiter(ctx).await?;

        // Org-level check
        {
            let limiters = self.rate_limiters.read().await;
            let state = limiters.get(&ctx.organization.id).unwrap();
            state.touch();

            if !state.consume_limiter.try_acquire(bytes) {
                return Ok(QuotaCheck::Denied(format!(
                    "Consume rate limit exceeded ({} bytes/s)",
                    ctx.quota.max_consume_bytes_per_sec
                )));
            }
        }

        // Per-key check
        if let Some(key) = api_key {
            if let Some(max_consume) = key.max_consume_bytes_per_sec {
                self.ensure_key_limiter(&ctx.organization.id, key).await;
                let map_key = (ctx.organization.id.clone(), key.id.clone());
                let limiters = self.key_limiters.read().await;
                if let Some(state) = limiters.get(&map_key) {
                    if let Some(ref limiter) = state.consume_limiter {
                        if !limiter.try_acquire(bytes) {
                            return Ok(QuotaCheck::Denied(format!(
                                "API key consume rate limit exceeded ({} bytes/s)",
                                max_consume
                            )));
                        }
                    }
                }
            }
        }

        // Warning
        {
            let limiters = self.rate_limiters.read().await;
            let state = limiters.get(&ctx.organization.id).unwrap();
            let usage_percent = state.consume_limiter.usage_percent();
            if usage_percent > 80.0 {
                return Ok(QuotaCheck::Warning(format!(
                    "Consume rate at {:.0}% of limit",
                    usage_percent
                )));
            }
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Check if a request can be made (generic request rate check).
    pub async fn check_request(
        &self,
        ctx: &TenantContext,
        api_key: Option<&ApiKey>,
    ) -> Result<QuotaCheck> {
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

        self.get_or_create_limiter(ctx).await?;

        // Org-level check
        {
            let limiters = self.rate_limiters.read().await;
            let state = limiters.get(&ctx.organization.id).unwrap();
            state.touch();

            if !state.request_limiter.try_acquire(1) {
                return Ok(QuotaCheck::Denied(format!(
                    "Request rate limit exceeded ({}/s)",
                    ctx.quota.max_requests_per_sec
                )));
            }
        }

        // Per-key check
        if let Some(key) = api_key {
            if let Some(max_requests) = key.max_requests_per_sec {
                self.ensure_key_limiter(&ctx.organization.id, key).await;
                let map_key = (ctx.organization.id.clone(), key.id.clone());
                let limiters = self.key_limiters.read().await;
                if let Some(state) = limiters.get(&map_key) {
                    if let Some(ref limiter) = state.request_limiter {
                        if !limiter.try_acquire(1) {
                            return Ok(QuotaCheck::Denied(format!(
                                "API key request rate limit exceeded ({}/s)",
                                max_requests
                            )));
                        }
                    }
                }
            }
        }

        Ok(QuotaCheck::Allowed)
    }

    /// Compute throttle time for Kafka protocol (how long client should wait).
    /// Returns 0 if not throttled.
    pub async fn throttle_time_ms(&self, ctx: &TenantContext, api_key: Option<&ApiKey>) -> i32 {
        self.throttle_time_ms_for(ctx, api_key, None, None).await
    }

    /// Compute throttle time considering request rate, produce bytes, and consume bytes.
    pub async fn throttle_time_ms_for(
        &self,
        ctx: &TenantContext,
        api_key: Option<&ApiKey>,
        produce_bytes: Option<i64>,
        consume_bytes: Option<i64>,
    ) -> i32 {
        self.get_or_create_limiter(ctx).await.ok();

        let mut max_wait = Duration::ZERO;

        let limiters = self.rate_limiters.read().await;
        if let Some(state) = limiters.get(&ctx.organization.id) {
            let request_wait = state.request_limiter.time_until_available(1);
            if request_wait > max_wait {
                max_wait = request_wait;
            }

            if let Some(bytes) = produce_bytes {
                let produce_wait = state.produce_limiter.time_until_available(bytes);
                if produce_wait > max_wait {
                    max_wait = produce_wait;
                }
            }

            if let Some(bytes) = consume_bytes {
                let consume_wait = state.consume_limiter.time_until_available(bytes);
                if consume_wait > max_wait {
                    max_wait = consume_wait;
                }
            }
        }

        // Also check per-key
        if let Some(key) = api_key {
            if key.max_requests_per_sec.is_some() {
                let map_key = (ctx.organization.id.clone(), key.id.clone());
                let key_limiters = self.key_limiters.read().await;
                if let Some(state) = key_limiters.get(&map_key) {
                    if let Some(ref limiter) = state.request_limiter {
                        let wait = limiter.time_until_available(1);
                        if wait > max_wait {
                            max_wait = wait;
                        }
                    }
                }
            }
        }

        max_wait.as_millis() as i32
    }

    /// Check storage usage.
    pub async fn check_storage(&self, ctx: &TenantContext) -> Result<QuotaCheck> {
        if ctx.organization.status != OrganizationStatus::Active {
            return Ok(QuotaCheck::Denied(format!(
                "Organization is {}",
                ctx.organization.status
            )));
        }

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
        let topics = self.store.list_topics_for_org(&ctx.organization.id).await?;
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

    /// Resolve a TenantContext for quota checking by looking up the real org from the store.
    /// Falls back to Free plan defaults if the org is not found.
    pub async fn resolve_tenant_context(&self, org_id: &str) -> TenantContext {
        match self.store.get_organization(org_id).await {
            Ok(Some(org)) => {
                let quota = self
                    .store
                    .get_organization_quota(org_id)
                    .await
                    .unwrap_or_else(|_| Self::default_quotas_for_plan(org.plan, org_id));
                TenantContext {
                    organization: org,
                    api_key: None,
                    quota,
                    is_default: org_id == crate::TEST_ORG_ID,
                }
            }
            _ => {
                // Org not found — use Free plan defaults
                TenantContext {
                    organization: crate::Organization {
                        id: org_id.to_string(),
                        name: String::new(),
                        slug: String::new(),
                        plan: OrganizationPlan::Free,
                        status: OrganizationStatus::Active,
                        created_at: 0,
                        settings: std::collections::HashMap::new(),
                        external_id: None,
                        deployment_mode: Default::default(),
                    },
                    api_key: None,
                    quota: Self::default_quotas_for_plan(OrganizationPlan::Free, org_id),
                    is_default: false,
                }
            }
        }
    }

    /// Spawn a background task that periodically refreshes quotas and evicts
    /// idle org limiters. Returns a JoinHandle that can be dropped to stop.
    pub fn spawn_quota_refresh_task(
        self: &Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let enforcer = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                // Collect active org IDs
                let org_ids: Vec<String> = {
                    let limiters = enforcer.rate_limiters.read().await;
                    limiters.keys().cloned().collect()
                };

                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let mut to_evict = Vec::new();

                for org_id in &org_ids {
                    // Check if inactive for >5 minutes
                    let last_activity = {
                        let limiters = enforcer.rate_limiters.read().await;
                        limiters
                            .get(org_id)
                            .map(|s| s.last_activity_secs())
                            .unwrap_or(0)
                    };

                    if now_secs.saturating_sub(last_activity) > 300 {
                        to_evict.push(org_id.clone());
                        continue;
                    }

                    // Reload quota from store and update rates
                    if let Ok(quota) = enforcer.store.get_organization_quota(org_id).await {
                        let limiters = enforcer.rate_limiters.read().await;
                        if let Some(state) = limiters.get(org_id) {
                            state.update_rates(&quota);
                        }
                    }
                }

                // Evict inactive orgs
                if !to_evict.is_empty() {
                    let mut limiters = enforcer.rate_limiters.write().await;
                    for org_id in &to_evict {
                        limiters.remove(org_id);
                    }
                    // Also evict per-key limiters for those orgs
                    let mut key_limiters = enforcer.key_limiters.write().await;
                    key_limiters.retain(|(org_id, _), _| !to_evict.contains(org_id));

                    tracing::debug!("Evicted {} inactive org rate limiters", to_evict.len());
                }
            }
        })
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
/// enforce_quota!(enforcer.check_produce(&ctx, bytes, None).await)?;
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

    #[allow(dead_code)]
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
                external_id: None,
                deployment_mode: Default::default(),
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
    fn test_token_bucket_basic() {
        let bucket = TokenBucket::with_burst(100, 100);

        // Should allow acquiring within limit
        assert!(bucket.try_acquire(50));
        assert!(bucket.try_acquire(30));
        // Should deny when exceeding limit
        assert!(!bucket.try_acquire(30));
    }

    #[test]
    fn test_token_bucket_burst() {
        // Default burst = 2x rate
        let bucket = TokenBucket::new(100);
        // Should allow up to 200 (2x burst)
        assert!(bucket.try_acquire(200));
        assert!(!bucket.try_acquire(1));
    }

    #[test]
    fn test_token_bucket_time_until_available() {
        let bucket = TokenBucket::with_burst(1000, 100);
        // Drain all tokens
        assert!(bucket.try_acquire(100));
        // Should need to wait
        let wait = bucket.time_until_available(10);
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_millis(15)); // ~10ms at 1000/s
    }

    #[test]
    fn test_token_bucket_zero_amount() {
        let bucket = TokenBucket::new(100);
        assert!(bucket.try_acquire(0));
        assert_eq!(bucket.time_until_available(0), Duration::ZERO);
    }

    #[test]
    fn test_token_bucket_usage_percent() {
        let bucket = TokenBucket::with_burst(100, 100);
        assert!(bucket.try_acquire(80));
        let usage = bucket.usage_percent();
        assert!(usage > 70.0 && usage < 90.0);
    }

    #[test]
    fn test_token_bucket_update_rate() {
        let bucket = TokenBucket::new(100);
        bucket.update_rate(200);
        assert_eq!(bucket.rate.load(Ordering::Acquire), 200);
        assert_eq!(bucket.burst.load(Ordering::Acquire), 400);
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

    #[test]
    fn test_key_rate_limit_state_none_when_no_limits() {
        let key = ApiKey {
            id: "k1".to_string(),
            organization_id: "org1".to_string(),
            name: "test".to_string(),
            key_prefix: "sk_live_".to_string(),
            permissions: vec![],
            scopes: vec![],
            expires_at: None,
            last_used_at: None,
            created_at: 0,
            created_by: None,
            max_requests_per_sec: None,
            max_produce_bytes_per_sec: None,
            max_consume_bytes_per_sec: None,
        };
        assert!(KeyRateLimitState::from_api_key(&key).is_none());
    }

    #[test]
    fn test_key_rate_limit_state_some_when_limits() {
        let key = ApiKey {
            id: "k1".to_string(),
            organization_id: "org1".to_string(),
            name: "test".to_string(),
            key_prefix: "sk_live_".to_string(),
            permissions: vec![],
            scopes: vec![],
            expires_at: None,
            last_used_at: None,
            created_at: 0,
            created_by: None,
            max_requests_per_sec: Some(50),
            max_produce_bytes_per_sec: Some(500_000),
            max_consume_bytes_per_sec: None,
        };
        let state = KeyRateLimitState::from_api_key(&key).unwrap();
        assert!(state.request_limiter.is_some());
        assert!(state.produce_limiter.is_some());
        assert!(state.consume_limiter.is_none());
    }
}
