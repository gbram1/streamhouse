//! StreamHouse Observability
//!
//! Provides metrics, logging, and monitoring for StreamHouse.
//!
//! # Features
//!
//! - Prometheus metrics export
//! - Structured logging with tracing
//! - Health check endpoints
//! - Grafana dashboards
//!
//! # Usage
//!
//! ```no_run
//! use streamhouse_observability::{metrics, exporter};
//!
//! // Initialize metrics
//! metrics::init();
//!
//! // Create metrics router
//! let metrics_router = exporter::create_metrics_router();
//! ```

pub mod exporter;
pub mod metrics;

// Re-export commonly used items
pub use metrics::{init as init_metrics, REGISTRY};

/// Initialize all observability components
pub fn init() {
    // Initialize metrics
    metrics::init();

    // Initialize tracing (when implemented in 7.2)
    // tracing::init();
}
