//! Consumer API for reading records from StreamHouse topics.
//!
//! This module provides a high-level Consumer API similar to Kafka's consumer,
//! with support for consumer groups, offset management, and multi-partition subscriptions.
//!
//! ## Architecture
//!
//! The Consumer reads directly from storage using PartitionReader (not via agents):
//! ```text
//! Consumer → PartitionReader → SegmentCache → S3
//!          → MetadataStore (offset commits, segment lookup)
//! ```
//!
//! ## Features
//!
//! - **Multi-partition subscription**: Subscribe to topics and automatically discover partitions
//! - **Consumer groups**: Multiple consumers coordinate via committed offsets
//! - **Offset management**: Manual and auto-commit with configurable intervals
//! - **Offset reset strategies**: Earliest, Latest, or None
//! - **High throughput**: Leverages PartitionReader's caching and prefetching

use crate::error::{ClientError, Result};
use bytes::Bytes;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::MetadataStore;
use streamhouse_storage::{PartitionReader, SegmentCache};
use tokio::sync::RwLock;

/// High-level consumer for reading records from StreamHouse topics.
///
/// The Consumer subscribes to one or more topics and automatically discovers
/// partitions. It manages offset tracking and commits for consumer groups.
///
/// ## Example
///
/// ```ignore
/// use streamhouse_client::{Consumer, OffsetReset};
/// use std::time::Duration;
///
/// let consumer = Consumer::builder()
///     .group_id("analytics")
///     .topics(vec!["orders".to_string()])
///     .metadata_store(metadata_store)
///     .object_store(object_store)
///     .offset_reset(OffsetReset::Earliest)
///     .build()
///     .await?;
///
/// loop {
///     let records = consumer.poll(Duration::from_secs(1)).await?;
///     for record in records {
///         println!("Received: {:?}", record);
///     }
///     consumer.commit().await?;
/// }
/// ```
///
/// Prometheus metrics for the Consumer.
///
/// Tracks throughput, latency, consumer lag, and offset positions.
/// All metrics use atomic operations for lock-free updates.
///
/// ## Metrics
///
/// - `records_consumed_total`: Total records consumed (counter)
/// - `bytes_consumed_total`: Total bytes consumed (counter)
/// - `poll_duration_seconds`: Latency of poll() calls (histogram)
/// - `consumer_lag_records`: Consumer lag in records (gauge)
/// - `consumer_lag_seconds`: Consumer lag in time (gauge)
/// - `last_committed_offset`: Last committed offset (gauge)
/// - `current_offset`: Current read position (gauge)
#[cfg(feature = "metrics")]
pub struct ConsumerMetrics {
    records_consumed_total: prometheus_client::metrics::counter::Counter<u64>,
    bytes_consumed_total: prometheus_client::metrics::counter::Counter<u64>,
    poll_duration_seconds: prometheus_client::metrics::histogram::Histogram,
    consumer_lag_records: prometheus_client::metrics::gauge::Gauge<i64>,
    consumer_lag_seconds: prometheus_client::metrics::gauge::Gauge<i64>,
    last_committed_offset: prometheus_client::metrics::gauge::Gauge<i64>,
    current_offset: prometheus_client::metrics::gauge::Gauge<i64>,
}

#[cfg(feature = "metrics")]
impl ConsumerMetrics {
    /// Create new ConsumerMetrics and register with the given registry.
    pub fn new(registry: &mut prometheus_client::registry::Registry) -> Self {
        let records_consumed = prometheus_client::metrics::counter::Counter::<u64>::default();
        let bytes_consumed = prometheus_client::metrics::counter::Counter::<u64>::default();

        let poll_duration = prometheus_client::metrics::histogram::Histogram::new(
            prometheus_client::metrics::histogram::exponential_buckets(0.001, 2.0, 15),
        );

        let lag_records = prometheus_client::metrics::gauge::Gauge::<i64>::default();
        let lag_seconds = prometheus_client::metrics::gauge::Gauge::<i64>::default();
        let last_committed = prometheus_client::metrics::gauge::Gauge::<i64>::default();
        let current = prometheus_client::metrics::gauge::Gauge::<i64>::default();

        registry.register(
            "streamhouse_consumer_records_consumed_total",
            "Total number of records consumed",
            records_consumed.clone(),
        );

        registry.register(
            "streamhouse_consumer_bytes_consumed_total",
            "Total bytes consumed",
            bytes_consumed.clone(),
        );

        registry.register(
            "streamhouse_consumer_poll_duration_seconds",
            "Duration of poll() calls in seconds",
            poll_duration.clone(),
        );

        registry.register(
            "streamhouse_consumer_lag_records",
            "Consumer lag in records",
            lag_records.clone(),
        );

        registry.register(
            "streamhouse_consumer_lag_seconds",
            "Consumer lag in seconds",
            lag_seconds.clone(),
        );

        registry.register(
            "streamhouse_consumer_last_committed_offset",
            "Last committed offset",
            last_committed.clone(),
        );

        registry.register(
            "streamhouse_consumer_current_offset",
            "Current read offset",
            current.clone(),
        );

        Self {
            records_consumed_total: records_consumed,
            bytes_consumed_total: bytes_consumed,
            poll_duration_seconds: poll_duration,
            consumer_lag_records: lag_records,
            consumer_lag_seconds: lag_seconds,
            last_committed_offset: last_committed,
            current_offset: current,
        }
    }

    /// Record a successful poll operation.
    pub fn record_poll(&self, record_count: usize, total_bytes: u64, duration_secs: f64) {
        self.records_consumed_total.inc_by(record_count as u64);
        self.bytes_consumed_total.inc_by(total_bytes);
        self.poll_duration_seconds.observe(duration_secs);
    }

    /// Update consumer lag metrics.
    pub fn update_lag(&self, lag_records: i64, lag_seconds: i64) {
        self.consumer_lag_records.set(lag_records);
        self.consumer_lag_seconds.set(lag_seconds);
    }

    /// Update offset positions.
    pub fn update_offsets(&self, current: i64, committed: i64) {
        self.current_offset.set(current);
        self.last_committed_offset.set(committed);
    }
}

pub struct Consumer {
    config: ConsumerConfig,
    group_id: Option<String>,

    // Per-partition readers
    readers: Arc<RwLock<HashMap<PartitionKey, PartitionConsumer>>>,

    // Shared resources
    metadata_store: Arc<dyn MetadataStore>,
    object_store: Arc<dyn object_store::ObjectStore>,
    cache: Arc<SegmentCache>,

    // Topic subscription
    subscribed_topics: Vec<String>,

    // Offset management
    auto_commit: bool,
    auto_commit_interval: Duration,
    last_commit: tokio::time::Instant,

    // Observability
    last_lag_update: tokio::time::Instant,
}

/// Key for identifying a partition in the readers map.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct PartitionKey {
    topic: String,
    partition_id: u32,
}

/// Per-partition consumer state.
struct PartitionConsumer {
    reader: Arc<PartitionReader>,
    current_offset: u64,
    last_committed_offset: u64,
}

/// Configuration for Consumer.
pub struct ConsumerConfig {
    pub group_id: Option<String>,
    pub topics: Vec<String>,
    pub object_store: Arc<dyn object_store::ObjectStore>,
    pub metadata_store: Arc<dyn MetadataStore>,
    pub cache_dir: String,
    pub cache_size_bytes: u64,
    pub auto_commit: bool,
    pub auto_commit_interval: Duration,
    pub offset_reset: OffsetReset,
}

/// Strategy for resetting offsets when no committed offset exists.
#[derive(Debug, Clone, Copy)]
pub enum OffsetReset {
    /// Start from offset 0 (beginning of partition).
    Earliest,
    /// Start from high watermark (latest offset).
    Latest,
    /// Fail if no committed offset exists.
    None,
}

/// A record consumed from a topic partition.
#[derive(Debug, Clone)]
pub struct ConsumedRecord {
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub timestamp: u64,
    pub key: Option<Bytes>,
    pub value: Bytes,
}

/// Builder for constructing a Consumer.
pub struct ConsumerBuilder {
    group_id: Option<String>,
    topics: Vec<String>,
    object_store: Option<Arc<dyn object_store::ObjectStore>>,
    metadata_store: Option<Arc<dyn MetadataStore>>,
    cache_dir: String,
    cache_size_bytes: u64,
    auto_commit: bool,
    auto_commit_interval: Duration,
    offset_reset: OffsetReset,
}

impl ConsumerBuilder {
    /// Create a new ConsumerBuilder with default settings.
    pub fn new() -> Self {
        Self {
            group_id: None,
            topics: Vec::new(),
            object_store: None,
            metadata_store: None,
            cache_dir: "/tmp/streamhouse-cache".to_string(),
            cache_size_bytes: 100 * 1024 * 1024, // 100MB default
            auto_commit: true,
            auto_commit_interval: Duration::from_secs(5),
            offset_reset: OffsetReset::Latest,
        }
    }

    /// Set the consumer group ID.
    ///
    /// If specified, the consumer will commit offsets to this group and
    /// resume from the last committed offset on restart.
    pub fn group_id(mut self, group_id: impl Into<String>) -> Self {
        self.group_id = Some(group_id.into());
        self
    }

    /// Set the topics to subscribe to.
    ///
    /// The consumer will automatically discover all partitions for these topics.
    pub fn topics(mut self, topics: Vec<String>) -> Self {
        self.topics = topics;
        self
    }

    /// Set the object store for reading segments.
    pub fn object_store(mut self, store: Arc<dyn object_store::ObjectStore>) -> Self {
        self.object_store = Some(store);
        self
    }

    /// Set the metadata store for offset commits and topic metadata.
    pub fn metadata_store(mut self, store: Arc<dyn MetadataStore>) -> Self {
        self.metadata_store = Some(store);
        self
    }

    /// Set the cache directory for segment caching.
    pub fn cache_dir(mut self, dir: impl Into<String>) -> Self {
        self.cache_dir = dir.into();
        self
    }

    /// Set the cache size in bytes.
    pub fn cache_size_bytes(mut self, bytes: u64) -> Self {
        self.cache_size_bytes = bytes;
        self
    }

    /// Enable or disable auto-commit.
    ///
    /// If enabled, offsets will be automatically committed at the configured interval.
    pub fn auto_commit(mut self, enabled: bool) -> Self {
        self.auto_commit = enabled;
        self
    }

    /// Set the auto-commit interval.
    pub fn auto_commit_interval(mut self, interval: Duration) -> Self {
        self.auto_commit_interval = interval;
        self
    }

    /// Set the offset reset strategy.
    ///
    /// This determines the starting offset when no committed offset exists.
    pub fn offset_reset(mut self, reset: OffsetReset) -> Self {
        self.offset_reset = reset;
        self
    }

    /// Set optional Prometheus metrics for observability.
    ///
    /// When provided, the Consumer will record throughput, latency, consumer lag,
    /// and offset positions to Prometheus. Metrics are atomic and lock-free.
    ///
    /// # Arguments
    ///
    /// * `metrics` - ConsumerMetrics instance (created with a Prometheus registry)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use prometheus_client::registry::Registry;
    /// use streamhouse_client::ConsumerMetrics;
    ///
    /// let mut registry = Registry::default();
    /// let metrics = Arc::new(ConsumerMetrics::new(&mut registry));
    ///
    /// let consumer = Consumer::builder()
    ///     .group_id("my-group")
    ///     .topics(vec!["orders".to_string()])
    ///     .metadata_store(metadata_store)
    ///     .object_store(object_store)
    ///     .metrics(metrics)
    ///     .build()
    ///     .await?;
    /// ```
    #[cfg(feature = "metrics")]
    pub fn metrics(mut self, metrics: Arc<ConsumerMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Build the Consumer.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Required fields (metadata_store, object_store, topics) are missing
    /// - Cache directory cannot be created
    /// - Consumer group registration fails
    /// - Topic metadata cannot be fetched
    pub async fn build(self) -> Result<Consumer> {
        // Validate required fields
        let metadata_store = self
            .metadata_store
            .ok_or_else(|| ClientError::ConfigError("metadata_store required".into()))?;
        let object_store = self
            .object_store
            .ok_or_else(|| ClientError::ConfigError("object_store required".into()))?;

        if self.topics.is_empty() {
            return Err(ClientError::ConfigError("topics required".into()));
        }

        // Create segment cache
        let cache = Arc::new(
            SegmentCache::new(&self.cache_dir, self.cache_size_bytes)
                .map_err(|e| ClientError::StorageError(e.to_string()))?,
        );

        // Register consumer group if specified
        if let Some(ref group_id) = self.group_id {
            metadata_store.ensure_consumer_group(group_id).await?;
        }

        // Create consumer
        let consumer = Consumer {
            config: ConsumerConfig {
                group_id: self.group_id.clone(),
                topics: self.topics.clone(),
                object_store: Arc::clone(&object_store),
                metadata_store: Arc::clone(&metadata_store),
                cache_dir: self.cache_dir,
                cache_size_bytes: self.cache_size_bytes,
                auto_commit: self.auto_commit,
                auto_commit_interval: self.auto_commit_interval,
                offset_reset: self.offset_reset,
            },
            group_id: self.group_id,
            readers: Arc::new(RwLock::new(HashMap::new())),
            metadata_store,
            object_store,
            cache,
            subscribed_topics: self.topics,
            auto_commit: self.auto_commit,
            auto_commit_interval: self.auto_commit_interval,
            last_commit: tokio::time::Instant::now(),
            last_lag_update: tokio::time::Instant::now(),
        };

        // Initialize partition readers for subscribed topics
        consumer.subscribe_topics().await?;

        Ok(consumer)
    }
}

impl Default for ConsumerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Consumer {
    /// Create a new ConsumerBuilder.
    pub fn builder() -> ConsumerBuilder {
        ConsumerBuilder::new()
    }

    /// Subscribe to topics by discovering partitions and creating readers.
    async fn subscribe_topics(&self) -> Result<()> {
        let mut readers = self.readers.write().await;

        for topic in &self.subscribed_topics {
            // Get topic metadata
            let topic_meta = self
                .metadata_store
                .get_topic(topic)
                .await?
                .ok_or_else(|| ClientError::TopicNotFound(topic.clone()))?;

            // Create reader for each partition
            for partition_id in 0..topic_meta.partition_count {
                let key = PartitionKey {
                    topic: topic.clone(),
                    partition_id,
                };

                // Create PartitionReader
                let reader = Arc::new(PartitionReader::new(
                    topic.clone(),
                    partition_id,
                    Arc::clone(&self.metadata_store),
                    Arc::clone(&self.object_store),
                    Arc::clone(&self.cache),
                ));

                // Determine starting offset
                let start_offset = self.determine_start_offset(topic, partition_id).await?;

                readers.insert(
                    key,
                    PartitionConsumer {
                        reader,
                        current_offset: start_offset,
                        last_committed_offset: start_offset,
                    },
                );
            }
        }

        Ok(())
    }

    /// Determine starting offset for a partition.
    async fn determine_start_offset(&self, topic: &str, partition_id: u32) -> Result<u64> {
        // If consumer group, check committed offset
        if let Some(ref group_id) = self.group_id {
            let offsets = self.metadata_store.get_consumer_offsets(group_id).await?;

            for offset in offsets {
                if offset.topic == topic && offset.partition_id == partition_id {
                    return Ok(offset.committed_offset);
                }
            }
        }

        // No committed offset - use offset_reset strategy
        match self.config.offset_reset {
            OffsetReset::Earliest => Ok(0),
            OffsetReset::Latest => {
                // Get high watermark from partition
                let partition = self
                    .metadata_store
                    .get_partition(topic, partition_id)
                    .await?
                    .ok_or_else(|| {
                        ClientError::InvalidPartition(partition_id, topic.to_string(), 0)
                    })?;
                Ok(partition.high_watermark)
            }
            OffsetReset::None => Err(ClientError::ConfigError(format!(
                "No committed offset for {}-{} and offset_reset=None",
                topic, partition_id
            ))),
        }
    }

    /// Poll for records across all subscribed partitions.
    ///
    /// This method reads from all partitions in round-robin fashion and returns
    /// all available records up to the timeout.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for records
    ///
    /// # Returns
    ///
    /// A vector of consumed records (may be empty if no records available).
    ///
    /// # Errors
    ///
    /// Returns an error if reading from storage fails.
    #[tracing::instrument(skip(self), fields(timeout_ms = timeout.as_millis()))]
    pub async fn poll(&mut self, timeout: Duration) -> Result<Vec<ConsumedRecord>> {
        let start = tokio::time::Instant::now();
        let mut all_records = Vec::new();

        // Read from each partition (round-robin)
        let readers = self.readers.read().await;

        for (key, partition_consumer) in readers.iter() {
            // Check timeout
            if start.elapsed() >= timeout {
                break;
            }

            // Read batch from partition
            let result = match partition_consumer
                .reader
                .read(partition_consumer.current_offset, 100) // Batch size: 100
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    // If offset not found, it might be because partition is empty or we're at the end
                    // Just skip this partition for now
                    if e.to_string().contains("Offset not found") {
                        continue;
                    }
                    return Err(ClientError::StorageError(e.to_string()));
                }
            };

            // Convert to ConsumedRecord
            for record in result.records {
                all_records.push(ConsumedRecord {
                    topic: key.topic.clone(),
                    partition: key.partition_id,
                    offset: record.offset,
                    timestamp: record.timestamp,
                    key: record.key,
                    value: record.value,
                });
            }
        }

        // Update current offsets
        drop(readers); // Release read lock
        let mut readers = self.readers.write().await;

        for record in &all_records {
            let key = PartitionKey {
                topic: record.topic.clone(),
                partition_id: record.partition,
            };

            if let Some(partition_consumer) = readers.get_mut(&key) {
                // Update to next offset after this record
                partition_consumer.current_offset = record.offset + 1;
            }
        }

        // Auto-commit if enabled and interval passed
        if self.auto_commit && self.last_commit.elapsed() >= self.auto_commit_interval {
            drop(readers); // Release write lock before commit
            self.commit().await?;
            self.last_commit = tokio::time::Instant::now();
        }

        // Record metrics (Phase 7)
        // Record per-topic metrics
        let mut topic_stats: HashMap<String, usize> = HashMap::new();
        for record in &all_records {
            *topic_stats.entry(record.topic.clone()).or_insert(0) += 1;
        }

        for (topic, count) in topic_stats {
            streamhouse_observability::metrics::CONSUMER_RECORDS_TOTAL
                .with_label_values(&[&topic, self.group_id.as_deref().unwrap_or("default")])
                .inc_by(count as u64);
        }

        // Update lag metrics every 30 seconds
        if self.last_lag_update.elapsed() >= Duration::from_secs(30) {
            self.update_lag_internal().await.ok(); // Ignore errors
            self.last_lag_update = tokio::time::Instant::now();
        }

        Ok(all_records)
    }

    /// Commit current offsets for all partitions.
    ///
    /// This stores the current offsets in the metadata store for the consumer group.
    /// On restart, the consumer will resume from these offsets.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Consumer was created without a group_id
    /// - Metadata store commit fails
    #[tracing::instrument(skip(self))]
    pub async fn commit(&self) -> Result<()> {
        let group_id = self
            .group_id
            .as_ref()
            .ok_or_else(|| ClientError::ConfigError("commit requires group_id".into()))?;

        let readers = self.readers.read().await;

        for (key, partition_consumer) in readers.iter() {
            // Only commit if offset changed
            if partition_consumer.current_offset > partition_consumer.last_committed_offset {
                self.metadata_store
                    .commit_offset(
                        group_id,
                        &key.topic,
                        key.partition_id,
                        partition_consumer.current_offset,
                        None, // No metadata
                    )
                    .await?;
            }
        }

        // Update last committed offsets
        drop(readers);
        let mut readers = self.readers.write().await;

        for partition_consumer in readers.values_mut() {
            partition_consumer.last_committed_offset = partition_consumer.current_offset;
        }

        Ok(())
    }

    /// Seek to a specific offset for a partition.
    ///
    /// This allows manual control of the consumer position. The next poll()
    /// will start reading from this offset.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `offset` - Target offset
    ///
    /// # Errors
    ///
    /// Returns an error if the partition is not subscribed.
    pub async fn seek(&self, topic: &str, partition_id: u32, offset: u64) -> Result<()> {
        let key = PartitionKey {
            topic: topic.to_string(),
            partition_id,
        };

        let mut readers = self.readers.write().await;

        let partition_consumer = readers
            .get_mut(&key)
            .ok_or_else(|| ClientError::InvalidPartition(partition_id, topic.to_string(), 0))?;

        partition_consumer.current_offset = offset;
        Ok(())
    }

    /// Get the current position (next offset to read) for a partition.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    ///
    /// # Returns
    ///
    /// The current offset position.
    ///
    /// # Errors
    ///
    /// Returns an error if the partition is not subscribed.
    pub async fn position(&self, topic: &str, partition_id: u32) -> Result<u64> {
        let key = PartitionKey {
            topic: topic.to_string(),
            partition_id,
        };

        let readers = self.readers.read().await;

        let partition_consumer = readers
            .get(&key)
            .ok_or_else(|| ClientError::InvalidPartition(partition_id, topic.to_string(), 0))?;

        Ok(partition_consumer.current_offset)
    }

    /// Get the current consumer lag for a specific partition (Phase 7).
    ///
    /// Consumer lag is the difference between the partition's high watermark
    /// (latest offset) and the consumer's current position.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    ///
    /// # Returns
    ///
    /// The lag in records (high_watermark - current_offset).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Partition metadata cannot be fetched
    /// - Partition is not subscribed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let lag = consumer.lag("orders", 0).await?;
    /// println!("Current lag: {} records", lag);
    /// ```
    pub async fn lag(&self, topic: &str, partition: u32) -> Result<i64> {
        // Get partition metadata for high watermark
        let partition_meta = self
            .metadata_store
            .get_partition(topic, partition)
            .await
            .map_err(ClientError::MetadataError)?
            .ok_or_else(|| {
                ClientError::TopicNotFound(format!("Partition {}/{} not found", topic, partition))
            })?;

        // Get current offset
        let current = self.position(topic, partition).await?;

        // Calculate lag
        Ok(partition_meta.high_watermark as i64 - current as i64)
    }

    /// Internal method to update lag metrics for all partitions (Phase 7).
    async fn update_lag_internal(&self) -> Result<()> {
        let readers = self.readers.read().await;
        let group_id = self.group_id.as_deref().unwrap_or("default");

        for (key, partition_consumer) in readers.iter() {
            // Get partition metadata
            if let Ok(Some(partition_meta)) = self
                .metadata_store
                .get_partition(&key.topic, key.partition_id)
                .await
            {
                let current = partition_consumer.current_offset as i64;
                let high_watermark = partition_meta.high_watermark as i64;
                let lag_records = high_watermark - current;

                // Update lag metric
                streamhouse_observability::metrics::CONSUMER_LAG
                    .with_label_values(&[
                        &key.topic,
                        &key.partition_id.to_string(),
                        group_id,
                    ])
                    .set(lag_records);
            }
        }
        Ok(())
    }

    /// Close the consumer and commit final offsets.
    ///
    /// This performs a final offset commit if the consumer has a group_id.
    pub async fn close(self) -> Result<()> {
        // Final commit if group_id is set
        if self.group_id.is_some() {
            self.commit().await?;
        }

        Ok(())
    }
}
