//! HTTP server for health checks and Prometheus metrics.
//!
//! This module provides a simple HTTP server that exposes:
//! - `/health` - Always returns 200 OK if the process is running
//! - `/ready` - Returns 200 OK if the agent has active leases
//! - `/metrics` - Prometheus exposition format metrics
//!
//! ## Architecture
//!
//! The metrics server runs alongside the gRPC server on a separate port (default: 8080).
//! It uses axum for HTTP routing and prometheus-client for metrics encoding.
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_agent::metrics_server::MetricsServer;
//!
//! let server = MetricsServer::new(
//!     "0.0.0.0:8080".parse().unwrap(),
//!     registry,
//!     agent,
//! );
//!
//! // Start server (blocks until shutdown)
//! server.start().await?;
//! ```

#[cfg(feature = "metrics")]
use crate::grpc_service::AgentMetrics;
#[cfg(feature = "metrics")]
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
#[cfg(feature = "metrics")]
use std::net::SocketAddr;
#[cfg(feature = "metrics")]
use std::sync::Arc;

/// HTTP server for health checks and metrics.
#[cfg(feature = "metrics")]
pub struct MetricsServer {
    addr: SocketAddr,
    registry: Arc<prometheus_client::registry::Registry>,
    has_active_leases: Arc<dyn Fn() -> bool + Send + Sync>,
}

#[cfg(feature = "metrics")]
impl MetricsServer {
    /// Create a new MetricsServer.
    ///
    /// # Arguments
    ///
    /// * `addr` - Socket address to bind to (e.g., "0.0.0.0:8080")
    /// * `registry` - Prometheus registry containing metrics
    /// * `has_active_leases` - Function to check if agent has active partition leases
    pub fn new(
        addr: SocketAddr,
        registry: Arc<prometheus_client::registry::Registry>,
        has_active_leases: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            addr,
            registry,
            has_active_leases,
        }
    }

    /// Start the HTTP server.
    ///
    /// This method blocks until the server is shut down.
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let app_state = Arc::new(AppState {
            registry: self.registry,
            has_active_leases: self.has_active_leases,
        });

        let app = Router::new()
            .route("/health", get(health_handler))
            .route("/ready", get(ready_handler))
            .route("/metrics", get(metrics_handler))
            .with_state(app_state);

        let listener = tokio::net::TcpListener::bind(&self.addr).await?;
        tracing::info!(addr = %self.addr, "Metrics server listening");

        axum::serve(listener, app).await?;

        Ok(())
    }
}

#[cfg(feature = "metrics")]
struct AppState {
    registry: Arc<prometheus_client::registry::Registry>,
    has_active_leases: Arc<dyn Fn() -> bool + Send + Sync>,
}

/// Health check handler - always returns 200 OK if process is running.
#[cfg(feature = "metrics")]
async fn health_handler() -> impl IntoResponse {
    "OK"
}

/// Readiness check handler - returns 200 OK if agent has active leases.
#[cfg(feature = "metrics")]
async fn ready_handler(State(state): State<Arc<AppState>>) -> Response {
    if (state.has_active_leases)() {
        "READY".into_response()
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}

/// Prometheus metrics handler - returns metrics in exposition format.
#[cfg(feature = "metrics")]
async fn metrics_handler(State(state): State<Arc<AppState>>) -> Response {
    use prometheus_client::encoding::text::encode;

    let mut buffer = String::new();
    if let Err(e) = encode(&mut buffer, &state.registry) {
        tracing::error!(error = %e, "Failed to encode metrics");
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    buffer.into_response()
}
