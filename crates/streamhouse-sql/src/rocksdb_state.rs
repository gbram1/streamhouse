//! RocksDB-backed state store implementation
//!
//! Provides a persistent [`StateStore`] backed by RocksDB. State is isolated
//! by namespace using a key-prefix scheme (`{namespace}\x00{key}`), and
//! supports checkpointing / restore for fault-tolerant stream processing.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use rocksdb::{Options, DB};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::SqlError;
use crate::streaming::{StateCheckpoint, StateStore};
use crate::Result;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for a [`RocksDbStateStore`].
#[derive(Debug, Clone)]
pub struct RocksDbConfig {
    /// Filesystem path for the RocksDB database.
    pub path: String,
    /// Create the database directory if it does not exist.
    pub create_if_missing: bool,
    /// Maximum number of open files RocksDB may use.
    pub max_open_files: Option<i32>,
}

impl Default for RocksDbConfig {
    fn default() -> Self {
        Self {
            path: "/tmp/streamhouse-state".to_string(),
            create_if_missing: true,
            max_open_files: Some(256),
        }
    }
}

// ---------------------------------------------------------------------------
// TTL entry wrapper
// ---------------------------------------------------------------------------

/// A value wrapper that optionally includes a TTL expiry timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TtlEntry {
    value: Vec<u8>,
    /// `None` means no TTL (lives forever).
    expires_at_ms: Option<i64>,
}

impl TtlEntry {
    fn new(value: Vec<u8>, ttl_ms: Option<i64>) -> Self {
        let expires_at_ms =
            ttl_ms.map(|ttl| chrono::Utc::now().timestamp_millis() + ttl);
        Self { value, expires_at_ms }
    }

    fn is_expired(&self) -> bool {
        match self.expires_at_ms {
            Some(exp) => chrono::Utc::now().timestamp_millis() > exp,
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a RocksDB key by prefixing the namespace: `{namespace}\x00{key}`.
fn make_prefixed_key(namespace: &str, key: &[u8]) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(namespace.len() + 1 + key.len());
    prefixed.extend_from_slice(namespace.as_bytes());
    prefixed.push(0x00);
    prefixed.extend_from_slice(key);
    prefixed
}

/// Strip the namespace prefix from a raw RocksDB key.
///
/// Returns `None` if the key does not start with `{namespace}\x00`.
#[allow(dead_code)]
fn strip_namespace_prefix(namespace: &str, raw_key: &[u8]) -> Option<Vec<u8>> {
    let prefix = {
        let mut p = Vec::with_capacity(namespace.len() + 1);
        p.extend_from_slice(namespace.as_bytes());
        p.push(0x00);
        p
    };
    if raw_key.starts_with(&prefix) {
        Some(raw_key[prefix.len()..].to_vec())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// State snapshot serialisation
// ---------------------------------------------------------------------------

/// Serialisable representation of a single state entry (used for checkpoints).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateEntry {
    namespace: String,
    key: Vec<u8>,
    value: Vec<u8>,
}

// ---------------------------------------------------------------------------
// RocksDbStateStore
// ---------------------------------------------------------------------------

/// A persistent [`StateStore`] backed by RocksDB.
pub struct RocksDbStateStore {
    db: Arc<RwLock<DB>>,
}

impl RocksDbStateStore {
    /// Open (or create) a RocksDB-backed state store.
    pub fn open(config: &RocksDbConfig) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(config.create_if_missing);
        if let Some(max_files) = config.max_open_files {
            opts.set_max_open_files(max_files);
        }

        let db = DB::open(&opts, &config.path).map_err(|e| {
            SqlError::StateStoreError(format!("failed to open RocksDB at {}: {e}", config.path))
        })?;

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
        })
    }

    /// Open a RocksDB state store at the given path with default options.
    pub fn open_default(path: impl AsRef<Path>) -> Result<Self> {
        let config = RocksDbConfig {
            path: path.as_ref().to_string_lossy().to_string(),
            create_if_missing: true,
            max_open_files: Some(256),
        };
        Self::open(&config)
    }

    /// Put a value with an optional TTL (in milliseconds).
    pub async fn put_with_ttl(
        &self,
        namespace: &str,
        key: &[u8],
        value: &[u8],
        ttl_ms: Option<i64>,
    ) -> Result<()> {
        let prefixed = make_prefixed_key(namespace, key);
        let entry = TtlEntry::new(value.to_vec(), ttl_ms);
        let encoded = bincode::serialize(&entry).map_err(|e| {
            SqlError::StateStoreError(format!("failed to serialize TTL entry: {e}"))
        })?;
        let db = self.db.write().await;
        db.put(&prefixed, &encoded).map_err(|e| {
            SqlError::StateStoreError(format!("RocksDB put error: {e}"))
        })
    }

    /// Collect all entries that belong to a given namespace.
    #[allow(dead_code)]
    async fn collect_namespace(
        &self,
        namespace: &str,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let prefix = {
            let mut p = Vec::with_capacity(namespace.len() + 1);
            p.extend_from_slice(namespace.as_bytes());
            p.push(0x00);
            p
        };
        let db = self.db.read().await;
        let iter = db.prefix_iterator(&prefix);
        let mut results = Vec::new();
        for item in iter {
            let (raw_key, raw_val) = item.map_err(|e| {
                SqlError::StateStoreError(format!("RocksDB iterator error: {e}"))
            })?;
            if !raw_key.starts_with(&prefix) {
                break;
            }
            let stripped_key = raw_key[prefix.len()..].to_vec();
            // Decode TTL entry
            if let Ok(entry) = bincode::deserialize::<TtlEntry>(&raw_val) {
                if !entry.is_expired() {
                    results.push((stripped_key, entry.value));
                }
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl StateStore for RocksDbStateStore {
    async fn get(&self, namespace: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let prefixed = make_prefixed_key(namespace, key);
        let db = self.db.read().await;
        match db.get(&prefixed) {
            Ok(Some(raw)) => {
                let entry: TtlEntry = bincode::deserialize(&raw).map_err(|e| {
                    SqlError::StateStoreError(format!("failed to deserialize TTL entry: {e}"))
                })?;
                if entry.is_expired() {
                    Ok(None)
                } else {
                    Ok(Some(entry.value))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(SqlError::StateStoreError(format!("RocksDB get error: {e}"))),
        }
    }

    async fn put(&self, namespace: &str, key: &[u8], value: &[u8]) -> Result<()> {
        self.put_with_ttl(namespace, key, value, None).await
    }

    async fn delete(&self, namespace: &str, key: &[u8]) -> Result<()> {
        let prefixed = make_prefixed_key(namespace, key);
        let db = self.db.write().await;
        db.delete(&prefixed).map_err(|e| {
            SqlError::StateStoreError(format!("RocksDB delete error: {e}"))
        })
    }

    async fn scan_prefix(
        &self,
        namespace: &str,
        prefix: &[u8],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let full_prefix = make_prefixed_key(namespace, prefix);
        let ns_prefix = {
            let mut p = Vec::with_capacity(namespace.len() + 1);
            p.extend_from_slice(namespace.as_bytes());
            p.push(0x00);
            p
        };
        let db = self.db.read().await;
        let iter = db.prefix_iterator(&full_prefix);
        let mut results = Vec::new();
        for item in iter {
            let (raw_key, raw_val) = item.map_err(|e| {
                SqlError::StateStoreError(format!("RocksDB iterator error: {e}"))
            })?;
            if !raw_key.starts_with(&full_prefix) {
                break;
            }
            let stripped_key = raw_key[ns_prefix.len()..].to_vec();
            if let Ok(entry) = bincode::deserialize::<TtlEntry>(&raw_val) {
                if !entry.is_expired() {
                    results.push((stripped_key, entry.value));
                }
            }
        }
        Ok(results)
    }

    async fn checkpoint(&self) -> Result<StateCheckpoint> {
        // Iterate all entries and collect them
        let db = self.db.read().await;
        let iter = db.iterator(rocksdb::IteratorMode::Start);
        let mut entries = Vec::new();
        for item in iter {
            let (raw_key, raw_val) = item.map_err(|e| {
                SqlError::StateStoreError(format!("RocksDB iterator error: {e}"))
            })?;
            // Decode the namespace from the key
            let raw_key_vec = raw_key.to_vec();
            if let Some(sep_pos) = raw_key_vec.iter().position(|&b| b == 0x00) {
                let namespace = String::from_utf8_lossy(&raw_key_vec[..sep_pos]).to_string();
                let key = raw_key_vec[sep_pos + 1..].to_vec();
                // Decode TTL entry
                if let Ok(ttl_entry) = bincode::deserialize::<TtlEntry>(&raw_val) {
                    if !ttl_entry.is_expired() {
                        entries.push(StateEntry {
                            namespace,
                            key,
                            value: ttl_entry.value,
                        });
                    }
                }
            }
        }

        let snapshot = bincode::serialize(&entries).map_err(|e| {
            SqlError::CheckpointError(format!("failed to serialize state entries: {e}"))
        })?;

        Ok(StateCheckpoint {
            checkpoint_id: Uuid::new_v4().to_string(),
            processor_id: String::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            offsets: HashMap::new(),
            state_snapshot: snapshot,
        })
    }

    async fn restore(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        let entries: Vec<StateEntry> =
            bincode::deserialize(&checkpoint.state_snapshot).map_err(|e| {
                SqlError::CheckpointError(format!("failed to deserialize state entries: {e}"))
            })?;

        let db = self.db.write().await;

        // Clear existing data
        let iter = db.iterator(rocksdb::IteratorMode::Start);
        let keys_to_delete: Vec<Vec<u8>> = iter
            .filter_map(|item| item.ok().map(|(k, _)| k.to_vec()))
            .collect();
        for key in keys_to_delete {
            db.delete(&key).map_err(|e| {
                SqlError::StateStoreError(format!("RocksDB delete error during restore: {e}"))
            })?;
        }

        // Re-insert from checkpoint
        for entry in entries {
            let prefixed = make_prefixed_key(&entry.namespace, &entry.key);
            let ttl_entry = TtlEntry::new(entry.value, None);
            let encoded = bincode::serialize(&ttl_entry).map_err(|e| {
                SqlError::StateStoreError(format!("failed to serialize entry during restore: {e}"))
            })?;
            db.put(&prefixed, &encoded).map_err(|e| {
                SqlError::StateStoreError(format!("RocksDB put error during restore: {e}"))
            })?;
        }

        Ok(())
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_temp_store() -> (RocksDbStateStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let config = RocksDbConfig {
            path: dir.path().to_string_lossy().to_string(),
            create_if_missing: true,
            max_open_files: Some(64),
        };
        let store = RocksDbStateStore::open(&config).unwrap();
        (store, dir)
    }

    // -- basic put / get --

    #[tokio::test]
    async fn test_put_and_get() {
        let (store, _dir) = open_temp_store();
        store.put("ns", b"key1", b"value1").await.unwrap();
        let val = store.get("ns", b"key1").await.unwrap();
        assert_eq!(val, Some(b"value1".to_vec()));
    }

    #[tokio::test]
    async fn test_get_missing_key() {
        let (store, _dir) = open_temp_store();
        let val = store.get("ns", b"nonexistent").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_get_missing_namespace() {
        let (store, _dir) = open_temp_store();
        let val = store.get("missing-ns", b"key").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_overwrite_value() {
        let (store, _dir) = open_temp_store();
        store.put("ns", b"key", b"v1").await.unwrap();
        store.put("ns", b"key", b"v2").await.unwrap();
        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, Some(b"v2".to_vec()));
    }

    // -- delete --

    #[tokio::test]
    async fn test_delete_existing_key() {
        let (store, _dir) = open_temp_store();
        store.put("ns", b"key", b"val").await.unwrap();
        store.delete("ns", b"key").await.unwrap();
        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_key_is_ok() {
        let (store, _dir) = open_temp_store();
        store.delete("ns", b"nope").await.unwrap();
    }

    // -- scan_prefix --

    #[tokio::test]
    async fn test_scan_prefix() {
        let (store, _dir) = open_temp_store();
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
    async fn test_scan_prefix_empty_result() {
        let (store, _dir) = open_temp_store();
        let results = store.scan_prefix("ns", b"anything").await.unwrap();
        assert!(results.is_empty());
    }

    // -- namespace isolation --

    #[tokio::test]
    async fn test_namespace_isolation() {
        let (store, _dir) = open_temp_store();
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

    // -- checkpoint / restore --

    #[tokio::test]
    async fn test_checkpoint_and_restore() {
        let (store, _dir) = open_temp_store();
        store.put("ns", b"k1", b"v1").await.unwrap();
        store.put("ns", b"k2", b"v2").await.unwrap();

        let cp = store.checkpoint().await.unwrap();
        assert!(!cp.checkpoint_id.is_empty());

        // Delete everything
        store.delete("ns", b"k1").await.unwrap();
        store.delete("ns", b"k2").await.unwrap();
        assert_eq!(store.get("ns", b"k1").await.unwrap(), None);

        // Restore
        store.restore(&cp).await.unwrap();
        assert_eq!(store.get("ns", b"k1").await.unwrap(), Some(b"v1".to_vec()));
        assert_eq!(store.get("ns", b"k2").await.unwrap(), Some(b"v2".to_vec()));
    }

    #[tokio::test]
    async fn test_checkpoint_multiple_namespaces() {
        let (store, _dir) = open_temp_store();
        store.put("ns1", b"a", b"1").await.unwrap();
        store.put("ns2", b"b", b"2").await.unwrap();

        let cp = store.checkpoint().await.unwrap();

        store.delete("ns1", b"a").await.unwrap();
        store.delete("ns2", b"b").await.unwrap();

        store.restore(&cp).await.unwrap();
        assert_eq!(store.get("ns1", b"a").await.unwrap(), Some(b"1".to_vec()));
        assert_eq!(store.get("ns2", b"b").await.unwrap(), Some(b"2".to_vec()));
    }

    #[tokio::test]
    async fn test_restore_clears_existing_data() {
        let (store, _dir) = open_temp_store();
        store.put("ns", b"k1", b"v1").await.unwrap();
        let cp = store.checkpoint().await.unwrap();

        // Add extra data after checkpoint
        store.put("ns", b"k2", b"v2").await.unwrap();

        // Restore should clear k2
        store.restore(&cp).await.unwrap();
        assert_eq!(store.get("ns", b"k1").await.unwrap(), Some(b"v1".to_vec()));
        assert_eq!(store.get("ns", b"k2").await.unwrap(), None);
    }

    // -- TTL support --

    #[tokio::test]
    async fn test_put_with_ttl_alive() {
        let (store, _dir) = open_temp_store();
        // TTL of 60 seconds — should still be alive
        store
            .put_with_ttl("ns", b"key", b"val", Some(60_000))
            .await
            .unwrap();
        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, Some(b"val".to_vec()));
    }

    #[tokio::test]
    async fn test_put_with_ttl_expired() {
        let (store, _dir) = open_temp_store();
        // TTL of 0 ms — should expire immediately
        store
            .put_with_ttl("ns", b"key", b"val", Some(0))
            .await
            .unwrap();
        // Sleep a tiny bit to ensure time has moved past the expiry
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_put_without_ttl_persists() {
        let (store, _dir) = open_temp_store();
        store.put_with_ttl("ns", b"key", b"val", None).await.unwrap();
        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, Some(b"val".to_vec()));
    }

    // -- helper functions --

    #[test]
    fn test_make_prefixed_key() {
        let key = make_prefixed_key("myns", b"mykey");
        assert_eq!(key, b"myns\x00mykey".to_vec());
    }

    #[test]
    fn test_strip_namespace_prefix_valid() {
        let raw = b"myns\x00mykey".to_vec();
        let stripped = strip_namespace_prefix("myns", &raw);
        assert_eq!(stripped, Some(b"mykey".to_vec()));
    }

    #[test]
    fn test_strip_namespace_prefix_invalid() {
        let raw = b"otherns\x00mykey".to_vec();
        let stripped = strip_namespace_prefix("myns", &raw);
        assert_eq!(stripped, None);
    }

    #[test]
    fn test_strip_namespace_prefix_empty_key() {
        let raw = b"ns\x00".to_vec();
        let stripped = strip_namespace_prefix("ns", &raw);
        assert_eq!(stripped, Some(Vec::new()));
    }

    // -- open_default --

    #[tokio::test]
    async fn test_open_default() {
        let dir = TempDir::new().unwrap();
        let store = RocksDbStateStore::open_default(dir.path()).unwrap();
        store.put("ns", b"key", b"val").await.unwrap();
        let val = store.get("ns", b"key").await.unwrap();
        assert_eq!(val, Some(b"val".to_vec()));
    }

    // -- config defaults --

    #[test]
    fn test_config_default() {
        let cfg = RocksDbConfig::default();
        assert!(cfg.create_if_missing);
        assert_eq!(cfg.max_open_files, Some(256));
    }

    // -- collect_namespace --

    #[tokio::test]
    async fn test_collect_namespace() {
        let (store, _dir) = open_temp_store();
        store.put("ns", b"a", b"1").await.unwrap();
        store.put("ns", b"b", b"2").await.unwrap();
        store.put("other", b"c", b"3").await.unwrap();

        let entries = store.collect_namespace("ns").await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    // -- large batch --

    #[tokio::test]
    async fn test_large_batch() {
        let (store, _dir) = open_temp_store();
        for i in 0u32..100 {
            let key = i.to_be_bytes();
            let val = format!("value-{i}");
            store.put("ns", &key, val.as_bytes()).await.unwrap();
        }
        let results = store.scan_prefix("ns", b"").await.unwrap();
        assert_eq!(results.len(), 100);
    }

    // -- empty checkpoint --

    #[tokio::test]
    async fn test_empty_checkpoint() {
        let (store, _dir) = open_temp_store();
        let cp = store.checkpoint().await.unwrap();
        assert!(!cp.checkpoint_id.is_empty());
        assert!(cp.state_snapshot.len() > 0);

        // Restoring empty snapshot should work fine
        store.restore(&cp).await.unwrap();
    }
}
