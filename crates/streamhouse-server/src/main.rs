//! StreamHouse gRPC Server
//!
//! Starts the gRPC server on port 9090 by default.

use std::sync::Arc;
use streamhouse_metadata::SqliteMetadataStore;
use streamhouse_server::{pb::stream_house_server::StreamHouseServer, StreamHouseService};
use streamhouse_storage::{SegmentCache, WriteConfig};
use tonic::transport::Server;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Configuration
    let bind_addr = std::env::var("STREAMHOUSE_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:9090".to_string())
        .parse()?;

    let metadata_path =
        std::env::var("STREAMHOUSE_METADATA").unwrap_or_else(|_| "./data/metadata.db".to_string());

    let cache_dir =
        std::env::var("STREAMHOUSE_CACHE").unwrap_or_else(|_| "./data/cache".to_string());

    let cache_size: u64 = std::env::var("STREAMHOUSE_CACHE_SIZE")
        .unwrap_or_else(|_| "1073741824".to_string()) // 1GB default
        .parse()?;

    let s3_bucket =
        std::env::var("STREAMHOUSE_BUCKET").unwrap_or_else(|_| "streamhouse".to_string());

    let _s3_prefix =
        std::env::var("STREAMHOUSE_PREFIX").unwrap_or_else(|_| "data".to_string());

    // Initialize metadata store
    tracing::info!("Initializing metadata store at {}", metadata_path);
    let metadata = Arc::new(SqliteMetadataStore::new(&metadata_path).await?);

    // Initialize object store (S3)
    tracing::info!("Initializing object store (bucket: {})", s3_bucket);
    let object_store: Arc<dyn object_store::ObjectStore> = if std::env::var("USE_LOCAL_STORAGE").is_ok() {
        // Use local filesystem for development
        let local_path = std::env::var("LOCAL_STORAGE_PATH")
            .unwrap_or_else(|_| "./data/storage".to_string());
        tracing::info!("Using local storage at {}", local_path);
        Arc::new(object_store::local::LocalFileSystem::new_with_prefix(local_path)?)
    } else {
        // Use S3
        let s3 = object_store::aws::AmazonS3Builder::from_env()
            .with_bucket_name(&s3_bucket)
            .build()?;
        Arc::new(s3)
    };

    // Initialize cache
    tracing::info!("Initializing cache at {} (max size: {} bytes)", cache_dir, cache_size);
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
    };

    // Create service
    let service = StreamHouseService::new(metadata, object_store, cache, config);

    tracing::info!("StreamHouse server starting on {}", bind_addr);
    tracing::info!("Configuration:");
    tracing::info!("  Bucket: {}", s3_bucket);
    tracing::info!("  Cache: {} ({} bytes)", cache_dir, cache_size);

    // Start server
    Server::builder()
        .add_service(StreamHouseServer::new(service))
        .serve(bind_addr)
        .await?;

    Ok(())
}
