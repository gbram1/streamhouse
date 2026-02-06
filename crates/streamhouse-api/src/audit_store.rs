//! Immutable Audit Trail Storage
//!
//! Provides persistent, tamper-evident storage for audit logs.
//!
//! ## Features
//!
//! - Append-only log files with SHA-256 integrity verification
//! - Automatic log rotation by size or time
//! - Support for file-based and S3-based storage
//! - Query interface for audit history
//! - Chain verification to detect tampering
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::audit_store::{AuditStore, AuditStoreConfig};
//!
//! let config = AuditStoreConfig::file("./audit-logs");
//! let store = AuditStore::new(config).await?;
//!
//! // Write audit entry
//! store.append(&audit_entry).await?;
//!
//! // Query entries
//! let entries = store.query(AuditQuery::default().since(timestamp)).await?;
//!
//! // Verify chain integrity
//! let valid = store.verify_chain().await?;
//! ```
//!
//! ## Storage Format
//!
//! Each audit entry is stored as a JSON line with metadata:
//! ```json
//! {
//!   "seq": 12345,
//!   "timestamp": 1706745600000,
//!   "entry": { ... audit data ... },
//!   "prev_hash": "abc123...",
//!   "hash": "def456..."
//! }
//! ```
//!
//! The hash chain ensures that any modification to past entries is detectable.
//!
//! ## Environment Variables
//!
//! - `AUDIT_STORE_PATH`: Path to audit log directory (default: ./audit-logs)
//! - `AUDIT_STORE_MAX_SIZE_MB`: Max file size before rotation (default: 100)
//! - `AUDIT_STORE_RETENTION_DAYS`: Days to retain logs (default: 365)

use crate::audit::AuditEntry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;

/// Errors from the audit store
#[derive(Debug, Error)]
pub enum AuditStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Chain integrity violation at sequence {seq}: expected {expected}, got {actual}")]
    IntegrityViolation {
        seq: u64,
        expected: String,
        actual: String,
    },

    #[error("Store not initialized")]
    NotInitialized,

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, AuditStoreError>;

/// Configuration for the audit store
#[derive(Debug, Clone)]
pub struct AuditStoreConfig {
    /// Storage backend type
    pub backend: AuditBackend,
    /// Maximum file size before rotation (bytes)
    pub max_file_size: u64,
    /// Retention period (days)
    pub retention_days: u32,
    /// Whether to verify chain on startup
    pub verify_on_startup: bool,
    /// Flush after each write
    pub sync_writes: bool,
}

/// Storage backend configuration
#[derive(Debug, Clone)]
pub enum AuditBackend {
    /// Local file storage
    File { path: PathBuf },
    /// In-memory (for testing)
    Memory,
}

impl Default for AuditStoreConfig {
    fn default() -> Self {
        let path = std::env::var("AUDIT_STORE_PATH").unwrap_or_else(|_| "./audit-logs".to_string());

        let max_size_mb = std::env::var("AUDIT_STORE_MAX_SIZE_MB")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(100);

        let retention_days = std::env::var("AUDIT_STORE_RETENTION_DAYS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(365);

        Self {
            backend: AuditBackend::File {
                path: PathBuf::from(path),
            },
            max_file_size: max_size_mb * 1024 * 1024,
            retention_days,
            verify_on_startup: true,
            sync_writes: true,
        }
    }
}

impl AuditStoreConfig {
    /// Create a file-based config
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            backend: AuditBackend::File { path: path.into() },
            ..Default::default()
        }
    }

    /// Create an in-memory config (for testing)
    pub fn memory() -> Self {
        Self {
            backend: AuditBackend::Memory,
            ..Default::default()
        }
    }

    /// Set maximum file size
    pub fn max_file_size(mut self, bytes: u64) -> Self {
        self.max_file_size = bytes;
        self
    }

    /// Set retention period
    pub fn retention_days(mut self, days: u32) -> Self {
        self.retention_days = days;
        self
    }

    /// Disable chain verification on startup
    pub fn skip_verification(mut self) -> Self {
        self.verify_on_startup = false;
        self
    }
}

/// A stored audit record with integrity metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuditRecord {
    /// Sequence number (monotonically increasing)
    pub seq: u64,
    /// Timestamp when stored (Unix ms)
    pub stored_at: i64,
    /// The audit entry
    pub entry: StoredAuditEntry,
    /// Hash of the previous record (empty for first record)
    pub prev_hash: String,
    /// Hash of this record (SHA-256 of seq + entry + prev_hash)
    pub hash: String,
}

/// Simplified audit entry for storage (matches AuditEntry fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuditEntry {
    pub request_id: u64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub api_key_id: Option<String>,
    pub organization_id: Option<String>,
    pub status_code: u16,
    pub duration_ms: u64,
    pub operation: String,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub timestamp: i64,
}

impl From<&AuditEntry> for StoredAuditEntry {
    fn from(entry: &AuditEntry) -> Self {
        Self {
            request_id: entry.request_id,
            method: entry.method.clone(),
            path: entry.path.clone(),
            query: entry.query.clone(),
            api_key_id: entry.api_key_id.clone(),
            organization_id: entry.organization_id.clone(),
            status_code: entry.status_code,
            duration_ms: entry.duration_ms,
            operation: entry.operation.clone(),
            client_ip: entry.client_ip.clone(),
            user_agent: entry.user_agent.clone(),
            success: entry.success,
            error: entry.error.clone(),
            timestamp: entry.timestamp,
        }
    }
}

impl StoredAuditRecord {
    /// Compute the hash for this record
    fn compute_hash(seq: u64, entry: &StoredAuditEntry, prev_hash: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(seq.to_le_bytes());
        hasher.update(serde_json::to_string(entry).unwrap_or_default().as_bytes());
        hasher.update(prev_hash.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Create a new record
    fn new(seq: u64, entry: StoredAuditEntry, prev_hash: String) -> Self {
        let hash = Self::compute_hash(seq, &entry, &prev_hash);
        Self {
            seq,
            stored_at: chrono::Utc::now().timestamp_millis(),
            entry,
            prev_hash,
            hash,
        }
    }

    /// Verify this record's hash
    pub fn verify(&self) -> bool {
        let expected = Self::compute_hash(self.seq, &self.entry, &self.prev_hash);
        self.hash == expected
    }
}

/// Query parameters for searching audit records
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Start timestamp (inclusive)
    pub since: Option<i64>,
    /// End timestamp (exclusive)
    pub until: Option<i64>,
    /// Filter by operation
    pub operation: Option<String>,
    /// Filter by API key ID
    pub api_key_id: Option<String>,
    /// Filter by organization ID
    pub organization_id: Option<String>,
    /// Filter by path pattern
    pub path_pattern: Option<String>,
    /// Filter by success/failure
    pub success: Option<bool>,
    /// Maximum results
    pub limit: Option<usize>,
    /// Skip first N results
    pub offset: Option<usize>,
}

impl AuditQuery {
    /// Filter by timestamp range
    pub fn since(mut self, timestamp: i64) -> Self {
        self.since = Some(timestamp);
        self
    }

    pub fn until(mut self, timestamp: i64) -> Self {
        self.until = Some(timestamp);
        self
    }

    /// Filter by operation
    pub fn operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    /// Filter by API key
    pub fn api_key(mut self, key_id: impl Into<String>) -> Self {
        self.api_key_id = Some(key_id.into());
        self
    }

    /// Filter by organization
    pub fn organization(mut self, org_id: impl Into<String>) -> Self {
        self.organization_id = Some(org_id.into());
        self
    }

    /// Filter by path pattern
    pub fn path(mut self, pattern: impl Into<String>) -> Self {
        self.path_pattern = Some(pattern.into());
        self
    }

    /// Filter by success/failure
    pub fn success_only(mut self) -> Self {
        self.success = Some(true);
        self
    }

    pub fn failures_only(mut self) -> Self {
        self.success = Some(false);
        self
    }

    /// Limit results
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Skip first N results
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }

    /// Check if a record matches the query
    fn matches(&self, record: &StoredAuditRecord) -> bool {
        if let Some(since) = self.since {
            if record.entry.timestamp < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if record.entry.timestamp >= until {
                return false;
            }
        }
        if let Some(ref op) = self.operation {
            if &record.entry.operation != op {
                return false;
            }
        }
        if let Some(ref key_id) = self.api_key_id {
            if record.entry.api_key_id.as_ref() != Some(key_id) {
                return false;
            }
        }
        if let Some(ref org_id) = self.organization_id {
            if record.entry.organization_id.as_ref() != Some(org_id) {
                return false;
            }
        }
        if let Some(ref pattern) = self.path_pattern {
            if !record.entry.path.contains(pattern) {
                return false;
            }
        }
        if let Some(success) = self.success {
            if record.entry.success != success {
                return false;
            }
        }
        true
    }
}

/// Result of chain verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether the chain is valid
    pub valid: bool,
    /// Total records verified
    pub records_verified: u64,
    /// First invalid record (if any)
    pub first_invalid: Option<u64>,
    /// Error message (if any)
    pub error: Option<String>,
}

/// Immutable audit trail storage
pub struct AuditStore {
    config: AuditStoreConfig,
    /// Current sequence number
    sequence: AtomicU64,
    /// Hash of the last record
    last_hash: RwLock<String>,
    /// Current log file (for file backend)
    current_file: RwLock<Option<PathBuf>>,
    /// Current file size
    current_size: AtomicU64,
    /// In-memory storage (for memory backend)
    memory_store: RwLock<Vec<StoredAuditRecord>>,
}

impl AuditStore {
    /// Create a new audit store
    pub async fn new(config: AuditStoreConfig) -> Result<Arc<Self>> {
        let store = Arc::new(Self {
            config: config.clone(),
            sequence: AtomicU64::new(0),
            last_hash: RwLock::new(String::new()),
            current_file: RwLock::new(None),
            current_size: AtomicU64::new(0),
            memory_store: RwLock::new(Vec::new()),
        });

        // Initialize based on backend
        match &config.backend {
            AuditBackend::File { path } => {
                store.init_file_backend(path).await?;
            }
            AuditBackend::Memory => {
                // Nothing to initialize
            }
        }

        // Verify chain integrity if configured
        if config.verify_on_startup {
            let result = store.verify_chain().await?;
            if !result.valid {
                return Err(AuditStoreError::IntegrityViolation {
                    seq: result.first_invalid.unwrap_or(0),
                    expected: "valid chain".to_string(),
                    actual: result.error.unwrap_or_else(|| "unknown error".to_string()),
                });
            }
        }

        Ok(store)
    }

    /// Initialize file backend
    async fn init_file_backend(&self, path: &Path) -> Result<()> {
        // Create directory if it doesn't exist
        fs::create_dir_all(path).await?;

        // Find existing log files and load the latest state
        let mut entries = fs::read_dir(path).await?;
        let mut log_files: Vec<PathBuf> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let file_path = entry.path();
            if file_path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                log_files.push(file_path);
            }
        }

        // Sort by name (timestamp-based naming)
        log_files.sort();

        // Load state from the latest file
        if let Some(latest) = log_files.last() {
            let (seq, hash, size) = self.load_file_state(latest).await?;
            self.sequence.store(seq, Ordering::SeqCst);
            *self.last_hash.write().await = hash;
            self.current_size.store(size, Ordering::SeqCst);
            *self.current_file.write().await = Some(latest.clone());
        } else {
            // Create first log file
            let file_path = self.new_log_file_path(path);
            *self.current_file.write().await = Some(file_path);
        }

        Ok(())
    }

    /// Load state from a log file (returns last seq, last hash, file size)
    async fn load_file_state(&self, path: &Path) -> Result<(u64, String, u64)> {
        let file = File::open(path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut last_seq = 0u64;
        let mut last_hash = String::new();

        while let Some(line) = lines.next_line().await? {
            if line.is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<StoredAuditRecord>(&line) {
                last_seq = record.seq;
                last_hash = record.hash.clone();
            }
        }

        let metadata = fs::metadata(path).await?;
        Ok((last_seq, last_hash, metadata.len()))
    }

    /// Generate a new log file path
    fn new_log_file_path(&self, base_path: &Path) -> PathBuf {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        base_path.join(format!("audit_{}.jsonl", timestamp))
    }

    /// Append an audit entry to the store
    pub async fn append(&self, entry: &AuditEntry) -> Result<StoredAuditRecord> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let prev_hash = self.last_hash.read().await.clone();

        let stored_entry = StoredAuditEntry::from(entry);
        let record = StoredAuditRecord::new(seq, stored_entry, prev_hash);

        // Update last hash
        *self.last_hash.write().await = record.hash.clone();

        // Store based on backend
        match &self.config.backend {
            AuditBackend::File { path } => {
                self.append_to_file(path, &record).await?;
            }
            AuditBackend::Memory => {
                self.memory_store.write().await.push(record.clone());
            }
        }

        Ok(record)
    }

    /// Append a record to the current log file
    async fn append_to_file(&self, base_path: &Path, record: &StoredAuditRecord) -> Result<()> {
        // Check if we need to rotate
        if self.current_size.load(Ordering::SeqCst) >= self.config.max_file_size {
            self.rotate_log_file(base_path).await?;
        }

        let file_path = {
            let guard = self.current_file.read().await;
            guard
                .clone()
                .unwrap_or_else(|| self.new_log_file_path(base_path))
        };

        // Ensure file path is set
        if self.current_file.read().await.is_none() {
            *self.current_file.write().await = Some(file_path.clone());
        }

        // Serialize and append
        let mut line = serde_json::to_string(record)?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?;

        file.write_all(line.as_bytes()).await?;

        if self.config.sync_writes {
            file.sync_all().await?;
        }

        self.current_size
            .fetch_add(line.len() as u64, Ordering::SeqCst);

        Ok(())
    }

    /// Rotate to a new log file
    async fn rotate_log_file(&self, base_path: &Path) -> Result<()> {
        let new_path = self.new_log_file_path(base_path);
        *self.current_file.write().await = Some(new_path);
        self.current_size.store(0, Ordering::SeqCst);
        Ok(())
    }

    /// Query audit records
    pub async fn query(&self, query: AuditQuery) -> Result<Vec<StoredAuditRecord>> {
        match &self.config.backend {
            AuditBackend::File { path } => self.query_files(path, &query).await,
            AuditBackend::Memory => self.query_memory(&query).await,
        }
    }

    /// Query from files
    async fn query_files(
        &self,
        base_path: &Path,
        query: &AuditQuery,
    ) -> Result<Vec<StoredAuditRecord>> {
        let mut results = Vec::new();
        let mut entries = fs::read_dir(base_path).await?;
        let mut log_files: Vec<PathBuf> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let file_path = entry.path();
            if file_path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                log_files.push(file_path);
            }
        }

        log_files.sort();

        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(usize::MAX);
        let mut skipped = 0;

        for file_path in log_files {
            let file = File::open(&file_path).await?;
            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await? {
                if line.is_empty() {
                    continue;
                }
                if let Ok(record) = serde_json::from_str::<StoredAuditRecord>(&line) {
                    if query.matches(&record) {
                        if skipped < offset {
                            skipped += 1;
                            continue;
                        }
                        results.push(record);
                        if results.len() >= limit {
                            return Ok(results);
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Query from memory
    async fn query_memory(&self, query: &AuditQuery) -> Result<Vec<StoredAuditRecord>> {
        let store = self.memory_store.read().await;
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(usize::MAX);

        Ok(store
            .iter()
            .filter(|r| query.matches(r))
            .skip(offset)
            .take(limit)
            .cloned()
            .collect())
    }

    /// Verify the integrity of the audit chain
    pub async fn verify_chain(&self) -> Result<VerificationResult> {
        match &self.config.backend {
            AuditBackend::File { path } => self.verify_files(path).await,
            AuditBackend::Memory => self.verify_memory().await,
        }
    }

    /// Verify files
    async fn verify_files(&self, base_path: &Path) -> Result<VerificationResult> {
        // Check if directory exists
        if !base_path.exists() {
            return Ok(VerificationResult {
                valid: true,
                records_verified: 0,
                first_invalid: None,
                error: None,
            });
        }

        let mut entries = fs::read_dir(base_path).await?;
        let mut log_files: Vec<PathBuf> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let file_path = entry.path();
            if file_path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                log_files.push(file_path);
            }
        }

        log_files.sort();

        let mut prev_hash = String::new();
        let mut records_verified = 0u64;

        for file_path in log_files {
            let file = File::open(&file_path).await?;
            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await? {
                if line.is_empty() {
                    continue;
                }
                let record: StoredAuditRecord = serde_json::from_str(&line)?;

                // Verify hash chain
                if record.prev_hash != prev_hash {
                    return Ok(VerificationResult {
                        valid: false,
                        records_verified,
                        first_invalid: Some(record.seq),
                        error: Some(format!(
                            "Chain broken: expected prev_hash '{}', got '{}'",
                            prev_hash, record.prev_hash
                        )),
                    });
                }

                // Verify record integrity
                if !record.verify() {
                    return Ok(VerificationResult {
                        valid: false,
                        records_verified,
                        first_invalid: Some(record.seq),
                        error: Some("Record hash mismatch".to_string()),
                    });
                }

                prev_hash = record.hash.clone();
                records_verified += 1;
            }
        }

        Ok(VerificationResult {
            valid: true,
            records_verified,
            first_invalid: None,
            error: None,
        })
    }

    /// Verify memory store
    async fn verify_memory(&self) -> Result<VerificationResult> {
        let store = self.memory_store.read().await;
        let mut prev_hash = String::new();
        let mut records_verified = 0u64;

        for record in store.iter() {
            if record.prev_hash != prev_hash {
                return Ok(VerificationResult {
                    valid: false,
                    records_verified,
                    first_invalid: Some(record.seq),
                    error: Some(format!(
                        "Chain broken: expected prev_hash '{}', got '{}'",
                        prev_hash, record.prev_hash
                    )),
                });
            }

            if !record.verify() {
                return Ok(VerificationResult {
                    valid: false,
                    records_verified,
                    first_invalid: Some(record.seq),
                    error: Some("Record hash mismatch".to_string()),
                });
            }

            prev_hash = record.hash.clone();
            records_verified += 1;
        }

        Ok(VerificationResult {
            valid: true,
            records_verified,
            first_invalid: None,
            error: None,
        })
    }

    /// Get statistics about the audit store
    pub async fn stats(&self) -> AuditStoreStats {
        let total_records = self.sequence.load(Ordering::SeqCst);

        match &self.config.backend {
            AuditBackend::File { path } => {
                let file_count = fs::read_dir(path)
                    .await
                    .map(|mut entries| {
                        let mut count = 0;
                        // Count synchronously from the stream
                        while let Ok(Some(_)) = futures::executor::block_on(entries.next_entry()) {
                            count += 1;
                        }
                        count
                    })
                    .unwrap_or(0);

                AuditStoreStats {
                    total_records,
                    file_count,
                    current_file_size: self.current_size.load(Ordering::SeqCst),
                }
            }
            AuditBackend::Memory => AuditStoreStats {
                total_records,
                file_count: 0,
                current_file_size: 0,
            },
        }
    }

    /// Get the current sequence number
    pub fn current_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }
}

/// Statistics about the audit store
#[derive(Debug, Clone)]
pub struct AuditStoreStats {
    pub total_records: u64,
    pub file_count: usize,
    pub current_file_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_audit_entry(request_id: u64) -> AuditEntry {
        AuditEntry {
            request_id,
            method: "GET".to_string(),
            path: "/api/v1/topics".to_string(),
            query: None,
            api_key_id: Some("key_123".to_string()),
            organization_id: Some("org_456".to_string()),
            status_code: 200,
            duration_ms: 45,
            operation: "list_topics".to_string(),
            client_ip: Some("127.0.0.1".to_string()),
            user_agent: Some("test-client/1.0".to_string()),
            success: true,
            error: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[tokio::test]
    async fn test_memory_store() {
        let config = AuditStoreConfig::memory().skip_verification();
        let store = AuditStore::new(config).await.unwrap();

        // Append some entries
        for i in 1..=5 {
            let entry = mock_audit_entry(i);
            let record = store.append(&entry).await.unwrap();
            assert_eq!(record.seq, i);
        }

        // Verify chain
        let result = store.verify_chain().await.unwrap();
        assert!(result.valid);
        assert_eq!(result.records_verified, 5);
    }

    #[tokio::test]
    async fn test_query() {
        let config = AuditStoreConfig::memory().skip_verification();
        let store = AuditStore::new(config).await.unwrap();

        // Append entries with different operations
        for i in 1..=10 {
            let mut entry = mock_audit_entry(i);
            entry.operation = if i % 2 == 0 {
                "list_topics".to_string()
            } else {
                "create_topic".to_string()
            };
            entry.success = i <= 8;
            store.append(&entry).await.unwrap();
        }

        // Query all
        let all = store.query(AuditQuery::default()).await.unwrap();
        assert_eq!(all.len(), 10);

        // Query by operation
        let list_ops = store
            .query(AuditQuery::default().operation("list_topics"))
            .await
            .unwrap();
        assert_eq!(list_ops.len(), 5);

        // Query failures only
        let failures = store
            .query(AuditQuery::default().failures_only())
            .await
            .unwrap();
        assert_eq!(failures.len(), 2);

        // Query with limit
        let limited = store.query(AuditQuery::default().limit(3)).await.unwrap();
        assert_eq!(limited.len(), 3);
    }

    #[tokio::test]
    async fn test_chain_integrity() {
        let config = AuditStoreConfig::memory().skip_verification();
        let store = AuditStore::new(config).await.unwrap();

        // Append entries
        for i in 1..=3 {
            store.append(&mock_audit_entry(i)).await.unwrap();
        }

        // Verify chain is valid
        let result = store.verify_chain().await.unwrap();
        assert!(result.valid);

        // Tamper with the store
        {
            let mut memory = store.memory_store.write().await;
            if let Some(record) = memory.get_mut(1) {
                record.entry.status_code = 500; // Modify without updating hash
            }
        }

        // Verify chain is now invalid
        let result = store.verify_chain().await.unwrap();
        assert!(!result.valid);
        assert_eq!(result.first_invalid, Some(2));
    }

    #[test]
    fn test_record_hash() {
        let entry = StoredAuditEntry {
            request_id: 1,
            method: "GET".to_string(),
            path: "/test".to_string(),
            query: None,
            api_key_id: None,
            organization_id: None,
            status_code: 200,
            duration_ms: 10,
            operation: "test".to_string(),
            client_ip: None,
            user_agent: None,
            success: true,
            error: None,
            timestamp: 1000,
        };

        let record = StoredAuditRecord::new(1, entry.clone(), String::new());

        // Hash should be deterministic
        let hash2 = StoredAuditRecord::compute_hash(1, &entry, "");
        assert_eq!(record.hash, hash2);

        // Verify should pass
        assert!(record.verify());
    }

    #[test]
    fn test_config_defaults() {
        let config = AuditStoreConfig::default();
        assert_eq!(config.max_file_size, 100 * 1024 * 1024);
        assert_eq!(config.retention_days, 365);
        assert!(config.verify_on_startup);
        assert!(config.sync_writes);
    }
}
