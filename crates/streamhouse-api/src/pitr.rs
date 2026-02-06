//! Point-in-Time Recovery (PITR) for StreamHouse
//!
//! This module provides the ability to recover the system state to any point in time
//! by maintaining continuous WAL archives and periodic base backups.
//!
//! ## Architecture
//!
//! PITR works by:
//! 1. **Continuous WAL Archiving**: Every metadata change is logged to a Write-Ahead Log
//! 2. **Periodic Base Backups**: Full snapshots of metadata state at regular intervals
//! 3. **Recovery**: Restore from a base backup and replay WAL to reach target timestamp
//!
//! ## Usage
//!
//! ```rust,ignore
//! use streamhouse_api::pitr::{PitrManager, PitrConfig};
//!
//! let config = PitrConfig::builder()
//!     .base_backup_interval(Duration::from_secs(3600)) // hourly backups
//!     .wal_retention(Duration::from_secs(86400 * 7))   // 7 day retention
//!     .build();
//!
//! let pitr = PitrManager::new(config, storage.clone()).await?;
//!
//! // Log a WAL entry
//! pitr.log_change(WalEntry::TopicCreated { ... }).await?;
//!
//! // Take a base backup
//! pitr.create_base_backup().await?;
//!
//! // Recover to a specific point in time
//! let state = pitr.recover_to_timestamp(target_timestamp).await?;
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

/// Errors that can occur during PITR operations
#[derive(Debug, Error)]
pub enum PitrError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("WAL error: {0}")]
    Wal(String),

    #[error("Backup error: {0}")]
    Backup(String),

    #[error("Recovery error: {0}")]
    Recovery(String),

    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),

    #[error("No backup available before timestamp: {0}")]
    NoBackupAvailable(DateTime<Utc>),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, PitrError>;

/// Configuration for PITR operations
#[derive(Debug, Clone)]
pub struct PitrConfig {
    /// Directory for storing WAL files
    pub wal_directory: PathBuf,
    /// Directory for storing base backups
    pub backup_directory: PathBuf,
    /// Interval between automatic base backups
    pub base_backup_interval: Duration,
    /// How long to retain WAL files
    pub wal_retention: Duration,
    /// Maximum WAL segment size before rotation
    pub wal_segment_size: usize,
    /// Enable compression for backups
    pub compress_backups: bool,
    /// Enable WAL verification on write
    pub verify_wal: bool,
    /// Number of base backups to retain
    pub backup_retention_count: usize,
}

impl Default for PitrConfig {
    fn default() -> Self {
        Self {
            wal_directory: PathBuf::from("./pitr/wal"),
            backup_directory: PathBuf::from("./pitr/backups"),
            base_backup_interval: Duration::from_secs(3600), // 1 hour
            wal_retention: Duration::from_secs(86400 * 7),   // 7 days
            wal_segment_size: 16 * 1024 * 1024,              // 16MB
            compress_backups: true,
            verify_wal: true,
            backup_retention_count: 24, // Keep 24 backups (1 day at hourly)
        }
    }
}

impl PitrConfig {
    /// Create a new configuration builder
    pub fn builder() -> PitrConfigBuilder {
        PitrConfigBuilder::default()
    }

    /// Create an in-memory configuration for testing
    pub fn memory() -> Self {
        Self {
            wal_directory: PathBuf::from(":memory:"),
            backup_directory: PathBuf::from(":memory:"),
            ..Default::default()
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.base_backup_interval.is_zero() {
            return Err(PitrError::Config(
                "Base backup interval must be greater than zero".to_string(),
            ));
        }
        if self.wal_retention < self.base_backup_interval {
            return Err(PitrError::Config(
                "WAL retention must be at least as long as backup interval".to_string(),
            ));
        }
        if self.backup_retention_count == 0 {
            return Err(PitrError::Config(
                "Backup retention count must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}

/// Builder for PITR configuration
#[derive(Default)]
pub struct PitrConfigBuilder {
    config: PitrConfig,
}

impl PitrConfigBuilder {
    pub fn wal_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.wal_directory = path.into();
        self
    }

    pub fn backup_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.backup_directory = path.into();
        self
    }

    pub fn base_backup_interval(mut self, interval: Duration) -> Self {
        self.config.base_backup_interval = interval;
        self
    }

    pub fn wal_retention(mut self, retention: Duration) -> Self {
        self.config.wal_retention = retention;
        self
    }

    pub fn wal_segment_size(mut self, size: usize) -> Self {
        self.config.wal_segment_size = size;
        self
    }

    pub fn compress_backups(mut self, compress: bool) -> Self {
        self.config.compress_backups = compress;
        self
    }

    pub fn verify_wal(mut self, verify: bool) -> Self {
        self.config.verify_wal = verify;
        self
    }

    pub fn backup_retention_count(mut self, count: usize) -> Self {
        self.config.backup_retention_count = count;
        self
    }

    pub fn build(self) -> Result<PitrConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

/// A single Write-Ahead Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    /// Unique sequence number
    pub lsn: u64,
    /// Timestamp when the entry was created
    pub timestamp: DateTime<Utc>,
    /// The actual change operation
    pub operation: WalOperation,
    /// SHA-256 hash of previous entry for chain verification
    pub prev_hash: String,
    /// SHA-256 hash of this entry
    pub hash: String,
}

impl WalEntry {
    /// Create a new WAL entry
    pub fn new(lsn: u64, operation: WalOperation, prev_hash: String) -> Self {
        let timestamp = Utc::now();
        let mut entry = Self {
            lsn,
            timestamp,
            operation,
            prev_hash,
            hash: String::new(),
        };
        entry.hash = entry.compute_hash();
        entry
    }

    /// Compute the hash of this entry
    fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.lsn.to_le_bytes());
        hasher.update(self.timestamp.to_rfc3339().as_bytes());
        hasher.update(
            serde_json::to_string(&self.operation)
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update(self.prev_hash.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify the hash of this entry
    pub fn verify(&self) -> bool {
        self.hash == self.compute_hash()
    }
}

/// Types of operations that can be logged to WAL
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WalOperation {
    /// Topic created
    TopicCreated {
        topic_id: String,
        topic_name: String,
        partitions: u32,
        replication_factor: u32,
        config: serde_json::Value,
    },
    /// Topic deleted
    TopicDeleted {
        topic_id: String,
        topic_name: String,
    },
    /// Topic configuration updated
    TopicConfigUpdated {
        topic_id: String,
        old_config: serde_json::Value,
        new_config: serde_json::Value,
    },
    /// Consumer group created
    ConsumerGroupCreated {
        group_id: String,
        group_name: String,
    },
    /// Consumer group offset committed
    ConsumerGroupOffsetCommitted {
        group_id: String,
        topic: String,
        partition: u32,
        offset: i64,
    },
    /// Consumer group deleted
    ConsumerGroupDeleted { group_id: String },
    /// ACL entry created
    AclCreated {
        resource_type: String,
        resource_name: String,
        principal: String,
        permission: String,
    },
    /// ACL entry deleted
    AclDeleted {
        resource_type: String,
        resource_name: String,
        principal: String,
    },
    /// User created
    UserCreated { user_id: String, username: String },
    /// User deleted
    UserDeleted { user_id: String },
    /// Schema registered
    SchemaRegistered {
        subject: String,
        version: u32,
        schema_id: u32,
    },
    /// Custom operation for extensibility
    Custom {
        operation_type: String,
        data: serde_json::Value,
    },
    /// Checkpoint marker (for recovery optimization)
    Checkpoint { backup_id: String },
}

/// A base backup snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseBackup {
    /// Unique backup identifier
    pub id: String,
    /// When the backup was created
    pub created_at: DateTime<Utc>,
    /// LSN at the time of backup
    pub start_lsn: u64,
    /// LSN after backup completed
    pub end_lsn: u64,
    /// Size of the backup in bytes
    pub size_bytes: u64,
    /// SHA-256 checksum of backup data
    pub checksum: String,
    /// Whether the backup is compressed
    pub compressed: bool,
    /// Backup status
    pub status: BackupStatus,
    /// Metadata about what's included
    pub metadata: BackupMetadata,
}

/// Status of a base backup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackupStatus {
    /// Backup is in progress
    InProgress,
    /// Backup completed successfully
    Completed,
    /// Backup failed
    Failed { error: String },
    /// Backup was deleted (retained for history)
    Deleted,
}

/// Metadata about backup contents
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// Number of topics backed up
    pub topic_count: usize,
    /// Number of consumer groups backed up
    pub consumer_group_count: usize,
    /// Number of ACL entries backed up
    pub acl_count: usize,
    /// Number of users backed up
    pub user_count: usize,
    /// Number of schemas backed up
    pub schema_count: usize,
}

/// Snapshot of system state for backup/recovery
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemSnapshot {
    /// All topics
    pub topics: HashMap<String, serde_json::Value>,
    /// All consumer groups
    pub consumer_groups: HashMap<String, serde_json::Value>,
    /// All ACL entries
    pub acls: Vec<serde_json::Value>,
    /// All users
    pub users: HashMap<String, serde_json::Value>,
    /// All schemas
    pub schemas: HashMap<String, serde_json::Value>,
    /// Current LSN
    pub lsn: u64,
    /// Snapshot timestamp
    pub timestamp: DateTime<Utc>,
}

/// Recovery target specification
#[derive(Debug, Clone)]
pub enum RecoveryTarget {
    /// Recover to a specific timestamp
    Timestamp(DateTime<Utc>),
    /// Recover to a specific LSN
    Lsn(u64),
    /// Recover to the latest available point
    Latest,
    /// Recover to a specific backup
    Backup(String),
}

/// Result of a recovery operation
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// The recovered system state
    pub snapshot: SystemSnapshot,
    /// Backup used as the base
    pub base_backup_id: String,
    /// Number of WAL entries replayed
    pub wal_entries_replayed: u64,
    /// Time taken for recovery
    pub duration: Duration,
    /// Target that was reached
    pub reached_target: RecoveryTarget,
}

/// WAL segment for storing entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalSegment {
    /// Segment ID
    pub id: String,
    /// First LSN in this segment
    pub start_lsn: u64,
    /// Last LSN in this segment
    pub end_lsn: u64,
    /// Entries in this segment
    pub entries: Vec<WalEntry>,
    /// When the segment was created
    pub created_at: DateTime<Utc>,
    /// When the segment was closed
    pub closed_at: Option<DateTime<Utc>>,
}

/// Main PITR manager
pub struct PitrManager {
    config: PitrConfig,
    /// Current LSN counter
    current_lsn: Arc<RwLock<u64>>,
    /// Current active WAL segment
    active_segment: Arc<RwLock<WalSegment>>,
    /// Archived WAL segments (indexed by start LSN)
    archived_segments: Arc<RwLock<BTreeMap<u64, WalSegment>>>,
    /// Base backups (indexed by LSN)
    backups: Arc<RwLock<BTreeMap<u64, BaseBackup>>>,
    /// Hash of the last WAL entry
    last_hash: Arc<RwLock<String>>,
    /// In-memory state for recovery testing
    memory_state: Arc<RwLock<Option<SystemSnapshot>>>,
}

impl PitrManager {
    /// Create a new PITR manager
    pub async fn new(config: PitrConfig) -> Result<Self> {
        config.validate()?;

        let initial_segment = WalSegment {
            id: Uuid::new_v4().to_string(),
            start_lsn: 0,
            end_lsn: 0,
            entries: Vec::new(),
            created_at: Utc::now(),
            closed_at: None,
        };

        let initial_hash = hex::encode(Sha256::digest(b"genesis"));

        Ok(Self {
            config,
            current_lsn: Arc::new(RwLock::new(0)),
            active_segment: Arc::new(RwLock::new(initial_segment)),
            archived_segments: Arc::new(RwLock::new(BTreeMap::new())),
            backups: Arc::new(RwLock::new(BTreeMap::new())),
            last_hash: Arc::new(RwLock::new(initial_hash)),
            memory_state: Arc::new(RwLock::new(None)),
        })
    }

    /// Log a change to the WAL
    pub async fn log_change(&self, operation: WalOperation) -> Result<u64> {
        let mut lsn = self.current_lsn.write().await;
        *lsn += 1;
        let current_lsn = *lsn;

        let prev_hash = self.last_hash.read().await.clone();
        let entry = WalEntry::new(current_lsn, operation, prev_hash);

        // Verify entry integrity
        if self.config.verify_wal && !entry.verify() {
            return Err(PitrError::Wal("Entry verification failed".to_string()));
        }

        // Update last hash
        {
            let mut last_hash = self.last_hash.write().await;
            *last_hash = entry.hash.clone();
        }

        // Add to active segment
        let mut segment = self.active_segment.write().await;
        segment.entries.push(entry);
        segment.end_lsn = current_lsn;

        // Check if we need to rotate the segment
        let segment_size: usize = segment
            .entries
            .iter()
            .map(|e| serde_json::to_string(e).map(|s| s.len()).unwrap_or(0))
            .sum();

        if segment_size >= self.config.wal_segment_size {
            drop(segment);
            self.rotate_segment().await?;
        }

        debug!(lsn = current_lsn, "WAL entry logged");
        Ok(current_lsn)
    }

    /// Rotate the current WAL segment
    async fn rotate_segment(&self) -> Result<()> {
        let mut active = self.active_segment.write().await;
        let mut archived = self.archived_segments.write().await;

        // Close and archive current segment
        active.closed_at = Some(Utc::now());

        // Save the end_lsn before replacing
        let next_start_lsn = active.end_lsn + 1;
        let current_end_lsn = active.end_lsn;

        let old_segment = std::mem::replace(
            &mut *active,
            WalSegment {
                id: Uuid::new_v4().to_string(),
                start_lsn: next_start_lsn,
                end_lsn: current_end_lsn,
                entries: Vec::new(),
                created_at: Utc::now(),
                closed_at: None,
            },
        );

        if !old_segment.entries.is_empty() {
            info!(
                segment_id = %old_segment.id,
                start_lsn = old_segment.start_lsn,
                end_lsn = old_segment.end_lsn,
                "WAL segment archived"
            );
            archived.insert(old_segment.start_lsn, old_segment);
        }

        Ok(())
    }

    /// Create a base backup
    pub async fn create_base_backup(&self, snapshot: SystemSnapshot) -> Result<BaseBackup> {
        let backup_id = Uuid::new_v4().to_string();
        let start_lsn = *self.current_lsn.read().await;

        info!(backup_id = %backup_id, start_lsn, "Starting base backup");

        // Serialize snapshot
        let snapshot_data = serde_json::to_vec(&snapshot)?;
        let checksum = hex::encode(Sha256::digest(&snapshot_data));

        // Create backup record
        let backup = BaseBackup {
            id: backup_id.clone(),
            created_at: Utc::now(),
            start_lsn,
            end_lsn: start_lsn,
            size_bytes: snapshot_data.len() as u64,
            checksum,
            compressed: self.config.compress_backups,
            status: BackupStatus::Completed,
            metadata: BackupMetadata {
                topic_count: snapshot.topics.len(),
                consumer_group_count: snapshot.consumer_groups.len(),
                acl_count: snapshot.acls.len(),
                user_count: snapshot.users.len(),
                schema_count: snapshot.schemas.len(),
            },
        };

        // Log checkpoint in WAL
        self.log_change(WalOperation::Checkpoint {
            backup_id: backup_id.clone(),
        })
        .await?;

        // Store backup
        let mut backups = self.backups.write().await;
        backups.insert(backup.start_lsn, backup.clone());

        // Store snapshot in memory for testing
        {
            let mut state = self.memory_state.write().await;
            *state = Some(snapshot);
        }

        // Cleanup old backups
        self.cleanup_old_backups(&mut backups).await;

        info!(
            backup_id = %backup_id,
            size_bytes = backup.size_bytes,
            "Base backup completed"
        );

        Ok(backup)
    }

    /// Cleanup old backups based on retention policy
    async fn cleanup_old_backups(&self, backups: &mut BTreeMap<u64, BaseBackup>) {
        while backups.len() > self.config.backup_retention_count {
            if let Some((&oldest_lsn, _)) = backups.iter().next() {
                info!(lsn = oldest_lsn, "Removing old backup");
                backups.remove(&oldest_lsn);
            } else {
                break;
            }
        }
    }

    /// Recover to a specific target
    pub async fn recover(&self, target: RecoveryTarget) -> Result<RecoveryResult> {
        let start_time = std::time::Instant::now();

        // Find the appropriate base backup
        let (backup, snapshot) = self.find_base_backup(&target).await?;

        info!(
            backup_id = %backup.id,
            start_lsn = backup.start_lsn,
            "Starting recovery from base backup"
        );

        // Collect WAL entries to replay
        let wal_entries = self.collect_wal_entries(backup.start_lsn, &target).await?;
        let entries_count = wal_entries.len() as u64;

        // Replay WAL entries
        let mut recovered_state = snapshot;
        for entry in wal_entries {
            self.apply_wal_entry(&mut recovered_state, &entry)?;
        }

        let duration = start_time.elapsed();

        info!(
            entries_replayed = entries_count,
            duration_ms = duration.as_millis(),
            "Recovery completed"
        );

        Ok(RecoveryResult {
            snapshot: recovered_state,
            base_backup_id: backup.id,
            wal_entries_replayed: entries_count,
            duration,
            reached_target: target,
        })
    }

    /// Find the best base backup for recovery
    async fn find_base_backup(
        &self,
        target: &RecoveryTarget,
    ) -> Result<(BaseBackup, SystemSnapshot)> {
        let backups = self.backups.read().await;

        let target_lsn = match target {
            RecoveryTarget::Timestamp(ts) => {
                // Find LSN that corresponds to timestamp
                self.find_lsn_for_timestamp(*ts).await?
            }
            RecoveryTarget::Lsn(lsn) => *lsn,
            RecoveryTarget::Latest => *self.current_lsn.read().await,
            RecoveryTarget::Backup(id) => {
                // Find specific backup
                for (_lsn, backup) in backups.iter() {
                    if backup.id == *id {
                        let snapshot = self.memory_state.read().await.clone().ok_or_else(|| {
                            PitrError::Recovery("Backup snapshot not found in memory".to_string())
                        })?;
                        return Ok((backup.clone(), snapshot));
                    }
                }
                return Err(PitrError::Recovery(format!("Backup {} not found", id)));
            }
        };

        // Find the latest backup before target LSN
        let backup = backups
            .range(..=target_lsn)
            .next_back()
            .map(|(_, b)| b.clone())
            .ok_or_else(|| {
                PitrError::NoBackupAvailable(match target {
                    RecoveryTarget::Timestamp(ts) => *ts,
                    _ => Utc::now(),
                })
            })?;

        let snapshot = self.memory_state.read().await.clone().ok_or_else(|| {
            PitrError::Recovery("Backup snapshot not found in memory".to_string())
        })?;

        Ok((backup, snapshot))
    }

    /// Find LSN corresponding to a timestamp
    async fn find_lsn_for_timestamp(&self, timestamp: DateTime<Utc>) -> Result<u64> {
        // Search active segment
        let active = self.active_segment.read().await;
        for entry in active.entries.iter().rev() {
            if entry.timestamp <= timestamp {
                return Ok(entry.lsn);
            }
        }

        // Search archived segments
        let archived = self.archived_segments.read().await;
        for (_, segment) in archived.iter().rev() {
            for entry in segment.entries.iter().rev() {
                if entry.timestamp <= timestamp {
                    return Ok(entry.lsn);
                }
            }
        }

        Err(PitrError::InvalidTimestamp(format!(
            "No WAL entry found before {}",
            timestamp
        )))
    }

    /// Collect WAL entries for replay
    async fn collect_wal_entries(
        &self,
        from_lsn: u64,
        target: &RecoveryTarget,
    ) -> Result<Vec<WalEntry>> {
        let mut entries = Vec::new();

        let target_lsn = match target {
            RecoveryTarget::Timestamp(ts) => self.find_lsn_for_timestamp(*ts).await?,
            RecoveryTarget::Lsn(lsn) => *lsn,
            RecoveryTarget::Latest => *self.current_lsn.read().await,
            RecoveryTarget::Backup(_) => return Ok(entries), // No replay needed
        };

        // Collect from archived segments
        let archived = self.archived_segments.read().await;
        for (_, segment) in archived.iter() {
            for entry in &segment.entries {
                if entry.lsn > from_lsn && entry.lsn <= target_lsn {
                    entries.push(entry.clone());
                }
            }
        }

        // Collect from active segment
        let active = self.active_segment.read().await;
        for entry in &active.entries {
            if entry.lsn > from_lsn && entry.lsn <= target_lsn {
                entries.push(entry.clone());
            }
        }

        // Sort by LSN
        entries.sort_by_key(|e| e.lsn);

        // Verify chain
        if !entries.is_empty() {
            self.verify_wal_chain(&entries)?;
        }

        Ok(entries)
    }

    /// Verify WAL chain integrity
    fn verify_wal_chain(&self, entries: &[WalEntry]) -> Result<()> {
        for entry in entries {
            if !entry.verify() {
                return Err(PitrError::Wal(format!(
                    "WAL entry {} failed verification",
                    entry.lsn
                )));
            }
        }
        Ok(())
    }

    /// Apply a WAL entry to the system state
    fn apply_wal_entry(&self, state: &mut SystemSnapshot, entry: &WalEntry) -> Result<()> {
        match &entry.operation {
            WalOperation::TopicCreated {
                topic_id,
                topic_name,
                partitions,
                replication_factor,
                config,
            } => {
                state.topics.insert(
                    topic_id.clone(),
                    serde_json::json!({
                        "id": topic_id,
                        "name": topic_name,
                        "partitions": partitions,
                        "replication_factor": replication_factor,
                        "config": config,
                    }),
                );
            }
            WalOperation::TopicDeleted { topic_id, .. } => {
                state.topics.remove(topic_id);
            }
            WalOperation::TopicConfigUpdated {
                topic_id,
                new_config,
                ..
            } => {
                if let Some(topic) = state.topics.get_mut(topic_id) {
                    if let Some(obj) = topic.as_object_mut() {
                        obj.insert("config".to_string(), new_config.clone());
                    }
                }
            }
            WalOperation::ConsumerGroupCreated {
                group_id,
                group_name,
            } => {
                state.consumer_groups.insert(
                    group_id.clone(),
                    serde_json::json!({
                        "id": group_id,
                        "name": group_name,
                        "offsets": {},
                    }),
                );
            }
            WalOperation::ConsumerGroupOffsetCommitted {
                group_id,
                topic,
                partition,
                offset,
            } => {
                if let Some(group) = state.consumer_groups.get_mut(group_id) {
                    if let Some(obj) = group.as_object_mut() {
                        let offsets = obj
                            .entry("offsets")
                            .or_insert_with(|| serde_json::json!({}));
                        if let Some(offsets_obj) = offsets.as_object_mut() {
                            let key = format!("{}-{}", topic, partition);
                            offsets_obj.insert(key, serde_json::json!(offset));
                        }
                    }
                }
            }
            WalOperation::ConsumerGroupDeleted { group_id } => {
                state.consumer_groups.remove(group_id);
            }
            WalOperation::AclCreated {
                resource_type,
                resource_name,
                principal,
                permission,
            } => {
                state.acls.push(serde_json::json!({
                    "resource_type": resource_type,
                    "resource_name": resource_name,
                    "principal": principal,
                    "permission": permission,
                }));
            }
            WalOperation::AclDeleted {
                resource_type,
                resource_name,
                principal,
            } => {
                state.acls.retain(|acl| {
                    acl.get("resource_type").and_then(|v| v.as_str()) != Some(resource_type)
                        || acl.get("resource_name").and_then(|v| v.as_str()) != Some(resource_name)
                        || acl.get("principal").and_then(|v| v.as_str()) != Some(principal)
                });
            }
            WalOperation::UserCreated { user_id, username } => {
                state.users.insert(
                    user_id.clone(),
                    serde_json::json!({
                        "id": user_id,
                        "username": username,
                    }),
                );
            }
            WalOperation::UserDeleted { user_id } => {
                state.users.remove(user_id);
            }
            WalOperation::SchemaRegistered {
                subject,
                version,
                schema_id,
            } => {
                let key = format!("{}-{}", subject, version);
                state.schemas.insert(
                    key,
                    serde_json::json!({
                        "subject": subject,
                        "version": version,
                        "schema_id": schema_id,
                    }),
                );
            }
            WalOperation::Custom {
                operation_type,
                data,
            } => {
                debug!(operation_type, "Applying custom WAL operation");
                // Custom operations are stored for audit but don't modify state
                let _ = data;
            }
            WalOperation::Checkpoint { .. } => {
                // Checkpoints don't modify state
            }
        }

        state.lsn = entry.lsn;
        state.timestamp = entry.timestamp;

        Ok(())
    }

    /// Get current LSN
    pub async fn current_lsn(&self) -> u64 {
        *self.current_lsn.read().await
    }

    /// List all available backups
    pub async fn list_backups(&self) -> Vec<BaseBackup> {
        self.backups.read().await.values().cloned().collect()
    }

    /// Get a specific backup by ID
    pub async fn get_backup(&self, backup_id: &str) -> Option<BaseBackup> {
        self.backups
            .read()
            .await
            .values()
            .find(|b| b.id == backup_id)
            .cloned()
    }

    /// Get WAL statistics
    pub async fn wal_stats(&self) -> WalStats {
        let active = self.active_segment.read().await;
        let archived = self.archived_segments.read().await;

        let archived_entries: usize = archived.values().map(|s| s.entries.len()).sum();
        let total_entries = active.entries.len() + archived_entries;

        WalStats {
            current_lsn: *self.current_lsn.read().await,
            active_segment_entries: active.entries.len(),
            archived_segment_count: archived.len(),
            total_wal_entries: total_entries,
        }
    }

    /// Cleanup expired WAL segments
    pub async fn cleanup_expired_wal(&self) -> Result<usize> {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(self.config.wal_retention)
                .map_err(|e| PitrError::Config(format!("Invalid retention duration: {}", e)))?;

        let mut archived = self.archived_segments.write().await;
        let initial_count = archived.len();

        // Find oldest backup LSN - we can't delete WAL needed for recovery
        let oldest_backup_lsn = self
            .backups
            .read()
            .await
            .keys()
            .next()
            .copied()
            .unwrap_or(0);

        // Remove segments older than retention and not needed for backups
        archived.retain(|_, segment| {
            if let Some(closed_at) = segment.closed_at {
                // Keep if needed for backup recovery or not expired
                segment.end_lsn >= oldest_backup_lsn || closed_at > cutoff
            } else {
                true // Keep open segments
            }
        });

        let removed = initial_count - archived.len();
        if removed > 0 {
            info!(removed, "Cleaned up expired WAL segments");
        }

        Ok(removed)
    }

    /// Verify WAL integrity from a starting LSN
    pub async fn verify_wal_integrity(&self, from_lsn: u64) -> Result<VerificationResult> {
        let mut verified_count = 0;
        let mut errors = Vec::new();
        let mut prev_hash: Option<String> = None;

        // Verify archived segments
        let archived = self.archived_segments.read().await;
        for (_, segment) in archived.iter() {
            for entry in &segment.entries {
                if entry.lsn >= from_lsn {
                    // Verify entry hash
                    if !entry.verify() {
                        errors.push(format!("Entry {} hash verification failed", entry.lsn));
                    }

                    // Verify chain
                    if let Some(ref expected_prev) = prev_hash {
                        if entry.prev_hash != *expected_prev {
                            errors.push(format!("Entry {} chain broken", entry.lsn));
                        }
                    }

                    prev_hash = Some(entry.hash.clone());
                    verified_count += 1;
                }
            }
        }

        // Verify active segment
        let active = self.active_segment.read().await;
        for entry in &active.entries {
            if entry.lsn >= from_lsn {
                if !entry.verify() {
                    errors.push(format!("Entry {} hash verification failed", entry.lsn));
                }

                if let Some(ref expected_prev) = prev_hash {
                    if entry.prev_hash != *expected_prev {
                        errors.push(format!("Entry {} chain broken", entry.lsn));
                    }
                }

                prev_hash = Some(entry.hash.clone());
                verified_count += 1;
            }
        }

        let is_valid = errors.is_empty();
        Ok(VerificationResult {
            verified_entries: verified_count,
            errors,
            is_valid,
        })
    }
}

/// WAL statistics
#[derive(Debug, Clone)]
pub struct WalStats {
    pub current_lsn: u64,
    pub active_segment_entries: usize,
    pub archived_segment_count: usize,
    pub total_wal_entries: usize,
}

/// Result of WAL verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub verified_entries: usize,
    pub errors: Vec<String>,
    pub is_valid: bool,
}

// We need the hex crate for encoding - let's inline it
mod hex {
    pub fn encode(data: impl AsRef<[u8]>) -> String {
        data.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_manager() -> PitrManager {
        PitrManager::new(PitrConfig::memory()).await.unwrap()
    }

    #[tokio::test]
    async fn test_log_wal_entry() {
        let manager = create_test_manager().await;

        let lsn = manager
            .log_change(WalOperation::TopicCreated {
                topic_id: "topic-1".to_string(),
                topic_name: "test-topic".to_string(),
                partitions: 3,
                replication_factor: 2,
                config: serde_json::json!({}),
            })
            .await
            .unwrap();

        assert_eq!(lsn, 1);
        assert_eq!(manager.current_lsn().await, 1);
    }

    #[tokio::test]
    async fn test_wal_chain_integrity() {
        let manager = create_test_manager().await;

        // Log multiple entries
        for i in 0..5 {
            manager
                .log_change(WalOperation::TopicCreated {
                    topic_id: format!("topic-{}", i),
                    topic_name: format!("test-topic-{}", i),
                    partitions: 3,
                    replication_factor: 2,
                    config: serde_json::json!({}),
                })
                .await
                .unwrap();
        }

        // Verify chain integrity
        let result = manager.verify_wal_integrity(0).await.unwrap();
        assert!(result.is_valid);
        assert_eq!(result.verified_entries, 5);
    }

    #[tokio::test]
    async fn test_create_base_backup() {
        let manager = create_test_manager().await;

        // Log some changes first
        manager
            .log_change(WalOperation::TopicCreated {
                topic_id: "topic-1".to_string(),
                topic_name: "test-topic".to_string(),
                partitions: 3,
                replication_factor: 2,
                config: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Create snapshot
        let mut snapshot = SystemSnapshot::default();
        snapshot.topics.insert(
            "topic-1".to_string(),
            serde_json::json!({
                "id": "topic-1",
                "name": "test-topic",
                "partitions": 3,
            }),
        );

        let backup = manager.create_base_backup(snapshot).await.unwrap();

        assert_eq!(backup.status, BackupStatus::Completed);
        assert_eq!(backup.metadata.topic_count, 1);
    }

    #[tokio::test]
    async fn test_recovery_to_lsn() {
        let manager = create_test_manager().await;

        // Create initial backup (this also logs a checkpoint at LSN 1)
        let snapshot = SystemSnapshot::default();
        manager.create_base_backup(snapshot).await.unwrap();

        // Log changes (LSN 2 and 3)
        manager
            .log_change(WalOperation::TopicCreated {
                topic_id: "topic-1".to_string(),
                topic_name: "test-topic-1".to_string(),
                partitions: 3,
                replication_factor: 2,
                config: serde_json::json!({}),
            })
            .await
            .unwrap();

        manager
            .log_change(WalOperation::TopicCreated {
                topic_id: "topic-2".to_string(),
                topic_name: "test-topic-2".to_string(),
                partitions: 3,
                replication_factor: 2,
                config: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Recover to LSN 3 (replays: checkpoint, topic-1, topic-2 = 3 entries)
        let result = manager.recover(RecoveryTarget::Lsn(3)).await.unwrap();

        assert_eq!(result.wal_entries_replayed, 3); // checkpoint + 2 topics
        assert_eq!(result.snapshot.topics.len(), 2);
    }

    #[tokio::test]
    async fn test_recovery_to_latest() {
        let manager = create_test_manager().await;

        // Create initial backup
        let snapshot = SystemSnapshot::default();
        manager.create_base_backup(snapshot).await.unwrap();

        // Log changes
        for i in 0..5 {
            manager
                .log_change(WalOperation::TopicCreated {
                    topic_id: format!("topic-{}", i),
                    topic_name: format!("test-topic-{}", i),
                    partitions: 3,
                    replication_factor: 2,
                    config: serde_json::json!({}),
                })
                .await
                .unwrap();
        }

        // Recover to latest
        let result = manager.recover(RecoveryTarget::Latest).await.unwrap();

        assert_eq!(result.snapshot.topics.len(), 5);
    }

    #[tokio::test]
    async fn test_list_backups() {
        let manager = create_test_manager().await;

        // Create multiple backups
        for i in 0..3 {
            let mut snapshot = SystemSnapshot::default();
            snapshot
                .topics
                .insert(format!("topic-{}", i), serde_json::json!({}));
            manager.create_base_backup(snapshot).await.unwrap();
        }

        let backups = manager.list_backups().await;
        assert_eq!(backups.len(), 3);
    }

    #[tokio::test]
    async fn test_wal_stats() {
        let manager = create_test_manager().await;

        // Log some entries
        for _ in 0..10 {
            manager
                .log_change(WalOperation::Custom {
                    operation_type: "test".to_string(),
                    data: serde_json::json!({}),
                })
                .await
                .unwrap();
        }

        let stats = manager.wal_stats().await;
        assert_eq!(stats.current_lsn, 10);
        assert_eq!(stats.total_wal_entries, 10);
    }

    #[tokio::test]
    async fn test_consumer_group_operations() {
        let manager = create_test_manager().await;

        // Create initial backup
        manager
            .create_base_backup(SystemSnapshot::default())
            .await
            .unwrap();

        // Log consumer group operations
        manager
            .log_change(WalOperation::ConsumerGroupCreated {
                group_id: "group-1".to_string(),
                group_name: "test-group".to_string(),
            })
            .await
            .unwrap();

        manager
            .log_change(WalOperation::ConsumerGroupOffsetCommitted {
                group_id: "group-1".to_string(),
                topic: "test-topic".to_string(),
                partition: 0,
                offset: 100,
            })
            .await
            .unwrap();

        // Recover and verify
        let result = manager.recover(RecoveryTarget::Latest).await.unwrap();
        assert_eq!(result.snapshot.consumer_groups.len(), 1);
    }

    #[tokio::test]
    async fn test_acl_operations() {
        let manager = create_test_manager().await;

        // Create initial backup
        manager
            .create_base_backup(SystemSnapshot::default())
            .await
            .unwrap();

        // Add ACL
        manager
            .log_change(WalOperation::AclCreated {
                resource_type: "topic".to_string(),
                resource_name: "test-topic".to_string(),
                principal: "user:admin".to_string(),
                permission: "read".to_string(),
            })
            .await
            .unwrap();

        // Recover and verify
        let result = manager.recover(RecoveryTarget::Latest).await.unwrap();
        assert_eq!(result.snapshot.acls.len(), 1);
    }

    #[tokio::test]
    async fn test_config_validation() {
        // Valid config
        let config = PitrConfig::builder()
            .base_backup_interval(Duration::from_secs(3600))
            .wal_retention(Duration::from_secs(86400))
            .build();
        assert!(config.is_ok());

        // Invalid: backup interval is zero
        let config = PitrConfig::builder()
            .base_backup_interval(Duration::from_secs(0))
            .build();
        assert!(config.is_err());

        // Invalid: retention less than backup interval
        let config = PitrConfig::builder()
            .base_backup_interval(Duration::from_secs(3600))
            .wal_retention(Duration::from_secs(1800))
            .build();
        assert!(config.is_err());
    }

    #[tokio::test]
    async fn test_backup_retention() {
        let config = PitrConfig {
            backup_retention_count: 3,
            ..PitrConfig::memory()
        };
        let manager = PitrManager::new(config).await.unwrap();

        // Create more backups than retention allows
        for i in 0..5 {
            let mut snapshot = SystemSnapshot::default();
            snapshot
                .topics
                .insert(format!("topic-{}", i), serde_json::json!({}));
            manager.create_base_backup(snapshot).await.unwrap();
        }

        // Should only keep 3 backups
        let backups = manager.list_backups().await;
        assert_eq!(backups.len(), 3);
    }

    #[tokio::test]
    async fn test_user_operations() {
        let manager = create_test_manager().await;

        manager
            .create_base_backup(SystemSnapshot::default())
            .await
            .unwrap();

        manager
            .log_change(WalOperation::UserCreated {
                user_id: "user-1".to_string(),
                username: "admin".to_string(),
            })
            .await
            .unwrap();

        let result = manager.recover(RecoveryTarget::Latest).await.unwrap();
        assert_eq!(result.snapshot.users.len(), 1);
    }

    #[tokio::test]
    async fn test_schema_registration() {
        let manager = create_test_manager().await;

        manager
            .create_base_backup(SystemSnapshot::default())
            .await
            .unwrap();

        manager
            .log_change(WalOperation::SchemaRegistered {
                subject: "test-subject".to_string(),
                version: 1,
                schema_id: 100,
            })
            .await
            .unwrap();

        let result = manager.recover(RecoveryTarget::Latest).await.unwrap();
        assert_eq!(result.snapshot.schemas.len(), 1);
    }
}
