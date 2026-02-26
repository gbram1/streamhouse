//! StreamHouse Unified Server
//!
//! Single server binary that provides:
//! - gRPC API (port 50051) for producers/consumers
//! - Kafka Protocol (port 9092) for Kafka-compatible clients
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
//! - `KAFKA_ADDR`: Kafka protocol server bind address (default: 0.0.0.0:9092)
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

use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_kafka::{GroupCoordinator, KafkaServer, KafkaServerConfig, KafkaServerState};
#[cfg(feature = "postgres")]
use streamhouse_metadata::PostgresMetadataStore;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};
use streamhouse_schema_registry::{SchemaRegistry, SchemaRegistryApi};
use streamhouse_server::{pb::stream_house_server::StreamHouseServer, StreamHouseService};
use streamhouse_storage::{SegmentCache, SyncPolicy, WALConfig, WriteConfig, WriterPool};
use tonic::transport::Server as GrpcServer;
use tonic_reflection::server::Builder as ReflectionBuilder;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging with JSON support for production (Phase 7.2c)
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    match log_format.to_lowercase().as_str() {
        "json" => {
            // Production JSON format (structured logs for log aggregation)
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .with_current_span(true)
                .with_span_list(true)
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .init();
        }
        _ => {
            // Development text format (human-readable)
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(true)
                .with_thread_ids(false)
                .init();
        }
    }

    // Initialize observability (metrics)
    streamhouse_observability::init();

    tracing::info!("🚀 Starting StreamHouse Unified Server");

    // Configuration
    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_string())
        .parse()?;

    let http_addr: SocketAddr = std::env::var("HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;

    let kafka_addr = std::env::var("KAFKA_ADDR").unwrap_or_else(|_| "0.0.0.0:9092".to_string());

    let web_console_path =
        std::env::var("WEB_CONSOLE_PATH").unwrap_or_else(|_| "./web/out".to_string());

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

    // Initialize metadata store (PostgreSQL in production, SQLite for development)
    let metadata: Arc<dyn MetadataStore> = {
        #[cfg(feature = "postgres")]
        {
            if let Ok(database_url) = std::env::var("DATABASE_URL") {
                tracing::info!("📦 Initializing PostgreSQL metadata store");
                Arc::new(PostgresMetadataStore::new(&database_url).await?)
            } else {
                tracing::info!("📦 Initializing SQLite metadata store at {}", metadata_path);
                Arc::new(SqliteMetadataStore::new(&metadata_path).await?)
            }
        }
        #[cfg(not(feature = "postgres"))]
        {
            tracing::info!("📦 Initializing SQLite metadata store at {}", metadata_path);
            Arc::new(SqliteMetadataStore::new(&metadata_path).await?)
        }
    };

    // Initialize object store (S3)
    tracing::info!("☁️  Initializing object store (bucket: {})", s3_bucket);
    let object_store: Arc<dyn object_store::ObjectStore> = if std::env::var("USE_LOCAL_STORAGE")
        .is_ok()
    {
        // Use local filesystem for development
        let local_path =
            std::env::var("LOCAL_STORAGE_PATH").unwrap_or_else(|_| "./data/storage".to_string());
        tracing::info!("   Using local storage at {}", local_path);
        Arc::new(object_store::local::LocalFileSystem::new_with_prefix(
            local_path,
        )?)
    } else {
        // Use S3 (or MinIO with custom endpoint)
        let mut builder =
            object_store::aws::AmazonS3Builder::from_env().with_bucket_name(&s3_bucket);

        // Set custom endpoint for MinIO
        if let Ok(endpoint) = std::env::var("S3_ENDPOINT") {
            tracing::info!("   Using custom S3 endpoint: {}", endpoint);
            builder = builder.with_endpoint(endpoint).with_allow_http(true); // Allow HTTP for local MinIO
        }

        let s3 = builder.build()?;
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
    // For development: smaller segments (1MB) and shorter age (10 seconds) for faster visibility
    // For production: use larger values (64MB, 10 minutes) for better batching
    let segment_max_size = std::env::var("SEGMENT_MAX_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1024 * 1024); // 1MB default for dev
    let segment_max_age_ms = std::env::var("SEGMENT_MAX_AGE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10 * 1000); // 10 seconds default for dev

    // Generate agent_id early (needed for WAL config)
    let agent_id = format!("unified-{}", std::process::id());

    // WAL configuration (Phase 12.4.5: Shared WAL for Zero-Loss Failover)
    let wal_enabled = std::env::var("WAL_ENABLED")
        .ok()
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(false); // Disabled by default for backward compat

    let wal_config = if wal_enabled {
        let wal_dir = std::env::var("WAL_DIR").unwrap_or_else(|_| "./data/wal".to_string());
        let wal_sync_interval_ms = std::env::var("WAL_SYNC_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(100);
        let wal_max_size = std::env::var("WAL_MAX_SIZE")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1024 * 1024 * 1024); // 1GB

        tracing::info!("WAL enabled:");
        tracing::info!("  Directory: {}", wal_dir);
        tracing::info!("  Sync interval: {}ms", wal_sync_interval_ms);
        tracing::info!("  Max size: {}MB", wal_max_size / (1024 * 1024));
        tracing::info!("  Agent ID: {}", agent_id);

        Some(WALConfig {
            directory: wal_dir.into(),
            sync_policy: SyncPolicy::Interval {
                interval: Duration::from_millis(wal_sync_interval_ms),
            },
            max_size_bytes: wal_max_size,
            batch_enabled: true,
            batch_max_records: 1000,
            batch_max_bytes: 1024 * 1024,
            batch_max_age_ms: 10,
            agent_id: Some(agent_id.clone()),
        })
    } else {
        tracing::info!("WAL disabled (set WAL_ENABLED=true to enable)");
        None
    };

    // Throttle configuration (enabled by default for production safety)
    let throttle_enabled = std::env::var("THROTTLE_ENABLED")
        .ok()
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(true);

    let throttle_config = if throttle_enabled {
        tracing::info!("S3 throttling protection enabled");
        Some(streamhouse_storage::ThrottleConfig::default())
    } else {
        tracing::warn!("S3 throttling protection DISABLED - not recommended for production");
        None
    };

    // Durable flush batching (ACK_DURABLE optimization)
    let durable_batch_max_age_ms = std::env::var("DURABLE_BATCH_MAX_AGE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(200);
    let durable_batch_max_records = std::env::var("DURABLE_BATCH_MAX_RECORDS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10_000);
    let durable_batch_max_bytes = std::env::var("DURABLE_BATCH_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(16 * 1024 * 1024);

    // S3 upload tuning
    let s3_upload_retries = std::env::var("S3_UPLOAD_RETRIES")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(3);
    let multipart_threshold = std::env::var("MULTIPART_THRESHOLD")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(8 * 1024 * 1024);
    let multipart_part_size = std::env::var("MULTIPART_PART_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(8 * 1024 * 1024);
    let parallel_upload_parts = std::env::var("PARALLEL_UPLOAD_PARTS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);

    let config = WriteConfig {
        segment_max_size,
        segment_max_age_ms,
        s3_bucket: s3_bucket.clone(),
        s3_region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
        s3_endpoint: std::env::var("S3_ENDPOINT").ok(),
        block_size_target: 1024 * 1024, // 1MB
        s3_upload_retries,
        multipart_threshold,
        multipart_part_size,
        parallel_upload_parts,
        durable_batch_max_age_ms,
        durable_batch_max_records,
        durable_batch_max_bytes,
        wal_config,
        throttle_config,
    };

    // Log all performance-related configuration
    tracing::info!("⚙️  Write configuration:");
    tracing::info!("  Segment max size: {}MB", segment_max_size / (1024 * 1024));
    tracing::info!("  Segment max age: {}ms", segment_max_age_ms);
    tracing::info!("  S3 upload retries: {}", s3_upload_retries);
    tracing::info!("  Multipart threshold: {}MB", multipart_threshold / (1024 * 1024));
    tracing::info!("  Multipart part size: {}MB", multipart_part_size / (1024 * 1024));
    tracing::info!("  Parallel upload parts: {}", parallel_upload_parts);
    tracing::info!("  Durable batch max age: {}ms", durable_batch_max_age_ms);
    tracing::info!("  Durable batch max records: {}", durable_batch_max_records);
    tracing::info!("  Durable batch max bytes: {}MB", durable_batch_max_bytes / (1024 * 1024));

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

    // Start Kafka protocol server
    tracing::info!("📡 Initializing Kafka protocol server");
    let kafka_config = KafkaServerConfig {
        bind_addr: kafka_addr.clone(),
        node_id: 0,
        advertised_host: std::env::var("KAFKA_ADVERTISED_HOST")
            .unwrap_or_else(|_| "localhost".to_string()),
        advertised_port: std::env::var("KAFKA_ADVERTISED_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9092),
    };

    let kafka_state = Arc::new(KafkaServerState {
        config: kafka_config,
        metadata: metadata.clone(),
        writer_pool: writer_pool.clone(),
        segment_cache: cache.clone(),
        object_store: object_store.clone(),
        group_coordinator: Arc::new(GroupCoordinator::new(metadata.clone())),
    });

    let kafka_server = KafkaServer::new(kafka_state);
    let kafka_addr_display = kafka_addr.clone();
    let (kafka_shutdown_tx, kafka_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        if let Err(e) = kafka_server.run_until(kafka_shutdown_rx).await {
            tracing::error!("Kafka server error: {}", e);
        }
    });
    tracing::info!("   Kafka server started on {}", kafka_addr_display);

    // Create gRPC service (unified: admin + producer + consumer + lifecycle)
    let grpc_service = StreamHouseService::new(
        metadata.clone(),
        object_store.clone(),
        cache.clone(),
        writer_pool.clone(),
        config.clone(),
        agent_id.clone(),
    );

    // Register unified server as an agent
    tracing::info!("🤖 Registering unified server as agent");
    use streamhouse_metadata::AgentInfo;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let agent_info = AgentInfo {
        agent_id: agent_id.clone(),
        address: format!("{}:{}", grpc_addr.ip(), grpc_addr.port()),
        availability_zone: std::env::var("AVAILABILITY_ZONE")
            .unwrap_or_else(|_| "default".to_string()),
        agent_group: "default".to_string(),
        last_heartbeat: now_ms,
        started_at: now_ms,
        metadata: std::collections::HashMap::new(),
    };
    metadata.register_agent(agent_info).await?;
    tracing::info!("   Registered as agent: {}", agent_id);

    // Spawn heartbeat task to keep agent alive
    {
        let metadata = metadata.clone();
        let agent_id = agent_id.clone();
        let address = format!("{}:{}", grpc_addr.ip(), grpc_addr.port());
        let availability_zone =
            std::env::var("AVAILABILITY_ZONE").unwrap_or_else(|_| "default".to_string());

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;

                let agent_info = AgentInfo {
                    agent_id: agent_id.clone(),
                    address: address.clone(),
                    availability_zone: availability_zone.clone(),
                    agent_group: "default".to_string(),
                    last_heartbeat: now_ms,
                    started_at: now_ms, // Will be ignored by update
                    metadata: std::collections::HashMap::new(),
                };

                if let Err(e) = metadata.register_agent(agent_info).await {
                    tracing::warn!("Failed to update agent heartbeat: {}", e);
                }
            }
        });
    }

    // Start partition assignment
    // This allows the agent to acquire leases for partitions and manage them.
    // The assigner wakes up immediately when topics are created/deleted via
    // the topic_changed Notify, so there's no 60-second delay for new topics.
    {
        let metadata_for_assigner = metadata.clone();
        let agent_id_for_assigner = agent_id.clone();
        let topic_notify = grpc_service.topic_change_notify();

        tokio::spawn(async move {
            // Wait a bit for server to fully start and topics to be created
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            loop {
                // Get current list of topics
                let topics = match metadata_for_assigner.list_topics().await {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!("Failed to list topics for partition assignment: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        continue;
                    }
                };

                if topics.is_empty() {
                    tracing::debug!("No topics to manage, waiting for topic creation...");
                    // Wait for either a topic change notification or a fallback timeout
                    tokio::select! {
                        _ = topic_notify.notified() => {
                            tracing::info!("Topic change detected, checking for new topics");
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                    }
                    continue;
                }

                let managed_topics: Vec<String> = topics.iter().map(|t| t.name.clone()).collect();
                tracing::info!(
                    "📋 Managing {} topics for partition assignment: {:?}",
                    managed_topics.len(),
                    managed_topics
                );

                // Create lease manager for this agent (agent_id first, then metadata_store)
                let lease_manager = std::sync::Arc::new(streamhouse_agent::LeaseManager::new(
                    agent_id_for_assigner.clone(),
                    metadata_for_assigner.clone(),
                ));

                // Start lease renewal in background (spawns its own task)
                let lease_manager_for_renewal = lease_manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = lease_manager_for_renewal.start_renewal_task().await {
                        tracing::warn!("Lease renewal task ended with error: {}", e);
                    }
                });

                // Create partition assigner with all 5 parameters:
                // (agent_id, agent_group, metadata_store, lease_manager, topics)
                let assigner = streamhouse_agent::PartitionAssigner::new(
                    agent_id_for_assigner.clone(),
                    "default".to_string(), // agent_group
                    metadata_for_assigner.clone(),
                    lease_manager.clone(),
                    managed_topics.clone(),
                );

                // Start the partition assigner background task
                if let Err(e) = assigner.start().await {
                    tracing::warn!("Failed to start partition assigner: {}", e);
                } else {
                    tracing::info!(
                        "✅ Partition assigner started for {} topics",
                        managed_topics.len()
                    );
                    // Wait for topic changes (instant) or poll as fallback (60s)
                    loop {
                        tokio::select! {
                            _ = topic_notify.notified() => {
                                tracing::info!("Topic change detected, restarting partition assigner");
                            }
                            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                                // Fallback: check periodically in case we missed a notification
                                let current_topics = match metadata_for_assigner.list_topics().await {
                                    Ok(t) => t,
                                    Err(_) => continue,
                                };

                                let current_topic_names: Vec<String> =
                                    current_topics.iter().map(|t| t.name.clone()).collect();

                                if current_topic_names == managed_topics {
                                    continue; // No change, keep looping
                                }

                                tracing::info!("Topic list changed on periodic check");
                            }
                        }

                        // Topic list changed — restart assigner with new topics
                        let _ = assigner.stop().await;
                        break;
                    }
                }
            }
        });
    }
    tracing::info!("🔄 Partition assignment background task started");

    // Start materialized view maintenance task
    // Keep shutdown_tx alive for the server lifetime to prevent immediate shutdown
    use streamhouse_server::{MaintenanceConfig, MaterializedViewMaintenance};

    let maintenance = Arc::new(MaterializedViewMaintenance::new(
        metadata.clone(),
        object_store.clone(),
        cache.clone(),
        MaintenanceConfig::default(),
    ));

    let (_mv_shutdown_tx, mv_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let _mv_handle = maintenance.start(mv_shutdown_rx);

    tracing::info!("🔄 Materialized view maintenance task started");

    // Create Prometheus client for real metrics (optional)
    let prometheus_client = std::env::var("PROMETHEUS_URL").ok().map(|url| {
        tracing::info!("📊 Prometheus metrics enabled: {}", url);
        std::sync::Arc::new(streamhouse_api::PrometheusClient::new(&url))
    });

    // Create REST API state (use WriterPool directly in unified server)
    // Auth is disabled by default in development; enable with --enable-auth flag
    let auth_enabled = std::env::var("STREAMHOUSE_AUTH_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let api_state = streamhouse_api::AppState {
        metadata: metadata.clone(),
        producer: None,
        writer_pool: Some(writer_pool.clone()),
        object_store: object_store.clone(),
        segment_cache: cache.clone(),
        prometheus: prometheus_client,
        auth_config: streamhouse_api::AuthConfig {
            enabled: auth_enabled,
            ..Default::default()
        },
        topic_changed: Some(grpc_service.topic_change_notify()),
    };

    // Create REST API router
    let api_router = streamhouse_api::create_router(api_state);

    // Create Schema Registry
    tracing::info!("📋 Initializing Schema Registry");

    #[cfg(feature = "postgres")]
    let schema_storage: Arc<dyn streamhouse_schema_registry::SchemaStorage> = {
        use streamhouse_schema_registry::PostgresSchemaStorage;
        // Create PostgreSQL pool for schema registry
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set when using postgres feature");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to PostgreSQL for schema registry");
        tracing::info!("   Using PostgreSQL storage backend");
        Arc::new(PostgresSchemaStorage::new(pool))
    };

    #[cfg(not(feature = "postgres"))]
    let schema_storage: Arc<dyn streamhouse_schema_registry::SchemaStorage> = {
        use streamhouse_schema_registry::MemorySchemaStorage;
        tracing::info!("   Using in-memory storage backend");
        Arc::new(MemorySchemaStorage::new(metadata.clone()))
    };

    let schema_registry = Arc::new(SchemaRegistry::new(schema_storage));
    let schema_api = SchemaRegistryApi::new(schema_registry);
    let schema_router = schema_api.router();

    // Create metrics router
    let metrics_router = streamhouse_observability::exporter::create_metrics_router();

    // Build unified HTTP router
    let http_router = Router::new()
        // Merge REST API (already includes /api/v1 and /health routes)
        .merge(api_router)
        // Mount Schema Registry at /schemas
        .nest("/schemas", schema_router)
        // Merge Prometheus metrics endpoint at /metrics
        .merge(metrics_router)
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
        let _ = kafka_shutdown_tx.send(());
    });

    tracing::info!("✅ StreamHouse Unified Server starting");
    tracing::info!("   gRPC:           {}", grpc_addr);
    tracing::info!("   Kafka:          {}", kafka_addr);
    tracing::info!("   REST API:       http://{}/api/v1", http_addr);
    tracing::info!("   Schema Registry: http://{}/schemas", http_addr);
    tracing::info!("   Web Console:    http://{}", http_addr);
    tracing::info!("   Health:         http://{}/health", http_addr);
    tracing::info!("   Metrics:        http://{}/metrics", http_addr);
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

    // gRPC server tuning
    let grpc_max_concurrent_streams = std::env::var("GRPC_MAX_CONCURRENT_STREAMS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    let grpc_keepalive_interval_secs = std::env::var("GRPC_KEEPALIVE_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    let grpc_keepalive_timeout_secs = std::env::var("GRPC_KEEPALIVE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    let grpc_initial_stream_window = std::env::var("GRPC_INITIAL_STREAM_WINDOW_SIZE")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    let grpc_initial_connection_window = std::env::var("GRPC_INITIAL_CONNECTION_WINDOW_SIZE")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());

    // Start gRPC server (blocks until shutdown)
    let mut grpc_builder = GrpcServer::builder();

    if let Some(max_streams) = grpc_max_concurrent_streams {
        tracing::info!("  gRPC max concurrent streams: {}", max_streams);
        grpc_builder = grpc_builder.concurrency_limit_per_connection(max_streams as usize);
    }
    if let Some(interval) = grpc_keepalive_interval_secs {
        tracing::info!("  gRPC keepalive interval: {}s", interval);
        grpc_builder = grpc_builder.http2_keepalive_interval(Some(Duration::from_secs(interval)));
    }
    if let Some(timeout) = grpc_keepalive_timeout_secs {
        tracing::info!("  gRPC keepalive timeout: {}s", timeout);
        grpc_builder = grpc_builder.http2_keepalive_timeout(Some(Duration::from_secs(timeout)));
    }
    if let Some(window) = grpc_initial_stream_window {
        tracing::info!("  gRPC initial stream window: {} bytes", window);
        grpc_builder = grpc_builder.initial_stream_window_size(Some(window));
    }
    if let Some(window) = grpc_initial_connection_window {
        tracing::info!("  gRPC initial connection window: {} bytes", window);
        grpc_builder = grpc_builder.initial_connection_window_size(Some(window));
    }

    let grpc_result = grpc_builder
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
