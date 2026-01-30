//! REST API for Schema Registry
//!
//! Implements Confluent Schema Registry-compatible HTTP API.

use crate::{
    error::{Result, SchemaError},
    registry::SchemaRegistry,
    types::*,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

/// Schema Registry API Server
pub struct SchemaRegistryApi {
    registry: Arc<SchemaRegistry>,
}

impl SchemaRegistryApi {
    pub fn new(registry: Arc<SchemaRegistry>) -> Self {
        Self { registry }
    }

    /// Create router with all API endpoints
    pub fn router(&self) -> Router {
        Router::new()
            // Subjects
            .route("/subjects", get(list_subjects))
            .route("/subjects/:subject/versions", get(list_versions))
            .route("/subjects/:subject/versions", post(register_schema))
            .route(
                "/subjects/:subject/versions/:version",
                get(get_schema_by_version),
            )
            .route(
                "/subjects/:subject/versions/:version",
                delete(delete_schema_version),
            )
            .route("/subjects/:subject", delete(delete_subject))
            // Schemas by ID
            .route("/schemas/ids/:id", get(get_schema_by_id))
            // Config
            .route("/config", get(get_global_config))
            .route("/config", put(set_global_config))
            .route("/config/:subject", get(get_subject_config))
            .route("/config/:subject", put(set_subject_config))
            // Health
            .route("/health", get(health_check))
            .layer(CorsLayer::permissive())
            .with_state(Arc::clone(&self.registry))
    }

    /// Start the API server
    pub async fn serve(self, addr: &str) -> Result<()> {
        let app = self.router();

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| SchemaError::IoError(e))?;

        info!(addr = %addr, "Schema Registry API server listening");

        axum::serve(listener, app)
            .await
            .map_err(|e| SchemaError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }
}

// API Handlers

/// List all subjects
async fn list_subjects(State(registry): State<Arc<SchemaRegistry>>) -> Result<Json<Vec<String>>> {
    let subjects = registry.get_subjects().await?;
    Ok(Json(subjects))
}

/// List all versions for a subject
async fn list_versions(
    State(registry): State<Arc<SchemaRegistry>>,
    Path(subject): Path<String>,
) -> Result<Json<Vec<i32>>> {
    let versions = registry.get_versions(&subject).await?;
    Ok(Json(versions))
}

/// Register a new schema
async fn register_schema(
    State(registry): State<Arc<SchemaRegistry>>,
    Path(subject): Path<String>,
    Json(request): Json<RegisterSchemaRequest>,
) -> Result<Json<RegisterSchemaResponse>> {
    let id = registry.register_schema(&subject, request).await?;
    Ok(Json(RegisterSchemaResponse { id }))
}

/// Get schema by subject and version
async fn get_schema_by_version(
    State(registry): State<Arc<SchemaRegistry>>,
    Path((subject, version)): Path<(String, String)>,
) -> Result<Json<SchemaResponse>> {
    let version_num = if version == "latest" {
        let schema = registry.get_latest_schema(&subject).await?;
        return Ok(Json(SchemaResponse {
            subject: schema.subject,
            version: schema.version,
            id: schema.id,
            schema: schema.schema,
            schema_type: schema.schema_type,
            references: schema.references,
        }));
    } else {
        version
            .parse::<i32>()
            .map_err(|_| SchemaError::InvalidFormat(format!("Invalid version: {}", version)))?
    };

    let schema = registry.get_schema(&subject, version_num).await?;
    Ok(Json(SchemaResponse {
        subject: schema.subject,
        version: schema.version,
        id: schema.id,
        schema: schema.schema,
        schema_type: schema.schema_type,
        references: schema.references,
    }))
}

/// Get schema by ID
async fn get_schema_by_id(
    State(registry): State<Arc<SchemaRegistry>>,
    Path(id): Path<i32>,
) -> Result<Json<SchemaResponse>> {
    let schema = registry.get_schema_by_id(id).await?;
    Ok(Json(SchemaResponse {
        subject: schema.subject,
        version: schema.version,
        id: schema.id,
        schema: schema.schema,
        schema_type: schema.schema_type,
        references: schema.references,
    }))
}

/// Delete a subject
async fn delete_subject(
    State(registry): State<Arc<SchemaRegistry>>,
    Path(subject): Path<String>,
) -> Result<Json<Vec<i32>>> {
    let versions = registry.delete_subject(&subject).await?;
    Ok(Json(versions))
}

/// Delete a specific schema version
async fn delete_schema_version(
    State(registry): State<Arc<SchemaRegistry>>,
    Path((subject, version)): Path<(String, i32)>,
) -> Result<Json<i32>> {
    let deleted_version = registry.delete_version(&subject, version).await?;
    Ok(Json(deleted_version))
}

/// Get global compatibility configuration
async fn get_global_config(
    State(registry): State<Arc<SchemaRegistry>>,
) -> Result<Json<CompatibilityResponse>> {
    let mode = registry.get_global_compatibility().await?;
    Ok(Json(CompatibilityResponse {
        compatibility_level: mode,
    }))
}

/// Set global compatibility configuration
async fn set_global_config(
    State(registry): State<Arc<SchemaRegistry>>,
    Json(request): Json<CompatibilityRequest>,
) -> Result<Json<CompatibilityResponse>> {
    registry
        .set_global_compatibility(request.compatibility)
        .await?;
    Ok(Json(CompatibilityResponse {
        compatibility_level: request.compatibility,
    }))
}

/// Get subject-specific compatibility configuration
async fn get_subject_config(
    State(registry): State<Arc<SchemaRegistry>>,
    Path(subject): Path<String>,
) -> Result<Json<CompatibilityResponse>> {
    if let Some(config) = registry.get_subject_config(&subject).await? {
        Ok(Json(CompatibilityResponse {
            compatibility_level: config.compatibility,
        }))
    } else {
        // Fall back to global config
        let mode = registry.get_global_compatibility().await?;
        Ok(Json(CompatibilityResponse {
            compatibility_level: mode,
        }))
    }
}

/// Set subject-specific compatibility configuration
async fn set_subject_config(
    State(registry): State<Arc<SchemaRegistry>>,
    Path(subject): Path<String>,
    Json(request): Json<CompatibilityRequest>,
) -> Result<Json<CompatibilityResponse>> {
    registry
        .set_subject_compatibility(&subject, request.compatibility)
        .await?;
    Ok(Json(CompatibilityResponse {
        compatibility_level: request.compatibility,
    }))
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

// Request/Response types

#[derive(Debug, serde::Deserialize)]
struct CompatibilityRequest {
    compatibility: CompatibilityMode,
}

#[derive(Debug, serde::Serialize)]
struct CompatibilityResponse {
    #[serde(rename = "compatibilityLevel")]
    compatibility_level: CompatibilityMode,
}

// Error handling

impl IntoResponse for SchemaError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match self {
            SchemaError::SchemaNotFound { subject, version } => (
                StatusCode::NOT_FOUND,
                40401,
                format!("Schema not found: {} version {}", subject, version),
            ),
            SchemaError::SubjectNotFound(subject) => (
                StatusCode::NOT_FOUND,
                40401,
                format!("Subject not found: {}", subject),
            ),
            SchemaError::InvalidSchema(msg) => (StatusCode::UNPROCESSABLE_ENTITY, 42201, msg),
            SchemaError::IncompatibleSchema(msg) => (StatusCode::CONFLICT, 409, msg),
            SchemaError::SchemaAlreadyExists(id) => (
                StatusCode::CONFLICT,
                409,
                format!("Schema already exists with ID {}", id),
            ),
            SchemaError::InvalidFormat(msg) => (StatusCode::BAD_REQUEST, 400, msg),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, 50001, self.to_string()),
        };

        let body = serde_json::json!({
            "error_code": error_code,
            "message": message,
        });

        (status, Json(body)).into_response()
    }
}
