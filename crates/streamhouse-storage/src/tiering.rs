//! Tiered Storage for StreamHouse
//!
//! Implements automatic storage tiering with hot/warm/cold/frozen tiers.
//! Segments are automatically moved between tiers based on age and access patterns,
//! optimizing for both performance and cost.
//!
//! ## Tier Overview
//!
//! | Tier   | Storage Class  | Access Pattern        | Typical Age    |
//! |--------|----------------|-----------------------|----------------|
//! | Hot    | STANDARD       | Frequently accessed   | < 24 hours     |
//! | Warm   | STANDARD_IA    | Infrequent access     | 1-30 days      |
//! | Cold   | GLACIER        | Rare access           | 30-365 days    |
//! | Frozen | DEEP_ARCHIVE   | Archive/compliance    | > 365 days     |
//!
//! ## Cost Estimation
//!
//! The module provides cost estimation based on S3-like pricing tiers,
//! helping operators understand the cost implications of their retention policies.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Errors specific to the tiered storage system.
#[derive(Debug, Error)]
pub enum TieringError {
    #[error("Segment not found: {0}")]
    SegmentNotFound(String),

    #[error("Invalid tier transition: {from} -> {to}")]
    InvalidTierTransition { from: String, to: String },

    #[error("Tiering disabled")]
    TieringDisabled,

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, TieringError>;

/// Storage tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum StorageTier {
    /// Frequently accessed, lowest latency, highest cost.
    Hot,
    /// Infrequently accessed, moderate latency, lower cost.
    Warm,
    /// Rarely accessed, high latency, low cost.
    Cold,
    /// Deep archive, very high latency, lowest cost.
    Frozen,
}

impl StorageTier {
    /// Get the S3-compatible storage class name for this tier.
    pub fn storage_class(&self) -> &'static str {
        match self {
            StorageTier::Hot => "STANDARD",
            StorageTier::Warm => "STANDARD_IA",
            StorageTier::Cold => "GLACIER",
            StorageTier::Frozen => "DEEP_ARCHIVE",
        }
    }

    /// Get the approximate cost per GB-month in USD for this tier.
    pub fn cost_per_gb_month(&self) -> f64 {
        match self {
            StorageTier::Hot => 0.023,
            StorageTier::Warm => 0.0125,
            StorageTier::Cold => 0.004,
            StorageTier::Frozen => 0.00099,
        }
    }

    /// Get the ordering index for comparison (lower = hotter).
    fn tier_order(&self) -> u8 {
        match self {
            StorageTier::Hot => 0,
            StorageTier::Warm => 1,
            StorageTier::Cold => 2,
            StorageTier::Frozen => 3,
        }
    }
}

impl std::fmt::Display for StorageTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageTier::Hot => write!(f, "hot"),
            StorageTier::Warm => write!(f, "warm"),
            StorageTier::Cold => write!(f, "cold"),
            StorageTier::Frozen => write!(f, "frozen"),
        }
    }
}

/// Configuration for tiered storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringConfig {
    /// Hours to keep segments in hot tier (default: 24).
    pub hot_retention_hours: u64,
    /// Days to keep segments in warm tier (default: 30).
    pub warm_retention_days: u64,
    /// Days to keep segments in cold tier (default: 365).
    pub cold_retention_days: u64,
    /// Storage class for hot tier.
    pub hot_storage_class: String,
    /// Storage class for warm tier.
    pub warm_storage_class: String,
    /// Storage class for cold tier.
    pub cold_storage_class: String,
    /// Whether auto-tiering is enabled.
    pub enable_auto_tiering: bool,
}

impl Default for TieringConfig {
    fn default() -> Self {
        Self {
            hot_retention_hours: 24,
            warm_retention_days: 30,
            cold_retention_days: 365,
            hot_storage_class: "STANDARD".to_string(),
            warm_storage_class: "STANDARD_IA".to_string(),
            cold_storage_class: "GLACIER".to_string(),
            enable_auto_tiering: true,
        }
    }
}

impl TieringConfig {
    /// Get hot retention as a Duration.
    pub fn hot_retention_duration(&self) -> Duration {
        Duration::from_secs(self.hot_retention_hours * 3600)
    }

    /// Get warm retention as a Duration.
    pub fn warm_retention_duration(&self) -> Duration {
        Duration::from_secs(self.warm_retention_days * 86400)
    }

    /// Get cold retention as a Duration.
    pub fn cold_retention_duration(&self) -> Duration {
        Duration::from_secs(self.cold_retention_days * 86400)
    }
}

/// Record of a tier transition for a segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierTransition {
    pub from: StorageTier,
    pub to: StorageTier,
    pub timestamp: i64,
    pub reason: String,
}

/// Metadata about a segment's tier placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentTier {
    pub segment_id: String,
    pub topic: String,
    pub partition: u32,
    pub current_tier: StorageTier,
    /// Creation timestamp in milliseconds since epoch.
    pub created_at: i64,
    /// Last access timestamp in milliseconds since epoch.
    pub last_accessed: i64,
    /// Size of the segment in bytes.
    pub size_bytes: u64,
    /// History of tier transitions.
    pub transition_history: Vec<TierTransition>,
}

/// Statistics about storage across all tiers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TieringStats {
    pub hot_segments: usize,
    pub warm_segments: usize,
    pub cold_segments: usize,
    pub frozen_segments: usize,
    pub hot_bytes: u64,
    pub warm_bytes: u64,
    pub cold_bytes: u64,
    pub frozen_bytes: u64,
    pub total_segments: usize,
    pub total_bytes: u64,
    pub estimated_monthly_cost_usd: f64,
}

/// Cost estimation breakdown by tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCostEstimate {
    pub hot_cost: f64,
    pub warm_cost: f64,
    pub cold_cost: f64,
    pub frozen_cost: f64,
    pub total_monthly_cost: f64,
    pub total_bytes: u64,
}

/// The tiered storage manager.
///
/// Manages segment placement across tiers and handles automatic transitions
/// based on age and access patterns.
pub struct StorageTiering {
    config: TieringConfig,
    segments: RwLock<HashMap<String, SegmentTier>>,
}

impl StorageTiering {
    /// Create a new StorageTiering manager with the given configuration.
    pub fn new(config: TieringConfig) -> Self {
        Self {
            config,
            segments: RwLock::new(HashMap::new()),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(TieringConfig::default())
    }

    /// Get the current configuration.
    pub fn config(&self) -> &TieringConfig {
        &self.config
    }

    /// Register a new segment in the tiering system.
    ///
    /// New segments always start in the Hot tier.
    pub async fn register_segment(
        &self,
        segment_id: String,
        topic: String,
        partition: u32,
        size_bytes: u64,
        created_at: i64,
    ) -> Result<()> {
        let mut segments = self.segments.write().await;

        let segment = SegmentTier {
            segment_id: segment_id.clone(),
            topic,
            partition,
            current_tier: StorageTier::Hot,
            created_at,
            last_accessed: created_at,
            size_bytes,
            transition_history: Vec::new(),
        };

        segments.insert(segment_id.clone(), segment);
        debug!("Registered segment {} in hot tier", segment_id);
        Ok(())
    }

    /// Check all segments for tier transitions based on age and configuration.
    ///
    /// Returns a list of (segment_id, old_tier, new_tier) transitions that were performed.
    pub async fn check_tier_transitions(
        &self,
        current_time_ms: i64,
    ) -> Result<Vec<(String, StorageTier, StorageTier)>> {
        if !self.config.enable_auto_tiering {
            return Err(TieringError::TieringDisabled);
        }

        let mut segments = self.segments.write().await;
        let mut transitions = Vec::new();

        let hot_threshold_ms = (self.config.hot_retention_hours * 3600 * 1000) as i64;
        let warm_threshold_ms = (self.config.warm_retention_days * 86400 * 1000) as i64;
        let cold_threshold_ms = (self.config.cold_retention_days * 86400 * 1000) as i64;

        for segment in segments.values_mut() {
            let age_ms = current_time_ms - segment.created_at;
            let new_tier = if age_ms > cold_threshold_ms {
                StorageTier::Frozen
            } else if age_ms > warm_threshold_ms {
                StorageTier::Cold
            } else if age_ms > hot_threshold_ms {
                StorageTier::Warm
            } else {
                StorageTier::Hot
            };

            if new_tier != segment.current_tier
                && new_tier.tier_order() > segment.current_tier.tier_order()
            {
                let old_tier = segment.current_tier;
                let reason = format!(
                    "Age-based transition: segment age {}ms exceeds threshold",
                    age_ms
                );

                segment.transition_history.push(TierTransition {
                    from: old_tier,
                    to: new_tier,
                    timestamp: current_time_ms,
                    reason,
                });

                info!(
                    "Transitioning segment {} from {} to {}",
                    segment.segment_id, old_tier, new_tier
                );

                segment.current_tier = new_tier;
                transitions.push((segment.segment_id.clone(), old_tier, new_tier));
            }
        }

        Ok(transitions)
    }

    /// Get the current tier for a segment.
    pub async fn get_tier(&self, segment_id: &str) -> Result<StorageTier> {
        let segments = self.segments.read().await;
        segments
            .get(segment_id)
            .map(|s| s.current_tier)
            .ok_or_else(|| TieringError::SegmentNotFound(segment_id.to_string()))
    }

    /// Promote a segment to a hotter tier (e.g., when it's accessed frequently).
    ///
    /// Only promotes upward (colder to hotter). Returns the new tier.
    pub async fn promote_segment(
        &self,
        segment_id: &str,
        target_tier: StorageTier,
        current_time_ms: i64,
    ) -> Result<StorageTier> {
        let mut segments = self.segments.write().await;

        let segment = segments
            .get_mut(segment_id)
            .ok_or_else(|| TieringError::SegmentNotFound(segment_id.to_string()))?;

        // Can only promote to a hotter tier
        if target_tier.tier_order() >= segment.current_tier.tier_order() {
            return Err(TieringError::InvalidTierTransition {
                from: segment.current_tier.to_string(),
                to: target_tier.to_string(),
            });
        }

        let old_tier = segment.current_tier;
        segment.transition_history.push(TierTransition {
            from: old_tier,
            to: target_tier,
            timestamp: current_time_ms,
            reason: "Promoted due to access pattern".to_string(),
        });

        segment.current_tier = target_tier;
        segment.last_accessed = current_time_ms;

        info!(
            "Promoted segment {} from {} to {}",
            segment_id, old_tier, target_tier
        );

        Ok(target_tier)
    }

    /// Get aggregate tiering statistics across all segments.
    pub async fn get_tiering_stats(&self) -> TieringStats {
        let segments = self.segments.read().await;
        let mut stats = TieringStats::default();

        for segment in segments.values() {
            match segment.current_tier {
                StorageTier::Hot => {
                    stats.hot_segments += 1;
                    stats.hot_bytes += segment.size_bytes;
                }
                StorageTier::Warm => {
                    stats.warm_segments += 1;
                    stats.warm_bytes += segment.size_bytes;
                }
                StorageTier::Cold => {
                    stats.cold_segments += 1;
                    stats.cold_bytes += segment.size_bytes;
                }
                StorageTier::Frozen => {
                    stats.frozen_segments += 1;
                    stats.frozen_bytes += segment.size_bytes;
                }
            }
        }

        stats.total_segments = segments.len();
        stats.total_bytes = stats.hot_bytes + stats.warm_bytes + stats.cold_bytes + stats.frozen_bytes;
        stats.estimated_monthly_cost_usd = self.calculate_monthly_cost(&stats);

        stats
    }

    /// Estimate storage cost based on current tier distribution.
    pub async fn estimate_storage_cost(&self) -> StorageCostEstimate {
        let stats = self.get_tiering_stats().await;
        let gb = 1_073_741_824.0_f64; // 1 GB in bytes

        let hot_cost = (stats.hot_bytes as f64 / gb) * StorageTier::Hot.cost_per_gb_month();
        let warm_cost = (stats.warm_bytes as f64 / gb) * StorageTier::Warm.cost_per_gb_month();
        let cold_cost = (stats.cold_bytes as f64 / gb) * StorageTier::Cold.cost_per_gb_month();
        let frozen_cost = (stats.frozen_bytes as f64 / gb) * StorageTier::Frozen.cost_per_gb_month();

        StorageCostEstimate {
            hot_cost,
            warm_cost,
            cold_cost,
            frozen_cost,
            total_monthly_cost: hot_cost + warm_cost + cold_cost + frozen_cost,
            total_bytes: stats.total_bytes,
        }
    }

    /// Get the full tier information for a segment.
    pub async fn get_segment_info(&self, segment_id: &str) -> Result<SegmentTier> {
        let segments = self.segments.read().await;
        segments
            .get(segment_id)
            .cloned()
            .ok_or_else(|| TieringError::SegmentNotFound(segment_id.to_string()))
    }

    /// Record an access to a segment (updates last_accessed timestamp).
    pub async fn record_access(&self, segment_id: &str, timestamp: i64) -> Result<()> {
        let mut segments = self.segments.write().await;
        let segment = segments
            .get_mut(segment_id)
            .ok_or_else(|| TieringError::SegmentNotFound(segment_id.to_string()))?;
        segment.last_accessed = timestamp;
        Ok(())
    }

    /// Remove a segment from the tiering system.
    pub async fn remove_segment(&self, segment_id: &str) -> Result<()> {
        let mut segments = self.segments.write().await;
        if segments.remove(segment_id).is_none() {
            warn!("Attempted to remove non-existent segment: {}", segment_id);
        }
        Ok(())
    }

    /// Calculate monthly cost from stats.
    fn calculate_monthly_cost(&self, stats: &TieringStats) -> f64 {
        let gb = 1_073_741_824.0_f64;
        (stats.hot_bytes as f64 / gb) * StorageTier::Hot.cost_per_gb_month()
            + (stats.warm_bytes as f64 / gb) * StorageTier::Warm.cost_per_gb_month()
            + (stats.cold_bytes as f64 / gb) * StorageTier::Cold.cost_per_gb_month()
            + (stats.frozen_bytes as f64 / gb) * StorageTier::Frozen.cost_per_gb_month()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tiering() -> StorageTiering {
        StorageTiering::with_defaults()
    }

    fn ms_from_hours(hours: u64) -> i64 {
        (hours * 3600 * 1000) as i64
    }

    fn ms_from_days(days: u64) -> i64 {
        (days * 86400 * 1000) as i64
    }

    // Test 1: Register segment starts in Hot tier
    #[tokio::test]
    async fn test_register_segment_hot_tier() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-1".to_string(), "topic-a".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        let tier = tiering.get_tier("seg-1").await.unwrap();
        assert_eq!(tier, StorageTier::Hot);
    }

    // Test 2: Auto-transition from Hot to Warm after threshold
    #[tokio::test]
    async fn test_hot_to_warm_transition() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-2".to_string(), "topic-a".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        // After 25 hours (> 24h hot retention), should transition to warm
        let future = now + ms_from_hours(25);
        let transitions = tiering.check_tier_transitions(future).await.unwrap();

        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].0, "seg-2");
        assert_eq!(transitions[0].1, StorageTier::Hot);
        assert_eq!(transitions[0].2, StorageTier::Warm);

        let tier = tiering.get_tier("seg-2").await.unwrap();
        assert_eq!(tier, StorageTier::Warm);
    }

    // Test 3: Auto-transition from Warm to Cold after threshold
    #[tokio::test]
    async fn test_warm_to_cold_transition() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-3".to_string(), "topic-a".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        // After 31 days (> 30d warm retention), should be in cold
        let future = now + ms_from_days(31);
        let transitions = tiering.check_tier_transitions(future).await.unwrap();

        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].2, StorageTier::Cold);

        let tier = tiering.get_tier("seg-3").await.unwrap();
        assert_eq!(tier, StorageTier::Cold);
    }

    // Test 4: Auto-transition to Frozen after cold retention
    #[tokio::test]
    async fn test_cold_to_frozen_transition() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-4".to_string(), "topic-a".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        // After 366 days (> 365d cold retention), should be frozen
        let future = now + ms_from_days(366);
        let transitions = tiering.check_tier_transitions(future).await.unwrap();

        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].2, StorageTier::Frozen);

        let tier = tiering.get_tier("seg-4").await.unwrap();
        assert_eq!(tier, StorageTier::Frozen);
    }

    // Test 5: Promote segment from Cold back to Hot
    #[tokio::test]
    async fn test_promote_segment() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-5".to_string(), "topic-a".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        // Move to cold first
        let future = now + ms_from_days(31);
        tiering.check_tier_transitions(future).await.unwrap();
        assert_eq!(tiering.get_tier("seg-5").await.unwrap(), StorageTier::Cold);

        // Promote back to hot
        let result = tiering
            .promote_segment("seg-5", StorageTier::Hot, future)
            .await
            .unwrap();
        assert_eq!(result, StorageTier::Hot);

        let info = tiering.get_segment_info("seg-5").await.unwrap();
        assert_eq!(info.transition_history.len(), 2); // cold, then hot
    }

    // Test 6: Cannot promote to same or colder tier
    #[tokio::test]
    async fn test_promote_invalid_direction() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-6".to_string(), "topic-a".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        // Cannot promote Hot to Warm (that's demotion)
        let result = tiering
            .promote_segment("seg-6", StorageTier::Warm, now)
            .await;
        assert!(result.is_err());

        // Cannot promote Hot to Hot (same tier)
        let result = tiering
            .promote_segment("seg-6", StorageTier::Hot, now)
            .await;
        assert!(result.is_err());
    }

    // Test 7: Tiering stats
    #[tokio::test]
    async fn test_tiering_stats() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;
        let gb = 1_073_741_824_u64;

        // Register segments at different ages
        tiering
            .register_segment("hot-seg".to_string(), "t1".to_string(), 0, gb, now)
            .await
            .unwrap();
        tiering
            .register_segment(
                "warm-seg".to_string(),
                "t1".to_string(),
                1,
                2 * gb,
                now - ms_from_days(2),
            )
            .await
            .unwrap();
        tiering
            .register_segment(
                "cold-seg".to_string(),
                "t1".to_string(),
                2,
                3 * gb,
                now - ms_from_days(31),
            )
            .await
            .unwrap();

        // Transition to correct tiers
        tiering.check_tier_transitions(now).await.unwrap();

        let stats = tiering.get_tiering_stats().await;
        assert_eq!(stats.total_segments, 3);
        assert_eq!(stats.hot_segments, 1);
        assert_eq!(stats.warm_segments, 1);
        assert_eq!(stats.cold_segments, 1);
        assert_eq!(stats.hot_bytes, gb);
        assert_eq!(stats.warm_bytes, 2 * gb);
        assert_eq!(stats.cold_bytes, 3 * gb);
    }

    // Test 8: Cost estimation
    #[tokio::test]
    async fn test_cost_estimation() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;
        let gb = 1_073_741_824_u64;

        // 1 GB in hot
        tiering
            .register_segment("seg-8".to_string(), "t1".to_string(), 0, gb, now)
            .await
            .unwrap();

        let cost = tiering.estimate_storage_cost().await;
        assert!((cost.hot_cost - 0.023).abs() < 0.001);
        assert_eq!(cost.warm_cost, 0.0);
        assert!((cost.total_monthly_cost - 0.023).abs() < 0.001);
    }

    // Test 9: Segment not found
    #[tokio::test]
    async fn test_segment_not_found() {
        let tiering = default_tiering();

        let result = tiering.get_tier("nonexistent").await;
        assert!(result.is_err());

        let result = tiering.promote_segment("nonexistent", StorageTier::Hot, 0).await;
        assert!(result.is_err());
    }

    // Test 10: Auto-tiering disabled
    #[tokio::test]
    async fn test_auto_tiering_disabled() {
        let config = TieringConfig {
            enable_auto_tiering: false,
            ..TieringConfig::default()
        };
        let tiering = StorageTiering::new(config);
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-10".to_string(), "t1".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        let result = tiering.check_tier_transitions(now + ms_from_days(100)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TieringError::TieringDisabled => {}
            e => panic!("Expected TieringDisabled, got {:?}", e),
        }

        // Segment should still be hot
        assert_eq!(tiering.get_tier("seg-10").await.unwrap(), StorageTier::Hot);
    }

    // Test 11: Multiple segments transition simultaneously
    #[tokio::test]
    async fn test_multiple_segment_transitions() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        // All created at the same time
        for i in 0..5 {
            tiering
                .register_segment(
                    format!("seg-11-{}", i),
                    "topic".to_string(),
                    i,
                    1_000_000,
                    now,
                )
                .await
                .unwrap();
        }

        let future = now + ms_from_hours(25);
        let transitions = tiering.check_tier_transitions(future).await.unwrap();
        assert_eq!(transitions.len(), 5);

        for (_, from, to) in &transitions {
            assert_eq!(*from, StorageTier::Hot);
            assert_eq!(*to, StorageTier::Warm);
        }
    }

    // Test 12: Record access and remove segment
    #[tokio::test]
    async fn test_record_access_and_remove() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-12".to_string(), "t1".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        // Record access
        tiering.record_access("seg-12", now + 1000).await.unwrap();
        let info = tiering.get_segment_info("seg-12").await.unwrap();
        assert_eq!(info.last_accessed, now + 1000);

        // Remove segment
        tiering.remove_segment("seg-12").await.unwrap();
        assert!(tiering.get_tier("seg-12").await.is_err());
    }

    // Test 13: Transition history is recorded
    #[tokio::test]
    async fn test_transition_history() {
        let tiering = default_tiering();
        let now = 1700000000000_i64;

        tiering
            .register_segment("seg-13".to_string(), "t1".to_string(), 0, 64_000_000, now)
            .await
            .unwrap();

        // Transition to warm
        let future1 = now + ms_from_hours(25);
        tiering.check_tier_transitions(future1).await.unwrap();

        // Transition to cold
        let future2 = now + ms_from_days(31);
        tiering.check_tier_transitions(future2).await.unwrap();

        let info = tiering.get_segment_info("seg-13").await.unwrap();
        assert_eq!(info.transition_history.len(), 2);
        assert_eq!(info.transition_history[0].from, StorageTier::Hot);
        assert_eq!(info.transition_history[0].to, StorageTier::Warm);
        assert_eq!(info.transition_history[1].from, StorageTier::Warm);
        assert_eq!(info.transition_history[1].to, StorageTier::Cold);
    }

    // Test 14: StorageTier display and storage class
    #[test]
    fn test_storage_tier_properties() {
        assert_eq!(StorageTier::Hot.to_string(), "hot");
        assert_eq!(StorageTier::Warm.to_string(), "warm");
        assert_eq!(StorageTier::Cold.to_string(), "cold");
        assert_eq!(StorageTier::Frozen.to_string(), "frozen");

        assert_eq!(StorageTier::Hot.storage_class(), "STANDARD");
        assert_eq!(StorageTier::Warm.storage_class(), "STANDARD_IA");
        assert_eq!(StorageTier::Cold.storage_class(), "GLACIER");
        assert_eq!(StorageTier::Frozen.storage_class(), "DEEP_ARCHIVE");

        // Costs should decrease as tier gets colder
        assert!(StorageTier::Hot.cost_per_gb_month() > StorageTier::Warm.cost_per_gb_month());
        assert!(StorageTier::Warm.cost_per_gb_month() > StorageTier::Cold.cost_per_gb_month());
        assert!(StorageTier::Cold.cost_per_gb_month() > StorageTier::Frozen.cost_per_gb_month());
    }
}
