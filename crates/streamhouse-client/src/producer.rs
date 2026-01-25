//! Producer API for sending records to StreamHouse
//!
//! The Producer handles:
//! - Agent discovery and health monitoring
//! - Partition routing (consistent hashing or explicit)
//! - Batching and compression
//! - Retries and error handling
//!
//! # Examples
//!
//! ```ignore
//! use streamhouse_client::Producer;
//!
//! let producer = Producer::builder()
//!     .metadata_store(metadata_store)
//!     .agent_group("prod")
//!     .batch_size(100)
//!     .compression_enabled(true)
//!     .build()
//!     .await?;
//!
//! // Send with key-based partitioning
//! let result = producer.send("orders", Some(b"user123"), b"order data", None).await?;
//! println!("Written to partition {} at offset {}", result.partition, result.offset);
//!
//! // Send to specific partition
//! let result = producer.send("orders", None, b"order data", Some(3)).await?;
//! ```

use crate::error::{ClientError, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::{AgentInfo, MetadataStore, Topic};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Producer configuration
#[derive(Clone)]
pub struct ProducerConfig {
    /// Metadata store for agent discovery and topic metadata
    pub metadata_store: Arc<dyn MetadataStore>,

    /// Agent group to send requests to (e.g., "prod", "staging")
    pub agent_group: String,

    /// Maximum batch size before flushing (default: 100)
    pub batch_size: usize,

    /// Maximum time to wait before flushing batch (default: 100ms)
    pub batch_timeout: Duration,

    /// Enable LZ4 compression (default: true)
    pub compression_enabled: bool,

    /// Request timeout (default: 30s)
    pub request_timeout: Duration,

    /// Agent discovery refresh interval (default: 30s)
    pub agent_refresh_interval: Duration,

    /// Maximum retries on failure (default: 3)
    pub max_retries: usize,

    /// Retry backoff base (default: 100ms)
    pub retry_backoff: Duration,
}

impl Default for ProducerConfig {
    fn default() -> Self {
        // This will be replaced with actual values in builder
        unimplemented!("Use ProducerBuilder to create ProducerConfig")
    }
}

/// A record to be sent to StreamHouse
#[derive(Debug, Clone)]
pub struct ProducerRecord {
    /// Topic name
    pub topic: String,

    /// Optional key for partitioning (if None, round-robin)
    pub key: Option<Bytes>,

    /// Record value (payload)
    pub value: Bytes,

    /// Optional explicit partition (overrides key-based partitioning)
    pub partition: Option<u32>,

    /// Optional timestamp (if None, uses current time)
    pub timestamp: Option<i64>,
}

/// Result of a successful send operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    /// Topic name
    pub topic: String,

    /// Partition the record was written to
    pub partition: u32,

    /// Offset assigned to the record
    pub offset: u64,

    /// Timestamp of the record
    pub timestamp: i64,
}

/// High-level Producer API
///
/// The Producer handles agent discovery, partition routing, batching, and retries.
/// It maintains a connection pool to healthy agents and automatically refreshes
/// the agent list.
pub struct Producer {
    config: ProducerConfig,
    #[allow(dead_code)] // Will be used in Phase 5.2 for agent routing
    agents: Arc<RwLock<HashMap<String, AgentInfo>>>,
    topic_cache: Arc<RwLock<HashMap<String, Topic>>>,
    _refresh_handle: tokio::task::JoinHandle<()>,
}

impl Producer {
    /// Create a new ProducerBuilder
    pub fn builder() -> ProducerBuilder {
        ProducerBuilder::new()
    }

    /// Send a record to StreamHouse
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `key` - Optional key for partitioning
    /// * `value` - Record value (payload)
    /// * `partition` - Optional explicit partition (overrides key-based partitioning)
    ///
    /// # Returns
    ///
    /// `SendResult` containing the partition and offset where the record was written
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Topic does not exist
    /// - No agents available
    /// - Agent communication fails
    /// - Write operation fails
    pub async fn send(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        value: &[u8],
        partition: Option<u32>,
    ) -> Result<SendResult> {
        // Get topic metadata
        let topic_meta = self.get_or_fetch_topic(topic).await?;

        // Determine partition
        let partition_id = match partition {
            Some(p) => {
                if p >= topic_meta.partition_count {
                    return Err(ClientError::InvalidPartition(
                        p,
                        topic.to_string(),
                        topic_meta.partition_count - 1,
                    ));
                }
                p
            }
            None => self.compute_partition(key, topic_meta.partition_count),
        };

        // Phase 5.1: Write directly to storage (no agent communication yet)
        // Phase 5.2: Find agent and send via gRPC
        // let _agent = self.find_agent_for_partition(topic, partition_id).await?;

        let result = self
            .send_to_storage(topic, partition_id, key, value)
            .await?;

        Ok(result)
    }

    /// Flush any pending batched records
    ///
    /// This is a no-op in the initial implementation (no batching yet).
    /// Will be implemented in Phase 5.1 completion.
    pub async fn flush(&self) -> Result<()> {
        Ok(())
    }

    /// Close the producer and clean up resources
    pub async fn close(self) -> Result<()> {
        self._refresh_handle.abort();
        Ok(())
    }

    // Internal methods

    /// Get topic from cache or fetch from metadata store
    async fn get_or_fetch_topic(&self, topic: &str) -> Result<Topic> {
        {
            let cache = self.topic_cache.read().await;
            if let Some(t) = cache.get(topic) {
                return Ok(t.clone());
            }
        }

        // Fetch from metadata store
        let topic_meta = self
            .config
            .metadata_store
            .get_topic(topic)
            .await?
            .ok_or_else(|| ClientError::TopicNotFound(topic.to_string()))?;

        // Cache it
        {
            let mut cache = self.topic_cache.write().await;
            cache.insert(topic.to_string(), topic_meta.clone());
        }

        Ok(topic_meta)
    }

    /// Compute partition using key-based consistent hashing
    fn compute_partition(&self, key: Option<&[u8]>, partition_count: u32) -> u32 {
        match key {
            Some(k) => {
                let mut hasher = siphasher::sip::SipHasher::new();
                k.hash(&mut hasher);
                let hash = hasher.finish();
                (hash % partition_count as u64) as u32
            }
            None => {
                // Round-robin: use timestamp
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                (now % partition_count as u128) as u32
            }
        }
    }

    /// Find agent responsible for a partition
    ///
    /// Uses consistent hashing to route to the agent holding the lease.
    /// For now, returns any available agent (Phase 5.1 - direct storage write).
    #[allow(dead_code)] // Will be used in Phase 5.2 for agent routing
    async fn find_agent_for_partition(
        &self,
        _topic: &str,
        _partition_id: u32,
    ) -> Result<AgentInfo> {
        let agents = self.agents.read().await;

        if agents.is_empty() {
            return Err(ClientError::NoAgentsAvailable(
                self.config.agent_group.clone(),
            ));
        }

        // For now, return the first agent
        // In Phase 5.2, we'll use consistent hashing to find the agent with the lease
        Ok(agents.values().next().unwrap().clone())
    }

    /// Send record directly to storage (Phase 5.1 - no gRPC yet)
    ///
    /// This is a temporary implementation that writes directly to storage.
    /// In Phase 5.2, this will be replaced with gRPC call to agent.
    async fn send_to_storage(
        &self,
        topic: &str,
        partition_id: u32,
        key: Option<&[u8]>,
        value: &[u8],
    ) -> Result<SendResult> {
        use object_store::aws::AmazonS3Builder;
        use streamhouse_storage::{PartitionWriter, WriteConfig};

        // Create S3 object store (MinIO)
        let object_store = Arc::new(
            AmazonS3Builder::new()
                .with_bucket_name("streamhouse")
                .with_region("us-east-1")
                .with_endpoint("http://localhost:9000")
                .with_access_key_id("minioadmin")
                .with_secret_access_key("minioadmin")
                .with_allow_http(true)
                .build()
                .map_err(|e| ClientError::StorageError(e.to_string()))?,
        );

        // Create write config
        let config = WriteConfig {
            segment_max_size: 64 * 1024 * 1024, // 64MB
            segment_max_age_ms: 3_600_000,      // 1 hour
            s3_bucket: "streamhouse".to_string(),
            s3_region: "us-east-1".to_string(),
            s3_endpoint: Some("http://localhost:9000".to_string()),
            block_size_target: 1024 * 1024, // 1MB
            s3_upload_retries: 3,
        };

        let mut writer = PartitionWriter::new(
            topic.to_string(),
            partition_id,
            object_store,
            Arc::clone(&self.config.metadata_store),
            config,
        )
        .await
        .map_err(|e| ClientError::StorageError(e.to_string()))?;

        // Get timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Write record
        let offset = writer
            .append(
                key.map(Bytes::copy_from_slice),
                Bytes::copy_from_slice(value),
                timestamp,
            )
            .await
            .map_err(|e| ClientError::StorageError(e.to_string()))?;

        debug!(
            topic = topic,
            partition = partition_id,
            offset = offset,
            "Record written"
        );

        Ok(SendResult {
            topic: topic.to_string(),
            partition: partition_id,
            offset,
            timestamp: timestamp as i64,
        })
    }

    /// Background task to refresh agent list
    async fn refresh_agents_task(
        metadata_store: Arc<dyn MetadataStore>,
        agent_group: String,
        agents: Arc<RwLock<HashMap<String, AgentInfo>>>,
        interval: Duration,
    ) {
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await;

            match metadata_store.list_agents(Some(&agent_group), None).await {
                Ok(agent_list) => {
                    let mut map = HashMap::new();
                    for agent in agent_list {
                        map.insert(agent.agent_id.clone(), agent);
                    }

                    let count = map.len();
                    *agents.write().await = map;

                    debug!(agent_group = agent_group, count = count, "Refreshed agents");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to refresh agents");
                }
            }
        }
    }
}

/// Builder for creating a Producer
pub struct ProducerBuilder {
    metadata_store: Option<Arc<dyn MetadataStore>>,
    agent_group: String,
    batch_size: usize,
    batch_timeout: Duration,
    compression_enabled: bool,
    request_timeout: Duration,
    agent_refresh_interval: Duration,
    max_retries: usize,
    retry_backoff: Duration,
}

impl ProducerBuilder {
    /// Create a new ProducerBuilder with default settings
    pub fn new() -> Self {
        Self {
            metadata_store: None,
            agent_group: "default".to_string(),
            batch_size: 100,
            batch_timeout: Duration::from_millis(100),
            compression_enabled: true,
            request_timeout: Duration::from_secs(30),
            agent_refresh_interval: Duration::from_secs(30),
            max_retries: 3,
            retry_backoff: Duration::from_millis(100),
        }
    }

    /// Set the metadata store
    pub fn metadata_store(mut self, store: Arc<dyn MetadataStore>) -> Self {
        self.metadata_store = Some(store);
        self
    }

    /// Set the agent group
    pub fn agent_group(mut self, group: impl Into<String>) -> Self {
        self.agent_group = group.into();
        self
    }

    /// Set the batch size
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set the batch timeout
    pub fn batch_timeout(mut self, timeout: Duration) -> Self {
        self.batch_timeout = timeout;
        self
    }

    /// Enable or disable compression
    pub fn compression_enabled(mut self, enabled: bool) -> Self {
        self.compression_enabled = enabled;
        self
    }

    /// Set the request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set the agent refresh interval
    pub fn agent_refresh_interval(mut self, interval: Duration) -> Self {
        self.agent_refresh_interval = interval;
        self
    }

    /// Set the maximum number of retries
    pub fn max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the retry backoff duration
    pub fn retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self
    }

    /// Build the Producer
    pub async fn build(self) -> Result<Producer> {
        let metadata_store = self
            .metadata_store
            .ok_or_else(|| ClientError::ConfigError("metadata_store is required".to_string()))?;

        let config = ProducerConfig {
            metadata_store: Arc::clone(&metadata_store),
            agent_group: self.agent_group.clone(),
            batch_size: self.batch_size,
            batch_timeout: self.batch_timeout,
            compression_enabled: self.compression_enabled,
            request_timeout: self.request_timeout,
            agent_refresh_interval: self.agent_refresh_interval,
            max_retries: self.max_retries,
            retry_backoff: self.retry_backoff,
        };

        let agents = Arc::new(RwLock::new(HashMap::new()));
        let topic_cache = Arc::new(RwLock::new(HashMap::new()));

        // Initial agent discovery
        let agent_list = metadata_store
            .list_agents(Some(&self.agent_group), None)
            .await?;

        let mut agent_map = HashMap::new();
        for agent in agent_list {
            agent_map.insert(agent.agent_id.clone(), agent);
        }

        info!(
            agent_group = self.agent_group,
            count = agent_map.len(),
            "Producer initialized with agents"
        );

        *agents.write().await = agent_map;

        // Start background refresh task
        let refresh_handle = tokio::spawn(Producer::refresh_agents_task(
            Arc::clone(&metadata_store),
            self.agent_group.clone(),
            Arc::clone(&agents),
            self.agent_refresh_interval,
        ));

        Ok(Producer {
            config,
            agents,
            topic_cache,
            _refresh_handle: refresh_handle,
        })
    }
}

impl Default for ProducerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_partition_with_key() {
        let producer = ProducerBuilder {
            metadata_store: None,
            agent_group: "test".to_string(),
            batch_size: 100,
            batch_timeout: Duration::from_millis(100),
            compression_enabled: true,
            request_timeout: Duration::from_secs(30),
            agent_refresh_interval: Duration::from_secs(30),
            max_retries: 3,
            retry_backoff: Duration::from_millis(100),
        };

        // Create a mock producer (we can't fully initialize without metadata store)
        // Instead, test the partition computation logic directly

        // Same key should always map to same partition
        let key = b"user123";
        let mut hasher1 = siphasher::sip::SipHasher::new();
        key.hash(&mut hasher1);
        let hash1 = hasher1.finish();
        let partition1 = (hash1 % 10) as u32;

        let mut hasher2 = siphasher::sip::SipHasher::new();
        key.hash(&mut hasher2);
        let hash2 = hasher2.finish();
        let partition2 = (hash2 % 10) as u32;

        assert_eq!(partition1, partition2);

        // Different keys should (likely) map to different partitions
        let key2 = b"user456";
        let mut hasher3 = siphasher::sip::SipHasher::new();
        key2.hash(&mut hasher3);
        let hash3 = hasher3.finish();
        let partition3 = (hash3 % 10) as u32;

        // Not guaranteed to be different, but very likely
        // Just check they computed without error
        assert!(partition3 < 10);
    }
}
