//! Schema Registry Service
//!
//! Central service for managing schemas, versions, and compatibility.

use crate::{
    compatibility::check_compatibility,
    error::{Result, SchemaError},
    storage::SchemaStorage,
    types::*,
};
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

/// Schema Registry Service
pub struct SchemaRegistry {
    storage: Arc<dyn SchemaStorage>,
    schema_cache: Cache<i32, Schema>,
    subject_cache: Cache<String, Schema>,
}

impl SchemaRegistry {
    /// Create a new schema registry
    pub fn new(storage: Arc<dyn SchemaStorage>) -> Self {
        // Cache up to 10,000 schemas for 1 hour
        let schema_cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(3600))
            .build();

        // Cache up to 5,000 subject lookups for 5 minutes
        let subject_cache = Cache::builder()
            .max_capacity(5_000)
            .time_to_live(Duration::from_secs(300))
            .build();

        Self {
            storage,
            schema_cache,
            subject_cache,
        }
    }

    /// Register a new schema
    pub async fn register_schema(
        &self,
        subject: &str,
        request: RegisterSchemaRequest,
        org_id: &str,
    ) -> Result<i32> {
        let schema_type = request.schema_type.unwrap_or(SchemaFormat::Avro);

        // Validate schema format
        self.validate_schema(&request.schema, schema_type)?;

        // Check compatibility if subject already has schemas
        if let Some(latest) = self.storage.get_latest_schema(subject, org_id).await? {
            let compatibility = self.get_compatibility_mode(subject, org_id).await?;

            if !check_compatibility(&latest, &request.schema, schema_type, compatibility)? {
                return Err(SchemaError::IncompatibleSchema(format!(
                    "New schema is not compatible with version {} under {:?} mode",
                    latest.version, compatibility
                )));
            }
        }

        // Check if this exact schema already exists
        if let Some(existing_id) = self
            .storage
            .schema_exists(subject, &request.schema, org_id)
            .await?
        {
            return Ok(existing_id);
        }

        // Register new schema
        let schema = Schema {
            id: 0, // Will be assigned by storage
            subject: subject.to_string(),
            version: 0, // Will be assigned by storage
            schema_type,
            schema: request.schema,
            references: request.references,
            metadata: request.metadata.unwrap_or_default(),
        };

        let id = self.storage.register_schema(schema, org_id).await?;

        // Invalidate caches
        self.subject_cache.invalidate(subject).await;

        Ok(id)
    }

    /// Get schema by ID
    pub async fn get_schema_by_id(&self, id: i32) -> Result<Schema> {
        // Check cache first
        if let Some(schema) = self.schema_cache.get(&id).await {
            return Ok(schema);
        }

        // Fetch from storage
        let schema = self.storage.get_schema_by_id(id).await?.ok_or_else(|| {
            SchemaError::SchemaNotFound {
                subject: format!("id:{}", id),
                version: 0,
            }
        })?;

        // Cache for future lookups
        self.schema_cache.insert(id, schema.clone()).await;

        Ok(schema)
    }

    /// Get schema by subject and version
    pub async fn get_schema(&self, subject: &str, version: i32, org_id: &str) -> Result<Schema> {
        let schema = self
            .storage
            .get_schema_by_subject_version(subject, version, org_id)
            .await?
            .ok_or_else(|| SchemaError::SchemaNotFound {
                subject: subject.to_string(),
                version,
            })?;

        // Cache by ID
        self.schema_cache.insert(schema.id, schema.clone()).await;

        Ok(schema)
    }

    /// Get latest schema for subject
    pub async fn get_latest_schema(&self, subject: &str, org_id: &str) -> Result<Schema> {
        // Check cache
        if let Some(schema) = self.subject_cache.get(subject).await {
            return Ok(schema);
        }

        // Fetch from storage
        let schema = self
            .storage
            .get_latest_schema(subject, org_id)
            .await?
            .ok_or_else(|| SchemaError::SubjectNotFound(subject.to_string()))?;

        // Cache
        self.subject_cache
            .insert(subject.to_string(), schema.clone())
            .await;
        self.schema_cache.insert(schema.id, schema.clone()).await;

        Ok(schema)
    }

    /// Get all versions for a subject
    pub async fn get_versions(&self, subject: &str, org_id: &str) -> Result<Vec<i32>> {
        self.storage.get_versions(subject, org_id).await
    }

    /// Get all subjects
    pub async fn get_subjects(&self, org_id: &str) -> Result<Vec<String>> {
        self.storage.get_subjects(org_id).await
    }

    /// Delete a subject
    pub async fn delete_subject(&self, subject: &str, org_id: &str) -> Result<Vec<i32>> {
        let versions = self.storage.delete_subject(subject, org_id).await?;

        // Invalidate caches
        self.subject_cache.invalidate(subject).await;

        Ok(versions)
    }

    /// Delete a specific version
    pub async fn delete_version(&self, subject: &str, version: i32, org_id: &str) -> Result<i32> {
        let deleted_version = self
            .storage
            .delete_version(subject, version, org_id)
            .await?;

        // Invalidate caches
        self.subject_cache.invalidate(subject).await;

        Ok(deleted_version)
    }

    /// Get compatibility mode for subject
    async fn get_compatibility_mode(
        &self,
        subject: &str,
        org_id: &str,
    ) -> Result<CompatibilityMode> {
        // Try subject-specific config first
        if let Some(config) = self.storage.get_subject_config(subject, org_id).await? {
            return Ok(config.compatibility);
        }

        // Fall back to global config
        self.storage.get_global_compatibility().await
    }

    /// Get subject configuration
    pub async fn get_subject_config(
        &self,
        subject: &str,
        org_id: &str,
    ) -> Result<Option<SubjectConfig>> {
        self.storage.get_subject_config(subject, org_id).await
    }

    /// Set subject compatibility mode
    pub async fn set_subject_compatibility(
        &self,
        subject: &str,
        mode: CompatibilityMode,
        org_id: &str,
    ) -> Result<()> {
        let config = SubjectConfig {
            subject: subject.to_string(),
            compatibility: mode,
        };

        self.storage.set_subject_config(config, org_id).await
    }

    /// Get global compatibility mode
    pub async fn get_global_compatibility(&self) -> Result<CompatibilityMode> {
        self.storage.get_global_compatibility().await
    }

    /// Set global compatibility mode
    pub async fn set_global_compatibility(&self, mode: CompatibilityMode) -> Result<()> {
        self.storage.set_global_compatibility(mode).await
    }

    /// Validate schema syntax
    fn validate_schema(&self, schema: &str, format: SchemaFormat) -> Result<()> {
        match format {
            SchemaFormat::Avro => {
                apache_avro::Schema::parse_str(schema).map_err(|e| {
                    SchemaError::InvalidSchema(format!("Invalid Avro schema: {}", e))
                })?;
            }
            SchemaFormat::Protobuf => {
                if schema.is_empty() {
                    return Err(SchemaError::InvalidSchema(
                        "Empty Protobuf schema".to_string(),
                    ));
                }
                // Validate that the schema is syntactically valid protobuf IDL.
                // We check for required structural elements: syntax declaration
                // and at least one message or enum definition.
                let trimmed = schema.trim();
                if !trimmed.contains("syntax") {
                    return Err(SchemaError::InvalidSchema(
                        "Protobuf schema missing 'syntax' declaration".to_string(),
                    ));
                }
                if !trimmed.contains("message") && !trimmed.contains("enum") {
                    return Err(SchemaError::InvalidSchema(
                        "Protobuf schema must contain at least one message or enum definition"
                            .to_string(),
                    ));
                }
                // Validate balanced braces as a basic structural check
                let open_braces = trimmed.chars().filter(|c| *c == '{').count();
                let close_braces = trimmed.chars().filter(|c| *c == '}').count();
                if open_braces != close_braces {
                    return Err(SchemaError::InvalidSchema(
                        "Protobuf schema has unbalanced braces".to_string(),
                    ));
                }
                if open_braces == 0 {
                    return Err(SchemaError::InvalidSchema(
                        "Protobuf schema has no message or enum bodies".to_string(),
                    ));
                }
            }
            SchemaFormat::Json => {
                let _: serde_json::Value = serde_json::from_str(schema).map_err(|e| {
                    SchemaError::InvalidSchema(format!("Invalid JSON schema: {}", e))
                })?;

                // Validate it's actually a JSON Schema
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(schema) {
                    if json.get("$schema").is_none() && json.get("type").is_none() {
                        return Err(SchemaError::InvalidSchema(
                            "Not a valid JSON Schema (missing $schema or type)".to_string(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemorySchemaStorage;
    use streamhouse_metadata::SqliteMetadataStore;

    #[tokio::test]
    async fn test_register_and_get_schema() {
        let metadata = Arc::new(SqliteMetadataStore::new(":memory:").await.unwrap());
        let storage = Arc::new(MemorySchemaStorage::new(metadata));
        let registry = SchemaRegistry::new(storage);

        let schema_str =
            r#"{"type": "record", "name": "User", "fields": [{"name": "name", "type": "string"}]}"#;

        let request = RegisterSchemaRequest {
            schema: schema_str.to_string(),
            schema_type: Some(SchemaFormat::Avro),
            references: vec![],
            metadata: None,
        };

        let id = registry
            .register_schema("test-subject", request, "test-org")
            .await
            .unwrap();
        assert!(id > 0);

        // Note: With current placeholder storage, duplicate detection doesn't work yet
        // Once we implement proper storage with SQL, this should return the same ID
        let request2 = RegisterSchemaRequest {
            schema: schema_str.to_string(),
            schema_type: Some(SchemaFormat::Avro),
            references: vec![],
            metadata: None,
        };

        let id2 = registry
            .register_schema("test-subject", request2, "test-org")
            .await
            .unwrap();
        assert_eq!(id, id2);
    }

    #[tokio::test]
    async fn test_invalid_schema_rejected() {
        let metadata = Arc::new(SqliteMetadataStore::new(":memory:").await.unwrap());
        let storage = Arc::new(MemorySchemaStorage::new(metadata));
        let registry = SchemaRegistry::new(storage);

        let invalid_schema = "not valid json";

        let request = RegisterSchemaRequest {
            schema: invalid_schema.to_string(),
            schema_type: Some(SchemaFormat::Avro),
            references: vec![],
            metadata: None,
        };

        let result = registry
            .register_schema("test-subject", request, "test-org")
            .await;
        assert!(result.is_err());
    }
}
