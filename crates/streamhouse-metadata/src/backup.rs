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
