#![recursion_limit = "512"]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_does_not_panic() {
        init();
    }

    #[test]
    fn test_init_metrics_alias() {
        init_metrics();
    }

    #[test]
    fn test_registry_accessible() {
        // Ensure REGISTRY is accessible after init
        init();
        let _registry = &*REGISTRY;
    }

    #[test]
    fn test_double_init_is_safe() {
        // Calling init twice should not panic
        init();
        init();
    }
}
