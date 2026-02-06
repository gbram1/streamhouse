//! SQL query endpoint

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;

/// SQL query request
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SqlQueryRequest {
    /// SQL query to execute
    pub query: String,
    /// Query timeout in milliseconds (default: 30000)
    pub timeout_ms: Option<u64>,
}

/// SQL query response
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SqlQueryResponse {
    /// Column metadata
    pub columns: Vec<ColumnInfo>,
    /// Result rows
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Number of rows returned
    pub row_count: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether results were truncated due to limit
    pub truncated: bool,
}

/// Column metadata
#[derive(Debug, Serialize, ToSchema)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

/// SQL error response
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SqlErrorResponse {
    pub error: String,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/sql",
    request_body = SqlQueryRequest,
    responses(
        (status = 200, description = "Query executed successfully", body = SqlQueryResponse),
        (status = 400, description = "Invalid SQL query", body = SqlErrorResponse),
        (status = 408, description = "Query timeout"),
        (status = 500, description = "Internal server error")
    ),
    tag = "sql"
)]
pub async fn execute_sql(
    State(state): State<AppState>,
    Json(req): Json<SqlQueryRequest>,
) -> Result<Json<SqlQueryResponse>, (StatusCode, Json<SqlErrorResponse>)> {
    // Create SQL executor
    let executor = streamhouse_sql::SqlExecutor::new(
        state.metadata.clone(),
        state.segment_cache.clone(),
        state.object_store.clone(),
    );

    // Execute query
    match executor.execute(&req.query, req.timeout_ms).await {
        Ok(result) => Ok(Json(SqlQueryResponse {
            columns: result
                .columns
                .into_iter()
                .map(|c| ColumnInfo {
                    name: c.name,
                    data_type: c.data_type,
                })
                .collect(),
            rows: result.rows,
            row_count: result.row_count,
            execution_time_ms: result.execution_time_ms,
            truncated: result.truncated,
        })),
        Err(e) => {
            let (status, error_type) = match &e {
                streamhouse_sql::SqlError::ParseError(_) => {
                    (StatusCode::BAD_REQUEST, "parse_error")
                }
                streamhouse_sql::SqlError::InvalidQuery(_) => {
                    (StatusCode::BAD_REQUEST, "invalid_query")
                }
                streamhouse_sql::SqlError::UnsupportedOperation(_) => {
                    (StatusCode::BAD_REQUEST, "unsupported_operation")
                }
                streamhouse_sql::SqlError::TopicNotFound(_) => {
                    (StatusCode::NOT_FOUND, "topic_not_found")
                }
                streamhouse_sql::SqlError::Timeout(_) => (StatusCode::REQUEST_TIMEOUT, "timeout"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "execution_error"),
            };

            Err((
                status,
                Json(SqlErrorResponse {
                    error: error_type.to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}
