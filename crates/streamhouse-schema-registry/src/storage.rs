//! Schema Storage Layer
//!
//! Provides persistent storage for schema metadata using the metadata store.

use crate::{
    error::{Result, SchemaError},
    types::{CompatibilityMode, Schema, SchemaFormat, SubjectConfig},
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
/// Uses the metadata store's generic KV storage for persistence.
pub struct MemorySchemaStorage {
    metadata: Arc<dyn MetadataStore>,
    next_id: std::sync::atomic::AtomicI32,
}

impl MemorySchemaStorage {
    pub fn new(metadata: Arc<dyn MetadataStore>) -> Self {
        Self {
            metadata,
            next_id: std::sync::atomic::AtomicI32::new(1),
        }
    }

    fn generate_id(&self) -> i32 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    fn schema_key(id: i32) -> String {
        format!("schema:{}", id)
    }

    fn subject_version_key(subject: &str, version: i32) -> String {
        format!("subject:{}:version:{}", subject, version)
    }

    fn subject_latest_key(subject: &str) -> String {
        format!("subject:{}:latest", subject)
    }

    fn subject_versions_key(subject: &str) -> String {
        format!("subject:{}:versions", subject)
    }

    fn subject_config_key(subject: &str) -> String {
        format!("subject:{}:config", subject)
    }

    fn global_config_key() -> String {
        "global:config".to_string()
    }

    fn subjects_key() -> String {
        "subjects".to_string()
    }
}

#[async_trait]
impl SchemaStorage for MemorySchemaStorage {
    async fn register_schema(&self, mut schema: Schema) -> Result<i32> {
        // Check if this exact schema already exists
        if let Some(existing_id) = self.schema_exists(&schema.subject, &schema.schema).await? {
            return Ok(existing_id);
        }

        // Generate new ID
        let id = self.generate_id();
        schema.id = id;

        // Determine version (latest + 1)
        let version = if let Some(latest) = self.get_latest_schema(&schema.subject).await? {
            latest.version + 1
        } else {
            1
        };
        schema.version = version;

        // Serialize schema
        let schema_json = serde_json::to_string(&schema)
            .map_err(|e| SchemaError::SerializationError(e.to_string()))?;

        // Store schema by ID
        // Note: This is a placeholder - metadata store doesn't have generic KV yet
        // In production, this would use PostgreSQL or a dedicated KV store
        tracing::info!(
            id = id,
            subject = %schema.subject,
            version = version,
            "Schema registered (storage not yet implemented)"
        );

        Ok(id)
    }

    async fn get_schema_by_id(&self, id: i32) -> Result<Option<Schema>> {
        // Placeholder: would fetch from metadata store
        tracing::debug!(id = id, "get_schema_by_id not yet implemented");
        Ok(None)
    }

    async fn get_schema_by_subject_version(
        &self,
        subject: &str,
        version: i32,
    ) -> Result<Option<Schema>> {
        // Placeholder: would fetch from metadata store
        tracing::debug!(
            subject = subject,
            version = version,
            "get_schema_by_subject_version not yet implemented"
        );
        Ok(None)
    }

    async fn get_latest_schema(&self, subject: &str) -> Result<Option<Schema>> {
        // Placeholder: would fetch from metadata store
        tracing::debug!(subject = subject, "get_latest_schema not yet implemented");
        Ok(None)
    }

    async fn get_versions(&self, subject: &str) -> Result<Vec<i32>> {
        // Placeholder: would fetch from metadata store
        tracing::debug!(subject = subject, "get_versions not yet implemented");
        Ok(vec![])
    }

    async fn get_subjects(&self) -> Result<Vec<String>> {
        // Placeholder: would fetch from metadata store
        tracing::debug!("get_subjects not yet implemented");
        Ok(vec![])
    }

    async fn delete_subject(&self, subject: &str) -> Result<Vec<i32>> {
        // Placeholder: would delete from metadata store
        tracing::debug!(subject = subject, "delete_subject not yet implemented");
        Ok(vec![])
    }

    async fn delete_version(&self, subject: &str, version: i32) -> Result<i32> {
        // Placeholder: would delete from metadata store
        tracing::debug!(
            subject = subject,
            version = version,
            "delete_version not yet implemented"
        );
        Ok(version)
    }

    async fn schema_exists(&self, subject: &str, schema: &str) -> Result<Option<i32>> {
        // Placeholder: would check in metadata store
        tracing::debug!(subject = subject, "schema_exists not yet implemented");
        Ok(None)
    }

    async fn get_subject_config(&self, subject: &str) -> Result<Option<SubjectConfig>> {
        // Placeholder: would fetch from metadata store
        tracing::debug!(subject = subject, "get_subject_config not yet implemented");
        Ok(None)
    }

    async fn set_subject_config(&self, config: SubjectConfig) -> Result<()> {
        // Placeholder: would store in metadata store
        tracing::debug!(subject = %config.subject, "set_subject_config not yet implemented");
        Ok(())
    }

    async fn get_global_compatibility(&self) -> Result<CompatibilityMode> {
        // Placeholder: would fetch from metadata store
        tracing::debug!("get_global_compatibility not yet implemented");
        Ok(CompatibilityMode::default())
    }

    async fn set_global_compatibility(&self, mode: CompatibilityMode) -> Result<()> {
        // Placeholder: would store in metadata store
        tracing::debug!(mode = ?mode, "set_global_compatibility not yet implemented");
        Ok(())
    }
}
