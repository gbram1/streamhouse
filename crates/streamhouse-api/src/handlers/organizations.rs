//! Organization management handlers
//!
//! REST API endpoints for multi-tenancy organization management.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use streamhouse_metadata::{CreateOrganization, OrganizationPlan, OrganizationStatus};

/// Organization details
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrganizationResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub plan: String,
    pub status: String,
    pub created_at: i64,
    pub deployment_mode: String,
}

/// Create organization request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub deployment_mode: Option<String>,
}

/// Update organization request
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateOrganizationRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Organization quota response
#[derive(Debug, Serialize, ToSchema)]
pub struct OrganizationQuotaResponse {
    pub organization_id: String,
    pub max_topics: i32,
    pub max_partitions_per_topic: i32,
    pub max_total_partitions: i32,
    pub max_storage_bytes: i64,
    pub max_retention_days: i32,
    pub max_produce_bytes_per_sec: i64,
    pub max_consume_bytes_per_sec: i64,
    pub max_requests_per_sec: i32,
    pub max_consumer_groups: i32,
    pub max_schemas: i32,
    pub max_connections: i32,
}

/// Organization usage response
#[derive(Debug, Serialize, ToSchema)]
pub struct OrganizationUsageResponse {
    pub organization_id: String,
    pub topics_count: i64,
    pub partitions_count: i64,
    pub storage_bytes: i64,
    pub produce_bytes_last_hour: i64,
    pub consume_bytes_last_hour: i64,
    pub requests_last_hour: i64,
    pub consumer_groups_count: i64,
    pub schemas_count: i64,
}

/// Resolve an external organization to a StreamHouse organization.
///
/// If a StreamHouse org already exists for the given external ID, returns it.
/// Otherwise creates a new StreamHouse org and returns it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveOrganizationRequest {
    pub external_id: String,
    pub name: String,
    pub slug: String,
}

pub async fn resolve_organization(
    State(state): State<AppState>,
    Json(req): Json<ResolveOrganizationRequest>,
) -> Result<Json<OrganizationResponse>, StatusCode> {
    // 1. Try to find existing org by external_id
    if let Some(org) = state
        .metadata
        .get_organization_by_external_id(&req.external_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to look up organization by external_id: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        return Ok(Json(OrganizationResponse {
            id: org.id,
            name: org.name,
            slug: org.slug,
            plan: format!("{:?}", org.plan).to_lowercase(),
            status: format!("{:?}", org.status).to_lowercase(),
            created_at: org.created_at,
            deployment_mode: org.deployment_mode.to_string(),
        }));
    }

    // 2. Sanitize slug — lowercase alphanumeric + hyphens only
    let base_slug: String = req
        .slug
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect();
    let base_slug = if base_slug.is_empty() {
        req.external_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_lowercase()
    } else {
        base_slug
    };

    // 3. Deduplicate slug if already taken
    let mut slug = base_slug.clone();
    let mut attempt = 0u32;
    loop {
        match state.metadata.get_organization_by_slug(&slug).await {
            Ok(Some(_)) => {
                attempt += 1;
                slug = format!("{}-{}", base_slug, attempt);
            }
            Ok(None) => break,
            Err(e) => {
                tracing::error!("Failed to check slug availability: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // 4. Create new org
    let config = CreateOrganization {
        name: req.name,
        slug,
        plan: OrganizationPlan::Free,
        settings: std::collections::HashMap::new(),
        external_id: Some(req.external_id),
        deployment_mode: Default::default(),
    };

    let org = state
        .metadata
        .create_organization(config)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create organization: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(OrganizationResponse {
        id: org.id,
        name: org.name,
        slug: org.slug,
        plan: format!("{:?}", org.plan).to_lowercase(),
        status: format!("{:?}", org.status).to_lowercase(),
        created_at: org.created_at,
        deployment_mode: org.deployment_mode.to_string(),
    }))
}

/// List all organizations
#[utoipa::path(
    get,
    path = "/api/v1/organizations",
    responses(
        (status = 200, description = "List of organizations", body = Vec<OrganizationResponse>),
    ),
    tag = "organizations"
)]
pub async fn list_organizations(
    State(state): State<AppState>,
) -> Result<Json<Vec<OrganizationResponse>>, StatusCode> {
    let organizations = state.metadata.list_organizations().await.map_err(|e| {
        tracing::error!("Failed to list organizations: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let responses: Vec<OrganizationResponse> = organizations
        .into_iter()
        .filter(|org| org.id != streamhouse_metadata::DEFAULT_ORGANIZATION_ID)
        .map(|org| OrganizationResponse {
            id: org.id,
            name: org.name,
            slug: org.slug,
            plan: format!("{:?}", org.plan).to_lowercase(),
            status: format!("{:?}", org.status).to_lowercase(),
            created_at: org.created_at,
            deployment_mode: org.deployment_mode.to_string(),
        })
        .collect();

    Ok(Json(responses))
}

/// Get organization by ID
#[utoipa::path(
    get,
    path = "/api/v1/organizations/{id}",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Organization details", body = OrganizationResponse),
        (status = 404, description = "Organization not found"),
    ),
    tag = "organizations"
)]
pub async fn get_organization(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OrganizationResponse>, StatusCode> {
    let org = state
        .metadata
        .get_organization(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get organization: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(OrganizationResponse {
        id: org.id,
        name: org.name,
        slug: org.slug,
        plan: format!("{:?}", org.plan).to_lowercase(),
        status: format!("{:?}", org.status).to_lowercase(),
        created_at: org.created_at,
        deployment_mode: org.deployment_mode.to_string(),
    }))
}

/// Create a new organization
#[utoipa::path(
    post,
    path = "/api/v1/organizations",
    request_body = CreateOrganizationRequest,
    responses(
        (status = 201, description = "Organization created", body = OrganizationResponse),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Organization slug already exists"),
    ),
    tag = "organizations"
)]
pub async fn create_organization(
    State(state): State<AppState>,
    Json(req): Json<CreateOrganizationRequest>,
) -> Result<(StatusCode, Json<OrganizationResponse>), StatusCode> {
    // Validate slug format
    if !req
        .slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check if slug already exists
    if let Ok(Some(_)) = state.metadata.get_organization_by_slug(&req.slug).await {
        return Err(StatusCode::CONFLICT);
    }

    let plan = match req.plan.as_deref() {
        Some("pro") => OrganizationPlan::Pro,
        Some("enterprise") => OrganizationPlan::Enterprise,
        _ => OrganizationPlan::Free,
    };

    let deployment_mode = match req.deployment_mode.as_deref() {
        Some("byoc") => streamhouse_metadata::DeploymentMode::Byoc,
        Some("managed") => streamhouse_metadata::DeploymentMode::Managed,
        _ => streamhouse_metadata::DeploymentMode::SelfHosted,
    };

    let config = CreateOrganization {
        name: req.name,
        slug: req.slug,
        plan,
        settings: std::collections::HashMap::new(),
        external_id: None,
        deployment_mode,
    };

    let org = state
        .metadata
        .create_organization(config)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create organization: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((
        StatusCode::CREATED,
        Json(OrganizationResponse {
            id: org.id,
            name: org.name,
            slug: org.slug,
            plan: format!("{:?}", org.plan).to_lowercase(),
            status: format!("{:?}", org.status).to_lowercase(),
            created_at: org.created_at,
            deployment_mode: org.deployment_mode.to_string(),
        }),
    ))
}

/// Update an organization
#[utoipa::path(
    patch,
    path = "/api/v1/organizations/{id}",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    request_body = UpdateOrganizationRequest,
    responses(
        (status = 200, description = "Organization updated", body = OrganizationResponse),
        (status = 404, description = "Organization not found"),
    ),
    tag = "organizations"
)]
pub async fn update_organization(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateOrganizationRequest>,
) -> Result<Json<OrganizationResponse>, StatusCode> {
    // Verify organization exists
    let _org = state
        .metadata
        .get_organization(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Update plan if provided
    if let Some(plan_str) = &req.plan {
        let plan = match plan_str.as_str() {
            "free" => OrganizationPlan::Free,
            "pro" => OrganizationPlan::Pro,
            "enterprise" => OrganizationPlan::Enterprise,
            _ => return Err(StatusCode::BAD_REQUEST),
        };
        state
            .metadata
            .update_organization_plan(&id, plan)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    // Update status if provided
    if let Some(status_str) = &req.status {
        let status = match status_str.as_str() {
            "active" => OrganizationStatus::Active,
            "suspended" => OrganizationStatus::Suspended,
            "deleted" => OrganizationStatus::Deleted,
            _ => return Err(StatusCode::BAD_REQUEST),
        };
        state
            .metadata
            .update_organization_status(&id, status)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    // Fetch updated organization
    let org = state
        .metadata
        .get_organization(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(OrganizationResponse {
        id: org.id,
        name: org.name,
        slug: org.slug,
        plan: format!("{:?}", org.plan).to_lowercase(),
        status: format!("{:?}", org.status).to_lowercase(),
        created_at: org.created_at,
        deployment_mode: org.deployment_mode.to_string(),
    }))
}

/// Delete an organization
#[utoipa::path(
    delete,
    path = "/api/v1/organizations/{id}",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 204, description = "Organization deleted"),
        (status = 404, description = "Organization not found"),
    ),
    tag = "organizations"
)]
pub async fn delete_organization(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Verify organization exists
    let _org = state
        .metadata
        .get_organization(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state.metadata.delete_organization(&id).await.map_err(|e| {
        tracing::error!("Failed to delete organization: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get organization quota
#[utoipa::path(
    get,
    path = "/api/v1/organizations/{id}/quota",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Organization quota", body = OrganizationQuotaResponse),
        (status = 404, description = "Organization not found"),
    ),
    tag = "organizations"
)]
pub async fn get_organization_quota(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OrganizationQuotaResponse>, StatusCode> {
    // Verify organization exists
    let _org = state
        .metadata
        .get_organization(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let quota = state
        .metadata
        .get_organization_quota(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get organization quota: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(OrganizationQuotaResponse {
        organization_id: quota.organization_id,
        max_topics: quota.max_topics,
        max_partitions_per_topic: quota.max_partitions_per_topic,
        max_total_partitions: quota.max_total_partitions,
        max_storage_bytes: quota.max_storage_bytes,
        max_retention_days: quota.max_retention_days,
        max_produce_bytes_per_sec: quota.max_produce_bytes_per_sec,
        max_consume_bytes_per_sec: quota.max_consume_bytes_per_sec,
        max_requests_per_sec: quota.max_requests_per_sec,
        max_consumer_groups: quota.max_consumer_groups,
        max_schemas: quota.max_schemas,
        max_connections: quota.max_connections,
    }))
}

/// Get organization usage
#[utoipa::path(
    get,
    path = "/api/v1/organizations/{id}/usage",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Organization usage", body = OrganizationUsageResponse),
        (status = 404, description = "Organization not found"),
    ),
    tag = "organizations"
)]
pub async fn get_organization_usage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OrganizationUsageResponse>, StatusCode> {
    // Verify organization exists
    let _org = state
        .metadata
        .get_organization(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let usage_records = state
        .metadata
        .get_organization_usage(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get organization usage: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Aggregate usage metrics
    let mut response = OrganizationUsageResponse {
        organization_id: id.clone(),
        topics_count: 0,
        partitions_count: 0,
        storage_bytes: 0,
        produce_bytes_last_hour: 0,
        consume_bytes_last_hour: 0,
        requests_last_hour: 0,
        consumer_groups_count: 0,
        schemas_count: 0,
    };

    for usage in usage_records {
        match usage.metric.as_str() {
            "topics" => response.topics_count = usage.value,
            "partitions" => response.partitions_count = usage.value,
            "storage_bytes" => response.storage_bytes = usage.value,
            "produce_bytes" => response.produce_bytes_last_hour = usage.value,
            "consume_bytes" => response.consume_bytes_last_hour = usage.value,
            "requests" => response.requests_last_hour = usage.value,
            "consumer_groups" => response.consumer_groups_count = usage.value,
            "schemas" => response.schemas_count = usage.value,
            _ => {}
        }
    }

    Ok(Json(response))
}
