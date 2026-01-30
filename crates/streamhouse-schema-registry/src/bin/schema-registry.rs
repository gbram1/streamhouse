//! StreamHouse Schema Registry Server
//!
//! Standalone schema registry server with REST API.
//!
//! # Environment Variables
//!
//! - `SCHEMA_REGISTRY_PORT`: HTTP port (default: 8081)
//! - `METADATA_STORE`: SQLite path or PostgreSQL URL (default: ./data/schema-registry.db)
//! - `RUST_LOG`: Log level (default: info)
//!
//! # Example
//!
//! ```bash
//! export SCHEMA_REGISTRY_PORT=8081
//! export METADATA_STORE=./data/schema-registry.db
//! cargo run --bin schema-registry
//! ```

use std::sync::Arc;
use streamhouse_metadata::SqliteMetadataStore;
use streamhouse_schema_registry::{MemorySchemaStorage, SchemaRegistry, SchemaRegistryApi};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    let log_level = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| "info".to_string())
        .parse()
        .unwrap_or(Level::INFO);

    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("🚀 StreamHouse Schema Registry starting...");

    // Load configuration
    let port = std::env::var("SCHEMA_REGISTRY_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8081);

    let metadata_store_url =
        std::env::var("METADATA_STORE").unwrap_or_else(|_| "./data/schema-registry.db".to_string());

    info!("Configuration:");
    info!("  Port: {}", port);
    info!("  Metadata store: {}", metadata_store_url);

    // Initialize metadata store
    info!("Connecting to metadata store...");
    let metadata = Arc::new(SqliteMetadataStore::new(&metadata_store_url).await?);
    info!("✓ Metadata store connected");

    // Initialize schema storage
    let storage = Arc::new(MemorySchemaStorage::new(metadata));

    // Create schema registry
    let registry = Arc::new(SchemaRegistry::new(storage));
    info!("✓ Schema registry initialized");

    // Create API server
    let api = SchemaRegistryApi::new(registry);
    let addr = format!("0.0.0.0:{}", port);

    info!("");
    info!("Schema Registry is now running");
    info!("  HTTP API: http://localhost:{}", port);
    info!("  Health:   http://localhost:{}/health", port);
    info!("");

    // Start server
    api.serve(&addr).await?;

    Ok(())
}
