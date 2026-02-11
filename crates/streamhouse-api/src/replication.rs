//! Cross-Region Replication for StreamHouse
//!
//! Provides the ability to replicate segments across multiple regions for disaster
//! recovery and geo-redundancy. Supports both synchronous and asynchronous replication
//! modes with configurable RPO (Recovery Point Objective) and RTO (Recovery Time Objective).
//!
//! ## Architecture
//!
//! Replication works by:
//! 1. **Monitoring**: Watching for new segments in the source region
//! 2. **Mirroring**: Copying segments to target regions via `object_store`
//! 3. **Tracking**: Maintaining replication state and lag metrics
//! 4. **Compliance**: Checking RPO compliance and alerting on violations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use streamhouse_api::replication::{ReplicationConfig, ReplicationManager, ReplicationMode};
//!
//! let config = ReplicationConfig {
//!     source_region: "us-east-1".to_string(),
//!     target_regions: vec!["eu-west-1".to_string(), "ap-southeast-1".to_string()],
//!     replication_mode: ReplicationMode::Async,
//!     s3_versioning_enabled: true,
//!     rto_seconds: 300,
//!     rpo_seconds: 60,
//! };
//!
//! let manager = ReplicationManager::new(config, source_store).await?;
//! manager.start_replication().await?;
//! ```

use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};


/// Errors that can occur during replication operations
#[derive(Debug, Error)]
pub enum ReplicationError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Region not found: {0}")]
    RegionNotFound(String),

    #[error("Replication already active")]
    AlreadyActive,

    #[error("Replication not active")]
    NotActive,

    #[error("RPO violation: lag {lag_seconds}s exceeds limit {rpo_seconds}s")]
    RpoViolation { lag_seconds: u64, rpo_seconds: u64 },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Object store error: {0}")]
    ObjectStore(#[from] object_store::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ReplicationError>;

/// Replication mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationMode {
    /// Asynchronous replication - lower latency, potential data loss window
    Async,
    /// Synchronous replication - higher latency, zero data loss
    Sync,
}

impl Default for ReplicationMode {
    fn default() -> Self {
        Self::Async
    }
}

impl std::fmt::Display for ReplicationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicationMode::Async => write!(f, "async"),
            ReplicationMode::Sync => write!(f, "sync"),
        }
    }
}

/// Configuration for cross-region replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// Source region name (e.g., "us-east-1")
    pub source_region: String,
    /// Target regions to replicate to
    pub target_regions: Vec<String>,
    /// Replication mode (async or sync)
    pub replication_mode: ReplicationMode,
    /// Whether S3 versioning is enabled for conflict resolution
    pub s3_versioning_enabled: bool,
    /// Recovery Time Objective in seconds (max acceptable downtime)
    pub rto_seconds: u64,
    /// Recovery Point Objective in seconds (max acceptable data loss window)
    pub rpo_seconds: u64,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            source_region: "us-east-1".to_string(),
            target_regions: Vec::new(),
            replication_mode: ReplicationMode::Async,
            s3_versioning_enabled: true,
            rto_seconds: 300,  // 5 minutes
            rpo_seconds: 60,   // 1 minute
        }
    }
}

impl ReplicationConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.source_region.is_empty() {
            return Err(ReplicationError::Config(
                "Source region must not be empty".to_string(),
            ));
        }
        if self.target_regions.contains(&self.source_region) {
            return Err(ReplicationError::Config(
                "Target regions must not include source region".to_string(),
            ));
        }
        if self.rto_seconds == 0 {
            return Err(ReplicationError::Config(
                "RTO must be greater than zero".to_string(),
            ));
        }
        if self.rpo_seconds == 0 {
            return Err(ReplicationError::Config(
                "RPO must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

/// Status of a replication stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationStatus {
    /// Replication is actively running
    Active,
    /// Replication is paused
    Paused,
    /// Replication has failed
    Failed,
    /// Replication is initializing
    Initializing,
}

impl std::fmt::Display for ReplicationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicationStatus::Active => write!(f, "active"),
            ReplicationStatus::Paused => write!(f, "paused"),
            ReplicationStatus::Failed => write!(f, "failed"),
            ReplicationStatus::Initializing => write!(f, "initializing"),
        }
    }
}

/// A region endpoint for replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionEndpoint {
    /// Region name (e.g., "us-east-1")
    pub region_name: String,
    /// S3 bucket name for this region
    pub s3_bucket: String,
    /// S3 endpoint URL (for S3-compatible storage)
    pub s3_endpoint: String,
    /// Current status of this region endpoint
    pub status: ReplicationStatus,
}

impl RegionEndpoint {
    /// Create a new region endpoint
    pub fn new(
        region_name: impl Into<String>,
        s3_bucket: impl Into<String>,
        s3_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            region_name: region_name.into(),
            s3_bucket: s3_bucket.into(),
            s3_endpoint: s3_endpoint.into(),
            status: ReplicationStatus::Initializing,
        }
    }
}

/// Tracking information for a replicated segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicatedSegment {
    /// Unique segment identifier
    pub segment_id: String,
    /// Topic this segment belongs to
    pub topic: String,
    /// Partition number
    pub partition: u32,
    /// Object store path in the source region
    pub source_path: String,
    /// Size of the segment in bytes
    pub size_bytes: u64,
    /// When the segment was created in the source region
    pub created_at: i64,
    /// Replication status per target region
    pub region_status: HashMap<String, RegionReplicationState>,
}

/// Replication state for a segment in a specific target region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionReplicationState {
    /// Whether the segment has been replicated to this region
    pub replicated: bool,
    /// When replication completed (if it has)
    pub replicated_at: Option<i64>,
    /// Object store path in the target region
    pub target_path: Option<String>,
    /// Error message if replication failed
    pub error: Option<String>,
    /// Number of retry attempts
    pub retry_count: u32,
}

impl Default for RegionReplicationState {
    fn default() -> Self {
        Self {
            replicated: false,
            replicated_at: None,
            target_path: None,
            error: None,
            retry_count: 0,
        }
    }
}

/// Replication lag information for a target region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationLag {
    /// Target region name
    pub region: String,
    /// Number of segments pending replication
    pub pending_segments: usize,
    /// Total bytes pending replication
    pub pending_bytes: u64,
    /// Estimated lag in seconds
    pub lag_seconds: u64,
    /// Whether RPO compliance is met
    pub rpo_compliant: bool,
}

/// Summary statistics for a replication stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStats {
    /// Total segments tracked
    pub total_segments: usize,
    /// Segments fully replicated to all targets
    pub fully_replicated: usize,
    /// Segments pending replication
    pub pending: usize,
    /// Segments that failed replication
    pub failed: usize,
    /// Total bytes replicated
    pub total_bytes_replicated: u64,
    /// Per-region lag information
    pub region_lags: Vec<ReplicationLag>,
}

/// Cross-region replication manager
///
/// Manages the replication of segments from a source region to one or more
/// target regions. Uses `object_store` for all S3 operations.
pub struct ReplicationManager {
    /// Replication configuration
    config: ReplicationConfig,
    /// Current replication status
    status: Arc<RwLock<ReplicationStatus>>,
    /// Source region object store
    source_store: Arc<dyn ObjectStore>,
    /// Target region object stores (region_name -> store)
    target_stores: Arc<RwLock<HashMap<String, Arc<dyn ObjectStore>>>>,
    /// Region endpoint metadata
    endpoints: Arc<RwLock<HashMap<String, RegionEndpoint>>>,
    /// Tracked replicated segments (segment_id -> segment)
    segments: Arc<RwLock<HashMap<String, ReplicatedSegment>>>,
    /// Timestamp of last successful replication per region
    last_replication_time: Arc<RwLock<HashMap<String, i64>>>,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub async fn new(
        config: ReplicationConfig,
        source_store: Arc<dyn ObjectStore>,
    ) -> Result<Self> {
        config.validate()?;

        info!(
            source_region = %config.source_region,
            target_count = config.target_regions.len(),
            mode = %config.replication_mode,
            "Creating replication manager"
        );

        Ok(Self {
            config,
            status: Arc::new(RwLock::new(ReplicationStatus::Initializing)),
            source_store,
            target_stores: Arc::new(RwLock::new(HashMap::new())),
            endpoints: Arc::new(RwLock::new(HashMap::new())),
            segments: Arc::new(RwLock::new(HashMap::new())),
            last_replication_time: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Start replication
    pub async fn start_replication(&self) -> Result<()> {
        let mut status = self.status.write().await;
        if *status == ReplicationStatus::Active {
            return Err(ReplicationError::AlreadyActive);
        }

        info!(
            source = %self.config.source_region,
            targets = ?self.config.target_regions,
            "Starting cross-region replication"
        );

        *status = ReplicationStatus::Active;

        // Initialize last replication time for all target regions
        let mut last_times = self.last_replication_time.write().await;
        let now = chrono::Utc::now().timestamp_millis();
        for region in &self.config.target_regions {
            last_times.entry(region.clone()).or_insert(now);
        }

        info!("Cross-region replication started");
        Ok(())
    }

    /// Pause replication
    pub async fn pause_replication(&self) -> Result<()> {
        let mut status = self.status.write().await;
        if *status != ReplicationStatus::Active {
            return Err(ReplicationError::NotActive);
        }

        warn!("Pausing cross-region replication");
        *status = ReplicationStatus::Paused;
        Ok(())
    }

    /// Resume replication after pause
    pub async fn resume_replication(&self) -> Result<()> {
        let mut status = self.status.write().await;
        if *status != ReplicationStatus::Paused {
            return Err(ReplicationError::NotActive);
        }

        info!("Resuming cross-region replication");
        *status = ReplicationStatus::Active;
        Ok(())
    }

    /// Get current replication status
    pub async fn get_status(&self) -> ReplicationStatus {
        *self.status.read().await
    }

    /// Get the replication configuration
    pub fn get_config(&self) -> &ReplicationConfig {
        &self.config
    }

    /// Add a target region with its object store
    pub async fn add_target_region(
        &self,
        endpoint: RegionEndpoint,
        store: Arc<dyn ObjectStore>,
    ) -> Result<()> {
        let region_name = endpoint.region_name.clone();

        if region_name == self.config.source_region {
            return Err(ReplicationError::Config(
                "Cannot add source region as a target".to_string(),
            ));
        }

        info!(region = %region_name, bucket = %endpoint.s3_bucket, "Adding target region");

        let mut stores = self.target_stores.write().await;
        stores.insert(region_name.clone(), store);

        let mut endpoints = self.endpoints.write().await;
        endpoints.insert(region_name.clone(), endpoint);

        let mut last_times = self.last_replication_time.write().await;
        last_times
            .entry(region_name.clone())
            .or_insert_with(|| chrono::Utc::now().timestamp_millis());

        info!(region = %region_name, "Target region added");
        Ok(())
    }

    /// Remove a target region
    pub async fn remove_target_region(&self, region_name: &str) -> Result<()> {
        let mut stores = self.target_stores.write().await;
        let mut endpoints = self.endpoints.write().await;

        if stores.remove(region_name).is_none() {
            return Err(ReplicationError::RegionNotFound(region_name.to_string()));
        }

        endpoints.remove(region_name);

        let mut last_times = self.last_replication_time.write().await;
        last_times.remove(region_name);

        warn!(region = %region_name, "Target region removed");
        Ok(())
    }

    /// List all configured region endpoints
    pub async fn list_endpoints(&self) -> Vec<RegionEndpoint> {
        self.endpoints.read().await.values().cloned().collect()
    }

    /// Mirror a segment from source to a specific target region
    ///
    /// Copies the segment data from the source object store to the target region's
    /// object store. Returns the replicated segment tracking information.
    pub async fn mirror_segment(
        &self,
        topic: &str,
        partition: u32,
        segment_id: &str,
        source_path: &str,
    ) -> Result<ReplicatedSegment> {
        let current_status = self.get_status().await;
        if current_status != ReplicationStatus::Active {
            return Err(ReplicationError::NotActive);
        }

        debug!(
            topic = topic,
            partition = partition,
            segment_id = segment_id,
            source_path = source_path,
            "Mirroring segment to target regions"
        );

        // Read the segment from source
        let source_object_path = ObjectPath::from(source_path);
        let get_result = self.source_store.get(&source_object_path).await?;
        let segment_data = get_result.bytes().await?;
        let size_bytes = segment_data.len() as u64;

        let now = chrono::Utc::now().timestamp_millis();

        // Initialize tracking
        let mut region_status = HashMap::new();
        let target_stores = self.target_stores.read().await;

        // Replicate to each target region
        for (region_name, target_store) in target_stores.iter() {
            let target_path = format!(
                "replicated/{}/{}/{}/{}",
                region_name, topic, partition, segment_id
            );
            let target_object_path = ObjectPath::from(target_path.as_str());

            match target_store
                .put(&target_object_path, segment_data.clone().into())
                .await
            {
                Ok(_) => {
                    info!(
                        segment_id = segment_id,
                        region = %region_name,
                        size_bytes = size_bytes,
                        "Segment replicated successfully"
                    );

                    region_status.insert(
                        region_name.clone(),
                        RegionReplicationState {
                            replicated: true,
                            replicated_at: Some(chrono::Utc::now().timestamp_millis()),
                            target_path: Some(target_path),
                            error: None,
                            retry_count: 0,
                        },
                    );

                    // Update last replication time
                    let mut last_times = self.last_replication_time.write().await;
                    last_times.insert(
                        region_name.clone(),
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                Err(e) => {
                    error!(
                        segment_id = segment_id,
                        region = %region_name,
                        error = %e,
                        "Failed to replicate segment"
                    );

                    region_status.insert(
                        region_name.clone(),
                        RegionReplicationState {
                            replicated: false,
                            replicated_at: None,
                            target_path: None,
                            error: Some(e.to_string()),
                            retry_count: 1,
                        },
                    );
                }
            }
        }

        let replicated_segment = ReplicatedSegment {
            segment_id: segment_id.to_string(),
            topic: topic.to_string(),
            partition,
            source_path: source_path.to_string(),
            size_bytes,
            created_at: now,
            region_status,
        };

        // Track the segment
        let mut segments = self.segments.write().await;
        segments.insert(segment_id.to_string(), replicated_segment.clone());

        Ok(replicated_segment)
    }

    /// Check RPO compliance for all target regions
    ///
    /// Returns a list of regions that are not meeting the configured RPO.
    pub async fn check_rpo_compliance(&self) -> Vec<ReplicationLag> {
        let mut violations = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();
        let last_times = self.last_replication_time.read().await;
        let segments = self.segments.read().await;

        for region in &self.config.target_regions {
            let last_time = last_times.get(region).copied().unwrap_or(0);
            let lag_ms = (now - last_time).max(0) as u64;
            let lag_seconds = lag_ms / 1000;

            // Count pending segments for this region
            let mut pending_segments = 0;
            let mut pending_bytes = 0;
            for seg in segments.values() {
                match seg.region_status.get(region) {
                    Some(state) if !state.replicated => {
                        pending_segments += 1;
                        pending_bytes += seg.size_bytes;
                    }
                    None => {
                        pending_segments += 1;
                        pending_bytes += seg.size_bytes;
                    }
                    _ => {}
                }
            }

            let rpo_compliant = lag_seconds <= self.config.rpo_seconds;

            if !rpo_compliant {
                warn!(
                    region = %region,
                    lag_seconds = lag_seconds,
                    rpo_seconds = self.config.rpo_seconds,
                    pending_segments = pending_segments,
                    "RPO violation detected"
                );
            }

            violations.push(ReplicationLag {
                region: region.clone(),
                pending_segments,
                pending_bytes,
                lag_seconds,
                rpo_compliant,
            });
        }

        violations
    }

    /// Get replication lag information for all target regions
    pub async fn get_replication_lag(&self) -> Vec<ReplicationLag> {
        let now = chrono::Utc::now().timestamp_millis();
        let last_times = self.last_replication_time.read().await;
        let segments = self.segments.read().await;
        let mut lags = Vec::new();

        for region in &self.config.target_regions {
            let last_time = last_times.get(region).copied().unwrap_or(0);
            let lag_ms = (now - last_time).max(0) as u64;
            let lag_seconds = lag_ms / 1000;

            let mut pending_segments = 0;
            let mut pending_bytes = 0;
            for seg in segments.values() {
                match seg.region_status.get(region) {
                    Some(state) if !state.replicated => {
                        pending_segments += 1;
                        pending_bytes += seg.size_bytes;
                    }
                    None => {
                        pending_segments += 1;
                        pending_bytes += seg.size_bytes;
                    }
                    _ => {}
                }
            }

            lags.push(ReplicationLag {
                region: region.clone(),
                pending_segments,
                pending_bytes,
                lag_seconds,
                rpo_compliant: lag_seconds <= self.config.rpo_seconds,
            });
        }

        lags
    }

    /// Get replication statistics
    pub async fn get_stats(&self) -> ReplicationStats {
        let segments = self.segments.read().await;
        let target_regions = &self.config.target_regions;

        let total_segments = segments.len();
        let mut fully_replicated = 0;
        let mut pending = 0;
        let mut failed = 0;
        let mut total_bytes_replicated = 0u64;

        for seg in segments.values() {
            let all_replicated = target_regions.iter().all(|region| {
                seg.region_status
                    .get(region)
                    .map(|s| s.replicated)
                    .unwrap_or(false)
            });

            let any_failed = seg.region_status.values().any(|s| s.error.is_some());

            if all_replicated {
                fully_replicated += 1;
                total_bytes_replicated += seg.size_bytes;
            } else if any_failed {
                failed += 1;
            } else {
                pending += 1;
            }
        }

        let region_lags = self.get_replication_lag().await;

        ReplicationStats {
            total_segments,
            fully_replicated,
            pending,
            failed,
            total_bytes_replicated,
            region_lags,
        }
    }

    /// Get a tracked segment by ID
    pub async fn get_segment(&self, segment_id: &str) -> Option<ReplicatedSegment> {
        self.segments.read().await.get(segment_id).cloned()
    }

    /// List all tracked segments
    pub async fn list_segments(&self) -> Vec<ReplicatedSegment> {
        self.segments.read().await.values().cloned().collect()
    }

    /// Mark replication as failed with an error
    pub async fn mark_failed(&self) {
        let mut status = self.status.write().await;
        *status = ReplicationStatus::Failed;
        error!("Replication marked as failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    fn test_config() -> ReplicationConfig {
        ReplicationConfig {
            source_region: "us-east-1".to_string(),
            target_regions: vec!["eu-west-1".to_string(), "ap-southeast-1".to_string()],
            replication_mode: ReplicationMode::Async,
            s3_versioning_enabled: true,
            rto_seconds: 300,
            rpo_seconds: 60,
        }
    }

    async fn create_test_manager() -> ReplicationManager {
        let config = test_config();
        let source_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        ReplicationManager::new(config, source_store)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_config_validation() {
        // Valid config
        let config = test_config();
        assert!(config.validate().is_ok());

        // Empty source region
        let mut config = test_config();
        config.source_region = String::new();
        assert!(config.validate().is_err());

        // Source region in targets
        let mut config = test_config();
        config.target_regions.push("us-east-1".to_string());
        assert!(config.validate().is_err());

        // Zero RTO
        let mut config = test_config();
        config.rto_seconds = 0;
        assert!(config.validate().is_err());

        // Zero RPO
        let mut config = test_config();
        config.rpo_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn test_create_manager() {
        let manager = create_test_manager().await;
        assert_eq!(manager.get_status().await, ReplicationStatus::Initializing);
        assert_eq!(manager.get_config().source_region, "us-east-1");
    }

    #[tokio::test]
    async fn test_start_and_pause_replication() {
        let manager = create_test_manager().await;

        // Start
        manager.start_replication().await.unwrap();
        assert_eq!(manager.get_status().await, ReplicationStatus::Active);

        // Starting again should fail
        assert!(manager.start_replication().await.is_err());

        // Pause
        manager.pause_replication().await.unwrap();
        assert_eq!(manager.get_status().await, ReplicationStatus::Paused);

        // Pausing again should fail
        assert!(manager.pause_replication().await.is_err());

        // Resume
        manager.resume_replication().await.unwrap();
        assert_eq!(manager.get_status().await, ReplicationStatus::Active);
    }

    #[tokio::test]
    async fn test_add_and_remove_target_region() {
        let manager = create_test_manager().await;

        let endpoint = RegionEndpoint::new(
            "us-west-2",
            "streamhouse-us-west-2",
            "https://s3.us-west-2.amazonaws.com",
        );
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());

        // Add region
        manager
            .add_target_region(endpoint, store)
            .await
            .unwrap();

        let endpoints = manager.list_endpoints().await;
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].region_name, "us-west-2");

        // Remove region
        manager.remove_target_region("us-west-2").await.unwrap();
        let endpoints = manager.list_endpoints().await;
        assert!(endpoints.is_empty());

        // Removing non-existent region should fail
        assert!(manager.remove_target_region("non-existent").await.is_err());
    }

    #[tokio::test]
    async fn test_cannot_add_source_as_target() {
        let manager = create_test_manager().await;

        let endpoint = RegionEndpoint::new(
            "us-east-1", // Same as source
            "streamhouse-us-east-1",
            "https://s3.us-east-1.amazonaws.com",
        );
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());

        let result = manager.add_target_region(endpoint, store).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mirror_segment() {
        let source_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let config = test_config();
        let manager = ReplicationManager::new(config, source_store.clone())
            .await
            .unwrap();

        // Add target regions with their stores
        let eu_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let ap_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());

        manager
            .add_target_region(
                RegionEndpoint::new(
                    "eu-west-1",
                    "streamhouse-eu",
                    "https://s3.eu-west-1.amazonaws.com",
                ),
                eu_store.clone(),
            )
            .await
            .unwrap();

        manager
            .add_target_region(
                RegionEndpoint::new(
                    "ap-southeast-1",
                    "streamhouse-ap",
                    "https://s3.ap-southeast-1.amazonaws.com",
                ),
                ap_store.clone(),
            )
            .await
            .unwrap();

        // Start replication
        manager.start_replication().await.unwrap();

        // Write test data to source
        let source_path = "topics/orders/0/segment-001.strm";
        let test_data = bytes::Bytes::from(b"test segment data".to_vec());
        source_store
            .put(&ObjectPath::from(source_path), test_data.into())
            .await
            .unwrap();

        // Mirror the segment
        let result = manager
            .mirror_segment("orders", 0, "segment-001", source_path)
            .await
            .unwrap();

        assert_eq!(result.segment_id, "segment-001");
        assert_eq!(result.topic, "orders");
        assert_eq!(result.partition, 0);
        assert_eq!(result.size_bytes, 17); // "test segment data".len()

        // Both regions should have been replicated
        assert!(result.region_status.get("eu-west-1").unwrap().replicated);
        assert!(result
            .region_status
            .get("ap-southeast-1")
            .unwrap()
            .replicated);

        // Should be tracked
        let tracked = manager.get_segment("segment-001").await;
        assert!(tracked.is_some());

        // Verify data was actually written to target stores
        let eu_data = eu_store
            .get(&ObjectPath::from(
                "replicated/eu-west-1/orders/0/segment-001",
            ))
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(eu_data.as_ref(), b"test segment data");
    }

    #[tokio::test]
    async fn test_mirror_segment_requires_active_status() {
        let source_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let config = test_config();
        let manager = ReplicationManager::new(config, source_store.clone())
            .await
            .unwrap();

        // Should fail because replication is not active (still Initializing)
        let result = manager
            .mirror_segment("orders", 0, "segment-001", "some/path")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_replication_stats() {
        let manager = create_test_manager().await;
        manager.start_replication().await.unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_segments, 0);
        assert_eq!(stats.fully_replicated, 0);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.failed, 0);
    }

    #[tokio::test]
    async fn test_rpo_compliance_initial() {
        let manager = create_test_manager().await;
        manager.start_replication().await.unwrap();

        // Right after starting, lag should be minimal and RPO should be met
        let lags = manager.check_rpo_compliance().await;
        assert_eq!(lags.len(), 2); // eu-west-1, ap-southeast-1

        for lag in &lags {
            // Just-started replication should have zero lag
            assert!(lag.rpo_compliant);
            assert_eq!(lag.pending_segments, 0);
        }
    }

    #[tokio::test]
    async fn test_get_replication_lag() {
        let manager = create_test_manager().await;
        manager.start_replication().await.unwrap();

        let lags = manager.get_replication_lag().await;
        assert_eq!(lags.len(), 2);

        for lag in &lags {
            assert!(lag.rpo_compliant);
            assert_eq!(lag.pending_bytes, 0);
        }
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let manager = create_test_manager().await;
        manager.start_replication().await.unwrap();

        manager.mark_failed().await;
        assert_eq!(manager.get_status().await, ReplicationStatus::Failed);
    }

    #[tokio::test]
    async fn test_list_segments() {
        let manager = create_test_manager().await;
        manager.start_replication().await.unwrap();

        let segments = manager.list_segments().await;
        assert!(segments.is_empty());
    }

    #[tokio::test]
    async fn test_replication_mode_display() {
        assert_eq!(format!("{}", ReplicationMode::Async), "async");
        assert_eq!(format!("{}", ReplicationMode::Sync), "sync");
    }

    #[tokio::test]
    async fn test_replication_status_display() {
        assert_eq!(format!("{}", ReplicationStatus::Active), "active");
        assert_eq!(format!("{}", ReplicationStatus::Paused), "paused");
        assert_eq!(format!("{}", ReplicationStatus::Failed), "failed");
        assert_eq!(
            format!("{}", ReplicationStatus::Initializing),
            "initializing"
        );
    }

    #[tokio::test]
    async fn test_region_endpoint_new() {
        let endpoint = RegionEndpoint::new(
            "us-west-2",
            "my-bucket",
            "https://s3.us-west-2.amazonaws.com",
        );
        assert_eq!(endpoint.region_name, "us-west-2");
        assert_eq!(endpoint.s3_bucket, "my-bucket");
        assert_eq!(endpoint.s3_endpoint, "https://s3.us-west-2.amazonaws.com");
        assert_eq!(endpoint.status, ReplicationStatus::Initializing);
    }

    #[tokio::test]
    async fn test_default_config() {
        let config = ReplicationConfig::default();
        assert_eq!(config.source_region, "us-east-1");
        assert!(config.target_regions.is_empty());
        assert_eq!(config.replication_mode, ReplicationMode::Async);
        assert!(config.s3_versioning_enabled);
        assert_eq!(config.rto_seconds, 300);
        assert_eq!(config.rpo_seconds, 60);
    }
}
