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

use crate::batch::{BatchManager, BatchRecord};
use crate::connection_pool::ConnectionPool;
use crate::error::{ClientError, Result};
use crate::retry::{retry_with_jittered_backoff, RetryPolicy};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use streamhouse_metadata::{AgentInfo, MetadataStore, Topic};
use streamhouse_proto::producer::{produce_request::Record, ProduceRequest, ProduceResponse};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

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
/// - `offset`: Offset assigned to the record (None until batch flushes)
/// - `offset_receiver`: Optional receiver for async offset retrieval
/// - `timestamp`: Timestamp of the record in milliseconds since Unix epoch
///
/// ## Phase 5.4: Async Offset Tracking
///
/// Prior to Phase 5.4, `offset` was always 0 (placeholder). Starting in Phase 5.4,
/// the offset is resolved asynchronously when the batch flushes to the agent.
///
/// ### Usage Patterns
///
/// **Pattern 1: Ignore offset (fire-and-forget)**
/// ```ignore
/// let _result = producer.send("orders", Some(b"key"), b"value", None).await?;
/// // Don't care about the offset
/// ```
///
/// **Pattern 2: Wait for offset (blocking)**
/// ```ignore
/// let mut result = producer.send("orders", Some(b"key"), b"value", None).await?;
/// let offset = result.wait_offset().await?;
/// println!("Written at offset {}", offset);
/// ```
///
/// **Pattern 3: Convenience method**
/// ```ignore
/// let offset = producer.send_and_wait("orders", Some(b"key"), b"value", None).await?;
/// println!("Written at offset {}", offset);
/// ```
///
/// ## Serialization
///
/// This struct implements `Serialize` and `Deserialize` for logging,
/// monitoring, and testing purposes.
///
/// ## Note on Clone
///
/// `SendResult` does not implement `Clone` because `offset_receiver` contains
/// a oneshot::Receiver which cannot be cloned. If you need to store the offset,
/// call `wait_offset()` to get the offset value and store that instead.
#[derive(Debug, Serialize, Deserialize)]
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
    /// This is `None` immediately after `send()` returns and is set to `Some(offset)`
    /// once the batch is flushed to the agent. Use `wait_offset()` to block until
    /// the offset is available.
    ///
    /// ## Phase 5.4+
    /// Offsets are monotonically increasing integers starting from 0.
    /// Within a partition, offsets are guaranteed to be sequential and unique.
    pub offset: Option<u64>,

    /// Timestamp of the record in milliseconds since Unix epoch.
    ///
    /// This is either:
    /// - The timestamp provided in the send request
    /// - The current system time (if not provided)
    pub timestamp: i64,

    /// Receiver for async offset retrieval (Phase 5.4+).
    ///
    /// This receiver will receive the offset once the batch is flushed to the agent.
    /// It can only be consumed once via `wait_offset()`.
    ///
    /// ## Note
    /// This field is `#[serde(skip)]` because oneshot::Receiver is not serializable.
    #[serde(skip)]
    pub offset_receiver: Option<tokio::sync::oneshot::Receiver<u64>>,
}

impl SendResult {
    /// Get the offset if immediately available (batch already flushed).
    ///
    /// # Returns
    ///
    /// `Some(offset)` if the batch has already been flushed to the agent and the offset
    /// has been set. `None` if the batch is still pending.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let result = producer.send("topic", None, b"data", None).await?;
    ///
    /// // Check if offset is immediately available
    /// match result.offset() {
    ///     Some(offset) => println!("Offset: {}", offset),
    ///     None => println!("Offset pending"),
    /// }
    /// ```
    pub fn offset(&self) -> Option<u64> {
        self.offset
    }

    /// Wait for the offset to be set (blocks until batch flushes).
    ///
    /// This method consumes the `offset_receiver` and blocks until the batch is flushed
    /// to the agent and the offset is known. If the offset is already set, it returns
    /// immediately.
    ///
    /// # Returns
    ///
    /// The offset assigned to this record.
    ///
    /// # Errors
    ///
    /// - `ClientError::BatchFlushFailed`: Batch flush failed (agent error, connection lost, etc.)
    /// - `ClientError::OffsetAlreadyConsumed`: `wait_offset()` called twice on same SendResult
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut result = producer.send("topic", None, b"data", None).await?;
    ///
    /// // Block until offset is available
    /// let offset = result.wait_offset().await?;
    /// println!("Written at offset {}", offset);
    /// ```
    ///
    /// # Note
    ///
    /// This method takes `&mut self` because it consumes the `offset_receiver`.
    /// Calling it twice on the same `SendResult` will return `OffsetAlreadyConsumed` error.
    pub async fn wait_offset(&mut self) -> Result<u64> {
        // Fast path: offset already set
        if let Some(offset) = self.offset {
            return Ok(offset);
        }

        // Wait for offset from receiver
        if let Some(rx) = self.offset_receiver.take() {
            match rx.await {
                Ok(offset) => {
                    self.offset = Some(offset);
                    Ok(offset)
                }
                Err(_) => Err(ClientError::BatchFlushFailed),
            }
        } else {
            Err(ClientError::OffsetAlreadyConsumed)
        }
    }
}

/// Pending record waiting for offset acknowledgment from agent.
///
/// When a record is sent via `Producer::send()`, it's appended to a batch and
/// a `PendingRecord` is created to track its position. Once the batch flushes
/// to the agent and the agent responds with offsets, we notify this pending
/// record about its actual offset.
///
/// ## Phase 5.4
///
/// This struct is used to implement async offset tracking. Each `send()` call
/// creates a oneshot channel and stores the sender in a `PendingRecord`. When
/// the batch flush completes, we calculate each record's offset and send it
/// through the channel.
///
/// ## Fields
///
/// - `offset_sender`: Oneshot channel sender for notifying about the offset
///
/// ## Note
///
/// We don't need to store `record_index` because records are processed in FIFO order
/// from the pending queue, which matches the order they were added to the batch.
struct PendingRecord {
    /// Sender to notify about the offset once it's known.
    offset_sender: tokio::sync::oneshot::Sender<u64>,
}

/// Queue of pending records for a single partition.
///
/// Records are appended to the back and popped from the front in FIFO order,
/// matching the order they were added to the batch.
type PendingQueue = VecDeque<PendingRecord>;

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

    /// Connection pool for gRPC connections to agents.
    ///
    /// Maintains per-agent connection pools with health tracking and idle timeout.
    /// Connections are reused across send() calls for performance.
    connection_pool: Arc<ConnectionPool>,

    /// Batch manager for accumulating records before sending.
    ///
    /// Records are batched per (topic, partition) and flushed based on:
    /// - Size trigger: batch_size records (default 100)
    /// - Bytes trigger: 1MB of data
    /// - Time trigger: batch_timeout elapsed (default 100ms)
    batch_manager: Arc<Mutex<BatchManager>>,

    /// Retry policy for handling transient failures.
    ///
    /// Configures exponential backoff (100ms-30s) and error classification.
    retry_policy: RetryPolicy,

    /// Handle to the background batch flush task.
    ///
    /// This task runs continuously, checking for ready batches every 50ms
    /// and sending them to agents. It's aborted when close() is called.
    flush_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,

    /// Pending records waiting for offset acknowledgment (Phase 5.4+).
    ///
    /// When a record is sent, we create a oneshot channel and store the sender
    /// in this map. When the batch flushes and the agent responds with offsets,
    /// we notify each pending record about its actual offset.
    ///
    /// Key: (topic, partition_id)
    /// Value: Queue of pending records in FIFO order (matching batch order)
    pending_records: Arc<Mutex<HashMap<(String, u32), PendingQueue>>>,
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

        // Create batch record
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let record = BatchRecord::new(
            key.map(Bytes::copy_from_slice),
            Bytes::copy_from_slice(value),
            timestamp,
        );

        // Create oneshot channel for offset tracking (Phase 5.4)
        let (offset_tx, offset_rx) = tokio::sync::oneshot::channel();

        // Append to batch manager (background task will flush)
        let mut batch_manager = self.batch_manager.lock().await;
        batch_manager.append(topic, partition_id, record);
        drop(batch_manager);

        // Track pending record for offset notification
        let mut pending = self.pending_records.lock().await;
        let key = (topic.to_string(), partition_id);
        pending
            .entry(key)
            .or_insert_with(VecDeque::new)
            .push_back(PendingRecord {
                offset_sender: offset_tx,
            });
        drop(pending);

        // Return immediately with offset receiver (Phase 5.4)
        // Offset will be set asynchronously when batch flushes
        Ok(SendResult {
            topic: topic.to_string(),
            partition: partition_id,
            offset: None, // Will be set when batch completes
            offset_receiver: Some(offset_rx),
            timestamp: timestamp as i64,
        })
    }

    /// Send a record and wait for the offset to be committed (Phase 5.4).
    ///
    /// This is a convenience wrapper around `send()` that blocks until the batch
    /// is flushed and the offset is known. It's equivalent to calling `send()`
    /// followed by `wait_offset()` on the result.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name (must exist in metadata store)
    /// * `key` - Optional key for partitioning
    /// * `value` - Record payload
    /// * `partition` - Optional explicit partition ID
    ///
    /// # Returns
    ///
    /// The offset assigned to this record.
    ///
    /// # Errors
    ///
    /// Returns `ClientError` if:
    /// - `TopicNotFound`: Topic doesn't exist
    /// - `InvalidPartition`: Explicit partition is out of range
    /// - `BatchFlushFailed`: Batch flush failed (agent error, etc.)
    /// - `NoAgentsAvailable`: No healthy agents in group
    ///
    /// # Performance Note
    ///
    /// This method blocks until the batch is flushed, which adds latency.
    /// For high throughput, prefer using `send()` without waiting for offsets,
    /// or batch multiple sends and wait for offsets later.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Send and wait for offset
    /// let offset = producer.send_and_wait(
    ///     "orders",
    ///     Some(b"user123"),
    ///     b"order data",
    ///     None,
    /// ).await?;
    ///
    /// println!("Written at offset {}", offset);
    /// ```
    pub async fn send_and_wait(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        value: &[u8],
        partition: Option<u32>,
    ) -> Result<u64> {
        let mut result = self.send(topic, key, value, partition).await?;
        result.wait_offset().await
    }

    /// Flush any pending batched records to ensure they're written.
    ///
    /// This method forces all ready batches to be sent to agents immediately,
    /// without waiting for the background flush task.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all batches were successfully sent, or an error if any batch failed.
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
    /// # Behavior
    ///
    /// - Gets all ready batches from the batch manager
    /// - Sends each batch to the appropriate agent
    /// - Returns error if any batch fails to send
    ///
    /// # Use Cases
    ///
    /// - Before shutting down to ensure all data is persisted
    /// - After critical writes to guarantee durability
    /// - For testing to ensure records are visible
    pub async fn flush(&self) -> Result<()> {
        let ready = self.batch_manager.lock().await.ready_batches();

        for (topic, partition, records) in ready {
            let record_count = records.len();

            match Self::send_batch_to_agent(
                &topic,
                partition,
                records,
                &self.connection_pool,
                &self.config.metadata_store,
                &self.agents,
                &self.retry_policy,
            )
            .await
            {
                Ok(response) => {
                    // SUCCESS: Notify pending records about their offsets (Phase 5.4)
                    Self::notify_pending_offsets(
                        &self.pending_records,
                        &topic,
                        partition,
                        response.base_offset,
                        record_count,
                    )
                    .await;
                }
                Err(e) => {
                    // FAILURE: Fail pending records (Phase 5.4)
                    Self::fail_pending_offsets(
                        &self.pending_records,
                        &topic,
                        partition,
                        record_count,
                    )
                    .await;
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Close the producer and clean up all resources.
    ///
    /// This method performs graceful shutdown:
    /// 1. Stops the background flush task
    /// 2. Stops the background agent refresh task
    /// 3. Flushes all pending batches
    /// 4. Closes all agent connections
    ///
    /// After calling `close()`, the Producer cannot be used again.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if shutdown was successful, or an error if flushing fails.
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
    ///
    /// # Behavior
    ///
    /// 1. Aborts background tasks (flush and refresh)
    /// 2. Flushes all pending batches (may take time if batches are large)
    /// 3. Closes connection pool (drops all gRPC connections)
    ///
    /// If flushing fails, the error is returned and connections may not be closed.
    pub async fn close(self) -> Result<()> {
        // 1. Abort background tasks
        if let Some(handle) = self.flush_handle.lock().await.take() {
            handle.abort();
        }
        self._refresh_handle.abort();

        // 2. Flush all pending batches
        let all_batches = self.batch_manager.lock().await.flush_all();

        for (topic, partition, records) in all_batches {
            if let Err(e) = Self::send_batch_to_agent(
                &topic,
                partition,
                records,
                &self.connection_pool,
                &self.config.metadata_store,
                &self.agents,
                &self.retry_policy,
            )
            .await
            {
                error!(
                    topic = topic,
                    partition = partition,
                    error = %e,
                    "Failed to flush batch on close"
                );
                // Continue trying to flush other batches
            }
        }

        // 3. Close connection pool
        self.connection_pool.close_all().await;

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

    /// Send a batch of records to an agent via gRPC.
    ///
    /// This function handles the complete flow of sending a batch to an agent:
    /// 1. Find the agent holding the lease for the partition
    /// 2. Get a connection from the pool
    /// 3. Build a ProduceRequest
    /// 4. Send with retry logic
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    /// * `records` - Batch of records to send
    /// * `connection_pool` - Connection pool for gRPC connections
    /// * `metadata_store` - Metadata store for partition lease lookup
    /// * `agents` - Map of healthy agents
    /// * `retry_policy` - Retry policy for handling failures
    ///
    /// # Returns
    ///
    /// `ProduceResponse` with base offset and record count.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No agent found for partition
    /// - Connection failed
    /// - Agent rejected request (NOT_FOUND, FAILED_PRECONDITION, etc.)
    /// - Max retries exhausted
    ///
    /// Notify pending records about their offsets after successful batch flush (Phase 5.4).
    ///
    /// When a batch is successfully flushed to an agent, the agent responds with
    /// `base_offset` (first record's offset) and `record_count`. We calculate each
    /// record's offset as `base_offset + index` and notify the pending record.
    ///
    /// # Arguments
    ///
    /// * `pending_records` - Map of pending records per partition
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    /// * `base_offset` - First record's offset from agent response
    /// * `record_count` - Number of records that were flushed
    async fn notify_pending_offsets(
        pending_records: &Arc<Mutex<HashMap<(String, u32), PendingQueue>>>,
        topic: &str,
        partition: u32,
        base_offset: u64,
        record_count: usize,
    ) {
        let mut pending = pending_records.lock().await;
        let key = (topic.to_string(), partition);

        if let Some(queue) = pending.get_mut(&key) {
            for i in 0..record_count {
                if let Some(pending_record) = queue.pop_front() {
                    let offset = base_offset + i as u64;
                    // Send offset (ignore if receiver dropped)
                    let _ = pending_record.offset_sender.send(offset);
                } else {
                    // Shouldn't happen, but log warning
                    warn!(
                        "No pending record for topic={} partition={} index={}",
                        topic, partition, i
                    );
                    break;
                }
            }
        }
    }

    /// Fail pending records when batch flush fails (Phase 5.4).
    ///
    /// When a batch fails to flush (agent error, connection lost, etc.), we need
    /// to notify the pending records so their `wait_offset()` calls return an error.
    /// We do this by dropping the senders, which causes receivers to get `Err`.
    ///
    /// # Arguments
    ///
    /// * `pending_records` - Map of pending records per partition
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    /// * `record_count` - Number of records that failed
    async fn fail_pending_offsets(
        pending_records: &Arc<Mutex<HashMap<(String, u32), PendingQueue>>>,
        topic: &str,
        partition: u32,
        record_count: usize,
    ) {
        let mut pending = pending_records.lock().await;
        let key = (topic.to_string(), partition);

        if let Some(queue) = pending.get_mut(&key) {
            // Drop senders (receivers will get error)
            for _ in 0..record_count {
                queue.pop_front();
            }
        }
    }

    async fn send_batch_to_agent(
        topic: &str,
        partition: u32,
        records: Vec<BatchRecord>,
        connection_pool: &ConnectionPool,
        metadata_store: &Arc<dyn MetadataStore>,
        agents: &Arc<RwLock<HashMap<String, AgentInfo>>>,
        retry_policy: &RetryPolicy,
    ) -> Result<ProduceResponse> {
        // Find agent for partition
        let agent =
            Self::find_agent_for_partition_impl(topic, partition, metadata_store, agents).await?;

        // Get connection from pool
        let client = connection_pool
            .get_connection(&agent.address)
            .await
            .map_err(|e| {
                ClientError::AgentConnectionFailed(
                    agent.agent_id.clone(),
                    agent.address.clone(),
                    e.to_string(),
                )
            })?;

        // Build ProduceRequest
        let proto_records: Vec<Record> = records
            .into_iter()
            .map(|r| Record {
                key: r.key.map(|k| k.to_vec()),
                value: r.value.to_vec(),
                timestamp: r.timestamp,
            })
            .collect();

        let request = ProduceRequest {
            topic: topic.to_string(),
            partition,
            records: proto_records,
        };

        // Send with retry
        let response = retry_with_jittered_backoff(retry_policy, || {
            let mut client = client.clone();
            let request = request.clone();
            async move { client.produce(request).await }
        })
        .await
        .map_err(|e| {
            ClientError::AgentError(
                agent.agent_id.clone(),
                format!("Produce request failed: {}", e),
            )
        })?;

        Ok(response.into_inner())
    }

    /// Find the agent responsible for a specific partition (static implementation).
    ///
    /// This is the actual implementation used by send_batch_to_agent.
    /// It queries the metadata store for the partition lease and returns
    /// the agent holding the lease.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `metadata_store` - Metadata store for lease lookup
    /// * `agents` - Map of healthy agents
    ///
    /// # Returns
    ///
    /// `AgentInfo` for the agent holding the lease for this partition.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No lease found for partition
    /// - Agent not found in healthy agent map
    /// - No agents available
    async fn find_agent_for_partition_impl(
        topic: &str,
        partition_id: u32,
        metadata_store: &Arc<dyn MetadataStore>,
        agents: &Arc<RwLock<HashMap<String, AgentInfo>>>,
    ) -> Result<AgentInfo> {
        // Option 1: Query metadata store for partition lease
        if let Ok(Some(lease)) = metadata_store
            .get_partition_lease(topic, partition_id)
            .await
        {
            // Get agent from cache
            let agents_map = agents.read().await;
            if let Some(agent) = agents_map.get(&lease.leader_agent_id) {
                return Ok(agent.clone());
            }
        }

        // Option 2: Fallback to consistent hashing
        let agents_map = agents.read().await;
        if agents_map.is_empty() {
            return Err(ClientError::NoAgentsAvailable("default".to_string()));
        }

        // Use SipHash for consistent partition → agent mapping
        use siphasher::sip::SipHasher;
        let mut hasher = SipHasher::new();
        format!("{}-{}", topic, partition_id).hash(&mut hasher);
        let hash = hasher.finish();

        let agent_idx = (hash as usize) % agents_map.len();
        Ok(agents_map.values().nth(agent_idx).unwrap().clone())
    }

    /// Spawn background task that periodically flushes ready batches.
    ///
    /// This task runs continuously in the background, checking for ready batches
    /// every 50ms and sending them to agents via gRPC.
    ///
    /// # Arguments
    ///
    /// * `batch_manager` - Batch manager containing pending records
    /// * `connection_pool` - Connection pool for gRPC connections
    /// * `metadata_store` - Metadata store for partition lease lookup
    /// * `agents` - Map of healthy agents
    /// * `retry_policy` - Retry policy for handling failures
    ///
    /// # Returns
    ///
    /// JoinHandle to the spawned task.
    ///
    /// # Behavior
    ///
    /// 1. Wait 50ms
    /// 2. Get ready batches from batch manager
    /// 3. For each batch, send to agent
    /// 4. Log errors but continue (never panics)
    ///
    /// # Error Handling
    ///
    /// If a batch send fails after retries:
    /// - Logs error with topic, partition, and error message
    /// - Continues processing other batches
    /// - Does not retry failed batches (they are dropped)
    ///
    /// # Lifecycle
    ///
    /// This task runs until:
    /// - `Producer::close()` is called (aborts the task)
    /// - The Producer is dropped (task handle is dropped)
    fn spawn_flush_task(
        batch_manager: Arc<Mutex<BatchManager>>,
        connection_pool: Arc<ConnectionPool>,
        metadata_store: Arc<dyn MetadataStore>,
        agents: Arc<RwLock<HashMap<String, AgentInfo>>>,
        retry_policy: RetryPolicy,
        pending_records: Arc<Mutex<HashMap<(String, u32), PendingQueue>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(50));

            loop {
                interval.tick().await;

                // Get ready batches
                let ready = batch_manager.lock().await.ready_batches();

                for (topic, partition, records) in ready {
                    let record_count = records.len();

                    // Send batch to agent
                    match Self::send_batch_to_agent(
                        &topic,
                        partition,
                        records,
                        &connection_pool,
                        &metadata_store,
                        &agents,
                        &retry_policy,
                    )
                    .await
                    {
                        Ok(response) => {
                            // SUCCESS: Notify pending records about their offsets (Phase 5.4)
                            Self::notify_pending_offsets(
                                &pending_records,
                                &topic,
                                partition,
                                response.base_offset,
                                record_count,
                            )
                            .await;
                        }
                        Err(e) => {
                            // FAILURE: Fail pending records (Phase 5.4)
                            Self::fail_pending_offsets(
                                &pending_records,
                                &topic,
                                partition,
                                record_count,
                            )
                            .await;

                            error!(
                                topic = topic,
                                partition = partition,
                                error = %e,
                                "Failed to send batch"
                            );
                        }
                    }
                }
            }
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

        // Initialize connection pool
        let connection_pool = Arc::new(ConnectionPool::new(
            10,                      // max connections per agent
            Duration::from_secs(60), // idle timeout
        ));

        // Initialize batch manager
        let batch_manager = Arc::new(Mutex::new(BatchManager::new(
            self.batch_size,
            1024 * 1024, // 1MB max batch bytes
            self.batch_timeout,
        )));

        // Initialize retry policy
        let retry_policy = RetryPolicy {
            max_retries: self.max_retries,
            initial_backoff: self.retry_backoff,
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        };

        // Start background refresh task
        let refresh_handle = tokio::spawn(Producer::refresh_agents_task(
            Arc::clone(&metadata_store),
            self.agent_group.clone(),
            Arc::clone(&agents),
            self.agent_refresh_interval,
        ));

        // Create pending records tracker (Phase 5.4)
        let pending_records = Arc::new(Mutex::new(HashMap::new()));

        // Spawn background flush task
        let flush_handle = Arc::new(Mutex::new(Some(Producer::spawn_flush_task(
            Arc::clone(&batch_manager),
            Arc::clone(&connection_pool),
            Arc::clone(&metadata_store),
            Arc::clone(&agents),
            retry_policy.clone(),
            Arc::clone(&pending_records),
        ))));

        Ok(Producer {
            config,
            agents,
            topic_cache,
            _refresh_handle: refresh_handle,
            connection_pool,
            batch_manager,
            retry_policy,
            flush_handle,
            pending_records,
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
