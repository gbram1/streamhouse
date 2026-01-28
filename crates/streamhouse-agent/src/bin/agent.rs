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
use streamhouse_agent::Agent;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};
use streamhouse_storage::{SegmentCache, WriteConfig, WriterPool};
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
    let write_config = WriteConfig {
        segment_max_size: 100 * 1024 * 1024, // 100MB
        segment_max_age_ms: 10 * 60 * 1000,  // 10 minutes
        s3_bucket: s3_bucket.clone(),
        s3_region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
        s3_endpoint: std::env::var("S3_ENDPOINT").ok(),
        block_size_target: 1024 * 1024, // 1MB
        s3_upload_retries: 3,
    };

    let _writer_pool = Arc::new(WriterPool::new(
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
    info!("");
    info!("Agent {} is now running", agent_id);
    info!("Listening on: {}", agent_address);
    info!("");

    // Setup graceful shutdown
    let agent_clone = Arc::clone(&agent);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal, stopping agent...");
        if let Err(e) = agent_clone.stop().await {
            error!("Error during shutdown: {}", e);
        }
        std::process::exit(0);
    });

    // Keep agent running
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
