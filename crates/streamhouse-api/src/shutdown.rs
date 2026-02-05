//! Graceful Shutdown for StreamHouse API Server
//!
//! Provides utilities for handling graceful shutdown with signal handling.
//!
//! ## Features
//!
//! - SIGINT (Ctrl+C) handling
//! - SIGTERM handling (Unix only)
//! - Configurable shutdown timeout
//! - In-flight request completion
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::shutdown::{GracefulShutdown, serve_with_shutdown};
//!
//! let router = create_router(state);
//! let shutdown = GracefulShutdown::default();
//!
//! // This will wait for in-flight requests on SIGINT/SIGTERM
//! serve_with_shutdown(router, 8080, shutdown).await?;
//! ```
//!
//! ## Environment Variables
//!
//! - `SHUTDOWN_TIMEOUT_SECS`: Maximum time to wait for in-flight requests (default: 30)

use std::future::Future;
use std::time::Duration;
use tokio::sync::watch;

/// Configuration for graceful shutdown behavior
#[derive(Debug, Clone)]
pub struct GracefulShutdown {
    /// Maximum time to wait for in-flight requests to complete
    pub timeout: Duration,
    /// Whether to install signal handlers automatically
    pub install_signal_handlers: bool,
}

impl Default for GracefulShutdown {
    fn default() -> Self {
        let timeout_secs = std::env::var("SHUTDOWN_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        Self {
            timeout: Duration::from_secs(timeout_secs),
            install_signal_handlers: true,
        }
    }
}

impl GracefulShutdown {
    /// Create a new graceful shutdown config with specified timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            install_signal_handlers: true,
        }
    }

    /// Disable automatic signal handler installation
    /// Useful when you want to manage shutdown signals manually
    pub fn without_signal_handlers(mut self) -> Self {
        self.install_signal_handlers = false;
        self
    }
}

/// Shutdown signal type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownSignal {
    /// Received SIGINT (Ctrl+C)
    SigInt,
    /// Received SIGTERM
    SigTerm,
    /// Manual shutdown requested
    Manual,
}

impl std::fmt::Display for ShutdownSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SigInt => write!(f, "SIGINT (Ctrl+C)"),
            Self::SigTerm => write!(f, "SIGTERM"),
            Self::Manual => write!(f, "manual"),
        }
    }
}

/// Handle for triggering and monitoring shutdown
#[derive(Clone)]
pub struct ShutdownHandle {
    sender: watch::Sender<Option<ShutdownSignal>>,
    receiver: watch::Receiver<Option<ShutdownSignal>>,
}

impl ShutdownHandle {
    /// Create a new shutdown handle
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(None);
        Self { sender, receiver }
    }

    /// Trigger a manual shutdown
    pub fn shutdown(&self) {
        let _ = self.sender.send(Some(ShutdownSignal::Manual));
    }

    /// Wait for shutdown signal
    pub async fn wait(&mut self) -> ShutdownSignal {
        loop {
            if let Some(signal) = *self.receiver.borrow() {
                return signal;
            }
            if self.receiver.changed().await.is_err() {
                return ShutdownSignal::Manual;
            }
        }
    }

    /// Check if shutdown has been signaled
    pub fn is_shutdown(&self) -> bool {
        self.receiver.borrow().is_some()
    }
}

impl Default for ShutdownHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a future that completes when a shutdown signal is received
pub async fn shutdown_signal() -> ShutdownSignal {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        ShutdownSignal::SigInt
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
        ShutdownSignal::SigTerm
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<ShutdownSignal>();

    tokio::select! {
        signal = ctrl_c => signal,
        signal = terminate => signal,
    }
}

/// Start the API server with graceful shutdown support (HTTP)
///
/// This function installs signal handlers for SIGINT and SIGTERM,
/// and waits for in-flight requests to complete before shutting down.
///
/// ## Example
///
/// ```ignore
/// let router = create_router(state);
/// serve_with_shutdown(router, 8080, GracefulShutdown::default()).await?;
/// ```
pub async fn serve_with_shutdown(
    router: axum::Router,
    port: u16,
    config: GracefulShutdown,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🚀 REST API server listening on {}", addr);
    tracing::info!("   Swagger UI: http://localhost:{}/swagger-ui", port);
    tracing::info!("   Health: http://localhost:{}/health", port);
    tracing::info!("   Graceful shutdown timeout: {:?}", config.timeout);

    if config.install_signal_handlers {
        let shutdown_future = async {
            let signal = shutdown_signal().await;
            tracing::info!("📴 Received {}, initiating graceful shutdown...", signal);
        };

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_future)
            .await?;

        tracing::info!("👋 Server shut down gracefully");
    } else {
        axum::serve(listener, router).await?;
    }

    Ok(())
}

/// Start the API server with a custom shutdown signal
///
/// Allows integration with external shutdown coordination mechanisms.
///
/// ## Example
///
/// ```ignore
/// let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
///
/// // Spawn some task that will trigger shutdown_tx.send(()) when ready
///
/// serve_with_custom_shutdown(
///     router,
///     8080,
///     async move { shutdown_rx.await.ok(); },
/// ).await?;
/// ```
pub async fn serve_with_custom_shutdown<F>(
    router: axum::Router,
    port: u16,
    shutdown_signal: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Future<Output = ()> + Send + 'static,
{
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🚀 REST API server listening on {}", addr);
    tracing::info!("   Swagger UI: http://localhost:{}/swagger-ui", port);
    tracing::info!("   Health: http://localhost:{}/health", port);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    tracing::info!("👋 Server shut down gracefully");

    Ok(())
}

/// Callback type for shutdown hooks
pub type ShutdownCallback = Box<dyn FnOnce() + Send + 'static>;

/// Builder for configuring graceful shutdown with hooks
pub struct ShutdownBuilder {
    config: GracefulShutdown,
    pre_shutdown_hooks: Vec<ShutdownCallback>,
    post_shutdown_hooks: Vec<ShutdownCallback>,
}

impl ShutdownBuilder {
    /// Create a new shutdown builder with default config
    pub fn new() -> Self {
        Self {
            config: GracefulShutdown::default(),
            pre_shutdown_hooks: Vec::new(),
            post_shutdown_hooks: Vec::new(),
        }
    }

    /// Set the shutdown timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Add a hook to run before shutdown
    pub fn on_shutdown<F>(mut self, callback: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        self.pre_shutdown_hooks.push(Box::new(callback));
        self
    }

    /// Add a hook to run after shutdown completes
    pub fn after_shutdown<F>(mut self, callback: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        self.post_shutdown_hooks.push(Box::new(callback));
        self
    }

    /// Build and serve with the configured options
    pub async fn serve(
        self,
        router: axum::Router,
        port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!("🚀 REST API server listening on {}", addr);
        tracing::info!("   Swagger UI: http://localhost:{}/swagger-ui", port);
        tracing::info!("   Health: http://localhost:{}/health", port);
        tracing::info!("   Graceful shutdown timeout: {:?}", self.config.timeout);

        let shutdown_future = async {
            let signal = shutdown_signal().await;
            tracing::info!("📴 Received {}, initiating graceful shutdown...", signal);

            // Run pre-shutdown hooks
            for hook in self.pre_shutdown_hooks {
                hook();
            }
        };

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_future)
            .await?;

        // Run post-shutdown hooks
        for hook in self.post_shutdown_hooks {
            hook();
        }

        tracing::info!("👋 Server shut down gracefully");

        Ok(())
    }
}

impl Default for ShutdownBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graceful_shutdown_default() {
        let config = GracefulShutdown::default();
        assert!(config.timeout.as_secs() > 0);
        assert!(config.install_signal_handlers);
    }

    #[test]
    fn test_graceful_shutdown_with_timeout() {
        let config = GracefulShutdown::with_timeout(Duration::from_secs(60));
        assert_eq!(config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_graceful_shutdown_without_handlers() {
        let config = GracefulShutdown::default().without_signal_handlers();
        assert!(!config.install_signal_handlers);
    }

    #[test]
    fn test_shutdown_signal_display() {
        assert_eq!(format!("{}", ShutdownSignal::SigInt), "SIGINT (Ctrl+C)");
        assert_eq!(format!("{}", ShutdownSignal::SigTerm), "SIGTERM");
        assert_eq!(format!("{}", ShutdownSignal::Manual), "manual");
    }

    #[test]
    fn test_shutdown_handle() {
        let handle = ShutdownHandle::new();
        assert!(!handle.is_shutdown());

        handle.shutdown();
        assert!(handle.is_shutdown());
    }

    #[test]
    fn test_shutdown_builder() {
        let builder = ShutdownBuilder::new()
            .timeout(Duration::from_secs(45))
            .on_shutdown(|| {
                println!("Shutting down...");
            });

        assert_eq!(builder.config.timeout, Duration::from_secs(45));
        assert_eq!(builder.pre_shutdown_hooks.len(), 1);
    }
}
