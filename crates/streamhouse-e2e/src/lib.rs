//! StreamHouse End-to-End Test Harness
//!
//! Provides [`TestCluster`] which spins up a complete StreamHouse server with
//! all three protocols (REST, gRPC, Kafka) on random ports, backed by in-memory
//! or temp-dir storage. Tests can use the included [`RestClient`] or obtain a
//! raw gRPC channel.
//!
//! # Usage
//!
//! ```rust,ignore
//! use streamhouse_e2e::TestCluster;
//!
//! #[tokio::test]
//! async fn test_produce_consume() {
//!     let cluster = TestCluster::start().await.unwrap();
//!     let client = cluster.rest_client();
//!
//!     // Create a topic
//!     let topic = client.create_topic("events", 2).await.unwrap();
//!     assert_eq!(topic.name, "events");
//!
//!     // Produce & consume through REST
//!     let resp = client.produce("events", Some("k1"), "v1").await.unwrap();
//!     assert_eq!(resp.offset, 0);
//! }
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use streamhouse_api::auth::AuthConfig;
use streamhouse_api::{create_router, AppState};
use streamhouse_kafka::{
    GroupCoordinator, KafkaServerConfig, KafkaServerState, KafkaTenantResolver,
};
use streamhouse_metadata::{AgentInfo, MetadataStore, SqliteMetadataStore};
use streamhouse_schema_registry::{MemorySchemaStorage, SchemaRegistry, SchemaRegistryApi};
use streamhouse_server::{pb::stream_house_server::StreamHouseServer, StreamHouseService};
use streamhouse_client::AgentRouter;
use streamhouse_storage::{SegmentCache, WriteConfig, WriterPool};
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use tonic::transport::{Channel, Server as GrpcServer};

const TEST_AGENT_ID: &str = "e2e-test-agent";

// ---------------------------------------------------------------------------
// TestCluster
// ---------------------------------------------------------------------------

/// A complete StreamHouse server running on random ports for E2E testing.
///
/// All three protocols are available:
/// - **REST** (HTTP) at [`http_addr`](TestCluster::http_addr)
/// - **gRPC** at [`grpc_addr`](TestCluster::grpc_addr)
/// - **Kafka** at [`kafka_addr`](TestCluster::kafka_addr)
///
/// The cluster is torn down when the value is dropped (servers are cancelled).
pub struct TestCluster {
    /// REST / HTTP address
    pub http_addr: SocketAddr,
    /// gRPC address
    pub grpc_addr: SocketAddr,
    /// Kafka protocol address
    pub kafka_addr: SocketAddr,
    /// Convenience URL for REST, e.g. `http://127.0.0.1:PORT`
    pub http_url: String,
    /// Shared metadata store (useful for direct test setup)
    pub metadata: Arc<dyn MetadataStore>,

    // Handles kept alive so background tasks keep running until drop
    _shutdown_tx: tokio::sync::watch::Sender<bool>,
    _http_handle: tokio::task::JoinHandle<()>,
    _grpc_handle: tokio::task::JoinHandle<()>,
    _kafka_handle: tokio::task::JoinHandle<()>,
    _temp_dir: tempfile::TempDir,
    // Container handles — kept alive so containers stay running until drop
    _postgres_container: Option<ContainerAsync<testcontainers_modules::postgres::Postgres>>,
    _minio_container: Option<ContainerAsync<testcontainers_modules::minio::MinIO>>,
}

impl TestCluster {
    /// Spin up a full StreamHouse server cluster for testing.
    ///
    /// All servers bind to `127.0.0.1:0` for random port assignment.
    /// Storage uses a temporary directory that is cleaned up on drop.
    pub async fn start() -> Result<Self> {
        Self::builder().start().await
    }

    /// Returns a [`TestClusterBuilder`] for customizing the cluster before start.
    pub fn builder() -> TestClusterBuilder {
        TestClusterBuilder::default()
    }

    /// Build a full REST URL, e.g. `cluster.api_url("/api/v1/topics")`.
    pub fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.http_url, path)
    }

    /// Create a [`RestClient`] pre-configured to talk to this cluster.
    pub fn rest_client(&self) -> RestClient {
        RestClient::new(self.http_url.clone())
    }

    /// Create a tonic gRPC channel connected to this cluster.
    pub async fn grpc_channel(&self) -> Result<Channel> {
        let endpoint = format!("http://{}", self.grpc_addr);
        let channel = Channel::from_shared(endpoint)?
            .connect()
            .await
            .context("connecting to gRPC server")?;
        Ok(channel)
    }

    /// Create a typed gRPC client connected to this cluster.
    pub async fn grpc_client(
        &self,
    ) -> Result<streamhouse_proto::streamhouse::stream_house_client::StreamHouseClient<Channel>>
    {
        let channel = self.grpc_channel().await?;
        Ok(streamhouse_proto::streamhouse::stream_house_client::StreamHouseClient::new(channel))
    }
}

// ---------------------------------------------------------------------------
// TestClusterBuilder
// ---------------------------------------------------------------------------

/// Builder for [`TestCluster`] with optional configuration overrides.
#[derive(Default)]
pub struct TestClusterBuilder {
    auth_enabled: bool,
    use_postgres: bool,
    use_minio: bool,
}

impl TestClusterBuilder {
    /// Enable REST API authentication.
    pub fn with_auth(mut self, enabled: bool) -> Self {
        self.auth_enabled = enabled;
        self
    }

    /// Use a real PostgreSQL container (via testcontainers) instead of in-memory SQLite.
    pub fn with_postgres(mut self) -> Self {
        self.use_postgres = true;
        self
    }

    /// Use a real MinIO container (via testcontainers) instead of local filesystem.
    pub fn with_minio(mut self) -> Self {
        self.use_minio = true;
        self
    }

    /// Build and start the cluster.
    pub async fn start(self) -> Result<TestCluster> {
        // Shutdown signal shared across all spawned servers
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // ── Storage layer ──────────────────────────────────────────────────
        let temp_dir = tempfile::tempdir().context("creating temp dir")?;
        let data_dir = temp_dir.path();

        let storage_dir = data_dir.join("storage");
        let cache_dir = data_dir.join("cache");
        std::fs::create_dir_all(&storage_dir)?;
        std::fs::create_dir_all(&cache_dir)?;

        // ── Metadata store (SQLite or Postgres) ────────────────────────────
        #[allow(unused_mut)]
        let mut postgres_container = None;
        let metadata: Arc<dyn MetadataStore> = if self.use_postgres {
            #[cfg(feature = "postgres")]
            {
                use testcontainers_modules::postgres::Postgres;
                let container = Postgres::default()
                    .start()
                    .await
                    .context("starting postgres container")?;
                let host_port = container
                    .get_host_port_ipv4(5432)
                    .await
                    .context("getting postgres port")?;
                let url = format!(
                    "postgres://postgres:postgres@127.0.0.1:{}/postgres",
                    host_port
                );
                tracing::info!("Postgres testcontainer started on port {}", host_port);
                let store = streamhouse_metadata::PostgresMetadataStore::new(&url)
                    .await
                    .context("connecting to postgres testcontainer")?;
                postgres_container = Some(container);
                Arc::new(store)
            }
            #[cfg(not(feature = "postgres"))]
            {
                anyhow::bail!("with_postgres() requires the `postgres` feature to be enabled");
            }
        } else {
            Arc::new(
                SqliteMetadataStore::new_in_memory()
                    .await
                    .context("creating in-memory metadata store")?,
            )
        };

        // ── Object store (local filesystem or MinIO) ───────────────────────
        let mut minio_container = None;
        let (object_store, s3_endpoint): (Arc<dyn object_store::ObjectStore>, Option<String>) =
            if self.use_minio {
                use testcontainers_modules::minio::MinIO;
                let container = MinIO::default()
                    .start()
                    .await
                    .context("starting minio container")?;
                let host_port = container
                    .get_host_port_ipv4(9000)
                    .await
                    .context("getting minio port")?;
                let endpoint = format!("http://127.0.0.1:{}", host_port);
                tracing::info!("MinIO testcontainer started on port {}", host_port);

                // Create the bucket using aws-sdk-s3 (properly signed)
                let s3_creds = aws_sdk_s3::config::Credentials::new(
                    "minioadmin",
                    "minioadmin",
                    None,
                    None,
                    "test",
                );
                let s3_config = aws_sdk_s3::Config::builder()
                    .region(aws_sdk_s3::config::Region::new("us-east-1"))
                    .endpoint_url(&endpoint)
                    .credentials_provider(s3_creds)
                    .force_path_style(true)
                    .behavior_version_latest()
                    .build();
                let s3_client = aws_sdk_s3::Client::from_conf(s3_config);
                let _ = s3_client.create_bucket().bucket("test-bucket").send().await;

                let store = object_store::aws::AmazonS3Builder::new()
                    .with_bucket_name("test-bucket")
                    .with_endpoint(&endpoint)
                    .with_region("us-east-1")
                    .with_access_key_id("minioadmin")
                    .with_secret_access_key("minioadmin")
                    .with_allow_http(true)
                    .build()
                    .context("creating S3 object store for minio")?;
                minio_container = Some(container);
                (Arc::new(store), Some(endpoint))
            } else {
                let store = object_store::local::LocalFileSystem::new_with_prefix(&storage_dir)
                    .context("creating local object store")?;
                (Arc::new(store), None)
            };

        let cache = Arc::new(
            SegmentCache::new(&cache_dir, 10 * 1024 * 1024).context("creating segment cache")?,
        );

        let config = WriteConfig {
            segment_max_size: 4096,
            segment_max_age_ms: 60_000,
            s3_bucket: "test-bucket".to_string(),
            s3_region: "us-east-1".to_string(),
            s3_endpoint: s3_endpoint.clone(),
            block_size_target: 4096,
            s3_upload_retries: 1,
            wal_config: None,
            ..Default::default()
        };

        let writer_pool = Arc::new(WriterPool::new(
            metadata.clone(),
            object_store.clone(),
            config.clone(),
        ));

        // Start background flush (short interval for tests)
        let _flush_handle = writer_pool
            .clone()
            .start_background_flush(Duration::from_secs(2));

        // ── Register test agent & acquire leases helper ────────────────────
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        metadata
            .register_agent(AgentInfo {
                agent_id: TEST_AGENT_ID.to_string(),
                address: "127.0.0.1:0".to_string(),
                availability_zone: "test".to_string(),
                agent_group: "default".to_string(),
                last_heartbeat: now_ms,
                started_at: now_ms,
                metadata: HashMap::new(),
            })
            .await
            .context("registering test agent")?;

        // ── Schema Registry ────────────────────────────────────────────────
        let schema_storage: Arc<dyn streamhouse_schema_registry::SchemaStorage> =
            Arc::new(MemorySchemaStorage::new(metadata.clone()));
        let schema_registry = Arc::new(SchemaRegistry::new(schema_storage));

        // ── gRPC server ────────────────────────────────────────────────────
        // Bind a TCP listener first to discover the random port, then pass
        // the address to tonic's serve_with_shutdown.
        let agent_router = Arc::new(AgentRouter::new(metadata.clone()));

        let grpc_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("binding gRPC listener")?;
        let grpc_addr = grpc_listener.local_addr()?;
        // Drop the listener so tonic can bind the same port.
        // (port reuse is safe here — we're on localhost in tests.)
        drop(grpc_listener);

        let mut grpc_service = StreamHouseService::new(
            metadata.clone(),
            object_store.clone(),
            cache.clone(),
            Some(agent_router.clone()),
            config.clone(),
            TEST_AGENT_ID.to_string(),
        );
        grpc_service.set_schema_registry(Some(schema_registry.clone()));

        let grpc_shutdown_rx = shutdown_rx.clone();
        let grpc_handle = tokio::spawn(async move {
            let _ = GrpcServer::builder()
                .add_service(StreamHouseServer::new(grpc_service))
                .serve_with_shutdown(grpc_addr, async move {
                    let mut rx = grpc_shutdown_rx;
                    while !*rx.borrow() {
                        if rx.changed().await.is_err() {
                            break;
                        }
                    }
                })
                .await;
        });

        // ── HTTP (REST + Schema Registry) server ───────────────────────────
        let http_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("binding HTTP listener")?;
        let http_addr = http_listener.local_addr()?;

        let api_state = AppState {
            metadata: metadata.clone(),
            agent_router: Some(agent_router.clone()),
            object_store: object_store.clone(),
            segment_cache: cache.clone(),
            prometheus: None,
            auth_config: AuthConfig {
                enabled: self.auth_enabled,
                ..Default::default()
            },
            oidc_auth: None,
            topic_changed: Some(grpc_service_topic_notify_placeholder()),
            schema_registry: Some(schema_registry.clone()),
            quota_enforcer: None,
            byoc_s3: None,
        };

        let api_router = create_router(api_state);
        let schema_api = SchemaRegistryApi::new(schema_registry.clone());
        let schema_router = schema_api.router();

        let http_router = axum::Router::new()
            .merge(api_router)
            .nest("/schemas", schema_router);

        let http_shutdown_rx = shutdown_rx.clone();
        let http_handle = tokio::spawn(async move {
            let _ = axum::serve(http_listener, http_router)
                .with_graceful_shutdown(async move {
                    let mut rx = http_shutdown_rx;
                    while !*rx.borrow() {
                        if rx.changed().await.is_err() {
                            break;
                        }
                    }
                })
                .await;
        });

        // ── Kafka protocol server ──────────────────────────────────────────
        let kafka_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("binding Kafka listener")?;
        let kafka_addr = kafka_listener.local_addr()?;

        let kafka_config = KafkaServerConfig {
            bind_addr: kafka_addr.to_string(),
            node_id: 0,
            advertised_host: "127.0.0.1".to_string(),
            advertised_port: kafka_addr.port() as i32,
        };

        let kafka_state = Arc::new(KafkaServerState {
            config: kafka_config,
            metadata: metadata.clone(),
            agent_router: agent_router.clone(),
            segment_cache: cache.clone(),
            object_store: object_store.clone(),
            group_coordinator: Arc::new(GroupCoordinator::new(metadata.clone())),
            tenant_resolver: Some(KafkaTenantResolver::new(metadata.clone())),
            quota_enforcer: None,
            active_connections: Arc::new(dashmap::DashMap::new()),
        });

        let kafka_shutdown_rx = shutdown_rx.clone();
        let kafka_handle = tokio::spawn(async move {
            // Accept connections until shutdown
            loop {
                tokio::select! {
                    result = kafka_listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                let state = kafka_state.clone();
                                tokio::spawn(async move {
                                    let _ = streamhouse_kafka::server::handle_connection_public(
                                        stream, addr, state,
                                    )
                                    .await;
                                });
                            }
                            Err(e) => {
                                tracing::warn!("Kafka accept error: {}", e);
                            }
                        }
                    }
                    _ = wait_for_shutdown(kafka_shutdown_rx.clone()) => {
                        break;
                    }
                }
            }
        });

        let http_url = format!("http://127.0.0.1:{}", http_addr.port());

        // Give the servers a moment to become ready
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(TestCluster {
            http_addr,
            grpc_addr,
            kafka_addr,
            http_url,
            metadata,
            _shutdown_tx: shutdown_tx,
            _http_handle: http_handle,
            _grpc_handle: grpc_handle,
            _kafka_handle: kafka_handle,
            _temp_dir: temp_dir,
            _postgres_container: postgres_container,
            _minio_container: minio_container,
        })
    }
}

/// Helper: wait for the shutdown watch channel to signal `true`.
async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// Placeholder for topic-change notify (tests don't need the full partition assigner).
fn grpc_service_topic_notify_placeholder() -> Arc<tokio::sync::Notify> {
    Arc::new(tokio::sync::Notify::new())
}

// ---------------------------------------------------------------------------
// TestCluster — lease helper
// ---------------------------------------------------------------------------

impl TestCluster {
    /// Create a topic and acquire partition leases so produce/consume works.
    ///
    /// This is a convenience that combines the REST `create_topic` call with
    /// direct metadata manipulation to set up leases (which normally the
    /// embedded agent would handle).
    pub async fn create_topic_with_leases(&self, name: &str, partitions: u32) -> Result<()> {
        // Create the topic through metadata (faster than going through REST)
        let topic_config = streamhouse_metadata::TopicConfig {
            name: name.to_string(),
            partition_count: partitions,
            retention_ms: Some(86_400_000), // 1 day
            cleanup_policy: Default::default(),
            config: HashMap::new(),
        };
        self.metadata
            .create_topic(topic_config)
            .await
            .context("creating topic")?;

        // Acquire leases for all partitions
        for p in 0..partitions {
            self.metadata
                .acquire_partition_lease(
                    streamhouse_metadata::DEFAULT_ORGANIZATION_ID,
                    name,
                    p,
                    TEST_AGENT_ID,
                    60_000,
                )
                .await
                .context("acquiring partition lease")?;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RestClient
// ---------------------------------------------------------------------------

/// Thin REST client for StreamHouse's HTTP API.
///
/// All methods return deserialized JSON responses. Errors are surfaced as
/// [`anyhow::Error`] with the HTTP status and body for easy debugging.
pub struct RestClient {
    base_url: String,
    client: reqwest::Client,
    auth_token: Option<String>,
    org_id: Option<String>,
}

impl RestClient {
    /// Create a new client pointing at the given base URL.
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to create reqwest client");
        Self {
            base_url,
            client,
            auth_token: None,
            org_id: None,
        }
    }

    /// Set a Bearer token used for all subsequent requests.
    pub fn set_auth(&mut self, token: impl Into<String>) {
        self.auth_token = Some(token.into());
    }

    /// Clear the Bearer token.
    pub fn clear_auth(&mut self) {
        self.auth_token = None;
    }

    /// Set the `X-Organization-Id` header for all subsequent requests (dev/no-auth mode).
    pub fn set_org_id(&mut self, org_id: impl Into<String>) {
        self.org_id = Some(org_id.into());
    }

    // -- internal helpers ---------------------------------------------------

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let builder = match &self.auth_token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        };
        match &self.org_id {
            Some(org_id) => builder.header("X-Organization-Id", org_id.as_str()),
            None => builder,
        }
    }

    async fn check_response(resp: reqwest::Response) -> Result<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {} — {}", status, body);
        }
    }

    // -- Health -------------------------------------------------------------

    /// GET /health
    pub async fn health(&self) -> Result<serde_json::Value> {
        let resp = self.client.get(self.url("/health")).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Organizations (admin) ----------------------------------------------

    /// POST /api/v1/organizations
    pub async fn create_org(&self, name: &str, slug: &str) -> Result<serde_json::Value> {
        let body = serde_json::json!({ "name": name, "slug": slug });
        let req = self
            .client
            .post(self.url("/api/v1/organizations"))
            .json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- API Keys -----------------------------------------------------------

    /// POST /api/v1/organizations/:org_id/api-keys
    pub async fn create_api_key(&self, org_id: &str, name: &str) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "name": name,
            "permissions": ["read", "write", "admin"],
        });
        let url = self.url(&format!("/api/v1/organizations/{}/api-keys", org_id));
        let req = self.client.post(url).json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Topics -------------------------------------------------------------

    /// POST /api/v1/topics
    pub async fn create_topic(&self, name: &str, partitions: u32) -> Result<TopicResponse> {
        let body = serde_json::json!({
            "name": name,
            "partitions": partitions,
            "replication_factor": 1,
        });
        let req = self.client.post(self.url("/api/v1/topics")).json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// GET /api/v1/topics
    pub async fn list_topics(&self) -> Result<Vec<TopicResponse>> {
        let req = self.client.get(self.url("/api/v1/topics"));
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// GET /api/v1/topics/:name
    pub async fn get_topic(&self, name: &str) -> Result<TopicResponse> {
        let req = self
            .client
            .get(self.url(&format!("/api/v1/topics/{}", name)));
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// DELETE /api/v1/topics/:name
    pub async fn delete_topic(&self, name: &str) -> Result<()> {
        let req = self
            .client
            .delete(self.url(&format!("/api/v1/topics/{}", name)));
        let resp = self.apply_auth(req).send().await?;
        let _ = Self::check_response(resp).await?;
        Ok(())
    }

    // -- Produce ------------------------------------------------------------

    /// POST /api/v1/produce  (single record)
    pub async fn produce(
        &self,
        topic: &str,
        key: Option<&str>,
        value: &str,
    ) -> Result<ProduceResponse> {
        let mut body = serde_json::json!({
            "topic": topic,
            "value": value,
        });
        if let Some(k) = key {
            body["key"] = serde_json::Value::String(k.to_string());
        }
        let req = self.client.post(self.url("/api/v1/produce")).json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// POST /api/v1/produce/batch
    pub async fn produce_batch(
        &self,
        topic: &str,
        records: Vec<BatchRecord>,
    ) -> Result<BatchProduceResponse> {
        let body = serde_json::json!({
            "topic": topic,
            "records": records,
        });
        let req = self
            .client
            .post(self.url("/api/v1/produce/batch"))
            .json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Consume ------------------------------------------------------------

    /// GET /api/v1/consume?topic=...&partition=...&offset=...&max_records=...
    pub async fn consume(
        &self,
        topic: &str,
        partition: u32,
        offset: u64,
        max_records: u32,
    ) -> Result<ConsumeResponse> {
        let url = self.url(&format!(
            "/api/v1/consume?topic={}&partition={}&offset={}&max_records={}",
            topic, partition, offset, max_records,
        ));
        let req = self.client.get(url);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- SQL ----------------------------------------------------------------

    /// POST /api/v1/sql
    pub async fn sql_query(&self, query: &str) -> Result<SqlResponse> {
        let body = serde_json::json!({ "query": query });
        let req = self.client.post(self.url("/api/v1/sql")).json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Schema Registry ----------------------------------------------------

    /// POST /schemas/subjects/:subject/versions
    pub async fn register_schema(
        &self,
        subject: &str,
        schema: &str,
        schema_type: &str,
    ) -> Result<SchemaResponse> {
        let body = serde_json::json!({
            "schema": schema,
            "schemaType": schema_type,
        });
        let url = self.url(&format!("/schemas/subjects/{}/versions", subject));
        let req = self.client.post(url).json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// GET /schemas/schemas/:id
    pub async fn get_schema(&self, id: i32) -> Result<serde_json::Value> {
        let url = self.url(&format!("/schemas/schemas/{}", id));
        let req = self.client.get(url);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Metrics ------------------------------------------------------------

    /// GET /api/v1/metrics
    pub async fn get_metrics(&self) -> Result<serde_json::Value> {
        let req = self.client.get(self.url("/api/v1/metrics"));
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Consumer Groups ----------------------------------------------------

    /// GET /api/v1/consumer-groups
    pub async fn list_consumer_groups(&self) -> Result<Vec<serde_json::Value>> {
        let req = self.client.get(self.url("/api/v1/consumer-groups"));
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// POST /api/v1/consumer-groups/commit
    pub async fn commit_offset(
        &self,
        group_id: &str,
        topic: &str,
        partition: u32,
        offset: u64,
    ) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "groupId": group_id,
            "topic": topic,
            "partition": partition,
            "offset": offset,
        });
        let req = self
            .client
            .post(self.url("/api/v1/consumer-groups/commit"))
            .json(&body);
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Agents -------------------------------------------------------------

    /// GET /api/v1/agents
    pub async fn list_agents(&self) -> Result<Vec<serde_json::Value>> {
        let req = self.client.get(self.url("/api/v1/agents"));
        let resp = self.apply_auth(req).send().await?;
        let resp = Self::check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // -- Raw request helpers ------------------------------------------------

    /// Send a raw GET and return the status + body as JSON.
    pub async fn get_raw(&self, path: &str) -> Result<(reqwest::StatusCode, serde_json::Value)> {
        let req = self.client.get(self.url(path));
        let resp = self.apply_auth(req).send().await?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        Ok((status, body))
    }

    /// Send a raw POST and return the status + body as JSON.
    pub async fn post_raw(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<(reqwest::StatusCode, serde_json::Value)> {
        let req = self.client.post(self.url(path)).json(&body);
        let resp = self.apply_auth(req).send().await?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        Ok((status, body))
    }

    /// Send a raw DELETE and return the status.
    pub async fn delete_raw(&self, path: &str) -> Result<reqwest::StatusCode> {
        let req = self.client.delete(self.url(path));
        let resp = self.apply_auth(req).send().await?;
        Ok(resp.status())
    }
}

// ---------------------------------------------------------------------------
// Response types (mirrors the API JSON shapes)
// ---------------------------------------------------------------------------

/// Topic as returned by the REST API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopicResponse {
    pub name: String,
    pub partitions: u32,
    #[serde(default)]
    pub replication_factor: u32,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub message_count: u64,
    #[serde(default)]
    pub size_bytes: u64,
}

/// Single-record produce response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProduceResponse {
    pub offset: u64,
    pub partition: u32,
}

/// Batch record for produce.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition: Option<u32>,
}

/// Batch produce response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchProduceResponse {
    pub count: usize,
    pub offsets: Vec<BatchRecordResult>,
}

/// Individual record result in a batch produce.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchRecordResult {
    pub partition: u32,
    pub offset: u64,
}

/// Consume response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeResponse {
    pub records: Vec<ConsumedRecord>,
    pub next_offset: u64,
}

/// Individual consumed record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumedRecord {
    pub partition: u32,
    pub offset: u64,
    pub key: Option<String>,
    pub value: String,
    pub timestamp: i64,
}

/// SQL query response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlResponse {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub execution_time_ms: u64,
    #[serde(default)]
    pub truncated: bool,
}

/// Column metadata in SQL response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

/// Schema registration response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SchemaResponse {
    pub id: i32,
}

// ---------------------------------------------------------------------------
// Utility: init tracing for tests (call once per test binary)
// ---------------------------------------------------------------------------

/// Initialize tracing for tests. Safe to call multiple times (subsequent calls
/// are no-ops thanks to `try_init`).
pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_test_writer()
        .try_init();
}
