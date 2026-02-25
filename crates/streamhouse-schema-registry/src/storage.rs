//! Schema Storage Layer
//!
//! Provides persistent storage for schema metadata using the metadata store.

use crate::{
    error::Result,
    types::{CompatibilityMode, Schema, SubjectConfig},
};
use async_trait::async_trait;
use std::sync::Arc;
use streamhouse_metadata::MetadataStore;

/// Schema storage trait
#[async_trait]
pub trait SchemaStorage: Send + Sync {
    /// Register a new schema and return its ID
    async fn register_schema(&self, schema: Schema) -> Result<i32>;

    /// Get schema by ID
    async fn get_schema_by_id(&self, id: i32) -> Result<Option<Schema>>;

    /// Get schema by subject and version
    async fn get_schema_by_subject_version(
        &self,
        subject: &str,
        version: i32,
    ) -> Result<Option<Schema>>;

    /// Get latest schema for subject
    async fn get_latest_schema(&self, subject: &str) -> Result<Option<Schema>>;

    /// Get all versions for a subject
    async fn get_versions(&self, subject: &str) -> Result<Vec<i32>>;

    /// Get all subjects
    async fn get_subjects(&self) -> Result<Vec<String>>;

    /// Delete a subject (soft delete - marks as deleted)
    async fn delete_subject(&self, subject: &str) -> Result<Vec<i32>>;

    /// Delete a specific version
    async fn delete_version(&self, subject: &str, version: i32) -> Result<i32>;

    /// Check if schema exists
    async fn schema_exists(&self, subject: &str, schema: &str) -> Result<Option<i32>>;

    /// Get subject configuration
    async fn get_subject_config(&self, subject: &str) -> Result<Option<SubjectConfig>>;

    /// Set subject configuration
    async fn set_subject_config(&self, config: SubjectConfig) -> Result<()>;

    /// Get global compatibility mode
    async fn get_global_compatibility(&self) -> Result<CompatibilityMode>;

    /// Set global compatibility mode
    async fn set_global_compatibility(&self, mode: CompatibilityMode) -> Result<()>;
}

/// In-memory schema storage implementation
///
/// Stores all schemas in memory using HashMaps protected by RwLock.
/// Suitable for development, testing, and single-server deployments.
pub struct MemorySchemaStorage {
    next_id: std::sync::atomic::AtomicI32,
    /// Schemas indexed by ID
    schemas_by_id: tokio::sync::RwLock<std::collections::HashMap<i32, Schema>>,
    /// Subject -> version -> schema ID mapping
    subject_versions: tokio::sync::RwLock<std::collections::HashMap<String, Vec<(i32, i32)>>>,
    /// Subject-level compatibility configs
    subject_configs: tokio::sync::RwLock<std::collections::HashMap<String, SubjectConfig>>,
    /// Global compatibility mode
    global_compat: tokio::sync::RwLock<CompatibilityMode>,
}

impl MemorySchemaStorage {
    pub fn new(_metadata: Arc<dyn MetadataStore>) -> Self {
        Self {
            next_id: std::sync::atomic::AtomicI32::new(1),
            schemas_by_id: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            subject_versions: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            subject_configs: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            global_compat: tokio::sync::RwLock::new(CompatibilityMode::default()),
        }
    }

    fn generate_id(&self) -> i32 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl SchemaStorage for MemorySchemaStorage {
    async fn register_schema(&self, mut schema: Schema) -> Result<i32> {
        // Check if this exact schema already exists for this subject
        if let Some(existing_id) = self.schema_exists(&schema.subject, &schema.schema).await? {
            return Ok(existing_id);
        }

        // Generate new ID
        let id = self.generate_id();
        schema.id = id;

        // Determine version (latest + 1)
        let version = {
            let versions = self.subject_versions.read().await;
            if let Some(v) = versions.get(&schema.subject) {
                v.iter().map(|(ver, _)| *ver).max().unwrap_or(0) + 1
            } else {
                1
            }
        };
        schema.version = version;

        // Store schema
        {
            let mut by_id = self.schemas_by_id.write().await;
            by_id.insert(id, schema.clone());
        }
        {
            let mut versions = self.subject_versions.write().await;
            versions
                .entry(schema.subject.clone())
                .or_default()
                .push((version, id));
        }

        tracing::info!(
            id = id,
            subject = %schema.subject,
            version = version,
            "Schema registered"
        );

        Ok(id)
    }

    async fn get_schema_by_id(&self, id: i32) -> Result<Option<Schema>> {
        let by_id = self.schemas_by_id.read().await;
        Ok(by_id.get(&id).cloned())
    }

    async fn get_schema_by_subject_version(
        &self,
        subject: &str,
        version: i32,
    ) -> Result<Option<Schema>> {
        let versions = self.subject_versions.read().await;
        if let Some(v) = versions.get(subject) {
            if let Some((_, schema_id)) = v.iter().find(|(ver, _)| *ver == version) {
                let by_id = self.schemas_by_id.read().await;
                return Ok(by_id.get(schema_id).cloned());
            }
        }
        Ok(None)
    }

    async fn get_latest_schema(&self, subject: &str) -> Result<Option<Schema>> {
        let versions = self.subject_versions.read().await;
        if let Some(v) = versions.get(subject) {
            if let Some((_, schema_id)) = v.iter().max_by_key(|(ver, _)| *ver) {
                let by_id = self.schemas_by_id.read().await;
                return Ok(by_id.get(schema_id).cloned());
            }
        }
        Ok(None)
    }

    async fn get_versions(&self, subject: &str) -> Result<Vec<i32>> {
        let versions = self.subject_versions.read().await;
        if let Some(v) = versions.get(subject) {
            let mut vers: Vec<i32> = v.iter().map(|(ver, _)| *ver).collect();
            vers.sort();
            Ok(vers)
        } else {
            Ok(vec![])
        }
    }

    async fn get_subjects(&self) -> Result<Vec<String>> {
        let versions = self.subject_versions.read().await;
        let mut subjects: Vec<String> = versions.keys().cloned().collect();
        subjects.sort();
        Ok(subjects)
    }

    async fn delete_subject(&self, subject: &str) -> Result<Vec<i32>> {
        let removed_versions = {
            let mut versions = self.subject_versions.write().await;
            versions.remove(subject).unwrap_or_default()
        };
        let version_nums: Vec<i32> = removed_versions.iter().map(|(v, _)| *v).collect();
        // Remove schemas
        {
            let mut by_id = self.schemas_by_id.write().await;
            for (_, schema_id) in &removed_versions {
                by_id.remove(schema_id);
            }
        }
        Ok(version_nums)
    }

    async fn delete_version(&self, subject: &str, version: i32) -> Result<i32> {
        let schema_id = {
            let mut versions = self.subject_versions.write().await;
            if let Some(v) = versions.get_mut(subject) {
                if let Some(pos) = v.iter().position(|(ver, _)| *ver == version) {
                    let (_, sid) = v.remove(pos);
                    Some(sid)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(sid) = schema_id {
            let mut by_id = self.schemas_by_id.write().await;
            by_id.remove(&sid);
        }
        Ok(version)
    }

    async fn schema_exists(&self, subject: &str, schema_str: &str) -> Result<Option<i32>> {
        let versions = self.subject_versions.read().await;
        if let Some(v) = versions.get(subject) {
            let by_id = self.schemas_by_id.read().await;
            for (_, schema_id) in v {
                if let Some(s) = by_id.get(schema_id) {
                    if s.schema == schema_str {
                        return Ok(Some(*schema_id));
                    }
                }
            }
        }
        Ok(None)
    }

    async fn get_subject_config(&self, subject: &str) -> Result<Option<SubjectConfig>> {
        let configs = self.subject_configs.read().await;
        Ok(configs.get(subject).cloned())
    }

    async fn set_subject_config(&self, config: SubjectConfig) -> Result<()> {
        let mut configs = self.subject_configs.write().await;
        configs.insert(config.subject.clone(), config);
        Ok(())
    }

    async fn get_global_compatibility(&self) -> Result<CompatibilityMode> {
        let compat = self.global_compat.read().await;
        Ok(*compat)
    }

    async fn set_global_compatibility(&self, mode: CompatibilityMode) -> Result<()> {
        let mut compat = self.global_compat.write().await;
        *compat = mode;
        Ok(())
    }
}
