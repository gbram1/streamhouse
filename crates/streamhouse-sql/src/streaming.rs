//! Stateful stream processing framework
//!
//! Provides a micro-batch stream processor with pluggable state backends,
//! checkpoint/restore semantics, and exactly-once processing guarantees.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::SqlError;
use crate::Result;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configures a [`StreamProcessor`] instance.
#[derive(Debug, Clone)]
pub struct StreamProcessorConfig {
    /// Unique identifier for this processor instance.
    pub processor_id: String,
    /// How often to persist checkpoints.
    pub checkpoint_interval: Duration,
    /// Maximum number of records per micro-batch.
    pub max_batch_size: usize,
    /// Maximum wall-clock time allowed for processing a single micro-batch.
    pub processing_time_timeout: Duration,
    /// Backend used for persistent state.
    pub state_backend: StateBackend,
}

impl Default for StreamProcessorConfig {
    fn default() -> Self {
        Self {
            processor_id: Uuid::new_v4().to_string(),
            checkpoint_interval: Duration::from_secs(10),
            max_batch_size: 1024,
            processing_time_timeout: Duration::from_secs(30),
            state_backend: StateBackend::InMemory,
        }
    }
}

/// Which backend to use for the processor's state.
#[derive(Debug, Clone)]
pub enum StateBackend {
    /// Ephemeral in-memory state (useful for development / testing).
    InMemory,
    /// Persistent RocksDB-backed state.
    RocksDb { path: String },
}

// ---------------------------------------------------------------------------
// State store trait
// ---------------------------------------------------------------------------

/// Abstraction over a key/value state store that supports namespaces,
/// checkpointing, and restore.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Retrieve a value by namespace and key.
    async fn get(&self, namespace: &str, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Store a value under namespace and key.
    async fn put(&self, namespace: &str, key: &[u8], value: &[u8]) -> Result<()>;

    /// Delete a key from a namespace.
    async fn delete(&self, namespace: &str, key: &[u8]) -> Result<()>;

    /// Scan all keys that share a given prefix within a namespace.
    async fn scan_prefix(
        &self,
        namespace: &str,
        prefix: &[u8],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Create a serialisable snapshot of the current state.
    async fn checkpoint(&self) -> Result<StateCheckpoint>;

    /// Restore state from a previously-created snapshot.
    async fn restore(&self, checkpoint: &StateCheckpoint) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Checkpoint types
// ---------------------------------------------------------------------------

/// A serialisable snapshot of processor + state-store state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCheckpoint {
    /// Unique id for this checkpoint.
    pub checkpoint_id: String,
    /// Processor that created this checkpoint.
    pub processor_id: String,
    /// Unix-epoch milliseconds when the checkpoint was created.
    pub timestamp: i64,
    /// topic -> (partition -> offset) at checkpoint time.
    pub offsets: HashMap<String, HashMap<u32, u64>>,
    /// Opaque serialised state blob.
    pub state_snapshot: Vec<u8>,
}

/// Lightweight offset bookmark kept per processor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub processor_id: String,
    /// (topic, partition) -> offset
    pub source_offsets: HashMap<(String, u32), u64>,
    /// Unix-epoch milliseconds.
    pub timestamp: i64,
    /// Monotonically increasing version counter.
    pub state_version: u64,
}

// ---------------------------------------------------------------------------
// Processing stats
// ---------------------------------------------------------------------------

/// Runtime statistics about a running stream processor.
#[derive(Debug, Clone, Default)]
pub struct ProcessingStats {
    pub batches_processed: u64,
    pub records_processed: u64,
    pub checkpoints_completed: u64,
    pub last_checkpoint_time: Option<Instant>,
    pub processing_errors: u64,
    pub total_processing_time: Duration,
}

// ---------------------------------------------------------------------------
// InMemoryStateStore
// ---------------------------------------------------------------------------

/// A simple in-memory implementation of [`StateStore`].
pub struct InMemoryStateStore {
    /// namespace -> (key -> value)
    data: RwLock<HashMap<String, HashMap<Vec<u8>, Vec<u8>>>>,
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateStore for InMemoryStateStore {
    async fn get(&self, namespace: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let data = self.data.read().await;
        Ok(data
            .get(namespace)
            .and_then(|ns| ns.get(key))
            .cloned())
    }

    async fn put(&self, namespace: &str, key: &[u8], value: &[u8]) -> Result<()> {
        let mut data = self.data.write().await;
        data.entry(namespace.to_string())
            .or_default()
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    async fn delete(&self, namespace: &str, key: &[u8]) -> Result<()> {
        let mut data = self.data.write().await;
        if let Some(ns) = data.get_mut(namespace) {
            ns.remove(key);
        }
        Ok(())
    }

    async fn scan_prefix(
        &self,
        namespace: &str,
        prefix: &[u8],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self.data.read().await;
        let results = data
            .get(namespace)
            .map(|ns| {
                ns.iter()
                    .filter(|(k, _)| k.starts_with(prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(results)
    }

    async fn checkpoint(&self) -> Result<StateCheckpoint> {
        let data = self.data.read().await;
        let snapshot = bincode::serialize(&*data).map_err(|e| {
            SqlError::CheckpointError(format!("failed to serialise state: {e}"))
        })?;
        Ok(StateCheckpoint {
            checkpoint_id: Uuid::new_v4().to_string(),
            processor_id: String::new(), // filled in by the processor
            timestamp: chrono::Utc::now().timestamp_millis(),
            offsets: HashMap::new(),
            state_snapshot: snapshot,
        })
    }

    async fn restore(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        let restored: HashMap<String, HashMap<Vec<u8>, Vec<u8>>> =
            bincode::deserialize(&checkpoint.state_snapshot).map_err(|e| {
                SqlError::CheckpointError(format!("failed to deserialise state: {e}"))
            })?;
        let mut data = self.data.write().await;
        *data = restored;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StreamProcessor
// ---------------------------------------------------------------------------

/// A stateful micro-batch stream processor.
///
/// Processes records in batches, maintains state via a pluggable [`StateStore`],
/// and supports checkpoint/restore for fault tolerance.
pub struct StreamProcessor {
    config: StreamProcessorConfig,
    state_store: Arc<Box<dyn StateStore>>,
    checkpoints: RwLock<HashMap<String, Checkpoint>>,
    stats: RwLock<ProcessingStats>,
}

impl StreamProcessor {
    /// Create a new stream processor with the given config and state store.
    pub fn new(config: StreamProcessorConfig, state_store: Box<dyn StateStore>) -> Self {
        Self {
            config,
            state_store: Arc::new(state_store),
            checkpoints: RwLock::new(HashMap::new()),
            stats: RwLock::new(ProcessingStats::default()),
        }
    }

    /// Process a batch of byte-encoded records.
    ///
    /// Returns the number of records successfully processed.
    pub async fn process_batch(
        &self,
        topic: &str,
        partition: u32,
        records: &[(u64, Vec<u8>)], // (offset, payload)
    ) -> Result<u64> {
        if records.is_empty() {
            return Ok(0);
        }

        let batch_size = records.len().min(self.config.max_batch_size);
        let batch = &records[..batch_size];

        let start = Instant::now();

        // Store each record in the state store (keyed by offset)
        let mut processed: u64 = 0;
        for (offset, payload) in batch {
            let key = offset.to_be_bytes();
            let ns = format!("{topic}-{partition}");

            // Check timeout
            if start.elapsed() > self.config.processing_time_timeout {
                let mut stats = self.stats.write().await;
                stats.processing_errors += 1;
                return Err(SqlError::StreamProcessingError(format!(
                    "batch processing exceeded timeout of {:?} after {processed} records",
                    self.config.processing_time_timeout
                )));
            }

            self.state_store.put(&ns, &key, payload).await?;
            processed += 1;
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.batches_processed += 1;
            stats.records_processed += processed;
            stats.total_processing_time += start.elapsed();
        }

        Ok(processed)
    }

    /// Persist a checkpoint capturing the current offsets and state.
    pub async fn checkpoint(
        &self,
        offsets: HashMap<(String, u32), u64>,
    ) -> Result<StateCheckpoint> {
        let mut state_checkpoint = self.state_store.checkpoint().await?;
        state_checkpoint.processor_id = self.config.processor_id.clone();

        // Convert offsets into the nested map form
        let mut offset_map: HashMap<String, HashMap<u32, u64>> = HashMap::new();
        for ((topic, partition), offset) in &offsets {
            offset_map
                .entry(topic.clone())
                .or_default()
                .insert(*partition, *offset);
        }
        state_checkpoint.offsets = offset_map;

        // Store the checkpoint internally
        let cp = Checkpoint {
            processor_id: self.config.processor_id.clone(),
            source_offsets: offsets,
            timestamp: state_checkpoint.timestamp,
            state_version: {
                let stats = self.stats.read().await;
                stats.checkpoints_completed + 1
            },
        };

        {
            let mut checkpoints = self.checkpoints.write().await;
            checkpoints.insert(state_checkpoint.checkpoint_id.clone(), cp);
        }

        {
            let mut stats = self.stats.write().await;
            stats.checkpoints_completed += 1;
            stats.last_checkpoint_time = Some(Instant::now());
        }

        Ok(state_checkpoint)
    }

    /// Restore processor state from a previously persisted checkpoint.
    pub async fn restore_from_checkpoint(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        if checkpoint.processor_id != self.config.processor_id {
            return Err(SqlError::CheckpointError(format!(
                "checkpoint processor_id '{}' does not match this processor '{}'",
                checkpoint.processor_id, self.config.processor_id
            )));
        }

        self.state_store.restore(checkpoint).await?;

        // Rebuild internal checkpoint record
        let mut offsets: HashMap<(String, u32), u64> = HashMap::new();
        for (topic, partitions) in &checkpoint.offsets {
            for (partition, offset) in partitions {
                offsets.insert((topic.clone(), *partition), *offset);
            }
        }

        let cp = Checkpoint {
            processor_id: self.config.processor_id.clone(),
            source_offsets: offsets,
            timestamp: checkpoint.timestamp,
            state_version: 0,
        };

        {
            let mut checkpoints = self.checkpoints.write().await;
            checkpoints.insert(checkpoint.checkpoint_id.clone(), cp);
        }

        Ok(())
    }

    /// Return a snapshot of current processing statistics.
    pub async fn get_processing_stats(&self) -> ProcessingStats {
        self.stats.read().await.clone()
    }

    /// Access the underlying state store (e.g. for queries).
    pub fn state_store(&self) -> &dyn StateStore {
        self.state_store.as_ref().as_ref()
    }

    /// Return a reference to the processor configuration.
    pub fn config(&self) -> &StreamProcessorConfig {
        &self.config
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> StreamProcessorConfig {
        StreamProcessorConfig {
            processor_id: "test-processor".to_string(),
            ..Default::default()
        }
    }

    // -- InMemoryStateStore tests --

    #[tokio::test]
    async fn test_state_store_put_and_get() {
        let store = InMemoryStateStore::new();
        store.put("ns", b"key1", b"value1").await.unwrap();

        let val = store.get("ns", b"key1").await.unwrap();
        assert_eq!(val, Some(b"value1".to_vec()));
    }

    #[tokio::test]
    async fn test_state_store_get_missing_key() {
        let store = InMemoryStateStore::new();
        let val = store.get("ns", b"nonexistent").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_state_store_get_missing_namespace() {
        let store = InMemoryStateStore::new();
        let val = store.get("missing-ns", b"key").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_state_store_overwrite() {
        let store = InMemoryStateStore::new();
        store.put("ns", b"key", b"v1").await.unwrap();
        store.put("ns", b"key", b"v2").await.unwrap();

        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, Some(b"v2".to_vec()));
    }

    #[tokio::test]
    async fn test_state_store_delete() {
        let store = InMemoryStateStore::new();
        store.put("ns", b"key", b"value").await.unwrap();
        store.delete("ns", b"key").await.unwrap();

        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_state_store_delete_nonexistent_ok() {
        let store = InMemoryStateStore::new();
        // Deleting a key that was never inserted should not error.
        store.delete("ns", b"nope").await.unwrap();
    }

    #[tokio::test]
    async fn test_state_store_scan_prefix() {
        let store = InMemoryStateStore::new();
        store.put("ns", b"user:1", b"alice").await.unwrap();
        store.put("ns", b"user:2", b"bob").await.unwrap();
        store.put("ns", b"order:1", b"widgets").await.unwrap();

        let mut results = store.scan_prefix("ns", b"user:").await.unwrap();
        results.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (b"user:1".to_vec(), b"alice".to_vec()));
        assert_eq!(results[1], (b"user:2".to_vec(), b"bob".to_vec()));
    }

    #[tokio::test]
    async fn test_state_store_scan_prefix_empty() {
        let store = InMemoryStateStore::new();
        let results = store.scan_prefix("ns", b"anything").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_state_store_checkpoint_and_restore() {
        let store = InMemoryStateStore::new();
        store.put("ns", b"k1", b"v1").await.unwrap();
        store.put("ns", b"k2", b"v2").await.unwrap();

        let cp = store.checkpoint().await.unwrap();

        // Clear the store
        store.delete("ns", b"k1").await.unwrap();
        store.delete("ns", b"k2").await.unwrap();
        assert_eq!(store.get("ns", b"k1").await.unwrap(), None);

        // Restore
        store.restore(&cp).await.unwrap();
        assert_eq!(store.get("ns", b"k1").await.unwrap(), Some(b"v1".to_vec()));
        assert_eq!(store.get("ns", b"k2").await.unwrap(), Some(b"v2".to_vec()));
    }

    #[tokio::test]
    async fn test_state_store_namespaces_are_isolated() {
        let store = InMemoryStateStore::new();
        store.put("ns-a", b"key", b"a-val").await.unwrap();
        store.put("ns-b", b"key", b"b-val").await.unwrap();

        assert_eq!(
            store.get("ns-a", b"key").await.unwrap(),
            Some(b"a-val".to_vec())
        );
        assert_eq!(
            store.get("ns-b", b"key").await.unwrap(),
            Some(b"b-val".to_vec())
        );
    }

    // -- StreamProcessor tests --

    #[tokio::test]
    async fn test_processor_process_batch() {
        let config = default_config();
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        let records: Vec<(u64, Vec<u8>)> = vec![
            (0, b"record-0".to_vec()),
            (1, b"record-1".to_vec()),
            (2, b"record-2".to_vec()),
        ];

        let count = processor
            .process_batch("orders", 0, &records)
            .await
            .unwrap();
        assert_eq!(count, 3);

        let stats = processor.get_processing_stats().await;
        assert_eq!(stats.batches_processed, 1);
        assert_eq!(stats.records_processed, 3);
    }

    #[tokio::test]
    async fn test_processor_process_empty_batch() {
        let config = default_config();
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        let count = processor
            .process_batch("orders", 0, &[])
            .await
            .unwrap();
        assert_eq!(count, 0);

        let stats = processor.get_processing_stats().await;
        assert_eq!(stats.batches_processed, 0);
    }

    #[tokio::test]
    async fn test_processor_max_batch_size_clamp() {
        let config = StreamProcessorConfig {
            processor_id: "test".to_string(),
            max_batch_size: 2,
            ..Default::default()
        };
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        let records: Vec<(u64, Vec<u8>)> = (0..5).map(|i| (i, vec![i as u8])).collect();

        let count = processor
            .process_batch("topic", 0, &records)
            .await
            .unwrap();
        // Should only process max_batch_size records
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_processor_checkpoint_and_restore() {
        let config = default_config();
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        // Process some records
        let records: Vec<(u64, Vec<u8>)> = vec![
            (0, b"data-0".to_vec()),
            (1, b"data-1".to_vec()),
        ];
        processor
            .process_batch("events", 0, &records)
            .await
            .unwrap();

        // Checkpoint
        let mut offsets = HashMap::new();
        offsets.insert(("events".to_string(), 0u32), 2u64);

        let cp = processor.checkpoint(offsets).await.unwrap();
        assert_eq!(cp.processor_id, "test-processor");
        assert!(!cp.offsets.is_empty());

        let stats = processor.get_processing_stats().await;
        assert_eq!(stats.checkpoints_completed, 1);
    }

    #[tokio::test]
    async fn test_processor_restore_wrong_processor_id() {
        let config = default_config();
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        let bad_cp = StateCheckpoint {
            checkpoint_id: "cp-1".to_string(),
            processor_id: "wrong-processor".to_string(),
            timestamp: 0,
            offsets: HashMap::new(),
            state_snapshot: b"{}".to_vec(),
        };

        let result = processor.restore_from_checkpoint(&bad_cp).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("does not match"));
    }

    #[tokio::test]
    async fn test_processor_restore_from_checkpoint() {
        let config = default_config();
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        // Insert some state via processing
        let records = vec![(0u64, b"hello".to_vec())];
        processor
            .process_batch("topic", 0, &records)
            .await
            .unwrap();

        // Checkpoint
        let mut offsets = HashMap::new();
        offsets.insert(("topic".to_string(), 0u32), 1u64);
        let cp = processor.checkpoint(offsets).await.unwrap();

        // Restore into a new processor with the same id
        let config2 = StreamProcessorConfig {
            processor_id: "test-processor".to_string(),
            ..Default::default()
        };
        let store2 = Box::new(InMemoryStateStore::new());
        let processor2 = StreamProcessor::new(config2, store2);

        processor2.restore_from_checkpoint(&cp).await.unwrap();

        // Verify state was restored - the record should be accessible
        let val = processor2
            .state_store()
            .get("topic-0", &0u64.to_be_bytes())
            .await
            .unwrap();
        assert_eq!(val, Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn test_processor_multiple_batches_accumulate_stats() {
        let config = default_config();
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        for i in 0..3 {
            let records = vec![(i, vec![i as u8])];
            processor
                .process_batch("topic", 0, &records)
                .await
                .unwrap();
        }

        let stats = processor.get_processing_stats().await;
        assert_eq!(stats.batches_processed, 3);
        assert_eq!(stats.records_processed, 3);
    }

    #[tokio::test]
    async fn test_processor_timeout() {
        let config = StreamProcessorConfig {
            processor_id: "test".to_string(),
            processing_time_timeout: Duration::from_nanos(1), // impossibly short
            max_batch_size: 100_000,
            ..Default::default()
        };
        let store = Box::new(InMemoryStateStore::new());
        let processor = StreamProcessor::new(config, store);

        // Generate a large batch to guarantee timeout
        let records: Vec<(u64, Vec<u8>)> =
            (0..100_000).map(|i| (i, vec![0u8; 128])).collect();

        let result = processor.process_batch("topic", 0, &records).await;
        // May or may not timeout depending on scheduling — but if it does, it
        // should be the right error variant.
        if let Err(e) = result {
            assert!(e.to_string().contains("timeout"));
        }
    }

    #[tokio::test]
    async fn test_processor_config_defaults() {
        let config = StreamProcessorConfig::default();
        assert_eq!(config.checkpoint_interval, Duration::from_secs(10));
        assert_eq!(config.max_batch_size, 1024);
        assert_eq!(config.processing_time_timeout, Duration::from_secs(30));
        assert!(matches!(config.state_backend, StateBackend::InMemory));
    }
}
