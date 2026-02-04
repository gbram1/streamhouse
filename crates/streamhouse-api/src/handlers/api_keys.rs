//! API Key management handlers
//!
//! REST API endpoints for API key creation and management.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use streamhouse_metadata::tenant::generate_api_key;
use streamhouse_metadata::CreateApiKey;

/// API key response (without the actual key for security)
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyResponse {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub key_prefix: String,
    pub permissions: Vec<String>,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
}

/// API key creation response (includes the actual key - only shown once)
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyCreatedResponse {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    /// The full API key - only returned at creation time
    pub key: String,
    pub key_prefix: String,
    pub permissions: Vec<String>,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
}

/// Create API key request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// Permissions: "read", "write", "admin"
    #[serde(default = "default_permissions")]
    pub permissions: Vec<String>,
    /// Topic scopes (patterns like "orders-*", empty = all topics)
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Optional expiration duration in milliseconds from now
    #[serde(default)]
    pub expires_in_ms: Option<i64>,
}

fn default_permissions() -> Vec<String> {
    vec!["read".to_string(), "write".to_string()]
}

/// List API keys for an organization
#[utoipa::path(
    get,
    path = "/api/v1/organizations/{org_id}/api-keys",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of API keys", body = Vec<ApiKeyResponse>),
        (status = 404, description = "Organization not found"),
    ),
    tag = "api-keys"
)]
pub async fn list_api_keys(
    State(state): State<AppState>,
    Path(org_id): Path<String>,
) -> Result<Json<Vec<ApiKeyResponse>>, StatusCode> {
    // Verify organization exists
    let _org = state
        .metadata
        .get_organization(&org_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let keys = state
        .metadata
        .list_api_keys(&org_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list API keys: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let responses: Vec<ApiKeyResponse> = keys
        .into_iter()
        .map(|key| ApiKeyResponse {
            id: key.id,
            organization_id: key.organization_id,
            name: key.name,
            key_prefix: key.key_prefix,
            permissions: key.permissions,
            scopes: key.scopes,
            expires_at: key.expires_at,
            last_used_at: key.last_used_at,
            created_at: key.created_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Get API key by ID
#[utoipa::path(
    get,
    path = "/api/v1/api-keys/{id}",
    params(
        ("id" = String, Path, description = "API Key ID")
    ),
    responses(
        (status = 200, description = "API key details", body = ApiKeyResponse),
        (status = 404, description = "API key not found"),
    ),
    tag = "api-keys"
)]
pub async fn get_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiKeyResponse>, StatusCode> {
    let key = state
        .metadata
        .get_api_key(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get API key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ApiKeyResponse {
        id: key.id,
        organization_id: key.organization_id,
        name: key.name,
        key_prefix: key.key_prefix,
        permissions: key.permissions,
        scopes: key.scopes,
        expires_at: key.expires_at,
        last_used_at: key.last_used_at,
        created_at: key.created_at,
    }))
}

/// Create a new API key
#[utoipa::path(
    post,
    path = "/api/v1/organizations/{org_id}/api-keys",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    request_body = CreateApiKeyRequest,
    responses(
        (status = 201, description = "API key created", body = ApiKeyCreatedResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Organization not found"),
    ),
    tag = "api-keys"
)]
pub async fn create_api_key(
    State(state): State<AppState>,
    Path(org_id): Path<String>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<ApiKeyCreatedResponse>), StatusCode> {
    // Verify organization exists
    let _org = state
        .metadata
        .get_organization(&org_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Validate permissions
    for perm in &req.permissions {
        match perm.as_str() {
            "read" | "write" | "admin" => {}
            _ => return Err(StatusCode::BAD_REQUEST),
        }
    }

    // Generate the API key
    let (raw_key, key_hash, key_prefix) = generate_api_key("sk_live");

    let config = CreateApiKey {
        name: req.name,
        permissions: req.permissions,
        scopes: req.scopes,
        expires_in_ms: req.expires_in_ms,
    };

    let key = state
        .metadata
        .create_api_key(&org_id, config, &key_hash, &key_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create API key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((
        StatusCode::CREATED,
        Json(ApiKeyCreatedResponse {
            id: key.id,
            organization_id: key.organization_id,
            name: key.name,
            key: raw_key,
            key_prefix: key.key_prefix,
            permissions: key.permissions,
            scopes: key.scopes,
            expires_at: key.expires_at,
            created_at: key.created_at,
        }),
    ))
}

/// Revoke (delete) an API key
#[utoipa::path(
    delete,
    path = "/api/v1/api-keys/{id}",
    params(
        ("id" = String, Path, description = "API Key ID")
    ),
    responses(
        (status = 204, description = "API key revoked"),
        (status = 404, description = "API key not found"),
    ),
    tag = "api-keys"
)]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Verify API key exists
    let _key = state
        .metadata
        .get_api_key(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .metadata
        .revoke_api_key(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to revoke API key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}
