//! StreamHouse Agent Binary
//!
//! Standalone agent process for distributed StreamHouse deployments.
//!
//! # Environment Variables
//!
//! - `AGENT_ID`: Unique agent identifier (default: hostname-based)
//! - `AGENT_ADDRESS`: gRPC address (default: 0.0.0.0:9090)
//! - `AGENT_ZONE`: Availability zone (default: "default")
//! - `AGENT_GROUP`: Agent group (default: "default")
//! - `METADATA_STORE`: SQLite path or PostgreSQL URL (required)
//! - `MANAGED_TOPICS`: Comma-separated list of topics to manage (optional)
//! - `HEARTBEAT_INTERVAL`: Seconds between heartbeats (default: 20)
//! - `S3_ENDPOINT`: MinIO/S3 endpoint (optional)
//! - `STREAMHOUSE_BUCKET`: S3 bucket name (default: streamhouse-data)
//! - `AWS_REGION`: AWS region (default: us-east-1)
//! - `AWS_ACCESS_KEY_ID`: S3 access key
//! - `AWS_SECRET_ACCESS_KEY`: S3 secret key
//! - `AWS_ENDPOINT_URL`: S3 endpoint URL for MinIO
//! - `SEGMENT_MAX_SIZE`: Max segment size in bytes before flush (default: 100MB)
//! - `SEGMENT_MAX_AGE_MS`: Max segment age in ms before flush (default: 10 minutes)
//! - `WAL_ENABLED`: Enable Write-Ahead Log for durability (default: false)
//! - `WAL_DIR`: Directory for WAL files (default: ./data/wal)
//! - `WAL_SYNC_INTERVAL_MS`: Fsync interval in milliseconds (default: 100ms)
//! - `WAL_MAX_SIZE`: Max WAL file size in bytes (default: 1GB)
//!
//! # Example
//!
//! ```bash
//! export AGENT_ID=agent-001
//! export AGENT_ADDRESS=0.0.0.0:9090
//! export METADATA_STORE=./data/metadata.db
//! export AWS_ENDPOINT_URL=http://localhost:9000
//! export AWS_ACCESS_KEY_ID=minioadmin
//! export AWS_SECRET_ACCESS_KEY=minioadmin
//! cargo run --bin agent
//! ```

use object_store::aws::AmazonS3Builder;
use object_store::ObjectStore;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::{Agent, ProducerServiceImpl};
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};
use streamhouse_proto::producer::producer_service_server::ProducerServiceServer;
use streamhouse_storage::{SegmentCache, SyncPolicy, WALConfig, WriteConfig, WriterPool};
use tonic::transport::Server;
use tracing::{error, info, Level};
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

    info!("🚀 StreamHouse Agent starting...");

    // Load configuration from environment
    let agent_id = std::env::var("AGENT_ID").unwrap_or_else(|_| {
        hostname::get()
            .ok()
            .and_then(|h| h.to_str().map(|s| format!("agent-{}", s)))
            .unwrap_or_else(|| format!("agent-{}", uuid::Uuid::new_v4()))
    });

    let agent_address =
        std::env::var("AGENT_ADDRESS").unwrap_or_else(|_| "0.0.0.0:9090".to_string());
    let availability_zone = std::env::var("AGENT_ZONE").unwrap_or_else(|_| "default".to_string());
    let agent_group = std::env::var("AGENT_GROUP").unwrap_or_else(|_| "default".to_string());

    let heartbeat_interval = std::env::var("HEARTBEAT_INTERVAL")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(20));

    let managed_topics: Vec<String> = std::env::var("MANAGED_TOPICS")
        .ok()
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    info!("Configuration:");
    info!("  Agent ID: {}", agent_id);
    info!("  Address: {}", agent_address);
    info!("  Zone: {}", availability_zone);
    info!("  Group: {}", agent_group);
    info!("  Heartbeat interval: {:?}", heartbeat_interval);
    info!("  Managed topics: {:?}", managed_topics);

    // Connect to metadata store
    info!("Connecting to metadata store...");
    let metadata_store_url =
        std::env::var("METADATA_STORE").expect("METADATA_STORE environment variable required");

    let metadata: Arc<dyn MetadataStore> = if metadata_store_url.starts_with("postgresql://")
        || metadata_store_url.starts_with("postgres://")
    {
        #[cfg(feature = "postgres")]
        {
            info!("  Using PostgreSQL: {}", metadata_store_url);
            Arc::new(streamhouse_metadata::PostgresMetadataStore::new(&metadata_store_url).await?)
        }
        #[cfg(not(feature = "postgres"))]
        {
            error!("PostgreSQL URL provided but postgres feature not enabled");
            error!("Rebuild with: cargo build --features postgres");
            return Err("postgres feature not enabled".into());
        }
    } else {
        info!("  Using SQLite: {}", metadata_store_url);
        Arc::new(SqliteMetadataStore::new(&metadata_store_url).await?)
    };

    info!("✓ Metadata store connected");

    // Setup object store (MinIO or S3)
    info!("Connecting to object store...");
    let s3_bucket =
        std::env::var("STREAMHOUSE_BUCKET").unwrap_or_else(|_| "streamhouse-data".to_string());

    let object_store: Arc<dyn ObjectStore> = Arc::new(
        AmazonS3Builder::from_env()
            .with_bucket_name(&s3_bucket)
            .with_allow_http(true) // Allow HTTP for MinIO/local development
            .build()?,
    );

    info!("✓ Object store connected (bucket: {})", s3_bucket);

    // Setup segment cache
    let cache_dir =
        std::env::var("STREAMHOUSE_CACHE").unwrap_or_else(|_| "./data/cache".to_string());
    let cache_size = std::env::var("STREAMHOUSE_CACHE_SIZE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1024 * 1024 * 1024); // 1GB default

    let _cache = Arc::new(SegmentCache::new(&cache_dir, cache_size)?);
    info!("✓ Segment cache initialized ({})", cache_dir);

    // Create writer pool
    // Configurable segment settings for development/testing vs production
    let segment_max_size = std::env::var("SEGMENT_MAX_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100 * 1024 * 1024); // Default: 100MB

    let segment_max_age_ms = std::env::var("SEGMENT_MAX_AGE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10 * 60 * 1000); // Default: 10 minutes

    info!(
        "Segment settings: max_size={}MB, max_age={}s",
        segment_max_size / (1024 * 1024),
        segment_max_age_ms / 1000
    );

    // WAL configuration
    let wal_enabled = std::env::var("WAL_ENABLED")
        .ok()
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(false);

    let wal_config = if wal_enabled {
        let wal_dir = std::env::var("WAL_DIR")
            .unwrap_or_else(|_| "./data/wal".to_string());
        let wal_sync_interval_ms = std::env::var("WAL_SYNC_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(100);
        let wal_max_size = std::env::var("WAL_MAX_SIZE")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1024 * 1024 * 1024); // 1GB

        info!("WAL enabled:");
        info!("  Directory: {}", wal_dir);
        info!("  Sync interval: {}ms", wal_sync_interval_ms);
        info!("  Max size: {}MB", wal_max_size / (1024 * 1024));

        Some(WALConfig {
            directory: wal_dir.into(),
            sync_policy: SyncPolicy::Interval {
                interval: Duration::from_millis(wal_sync_interval_ms),
            },
            max_size_bytes: wal_max_size,
        })
    } else {
        info!("WAL disabled (set WAL_ENABLED=true to enable)");
        None
    };

    let write_config = WriteConfig {
        segment_max_size,
        segment_max_age_ms,
        s3_bucket: s3_bucket.clone(),
        s3_region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
        s3_endpoint: std::env::var("S3_ENDPOINT").ok(),
        block_size_target: 1024 * 1024, // 1MB
        s3_upload_retries: 3,
        wal_config,
    };

    let writer_pool = Arc::new(WriterPool::new(
        metadata.clone(),
        object_store.clone(),
        write_config,
    ));

    info!("✓ Writer pool initialized");

    // Build and start agent
    info!("Starting agent...");
    let mut agent_builder = Agent::builder()
        .agent_id(&agent_id)
        .address(&agent_address)
        .availability_zone(&availability_zone)
        .agent_group(&agent_group)
        .heartbeat_interval(heartbeat_interval)
        .metadata_store(metadata.clone());

    if !managed_topics.is_empty() {
        agent_builder = agent_builder.managed_topics(managed_topics.clone());
    }

    let agent = Arc::new(agent_builder.build().await?);

    agent.start().await?;
    info!("✓ Agent started successfully");

    // Start gRPC server
    info!("Starting gRPC server...");
    let grpc_addr = agent_address.parse()?;

    #[cfg(feature = "metrics")]
    let grpc_service = ProducerServiceImpl::new(
        writer_pool.clone(),
        metadata.clone(),
        agent_id.clone(),
        None, // Metrics will be added separately
    );

    #[cfg(not(feature = "metrics"))]
    let grpc_service =
        ProducerServiceImpl::new(writer_pool.clone(), metadata.clone(), agent_id.clone());

    let grpc_server = ProducerServiceServer::new(grpc_service);

    let agent_for_shutdown = Arc::clone(&agent);
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = Server::builder()
            .add_service(grpc_server)
            .serve(grpc_addr)
            .await
        {
            error!("gRPC server error: {}", e);
        }
    });

    info!("✓ gRPC server started on {}", agent_address);

    // Start metrics server if enabled
    #[cfg(feature = "metrics")]
    {
        let metrics_port = std::env::var("METRICS_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);

        let metrics_addr = format!("0.0.0.0:{}", metrics_port);
        info!("Starting metrics server on {}...", metrics_addr);

        let registry = Arc::new(prometheus_client::registry::Registry::default());

        // Create a function to check if agent has active leases
        let has_active_leases = Arc::new(move || {
            // For now, always return true if agent is started
            // TODO: Add Agent::has_active_leases() method
            true
        });

        let metrics_server = streamhouse_agent::MetricsServer::new(
            metrics_addr.parse()?,
            registry,
            has_active_leases,
        );

        tokio::spawn(async move {
            if let Err(e) = metrics_server.start().await {
                error!("Metrics server error: {}", e);
            }
        });

        info!("✓ Metrics server started on {}", metrics_addr);
    }

    info!("");
    info!("Agent {} is now running", agent_id);
    info!("  gRPC:    {}", agent_address);
    #[cfg(feature = "metrics")]
    {
        let metrics_port = std::env::var("METRICS_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);
        info!("  Metrics: http://0.0.0.0:{}", metrics_port);
    }
    info!("");

    // Setup graceful shutdown
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal, stopping agent...");
        if let Err(e) = agent_for_shutdown.stop().await {
            error!("Error during shutdown: {}", e);
        }
        std::process::exit(0);
    });

    // Wait for gRPC server to complete (or Ctrl+C)
    grpc_handle.await?;

    Ok(())
}
