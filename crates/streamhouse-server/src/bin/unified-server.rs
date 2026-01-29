//! StreamHouse Unified Server
//!
//! Single server binary that provides:
//! - gRPC API (port 50051) for producers/consumers
//! - REST API (port 8080/api/v1/*) for management
//! - Schema Registry (port 8080/schemas/*) for schema management
//! - Web Console (port 8080/*) static file serving
//!
//! ## Architecture
//! This unified server consolidates three previously separate servers:
//! - streamhouse-server (gRPC)
//! - streamhouse-api (REST API)
//! - streamhouse-schema-registry (Schema Registry)
//!
//! All services share the same metadata store, object store, and configuration.
//!
//! ## Configuration
//! All configuration is done via environment variables:
//!
//! ### Server Settings
//! - `GRPC_ADDR`: gRPC server bind address (default: 0.0.0.0:50051)
//! - `HTTP_ADDR`: HTTP server bind address (default: 0.0.0.0:8080)
//! - `WEB_CONSOLE_PATH`: Path to web console static files (default: ./web/out)
//!
//! ### Storage Settings
//! - `STREAMHOUSE_METADATA`: SQLite database path (default: ./data/metadata.db)
//! - `STREAMHOUSE_BUCKET`: S3 bucket name (default: streamhouse)
//! - `STREAMHOUSE_PREFIX`: S3 key prefix (default: data)
//! - `AWS_REGION`: AWS region (default: us-east-1)
//! - `S3_ENDPOINT`: Custom S3 endpoint URL (optional)
//!
//! ### Local Development
//! - `USE_LOCAL_STORAGE`: Use local filesystem instead of S3 (any value)
//! - `LOCAL_STORAGE_PATH`: Path for local storage (default: ./data/storage)
//!
//! ### Cache Settings
//! - `STREAMHOUSE_CACHE`: Cache directory (default: ./data/cache)
//! - `STREAMHOUSE_CACHE_SIZE`: Cache size in bytes (default: 1073741824 = 1GB)
//!
//! ## Example Usage
//! ```bash
//! # Start with local storage (development)
//! export USE_LOCAL_STORAGE=1
//! cargo run -p streamhouse-server --bin unified-server
//!
//! # Access services
//! # gRPC: localhost:50051
//! # REST API: http://localhost:8080/api/v1
//! # Schema Registry: http://localhost:8080/schemas
//! # Web Console: http://localhost:8080
//! ```

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::SqliteMetadataStore;
use streamhouse_server::{pb::stream_house_server::StreamHouseServer, StreamHouseService};
use streamhouse_storage::{SegmentCache, WriteConfig, WriterPool};
use streamhouse_client::Producer;
use streamhouse_schema_registry::{SchemaRegistry, SchemaRegistryApi, MemorySchemaStorage};
use tonic::transport::Server as GrpcServer;
use tonic_reflection::server::Builder as ReflectionBuilder;
use axum::{Router, routing::get};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("🚀 Starting StreamHouse Unified Server");

    // Configuration
    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_string())
        .parse()?;

    let http_addr: SocketAddr = std::env::var("HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;

    let web_console_path = std::env::var("WEB_CONSOLE_PATH")
        .unwrap_or_else(|_| "./web/out".to_string());

    let metadata_path =
        std::env::var("STREAMHOUSE_METADATA").unwrap_or_else(|_| "./data/metadata.db".to_string());

    let cache_dir =
        std::env::var("STREAMHOUSE_CACHE").unwrap_or_else(|_| "./data/cache".to_string());

    let cache_size: u64 = std::env::var("STREAMHOUSE_CACHE_SIZE")
        .unwrap_or_else(|_| "1073741824".to_string()) // 1GB default
        .parse()?;

    let s3_bucket =
        std::env::var("STREAMHOUSE_BUCKET").unwrap_or_else(|_| "streamhouse".to_string());

    let _s3_prefix = std::env::var("STREAMHOUSE_PREFIX").unwrap_or_else(|_| "data".to_string());

    // Initialize metadata store
    tracing::info!("📦 Initializing metadata store at {}", metadata_path);
    let metadata = Arc::new(SqliteMetadataStore::new(&metadata_path).await?);

    // Initialize object store (S3)
    tracing::info!("☁️  Initializing object store (bucket: {})", s3_bucket);
    let object_store: Arc<dyn object_store::ObjectStore> =
        if std::env::var("USE_LOCAL_STORAGE").is_ok() {
            // Use local filesystem for development
            let local_path = std::env::var("LOCAL_STORAGE_PATH")
                .unwrap_or_else(|_| "./data/storage".to_string());
            tracing::info!("   Using local storage at {}", local_path);
            Arc::new(object_store::local::LocalFileSystem::new_with_prefix(
                local_path,
            )?)
        } else {
            // Use S3
            let s3 = object_store::aws::AmazonS3Builder::from_env()
                .with_bucket_name(&s3_bucket)
                .build()?;
            Arc::new(s3)
        };

    // Initialize cache
    tracing::info!(
        "💾 Initializing cache at {} (max size: {} bytes)",
        cache_dir,
        cache_size
    );
    let cache = Arc::new(SegmentCache::new(&cache_dir, cache_size)?);

    // Create storage config
    let config = WriteConfig {
        segment_max_size: 64 * 1024 * 1024, // 64MB
        segment_max_age_ms: 10 * 60 * 1000, // 10 minutes
        s3_bucket: s3_bucket.clone(),
        s3_region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
        s3_endpoint: std::env::var("S3_ENDPOINT").ok(),
        block_size_target: 1024 * 1024, // 1MB
        s3_upload_retries: 3,
        wal_config: None, // WAL disabled by default
    };

    // Create writer pool
    tracing::info!("✍️  Initializing writer pool");
    let writer_pool = Arc::new(WriterPool::new(
        metadata.clone(),
        object_store.clone(),
        config.clone(),
    ));

    // Start background flush thread
    let flush_interval_secs = std::env::var("FLUSH_INTERVAL_SECS")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<u64>()
        .unwrap_or(5);
    tracing::info!(
        "🔄 Starting background flush thread (interval: {}s)",
        flush_interval_secs
    );
    let _flush_handle = writer_pool
        .clone()
        .start_background_flush(Duration::from_secs(flush_interval_secs));

    // Create gRPC service
    let grpc_service =
        StreamHouseService::new(metadata.clone(), object_store.clone(), cache.clone(), writer_pool.clone(), config.clone());

    // Create Producer for REST API
    tracing::info!("🔌 Initializing Producer for REST API");
    let producer = Arc::new(
        Producer::builder()
            .metadata_store(metadata.clone())
            .build()
            .await?,
    );

    // Create REST API state
    let api_state = streamhouse_api::AppState {
        metadata: metadata.clone(),
        producer: producer.clone(),
        object_store: object_store.clone(),
        segment_cache: cache.clone(),
    };

    // Create REST API router
    let api_router = streamhouse_api::create_router(api_state);

    // Create Schema Registry
    tracing::info!("📋 Initializing Schema Registry");
    let schema_storage = Arc::new(MemorySchemaStorage::new(metadata.clone()));
    let schema_registry = Arc::new(SchemaRegistry::new(schema_storage));
    let schema_api = SchemaRegistryApi::new(schema_registry);
    let schema_router = schema_api.router();

    // Build unified HTTP router
    let http_router = Router::new()
        // Mount REST API at /api/v1
        .nest("/api", api_router)
        // Mount Schema Registry at /schemas
        .nest("/schemas", schema_router)
        // Health endpoint
        .route("/health", get(|| async { "OK" }))
        // Serve web console static files
        .fallback_service(ServeDir::new(&web_console_path))
        .layer(TraceLayer::new_for_http());

    // Set up gRPC reflection service
    let descriptor_bytes = include_bytes!("../../proto/streamhouse_descriptor.bin");
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(descriptor_bytes)
        .build()?;

    // Set up graceful shutdown
    let shutdown_pool = writer_pool.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (http_shutdown_tx, http_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn shutdown signal handler
    tokio::spawn(async move {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                tracing::info!("Received SIGINT (Ctrl+C), initiating graceful shutdown");
            },
            _ = terminate => {
                tracing::info!("Received SIGTERM, initiating graceful shutdown");
            },
        }

        // Flush all writers before shutdown
        tracing::info!("Flushing all pending writes...");
        if let Err(e) = shutdown_pool.shutdown().await {
            tracing::error!("Error during writer pool shutdown: {}", e);
        }

        let _ = shutdown_tx.send(());
        let _ = http_shutdown_tx.send(());
    });

    tracing::info!("✅ StreamHouse Unified Server starting");
    tracing::info!("   gRPC:           {}", grpc_addr);
    tracing::info!("   REST API:       http://{}/api/v1", http_addr);
    tracing::info!("   Schema Registry: http://{}/schemas", http_addr);
    tracing::info!("   Web Console:    http://{}", http_addr);
    tracing::info!("   Health:         http://{}/health", http_addr);
    tracing::info!("");
    tracing::info!("Configuration:");
    tracing::info!("   Bucket:         {}", s3_bucket);
    tracing::info!("   Cache:          {} ({} bytes)", cache_dir, cache_size);
    tracing::info!("   Flush interval: {}s", flush_interval_secs);
    tracing::info!("   Web console:    {}", web_console_path);

    // Start HTTP server in background
    let http_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&http_addr)
            .await
            .expect("Failed to bind HTTP server");

        axum::serve(listener, http_router)
            .with_graceful_shutdown(async {
                http_shutdown_rx.await.ok();
            })
            .await
            .expect("HTTP server error");
    });

    // Start gRPC server (blocks until shutdown)
    let grpc_result = GrpcServer::builder()
        .add_service(StreamHouseServer::new(grpc_service))
        .add_service(reflection_service)
        .serve_with_shutdown(grpc_addr, async {
            shutdown_rx.await.ok();
        })
        .await;

    // Wait for HTTP server to finish
    http_handle.await?;

    if let Err(e) = grpc_result {
        tracing::error!("gRPC server error: {}", e);
        return Err(e.into());
    }

    tracing::info!("👋 StreamHouse Unified Server shut down gracefully");

    Ok(())
}
