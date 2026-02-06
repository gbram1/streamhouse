//! TLS Configuration for StreamHouse API Server
//!
//! Supports both TLS (server authentication) and mTLS (mutual authentication).
//!
//! ## Usage
//!
//! ### TLS (Server-side only)
//! ```ignore
//! let tls_config = TlsConfig::new("server.crt", "server.key")?;
//! serve_with_tls(router, 8443, tls_config).await?;
//! ```
//!
//! ### mTLS (Mutual TLS)
//! ```ignore
//! let tls_config = TlsConfig::new("server.crt", "server.key")?
//!     .with_client_auth("ca.crt")?;  // Require client certificates
//! serve_with_tls(router, 8443, tls_config).await?;
//! ```
//!
//! ## Environment Variables
//!
//! - `TLS_CERT_PATH`: Path to server certificate (PEM format)
//! - `TLS_KEY_PATH`: Path to server private key (PEM format)
//! - `TLS_CA_CERT_PATH`: Path to CA certificate for client verification (mTLS)
//! - `TLS_ENABLED`: Set to "true" to enable TLS

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::RootCertStore;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

/// TLS configuration errors
#[derive(Debug, Error)]
pub enum TlsError {
    #[error("Failed to read certificate file: {0}")]
    CertificateRead(#[from] std::io::Error),

    #[error("Failed to parse certificate: {0}")]
    CertificateParse(String),

    #[error("Failed to parse private key: {0}")]
    PrivateKeyParse(String),

    #[error("Failed to build TLS config: {0}")]
    ConfigBuild(String),

    #[error("No private key found in file")]
    NoPrivateKey,
}

/// TLS configuration for the API server
pub struct TlsConfig {
    /// Server certificate chain (PEM)
    pub cert_chain: Vec<CertificateDer<'static>>,
    /// Server private key
    pub private_key: PrivateKeyDer<'static>,
    /// Optional CA certificates for client verification (mTLS)
    pub client_ca_certs: Option<RootCertStore>,
    /// Require client certificates (mTLS mode)
    pub require_client_cert: bool,
}

impl TlsConfig {
    /// Create a new TLS configuration from certificate and key files
    pub fn new<P: AsRef<Path>>(cert_path: P, key_path: P) -> Result<Self, TlsError> {
        let cert_chain = load_certs(cert_path.as_ref())?;
        let private_key = load_private_key(key_path.as_ref())?;

        Ok(Self {
            cert_chain,
            private_key,
            client_ca_certs: None,
            require_client_cert: false,
        })
    }

    /// Enable mTLS by requiring client certificates signed by the given CA
    pub fn with_client_auth<P: AsRef<Path>>(mut self, ca_cert_path: P) -> Result<Self, TlsError> {
        let ca_certs = load_certs(ca_cert_path.as_ref())?;
        let mut root_store = RootCertStore::empty();

        for cert in ca_certs {
            root_store
                .add(cert)
                .map_err(|e| TlsError::ConfigBuild(format!("Failed to add CA cert: {}", e)))?;
        }

        self.client_ca_certs = Some(root_store);
        self.require_client_cert = true;
        Ok(self)
    }

    /// Build the rustls ServerConfig
    pub fn build_server_config(&self) -> Result<rustls::ServerConfig, TlsError> {
        let builder = rustls::ServerConfig::builder();

        let config = if let Some(ref client_ca) = self.client_ca_certs {
            // mTLS mode: require client certificates
            let client_verifier = WebPkiClientVerifier::builder(Arc::new(client_ca.clone()))
                .build()
                .map_err(|e| {
                    TlsError::ConfigBuild(format!("Failed to build client verifier: {}", e))
                })?;

            builder
                .with_client_cert_verifier(client_verifier)
                .with_single_cert(self.cert_chain.clone(), self.private_key.clone_key())
                .map_err(|e| TlsError::ConfigBuild(format!("Failed to set server cert: {}", e)))?
        } else {
            // TLS mode: no client authentication
            builder
                .with_no_client_auth()
                .with_single_cert(self.cert_chain.clone(), self.private_key.clone_key())
                .map_err(|e| TlsError::ConfigBuild(format!("Failed to set server cert: {}", e)))?
        };

        Ok(config)
    }

    /// Create TLS config from environment variables
    ///
    /// - `TLS_CERT_PATH`: Server certificate path
    /// - `TLS_KEY_PATH`: Server private key path
    /// - `TLS_CA_CERT_PATH`: CA certificate for mTLS (optional)
    pub fn from_env() -> Result<Option<Self>, TlsError> {
        let enabled = std::env::var("TLS_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        if !enabled {
            return Ok(None);
        }

        let cert_path = std::env::var("TLS_CERT_PATH")
            .map_err(|_| TlsError::ConfigBuild("TLS_CERT_PATH not set".to_string()))?;
        let key_path = std::env::var("TLS_KEY_PATH")
            .map_err(|_| TlsError::ConfigBuild("TLS_KEY_PATH not set".to_string()))?;

        let mut config = Self::new(&cert_path, &key_path)?;

        // Check for mTLS
        if let Ok(ca_path) = std::env::var("TLS_CA_CERT_PATH") {
            config = config.with_client_auth(&ca_path)?;
            tracing::info!("🔐 mTLS enabled: client certificates required");
        }

        Ok(Some(config))
    }
}

/// Load certificates from a PEM file
fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let certs: Vec<_> = rustls_pemfile::certs(&mut reader)
        .filter_map(|result| result.ok())
        .collect();

    if certs.is_empty() {
        return Err(TlsError::CertificateParse(
            "No certificates found in file".to_string(),
        ));
    }

    Ok(certs)
}

/// Load private key from a PEM file
fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Try to read PKCS#8 key first, then RSA key, then EC key
    for item in rustls_pemfile::read_all(&mut reader).flatten() {
        match item {
            rustls_pemfile::Item::Pkcs1Key(key) => {
                return Ok(PrivateKeyDer::Pkcs1(key));
            }
            rustls_pemfile::Item::Pkcs8Key(key) => {
                return Ok(PrivateKeyDer::Pkcs8(key));
            }
            rustls_pemfile::Item::Sec1Key(key) => {
                return Ok(PrivateKeyDer::Sec1(key));
            }
            _ => continue,
        }
    }

    Err(TlsError::NoPrivateKey)
}

/// Serve an Axum router with TLS
pub async fn serve_with_tls(
    router: axum::Router,
    port: u16,
    tls_config: TlsConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use axum_server::tls_rustls::RustlsConfig;

    let rustls_config = tls_config.build_server_config()?;
    let axum_tls_config = RustlsConfig::from_config(Arc::new(rustls_config));

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    let mode = if tls_config.require_client_cert {
        "mTLS"
    } else {
        "TLS"
    };

    tracing::info!(
        "🔒 REST API server listening on {} ({} enabled)",
        addr,
        mode
    );
    tracing::info!("   Swagger UI: https://localhost:{}/swagger-ui", port);
    tracing::info!("   Health: https://localhost:{}/health", port);

    axum_server::bind_rustls(addr, axum_tls_config)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}

/// Serve an Axum router with TLS and graceful shutdown support
///
/// Installs signal handlers for SIGINT and SIGTERM, and waits for
/// in-flight requests to complete before shutting down.
///
/// ## Example
///
/// ```ignore
/// let tls_config = TlsConfig::new("server.crt", "server.key")?;
/// let shutdown = GracefulShutdown::default();
/// serve_with_tls_and_shutdown(router, 8443, tls_config, shutdown).await?;
/// ```
pub async fn serve_with_tls_and_shutdown(
    router: axum::Router,
    port: u16,
    tls_config: TlsConfig,
    shutdown_config: crate::shutdown::GracefulShutdown,
) -> Result<(), Box<dyn std::error::Error>> {
    use axum_server::tls_rustls::RustlsConfig;
    use axum_server::Handle;

    let rustls_config = tls_config.build_server_config()?;
    let axum_tls_config = RustlsConfig::from_config(Arc::new(rustls_config));

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    let mode = if tls_config.require_client_cert {
        "mTLS"
    } else {
        "TLS"
    };

    tracing::info!(
        "🔒 REST API server listening on {} ({} enabled)",
        addr,
        mode
    );
    tracing::info!("   Swagger UI: https://localhost:{}/swagger-ui", port);
    tracing::info!("   Health: https://localhost:{}/health", port);
    tracing::info!(
        "   Graceful shutdown timeout: {:?}",
        shutdown_config.timeout
    );

    // Create handle for graceful shutdown
    let handle = Handle::new();
    let shutdown_handle = handle.clone();
    let timeout = shutdown_config.timeout;

    // Spawn shutdown signal handler
    if shutdown_config.install_signal_handlers {
        tokio::spawn(async move {
            let signal = crate::shutdown::shutdown_signal().await;
            tracing::info!("📴 Received {}, initiating graceful shutdown...", signal);

            // Tell the server to stop accepting new connections
            // and wait for in-flight requests
            shutdown_handle.graceful_shutdown(Some(timeout));
        });
    }

    axum_server::bind_rustls(addr, axum_tls_config)
        .handle(handle)
        .serve(router.into_make_service())
        .await?;

    tracing::info!("👋 Server shut down gracefully");

    Ok(())
}

/// Serve an Axum router with TLS and a custom shutdown signal
///
/// Allows integration with external shutdown coordination mechanisms.
pub async fn serve_with_tls_custom_shutdown<F>(
    router: axum::Router,
    port: u16,
    tls_config: TlsConfig,
    timeout: std::time::Duration,
    shutdown_signal: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    use axum_server::tls_rustls::RustlsConfig;
    use axum_server::Handle;

    let rustls_config = tls_config.build_server_config()?;
    let axum_tls_config = RustlsConfig::from_config(Arc::new(rustls_config));

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    let mode = if tls_config.require_client_cert {
        "mTLS"
    } else {
        "TLS"
    };

    tracing::info!(
        "🔒 REST API server listening on {} ({} enabled)",
        addr,
        mode
    );
    tracing::info!("   Swagger UI: https://localhost:{}/swagger-ui", port);
    tracing::info!("   Health: https://localhost:{}/health", port);

    let handle = Handle::new();
    let shutdown_handle = handle.clone();

    tokio::spawn(async move {
        shutdown_signal.await;
        tracing::info!("📴 Shutdown signal received, initiating graceful shutdown...");
        shutdown_handle.graceful_shutdown(Some(timeout));
    });

    axum_server::bind_rustls(addr, axum_tls_config)
        .handle(handle)
        .serve(router.into_make_service())
        .await?;

    tracing::info!("👋 Server shut down gracefully");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_from_env_disabled() {
        std::env::remove_var("TLS_ENABLED");
        let config = TlsConfig::from_env().unwrap();
        assert!(config.is_none());
    }

    #[test]
    fn test_tls_config_from_env_enabled_missing_paths() {
        std::env::set_var("TLS_ENABLED", "true");
        std::env::remove_var("TLS_CERT_PATH");

        let result = TlsConfig::from_env();
        assert!(result.is_err());

        std::env::remove_var("TLS_ENABLED");
    }
}
