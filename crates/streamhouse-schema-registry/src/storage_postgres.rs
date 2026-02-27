//! PostgreSQL-backed Schema Storage
//!
//! Provides persistent storage for schemas using PostgreSQL.
//! Uses SHA-256 hashing for schema deduplication.

use crate::{
    error::{Result, SchemaError},
    types::{CompatibilityMode, Schema, SubjectConfig},
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use super::SchemaStorage;

/// PostgreSQL schema storage implementation
pub struct PostgresSchemaStorage {
    pool: PgPool,
}

impl PostgresSchemaStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Compute SHA-256 hash of schema definition for deduplication
    fn compute_hash(schema_definition: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(schema_definition.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Convert SchemaFormat to string for database storage
    fn format_to_string(format: crate::types::SchemaFormat) -> &'static str {
        match format {
            crate::types::SchemaFormat::Avro => "avro",
            crate::types::SchemaFormat::Protobuf => "protobuf",
            crate::types::SchemaFormat::Json => "json",
        }
    }

    /// Convert string from database to SchemaFormat
    fn string_to_format(s: &str) -> Result<crate::types::SchemaFormat> {
        match s.to_lowercase().as_str() {
            "avro" => Ok(crate::types::SchemaFormat::Avro),
            "protobuf" => Ok(crate::types::SchemaFormat::Protobuf),
            "json" => Ok(crate::types::SchemaFormat::Json),
            _ => Err(SchemaError::InvalidFormat(s.to_string())),
        }
    }

    /// Convert CompatibilityMode to string for database storage
    fn compatibility_to_string(mode: CompatibilityMode) -> &'static str {
        match mode {
            CompatibilityMode::Backward => "backward",
            CompatibilityMode::Forward => "forward",
            CompatibilityMode::Full => "full",
            CompatibilityMode::BackwardTransitive => "backward_transitive",
            CompatibilityMode::ForwardTransitive => "forward_transitive",
            CompatibilityMode::FullTransitive => "full_transitive",
            CompatibilityMode::None => "none",
        }
    }

    /// Convert string from database to CompatibilityMode
    fn string_to_compatibility(s: &str) -> Result<CompatibilityMode> {
        match s.to_lowercase().as_str() {
            "backward" => Ok(CompatibilityMode::Backward),
            "forward" => Ok(CompatibilityMode::Forward),
            "full" => Ok(CompatibilityMode::Full),
            "backward_transitive" => Ok(CompatibilityMode::BackwardTransitive),
            "forward_transitive" => Ok(CompatibilityMode::ForwardTransitive),
            "full_transitive" => Ok(CompatibilityMode::FullTransitive),
            "none" => Ok(CompatibilityMode::None),
            _ => Err(SchemaError::InvalidCompatibilityMode(s.to_string())),
        }
    }
}

#[async_trait]
impl SchemaStorage for PostgresSchemaStorage {
    async fn register_schema(&self, mut schema: Schema) -> Result<i32> {
        // Compute hash for deduplication
        let schema_hash = Self::compute_hash(&schema.schema);

        // Check if this exact schema already exists (by hash)
        if let Some(existing_id) = self.schema_exists(&schema.subject, &schema.schema).await? {
            tracing::debug!(
                schema_id = existing_id,
                subject = %schema.subject,
                "Schema already exists, returning existing ID"
            );
            return Ok(existing_id);
        }

        // Determine next version for this subject
        let version = if let Some(latest) = self.get_latest_schema(&schema.subject).await? {
            latest.version + 1
        } else {
            1
        };

        // Begin transaction
        let mut tx = self.pool.begin().await?;

        // Insert schema into schemas table (or get existing ID if hash matches)
        // Use default organization for now (multi-tenancy will pass org_id through context)
        let default_org = "00000000-0000-0000-0000-000000000000";
        let schema_id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO schema_registry_schemas (schema_format, schema_definition, schema_hash, organization_id)
            VALUES ($1, $2, $3, $4::UUID)
            ON CONFLICT (schema_hash) DO UPDATE SET schema_hash = schema_registry_schemas.schema_hash
            RETURNING id
            "#,
        )
        .bind(Self::format_to_string(schema.schema_type))
        .bind(&schema.schema)
        .bind(&schema_hash)
        .bind(default_org)
        .fetch_one(&mut *tx)
        .await?;

        // Insert version mapping
        sqlx::query(
            r#"
            INSERT INTO schema_registry_versions (subject, version, schema_id, organization_id)
            VALUES ($1, $2, $3, $4::UUID)
            "#,
        )
        .bind(&schema.subject)
        .bind(version)
        .bind(schema_id)
        .bind(default_org)
        .execute(&mut *tx)
        .await?;

        // Commit transaction
        tx.commit().await?;

        // Update schema object
        schema.id = schema_id;
        schema.version = version;

        tracing::info!(
            schema_id = schema_id,
            subject = %schema.subject,
            version = version,
            "Schema registered successfully"
        );

        Ok(schema_id)
    }

    async fn get_schema_by_id(&self, id: i32) -> Result<Option<Schema>> {
        let result = sqlx::query_as::<_, (i32, String, String, String, i32, String)>(
            r#"
            SELECT
                s.id,
                s.schema_format,
                s.schema_definition,
                v.subject,
                v.version,
                s.schema_hash
            FROM schema_registry_schemas s
            JOIN schema_registry_versions v ON s.id = v.schema_id
            WHERE s.id = $1
            ORDER BY v.version DESC
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id, format_str, definition, subject, version, _hash)) = result {
            Ok(Some(Schema {
                id,
                subject,
                version,
                schema_type: Self::string_to_format(&format_str)?,
                schema: definition,
                references: vec![],
                metadata: Default::default(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_schema_by_subject_version(
        &self,
        subject: &str,
        version: i32,
    ) -> Result<Option<Schema>> {
        let result = sqlx::query_as::<_, (i32, String, String, i32)>(
            r#"
            SELECT
                s.id,
                s.schema_format,
                s.schema_definition,
                v.version
            FROM schema_registry_schemas s
            JOIN schema_registry_versions v ON s.id = v.schema_id
            WHERE v.subject = $1 AND v.version = $2
            "#,
        )
        .bind(subject)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id, format_str, definition, version)) = result {
            Ok(Some(Schema {
                id,
                subject: subject.to_string(),
                version,
                schema_type: Self::string_to_format(&format_str)?,
                schema: definition,
                references: vec![],
                metadata: Default::default(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_latest_schema(&self, subject: &str) -> Result<Option<Schema>> {
        let result = sqlx::query_as::<_, (i32, String, String, i32)>(
            r#"
            SELECT
                s.id,
                s.schema_format,
                s.schema_definition,
                v.version
            FROM schema_registry_schemas s
            JOIN schema_registry_versions v ON s.id = v.schema_id
            WHERE v.subject = $1
            ORDER BY v.version DESC
            LIMIT 1
            "#,
        )
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id, format_str, definition, version)) = result {
            Ok(Some(Schema {
                id,
                subject: subject.to_string(),
                version,
                schema_type: Self::string_to_format(&format_str)?,
                schema: definition,
                references: vec![],
                metadata: Default::default(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_versions(&self, subject: &str) -> Result<Vec<i32>> {
        let versions = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT version
            FROM schema_registry_versions
            WHERE subject = $1
            ORDER BY version ASC
            "#,
        )
        .bind(subject)
        .fetch_all(&self.pool)
        .await?;

        Ok(versions)
    }

    async fn get_subjects(&self) -> Result<Vec<String>> {
        let subjects = sqlx::query_scalar::<_, String>(
            r#"
            SELECT DISTINCT subject
            FROM schema_registry_versions
            ORDER BY subject ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(subjects)
    }

    async fn delete_subject(&self, subject: &str) -> Result<Vec<i32>> {
        // Get all versions before deleting
        let versions = self.get_versions(subject).await?;

        // Delete all versions for this subject
        sqlx::query(
            r#"
            DELETE FROM schema_registry_versions
            WHERE subject = $1
            "#,
        )
        .bind(subject)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            subject = %subject,
            versions = ?versions,
            "Subject deleted"
        );

        Ok(versions)
    }

    async fn delete_version(&self, subject: &str, version: i32) -> Result<i32> {
        sqlx::query(
            r#"
            DELETE FROM schema_registry_versions
            WHERE subject = $1 AND version = $2
            "#,
        )
        .bind(subject)
        .bind(version)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            subject = %subject,
            version = version,
            "Version deleted"
        );

        Ok(version)
    }

    async fn schema_exists(&self, subject: &str, schema: &str) -> Result<Option<i32>> {
        let schema_hash = Self::compute_hash(schema);

        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT s.id
            FROM schema_registry_schemas s
            JOIN schema_registry_versions v ON s.id = v.schema_id
            WHERE v.subject = $1 AND s.schema_hash = $2
            LIMIT 1
            "#,
        )
        .bind(subject)
        .bind(&schema_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    async fn get_subject_config(&self, subject: &str) -> Result<Option<SubjectConfig>> {
        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT compatibility
            FROM schema_registry_subject_config
            WHERE subject = $1
            "#,
        )
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((compatibility_str,)) = result {
            Ok(Some(SubjectConfig {
                subject: subject.to_string(),
                compatibility: Self::string_to_compatibility(&compatibility_str)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_subject_config(&self, config: SubjectConfig) -> Result<()> {
        let default_org = "00000000-0000-0000-0000-000000000000";
        sqlx::query(
            r#"
            INSERT INTO schema_registry_subject_config (subject, compatibility, organization_id, updated_at)
            VALUES ($1, $2, $3::UUID, NOW())
            ON CONFLICT (subject) DO UPDATE
            SET compatibility = $2, updated_at = NOW()
            "#,
        )
        .bind(&config.subject)
        .bind(Self::compatibility_to_string(config.compatibility))
        .bind(default_org)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            subject = %config.subject,
            compatibility = ?config.compatibility,
            "Subject config updated"
        );

        Ok(())
    }

    async fn get_global_compatibility(&self) -> Result<CompatibilityMode> {
        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT compatibility
            FROM schema_registry_global_config
            WHERE id = TRUE
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Self::string_to_compatibility(&result.0)
    }

    async fn set_global_compatibility(&self, mode: CompatibilityMode) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE schema_registry_global_config
            SET compatibility = $1, updated_at = NOW()
            WHERE id = TRUE
            "#,
        )
        .bind(Self::compatibility_to_string(mode))
        .execute(&self.pool)
        .await?;

        tracing::info!(
            compatibility = ?mode,
            "Global compatibility mode updated"
        );

        Ok(())
    }
}
