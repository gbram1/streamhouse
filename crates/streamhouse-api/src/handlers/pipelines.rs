//! Pipeline management endpoints

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::Utc;

use crate::{
    auth::AuthenticatedKey,
    handlers::topics::extract_org_id,
    models::{
        CreatePipelineRequest, CreatePipelineTargetRequest, PipelineResponse,
        PipelineTargetResponse, UpdatePipelineRequest, ValidateTransformRequest,
        ValidateTransformResponse,
    },
    AppState,
};
use streamhouse_metadata::{PipelineInfo, PipelineTarget};

/// Convert a PipelineTarget into a PipelineTargetResponse
fn target_to_response(t: PipelineTarget) -> PipelineTargetResponse {
    PipelineTargetResponse {
        id: t.id,
        name: t.name,
        target_type: t.target_type,
        connection_config: t.connection_config,
        created_at: chrono::DateTime::from_timestamp_millis(t.created_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
        updated_at: chrono::DateTime::from_timestamp_millis(t.updated_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
    }
}

/// Convert a PipelineInfo into a PipelineResponse
fn pipeline_to_response(p: PipelineInfo) -> PipelineResponse {
    PipelineResponse {
        id: p.id,
        name: p.name,
        source_topic: p.source_topic,
        consumer_group: p.consumer_group,
        target_id: p.target_id,
        transform_sql: p.transform_sql,
        state: p.state,
        error_message: p.error_message,
        records_processed: p.records_processed,
        last_offset: p.last_offset,
        created_at: chrono::DateTime::from_timestamp_millis(p.created_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
        updated_at: chrono::DateTime::from_timestamp_millis(p.updated_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// Pipeline Targets
// ---------------------------------------------------------------------------

/// List all pipeline targets
///
/// Returns a list of all registered pipeline targets.
#[utoipa::path(
    get,
    path = "/api/v1/pipeline-targets",
    tag = "pipelines",
    responses(
        (status = 200, description = "List of pipeline targets", body = Vec<PipelineTargetResponse>),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list_pipeline_targets(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
) -> Result<Json<Vec<PipelineTargetResponse>>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));

    let targets = state
        .metadata
        .list_pipeline_targets_for_org(&org_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response: Vec<PipelineTargetResponse> =
        targets.into_iter().map(target_to_response).collect();

    Ok(Json(response))
}

/// Create a new pipeline target
///
/// Registers a new pipeline target with the given configuration.
#[utoipa::path(
    post,
    path = "/api/v1/pipeline-targets",
    tag = "pipelines",
    request_body = CreatePipelineTargetRequest,
    responses(
        (status = 201, description = "Pipeline target created", body = PipelineTargetResponse),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn create_pipeline_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Json(req): Json<CreatePipelineTargetRequest>,
) -> Result<(StatusCode, Json<PipelineTargetResponse>), StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));
    let now = Utc::now().timestamp_millis();

    let target = PipelineTarget {
        id: uuid::Uuid::new_v4().to_string(),
        organization_id: org_id.clone(),
        name: req.name,
        target_type: req.target_type,
        connection_config: req.connection_config,
        created_at: now,
        updated_at: now,
    };

    state
        .metadata
        .create_pipeline_target_for_org(&org_id, target.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = target_to_response(target);
    Ok((StatusCode::CREATED, Json(response)))
}

/// Get a specific pipeline target by name
///
/// Returns details for a single pipeline target.
#[utoipa::path(
    get,
    path = "/api/v1/pipeline-targets/{name}",
    tag = "pipelines",
    params(
        ("name" = String, Path, description = "Pipeline target name"),
    ),
    responses(
        (status = 200, description = "Pipeline target details", body = PipelineTargetResponse),
        (status = 404, description = "Pipeline target not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_pipeline_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<Json<PipelineTargetResponse>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));

    let target = state
        .metadata
        .get_pipeline_target_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(target_to_response(target)))
}

/// Delete a pipeline target
///
/// Removes a pipeline target by name.
#[utoipa::path(
    delete,
    path = "/api/v1/pipeline-targets/{name}",
    tag = "pipelines",
    params(
        ("name" = String, Path, description = "Pipeline target name"),
    ),
    responses(
        (status = 204, description = "Pipeline target deleted"),
        (status = 404, description = "Pipeline target not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn delete_pipeline_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));

    // Verify target exists before deleting
    state
        .metadata
        .get_pipeline_target_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .metadata
        .delete_pipeline_target_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Pipelines
// ---------------------------------------------------------------------------

/// List all pipelines
///
/// Returns a list of all registered pipelines.
#[utoipa::path(
    get,
    path = "/api/v1/pipelines",
    tag = "pipelines",
    responses(
        (status = 200, description = "List of pipelines", body = Vec<PipelineResponse>),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list_pipelines(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
) -> Result<Json<Vec<PipelineResponse>>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));

    let pipelines = state
        .metadata
        .list_pipelines_for_org(&org_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response: Vec<PipelineResponse> =
        pipelines.into_iter().map(pipeline_to_response).collect();

    Ok(Json(response))
}

/// Create a new pipeline
///
/// Registers a new pipeline with the given configuration.
#[utoipa::path(
    post,
    path = "/api/v1/pipelines",
    tag = "pipelines",
    request_body = CreatePipelineRequest,
    responses(
        (status = 201, description = "Pipeline created", body = PipelineResponse),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn create_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Json(req): Json<CreatePipelineRequest>,
) -> Result<(StatusCode, Json<PipelineResponse>), StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));
    let now = Utc::now().timestamp_millis();

    let pipeline = PipelineInfo {
        id: uuid::Uuid::new_v4().to_string(),
        organization_id: org_id.clone(),
        name: req.name.clone(),
        source_topic: req.source_topic,
        consumer_group: format!("pipeline-{}", req.name),
        target_id: req.target_id,
        transform_sql: req.transform_sql,
        state: "stopped".to_string(),
        error_message: None,
        records_processed: 0,
        last_offset: None,
        created_at: now,
        updated_at: now,
    };

    state
        .metadata
        .create_pipeline_for_org(&org_id, pipeline.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = pipeline_to_response(pipeline);
    Ok((StatusCode::CREATED, Json(response)))
}

/// Get a specific pipeline by name
///
/// Returns details for a single pipeline.
#[utoipa::path(
    get,
    path = "/api/v1/pipelines/{name}",
    tag = "pipelines",
    params(
        ("name" = String, Path, description = "Pipeline name"),
    ),
    responses(
        (status = 200, description = "Pipeline details", body = PipelineResponse),
        (status = 404, description = "Pipeline not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<Json<PipelineResponse>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));

    let pipeline = state
        .metadata
        .get_pipeline_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(pipeline_to_response(pipeline)))
}

/// Delete a pipeline
///
/// Removes a pipeline by name.
#[utoipa::path(
    delete,
    path = "/api/v1/pipelines/{name}",
    tag = "pipelines",
    params(
        ("name" = String, Path, description = "Pipeline name"),
    ),
    responses(
        (status = 204, description = "Pipeline deleted"),
        (status = 404, description = "Pipeline not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn delete_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));

    // Verify pipeline exists before deleting
    state
        .metadata
        .get_pipeline_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .metadata
        .delete_pipeline_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Update pipeline state
///
/// Updates the state and/or error message of a pipeline.
#[utoipa::path(
    patch,
    path = "/api/v1/pipelines/{name}",
    tag = "pipelines",
    params(
        ("name" = String, Path, description = "Pipeline name"),
    ),
    request_body = UpdatePipelineRequest,
    responses(
        (status = 200, description = "Pipeline updated", body = PipelineResponse),
        (status = 404, description = "Pipeline not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn update_pipeline_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
    Json(req): Json<UpdatePipelineRequest>,
) -> Result<Json<PipelineResponse>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0));

    // Verify pipeline exists
    state
        .metadata
        .get_pipeline_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(ref new_state) = req.state {
        state
            .metadata
            .update_pipeline_state_for_org(
                &org_id,
                &name,
                new_state,
                req.error_message.as_deref(),
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    // Re-fetch the updated pipeline
    let pipeline = state
        .metadata
        .get_pipeline_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(pipeline_to_response(pipeline)))
}

// ---------------------------------------------------------------------------
// Transform validation
// ---------------------------------------------------------------------------

/// Validate a SQL transform
///
/// Checks whether the given SQL is a valid SELECT statement for use in a pipeline transform.
#[utoipa::path(
    post,
    path = "/api/v1/transforms/validate",
    tag = "pipelines",
    request_body = ValidateTransformRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateTransformResponse),
    )
)]
pub async fn validate_transform(
    Json(req): Json<ValidateTransformRequest>,
) -> Json<ValidateTransformResponse> {
    match streamhouse_sql::transform::TransformEngine::validate_sql(&req.sql) {
        Ok(()) => Json(ValidateTransformResponse {
            valid: true,
            error: None,
        }),
        Err(e) => Json(ValidateTransformResponse {
            valid: false,
            error: Some(e.to_string()),
        }),
    }
}
