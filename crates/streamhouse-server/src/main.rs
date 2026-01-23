//! StreamHouse gRPC Server
//!
//! Main entry point for the StreamHouse streaming platform server.
//!
//! ## Overview
//! This server provides a gRPC API for producing and consuming records,
//! managing topics and partitions, and coordinating consumer groups.
//! It implements a Kafka-like streaming platform with S3-native storage.
//!
//! ## Architecture
//! The server integrates three main components:
//! - **Metadata Store**: SQLite database for topics, partitions, segments, and offsets
//! - **Object Store**: S3 (or local filesystem) for durable segment storage
//! - **Segment Cache**: LRU cache for frequently accessed segments
//!
//! ## Configuration
//! All configuration is done via environment variables:
//!
//! ### Server Settings
//! - `STREAMHOUSE_ADDR`: Server bind address (default: 0.0.0.0:9090)
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
//! ## gRPC API
//! The server exposes 9 RPC methods:
//! - `CreateTopic`: Create a new topic with specified partitions
//! - `GetTopic`: Get information about a topic
//! - `ListTopics`: List all topics
//! - `DeleteTopic`: Delete a topic and all its data
//! - `Produce`: Produce a single record to a partition
//! - `ProduceBatch`: Produce multiple records atomically
//! - `Consume`: Consume records from a partition (streaming)
//! - `CommitOffset`: Commit consumer group offset
//! - `GetOffset`: Get committed offset for a consumer group
//!
//! ## gRPC Reflection
//! The server includes gRPC reflection support, which allows tools like
//! `grpcurl` to discover and call services without needing proto files:
//! ```bash
//! grpcurl -plaintext localhost:9090 list
//! grpcurl -plaintext localhost:9090 streamhouse.StreamHouse/ListTopics
//! ```
//!
//! ## Example Usage
//! ```bash
//! # Start with local storage (development)
//! export USE_LOCAL_STORAGE=1
//! cargo run -p streamhouse-server
//!
//! # Start with S3 (production)
//! export STREAMHOUSE_BUCKET=my-bucket
//! export AWS_REGION=us-west-2
//! cargo run -p streamhouse-server --release
//! ```
//!
//! ## Logging
//! Logging is controlled via the `RUST_LOG` environment variable:
//! ```bash
//! RUST_LOG=debug cargo run -p streamhouse-server    # Detailed logs
//! RUST_LOG=info cargo run -p streamhouse-server     # Standard logs (default)
//! RUST_LOG=warn cargo run -p streamhouse-server     # Warnings only
//! ```

use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::SqliteMetadataStore;
use streamhouse_server::{pb::stream_house_server::StreamHouseServer, StreamHouseService};
use streamhouse_storage::{SegmentCache, WriteConfig, WriterPool};
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;

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

    let _s3_prefix = std::env::var("STREAMHOUSE_PREFIX").unwrap_or_else(|_| "data".to_string());

    // Initialize metadata store
    tracing::info!("Initializing metadata store at {}", metadata_path);
    let metadata = Arc::new(SqliteMetadataStore::new(&metadata_path).await?);

    // Initialize object store (S3)
    tracing::info!("Initializing object store (bucket: {})", s3_bucket);
    let object_store: Arc<dyn object_store::ObjectStore> =
        if std::env::var("USE_LOCAL_STORAGE").is_ok() {
            // Use local filesystem for development
            let local_path = std::env::var("LOCAL_STORAGE_PATH")
                .unwrap_or_else(|_| "./data/storage".to_string());
            tracing::info!("Using local storage at {}", local_path);
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
        "Initializing cache at {} (max size: {} bytes)",
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
    };

    // Create writer pool
    tracing::info!("Initializing writer pool");
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
        "Starting background flush thread (interval: {}s)",
        flush_interval_secs
    );
    let _flush_handle = writer_pool
        .clone()
        .start_background_flush(Duration::from_secs(flush_interval_secs));

    // Create service
    let service =
        StreamHouseService::new(metadata, object_store, cache, writer_pool.clone(), config);

    tracing::info!("StreamHouse server starting on {}", bind_addr);
    tracing::info!("Configuration:");
    tracing::info!("  Bucket: {}", s3_bucket);
    tracing::info!("  Cache: {} ({} bytes)", cache_dir, cache_size);
    tracing::info!("  Flush interval: {}s", flush_interval_secs);

    // Set up reflection service
    let descriptor_bytes = include_bytes!("../proto/streamhouse_descriptor.bin");
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(descriptor_bytes)
        .build()?;

    // Set up graceful shutdown
    let shutdown_pool = writer_pool.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

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
    });

    // Start server with reflection and graceful shutdown
    Server::builder()
        .add_service(StreamHouseServer::new(service))
        .add_service(reflection_service)
        .serve_with_shutdown(bind_addr, async {
            shutdown_rx.await.ok();
        })
        .await?;

    tracing::info!("StreamHouse server shut down gracefully");

    Ok(())
}
