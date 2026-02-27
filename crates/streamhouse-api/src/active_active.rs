//! Multi-Region Active-Active Coordination
//!
//! Implements active-active multi-region coordination for StreamHouse.
//! Multiple regions can accept writes simultaneously, with conflict resolution
//! handling divergent writes to the same key.
//!
//! ## Architecture
//!
//! Each region maintains a version vector tracking the latest version seen
//! from every region. When regions sync, they exchange records written since
//! the last sync and resolve any conflicts using the configured strategy.
//!
//! ## Conflict Resolution Strategies
//!
//! - **LastWriterWins**: Uses timestamps to pick the most recent write
//! - **RegionPriority**: Ordered list of regions determines priority
//! - **Custom**: Pluggable conflict resolver via the `ConflictResolver` trait
//!
//! ## Replication Lag Monitoring
//!
//! The coordinator monitors replication lag between regions and can alert
//! when lag exceeds the configured maximum threshold.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Errors specific to active-active coordination.
#[derive(Debug, Error)]
pub enum ActiveActiveError {
    #[error("Region not found: {0}")]
    RegionNotFound(String),

    #[error("Region already registered: {0}")]
    RegionAlreadyRegistered(String),

    #[error("Region not active: {0}")]
    RegionNotActive(String),

    #[error("Replication lag exceeds maximum: {lag:?} > {max:?}")]
    ReplicationLagExceeded { lag: Duration, max: Duration },

    #[error("Conflict resolution failed: {0}")]
    ConflictResolutionFailed(String),

    #[error("Sync failed: {0}")]
    SyncFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, ActiveActiveError>;

/// Strategy for resolving conflicts between regions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Timestamp-based resolution: most recent write wins.
    LastWriterWins,
    /// Ordered priority list of region IDs: first region in list wins.
    RegionPriority(Vec<String>),
    /// Custom resolver (uses the ConflictResolver trait).
    Custom,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        ConflictStrategy::LastWriterWins
    }
}

/// Configuration for active-active multi-region coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveActiveConfig {
    /// ID of the local region.
    pub local_region: String,
    /// List of peer region IDs.
    pub peer_regions: Vec<String>,
    /// Strategy for resolving conflicts.
    pub conflict_strategy: ConflictStrategy,
    /// How frequently to sync with peer regions.
    #[serde(with = "duration_serde")]
    pub sync_interval: Duration,
    /// Maximum acceptable replication lag before alerting.
    #[serde(with = "duration_serde")]
    pub max_replication_lag: Duration,
}

impl Default for ActiveActiveConfig {
    fn default() -> Self {
        Self {
            local_region: "us-east-1".to_string(),
            peer_regions: Vec::new(),
            conflict_strategy: ConflictStrategy::default(),
            sync_interval: Duration::from_secs(5),
            max_replication_lag: Duration::from_secs(30),
        }
    }
}

/// Serde helpers for Duration serialization.
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

/// State of a region in the active-active cluster.
#[derive(Debug, Clone)]
pub struct RegionState {
    pub region_id: String,
    pub endpoint: String,
    pub is_active: bool,
    pub last_sync: Option<Instant>,
    pub replication_lag: Duration,
    /// Version vector: maps region_id -> latest version seen from that region.
    pub version_vector: HashMap<String, u64>,
}

/// A record of a conflict between two regions.
#[derive(Debug, Clone)]
pub struct ConflictRecord {
    pub topic: String,
    pub key: Vec<u8>,
    pub local_value: Vec<u8>,
    pub remote_value: Vec<u8>,
    pub local_timestamp: u64,
    pub remote_timestamp: u64,
    pub local_region: String,
    pub remote_region: String,
}

/// The result of conflict resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep the local region's value.
    KeepLocal,
    /// Keep the remote region's value.
    KeepRemote,
    /// Merge into a new value.
    Merge(Vec<u8>),
}

/// Trait for custom conflict resolution.
#[async_trait]
pub trait ConflictResolver: Send + Sync {
    /// Resolve a conflict between a local and remote write.
    async fn resolve(&self, conflict: &ConflictRecord) -> ConflictResolution;
}

/// Default conflict resolver that uses LastWriterWins strategy.
pub struct LastWriterWinsResolver;

#[async_trait]
impl ConflictResolver for LastWriterWinsResolver {
    async fn resolve(&self, conflict: &ConflictRecord) -> ConflictResolution {
        if conflict.local_timestamp >= conflict.remote_timestamp {
            ConflictResolution::KeepLocal
        } else {
            ConflictResolution::KeepRemote
        }
    }
}

/// Region priority based conflict resolver.
pub struct RegionPriorityResolver {
    priority: Vec<String>,
}

impl RegionPriorityResolver {
    pub fn new(priority: Vec<String>) -> Self {
        Self { priority }
    }
}

#[async_trait]
impl ConflictResolver for RegionPriorityResolver {
    async fn resolve(&self, conflict: &ConflictRecord) -> ConflictResolution {
        let local_priority = self
            .priority
            .iter()
            .position(|r| r == &conflict.local_region);
        let remote_priority = self
            .priority
            .iter()
            .position(|r| r == &conflict.remote_region);

        match (local_priority, remote_priority) {
            (Some(l), Some(r)) => {
                if l <= r {
                    ConflictResolution::KeepLocal
                } else {
                    ConflictResolution::KeepRemote
                }
            }
            (Some(_), None) => ConflictResolution::KeepLocal,
            (None, Some(_)) => ConflictResolution::KeepRemote,
            (None, None) => {
                // Fall back to LWW when neither region is in priority list
                if conflict.local_timestamp >= conflict.remote_timestamp {
                    ConflictResolution::KeepLocal
                } else {
                    ConflictResolution::KeepRemote
                }
            }
        }
    }
}

/// Result of a sync operation with a peer region.
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub records_sent: u64,
    pub records_received: u64,
    pub conflicts_resolved: u64,
    pub conflicts_kept_local: u64,
    pub conflicts_kept_remote: u64,
    pub conflicts_merged: u64,
    pub duration: Duration,
}

/// Statistics for the active-active coordinator.
#[derive(Debug, Clone, Default)]
pub struct ActiveActiveStats {
    pub total_regions: usize,
    pub active_regions: usize,
    pub total_syncs: u64,
    pub total_conflicts: u64,
    pub max_replication_lag: Duration,
    pub regions_exceeding_lag: usize,
}

/// The Active-Active Multi-Region Coordinator.
///
/// Manages region registration, synchronization, and conflict resolution
/// for active-active multi-region deployments.
pub struct ActiveActiveCoordinator {
    config: ActiveActiveConfig,
    regions: RwLock<HashMap<String, RegionState>>,
    conflict_resolver: Box<dyn ConflictResolver>,
    sync_count: RwLock<u64>,
    conflict_count: RwLock<u64>,
}

impl ActiveActiveCoordinator {
    /// Create a new ActiveActiveCoordinator with the given configuration.
    ///
    /// The conflict resolver is automatically selected based on the configuration's
    /// conflict strategy.
    pub fn new(config: ActiveActiveConfig) -> Self {
        let resolver: Box<dyn ConflictResolver> = match &config.conflict_strategy {
            ConflictStrategy::LastWriterWins => Box::new(LastWriterWinsResolver),
            ConflictStrategy::RegionPriority(priority) => {
                Box::new(RegionPriorityResolver::new(priority.clone()))
            }
            ConflictStrategy::Custom => {
                // Default to LWW if no custom resolver provided
                Box::new(LastWriterWinsResolver)
            }
        };

        Self {
            config,
            regions: RwLock::new(HashMap::new()),
            conflict_resolver: resolver,
            sync_count: RwLock::new(0),
            conflict_count: RwLock::new(0),
        }
    }

    /// Create a new coordinator with a custom conflict resolver.
    pub fn with_resolver(
        config: ActiveActiveConfig,
        resolver: Box<dyn ConflictResolver>,
    ) -> Self {
        Self {
            config,
            regions: RwLock::new(HashMap::new()),
            conflict_resolver: resolver,
            sync_count: RwLock::new(0),
            conflict_count: RwLock::new(0),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &ActiveActiveConfig {
        &self.config
    }

    /// Register a new region in the active-active cluster.
    pub async fn register_region(
        &self,
        region_id: String,
        endpoint: String,
    ) -> Result<()> {
        let mut regions = self.regions.write().await;

        if regions.contains_key(&region_id) {
            return Err(ActiveActiveError::RegionAlreadyRegistered(region_id));
        }

        let mut version_vector = HashMap::new();
        version_vector.insert(region_id.clone(), 0);

        // Initialize version vector with all known regions
        for existing_id in regions.keys() {
            version_vector.insert(existing_id.clone(), 0);
        }

        // Also update existing regions' version vectors
        for existing in regions.values_mut() {
            existing.version_vector.insert(region_id.clone(), 0);
        }

        let state = RegionState {
            region_id: region_id.clone(),
            endpoint,
            is_active: true,
            last_sync: None,
            replication_lag: Duration::ZERO,
            version_vector,
        };

        regions.insert(region_id.clone(), state);
        info!("Registered region: {}", region_id);
        Ok(())
    }

    /// Deregister a region from the cluster.
    pub async fn deregister_region(&self, region_id: &str) -> Result<()> {
        let mut regions = self.regions.write().await;

        if regions.remove(region_id).is_none() {
            return Err(ActiveActiveError::RegionNotFound(region_id.to_string()));
        }

        // Remove from other regions' version vectors
        for state in regions.values_mut() {
            state.version_vector.remove(region_id);
        }

        info!("Deregistered region: {}", region_id);
        Ok(())
    }

    /// Simulate synchronization with a peer region.
    ///
    /// @Note, this would exchange records via gRPC/HTTP.
    /// Here we simulate the sync and conflict resolution process.
    pub async fn sync_with_peer(
        &self,
        peer_region_id: &str,
        conflicts: Vec<ConflictRecord>,
    ) -> Result<SyncResult> {
        let start = Instant::now();

        let regions = self.regions.read().await;
        let peer = regions.get(peer_region_id).ok_or_else(|| {
            ActiveActiveError::RegionNotFound(peer_region_id.to_string())
        })?;

        if !peer.is_active {
            return Err(ActiveActiveError::RegionNotActive(
                peer_region_id.to_string(),
            ));
        }

        drop(regions);

        let mut result = SyncResult::default();

        // Resolve conflicts
        for conflict in &conflicts {
            let resolution = self.conflict_resolver.resolve(conflict).await;
            result.conflicts_resolved += 1;

            match resolution {
                ConflictResolution::KeepLocal => result.conflicts_kept_local += 1,
                ConflictResolution::KeepRemote => result.conflicts_kept_remote += 1,
                ConflictResolution::Merge(_) => result.conflicts_merged += 1,
            }
        }

        result.duration = start.elapsed();

        // Update sync metadata
        let mut regions = self.regions.write().await;
        if let Some(peer) = regions.get_mut(peer_region_id) {
            peer.last_sync = Some(Instant::now());
            peer.replication_lag = Duration::ZERO; // Reset lag after sync
        }

        let mut sync_count = self.sync_count.write().await;
        *sync_count += 1;

        let mut conflict_count = self.conflict_count.write().await;
        *conflict_count += result.conflicts_resolved;

        info!(
            "Synced with {}: {} conflicts resolved in {:?}",
            peer_region_id, result.conflicts_resolved, result.duration
        );

        Ok(result)
    }

    /// Resolve a single conflict using the configured strategy.
    pub async fn resolve_conflict(
        &self,
        conflict: &ConflictRecord,
    ) -> Result<ConflictResolution> {
        let resolution = self.conflict_resolver.resolve(conflict).await;
        debug!(
            "Resolved conflict for key in {}/{}: {:?}",
            conflict.topic,
            conflict.local_region,
            resolution
        );
        Ok(resolution)
    }

    /// Check replication lag for all peer regions.
    ///
    /// Returns a list of (region_id, lag) for regions exceeding the max lag.
    pub async fn check_replication_lag(&self) -> Vec<(String, Duration)> {
        let regions = self.regions.read().await;
        let mut lagging = Vec::new();

        for state in regions.values() {
            if state.replication_lag > self.config.max_replication_lag {
                warn!(
                    "Region {} replication lag {:?} exceeds max {:?}",
                    state.region_id, state.replication_lag, self.config.max_replication_lag
                );
                lagging.push((state.region_id.clone(), state.replication_lag));
            }
        }

        lagging
    }

    /// Get the version vector for a specific region.
    pub async fn get_version_vector(
        &self,
        region_id: &str,
    ) -> Result<HashMap<String, u64>> {
        let regions = self.regions.read().await;
        let state = regions.get(region_id).ok_or_else(|| {
            ActiveActiveError::RegionNotFound(region_id.to_string())
        })?;

        Ok(state.version_vector.clone())
    }

    /// Increment the version for the local region.
    ///
    /// Called after a successful local write to advance the version vector.
    pub async fn increment_version(&self, region_id: &str) -> Result<u64> {
        let mut regions = self.regions.write().await;

        let state = regions.get_mut(region_id).ok_or_else(|| {
            ActiveActiveError::RegionNotFound(region_id.to_string())
        })?;

        let new_version = state.version_vector.get(region_id).unwrap_or(&0) + 1;
        state.version_vector.insert(region_id.to_string(), new_version);

        Ok(new_version)
    }

    /// Update the replication lag for a region.
    pub async fn update_replication_lag(
        &self,
        region_id: &str,
        lag: Duration,
    ) -> Result<()> {
        let mut regions = self.regions.write().await;

        let state = regions.get_mut(region_id).ok_or_else(|| {
            ActiveActiveError::RegionNotFound(region_id.to_string())
        })?;

        state.replication_lag = lag;
        Ok(())
    }

    /// Set a region's active status.
    pub async fn set_region_active(
        &self,
        region_id: &str,
        active: bool,
    ) -> Result<()> {
        let mut regions = self.regions.write().await;

        let state = regions.get_mut(region_id).ok_or_else(|| {
            ActiveActiveError::RegionNotFound(region_id.to_string())
        })?;

        state.is_active = active;
        info!(
            "Region {} is now {}",
            region_id,
            if active { "active" } else { "inactive" }
        );
        Ok(())
    }

    /// Get the state of a specific region.
    pub async fn get_region_state(&self, region_id: &str) -> Result<RegionState> {
        let regions = self.regions.read().await;
        regions
            .get(region_id)
            .cloned()
            .ok_or_else(|| ActiveActiveError::RegionNotFound(region_id.to_string()))
    }

    /// Get aggregate statistics for the active-active cluster.
    pub async fn get_stats(&self) -> ActiveActiveStats {
        let regions = self.regions.read().await;
        let sync_count = *self.sync_count.read().await;
        let conflict_count = *self.conflict_count.read().await;

        let mut stats = ActiveActiveStats {
            total_regions: regions.len(),
            total_syncs: sync_count,
            total_conflicts: conflict_count,
            ..Default::default()
        };

        for state in regions.values() {
            if state.is_active {
                stats.active_regions += 1;
            }
            if state.replication_lag > stats.max_replication_lag {
                stats.max_replication_lag = state.replication_lag;
            }
            if state.replication_lag > self.config.max_replication_lag {
                stats.regions_exceeding_lag += 1;
            }
        }

        stats
    }

    /// List all registered region IDs.
    pub async fn list_regions(&self) -> Vec<String> {
        let regions = self.regions.read().await;
        regions.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn default_config() -> ActiveActiveConfig {
        ActiveActiveConfig {
            local_region: "us-east-1".to_string(),
            peer_regions: vec!["us-west-2".to_string(), "eu-west-1".to_string()],
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_interval: Duration::from_secs(5),
            max_replication_lag: Duration::from_secs(30),
        }
    }

    fn make_conflict(
        local_ts: u64,
        remote_ts: u64,
        local_region: &str,
        remote_region: &str,
    ) -> ConflictRecord {
        ConflictRecord {
            topic: "orders".to_string(),
            key: b"order-123".to_vec(),
            local_value: b"local-value".to_vec(),
            remote_value: b"remote-value".to_vec(),
            local_timestamp: local_ts,
            remote_timestamp: remote_ts,
            local_region: local_region.to_string(),
            remote_region: remote_region.to_string(),
        }
    }

    // Test 1: Register and list regions
    #[tokio::test]
    async fn test_register_and_list_regions() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();
        coord
            .register_region("us-west-2".to_string(), "https://west.example.com".to_string())
            .await
            .unwrap();

        let regions = coord.list_regions().await;
        assert_eq!(regions.len(), 2);
        assert!(regions.contains(&"us-east-1".to_string()));
        assert!(regions.contains(&"us-west-2".to_string()));
    }

    // Test 2: Duplicate region registration fails
    #[tokio::test]
    async fn test_duplicate_region_registration() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();

        let result = coord
            .register_region("us-east-1".to_string(), "https://east2.example.com".to_string())
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ActiveActiveError::RegionAlreadyRegistered(_) => {}
            e => panic!("Expected RegionAlreadyRegistered, got {:?}", e),
        }
    }

    // Test 3: LastWriterWins conflict resolution
    #[tokio::test]
    async fn test_lww_conflict_resolution() {
        let coord = ActiveActiveCoordinator::new(default_config());

        // Local is newer -> keep local
        let conflict = make_conflict(2000, 1000, "us-east-1", "us-west-2");
        let resolution = coord.resolve_conflict(&conflict).await.unwrap();
        assert_eq!(resolution, ConflictResolution::KeepLocal);

        // Remote is newer -> keep remote
        let conflict = make_conflict(1000, 2000, "us-east-1", "us-west-2");
        let resolution = coord.resolve_conflict(&conflict).await.unwrap();
        assert_eq!(resolution, ConflictResolution::KeepRemote);

        // Same timestamp -> keep local (tie-breaker)
        let conflict = make_conflict(1000, 1000, "us-east-1", "us-west-2");
        let resolution = coord.resolve_conflict(&conflict).await.unwrap();
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    // Test 4: RegionPriority conflict resolution
    #[tokio::test]
    async fn test_region_priority_conflict_resolution() {
        let config = ActiveActiveConfig {
            conflict_strategy: ConflictStrategy::RegionPriority(vec![
                "us-east-1".to_string(),
                "us-west-2".to_string(),
                "eu-west-1".to_string(),
            ]),
            ..default_config()
        };
        let coord = ActiveActiveCoordinator::new(config);

        // us-east-1 has highest priority
        let conflict = make_conflict(1000, 2000, "us-east-1", "us-west-2");
        let resolution = coord.resolve_conflict(&conflict).await.unwrap();
        assert_eq!(resolution, ConflictResolution::KeepLocal);

        // us-west-2 vs eu-west-1: us-west-2 wins (lower index)
        let conflict = make_conflict(1000, 2000, "us-west-2", "eu-west-1");
        let resolution = coord.resolve_conflict(&conflict).await.unwrap();
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    // Test 5: Custom conflict resolver
    #[tokio::test]
    async fn test_custom_conflict_resolver() {
        struct MergeResolver;

        #[async_trait]
        impl ConflictResolver for MergeResolver {
            async fn resolve(&self, _conflict: &ConflictRecord) -> ConflictResolution {
                ConflictResolution::Merge(b"merged-value".to_vec())
            }
        }

        let coord = ActiveActiveCoordinator::with_resolver(
            default_config(),
            Box::new(MergeResolver),
        );

        let conflict = make_conflict(1000, 2000, "us-east-1", "us-west-2");
        let resolution = coord.resolve_conflict(&conflict).await.unwrap();
        assert_eq!(resolution, ConflictResolution::Merge(b"merged-value".to_vec()));
    }

    // Test 6: Sync with peer resolves conflicts
    #[tokio::test]
    async fn test_sync_with_peer() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();
        coord
            .register_region("us-west-2".to_string(), "https://west.example.com".to_string())
            .await
            .unwrap();

        let conflicts = vec![
            make_conflict(2000, 1000, "us-east-1", "us-west-2"),
            make_conflict(1000, 2000, "us-east-1", "us-west-2"),
            make_conflict(1500, 1500, "us-east-1", "us-west-2"),
        ];

        let result = coord.sync_with_peer("us-west-2", conflicts).await.unwrap();
        assert_eq!(result.conflicts_resolved, 3);
        assert_eq!(result.conflicts_kept_local, 2); // ts 2000 > 1000, and 1500 >= 1500
        assert_eq!(result.conflicts_kept_remote, 1); // ts 1000 < 2000
    }

    // Test 7: Replication lag monitoring
    #[tokio::test]
    async fn test_replication_lag_monitoring() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();
        coord
            .register_region("us-west-2".to_string(), "https://west.example.com".to_string())
            .await
            .unwrap();

        // Set high lag for one region
        coord
            .update_replication_lag("us-west-2", Duration::from_secs(60))
            .await
            .unwrap();

        let lagging = coord.check_replication_lag().await;
        assert_eq!(lagging.len(), 1);
        assert_eq!(lagging[0].0, "us-west-2");
        assert_eq!(lagging[0].1, Duration::from_secs(60));
    }

    // Test 8: Version vector management
    #[tokio::test]
    async fn test_version_vector() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();

        // Increment version
        let v1 = coord.increment_version("us-east-1").await.unwrap();
        assert_eq!(v1, 1);

        let v2 = coord.increment_version("us-east-1").await.unwrap();
        assert_eq!(v2, 2);

        let vv = coord.get_version_vector("us-east-1").await.unwrap();
        assert_eq!(vv.get("us-east-1"), Some(&2));
    }

    // Test 9: Deregister region
    #[tokio::test]
    async fn test_deregister_region() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();
        coord
            .register_region("us-west-2".to_string(), "https://west.example.com".to_string())
            .await
            .unwrap();

        coord.deregister_region("us-west-2").await.unwrap();

        let regions = coord.list_regions().await;
        assert_eq!(regions.len(), 1);
        assert!(!regions.contains(&"us-west-2".to_string()));

        // Version vector of remaining region should not reference deregistered region
        let vv = coord.get_version_vector("us-east-1").await.unwrap();
        assert!(!vv.contains_key("us-west-2"));
    }

    // Test 10: Sync with inactive region fails
    #[tokio::test]
    async fn test_sync_with_inactive_region() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();
        coord
            .register_region("us-west-2".to_string(), "https://west.example.com".to_string())
            .await
            .unwrap();

        // Deactivate the region
        coord.set_region_active("us-west-2", false).await.unwrap();

        let result = coord.sync_with_peer("us-west-2", vec![]).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ActiveActiveError::RegionNotActive(_) => {}
            e => panic!("Expected RegionNotActive, got {:?}", e),
        }
    }

    // Test 11: Stats tracking
    #[tokio::test]
    async fn test_stats_tracking() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();
        coord
            .register_region("us-west-2".to_string(), "https://west.example.com".to_string())
            .await
            .unwrap();

        // Perform a sync
        let conflicts = vec![make_conflict(2000, 1000, "us-east-1", "us-west-2")];
        coord.sync_with_peer("us-west-2", conflicts).await.unwrap();

        let stats = coord.get_stats().await;
        assert_eq!(stats.total_regions, 2);
        assert_eq!(stats.active_regions, 2);
        assert_eq!(stats.total_syncs, 1);
        assert_eq!(stats.total_conflicts, 1);
    }

    // Test 12: Region not found errors
    #[tokio::test]
    async fn test_region_not_found() {
        let coord = ActiveActiveCoordinator::new(default_config());

        assert!(coord.get_version_vector("nonexistent").await.is_err());
        assert!(coord.increment_version("nonexistent").await.is_err());
        assert!(coord
            .update_replication_lag("nonexistent", Duration::ZERO)
            .await
            .is_err());
        assert!(coord.deregister_region("nonexistent").await.is_err());
        assert!(coord.sync_with_peer("nonexistent", vec![]).await.is_err());
    }

    // Test 13: Version vectors updated when new region joins
    #[tokio::test]
    async fn test_version_vectors_updated_on_join() {
        let coord = ActiveActiveCoordinator::new(default_config());

        coord
            .register_region("us-east-1".to_string(), "https://east.example.com".to_string())
            .await
            .unwrap();

        // Register second region
        coord
            .register_region("us-west-2".to_string(), "https://west.example.com".to_string())
            .await
            .unwrap();

        // us-east-1 should know about us-west-2 in its version vector
        let vv = coord.get_version_vector("us-east-1").await.unwrap();
        assert!(vv.contains_key("us-west-2"));
        assert_eq!(vv.get("us-west-2"), Some(&0));

        // us-west-2 should know about us-east-1
        let vv = coord.get_version_vector("us-west-2").await.unwrap();
        assert!(vv.contains_key("us-east-1"));
        assert_eq!(vv.get("us-east-1"), Some(&0));
    }
}
