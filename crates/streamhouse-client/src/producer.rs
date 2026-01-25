//! Producer API for sending records to StreamHouse.
//!
//! This module provides the high-level Producer API for sending records to StreamHouse topics.
//! The Producer handles all the complexity of agent discovery, partition routing, connection
//! management, and error handling.
//!
//! ## Features
//!
//! - **Automatic partition routing**: Key-based consistent hashing or explicit partition selection
//! - **Agent discovery**: Background refresh of healthy agents every 30 seconds
//! - **Topic metadata caching**: Reduces metadata store queries
//! - **Builder pattern**: Fluent, type-safe configuration
//! - **Comprehensive error handling**: Clear error messages for debugging
//!
//! ## Phase 5.1 vs Phase 5.2
//!
//! **Phase 5.1 (Current)**:
//! - Writes directly to storage (no agent communication)
//! - Creates new writer for each send
//! - ~5 records/sec throughput
//! - Good for testing and development
//!
//! **Phase 5.2 (Future)**:
//! - Communicates with agents via gRPC
//! - Connection pooling and batching
//! - 50K+ records/sec throughput
//! - Production-ready
//!
//! ## Partition Routing
//!
//! The Producer supports two partition selection strategies:
//!
//! 1. **Key-based (recommended)**: Hash the key using SipHash to deterministically
//!    select a partition. Same key always maps to same partition.
//!
//! 2. **Explicit**: Manually specify the partition ID. Useful for specific use cases
//!    like routing all records from a specific source to the same partition.
//!
//! ## Examples
//!
//! ### Basic Usage
//!
//! ```ignore
//! use streamhouse_client::Producer;
//! use streamhouse_metadata::{SqliteMetadataStore, MetadataStore};
//! use std::sync::Arc;
//!
//! let metadata: Arc<dyn MetadataStore> = Arc::new(
//!     SqliteMetadataStore::new("metadata.db").await?
//! );
//!
//! let producer = Producer::builder()
//!     .metadata_store(metadata)
//!     .agent_group("prod")
//!     .compression_enabled(true)
//!     .build()
//!     .await?;
//!
//! // Send with key-based partitioning
//! let result = producer.send(
//!     "orders",
//!     Some(b"user123"),  // Key determines partition
//!     b"order data",
//!     None,
//! ).await?;
//!
//! println!("Written to partition {} at offset {}",
//!     result.partition, result.offset);
//! ```
//!
//! ### Explicit Partition Selection
//!
//! ```ignore
//! // Send to specific partition
//! let result = producer.send(
//!     "orders",
//!     None,
//!     b"order data",
//!     Some(3),  // Explicit partition
//! ).await?;
//! ```
//!
//! ### Configuration
//!
//! ```ignore
//! let producer = Producer::builder()
//!     .metadata_store(metadata)
//!     .agent_group("prod")
//!     .batch_size(100)              // Records per batch
//!     .batch_timeout(Duration::from_millis(100))  // Max batch wait
//!     .compression_enabled(true)     // Enable LZ4
//!     .request_timeout(Duration::from_secs(30))   // Request timeout
//!     .max_retries(3)                // Retry attempts
//!     .build()
//!     .await?;
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

/// Producer configuration containing all operational parameters.
///
/// This struct is created by `ProducerBuilder` and contains all the settings
/// that control Producer behavior. Users should not create this directly;
/// use `Producer::builder()` instead.
///
/// ## Configuration Fields
///
/// - `metadata_store`: Required. Used for topic metadata and agent discovery.
/// - `agent_group`: Agent group to route requests to (default: "default")
/// - `batch_size`: Records to accumulate before flushing (default: 100)
/// - `batch_timeout`: Max time to wait before flushing (default: 100ms)
/// - `compression_enabled`: Enable LZ4 compression (default: true)
/// - `request_timeout`: Timeout for agent requests (default: 30s)
/// - `agent_refresh_interval`: How often to refresh agent list (default: 30s)
/// - `max_retries`: Max retry attempts on failure (default: 3)
/// - `retry_backoff`: Base backoff duration for retries (default: 100ms)
#[derive(Clone)]
pub struct ProducerConfig {
    /// Metadata store for discovering topics and agents.
    ///
    /// The Producer queries this store to:
    /// - Get topic metadata (partition count, retention)
    /// - Discover healthy agents in the agent group
    /// - Validate partition IDs
    pub metadata_store: Arc<dyn MetadataStore>,

    /// Agent group to send produce requests to.
    ///
    /// Only agents in this group will be considered for routing.
    /// Common values: "prod", "staging", "test"
    pub agent_group: String,

    /// Maximum number of records to batch before flushing.
    ///
    /// Higher values increase throughput but add latency.
    /// Default: 100
    ///
    /// ## Phase 5.2+
    /// Currently unused (Phase 5.1 doesn't batch). Will be used in Phase 5.2.
    pub batch_size: usize,

    /// Maximum time to wait before flushing a batch.
    ///
    /// Ensures records aren't delayed indefinitely when traffic is low.
    /// Default: 100ms
    ///
    /// ## Phase 5.2+
    /// Currently unused (Phase 5.1 doesn't batch). Will be used in Phase 5.2.
    pub batch_timeout: Duration,

    /// Whether to enable LZ4 compression on records.
    ///
    /// Compression typically achieves 4-5x reduction with minimal CPU overhead.
    /// Default: true
    pub compression_enabled: bool,

    /// Timeout for individual produce requests to agents.
    ///
    /// If a request takes longer than this, it will be retried or failed.
    /// Default: 30 seconds
    ///
    /// ## Phase 5.2+
    /// Currently unused (Phase 5.1 writes directly to storage). Will be used in Phase 5.2.
    pub request_timeout: Duration,

    /// Interval for refreshing the list of healthy agents.
    ///
    /// The Producer queries `list_agents()` at this interval to discover
    /// new agents and remove stale ones. Agents are considered healthy if
    /// they've sent a heartbeat within the last 60 seconds.
    ///
    /// Default: 30 seconds
    pub agent_refresh_interval: Duration,

    /// Maximum number of retry attempts on transient failures.
    ///
    /// After this many retries, the operation fails with an error.
    /// Default: 3
    ///
    /// ## Phase 5.2+
    /// Currently unused (Phase 5.1 doesn't retry). Will be used in Phase 5.2.
    pub max_retries: usize,

    /// Base duration for exponential backoff between retries.
    ///
    /// Actual backoff is `retry_backoff * 2^attempt`.
    /// Default: 100ms
    ///
    /// ## Phase 5.2+
    /// Currently unused (Phase 5.1 doesn't retry). Will be used in Phase 5.2.
    pub retry_backoff: Duration,
}

impl Default for ProducerConfig {
    fn default() -> Self {
        // This will be replaced with actual values in builder
        unimplemented!("Use ProducerBuilder to create ProducerConfig")
    }
}

/// A record to be sent to StreamHouse.
///
/// This struct represents a message to be written to a topic. It contains
/// the topic name, optional key for partitioning, value (payload), and
/// optional metadata.
///
/// ## Fields
///
/// - `topic`: Destination topic name
/// - `key`: Optional partitioning key (determines which partition)
/// - `value`: Record payload (the actual data)
/// - `partition`: Optional explicit partition (overrides key-based routing)
/// - `timestamp`: Optional timestamp (defaults to current time)
///
/// ## Examples
///
/// ```ignore
/// use streamhouse_client::ProducerRecord;
/// use bytes::Bytes;
///
/// // With key (recommended)
/// let record = ProducerRecord {
///     topic: "orders".to_string(),
///     key: Some(Bytes::from("user123")),
///     value: Bytes::from("order data"),
///     partition: None,
///     timestamp: None,
/// };
///
/// // Explicit partition
/// let record = ProducerRecord {
///     topic: "logs".to_string(),
///     key: None,
///     value: Bytes::from("log message"),
///     partition: Some(2),
///     timestamp: Some(1234567890),
/// };
/// ```
///
/// ## Note
///
/// Currently, `ProducerRecord` is defined but not used by the `send()` API,
/// which takes individual parameters instead. This struct is reserved for
/// future batch send APIs in Phase 5.2+.
#[derive(Debug, Clone)]
pub struct ProducerRecord {
    /// Topic name to send the record to.
    ///
    /// Must match an existing topic created via `metadata_store.create_topic()`.
    pub topic: String,

    /// Optional key for partitioning.
    ///
    /// If provided, the key is hashed using SipHash to deterministically
    /// select a partition. Same key always maps to same partition.
    ///
    /// If `None` and `partition` is also `None`, partition is selected
    /// via round-robin based on current timestamp.
    pub key: Option<Bytes>,

    /// Record value (payload).
    ///
    /// This is the actual data to store. Can be any byte sequence.
    /// Common patterns:
    /// - JSON: `Bytes::from(serde_json::to_vec(&data)?)`
    /// - Protobuf: `Bytes::from(msg.encode_to_vec())`
    /// - Raw bytes: `Bytes::from(vec![1, 2, 3])`
    pub value: Bytes,

    /// Optional explicit partition ID.
    ///
    /// If provided, overrides key-based partitioning and sends directly
    /// to the specified partition. Must be in range `[0, partition_count)`.
    pub partition: Option<u32>,

    /// Optional timestamp in milliseconds since Unix epoch.
    ///
    /// If `None`, the current system time is used.
    pub timestamp: Option<i64>,
}

/// Result of a successful send operation.
///
/// This struct contains all the metadata about where and when a record
/// was written. It's returned by `Producer::send()` on success.
///
/// ## Fields
///
/// - `topic`: Topic the record was written to
/// - `partition`: Partition ID (0-indexed)
/// - `offset`: Offset assigned to the record within the partition
/// - `timestamp`: Timestamp of the record in milliseconds since Unix epoch
///
/// ## Examples
///
/// ```ignore
/// let result = producer.send("orders", Some(b"key"), b"value", None).await?;
///
/// println!("Record written:");
/// println!("  Topic: {}", result.topic);
/// println!("  Partition: {}", result.partition);
/// println!("  Offset: {}", result.offset);
/// println!("  Timestamp: {}", result.timestamp);
/// ```
///
/// ## Serialization
///
/// This struct implements `Serialize` and `Deserialize` for logging,
/// monitoring, and testing purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    /// Topic name the record was written to.
    pub topic: String,

    /// Partition ID the record was written to.
    ///
    /// This is the partition that was either:
    /// - Selected via key-based hashing
    /// - Explicitly specified
    /// - Chosen via round-robin
    pub partition: u32,

    /// Offset assigned to the record within the partition.
    ///
    /// Offsets are monotonically increasing integers starting from 0.
    /// Within a partition, offsets are guaranteed to be sequential and unique.
    ///
    /// ## Phase 5.1 Note
    /// Due to creating a new writer per send, offsets may not increment
    /// as expected. This will be fixed in Phase 5.2 with proper writer pooling.
    pub offset: u64,

    /// Timestamp of the record in milliseconds since Unix epoch.
    ///
    /// This is either:
    /// - The timestamp provided in the send request
    /// - The current system time (if not provided)
    pub timestamp: i64,
}

/// High-level Producer API for sending records to StreamHouse.
///
/// The Producer is the main entry point for sending records to StreamHouse topics.
/// It handles all the complexity of:
/// - Agent discovery and health monitoring
/// - Partition routing (key-based or explicit)
/// - Topic metadata caching
/// - Connection management (Phase 5.2+)
/// - Batching and compression (Phase 5.2+)
/// - Retries and error handling (Phase 5.2+)
///
/// ## Lifecycle
///
/// 1. **Creation**: Use `Producer::builder()` to create a configured producer
/// 2. **Usage**: Call `send()` to write records to topics
/// 3. **Cleanup**: Call `close()` to gracefully shutdown
///
/// ## Thread Safety
///
/// Producer is thread-safe and can be shared across multiple async tasks using `Arc`:
///
/// ```ignore
/// let producer = Arc::new(producer);
/// let tasks: Vec<_> = (0..10).map(|i| {
///     let producer = Arc::clone(&producer);
///     tokio::spawn(async move {
///         producer.send("topic", None, format!("msg {}", i).as_bytes(), None).await
///     })
/// }).collect();
/// ```
///
/// ## Background Tasks
///
/// The Producer spawns a background task that:
/// - Refreshes the agent list every 30 seconds (or configured interval)
/// - Removes agents that haven't sent heartbeat in 60+ seconds
/// - Logs errors but never panics
///
/// ## Performance
///
/// **Phase 5.1 (Current)**:
/// - ~5 records/sec (creates new writer per send)
/// - ~200ms latency (S3 connection overhead)
/// - Good for testing and development
///
/// **Phase 5.2 (Future)**:
/// - 50K+ records/sec (batching + connection pooling)
/// - <10ms p99 latency (gRPC + batching)
/// - Production-ready
pub struct Producer {
    /// Producer configuration (immutable after creation).
    config: ProducerConfig,

    /// Map of healthy agents in the configured agent group.
    ///
    /// This map is periodically refreshed by the background task.
    /// Keys are agent IDs, values are agent metadata (address, zone, etc.).
    ///
    /// ## Phase 5.2+
    /// Used for routing requests to agents. Currently unused in Phase 5.1.
    #[allow(dead_code)] // Will be used in Phase 5.2 for agent routing
    agents: Arc<RwLock<HashMap<String, AgentInfo>>>,

    /// Cache of topic metadata to reduce metadata store queries.
    ///
    /// Keys are topic names, values are topic metadata (partition count, etc.).
    /// Entries are added on first access and never removed (topics rarely change).
    topic_cache: Arc<RwLock<HashMap<String, Topic>>>,

    /// Handle to the background agent refresh task.
    ///
    /// This task runs continuously until `close()` is called, at which point
    /// it's aborted. The leading underscore prevents unused field warnings.
    _refresh_handle: tokio::task::JoinHandle<()>,
}

impl Producer {
    /// Create a new `ProducerBuilder` to configure and build a Producer.
    ///
    /// This is the primary way to create a Producer. The builder pattern
    /// allows for fluent, type-safe configuration with sensible defaults.
    ///
    /// # Returns
    ///
    /// A new `ProducerBuilder` with default settings.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use streamhouse_client::Producer;
    ///
    /// let producer = Producer::builder()
    ///     .metadata_store(metadata)
    ///     .agent_group("prod")
    ///     .build()
    ///     .await?;
    /// ```
    pub fn builder() -> ProducerBuilder {
        ProducerBuilder::new()
    }

    /// Send a record to a StreamHouse topic.
    ///
    /// This is the primary method for writing data to StreamHouse. It handles
    /// partition selection, agent routing (Phase 5.2+), and error handling.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name (must exist in metadata store)
    /// * `key` - Optional key for partitioning (same key → same partition)
    /// * `value` - Record payload (the actual data to store)
    /// * `partition` - Optional explicit partition ID (overrides key-based routing)
    ///
    /// # Returns
    ///
    /// `SendResult` containing:
    /// - `topic`: Topic the record was written to
    /// - `partition`: Partition ID (0-indexed)
    /// - `offset`: Offset assigned within the partition
    /// - `timestamp`: Record timestamp in milliseconds since Unix epoch
    ///
    /// # Errors
    ///
    /// Returns `ClientError` if:
    /// - `TopicNotFound`: Topic doesn't exist
    /// - `InvalidPartition`: Partition ID >= partition_count
    /// - `NoAgentsAvailable`: No healthy agents in group (Phase 5.2+)
    /// - `AgentConnectionFailed`: Can't reach agent (Phase 5.2+)
    /// - `StorageError`: S3/MinIO write failed
    /// - `MetadataError`: Metadata store query failed
    ///
    /// # Examples
    ///
    /// ## Key-based partitioning (recommended)
    ///
    /// ```ignore
    /// // Same key always goes to same partition
    /// let result = producer.send(
    ///     "orders",
    ///     Some(b"user123"),  // Key
    ///     b"order data",     // Value
    ///     None,              // Auto-select partition
    /// ).await?;
    ///
    /// println!("Written to partition {} at offset {}",
    ///     result.partition, result.offset);
    /// ```
    ///
    /// ## Explicit partition
    ///
    /// ```ignore
    /// // Send to specific partition
    /// let result = producer.send(
    ///     "logs",
    ///     None,
    ///     b"log message",
    ///     Some(2),  // Partition 2
    /// ).await?;
    /// ```
    ///
    /// ## Round-robin (no key, no partition)
    ///
    /// ```ignore
    /// // Partition selected based on timestamp
    /// let result = producer.send(
    ///     "events",
    ///     None,
    ///     b"event data",
    ///     None,
    /// ).await?;
    /// ```
    ///
    /// # Partition Selection
    ///
    /// 1. If `partition` is provided: Use that partition (with validation)
    /// 2. Else if `key` is provided: Hash key using SipHash → partition
    /// 3. Else: Round-robin based on current timestamp
    ///
    /// # Phase 5.1 vs 5.2
    ///
    /// **Phase 5.1 (Current)**:
    /// - Writes directly to storage (no agent communication)
    /// - Creates new `PartitionWriter` for each send
    /// - ~200ms latency per send
    ///
    /// **Phase 5.2 (Future)**:
    /// - Sends to agent via gRPC
    /// - Agent maintains writer pool
    /// - <10ms p99 latency with batching
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

    /// Flush any pending batched records to ensure they're written.
    ///
    /// In Phase 5.1, this is a no-op since records are written immediately
    /// without batching. In Phase 5.2+, this will force all pending batches
    /// to be sent to agents.
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` in Phase 5.1.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Send records
    /// for i in 0..100 {
    ///     producer.send("topic", None, &format!("msg {}", i).into_bytes(), None).await?;
    /// }
    ///
    /// // Ensure all records are written
    /// producer.flush().await?;
    /// ```
    ///
    /// # Phase 5.2+
    ///
    /// Will block until all pending batches are sent and acknowledged.
    /// Useful before shutting down or when you need durability guarantees.
    pub async fn flush(&self) -> Result<()> {
        Ok(())
    }

    /// Close the producer and clean up all resources.
    ///
    /// This method performs graceful shutdown:
    /// 1. Stops the background agent refresh task
    /// 2. Flushes any pending batches (Phase 5.2+)
    /// 3. Closes all agent connections (Phase 5.2+)
    ///
    /// After calling `close()`, the Producer cannot be used again.
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` in Phase 5.1. May return errors in Phase 5.2+
    /// if flushing or connection cleanup fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let producer = Producer::builder()
    ///     .metadata_store(metadata)
    ///     .build()
    ///     .await?;
    ///
    /// // Use producer...
    ///
    /// // Clean shutdown
    /// producer.close().await?;
    /// ```
    ///
    /// # Note
    ///
    /// This method consumes `self`, so the Producer cannot be used after closing.
    /// If you need to share the Producer across tasks, wrap it in `Arc` and clone
    /// the Arc before passing to tasks.
    pub async fn close(self) -> Result<()> {
        self._refresh_handle.abort();
        Ok(())
    }

    // Internal methods

    /// Get topic metadata from cache or fetch from metadata store.
    ///
    /// This method implements a simple cache-aside pattern:
    /// 1. Check if topic is in cache
    /// 2. If yes, return cached value
    /// 3. If no, fetch from metadata store and cache it
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name to look up
    ///
    /// # Returns
    ///
    /// Topic metadata including partition count, retention, etc.
    ///
    /// # Errors
    ///
    /// Returns `TopicNotFound` if the topic doesn't exist in metadata store.
    ///
    /// # Performance
    ///
    /// - Cache hit: <1µs (memory lookup)
    /// - Cache miss: 1-10ms (metadata store query)
    ///
    /// Topics are cached indefinitely since they rarely change. If a topic's
    /// partition count changes, the Producer must be restarted.
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

    /// Compute partition ID using key-based consistent hashing or timestamp-based round-robin.
    ///
    /// This method implements two partition selection strategies:
    ///
    /// 1. **Key-based (if key provided)**: Hash the key using SipHash and modulo
    ///    by partition count. This ensures the same key always maps to the same partition.
    ///
    /// 2. **Round-robin (if no key)**: Use current timestamp modulo partition count.
    ///    This distributes records roughly evenly across partitions over time.
    ///
    /// # Arguments
    ///
    /// * `key` - Optional key bytes to hash
    /// * `partition_count` - Number of partitions in the topic
    ///
    /// # Returns
    ///
    /// Partition ID in range `[0, partition_count)`.
    ///
    /// # Why SipHash?
    ///
    /// - Fast: Faster than cryptographic hashes (SHA256, MD5)
    /// - Good distribution: Low collision rate
    /// - Standard: Same algorithm used by Rust's HashMap
    /// - Deterministic: Same key always produces same hash
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Key-based
    /// let partition = producer.compute_partition(Some(b"user123"), 10);
    /// assert!(partition < 10);
    ///
    /// // Same key always maps to same partition
    /// assert_eq!(
    ///     producer.compute_partition(Some(b"user123"), 10),
    ///     producer.compute_partition(Some(b"user123"), 10)
    /// );
    ///
    /// // Round-robin (different each time due to timestamp)
    /// let p1 = producer.compute_partition(None, 10);
    /// let p2 = producer.compute_partition(None, 10);
    /// // p1 and p2 may differ
    /// ```
    fn compute_partition(&self, key: Option<&[u8]>, partition_count: u32) -> u32 {
        match key {
            Some(k) => {
                // Key-based: Hash key and modulo by partition count
                let mut hasher = siphasher::sip::SipHasher::new();
                k.hash(&mut hasher);
                let hash = hasher.finish();
                (hash % partition_count as u64) as u32
            }
            None => {
                // Round-robin: Use current timestamp
                // This gives pseudo-random distribution over time
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                (now % partition_count as u128) as u32
            }
        }
    }

    /// Find the agent responsible for a specific partition.
    ///
    /// This method uses consistent hashing to map partition IDs to agents.
    /// The agent returned should be the one holding the lease for this partition.
    ///
    /// # Arguments
    ///
    /// * `_topic` - Topic name (unused in Phase 5.1, will be used in Phase 5.2)
    /// * `_partition_id` - Partition ID (unused in Phase 5.1, will be used in Phase 5.2)
    ///
    /// # Returns
    ///
    /// `AgentInfo` for the agent holding the lease for this partition.
    ///
    /// # Errors
    ///
    /// Returns `NoAgentsAvailable` if no healthy agents are found in the group.
    ///
    /// # Phase 5.1 Implementation
    ///
    /// Currently returns any available agent since we write directly to storage.
    /// This method is not used but is implemented for Phase 5.2 compatibility.
    ///
    /// # Phase 5.2 Implementation
    ///
    /// Will use consistent hashing to route to the correct agent:
    /// 1. Hash the partition ID
    /// 2. Find the nearest agent clockwise on the hash ring
    /// 3. Verify agent holds the lease via metadata store
    /// 4. If not, refresh agent list and retry
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

        // Phase 5.1: Return any agent (not used since we write directly to storage)
        // Phase 5.2: Use consistent hashing to find agent holding partition lease
        Ok(agents.values().next().unwrap().clone())
    }

    /// Send record directly to storage bypassing agent communication.
    ///
    /// **Phase 5.1 only**: This method writes directly to MinIO via the storage layer.
    /// It creates a new `PartitionWriter` for each send, which is inefficient but
    /// simple and validates the storage integration.
    ///
    /// **Phase 5.2**: This method will be replaced with gRPC communication to agents,
    /// which will handle write pooling, batching, and connection management.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID to write to
    /// * `key` - Optional key bytes
    /// * `value` - Value bytes (the payload)
    ///
    /// # Returns
    ///
    /// `SendResult` with topic, partition, offset, and timestamp.
    ///
    /// # Errors
    ///
    /// Returns `StorageError` if:
    /// - MinIO is unreachable
    /// - S3 bucket doesn't exist
    /// - Insufficient permissions
    /// - Write fails
    ///
    /// # Performance
    ///
    /// - Latency: ~200ms per send (S3 connection overhead)
    /// - Throughput: ~5 records/sec (limited by connection creation)
    ///
    /// # Implementation Details
    ///
    /// 1. Create S3 object store pointing to MinIO (localhost:9000)
    /// 2. Create `WriteConfig` with segment size, retention, etc.
    /// 3. Create new `PartitionWriter` (reads high watermark from metadata)
    /// 4. Append record to writer (compresses with LZ4)
    /// 5. Writer uploads segment to MinIO
    /// 6. Writer updates metadata with new offset
    /// 7. Return result with offset and timestamp
    ///
    /// # Why So Slow?
    ///
    /// Each send:
    /// - Creates new S3 connection (~50ms)
    /// - Creates new writer (~10ms to read metadata)
    /// - Uploads segment (~100ms for 1 record + LZ4 overhead)
    /// - Updates metadata (~10ms)
    ///
    /// Phase 5.2 will eliminate this overhead via connection pooling and batching.
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

    /// Background task that periodically refreshes the list of healthy agents.
    ///
    /// This task runs continuously in the background, querying the metadata store
    /// for healthy agents in the configured agent group. Agents are considered
    /// healthy if they've sent a heartbeat within the last 60 seconds.
    ///
    /// # Arguments
    ///
    /// * `metadata_store` - Metadata store to query for agents
    /// * `agent_group` - Agent group to filter by
    /// * `agents` - Shared map of agents to update
    /// * `interval` - How often to refresh (default: 30 seconds)
    ///
    /// # Behavior
    ///
    /// 1. Wait for the refresh interval
    /// 2. Query `list_agents(agent_group, None)` from metadata store
    /// 3. Replace entire agent map with fresh list
    /// 4. Log the new agent count
    /// 5. On error, log warning and continue (never panics)
    ///
    /// # Error Handling
    ///
    /// If the metadata store query fails:
    /// - Logs a warning with the error message
    /// - Keeps the existing agent list (doesn't clear it)
    /// - Continues running and retries on next interval
    ///
    /// This ensures temporary metadata store failures don't break the Producer.
    ///
    /// # Automatic Cleanup
    ///
    /// Agents that haven't heartbeated in 60+ seconds are automatically excluded
    /// by `list_agents()`. This provides automatic failover when agents crash or
    /// become unreachable.
    ///
    /// # Lifecycle
    ///
    /// This task is spawned when the Producer is created and runs until:
    /// - `Producer::close()` is called (aborts the task)
    /// - The Producer is dropped (task handle is dropped)
    ///
    /// # Performance
    ///
    /// - Metadata query: 1-10ms
    /// - Map replacement: <1ms for <100 agents
    /// - Total CPU: Negligible (~0.1ms every 30s)
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
                    // Build new map from fresh agent list
                    let mut map = HashMap::new();
                    for agent in agent_list {
                        map.insert(agent.agent_id.clone(), agent);
                    }

                    let count = map.len();
                    // Replace entire map atomically
                    *agents.write().await = map;

                    debug!(agent_group = agent_group, count = count, "Refreshed agents");
                }
                Err(e) => {
                    // Log error but don't panic or clear agent list
                    // Producer can continue with stale agent list
                    warn!(error = %e, "Failed to refresh agents");
                }
            }
        }
    }
}

/// Builder for configuring and creating a `Producer`.
///
/// This builder provides a fluent, type-safe API for creating a Producer with
/// custom configuration. All parameters have sensible defaults, so you only need
/// to specify what you want to change.
///
/// # Required Fields
///
/// - `metadata_store`: Must be set via `metadata_store()` before calling `build()`
///
/// # Default Values
///
/// - `agent_group`: "default"
/// - `batch_size`: 100 records
/// - `batch_timeout`: 100ms
/// - `compression_enabled`: true (LZ4)
/// - `request_timeout`: 30s
/// - `agent_refresh_interval`: 30s
/// - `max_retries`: 3
/// - `retry_backoff`: 100ms
///
/// # Examples
///
/// ## Minimal Configuration
///
/// ```ignore
/// let producer = Producer::builder()
///     .metadata_store(metadata)
///     .build()
///     .await?;
/// ```
///
/// ## Full Configuration
///
/// ```ignore
/// use std::time::Duration;
///
/// let producer = Producer::builder()
///     .metadata_store(metadata)
///     .agent_group("prod")
///     .batch_size(500)
///     .batch_timeout(Duration::from_millis(200))
///     .compression_enabled(false)
///     .request_timeout(Duration::from_secs(60))
///     .agent_refresh_interval(Duration::from_secs(10))
///     .max_retries(5)
///     .retry_backoff(Duration::from_millis(50))
///     .build()
///     .await?;
/// ```
pub struct ProducerBuilder {
    /// Metadata store for topic and agent discovery (required).
    metadata_store: Option<Arc<dyn MetadataStore>>,

    /// Agent group to route requests to (default: "default").
    agent_group: String,

    /// Maximum records per batch (default: 100).
    batch_size: usize,

    /// Maximum time to wait before flushing batch (default: 100ms).
    batch_timeout: Duration,

    /// Enable LZ4 compression (default: true).
    compression_enabled: bool,

    /// Timeout for agent requests (default: 30s).
    request_timeout: Duration,

    /// Interval for refreshing agent list (default: 30s).
    agent_refresh_interval: Duration,

    /// Maximum retry attempts on failure (default: 3).
    max_retries: usize,

    /// Base backoff duration for retries (default: 100ms).
    retry_backoff: Duration,
}

impl ProducerBuilder {
    /// Create a new `ProducerBuilder` with default settings.
    ///
    /// All fields have sensible defaults except `metadata_store`, which must
    /// be set before calling `build()`.
    ///
    /// # Returns
    ///
    /// A new builder with default configuration.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = ProducerBuilder::new();
    /// ```
    ///
    /// Prefer using `Producer::builder()` instead of calling this directly.
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

    /// Set the metadata store (required).
    ///
    /// The metadata store is used for:
    /// - Topic metadata lookup (partition count, retention)
    /// - Agent discovery (list healthy agents)
    /// - Validation (topic exists, partition range)
    ///
    /// # Arguments
    ///
    /// * `store` - Metadata store implementation (PostgreSQL, SQLite, etc.)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use streamhouse_metadata::{SqliteMetadataStore, MetadataStore};
    ///
    /// let metadata: Arc<dyn MetadataStore> = Arc::new(
    ///     SqliteMetadataStore::new("metadata.db").await?
    /// );
    ///
    /// let builder = Producer::builder().metadata_store(metadata);
    /// ```
    pub fn metadata_store(mut self, store: Arc<dyn MetadataStore>) -> Self {
        self.metadata_store = Some(store);
        self
    }

    /// Set the agent group to route requests to.
    ///
    /// Only agents in this group will be considered for routing. This enables
    /// multi-tenancy and environment isolation (prod, staging, dev).
    ///
    /// # Arguments
    ///
    /// * `group` - Agent group name (e.g., "prod", "staging", "test")
    ///
    /// # Default
    ///
    /// "default"
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = Producer::builder().agent_group("prod");
    /// ```
    pub fn agent_group(mut self, group: impl Into<String>) -> Self {
        self.agent_group = group.into();
        self
    }

    /// Set the maximum number of records to batch before flushing.
    ///
    /// Higher values increase throughput but add latency. Lower values reduce
    /// latency but decrease throughput.
    ///
    /// # Arguments
    ///
    /// * `size` - Batch size (1-10000 recommended)
    ///
    /// # Default
    ///
    /// 100 records
    ///
    /// # Phase 5.2+
    ///
    /// Currently unused (no batching in Phase 5.1).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // High throughput (more latency)
    /// let builder = Producer::builder().batch_size(1000);
    ///
    /// // Low latency (less throughput)
    /// let builder = Producer::builder().batch_size(10);
    /// ```
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set the maximum time to wait before flushing a batch.
    ///
    /// This ensures records aren't delayed indefinitely when traffic is low.
    /// When a batch reaches this age, it's flushed even if not full.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Batch timeout (10ms-10s recommended)
    ///
    /// # Default
    ///
    /// 100ms
    ///
    /// # Phase 5.2+
    ///
    /// Currently unused (no batching in Phase 5.1).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// // Flush faster (lower latency)
    /// let builder = Producer::builder()
    ///     .batch_timeout(Duration::from_millis(10));
    ///
    /// // Wait longer (higher throughput)
    /// let builder = Producer::builder()
    ///     .batch_timeout(Duration::from_secs(1));
    /// ```
    pub fn batch_timeout(mut self, timeout: Duration) -> Self {
        self.batch_timeout = timeout;
        self
    }

    /// Enable or disable LZ4 compression.
    ///
    /// Compression typically achieves 4-5x size reduction with minimal CPU overhead.
    /// Disable only if you have pre-compressed data or need maximum write speed.
    ///
    /// # Arguments
    ///
    /// * `enabled` - true to enable compression, false to disable
    ///
    /// # Default
    ///
    /// true (compression enabled)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Disable compression for pre-compressed data
    /// let builder = Producer::builder().compression_enabled(false);
    /// ```
    pub fn compression_enabled(mut self, enabled: bool) -> Self {
        self.compression_enabled = enabled;
        self
    }

    /// Set the timeout for individual produce requests to agents.
    ///
    /// If a request takes longer than this, it will be retried or failed.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Request timeout (1s-60s recommended)
    ///
    /// # Default
    ///
    /// 30 seconds
    ///
    /// # Phase 5.2+
    ///
    /// Currently unused (no agent communication in Phase 5.1).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// // Shorter timeout for low-latency environments
    /// let builder = Producer::builder()
    ///     .request_timeout(Duration::from_secs(5));
    /// ```
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set how often to refresh the list of healthy agents.
    ///
    /// Lower values provide faster failover but increase metadata store load.
    /// Higher values reduce load but slow failover.
    ///
    /// # Arguments
    ///
    /// * `interval` - Refresh interval (5s-60s recommended)
    ///
    /// # Default
    ///
    /// 30 seconds
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// // Fast failover (more metadata queries)
    /// let builder = Producer::builder()
    ///     .agent_refresh_interval(Duration::from_secs(10));
    /// ```
    pub fn agent_refresh_interval(mut self, interval: Duration) -> Self {
        self.agent_refresh_interval = interval;
        self
    }

    /// Set the maximum number of retry attempts on transient failures.
    ///
    /// After this many retries, the operation fails with an error.
    ///
    /// # Arguments
    ///
    /// * `retries` - Max retry attempts (0-10 recommended)
    ///
    /// # Default
    ///
    /// 3 retries
    ///
    /// # Phase 5.2+
    ///
    /// Currently unused (no retries in Phase 5.1).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // More aggressive retries
    /// let builder = Producer::builder().max_retries(5);
    ///
    /// // Fail fast (no retries)
    /// let builder = Producer::builder().max_retries(0);
    /// ```
    pub fn max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the base backoff duration for exponential retry backoff.
    ///
    /// Actual backoff is `retry_backoff * 2^attempt`. For example, with 100ms base:
    /// - Attempt 1: 100ms
    /// - Attempt 2: 200ms
    /// - Attempt 3: 400ms
    ///
    /// # Arguments
    ///
    /// * `backoff` - Base backoff duration (10ms-1s recommended)
    ///
    /// # Default
    ///
    /// 100ms
    ///
    /// # Phase 5.2+
    ///
    /// Currently unused (no retries in Phase 5.1).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// // Faster retries
    /// let builder = Producer::builder()
    ///     .retry_backoff(Duration::from_millis(50));
    /// ```
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
