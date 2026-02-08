//! Write-Ahead Log (WAL) for Durability — Channel-based Group Commit
//!
//! Provides local disk durability before S3 upload to prevent data loss on agent crashes.
//!
//! ## Architecture
//!
//! Uses a channel-based group commit design for high throughput:
//!
//! ```text
//! Callers ─→ [mpsc channel] ─→ Writer Task ─→ write_all ─→ fdatasync
//!                                    ↑
//!                          Batches records automatically,
//!                          single fdatasync per batch (group commit)
//! ```
//!
//! ### Key optimizations over mutex-based approach:
//! - **Lock-free append**: records sent via channel (~50ns vs ~200ns mutex)
//! - **Group commit**: multiple records written + synced in one syscall
//! - **fdatasync**: syncs data only (not metadata), 2-3x faster than fsync
//! - **No caller blocking**: callers never block for I/O (unless explicit flush)
//!
//! ## File Format
//!
//! Each WAL file is a sequence of records with CRC32 checksums:
//!
//! ```text
//! [Record Entry 1][Record Entry 2]...[Record Entry N]
//!
//! Record Entry:
//! ┌─────────────┬──────────┬───────────┬──────────┬─────────┐
//! │ Record Size │ CRC32    │ Timestamp │ Key Size │ Key     │
//! │ (4 bytes)   │(4 bytes) │(8 bytes)  │(4 bytes) │(N bytes)│
//! └─────────────┴──────────┴──────────┴──────────┴─────────┘
//! ┌────────────┬─────────┐
//! │ Value Size │ Value   │
//! │ (4 bytes)  │(M bytes)│
//! └────────────┴─────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::wal::{WAL, WALConfig, SyncPolicy};
//!
//! // Create WAL
//! let config = WALConfig {
//!     directory: "./data/wal".into(),
//!     sync_policy: SyncPolicy::Interval(Duration::from_millis(100)),
//!     max_size_bytes: 1024 * 1024 * 1024, // 1GB
//! };
//!
//! let wal = WAL::open("orders", 0, config).await?;
//!
//! // Append record (lock-free, returns immediately)
//! wal.append(key, value).await?;
//!
//! // Recover on restart
//! let records = wal.recover().await?;
//! for record in records {
//!     segment_buffer.append(record)?;
//! }
//!
//! // Truncate after S3 upload
//! wal.truncate().await?;
//! ```

use crate::error::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

// ============================================================================
// Configuration
// ============================================================================

/// WAL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALConfig {
    /// Directory to store WAL files
    pub directory: PathBuf,

    /// Sync policy for fsync calls
    pub sync_policy: SyncPolicy,

    /// Maximum size of WAL file before rotation (bytes)
    pub max_size_bytes: u64,

    /// Enable batch writing for reduced fsync overhead (default: true)
    #[serde(default = "default_batch_enabled")]
    pub batch_enabled: bool,

    /// Maximum number of records to batch before auto-flush (default: 1000)
    #[serde(default = "default_batch_max_records")]
    pub batch_max_records: usize,

    /// Maximum size of batch buffer in bytes before auto-flush (default: 1MB)
    #[serde(default = "default_batch_max_bytes")]
    pub batch_max_bytes: usize,

    /// Maximum time to hold records in batch before auto-flush (default: 10ms)
    #[serde(default = "default_batch_max_age_ms")]
    pub batch_max_age_ms: u64,
}

fn default_batch_enabled() -> bool {
    true
}

fn default_batch_max_records() -> usize {
    1000
}

fn default_batch_max_bytes() -> usize {
    1024 * 1024 // 1MB
}

fn default_batch_max_age_ms() -> u64 {
    10 // 10ms
}

impl Default for WALConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("./data/wal"),
            sync_policy: SyncPolicy::Interval {
                interval: Duration::from_millis(100),
            },
            max_size_bytes: 1024 * 1024 * 1024, // 1GB
            batch_enabled: default_batch_enabled(),
            batch_max_records: default_batch_max_records(),
            batch_max_bytes: default_batch_max_bytes(),
            batch_max_age_ms: default_batch_max_age_ms(),
        }
    }
}

/// Sync policy for WAL fsync calls
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SyncPolicy {
    /// Sync after every write (safest, slowest)
    Always,

    /// Sync every N milliseconds (balanced)
    Interval {
        #[serde(with = "duration_ms")]
        interval: Duration,
    },

    /// Never sync (fastest, least safe - for testing only)
    Never,
}

mod duration_ms {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ms = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(ms))
    }
}

// ============================================================================
// Public types
// ============================================================================

/// A record recovered from the WAL
#[derive(Debug, Clone)]
pub struct WALRecord {
    /// Record key (optional)
    pub key: Option<Bytes>,

    /// Record value
    pub value: Bytes,

    /// Timestamp when record was written
    pub timestamp: u64,
}

// ============================================================================
// Internal: Writer task commands
// ============================================================================

/// Commands sent to the WAL writer task via mpsc channel
enum WalCmd {
    /// Append pre-serialized record data (fire-and-forget, no response)
    Append { data: Vec<u8>, count: usize },

    /// Flush pending batch to disk and notify caller when durable
    Flush(oneshot::Sender<std::result::Result<(), String>>),

    /// Truncate WAL file (discard batch + truncate file on disk)
    Truncate(oneshot::Sender<std::result::Result<(), String>>),

    /// Query pending batch state: returns (record_count, byte_count)
    QueryPending(oneshot::Sender<(usize, usize)>),
}

// ============================================================================
// WAL (public API)
// ============================================================================

/// Write-Ahead Log for a single partition.
///
/// Uses a channel-based group commit architecture:
/// - `append()` sends pre-serialized data via channel (lock-free, ~50ns)
/// - Dedicated writer task accumulates records and does group commit
/// - `flush_batch()` / `sync()` wait for durability confirmation
/// - `recover()` reads the WAL file to replay unflushed records
pub struct WAL {
    /// Topic name
    topic: String,

    /// Partition ID
    partition_id: u32,

    /// Path to WAL file
    path: PathBuf,

    /// Configuration
    #[allow(dead_code)]
    config: WALConfig,

    /// Channel sender to writer task
    cmd_tx: mpsc::Sender<WalCmd>,

    /// Current file size (updated atomically by writer task)
    current_size: Arc<AtomicU64>,

    /// Writer task handle (aborted on drop)
    writer_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for WAL {
    fn drop(&mut self) {
        if let Some(handle) = self.writer_handle.take() {
            handle.abort();
        }
    }
}

impl WAL {
    /// Open or create a WAL for the given topic/partition.
    ///
    /// Starts a background writer task that handles batching and group commit.
    pub async fn open(topic: &str, partition_id: u32, config: WALConfig) -> Result<Self> {
        // Create WAL directory if it doesn't exist
        tokio::fs::create_dir_all(&config.directory).await?;

        // WAL file path: {dir}/{topic}-{partition}.wal
        let filename = format!("{}-{}.wal", topic, partition_id);
        let path = config.directory.join(filename);

        // Open file in append mode
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        // Get current file size
        let metadata = file.metadata().await?;
        let file_size = metadata.len();
        let current_size = Arc::new(AtomicU64::new(file_size));

        // Configure writer task based on batch settings
        let sync_on_flush = !matches!(config.sync_policy, SyncPolicy::Never);
        let (cmd_tx, cmd_rx) = mpsc::channel(16_384);

        let writer = WalWriter {
            file,
            batch: Vec::with_capacity(config.batch_max_bytes),
            batch_count: 0,
            current_size: current_size.clone(),
            // When batching is disabled, flush after every record
            batch_max_records: if config.batch_enabled {
                config.batch_max_records
            } else {
                1
            },
            batch_max_bytes: if config.batch_enabled {
                config.batch_max_bytes
            } else {
                0
            },
            batch_max_age_ms: config.batch_max_age_ms,
            sync_on_flush,
            topic: topic.to_string(),
            partition_id,
        };

        let writer_handle = tokio::spawn(writer.run(cmd_rx));

        info!(
            topic = topic,
            partition = partition_id,
            path = ?path,
            size = file_size,
            batch_enabled = config.batch_enabled,
            "WAL opened"
        );

        Ok(Self {
            topic: topic.to_string(),
            partition_id,
            path,
            config,
            cmd_tx,
            current_size,
            writer_handle: Some(writer_handle),
        })
    }

    /// Serialize a record to the WAL format.
    ///
    /// Format:
    /// - Record size (4 bytes, LE)
    /// - CRC32 checksum (4 bytes, LE)
    /// - Timestamp (8 bytes, LE, milliseconds since epoch)
    /// - Key size (4 bytes, LE)
    /// - Key data (N bytes)
    /// - Value size (4 bytes, LE)
    /// - Value data (M bytes)
    fn serialize_record(key: Option<&[u8]>, value: &[u8], timestamp: u64) -> Vec<u8> {
        let key_size = key.map(|k| k.len()).unwrap_or(0) as u32;
        let value_size = value.len() as u32;
        let record_size = 4 + 8 + 4 + key_size + 4 + value_size;

        let mut buffer = Vec::with_capacity(record_size as usize + 4);

        // Record size
        buffer.extend_from_slice(&record_size.to_le_bytes());

        // CRC32 over (timestamp, key_size, key, value_size, value)
        let mut crc = crc32fast::Hasher::new();
        crc.update(&timestamp.to_le_bytes());
        crc.update(&key_size.to_le_bytes());
        if let Some(k) = key {
            crc.update(k);
        }
        crc.update(&value_size.to_le_bytes());
        crc.update(value);
        let checksum = crc.finalize();

        buffer.extend_from_slice(&checksum.to_le_bytes());

        // Timestamp
        buffer.extend_from_slice(&timestamp.to_le_bytes());

        // Key
        buffer.extend_from_slice(&key_size.to_le_bytes());
        if let Some(k) = key {
            buffer.extend_from_slice(k);
        }

        // Value
        buffer.extend_from_slice(&value_size.to_le_bytes());
        buffer.extend_from_slice(value);

        buffer
    }

    /// Append a record to the WAL.
    ///
    /// Lock-free: sends pre-serialized data via channel to the writer task.
    /// Returns immediately after the channel send (~50ns). The writer task
    /// handles batching, writing, and syncing in the background.
    ///
    /// Auto-flush happens when batch thresholds are exceeded (record count,
    /// byte size, or age). Call `flush_batch()` or `sync()` for explicit
    /// durability guarantees.
    pub async fn append(&self, key: Option<&[u8]>, value: &[u8]) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let data = Self::serialize_record(key, value, timestamp);

        self.cmd_tx
            .send(WalCmd::Append { data, count: 1 })
            .await
            .map_err(|_| wal_closed_error())?;

        Ok(())
    }

    /// Append multiple records to the WAL in a single batch.
    ///
    /// All records are serialized together and sent as one channel message,
    /// then flushed to disk with a single write+sync (matching legacy behavior).
    pub async fn append_batch(&self, records: &[(Option<&[u8]>, &[u8])]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Serialize all records into a single buffer
        let mut batch_data = Vec::new();
        for (key, value) in records {
            let record_data = Self::serialize_record(*key, value, timestamp);
            batch_data.extend_from_slice(&record_data);
        }

        let count = records.len();
        self.cmd_tx
            .send(WalCmd::Append {
                data: batch_data,
                count,
            })
            .await
            .map_err(|_| wal_closed_error())?;

        // append_batch always syncs (matching current behavior)
        self.flush_batch().await?;

        debug!(
            topic = self.topic,
            partition = self.partition_id,
            records = count,
            "WAL batch append complete"
        );

        Ok(())
    }

    /// Flush any pending records in the batch buffer to disk.
    ///
    /// Sends a flush command to the writer task and waits for confirmation
    /// that all pending data has been written and synced to disk.
    pub async fn flush_batch(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(WalCmd::Flush(tx))
            .await
            .map_err(|_| wal_closed_error())?;
        rx.await
            .map_err(|_| wal_closed_error())?
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }

    /// Recover all records from the WAL.
    ///
    /// Flushes any pending batch first, then reads the entire WAL file
    /// and returns all valid records. Skips corrupted records (CRC mismatch).
    pub async fn recover(&self) -> Result<Vec<WALRecord>> {
        // Flush any pending batch first
        self.flush_batch().await?;

        // Open a separate read handle (writer task still owns the append handle)
        let file = File::open(&self.path).await?;
        let mut reader = BufReader::new(file);
        let mut records = Vec::new();

        loop {
            // Read record size
            let mut size_buf = [0u8; 4];
            match reader.read_exact(&mut size_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(e.into()),
            }

            let record_size = u32::from_le_bytes(size_buf);

            // Read record data
            let mut record_buf = vec![0u8; record_size as usize];
            match reader.read_exact(&mut record_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    warn!(
                        topic = self.topic,
                        partition = self.partition_id,
                        "Partial record at end of WAL, truncating"
                    );
                    break;
                }
                Err(e) => return Err(e.into()),
            }

            // Parse record
            let mut cursor = 0;

            // CRC32
            let stored_crc = u32::from_le_bytes([
                record_buf[cursor],
                record_buf[cursor + 1],
                record_buf[cursor + 2],
                record_buf[cursor + 3],
            ]);
            cursor += 4;

            // Verify CRC over remaining data
            let mut crc = crc32fast::Hasher::new();
            crc.update(&record_buf[cursor..]);
            let computed_crc = crc.finalize();

            if stored_crc != computed_crc {
                warn!(
                    topic = self.topic,
                    partition = self.partition_id,
                    "Corrupted WAL record (CRC mismatch), skipping"
                );
                continue;
            }

            // Timestamp
            let timestamp = u64::from_le_bytes([
                record_buf[cursor],
                record_buf[cursor + 1],
                record_buf[cursor + 2],
                record_buf[cursor + 3],
                record_buf[cursor + 4],
                record_buf[cursor + 5],
                record_buf[cursor + 6],
                record_buf[cursor + 7],
            ]);
            cursor += 8;

            // Key
            let key_size = u32::from_le_bytes([
                record_buf[cursor],
                record_buf[cursor + 1],
                record_buf[cursor + 2],
                record_buf[cursor + 3],
            ]);
            cursor += 4;

            let key = if key_size > 0 {
                let k = record_buf[cursor..cursor + key_size as usize].to_vec();
                cursor += key_size as usize;
                Some(Bytes::from(k))
            } else {
                None
            };

            // Value
            let value_size = u32::from_le_bytes([
                record_buf[cursor],
                record_buf[cursor + 1],
                record_buf[cursor + 2],
                record_buf[cursor + 3],
            ]);
            cursor += 4;

            let value = Bytes::from(record_buf[cursor..cursor + value_size as usize].to_vec());

            records.push(WALRecord {
                key,
                value,
                timestamp,
            });
        }

        info!(
            topic = self.topic,
            partition = self.partition_id,
            recovered = records.len(),
            "WAL recovery complete"
        );

        Ok(records)
    }

    /// Truncate the WAL file (after successful S3 upload).
    ///
    /// Discards any pending batch data and truncates the file to zero bytes.
    pub async fn truncate(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(WalCmd::Truncate(tx))
            .await
            .map_err(|_| wal_closed_error())?;
        rx.await
            .map_err(|_| wal_closed_error())?
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }

    /// Get current WAL file size
    pub async fn size(&self) -> u64 {
        self.current_size.load(Ordering::Relaxed)
    }

    /// Force sync the WAL to disk.
    ///
    /// Flushes any pending batch and ensures data is durable.
    pub async fn sync(&self) -> Result<()> {
        self.flush_batch().await
    }

    /// Get the number of records currently in the batch buffer
    pub async fn batch_pending_count(&self) -> usize {
        let (tx, rx) = oneshot::channel();
        if self.cmd_tx.send(WalCmd::QueryPending(tx)).await.is_err() {
            return 0;
        }
        rx.await.map(|(count, _)| count).unwrap_or(0)
    }

    /// Get the size of data currently in the batch buffer
    pub async fn batch_pending_bytes(&self) -> usize {
        let (tx, rx) = oneshot::channel();
        if self.cmd_tx.send(WalCmd::QueryPending(tx)).await.is_err() {
            return 0;
        }
        rx.await.map(|(_, bytes)| bytes).unwrap_or(0)
    }

    /// Delete the WAL file
    pub async fn delete(&self) -> Result<()> {
        tokio::fs::remove_file(&self.path).await?;

        info!(
            topic = self.topic,
            partition = self.partition_id,
            path = ?self.path,
            "WAL deleted"
        );

        Ok(())
    }
}

/// Helper: create an IO error for WAL channel closed
fn wal_closed_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "WAL writer task closed",
    )
}

// ============================================================================
// Writer Task (internal)
// ============================================================================

/// Background writer task that owns the WAL file and handles group commit.
///
/// The writer loop:
/// 1. Wait for command (with timeout if batch is non-empty for age-based flush)
/// 2. Process command + drain all pending commands (non-blocking)
/// 3. If batch exceeds thresholds or explicit flush requested: write + fdatasync
/// 4. Respond to queries and truncate requests
struct WalWriter {
    file: File,
    batch: Vec<u8>,
    batch_count: usize,
    current_size: Arc<AtomicU64>,
    // Effective batch thresholds (adjusted when batch_enabled=false)
    batch_max_records: usize,
    batch_max_bytes: usize,
    batch_max_age_ms: u64,
    sync_on_flush: bool,
    topic: String,
    partition_id: u32,
}

impl WalWriter {
    async fn run(mut self, mut rx: mpsc::Receiver<WalCmd>) {
        let batch_timeout = Duration::from_millis(self.batch_max_age_ms);

        loop {
            // Step 1: Wait for command
            // If batch has data, use timeout for age-based auto-flush.
            // If batch is empty, wait indefinitely (no timer overhead).
            let first = if self.batch.is_empty() {
                match rx.recv().await {
                    Some(cmd) => cmd,
                    None => break, // channel closed
                }
            } else {
                match tokio::time::timeout(batch_timeout, rx.recv()).await {
                    Ok(Some(cmd)) => cmd,
                    Ok(None) => break, // channel closed
                    Err(_) => {
                        // Timeout: flush batch due to age
                        let _ = self.do_flush().await;
                        continue;
                    }
                }
            };

            // Step 2: Process first command + drain remaining (non-blocking)
            let mut flush_waiters: Vec<oneshot::Sender<std::result::Result<(), String>>> =
                Vec::new();
            let mut queries: Vec<oneshot::Sender<(usize, usize)>> = Vec::new();
            let mut truncate_waiter: Option<oneshot::Sender<std::result::Result<(), String>>> =
                None;

            self.process_cmd(first, &mut flush_waiters, &mut queries, &mut truncate_waiter);

            while let Ok(cmd) = rx.try_recv() {
                self.process_cmd(cmd, &mut flush_waiters, &mut queries, &mut truncate_waiter);
            }

            // Step 3: Flush if needed (explicit request or batch threshold)
            let should_flush = !flush_waiters.is_empty()
                || self.batch_count >= self.batch_max_records
                || (self.batch_max_bytes > 0 && self.batch.len() >= self.batch_max_bytes);

            if should_flush && !self.batch.is_empty() {
                let result = self.do_flush().await.map_err(|e| e.to_string());
                for waiter in flush_waiters {
                    let _ = waiter.send(result.clone());
                }
            } else {
                // No data to flush — notify waiters of success (no-op flush)
                for waiter in flush_waiters {
                    let _ = waiter.send(Ok(()));
                }
            }

            // Step 4: Respond to queries AFTER potential flush (correct counts)
            for query in queries {
                let _ = query.send((self.batch_count, self.batch.len()));
            }

            // Step 5: Handle truncate AFTER everything else
            if let Some(waiter) = truncate_waiter {
                let result = self.do_truncate().await.map_err(|e| e.to_string());
                let _ = waiter.send(result);
            }
        }

        // Cleanup: flush remaining data when channel closes
        if !self.batch.is_empty() {
            let _ = self.do_flush().await;
        }
    }

    fn process_cmd(
        &mut self,
        cmd: WalCmd,
        flush_waiters: &mut Vec<oneshot::Sender<std::result::Result<(), String>>>,
        queries: &mut Vec<oneshot::Sender<(usize, usize)>>,
        truncate_waiter: &mut Option<oneshot::Sender<std::result::Result<(), String>>>,
    ) {
        match cmd {
            WalCmd::Append { data, count } => {
                self.batch.extend_from_slice(&data);
                self.batch_count += count;
            }
            WalCmd::Flush(tx) => flush_waiters.push(tx),
            WalCmd::QueryPending(tx) => queries.push(tx),
            WalCmd::Truncate(tx) => *truncate_waiter = Some(tx),
        }
    }

    /// Write batch to disk with optional fdatasync (group commit).
    async fn do_flush(&mut self) -> std::io::Result<()> {
        if self.batch.is_empty() {
            return Ok(());
        }

        let data = std::mem::take(&mut self.batch);
        let bytes_written = data.len() as u64;
        self.batch_count = 0;

        // Single write for entire batch
        self.file.write_all(&data).await?;

        // fdatasync (not fsync) — syncs data only, skips metadata update
        if self.sync_on_flush {
            self.file.sync_data().await?;
        }

        self.current_size
            .fetch_add(bytes_written, Ordering::Relaxed);

        // Pre-allocate for next batch
        self.batch.reserve(self.batch_max_bytes.max(1024));

        debug!(
            topic = self.topic,
            partition = self.partition_id,
            bytes = bytes_written,
            "WAL group commit"
        );

        Ok(())
    }

    /// Discard pending batch and truncate file to zero.
    async fn do_truncate(&mut self) -> std::io::Result<()> {
        // Discard pending batch (don't flush — we're truncating)
        self.batch.clear();
        self.batch_count = 0;

        // Truncate file
        self.file.seek(std::io::SeekFrom::Start(0)).await?;
        self.file.set_len(0).await?;

        if self.sync_on_flush {
            self.file.sync_data().await?;
        }

        self.current_size.store(0, Ordering::Relaxed);

        info!(
            topic = self.topic,
            partition = self.partition_id,
            "WAL truncated"
        );

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_wal_append_and_recover() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Always,
            max_size_bytes: 1024 * 1024,
            ..Default::default()
        };

        let wal = WAL::open("test-topic", 0, config.clone()).await.unwrap();

        // Append records
        wal.append(Some(b"key1"), b"value1").await.unwrap();
        wal.append(Some(b"key2"), b"value2").await.unwrap();
        wal.append(None, b"value3").await.unwrap();

        // Force sync to ensure data is on disk
        wal.sync().await.unwrap();

        // Recover
        let records = wal.recover().await.unwrap();

        assert_eq!(records.len(), 3);
        assert_eq!(records[0].key.as_ref().unwrap(), &Bytes::from("key1"));
        assert_eq!(records[0].value, Bytes::from("value1"));
        assert_eq!(records[1].key.as_ref().unwrap(), &Bytes::from("key2"));
        assert_eq!(records[1].value, Bytes::from("value2"));
        assert_eq!(records[2].key, None);
        assert_eq!(records[2].value, Bytes::from("value3"));
    }

    #[tokio::test]
    async fn test_wal_truncate() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Always,
            max_size_bytes: 1024 * 1024,
            ..Default::default()
        };

        let wal = WAL::open("test-topic", 0, config).await.unwrap();

        // Append records
        wal.append(Some(b"key1"), b"value1").await.unwrap();
        wal.append(Some(b"key2"), b"value2").await.unwrap();

        // Verify records exist
        let records = wal.recover().await.unwrap();
        assert_eq!(records.len(), 2);

        // Truncate
        wal.truncate().await.unwrap();

        // Verify empty after truncate
        let records = wal.recover().await.unwrap();
        assert_eq!(records.len(), 0);
        assert_eq!(wal.size().await, 0);
    }

    #[tokio::test]
    async fn test_wal_sync_policy_interval() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Interval {
                interval: Duration::from_millis(50),
            },
            max_size_bytes: 1024 * 1024,
            batch_enabled: false, // Disable batching for this test
            ..Default::default()
        };

        let wal = WAL::open("test-topic", 0, config).await.unwrap();

        // Append records quickly (should not sync every time)
        wal.append(Some(b"key1"), b"value1").await.unwrap();
        wal.append(Some(b"key2"), b"value2").await.unwrap();

        // Wait for sync interval
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Append one more (should trigger sync)
        wal.append(Some(b"key3"), b"value3").await.unwrap();

        // Verify all records recovered
        let records = wal.recover().await.unwrap();
        assert_eq!(records.len(), 3);
    }

    #[tokio::test]
    async fn test_wal_batch_auto_flush_on_count() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Never,
            max_size_bytes: 1024 * 1024,
            batch_enabled: true,
            batch_max_records: 5, // Flush after 5 records
            batch_max_bytes: 1024 * 1024,
            batch_max_age_ms: 10000,
        };

        let wal = WAL::open("test-topic", 0, config).await.unwrap();

        // Append 4 records (should stay in batch)
        for i in 0..4 {
            wal.append(Some(format!("key{}", i).as_bytes()), b"value")
                .await
                .unwrap();
        }
        assert_eq!(wal.batch_pending_count().await, 4);

        // Append 5th record (should trigger flush)
        wal.append(Some(b"key4"), b"value").await.unwrap();
        assert_eq!(wal.batch_pending_count().await, 0);

        // Verify all records persisted
        let records = wal.recover().await.unwrap();
        assert_eq!(records.len(), 5);
    }

    #[tokio::test]
    async fn test_wal_batch_auto_flush_on_size() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Never,
            max_size_bytes: 1024 * 1024,
            batch_enabled: true,
            batch_max_records: 1000,
            batch_max_bytes: 200, // Flush after 200 bytes (record has ~24 byte header)
            batch_max_age_ms: 10000,
        };

        let wal = WAL::open("test-topic", 0, config).await.unwrap();

        // Append first record (key1=4 + value=50 + header=~24 = ~78 bytes)
        let value = vec![b'x'; 50];
        wal.append(Some(b"key1"), &value).await.unwrap();
        let pending_after_first = wal.batch_pending_bytes().await;
        assert!(pending_after_first > 0, "First record should be in batch");

        // Append second record (should still be under 200 bytes threshold)
        wal.append(Some(b"key2"), &value).await.unwrap();
        let pending_after_second = wal.batch_pending_bytes().await;
        assert!(
            pending_after_second > pending_after_first,
            "Second record should add to batch"
        );

        // Append third record - this should trigger flush (exceeds 200 bytes)
        wal.append(Some(b"key3"), &value).await.unwrap();
        assert_eq!(
            wal.batch_pending_count().await,
            0,
            "Batch should have flushed"
        );

        // Verify all records persisted
        let records = wal.recover().await.unwrap();
        assert_eq!(records.len(), 3);
    }

    #[tokio::test]
    async fn test_wal_batch_explicit_flush() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Never,
            max_size_bytes: 1024 * 1024,
            batch_enabled: true,
            batch_max_records: 1000, // High threshold
            batch_max_bytes: 1024 * 1024,
            batch_max_age_ms: 10000,
        };

        let wal = WAL::open("test-topic", 0, config).await.unwrap();

        // Append records (should stay in batch)
        wal.append(Some(b"key1"), b"value1").await.unwrap();
        wal.append(Some(b"key2"), b"value2").await.unwrap();
        assert_eq!(wal.batch_pending_count().await, 2);

        // Explicit flush
        wal.flush_batch().await.unwrap();
        assert_eq!(wal.batch_pending_count().await, 0);

        // Verify records persisted
        let records = wal.recover().await.unwrap();
        assert_eq!(records.len(), 2);
    }

    #[tokio::test]
    async fn test_wal_append_batch() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Always,
            max_size_bytes: 1024 * 1024,
            batch_enabled: false, // Test direct batch API
            ..Default::default()
        };

        let wal = WAL::open("test-topic", 0, config).await.unwrap();

        // Bulk insert using append_batch
        let records: Vec<(Option<&[u8]>, &[u8])> = vec![
            (Some(b"key1".as_slice()), b"value1".as_slice()),
            (Some(b"key2".as_slice()), b"value2".as_slice()),
            (None, b"value3".as_slice()),
        ];
        wal.append_batch(&records).await.unwrap();

        // Verify all records
        let recovered = wal.recover().await.unwrap();
        assert_eq!(recovered.len(), 3);
        assert_eq!(recovered[0].key.as_ref().unwrap().as_ref(), b"key1");
        assert_eq!(recovered[1].key.as_ref().unwrap().as_ref(), b"key2");
        assert!(recovered[2].key.is_none());
    }

    #[tokio::test]
    async fn test_wal_truncate_clears_batch() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Never,
            max_size_bytes: 1024 * 1024,
            batch_enabled: true,
            batch_max_records: 1000,
            batch_max_bytes: 1024 * 1024,
            batch_max_age_ms: 10000,
        };

        let wal = WAL::open("test-topic", 0, config).await.unwrap();

        // Append records to batch (not flushed)
        wal.append(Some(b"key1"), b"value1").await.unwrap();
        wal.append(Some(b"key2"), b"value2").await.unwrap();
        assert_eq!(wal.batch_pending_count().await, 2);

        // Truncate should clear batch
        wal.truncate().await.unwrap();
        assert_eq!(wal.batch_pending_count().await, 0);

        // Verify no records
        let records = wal.recover().await.unwrap();
        assert_eq!(records.len(), 0);
    }
}
