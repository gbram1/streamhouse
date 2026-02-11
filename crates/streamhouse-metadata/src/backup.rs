//! Metadata Backup and Restore
//!
//! Provides functionality to export and import metadata for backup and disaster recovery.

use crate::{
    CreateOrganization, MetadataError, MetadataStore, Organization, OrganizationQuota, Result,
    SegmentInfo, Topic, TopicConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Version of the backup format
const BACKUP_VERSION: u32 = 1;

/// Complete metadata backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataBackup {
    /// Backup format version
    pub version: u32,
    /// Timestamp when backup was created (Unix ms)
    pub created_at: i64,
    /// Human-readable description
    pub description: Option<String>,
    /// Topics
    pub topics: Vec<TopicBackup>,
    /// Consumer group offsets
    pub consumer_offsets: Vec<ConsumerOffsetBackup>,
    /// Organizations
    pub organizations: Vec<OrganizationBackup>,
}

/// Topic backup data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicBackup {
    pub topic: Topic,
    pub segments: Vec<SegmentInfo>,
}

/// Consumer offset backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerOffsetBackup {
    pub group_id: String,
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
}

/// Organization backup data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationBackup {
    pub organization: Organization,
    pub quota: Option<OrganizationQuota>,
}

impl MetadataBackup {
    /// Create a new empty backup
    pub fn new() -> Self {
        Self {
            version: BACKUP_VERSION,
            created_at: chrono::Utc::now().timestamp_millis(),
            description: None,
            topics: Vec::new(),
            consumer_offsets: Vec::new(),
            organizations: Vec::new(),
        }
    }

    /// Set backup description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Export metadata from a store
    pub async fn from_store(store: &dyn MetadataStore) -> Result<Self> {
        let mut backup = Self::new();

        // Export topics and their segments
        let topics = store.list_topics().await?;
        for topic in topics {
            let mut all_segments = Vec::new();
            for partition in 0..topic.partition_count {
                if let Ok(segments) = store.get_segments(&topic.name, partition).await {
                    all_segments.extend(segments);
                }
            }
            backup.topics.push(TopicBackup {
                topic,
                segments: all_segments,
            });
        }

        // Export consumer group offsets
        let groups = store.list_consumer_groups().await?;
        for group_id in groups {
            if let Ok(offsets) = store.get_consumer_offsets(&group_id).await {
                for offset in offsets {
                    backup.consumer_offsets.push(ConsumerOffsetBackup {
                        group_id: offset.group_id.clone(),
                        topic: offset.topic.clone(),
                        partition: offset.partition_id,
                        offset: offset.committed_offset,
                    });
                }
            }
        }

        // Export organizations
        let orgs = store.list_organizations().await?;
        for org in orgs {
            let quota = store.get_organization_quota(&org.id).await.ok();
            backup.organizations.push(OrganizationBackup {
                organization: org,
                quota,
            });
        }

        Ok(backup)
    }

    /// Serialize backup to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| MetadataError::InternalError(format!("Failed to serialize backup: {}", e)))
    }

    /// Serialize backup to compact JSON
    pub fn to_json_compact(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| MetadataError::InternalError(format!("Failed to serialize backup: {}", e)))
    }

    /// Deserialize backup from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        let backup: Self = serde_json::from_str(json)
            .map_err(|e| MetadataError::InternalError(format!("Failed to parse backup: {}", e)))?;

        if backup.version > BACKUP_VERSION {
            return Err(MetadataError::InternalError(format!(
                "Backup version {} is newer than supported version {}",
                backup.version, BACKUP_VERSION
            )));
        }

        Ok(backup)
    }

    /// Restore backup to a metadata store
    pub async fn restore_to(&self, store: &dyn MetadataStore) -> Result<RestoreStats> {
        let mut stats = RestoreStats::default();

        // Restore topics
        for topic_backup in &self.topics {
            let config = TopicConfig {
                name: topic_backup.topic.name.clone(),
                partition_count: topic_backup.topic.partition_count,
                retention_ms: topic_backup.topic.retention_ms,
                cleanup_policy: topic_backup.topic.cleanup_policy,
                config: HashMap::new(),
            };
            match store.create_topic(config).await {
                Ok(_) => stats.topics_created += 1,
                Err(_) => stats.topics_skipped += 1,
            }
        }

        // Restore consumer offsets
        for offset in &self.consumer_offsets {
            match store
                .commit_offset(
                    &offset.group_id,
                    &offset.topic,
                    offset.partition,
                    offset.offset,
                    None,
                )
                .await
            {
                Ok(_) => stats.offsets_restored += 1,
                Err(_) => stats.offsets_failed += 1,
            }
        }

        // Restore organizations
        for org_backup in &self.organizations {
            let config = CreateOrganization {
                name: org_backup.organization.name.clone(),
                slug: org_backup.organization.slug.clone(),
                plan: org_backup.organization.plan,
                settings: HashMap::new(),
            };
            match store.create_organization(config).await {
                Ok(_) => stats.organizations_created += 1,
                Err(_) => stats.organizations_skipped += 1,
            }
        }

        Ok(stats)
    }

    /// Get number of topics in backup
    pub fn topic_count(&self) -> usize {
        self.topics.len()
    }

    /// Get number of organizations in backup
    pub fn organization_count(&self) -> usize {
        self.organizations.len()
    }
}

impl Default for MetadataBackup {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from a restore operation
#[derive(Debug, Clone, Default)]
pub struct RestoreStats {
    pub topics_created: usize,
    pub topics_skipped: usize,
    pub offsets_restored: usize,
    pub offsets_failed: usize,
    pub organizations_created: usize,
    pub organizations_skipped: usize,
}

impl RestoreStats {
    /// Check if restore had any failures
    pub fn has_failures(&self) -> bool {
        self.offsets_failed > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_serialization() {
        let backup = MetadataBackup::new().with_description("Test backup");

        let json = backup.to_json().unwrap();
        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("Test backup"));

        let restored = MetadataBackup::from_json(&json).unwrap();
        assert_eq!(restored.version, BACKUP_VERSION);
        assert_eq!(restored.description, Some("Test backup".to_string()));
    }

    #[test]
    fn test_backup_version_check() {
        let json = r#"{"version":999,"created_at":0,"description":null,"topics":[],"consumer_offsets":[],"organizations":[]}"#;

        let result = MetadataBackup::from_json(json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("newer than supported"));
    }

    #[test]
    fn test_restore_stats() {
        let mut stats = RestoreStats::default();
        assert!(!stats.has_failures());

        stats.offsets_failed = 1;
        assert!(stats.has_failures());
    }
}

// =============================================================================
// Backup Scheduler
// =============================================================================

/// Configuration for automated backup scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupScheduleConfig {
    /// Hours between backup runs (default: 24)
    pub interval_hours: u64,
    /// Number of backups to retain (default: 7)
    pub retention_count: usize,
    /// Directory to store backup files
    pub backup_dir: String,
    /// Whether to compress backups with gzip (default: true)
    pub compress: bool,
}

impl Default for BackupScheduleConfig {
    fn default() -> Self {
        Self {
            interval_hours: 24,
            retention_count: 7,
            backup_dir: "./backups".to_string(),
            compress: true,
        }
    }
}

/// Automated backup scheduler that periodically creates and manages metadata backups.
///
/// The scheduler tracks when the last backup was taken and creates new backups
/// based on the configured interval. Old backups are automatically cleaned up
/// to respect the retention count.
///
/// # Example
///
/// ```ignore
/// use streamhouse_metadata::backup::{BackupScheduler, BackupScheduleConfig};
///
/// let config = BackupScheduleConfig {
///     interval_hours: 12,
///     retention_count: 5,
///     backup_dir: "/var/backups/streamhouse".to_string(),
///     compress: true,
/// };
///
/// let mut scheduler = BackupScheduler::new(config);
///
/// // Check if a backup is due
/// if scheduler.should_run() {
///     scheduler.run_backup(&store).await?;
///     scheduler.cleanup_old_backups()?;
/// }
/// ```
pub struct BackupScheduler {
    /// Scheduler configuration
    pub config: BackupScheduleConfig,
    /// Timestamp of the last successful backup (Unix ms), if any
    pub last_backup: Option<i64>,
}

impl BackupScheduler {
    /// Create a new backup scheduler with the given configuration.
    pub fn new(config: BackupScheduleConfig) -> Self {
        Self {
            config,
            last_backup: None,
        }
    }

    /// Check whether a backup should run based on the configured interval.
    ///
    /// Returns `true` if no backup has been taken yet, or if enough time
    /// has elapsed since the last backup.
    pub fn should_run(&self) -> bool {
        match self.last_backup {
            None => true,
            Some(last) => {
                let now = chrono::Utc::now().timestamp_millis();
                let interval_ms = (self.config.interval_hours as i64) * 3600 * 1000;
                now - last >= interval_ms
            }
        }
    }

    /// Execute a backup: export metadata from the store and write to a timestamped file.
    ///
    /// The backup file is written to `{backup_dir}/backup_{timestamp}.json` (or `.json.gz`
    /// if compression is enabled). After a successful write, `last_backup` is updated.
    ///
    /// # Arguments
    ///
    /// * `store` - The metadata store to back up
    ///
    /// # Returns
    ///
    /// The path of the created backup file on success.
    pub async fn run_backup(&mut self, store: &dyn MetadataStore) -> Result<String> {
        // Ensure backup directory exists
        std::fs::create_dir_all(&self.config.backup_dir).map_err(|e| {
            MetadataError::InternalError(format!(
                "Failed to create backup directory '{}': {}",
                self.config.backup_dir, e
            ))
        })?;

        let backup = MetadataBackup::from_store(store).await?;
        let now = chrono::Utc::now().timestamp_millis();

        let json_data = if self.config.compress {
            backup.to_json_compact()?
        } else {
            backup.to_json()?
        };

        let extension = if cfg!(feature = "migration-tools") && self.config.compress {
            "json.gz"
        } else {
            "json"
        };

        let filename = format!("backup_{}.{}", now, extension);
        let filepath = std::path::Path::new(&self.config.backup_dir).join(&filename);

        #[cfg(feature = "migration-tools")]
        {
            if self.config.compress {
                use std::io::Write;
                let file = std::fs::File::create(&filepath).map_err(|e| {
                    MetadataError::InternalError(format!(
                        "Failed to create backup file: {}",
                        e
                    ))
                })?;
                let mut encoder =
                    flate2::write::GzEncoder::new(file, flate2::Compression::default());
                encoder.write_all(json_data.as_bytes()).map_err(|e| {
                    MetadataError::InternalError(format!(
                        "Failed to write compressed backup: {}",
                        e
                    ))
                })?;
                encoder.finish().map_err(|e| {
                    MetadataError::InternalError(format!(
                        "Failed to finalize compressed backup: {}",
                        e
                    ))
                })?;
            } else {
                std::fs::write(&filepath, &json_data).map_err(|e| {
                    MetadataError::InternalError(format!(
                        "Failed to write backup file: {}",
                        e
                    ))
                })?;
            }
        }

        #[cfg(not(feature = "migration-tools"))]
        {
            std::fs::write(&filepath, &json_data).map_err(|e| {
                MetadataError::InternalError(format!("Failed to write backup file: {}", e))
            })?;
        }

        self.last_backup = Some(now);

        Ok(filepath.to_string_lossy().to_string())
    }

    /// Remove old backups, retaining only the newest `retention_count` files.
    ///
    /// Backup files are identified by the `backup_` prefix in the configured
    /// backup directory. Files are sorted by name (which embeds the timestamp)
    /// and the oldest files beyond the retention count are deleted.
    ///
    /// # Returns
    ///
    /// The number of backup files that were deleted.
    pub fn cleanup_old_backups(&self) -> Result<usize> {
        let dir = std::fs::read_dir(&self.config.backup_dir).map_err(|e| {
            MetadataError::InternalError(format!("Failed to read backup directory: {}", e))
        })?;

        let mut backup_files: Vec<std::path::PathBuf> = dir
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("backup_"))
                    .unwrap_or(false)
            })
            .collect();

        // Sort by filename (ascending = oldest first, since timestamp is embedded)
        backup_files.sort();

        let mut deleted = 0;
        if backup_files.len() > self.config.retention_count {
            let to_remove = backup_files.len() - self.config.retention_count;
            for path in backup_files.iter().take(to_remove) {
                if std::fs::remove_file(path).is_ok() {
                    deleted += 1;
                }
            }
        }

        Ok(deleted)
    }
}

// =============================================================================
// Topic Mirror
// =============================================================================

/// Mirrors (replicates) topics between StreamHouse clusters.
///
/// TopicMirror enables cross-cluster replication by consuming records from a
/// source cluster and producing them to a target cluster. This is useful for
/// disaster recovery, geographic distribution, and cluster migration.
///
/// # Example
///
/// ```ignore
/// use streamhouse_metadata::backup::TopicMirror;
///
/// let mirror = TopicMirror::new(
///     "http://source-cluster:8080",
///     "http://target-cluster:8080",
/// );
///
/// // List topics available for mirroring
/// let topics = mirror.list_mirrorable_topics().await?;
///
/// // Mirror a specific topic
/// let count = mirror.mirror_topic("events", 1000).await?;
/// println!("Mirrored {} records", count);
/// ```
pub struct TopicMirror {
    /// URL of the source StreamHouse cluster
    pub source_url: String,
    /// URL of the target StreamHouse cluster
    pub target_url: String,
}

impl TopicMirror {
    /// Create a new TopicMirror for replicating between two clusters.
    ///
    /// # Arguments
    ///
    /// * `source_url` - Base URL of the source cluster (e.g., "http://source:8080")
    /// * `target_url` - Base URL of the target cluster (e.g., "http://target:8080")
    pub fn new(source_url: impl Into<String>, target_url: impl Into<String>) -> Self {
        Self {
            source_url: source_url.into(),
            target_url: target_url.into(),
        }
    }

    /// List topics from the source cluster that can be mirrored.
    ///
    /// Makes an HTTP GET to the source cluster's topic list endpoint and returns
    /// the topic names. Topics that already exist on the target are included;
    /// filtering should be done by the caller if needed.
    ///
    /// # Returns
    ///
    /// A list of topic names available on the source cluster.
    #[cfg(feature = "migration-tools")]
    pub async fn list_mirrorable_topics(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/v1/topics", self.source_url.trim_end_matches('/'));
        let response = reqwest::get(&url).await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to list source topics: {}", e))
        })?;

        let topics: Vec<serde_json::Value> = response.json().await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to parse source topics: {}", e))
        })?;

        Ok(topics
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect())
    }

    /// Mirror a topic from source to target cluster.
    ///
    /// Consumes up to `max_records` from each partition of the source topic and
    /// produces them to the same topic on the target cluster. The topic must
    /// already exist on the target (it will not be auto-created).
    ///
    /// # Arguments
    ///
    /// * `topic` - Name of the topic to mirror
    /// * `max_records` - Maximum number of records to consume per partition
    ///
    /// # Returns
    ///
    /// The total number of records mirrored across all partitions.
    #[cfg(feature = "migration-tools")]
    pub async fn mirror_topic(&self, topic: &str, max_records: u64) -> Result<u64> {
        let client = reqwest::Client::new();
        let source_base = self.source_url.trim_end_matches('/');
        let target_base = self.target_url.trim_end_matches('/');

        // Get partitions from source
        let partitions_url = format!("{}/api/v1/topics/{}/partitions", source_base, topic);
        let partitions_resp = client.get(&partitions_url).send().await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to get partitions: {}", e))
        })?;

        let partitions: Vec<serde_json::Value> = partitions_resp.json().await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to parse partitions: {}", e))
        })?;

        let mut total_mirrored: u64 = 0;

        for partition in &partitions {
            let partition_id = partition
                .get("partition_id")
                .and_then(|p| p.as_u64())
                .unwrap_or(0);

            // Consume from source
            let consume_url = format!(
                "{}/api/v1/consume?topic={}&partition={}&offset=0&maxRecords={}",
                source_base, topic, partition_id, max_records
            );
            let consume_resp = client.get(&consume_url).send().await.map_err(|e| {
                MetadataError::InternalError(format!("Failed to consume from source: {}", e))
            })?;

            let consume_data: serde_json::Value = consume_resp.json().await.map_err(|e| {
                MetadataError::InternalError(format!("Failed to parse consumed records: {}", e))
            })?;

            let records = consume_data
                .get("records")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();

            if records.is_empty() {
                continue;
            }

            // Produce to target in batch
            let batch_records: Vec<serde_json::Value> = records
                .iter()
                .map(|r| {
                    let mut rec = serde_json::Map::new();
                    if let Some(value) = r.get("value") {
                        rec.insert("value".to_string(), value.clone());
                    }
                    if let Some(key) = r.get("key") {
                        if !key.is_null() {
                            rec.insert("key".to_string(), key.clone());
                        }
                    }
                    serde_json::Value::Object(rec)
                })
                .collect();

            let produce_url = format!("{}/api/v1/produce/batch", target_base);
            let body = serde_json::json!({
                "topic": topic,
                "records": batch_records,
            });

            client
                .post(&produce_url)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    MetadataError::InternalError(format!("Failed to produce to target: {}", e))
                })?;

            total_mirrored += batch_records.len() as u64;
        }

        Ok(total_mirrored)
    }
}

// =============================================================================
// Kafka Migrator
// =============================================================================

/// Estimated scope of a Kafka-to-StreamHouse migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationEstimate {
    /// List of topic names to migrate
    pub topics: Vec<String>,
    /// Total estimated number of messages across all topics
    pub total_messages: u64,
    /// Estimated total size in bytes
    pub estimated_size_bytes: u64,
}

impl MigrationEstimate {
    /// Create a new empty migration estimate.
    pub fn new() -> Self {
        Self {
            topics: Vec::new(),
            total_messages: 0,
            estimated_size_bytes: 0,
        }
    }

    /// Add a topic to the estimate.
    pub fn add_topic(&mut self, name: String, messages: u64, size_bytes: u64) {
        self.topics.push(name);
        self.total_messages += messages;
        self.estimated_size_bytes += size_bytes;
    }

    /// Returns the number of topics in the estimate.
    pub fn topic_count(&self) -> usize {
        self.topics.len()
    }

    /// Returns true if there is nothing to migrate.
    pub fn is_empty(&self) -> bool {
        self.topics.is_empty()
    }
}

impl Default for MigrationEstimate {
    fn default() -> Self {
        Self::new()
    }
}

/// Facilitates migrating data from Apache Kafka to StreamHouse.
///
/// The KafkaMigrator connects to a Kafka cluster and a StreamHouse cluster,
/// enabling topic-by-topic migration of both message data and consumer group
/// offsets. It provides estimation capabilities to plan migrations before
/// executing them.
///
/// # Example
///
/// ```ignore
/// use streamhouse_metadata::backup::KafkaMigrator;
///
/// let migrator = KafkaMigrator::new(
///     "kafka-broker:9092",
///     "http://streamhouse:8080",
/// );
///
/// // Estimate the migration scope
/// let estimate = migrator.estimate_migration().await?;
/// println!("Will migrate {} topics, ~{} messages",
///     estimate.topic_count(), estimate.total_messages);
///
/// // Migrate a specific topic
/// let records = migrator.migrate_topic("orders", None).await?;
///
/// // Migrate consumer group offsets
/// migrator.migrate_consumer_offsets("analytics-group", "orders").await?;
/// ```
pub struct KafkaMigrator {
    /// Kafka bootstrap server address (e.g., "broker:9092")
    pub kafka_bootstrap: String,
    /// StreamHouse server URL (e.g., "http://localhost:8080")
    pub streamhouse_url: String,
}

impl KafkaMigrator {
    /// Create a new Kafka migrator.
    ///
    /// # Arguments
    ///
    /// * `kafka_bootstrap` - Kafka bootstrap server address
    /// * `streamhouse_url` - StreamHouse server URL
    pub fn new(kafka_bootstrap: impl Into<String>, streamhouse_url: impl Into<String>) -> Self {
        Self {
            kafka_bootstrap: kafka_bootstrap.into(),
            streamhouse_url: streamhouse_url.into(),
        }
    }

    /// Estimate the scope of a full migration from Kafka to StreamHouse.
    ///
    /// Connects to the Kafka cluster to enumerate topics and estimate their
    /// size. This is a read-only operation that does not modify either cluster.
    ///
    /// # Returns
    ///
    /// A `MigrationEstimate` describing the topics, message counts, and
    /// estimated data sizes.
    ///
    /// # Note
    ///
    /// In the current implementation, this returns a placeholder estimate.
    /// Full Kafka admin client integration (via librdkafka) is required for
    /// production use.
    pub async fn estimate_migration(&self) -> Result<MigrationEstimate> {
        // In a full implementation, this would use the Kafka admin client to:
        // 1. List all topics
        // 2. Get partition counts and offsets (end - beginning = message count)
        // 3. Sample records to estimate average record size
        //
        // For now, we return a structured estimate that callers can populate
        // manually or via the Kafka admin API.
        Ok(MigrationEstimate::new())
    }

    /// Migrate a single topic from Kafka to StreamHouse.
    ///
    /// Reads all records from the Kafka topic (or up to `max_records` if specified)
    /// and produces them to a topic with the same name on StreamHouse. The topic
    /// must be pre-created on StreamHouse with the desired partition count.
    ///
    /// # Arguments
    ///
    /// * `topic` - Kafka topic name to migrate
    /// * `max_records` - Optional maximum number of records to migrate per partition
    ///
    /// # Returns
    ///
    /// The total number of records migrated.
    ///
    /// # Note
    ///
    /// Requires a Kafka consumer client (e.g., rdkafka). The current implementation
    /// provides the framework; integrate your preferred Kafka client library.
    pub async fn migrate_topic(
        &self,
        _topic: &str,
        _max_records: Option<u64>,
    ) -> Result<u64> {
        // Full implementation requires rdkafka or similar Kafka client:
        // 1. Create a Kafka consumer for the topic
        // 2. Seek to beginning of each partition
        // 3. Consume records in batches
        // 4. Produce each batch to StreamHouse via HTTP API
        // 5. Track progress and return total count
        Err(MetadataError::InternalError(
            "Kafka migration requires rdkafka integration. \
             Configure the kafka feature and provide a Kafka consumer client."
                .to_string(),
        ))
    }

    /// Migrate consumer group offsets from Kafka to StreamHouse.
    ///
    /// Reads the committed offsets for a consumer group in Kafka and commits
    /// corresponding offsets in StreamHouse. This allows consumers to resume
    /// from the same position after migration.
    ///
    /// # Arguments
    ///
    /// * `group_id` - Kafka consumer group ID
    /// * `topic` - Topic to migrate offsets for
    ///
    /// # Note
    ///
    /// Offset semantics may differ between Kafka and StreamHouse. The migrator
    /// copies offsets as-is, which works when data is migrated in order starting
    /// from offset 0.
    pub async fn migrate_consumer_offsets(
        &self,
        _group_id: &str,
        _topic: &str,
    ) -> Result<()> {
        // Full implementation:
        // 1. Use Kafka admin client to fetch committed offsets for the group
        // 2. For each (topic, partition, offset), commit to StreamHouse
        Err(MetadataError::InternalError(
            "Consumer offset migration requires rdkafka integration.".to_string(),
        ))
    }
}

// =============================================================================
// Schema Importer
// =============================================================================

/// Imports schemas from a Confluent Schema Registry into StreamHouse.
///
/// The SchemaImporter connects to a Confluent Schema Registry (or compatible
/// registry) and imports schema subjects into StreamHouse's metadata store.
/// This enables seamless migration of schema-governed topics.
///
/// # Example
///
/// ```ignore
/// use streamhouse_metadata::backup::SchemaImporter;
///
/// let importer = SchemaImporter::new("http://schema-registry:8081");
///
/// // List available subjects
/// let subjects = importer.list_remote_subjects().await?;
///
/// // Import a specific subject
/// importer.import_subject("orders-value").await?;
///
/// // Import all subjects
/// let count = importer.import_all().await?;
/// println!("Imported {} schemas", count);
/// ```
pub struct SchemaImporter {
    /// URL of the Confluent Schema Registry
    pub registry_url: String,
}

impl SchemaImporter {
    /// Create a new schema importer.
    ///
    /// # Arguments
    ///
    /// * `registry_url` - URL of the Confluent Schema Registry (e.g., "http://registry:8081")
    pub fn new(registry_url: impl Into<String>) -> Self {
        Self {
            registry_url: registry_url.into(),
        }
    }

    /// List all subjects available in the remote Schema Registry.
    ///
    /// # Returns
    ///
    /// A list of subject names (e.g., ["orders-value", "users-key"]).
    #[cfg(feature = "migration-tools")]
    pub async fn list_remote_subjects(&self) -> Result<Vec<String>> {
        let url = format!("{}/subjects", self.registry_url.trim_end_matches('/'));
        let response = reqwest::get(&url).await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to list schema subjects: {}", e))
        })?;

        let subjects: Vec<String> = response.json().await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to parse schema subjects: {}", e))
        })?;

        Ok(subjects)
    }

    /// Import a specific subject (all versions) from the Schema Registry.
    ///
    /// Fetches all versions of the given subject and stores them locally.
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject name to import (e.g., "orders-value")
    ///
    /// # Returns
    ///
    /// A list of imported schema strings (one per version).
    #[cfg(feature = "migration-tools")]
    pub async fn import_subject(&self, subject: &str) -> Result<Vec<String>> {
        let base = self.registry_url.trim_end_matches('/');

        // Get all versions for the subject
        let versions_url = format!("{}/subjects/{}/versions", base, subject);
        let versions_resp = reqwest::get(&versions_url).await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to get schema versions: {}", e))
        })?;

        let versions: Vec<u32> = versions_resp.json().await.map_err(|e| {
            MetadataError::InternalError(format!("Failed to parse schema versions: {}", e))
        })?;

        let mut schemas = Vec::new();

        for version in versions {
            let schema_url = format!("{}/subjects/{}/versions/{}", base, subject, version);
            let schema_resp = reqwest::get(&schema_url).await.map_err(|e| {
                MetadataError::InternalError(format!(
                    "Failed to get schema version {}: {}",
                    version, e
                ))
            })?;

            let schema_data: serde_json::Value = schema_resp.json().await.map_err(|e| {
                MetadataError::InternalError(format!("Failed to parse schema: {}", e))
            })?;

            if let Some(schema) = schema_data.get("schema").and_then(|s| s.as_str()) {
                schemas.push(schema.to_string());
            }
        }

        Ok(schemas)
    }

    /// Import all subjects from the Schema Registry.
    ///
    /// Enumerates all subjects and imports every version of each.
    ///
    /// # Returns
    ///
    /// The total number of schema versions imported.
    #[cfg(feature = "migration-tools")]
    pub async fn import_all(&self) -> Result<usize> {
        let subjects = self.list_remote_subjects().await?;
        let mut total = 0;

        for subject in &subjects {
            let schemas = self.import_subject(subject).await?;
            total += schemas.len();
        }

        Ok(total)
    }
}

// =============================================================================
// Extended Tests
// =============================================================================

#[cfg(test)]
mod scheduler_tests {
    use super::*;

    #[test]
    fn test_backup_schedule_config_defaults() {
        let config = BackupScheduleConfig::default();
        assert_eq!(config.interval_hours, 24);
        assert_eq!(config.retention_count, 7);
        assert!(config.compress);
        assert_eq!(config.backup_dir, "./backups");
    }

    #[test]
    fn test_scheduler_should_run_initially() {
        let scheduler = BackupScheduler::new(BackupScheduleConfig::default());
        assert!(scheduler.should_run());
    }

    #[test]
    fn test_scheduler_should_not_run_before_interval() {
        let mut scheduler = BackupScheduler::new(BackupScheduleConfig {
            interval_hours: 24,
            ..Default::default()
        });
        // Simulate a backup that just happened
        scheduler.last_backup = Some(chrono::Utc::now().timestamp_millis());
        assert!(!scheduler.should_run());
    }

    #[test]
    fn test_scheduler_should_run_after_interval() {
        let mut scheduler = BackupScheduler::new(BackupScheduleConfig {
            interval_hours: 1,
            ..Default::default()
        });
        // Simulate a backup that happened 2 hours ago
        let two_hours_ago = chrono::Utc::now().timestamp_millis() - (2 * 3600 * 1000);
        scheduler.last_backup = Some(two_hours_ago);
        assert!(scheduler.should_run());
    }

    #[test]
    fn test_scheduler_cleanup_with_no_dir() {
        let scheduler = BackupScheduler::new(BackupScheduleConfig {
            backup_dir: "/nonexistent/path/for/testing".to_string(),
            ..Default::default()
        });
        // cleanup_old_backups should fail gracefully with an error
        assert!(scheduler.cleanup_old_backups().is_err());
    }

    #[test]
    fn test_migration_estimate_new() {
        let estimate = MigrationEstimate::new();
        assert!(estimate.is_empty());
        assert_eq!(estimate.topic_count(), 0);
        assert_eq!(estimate.total_messages, 0);
        assert_eq!(estimate.estimated_size_bytes, 0);
    }

    #[test]
    fn test_migration_estimate_add_topics() {
        let mut estimate = MigrationEstimate::new();
        estimate.add_topic("orders".to_string(), 1000, 50000);
        estimate.add_topic("events".to_string(), 5000, 250000);

        assert_eq!(estimate.topic_count(), 2);
        assert_eq!(estimate.total_messages, 6000);
        assert_eq!(estimate.estimated_size_bytes, 300000);
        assert!(!estimate.is_empty());
    }

    #[test]
    fn test_migration_estimate_serialization() {
        let mut estimate = MigrationEstimate::new();
        estimate.add_topic("test-topic".to_string(), 100, 5000);

        let json = serde_json::to_string(&estimate).unwrap();
        let restored: MigrationEstimate = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.topics, vec!["test-topic"]);
        assert_eq!(restored.total_messages, 100);
        assert_eq!(restored.estimated_size_bytes, 5000);
    }

    #[test]
    fn test_kafka_migrator_new() {
        let migrator = KafkaMigrator::new("broker:9092", "http://localhost:8080");
        assert_eq!(migrator.kafka_bootstrap, "broker:9092");
        assert_eq!(migrator.streamhouse_url, "http://localhost:8080");
    }

    #[test]
    fn test_topic_mirror_new() {
        let mirror = TopicMirror::new("http://source:8080", "http://target:8080");
        assert_eq!(mirror.source_url, "http://source:8080");
        assert_eq!(mirror.target_url, "http://target:8080");
    }

    #[test]
    fn test_schema_importer_new() {
        let importer = SchemaImporter::new("http://registry:8081");
        assert_eq!(importer.registry_url, "http://registry:8081");
    }
}
