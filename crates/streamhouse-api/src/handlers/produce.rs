//! Message produce endpoint

use axum::{extract::State, http::StatusCode, Json};

use crate::{models::*, AppState};

#[utoipa::path(
    post,
    path = "/api/v1/produce",
    request_body = ProduceRequest,
    responses(
        (status = 200, description = "Message produced", body = ProduceResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Topic not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "produce"
)]
pub async fn produce(
    State(state): State<AppState>,
    Json(req): Json<ProduceRequest>,
) -> Result<Json<ProduceResponse>, StatusCode> {
    // Validate topic exists
    state
        .metadata
        .get_topic(&req.topic)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Send message
    let result = state
        .producer
        .send(
            &req.topic,
            req.key.as_deref().map(|k| k.as_bytes()),
            req.value.as_bytes(),
            req.partition,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ProduceResponse {
        offset: result.offset.unwrap_or(0),
        partition: result.partition,
    }))
}
