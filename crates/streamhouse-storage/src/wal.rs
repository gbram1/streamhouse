//! Write-Ahead Log (WAL) for Durability
//!
//! Provides local disk durability before S3 upload to prevent data loss on agent crashes.
//!
//! ## Problem
//!
//! Without WAL, unflushed data in SegmentBuffer (in-memory) is LOST if agent crashes
//! before segment flush to S3. This is the same issue as "Glacier Kafka" multi-part PUT approach.
//!
//! ## Solution
//!
//! Write records to a local sequential log (WAL) before adding to in-memory buffer.
//! On agent restart, replay WAL to recover unflushed records.
//!
//! ## Architecture
//!
//! ```text
//! Producer → Agent → WAL (disk) → SegmentBuffer (RAM) → S3
//!                     ↓
//!                 (durable!)
//! ```
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
//! └─────────────┴──────────┴───────────┴──────────┴─────────┘
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
//! // Append record
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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// WAL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALConfig {
    /// Directory to store WAL files
    pub directory: PathBuf,

    /// Sync policy for fsync calls
    pub sync_policy: SyncPolicy,

    /// Maximum size of WAL file before rotation (bytes)
    pub max_size_bytes: u64,
}

impl Default for WALConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("./data/wal"),
            sync_policy: SyncPolicy::Interval {
                interval: Duration::from_millis(100),
            },
            max_size_bytes: 1024 * 1024 * 1024, // 1GB
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

/// Write-Ahead Log for a single partition
pub struct WAL {
    /// Topic name
    topic: String,

    /// Partition ID
    partition_id: u32,

    /// Path to WAL file
    path: PathBuf,

    /// File handle for writing
    file: Mutex<File>,

    /// Configuration
    config: WALConfig,

    /// Current file size
    current_size: Mutex<u64>,

    /// Last sync timestamp
    last_sync: Mutex<SystemTime>,
}

impl WAL {
    /// Open or create a WAL for the given topic/partition
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
        let current_size = metadata.len();

        info!(
            topic = topic,
            partition = partition_id,
            path = ?path,
            size = current_size,
            "WAL opened"
        );

        Ok(Self {
            topic: topic.to_string(),
            partition_id,
            path,
            file: Mutex::new(file),
            config,
            current_size: Mutex::new(current_size),
            last_sync: Mutex::new(SystemTime::now()),
        })
    }

    /// Append a record to the WAL
    ///
    /// Format:
    /// - Record size (4 bytes)
    /// - CRC32 checksum (4 bytes)
    /// - Timestamp (8 bytes, milliseconds since epoch)
    /// - Key size (4 bytes)
    /// - Key data (N bytes)
    /// - Value size (4 bytes)
    /// - Value data (M bytes)
    pub async fn append(&self, key: Option<&[u8]>, value: &[u8]) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Calculate record size
        let key_size = key.map(|k| k.len()).unwrap_or(0) as u32;
        let value_size = value.len() as u32;
        let record_size = 4 + 8 + 4 + key_size + 4 + value_size; // CRC(4) + timestamp(8) + key_size(4) + key(N) + value_size(4) + value(M)

        // Build record buffer
        let mut buffer = Vec::with_capacity(record_size as usize + 4); // +4 for record size field

        // Record size
        buffer.extend_from_slice(&record_size.to_le_bytes());

        // Calculate CRC32 over (timestamp, key_size, key, value_size, value)
        let mut crc = crc32fast::Hasher::new();
        crc.update(&timestamp.to_le_bytes());
        crc.update(&key_size.to_le_bytes());
        if let Some(k) = key {
            crc.update(k);
        }
        crc.update(&value_size.to_le_bytes());
        crc.update(value);
        let checksum = crc.finalize();

        // CRC32
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

        // Write to file
        let mut file = self.file.lock().await;
        file.write_all(&buffer).await?;

        // Update file size
        let mut size = self.current_size.lock().await;
        *size += buffer.len() as u64;

        // Sync based on policy
        let should_sync = match self.config.sync_policy {
            SyncPolicy::Always => true,
            SyncPolicy::Interval { interval } => {
                let mut last_sync = self.last_sync.lock().await;
                let elapsed = SystemTime::now()
                    .duration_since(*last_sync)
                    .unwrap_or(Duration::ZERO);

                if elapsed >= interval {
                    *last_sync = SystemTime::now();
                    true
                } else {
                    false
                }
            }
            SyncPolicy::Never => false,
        };

        if should_sync {
            file.sync_all().await?;
            debug!(
                topic = self.topic,
                partition = self.partition_id,
                "WAL synced"
            );
        }

        Ok(())
    }

    /// Recover all records from the WAL
    ///
    /// Reads the entire WAL file and returns all valid records.
    /// Skips corrupted records (CRC mismatch) with a warning.
    pub async fn recover(&self) -> Result<Vec<WALRecord>> {
        let mut file = File::open(&self.path).await?;
        let mut reader = BufReader::new(&mut file);
        let mut records = Vec::new();

        loop {
            // Read record size
            let mut size_buf = [0u8; 4];
            match reader.read_exact(&mut size_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // End of file
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
                    // Partial record at end of file (corruption)
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

            // Calculate CRC over remaining data
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

    /// Truncate the WAL file (after successful S3 upload)
    ///
    /// This removes all records from the WAL, resetting it to empty.
    pub async fn truncate(&self) -> Result<()> {
        let mut file = self.file.lock().await;

        // Seek to start and set length to 0
        file.seek(std::io::SeekFrom::Start(0)).await?;
        file.set_len(0).await?;
        file.sync_all().await?;

        // Reset size
        let mut size = self.current_size.lock().await;
        *size = 0;

        info!(
            topic = self.topic,
            partition = self.partition_id,
            "WAL truncated"
        );

        Ok(())
    }

    /// Get current WAL file size
    pub async fn size(&self) -> u64 {
        *self.current_size.lock().await
    }

    /// Force sync the WAL to disk
    ///
    /// This is useful for testing or when you need to ensure data is durable
    /// before performing other operations (like recovery).
    pub async fn sync(&self) -> Result<()> {
        let file = self.file.lock().await;
        file.sync_all().await?;
        Ok(())
    }

    /// Delete the WAL file
    pub async fn delete(&self) -> Result<()> {
        drop(self.file.lock().await); // Close file
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
}
