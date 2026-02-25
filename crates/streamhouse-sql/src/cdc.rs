//! Change Data Capture (CDC) analytics
//!
//! Processes CDC events from database change streams (Debezium-compatible)
//! within the SQL engine. Supports parsing the standard Debezium JSON envelope,
//! filtering by operation type, and tracking CDC processing statistics.

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::SqlError;
use crate::Result;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for a [`CdcProcessor`].
#[derive(Debug, Clone)]
pub struct CdcConfig {
    /// Source topic containing CDC events.
    pub source_topic: String,
    /// Optional target topic for processed/transformed CDC events.
    pub target_topic: Option<String>,
    /// Whether to include the `before` image in the output.
    pub include_before: bool,
    /// Whether to include schema-change events.
    pub include_schema_changes: bool,
    /// If non-empty, only process these operation types.
    pub filter_operations: Vec<CdcOperation>,
    /// Columns that form the primary key of the source table.
    pub key_columns: Vec<String>,
}

impl Default for CdcConfig {
    fn default() -> Self {
        Self {
            source_topic: String::new(),
            target_topic: None,
            include_before: true,
            include_schema_changes: false,
            filter_operations: Vec::new(),
            key_columns: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// CDC operation types
// ---------------------------------------------------------------------------

/// The type of mutation represented by a CDC event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CdcOperation {
    Insert,
    Update,
    Delete,
    SchemaChange,
    Truncate,
    /// Debezium snapshot read (`r`).
    Read,
}

impl CdcOperation {
    /// Convert from the single-character Debezium `op` field.
    pub fn from_debezium_op(op: &str) -> Result<Self> {
        match op {
            "c" => Ok(Self::Insert),
            "u" => Ok(Self::Update),
            "d" => Ok(Self::Delete),
            "r" => Ok(Self::Read),
            "t" => Ok(Self::Truncate),
            other => Err(SqlError::CdcError(format!(
                "unknown Debezium operation: '{other}'"
            ))),
        }
    }

    /// Convert to the single-character Debezium `op` field.
    pub fn to_debezium_op(&self) -> &'static str {
        match self {
            Self::Insert => "c",
            Self::Update => "u",
            Self::Delete => "d",
            Self::Read => "r",
            Self::Truncate => "t",
            Self::SchemaChange => "s",
        }
    }
}

// ---------------------------------------------------------------------------
// CDC record
// ---------------------------------------------------------------------------

/// A single CDC event representing a row-level change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcRecord {
    /// Type of change.
    pub operation: CdcOperation,
    /// Name of the affected table.
    pub table: String,
    /// Optional schema/namespace.
    pub schema: Option<String>,
    /// Row image before the change (for updates/deletes).
    pub before: Option<serde_json::Value>,
    /// Row image after the change (for inserts/updates).
    pub after: Option<serde_json::Value>,
    /// Event timestamp (ms since epoch).
    pub timestamp: i64,
    /// Metadata about the source database.
    pub source: CdcSource,
}

/// Metadata identifying where a CDC event originated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcSource {
    /// Connector type (e.g. "postgresql", "mysql", "mongodb").
    pub connector: String,
    /// Database name.
    pub database: String,
    /// Table name.
    pub table: String,
    /// Optional server identifier.
    pub server_id: Option<String>,
    /// PostgreSQL Log Sequence Number.
    pub lsn: Option<String>,
    /// MySQL binlog file.
    pub binlog_file: Option<String>,
    /// MySQL binlog position.
    pub binlog_pos: Option<u64>,
}

// ---------------------------------------------------------------------------
// CDC processing state / statistics
// ---------------------------------------------------------------------------

/// Mutable processing state maintained by [`CdcProcessor`].
#[derive(Debug, Clone, Default)]
pub struct CdcState {
    pub records_processed: u64,
    pub inserts: u64,
    pub updates: u64,
    pub deletes: u64,
    pub schema_changes: u64,
    pub truncates: u64,
    pub reads: u64,
    pub filtered_out: u64,
    pub last_lsn: Option<String>,
    pub last_processed: Option<i64>,
}

// ---------------------------------------------------------------------------
// CdcProcessor
// ---------------------------------------------------------------------------

/// Processes CDC events, parsing Debezium envelopes and maintaining statistics.
pub struct CdcProcessor {
    config: CdcConfig,
    state: RwLock<CdcState>,
}

impl CdcProcessor {
    /// Create a new CDC processor with the given configuration.
    pub fn new(config: CdcConfig) -> Self {
        Self {
            config,
            state: RwLock::new(CdcState::default()),
        }
    }

    /// Process a single [`CdcRecord`], updating internal statistics.
    ///
    /// Returns `true` if the record passed the configured filters and was
    /// processed, `false` if it was filtered out.
    pub async fn process_record(&self, record: &CdcRecord) -> Result<bool> {
        // Apply filters
        if !self.apply_filter(record) {
            let mut state = self.state.write().await;
            state.filtered_out += 1;
            return Ok(false);
        }

        let mut state = self.state.write().await;
        state.records_processed += 1;
        state.last_processed = Some(record.timestamp);

        match record.operation {
            CdcOperation::Insert => state.inserts += 1,
            CdcOperation::Update => state.updates += 1,
            CdcOperation::Delete => state.deletes += 1,
            CdcOperation::SchemaChange => state.schema_changes += 1,
            CdcOperation::Truncate => state.truncates += 1,
            CdcOperation::Read => state.reads += 1,
        }

        // Track LSN if present
        if let Some(ref lsn) = record.source.lsn {
            state.last_lsn = Some(lsn.clone());
        }

        Ok(true)
    }

    /// Parse a raw JSON payload in the standard Debezium envelope format into
    /// a [`CdcRecord`].
    ///
    /// Expected envelope shape:
    /// ```json
    /// {
    ///   "before": { ... },
    ///   "after": { ... },
    ///   "source": { "connector": "postgresql", "db": "mydb", "table": "orders", ... },
    ///   "op": "c",
    ///   "ts_ms": 1234567890
    /// }
    /// ```
    pub fn parse_debezium_envelope(&self, payload: &[u8]) -> Result<CdcRecord> {
        let envelope: serde_json::Value =
            serde_json::from_slice(payload).map_err(|e| {
                SqlError::CdcError(format!("invalid JSON in CDC envelope: {e}"))
            })?;

        let op_str = envelope
            .get("op")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SqlError::CdcError("missing 'op' field in envelope".to_string()))?;

        let operation = CdcOperation::from_debezium_op(op_str)?;

        let timestamp = envelope
            .get("ts_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let before = envelope.get("before").cloned();
        let after = envelope.get("after").cloned();

        // Parse source metadata
        let source_obj = envelope.get("source");
        let source = self.parse_source(source_obj)?;

        Ok(CdcRecord {
            operation,
            table: source.table.clone(),
            schema: source_obj
                .and_then(|s| s.get("schema"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            before,
            after,
            timestamp,
            source,
        })
    }

    /// Check whether a record passes the configured filters.
    pub fn apply_filter(&self, record: &CdcRecord) -> bool {
        // If filter_operations is non-empty, only allow listed operations
        if !self.config.filter_operations.is_empty()
            && !self.config.filter_operations.contains(&record.operation)
        {
            return false;
        }

        // Skip schema changes unless explicitly included
        if record.operation == CdcOperation::SchemaChange && !self.config.include_schema_changes {
            return false;
        }

        true
    }

    /// Return a snapshot of the current processing statistics.
    pub async fn get_stats(&self) -> CdcState {
        self.state.read().await.clone()
    }

    /// Reset all counters to zero.
    pub async fn reset_state(&self) {
        let mut state = self.state.write().await;
        *state = CdcState::default();
    }

    /// Return a reference to the processor configuration.
    pub fn config(&self) -> &CdcConfig {
        &self.config
    }

    // -- internal helpers --

    fn parse_source(&self, source_obj: Option<&serde_json::Value>) -> Result<CdcSource> {
        match source_obj {
            None => {
                // No source metadata — use defaults from config
                Ok(CdcSource {
                    connector: String::new(),
                    database: String::new(),
                    table: self.config.source_topic.clone(),
                    server_id: None,
                    lsn: None,
                    binlog_file: None,
                    binlog_pos: None,
                })
            }
            Some(src) => {
                let connector = src
                    .get("connector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let database = src
                    .get("db")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let table = src
                    .get("table")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&self.config.source_topic)
                    .to_string();

                let server_id = src
                    .get("server_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let lsn = src
                    .get("lsn")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let binlog_file = src
                    .get("file")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let binlog_pos = src.get("pos").and_then(|v| v.as_u64());

                Ok(CdcSource {
                    connector,
                    database,
                    table,
                    server_id,
                    lsn,
                    binlog_file,
                    binlog_pos,
                })
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_processor() -> CdcProcessor {
        CdcProcessor::new(CdcConfig {
            source_topic: "test-topic".to_string(),
            ..Default::default()
        })
    }

    fn make_record(op: CdcOperation) -> CdcRecord {
        CdcRecord {
            operation: op,
            table: "orders".to_string(),
            schema: Some("public".to_string()),
            before: None,
            after: Some(serde_json::json!({"id": 1, "amount": 100})),
            timestamp: 1_700_000_000_000,
            source: CdcSource {
                connector: "postgresql".to_string(),
                database: "mydb".to_string(),
                table: "orders".to_string(),
                server_id: None,
                lsn: Some("0/1234ABCD".to_string()),
                binlog_file: None,
                binlog_pos: None,
            },
        }
    }

    // -- Debezium parsing --

    #[tokio::test]
    async fn test_parse_debezium_insert() {
        let proc = default_processor();
        let payload = serde_json::json!({
            "before": null,
            "after": {"id": 1, "name": "Alice"},
            "source": {
                "connector": "postgresql",
                "db": "testdb",
                "table": "users",
                "lsn": "0/ABCDEF"
            },
            "op": "c",
            "ts_ms": 1_700_000_000_000_i64
        });

        let record = proc
            .parse_debezium_envelope(payload.to_string().as_bytes())
            .unwrap();

        assert_eq!(record.operation, CdcOperation::Insert);
        assert_eq!(record.table, "users");
        assert_eq!(record.source.connector, "postgresql");
        assert_eq!(record.source.database, "testdb");
        assert_eq!(record.source.lsn, Some("0/ABCDEF".to_string()));
        assert_eq!(record.timestamp, 1_700_000_000_000);
        assert!(record.before.is_some()); // null is still Some(Value::Null)
        assert!(record.after.is_some());
    }

    #[tokio::test]
    async fn test_parse_debezium_update() {
        let proc = default_processor();
        let payload = serde_json::json!({
            "before": {"id": 1, "name": "Alice"},
            "after": {"id": 1, "name": "Alicia"},
            "source": {
                "connector": "postgresql",
                "db": "testdb",
                "table": "users"
            },
            "op": "u",
            "ts_ms": 1_700_000_001_000_i64
        });

        let record = proc
            .parse_debezium_envelope(payload.to_string().as_bytes())
            .unwrap();

        assert_eq!(record.operation, CdcOperation::Update);
        assert!(record.before.is_some());
        assert!(record.after.is_some());
    }

    #[tokio::test]
    async fn test_parse_debezium_delete() {
        let proc = default_processor();
        let payload = serde_json::json!({
            "before": {"id": 1, "name": "Alice"},
            "after": null,
            "source": {
                "connector": "postgresql",
                "db": "testdb",
                "table": "users"
            },
            "op": "d",
            "ts_ms": 1_700_000_002_000_i64
        });

        let record = proc
            .parse_debezium_envelope(payload.to_string().as_bytes())
            .unwrap();

        assert_eq!(record.operation, CdcOperation::Delete);
    }

    #[tokio::test]
    async fn test_parse_debezium_snapshot_read() {
        let proc = default_processor();
        let payload = serde_json::json!({
            "before": null,
            "after": {"id": 1},
            "source": {
                "connector": "postgresql",
                "db": "testdb",
                "table": "users"
            },
            "op": "r",
            "ts_ms": 1_700_000_000_000_i64
        });

        let record = proc
            .parse_debezium_envelope(payload.to_string().as_bytes())
            .unwrap();
        assert_eq!(record.operation, CdcOperation::Read);
    }

    #[tokio::test]
    async fn test_parse_debezium_invalid_op() {
        let proc = default_processor();
        let payload = serde_json::json!({
            "before": null,
            "after": null,
            "source": {"connector": "pg", "db": "db", "table": "t"},
            "op": "x",
            "ts_ms": 0
        });

        let result = proc.parse_debezium_envelope(payload.to_string().as_bytes());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown Debezium operation"));
    }

    #[tokio::test]
    async fn test_parse_debezium_missing_op() {
        let proc = default_processor();
        let payload = serde_json::json!({
            "before": null,
            "after": null,
            "source": {"connector": "pg", "db": "db", "table": "t"},
            "ts_ms": 0
        });

        let result = proc.parse_debezium_envelope(payload.to_string().as_bytes());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing 'op' field"));
    }

    #[tokio::test]
    async fn test_parse_debezium_invalid_json() {
        let proc = default_processor();
        let result = proc.parse_debezium_envelope(b"not json at all");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid JSON"));
    }

    // -- process_record & stats --

    #[tokio::test]
    async fn test_process_record_insert() {
        let proc = default_processor();
        let record = make_record(CdcOperation::Insert);

        let accepted = proc.process_record(&record).await.unwrap();
        assert!(accepted);

        let stats = proc.get_stats().await;
        assert_eq!(stats.records_processed, 1);
        assert_eq!(stats.inserts, 1);
        assert_eq!(stats.last_lsn, Some("0/1234ABCD".to_string()));
    }

    #[tokio::test]
    async fn test_process_record_counts_all_ops() {
        let proc = default_processor();

        proc.process_record(&make_record(CdcOperation::Insert))
            .await
            .unwrap();
        proc.process_record(&make_record(CdcOperation::Update))
            .await
            .unwrap();
        proc.process_record(&make_record(CdcOperation::Delete))
            .await
            .unwrap();

        let stats = proc.get_stats().await;
        assert_eq!(stats.records_processed, 3);
        assert_eq!(stats.inserts, 1);
        assert_eq!(stats.updates, 1);
        assert_eq!(stats.deletes, 1);
    }

    // -- filters --

    #[tokio::test]
    async fn test_filter_by_operation() {
        let proc = CdcProcessor::new(CdcConfig {
            source_topic: "t".to_string(),
            filter_operations: vec![CdcOperation::Insert],
            ..Default::default()
        });

        // Insert should pass
        let insert = make_record(CdcOperation::Insert);
        assert!(proc.process_record(&insert).await.unwrap());

        // Delete should be filtered out
        let delete = make_record(CdcOperation::Delete);
        assert!(!proc.process_record(&delete).await.unwrap());

        let stats = proc.get_stats().await;
        assert_eq!(stats.records_processed, 1);
        assert_eq!(stats.filtered_out, 1);
    }

    #[tokio::test]
    async fn test_schema_change_filtered_by_default() {
        let proc = default_processor();

        let record = make_record(CdcOperation::SchemaChange);
        assert!(!proc.process_record(&record).await.unwrap());
    }

    #[tokio::test]
    async fn test_schema_change_included_when_configured() {
        let proc = CdcProcessor::new(CdcConfig {
            source_topic: "t".to_string(),
            include_schema_changes: true,
            ..Default::default()
        });

        let record = make_record(CdcOperation::SchemaChange);
        assert!(proc.process_record(&record).await.unwrap());

        let stats = proc.get_stats().await;
        assert_eq!(stats.schema_changes, 1);
    }

    // -- reset state --

    #[tokio::test]
    async fn test_reset_state() {
        let proc = default_processor();

        proc.process_record(&make_record(CdcOperation::Insert))
            .await
            .unwrap();
        assert_eq!(proc.get_stats().await.records_processed, 1);

        proc.reset_state().await;

        let stats = proc.get_stats().await;
        assert_eq!(stats.records_processed, 0);
        assert_eq!(stats.inserts, 0);
        assert_eq!(stats.last_lsn, None);
    }

    // -- Debezium op round-trip --

    #[test]
    fn test_cdc_operation_round_trip() {
        for (op_char, expected) in [
            ("c", CdcOperation::Insert),
            ("u", CdcOperation::Update),
            ("d", CdcOperation::Delete),
            ("r", CdcOperation::Read),
            ("t", CdcOperation::Truncate),
        ] {
            let parsed = CdcOperation::from_debezium_op(op_char).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_debezium_op(), op_char);
        }
    }

    // -- MySQL source parsing --

    #[tokio::test]
    async fn test_parse_debezium_mysql_source() {
        let proc = default_processor();
        let payload = serde_json::json!({
            "before": null,
            "after": {"id": 42},
            "source": {
                "connector": "mysql",
                "db": "shop",
                "table": "products",
                "server_id": "db-primary",
                "file": "mysql-bin.000003",
                "pos": 12345
            },
            "op": "c",
            "ts_ms": 1_700_000_000_000_i64
        });

        let record = proc
            .parse_debezium_envelope(payload.to_string().as_bytes())
            .unwrap();

        assert_eq!(record.source.connector, "mysql");
        assert_eq!(record.source.binlog_file, Some("mysql-bin.000003".to_string()));
        assert_eq!(record.source.binlog_pos, Some(12345));
        assert_eq!(record.source.server_id, Some("db-primary".to_string()));
    }
}
