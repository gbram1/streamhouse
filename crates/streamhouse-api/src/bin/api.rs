//! StreamHouse REST API Server Binary
//!
//! # Environment Variables
//!
//! - `METADATA_STORE`: SQLite path or PostgreSQL URL (required)
//! - `API_PORT`: HTTP port (default: 3001)
//! - `AWS_ACCESS_KEY_ID`: S3 access key (required)
//! - `AWS_SECRET_ACCESS_KEY`: S3 secret key (required)
//! - `AWS_ENDPOINT_URL`: MinIO/S3 endpoint (optional)
//! - `AWS_REGION`: AWS region (default: us-east-1)
//! - `STREAMHOUSE_BUCKET`: S3 bucket name (default: streamhouse-data)
//! - `RUST_LOG`: Log level (default: info)
//!
//! # Example
//!
//! ```bash
//! export METADATA_STORE=./data/metadata.db
//! export AWS_ENDPOINT_URL=http://localhost:9000
//! export AWS_ACCESS_KEY_ID=minioadmin
//! export AWS_SECRET_ACCESS_KEY=minioadmin
//! export API_PORT=3001
//! cargo run --bin api
//! ```

use std::sync::Arc;
use streamhouse_api::{create_router, serve, AppState};
use streamhouse_client::Producer;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};
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

    info!("🚀 StreamHouse REST API starting...");

    // Load configuration
    let metadata_store_url =
        std::env::var("METADATA_STORE").expect("METADATA_STORE environment variable required");

    let api_port = std::env::var("API_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3001);

    info!("Configuration:");
    info!("  Metadata Store: {}", metadata_store_url);
    info!("  API Port: {}", api_port);

    // Connect to metadata store
    info!("Connecting to metadata store...");
    let metadata: Arc<dyn MetadataStore> = if metadata_store_url.starts_with("postgresql://")
        || metadata_store_url.starts_with("postgres://")
    {
        #[cfg(feature = "postgres")]
        {
            info!("  Using PostgreSQL");
            Arc::new(streamhouse_metadata::PostgresMetadataStore::new(&metadata_store_url).await?)
        }
        #[cfg(not(feature = "postgres"))]
        {
            return Err("PostgreSQL URL provided but postgres feature not enabled".into());
        }
    } else {
        info!("  Using SQLite");
        Arc::new(SqliteMetadataStore::new(&metadata_store_url).await?)
    };

    info!("✓ Metadata store connected");

    // Create producer
    info!("Initializing producer...");
    let producer = Producer::builder()
        .metadata_store(metadata.clone())
        .build()
        .await?;

    info!("✓ Producer initialized");

    // Setup object store for consumer
    info!("Connecting to object store...");
    let s3_bucket =
        std::env::var("STREAMHOUSE_BUCKET").unwrap_or_else(|_| "streamhouse-data".to_string());

    let object_store: Arc<dyn object_store::ObjectStore> = Arc::new(
        object_store::aws::AmazonS3Builder::from_env()
            .with_bucket_name(&s3_bucket)
            .with_allow_http(true) // Allow HTTP for MinIO
            .build()?,
    );

    info!("✓ Object store connected (bucket: {})", s3_bucket);

    // Setup segment cache
    let cache_dir =
        std::env::var("STREAMHOUSE_CACHE").unwrap_or_else(|_| "./data/cache".to_string());
    let cache_size = std::env::var("STREAMHOUSE_CACHE_SIZE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(100 * 1024 * 1024); // 100MB default

    let segment_cache = Arc::new(streamhouse_storage::SegmentCache::new(
        &cache_dir, cache_size,
    )?);

    info!("✓ Segment cache initialized ({})", cache_dir);

    // Create Prometheus client for real metrics (optional)
    let prometheus_client = std::env::var("PROMETHEUS_URL").ok().map(|url| {
        info!("📊 Prometheus metrics enabled: {}", url);
        Arc::new(streamhouse_api::PrometheusClient::new(&url))
    });

    // Create app state
    // Auth is disabled by default in development; enable with STREAMHOUSE_AUTH_ENABLED=true
    let auth_enabled = std::env::var("STREAMHOUSE_AUTH_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let state = AppState {
        metadata,
        producer: Some(Arc::new(producer)),
        writer_pool: None,
        object_store,
        segment_cache,
        prometheus: prometheus_client,
        auth_config: streamhouse_api::AuthConfig {
            enabled: auth_enabled,
            ..Default::default()
        },
        topic_changed: None,
    };

    // Create router
    let router = create_router(state);

    info!("");
    info!("REST API Server Ready");
    info!("  API: http://localhost:{}/api/v1", api_port);
    info!("  Swagger UI: http://localhost:{}/swagger-ui", api_port);
    info!("  Health: http://localhost:{}/health", api_port);
    info!("");

    // Start server
    serve(router, api_port).await?;

    Ok(())
}
