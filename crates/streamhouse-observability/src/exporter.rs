use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use prometheus::{Encoder, TextEncoder};

use crate::metrics::REGISTRY;

/// Handler for Prometheus metrics endpoint
pub async fn metrics_handler() -> Response {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();

    let mut buffer = vec![];
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => (
            StatusCode::OK,
            [("content-type", encoder.format_type())],
            buffer,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to encode metrics: {}", e),
        )
            .into_response(),
    }
}

/// Create metrics router
pub fn create_metrics_router() -> Router {
    Router::new().route("/metrics", get(metrics_handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn test_metrics_endpoint() {
        // Metrics are automatically initialized via lazy_static
        // No need to call init() here

        // Create app
        let app = create_metrics_router();

        // Make request
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Check content type
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain; version=0.0.4"
        );
    }
}
