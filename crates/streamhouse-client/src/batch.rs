//! Batching Logic for StreamHouse Producer
//!
//! This module implements record batching to amortize the cost of network round-trips.
//! Records are accumulated in memory until size or time thresholds are met, then flushed
//! to agents via gRPC.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐
//! │  send(...)   │ Producer API
//! └──────┬───────┘
//!        │
//!        ▼
//! ┌──────────────────────────────┐
//! │  BatchBuffer                 │ Per-partition buffer
//! │  - records: Vec<Record>      │
//! │  - size_bytes: usize         │
//! │  - created_at: Instant       │
//! └──────┬───────────────────────┘
//!        │
//!        ├─→ Flush on size (100 records or 1MB)
//!        ├─→ Flush on time (100ms)
//!        ├─→ Flush on explicit flush() call
//!        │
//!        ▼
//! ┌──────────────────────────────┐
//! │  gRPC ProduceRequest         │
//! │  topic, partition, records[] │
//! └──────────────────────────────┘
//! ```
//!
//! ## Flush Triggers
//!
//! Batches are flushed when ANY of these conditions are met:
//! - **Size**: Batch reaches `max_batch_size` records (default: 100)
//! - **Bytes**: Batch reaches `max_batch_bytes` bytes (default: 1MB)
//! - **Time**: Batch age exceeds `linger_ms` (default: 100ms)
//! - **Manual**: User calls `flush()` (e.g., shutdown)
//!
//! ## Performance
//!
//! Batching provides massive throughput improvements:
//! - **Phase 5.1 (no batching)**: ~5 records/sec (one RPC per record)
//! - **Phase 5.2 (batching)**: 50K+ records/sec (100 records per RPC)
//!
//! ## Thread Safety
//!
//! BatchBuffer is NOT thread-safe. The Producer wraps it in Arc<Mutex> to allow
//! concurrent sends from multiple threads.

use bytes::Bytes;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, trace};

/// A single record to be batched.
///
/// # Fields
///
/// * `key` - Optional record key (used for compaction, routing)
/// * `value` - Record value (the actual data)
/// * `timestamp` - Record timestamp (milliseconds since Unix epoch)
///
/// # Memory Layout
///
/// Records are stored as `Bytes` (zero-copy reference-counted buffers) to avoid
/// unnecessary copies when sending over gRPC.
#[derive(Debug, Clone)]
pub struct BatchRecord {
    /// Optional record key
    pub key: Option<Bytes>,
    /// Record value
    pub value: Bytes,
    /// Timestamp in milliseconds since Unix epoch
    pub timestamp: u64,
}

impl BatchRecord {
    /// Create a new batch record.
    ///
    /// # Arguments
    ///
    /// * `key` - Optional record key
    /// * `value` - Record value
    /// * `timestamp` - Timestamp in milliseconds since Unix epoch
    ///
    /// # Returns
    ///
    /// A new `BatchRecord` instance.
    pub fn new(key: Option<Bytes>, value: Bytes, timestamp: u64) -> Self {
        Self {
            key,
            value,
            timestamp,
        }
    }

    /// Calculate the size of this record in bytes.
    ///
    /// # Returns
    ///
    /// Total size = key_size + value_size + overhead (16 bytes for timestamp and metadata)
    pub fn size_bytes(&self) -> usize {
        let key_size = self.key.as_ref().map_or(0, |k| k.len());
        let value_size = self.value.len();
        key_size + value_size + 16 // 16 bytes overhead for timestamp + metadata
    }
}

/// Buffer for batching records destined for a single partition.
///
/// # Lifecycle
///
/// 1. **Create**: Initialize empty buffer
/// 2. **Append**: Add records until flush trigger
/// 3. **Flush**: Send batch to agent via gRPC, clear buffer
/// 4. **Repeat**: Continue appending new records
///
/// # Flush Triggers
///
/// - Size: `records.len() >= max_batch_size`
/// - Bytes: `size_bytes >= max_batch_bytes`
/// - Time: `age >= linger_ms`
/// - Manual: `should_flush_now() returns true`
///
/// # Thread Safety
///
/// NOT thread-safe. Must be wrapped in Mutex for concurrent access.
///
/// # Examples
///
/// ```ignore
/// let mut buffer = BatchBuffer::new(100, 1024 * 1024, Duration::from_millis(100));
///
/// // Append records
/// buffer.append(BatchRecord::new(None, Bytes::from("hello"), 1000));
/// buffer.append(BatchRecord::new(None, Bytes::from("world"), 1001));
///
/// // Check if should flush
/// if buffer.should_flush() {
///     let records = buffer.drain();
///     // ... send to agent via gRPC ...
/// }
/// ```
#[derive(Debug)]
pub struct BatchBuffer {
    /// Buffered records
    records: Vec<BatchRecord>,

    /// Total size in bytes (sum of all record sizes)
    size_bytes: usize,

    /// When this batch was created (for linger_ms)
    created_at: Instant,

    /// Maximum number of records per batch
    max_batch_size: usize,

    /// Maximum batch size in bytes
    max_batch_bytes: usize,

    /// Maximum time to wait before flushing (linger time)
    linger_ms: Duration,
}

impl BatchBuffer {
    /// Create a new empty batch buffer.
    ///
    /// # Arguments
    ///
    /// * `max_batch_size` - Maximum number of records per batch (default: 100)
    /// * `max_batch_bytes` - Maximum batch size in bytes (default: 1MB)
    /// * `linger_ms` - Maximum time to wait before flushing (default: 100ms)
    ///
    /// # Returns
    ///
    /// A new empty `BatchBuffer`.
    ///
    /// # Performance Tuning
    ///
    /// - **High throughput**: Increase `max_batch_size` to 1000+ (more records per RPC)
    /// - **Low latency**: Decrease `linger_ms` to 10ms (flush more frequently)
    /// - **Large records**: Decrease `max_batch_bytes` to avoid memory pressure
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Default settings (balanced)
    /// let buffer = BatchBuffer::new(100, 1024 * 1024, Duration::from_millis(100));
    ///
    /// // High throughput settings
    /// let buffer = BatchBuffer::new(1000, 10 * 1024 * 1024, Duration::from_millis(500));
    ///
    /// // Low latency settings
    /// let buffer = BatchBuffer::new(10, 256 * 1024, Duration::from_millis(10));
    /// ```
    pub fn new(max_batch_size: usize, max_batch_bytes: usize, linger_ms: Duration) -> Self {
        Self {
            records: Vec::with_capacity(max_batch_size),
            size_bytes: 0,
            created_at: Instant::now(),
            max_batch_size,
            max_batch_bytes,
            linger_ms,
        }
    }

    /// Append a record to the batch.
    ///
    /// # Arguments
    ///
    /// * `record` - Record to append
    ///
    /// # Examples
    ///
    /// ```ignore
    /// buffer.append(BatchRecord::new(
    ///     Some(Bytes::from("user123")),
    ///     Bytes::from("order data"),
    ///     1234567890,
    /// ));
    /// ```
    pub fn append(&mut self, record: BatchRecord) {
        self.size_bytes += record.size_bytes();
        self.records.push(record);
        trace!(
            record_count = self.records.len(),
            size_bytes = self.size_bytes,
            "Appended record to batch"
        );
    }

    /// Check if this batch should be flushed.
    ///
    /// # Returns
    ///
    /// `true` if ANY of these conditions are met:
    /// - Record count >= max_batch_size
    /// - Size in bytes >= max_batch_bytes
    /// - Age >= linger_ms
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if buffer.should_flush() {
    ///     let records = buffer.drain();
    ///     // ... send to agent ...
    /// }
    /// ```
    pub fn should_flush(&self) -> bool {
        if self.records.is_empty() {
            return false;
        }

        // Flush on size
        if self.records.len() >= self.max_batch_size {
            trace!(
                record_count = self.records.len(),
                max_batch_size = self.max_batch_size,
                "Batch should flush: size threshold"
            );
            return true;
        }

        // Flush on bytes
        if self.size_bytes >= self.max_batch_bytes {
            trace!(
                size_bytes = self.size_bytes,
                max_batch_bytes = self.max_batch_bytes,
                "Batch should flush: bytes threshold"
            );
            return true;
        }

        // Flush on time
        let age = self.created_at.elapsed();
        if age >= self.linger_ms {
            trace!(
                age_ms = age.as_millis(),
                linger_ms = self.linger_ms.as_millis(),
                "Batch should flush: time threshold"
            );
            return true;
        }

        false
    }

    /// Drain all records from the batch and reset.
    ///
    /// # Returns
    ///
    /// All buffered records. The buffer is reset to empty.
    ///
    /// # Side Effects
    ///
    /// - Resets `records` to empty
    /// - Resets `size_bytes` to 0
    /// - Resets `created_at` to current time
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let records = buffer.drain();
    /// assert!(buffer.is_empty());
    /// ```
    pub fn drain(&mut self) -> Vec<BatchRecord> {
        let records = std::mem::take(&mut self.records);
        self.size_bytes = 0;
        self.created_at = Instant::now();
        debug!(record_count = records.len(), "Drained batch buffer");
        records
    }

    /// Check if the batch is empty.
    ///
    /// # Returns
    ///
    /// `true` if no records are buffered.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Get the number of buffered records.
    ///
    /// # Returns
    ///
    /// Number of records in the batch.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Get the total size in bytes of buffered records.
    ///
    /// # Returns
    ///
    /// Sum of all record sizes in bytes.
    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    /// Get the age of this batch.
    ///
    /// # Returns
    ///
    /// Time elapsed since batch was created or last drained.
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Manager for multiple batch buffers (one per partition).
///
/// # Architecture
///
/// ```text
/// ┌────────────────────────────────────────┐
/// │  BatchManager                          │
/// ├────────────────────────────────────────┤
/// │  buffers: HashMap<PartitionKey, Batch> │
/// │                                        │
/// │  PartitionKey = (topic, partition_id)  │
/// └────────────────────────────────────────┘
///       │
///       ├─→ ("orders", 0) → BatchBuffer { 50 records, 128KB }
///       ├─→ ("orders", 1) → BatchBuffer { 20 records, 64KB }
///       └─→ ("events", 0) → BatchBuffer { 100 records, 512KB }
/// ```
///
/// # Thread Safety
///
/// NOT thread-safe. Must be wrapped in Mutex for concurrent access.
///
/// # Examples
///
/// ```ignore
/// let mut manager = BatchManager::new(100, 1024 * 1024, Duration::from_millis(100));
///
/// // Append to partition
/// manager.append("orders", 0, BatchRecord::new(...));
///
/// // Get batches ready to flush
/// let ready = manager.ready_batches();
/// for (topic, partition, records) in ready {
///     // ... send to agent via gRPC ...
/// }
/// ```
pub struct BatchManager {
    /// Per-partition buffers
    /// Key: (topic, partition_id)
    buffers: HashMap<PartitionKey, BatchBuffer>,

    /// Configuration for new buffers
    max_batch_size: usize,
    max_batch_bytes: usize,
    linger_ms: Duration,
}

/// Key for identifying a partition.
type PartitionKey = (String, u32);

impl BatchManager {
    /// Create a new batch manager.
    ///
    /// # Arguments
    ///
    /// * `max_batch_size` - Maximum records per batch
    /// * `max_batch_bytes` - Maximum bytes per batch
    /// * `linger_ms` - Maximum time before flush
    ///
    /// # Returns
    ///
    /// A new empty `BatchManager`.
    pub fn new(max_batch_size: usize, max_batch_bytes: usize, linger_ms: Duration) -> Self {
        Self {
            buffers: HashMap::new(),
            max_batch_size,
            max_batch_bytes,
            linger_ms,
        }
    }

    /// Append a record to the appropriate partition buffer.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    /// * `record` - Record to append
    ///
    /// # Side Effects
    ///
    /// Creates a new buffer if this is the first record for this partition.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// manager.append("orders", 0, BatchRecord::new(
    ///     Some(Bytes::from("user123")),
    ///     Bytes::from("order data"),
    ///     1234567890,
    /// ));
    /// ```
    pub fn append(&mut self, topic: &str, partition: u32, record: BatchRecord) {
        let key = (topic.to_string(), partition);
        let buffer = self.buffers.entry(key).or_insert_with(|| {
            BatchBuffer::new(self.max_batch_size, self.max_batch_bytes, self.linger_ms)
        });
        buffer.append(record);
    }

    /// Get all batches that are ready to flush.
    ///
    /// # Returns
    ///
    /// Vec of (topic, partition, records) tuples for all batches that should be flushed.
    /// The returned batches are drained from the manager.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// for (topic, partition, records) in manager.ready_batches() {
    ///     send_to_agent(&topic, partition, records).await?;
    /// }
    /// ```
    pub fn ready_batches(&mut self) -> Vec<(String, u32, Vec<BatchRecord>)> {
        let mut ready = Vec::new();

        for ((topic, partition), buffer) in &mut self.buffers {
            if buffer.should_flush() {
                let records = buffer.drain();
                ready.push((topic.clone(), *partition, records));
            }
        }

        debug!(batch_count = ready.len(), "Found ready batches");
        ready
    }

    /// Flush all batches (used for shutdown).
    ///
    /// # Returns
    ///
    /// Vec of (topic, partition, records) tuples for ALL batches, regardless of
    /// whether they meet flush criteria.
    ///
    /// # Side Effects
    ///
    /// Clears all buffers.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Graceful shutdown
    /// for (topic, partition, records) in manager.flush_all() {
    ///     send_to_agent(&topic, partition, records).await?;
    /// }
    /// ```
    pub fn flush_all(&mut self) -> Vec<(String, u32, Vec<BatchRecord>)> {
        let mut all = Vec::new();

        for ((topic, partition), buffer) in &mut self.buffers {
            if !buffer.is_empty() {
                let records = buffer.drain();
                all.push((topic.clone(), *partition, records));
            }
        }

        debug!(batch_count = all.len(), "Flushed all batches");
        all
    }

    /// Get statistics about buffered data.
    ///
    /// # Returns
    ///
    /// Tuple of (partition_count, total_records, total_bytes)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let (partitions, records, bytes) = manager.stats();
    /// println!("Buffered: {} partitions, {} records, {} bytes", partitions, records, bytes);
    /// ```
    pub fn stats(&self) -> (usize, usize, usize) {
        let partition_count = self.buffers.len();
        let total_records: usize = self.buffers.values().map(|b| b.len()).sum();
        let total_bytes: usize = self.buffers.values().map(|b| b.size_bytes()).sum();
        (partition_count, total_records, total_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_record_size() {
        let record = BatchRecord::new(Some(Bytes::from("key")), Bytes::from("value"), 1234567890);
        // key (3) + value (5) + overhead (16) = 24
        assert_eq!(record.size_bytes(), 24);
    }

    #[test]
    fn test_batch_buffer_append() {
        let mut buffer = BatchBuffer::new(100, 1024 * 1024, Duration::from_millis(100));
        assert!(buffer.is_empty());

        buffer.append(BatchRecord::new(None, Bytes::from("test"), 1000));
        assert_eq!(buffer.len(), 1);
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_batch_buffer_flush_on_size() {
        let mut buffer = BatchBuffer::new(2, 1024 * 1024, Duration::from_secs(60));

        buffer.append(BatchRecord::new(None, Bytes::from("test1"), 1000));
        assert!(!buffer.should_flush());

        buffer.append(BatchRecord::new(None, Bytes::from("test2"), 1001));
        assert!(buffer.should_flush()); // Hit size threshold
    }

    #[test]
    fn test_batch_buffer_flush_on_bytes() {
        let mut buffer = BatchBuffer::new(100, 50, Duration::from_secs(60));

        // Each record is ~20 bytes, 3 records = 60 bytes > 50 byte limit
        buffer.append(BatchRecord::new(None, Bytes::from("test"), 1000));
        buffer.append(BatchRecord::new(None, Bytes::from("test"), 1001));
        assert!(!buffer.should_flush());

        buffer.append(BatchRecord::new(None, Bytes::from("test"), 1002));
        assert!(buffer.should_flush()); // Hit bytes threshold
    }

    #[test]
    fn test_batch_buffer_drain() {
        let mut buffer = BatchBuffer::new(100, 1024 * 1024, Duration::from_millis(100));

        buffer.append(BatchRecord::new(None, Bytes::from("test1"), 1000));
        buffer.append(BatchRecord::new(None, Bytes::from("test2"), 1001));

        let records = buffer.drain();
        assert_eq!(records.len(), 2);
        assert!(buffer.is_empty());
        assert_eq!(buffer.size_bytes(), 0);
    }

    #[test]
    fn test_batch_manager_append() {
        let mut manager = BatchManager::new(100, 1024 * 1024, Duration::from_millis(100));

        manager.append(
            "orders",
            0,
            BatchRecord::new(None, Bytes::from("test1"), 1000),
        );
        manager.append(
            "orders",
            1,
            BatchRecord::new(None, Bytes::from("test2"), 1001),
        );

        let (partitions, records, _bytes) = manager.stats();
        assert_eq!(partitions, 2);
        assert_eq!(records, 2);
    }

    #[test]
    fn test_batch_manager_ready_batches() {
        let mut manager = BatchManager::new(2, 1024 * 1024, Duration::from_secs(60));

        manager.append(
            "orders",
            0,
            BatchRecord::new(None, Bytes::from("test1"), 1000),
        );
        assert!(manager.ready_batches().is_empty());

        manager.append(
            "orders",
            0,
            BatchRecord::new(None, Bytes::from("test2"), 1001),
        );
        let ready = manager.ready_batches();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, "orders");
        assert_eq!(ready[0].1, 0);
        assert_eq!(ready[0].2.len(), 2);
    }

    #[test]
    fn test_batch_manager_flush_all() {
        let mut manager = BatchManager::new(100, 1024 * 1024, Duration::from_secs(60));

        manager.append(
            "orders",
            0,
            BatchRecord::new(None, Bytes::from("test1"), 1000),
        );
        manager.append(
            "events",
            0,
            BatchRecord::new(None, Bytes::from("test2"), 1001),
        );

        let all = manager.flush_all();
        assert_eq!(all.len(), 2);

        let (partitions, records, _bytes) = manager.stats();
        assert_eq!(partitions, 2); // Buffers still exist but empty
        assert_eq!(records, 0);
    }
}
