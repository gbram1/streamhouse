//! Connector management endpoints

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::Utc;

use crate::{
    auth::AuthenticatedKey,
    handlers::topics::extract_org_id,
    models::{ConnectorResponse, CreateConnectorRequest},
    AppState,
};
use streamhouse_metadata::ConnectorInfo;

/// Convert a ConnectorInfo into a ConnectorResponse
fn connector_to_response(info: ConnectorInfo) -> ConnectorResponse {
    ConnectorResponse {
        name: info.name,
        connector_type: info.connector_type,
        connector_class: info.connector_class,
        topics: info.topics,
        config: info.config,
        state: info.state,
        error_message: info.error_message,
        records_processed: info.records_processed,
        created_at: chrono::DateTime::from_timestamp_millis(info.created_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
        updated_at: chrono::DateTime::from_timestamp_millis(info.updated_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
    }
}

/// List all connectors
///
/// Returns a list of all registered connectors.
#[utoipa::path(
    get,
    path = "/api/v1/connectors",
    tag = "connectors",
    responses(
        (status = 200, description = "List of connectors", body = Vec<ConnectorResponse>),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list_connectors(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
) -> Result<Json<Vec<ConnectorResponse>>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;

    let connectors = state
        .metadata
        .list_connectors_for_org(&org_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response: Vec<ConnectorResponse> =
        connectors.into_iter().map(connector_to_response).collect();

    Ok(Json(response))
}

/// Create a new connector
///
/// Registers a new connector with the given configuration.
#[utoipa::path(
    post,
    path = "/api/v1/connectors",
    tag = "connectors",
    request_body = CreateConnectorRequest,
    responses(
        (status = 201, description = "Connector created", body = ConnectorResponse),
        (status = 409, description = "Connector already exists"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn create_connector(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Json(req): Json<CreateConnectorRequest>,
) -> Result<(StatusCode, Json<ConnectorResponse>), StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;
    let now = Utc::now().timestamp_millis();

    let info = ConnectorInfo {
        organization_id: org_id.clone(),
        name: req.name,
        connector_type: req.connector_type,
        connector_class: req.connector_class,
        topics: req.topics,
        config: req.config,
        state: "stopped".to_string(),
        error_message: None,
        records_processed: 0,
        created_at: now,
        updated_at: now,
    };

    state
        .metadata
        .create_connector_for_org(&org_id, info.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = connector_to_response(info);
    Ok((StatusCode::CREATED, Json(response)))
}

/// Get a specific connector by name
///
/// Returns details for a single connector.
#[utoipa::path(
    get,
    path = "/api/v1/connectors/{name}",
    tag = "connectors",
    params(
        ("name" = String, Path, description = "Connector name"),
    ),
    responses(
        (status = 200, description = "Connector details", body = ConnectorResponse),
        (status = 404, description = "Connector not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_connector(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<Json<ConnectorResponse>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;

    let connector = state
        .metadata
        .get_connector_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(connector_to_response(connector)))
}

/// Delete a connector
///
/// Removes a connector and its configuration.
#[utoipa::path(
    delete,
    path = "/api/v1/connectors/{name}",
    tag = "connectors",
    params(
        ("name" = String, Path, description = "Connector name"),
    ),
    responses(
        (status = 204, description = "Connector deleted"),
        (status = 404, description = "Connector not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn delete_connector(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;

    // Verify connector exists before deleting
    state
        .metadata
        .get_connector_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .metadata
        .delete_connector_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Pause a connector
///
/// Sets the connector state to "paused".
#[utoipa::path(
    post,
    path = "/api/v1/connectors/{name}/pause",
    tag = "connectors",
    params(
        ("name" = String, Path, description = "Connector name"),
    ),
    responses(
        (status = 200, description = "Connector paused", body = ConnectorResponse),
        (status = 404, description = "Connector not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn pause_connector(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<Json<ConnectorResponse>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;

    // Verify connector exists
    state
        .metadata
        .get_connector_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .metadata
        .update_connector_state_for_org(&org_id, &name, "paused", None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Re-fetch the updated connector
    let connector = state
        .metadata
        .get_connector_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(connector_to_response(connector)))
}

/// Resume a connector
///
/// Sets the connector state to "running".
#[utoipa::path(
    post,
    path = "/api/v1/connectors/{name}/resume",
    tag = "connectors",
    params(
        ("name" = String, Path, description = "Connector name"),
    ),
    responses(
        (status = 200, description = "Connector resumed", body = ConnectorResponse),
        (status = 404, description = "Connector not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn resume_connector(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_key: Option<Extension<AuthenticatedKey>>,
    Path(name): Path<String>,
) -> Result<Json<ConnectorResponse>, StatusCode> {
    let org_id = extract_org_id(&headers, auth_key.as_ref().map(|e| &e.0))?;

    // Verify connector exists
    state
        .metadata
        .get_connector_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .metadata
        .update_connector_state_for_org(&org_id, &name, "running", None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Re-fetch the updated connector
    let connector = state
        .metadata
        .get_connector_for_org(&org_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(connector_to_response(connector)))
}
