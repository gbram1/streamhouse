//! Debezium CDC Source Connector
//!
//! Captures change data capture (CDC) events from databases and produces them
//! into StreamHouse. This implementation provides the configuration parsing,
//! validation, CDC event model, and lifecycle framework; the actual database
//! drivers can be plugged in later.
//!
//! ## Configuration
//!
//! | Key               | Description                                      | Default    |
//! |-------------------|--------------------------------------------------|------------|
//! | `database.type`   | Database type: `postgresql`, `mysql`, `mongodb`  | required   |
//! | `database.hostname` | Database server hostname                       | required   |
//! | `database.port`   | Database server port                             | required   |
//! | `database.dbname` | Database name                                    | required   |
//! | `database.user`   | Database username                                | required   |
//! | `database.password` | Database password                              | required   |
//! | `table.include.list` | Comma-separated list of tables to capture      | required   |
//! | `database.server.name` | Logical server name for event keys           | required   |
//! | `slot.name`       | PostgreSQL replication slot name                 | (none)     |
//! | `snapshot.mode`   | Snapshot mode: `initial`, `never`, `always`      | `initial`  |

use std::collections::HashMap;

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing;

use crate::config::ConnectorState;
use crate::error::{ConnectorError, Result};
use crate::traits::{SourceConnector, SourceRecord};

/// Supported database types for CDC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    MongoDB,
}

impl DatabaseType {
    /// Parse from a string (case-insensitive).
    pub fn from_str_config(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "postgresql" | "postgres" | "pg" => Ok(DatabaseType::PostgreSQL),
            "mysql" => Ok(DatabaseType::MySQL),
            "mongodb" | "mongo" => Ok(DatabaseType::MongoDB),
            other => Err(ConnectorError::ConfigError(format!(
                "unsupported database type '{}': must be 'postgresql', 'mysql', or 'mongodb'",
                other
            ))),
        }
    }
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseType::PostgreSQL => write!(f, "postgresql"),
            DatabaseType::MySQL => write!(f, "mysql"),
            DatabaseType::MongoDB => write!(f, "mongodb"),
        }
    }
}

/// Snapshot mode controlling how the connector handles initial data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotMode {
    /// Take an initial snapshot of existing data, then stream changes.
    Initial,
    /// Stream changes only, skip snapshot.
    Never,
    /// Always take a full snapshot on startup.
    Always,
}

impl SnapshotMode {
    /// Parse from a string (case-insensitive).
    pub fn from_str_config(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "initial" => Ok(SnapshotMode::Initial),
            "never" => Ok(SnapshotMode::Never),
            "always" => Ok(SnapshotMode::Always),
            other => Err(ConnectorError::ConfigError(format!(
                "invalid snapshot.mode '{}': must be 'initial', 'never', or 'always'",
                other
            ))),
        }
    }
}

impl std::fmt::Display for SnapshotMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotMode::Initial => write!(f, "initial"),
            SnapshotMode::Never => write!(f, "never"),
            SnapshotMode::Always => write!(f, "always"),
        }
    }
}

/// The type of CDC operation captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CdcOperation {
    /// A new row was inserted.
    Create,
    /// An existing row was updated.
    Update,
    /// A row was deleted.
    Delete,
    /// A row from the initial snapshot.
    Snapshot,
}

impl std::fmt::Display for CdcOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdcOperation::Create => write!(f, "c"),
            CdcOperation::Update => write!(f, "u"),
            CdcOperation::Delete => write!(f, "d"),
            CdcOperation::Snapshot => write!(f, "r"),
        }
    }
}

/// A single CDC event captured from a database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcEvent {
    /// The type of operation (create, update, delete, snapshot).
    pub operation: CdcOperation,
    /// The fully-qualified table name.
    pub table: String,
    /// The row state before the change (present for update and delete).
    pub before: Option<serde_json::Value>,
    /// The row state after the change (present for create, update, and snapshot).
    pub after: Option<serde_json::Value>,
    /// Timestamp of the event in milliseconds since epoch.
    pub timestamp: u64,
    /// Optional database transaction ID.
    pub transaction_id: Option<String>,
}

impl CdcEvent {
    /// Convert this CDC event into a `SourceRecord`.
    ///
    /// The table name is used as the record key and the full event is
    /// serialized as JSON in the record value.
    pub fn to_source_record(&self) -> Result<SourceRecord> {
        let value = serde_json::to_vec(self).map_err(|e| {
            ConnectorError::SerializationError(format!(
                "failed to serialize CdcEvent: {}",
                e
            ))
        })?;

        Ok(SourceRecord {
            key: Some(Bytes::from(self.table.clone())),
            value: Bytes::from(value),
            timestamp: Some(self.timestamp),
            partition: None,
        })
    }
}

/// Parsed configuration for the Debezium CDC source connector.
#[derive(Debug, Clone)]
pub struct DebeziumConfig {
    /// The type of database to connect to.
    pub database_type: DatabaseType,
    /// Database server hostname.
    pub hostname: String,
    /// Database server port.
    pub port: u16,
    /// Database name.
    pub database: String,
    /// Database username.
    pub username: String,
    /// Database password.
    pub password: String,
    /// Tables to capture changes from.
    pub tables: Vec<String>,
    /// Logical server name used as a prefix for event keys.
    pub server_name: String,
    /// PostgreSQL replication slot name (only for PostgreSQL).
    pub slot_name: Option<String>,
    /// Snapshot mode controlling initial data handling.
    pub snapshot_mode: SnapshotMode,
}

impl DebeziumConfig {
    /// Parse a `DebeziumConfig` from a string key-value map.
    ///
    /// Required keys: `database.type`, `database.hostname`, `database.port`,
    /// `database.dbname`, `database.user`, `database.password`,
    /// `table.include.list`, `database.server.name`.
    pub fn from_config_map(config: &HashMap<String, String>) -> Result<Self> {
        let database_type = config
            .get("database.type")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'database.type'".to_string(),
                )
            })
            .and_then(|s| DatabaseType::from_str_config(s))?;

        let hostname = config
            .get("database.hostname")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'database.hostname'".to_string(),
                )
            })?
            .clone();

        if hostname.trim().is_empty() {
            return Err(ConnectorError::ConfigError(
                "'database.hostname' must not be empty".to_string(),
            ));
        }

        let port = config
            .get("database.port")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'database.port'".to_string(),
                )
            })
            .and_then(|s| {
                s.parse::<u16>().map_err(|e| {
                    ConnectorError::ConfigError(format!("invalid database.port: {}", e))
                })
            })?;

        let database = config
            .get("database.dbname")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'database.dbname'".to_string(),
                )
            })?
            .clone();

        let username = config
            .get("database.user")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'database.user'".to_string(),
                )
            })?
            .clone();

        let password = config
            .get("database.password")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'database.password'".to_string(),
                )
            })?
            .clone();

        let tables_raw = config
            .get("table.include.list")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'table.include.list'".to_string(),
                )
            })?
            .clone();

        let tables: Vec<String> = tables_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if tables.is_empty() {
            return Err(ConnectorError::ConfigError(
                "'table.include.list' must contain at least one table".to_string(),
            ));
        }

        let server_name = config
            .get("database.server.name")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'database.server.name'".to_string(),
                )
            })?
            .clone();

        if server_name.trim().is_empty() {
            return Err(ConnectorError::ConfigError(
                "'database.server.name' must not be empty".to_string(),
            ));
        }

        let slot_name = config.get("slot.name").cloned();

        let snapshot_mode = config
            .get("snapshot.mode")
            .map(|s| SnapshotMode::from_str_config(s))
            .transpose()?
            .unwrap_or(SnapshotMode::Initial);

        Ok(DebeziumConfig {
            database_type,
            hostname,
            port,
            database,
            username,
            password,
            tables,
            server_name,
            slot_name,
            snapshot_mode,
        })
    }
}

/// Debezium CDC Source Connector implementation.
///
/// Provides the lifecycle framework for capturing change events from databases.
/// The actual database driver integration can be plugged in later; the `poll()`
/// method currently returns an empty vec.
pub struct DebeziumSourceConnector {
    name: String,
    config: DebeziumConfig,
    state: ConnectorState,
    buffer: Vec<SourceRecord>,
}

impl DebeziumSourceConnector {
    /// Create a new `DebeziumSourceConnector` with the given name and config map.
    pub fn new(name: &str, config_map: &HashMap<String, String>) -> Result<Self> {
        let config = DebeziumConfig::from_config_map(config_map)?;
        Ok(Self {
            name: name.to_string(),
            config,
            state: ConnectorState::Stopped,
            buffer: Vec::new(),
        })
    }

    /// Create with an already-parsed config.
    pub fn with_config(name: &str, config: DebeziumConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            state: ConnectorState::Stopped,
            buffer: Vec::new(),
        }
    }

    /// Return a reference to the parsed configuration.
    pub fn config(&self) -> &DebeziumConfig {
        &self.config
    }

    /// Return the current connector state.
    pub fn connector_state(&self) -> ConnectorState {
        self.state
    }

    /// Enqueue a CDC event for the next poll.
    ///
    /// This is primarily useful for testing; @Note the
    /// connector would receive events from the database replication stream.
    pub fn enqueue_event(&mut self, event: &CdcEvent) -> Result<()> {
        let record = event.to_source_record()?;
        self.buffer.push(record);
        Ok(())
    }
}

#[async_trait]
impl SourceConnector for DebeziumSourceConnector {
    async fn start(&mut self) -> Result<()> {
        if self.state == ConnectorState::Running {
            return Err(ConnectorError::SourceError(
                "connector is already running".to_string(),
            ));
        }

        // @Note this would:
        // 1. Connect to the database
        // 2. Set up a replication slot (PostgreSQL) or binlog reader (MySQL)
        // 3. Optionally take an initial snapshot
        tracing::info!(
            connector = %self.name,
            database_type = %self.config.database_type,
            hostname = %self.config.hostname,
            port = %self.config.port,
            database = %self.config.database,
            tables = ?self.config.tables,
            snapshot_mode = %self.config.snapshot_mode,
            "Debezium CDC source connector started"
        );

        self.state = ConnectorState::Running;
        Ok(())
    }

    async fn poll(&mut self) -> Result<Vec<SourceRecord>> {
        if self.state != ConnectorState::Running {
            return Err(ConnectorError::SourceError(
                "connector is not running".to_string(),
            ));
        }

        // Drain any buffered records (from enqueue_event or a real driver).
        let records: Vec<SourceRecord> = self.buffer.drain(..).collect();
        Ok(records)
    }

    async fn stop(&mut self) -> Result<()> {
        if self.state == ConnectorState::Stopped {
            return Err(ConnectorError::SourceError(
                "connector is already stopped".to_string(),
            ));
        }

        // @Note this would close the replication stream
        // and database connection.
        tracing::info!(connector = %self.name, "Debezium CDC source connector stopped");

        self.state = ConnectorState::Stopped;
        self.buffer.clear();
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a minimal valid config map for PostgreSQL.
    fn base_config_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("database.type".to_string(), "postgresql".to_string());
        m.insert("database.hostname".to_string(), "localhost".to_string());
        m.insert("database.port".to_string(), "5432".to_string());
        m.insert("database.dbname".to_string(), "mydb".to_string());
        m.insert("database.user".to_string(), "replicator".to_string());
        m.insert("database.password".to_string(), "secret".to_string());
        m.insert(
            "table.include.list".to_string(),
            "public.users,public.orders".to_string(),
        );
        m.insert(
            "database.server.name".to_string(),
            "dbserver1".to_string(),
        );
        m
    }

    // ---------------------------------------------------------------
    // Config parsing - valid configurations
    // ---------------------------------------------------------------

    #[test]
    fn test_config_parse_minimal() {
        let m = base_config_map();
        let config = DebeziumConfig::from_config_map(&m).unwrap();
        assert_eq!(config.database_type, DatabaseType::PostgreSQL);
        assert_eq!(config.hostname, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.database, "mydb");
        assert_eq!(config.username, "replicator");
        assert_eq!(config.password, "secret");
        assert_eq!(config.tables, vec!["public.users", "public.orders"]);
        assert_eq!(config.server_name, "dbserver1");
        assert!(config.slot_name.is_none());
        assert_eq!(config.snapshot_mode, SnapshotMode::Initial);
    }

    #[test]
    fn test_config_parse_all_options() {
        let mut m = base_config_map();
        m.insert("slot.name".to_string(), "debezium_slot".to_string());
        m.insert("snapshot.mode".to_string(), "never".to_string());

        let config = DebeziumConfig::from_config_map(&m).unwrap();
        assert_eq!(config.slot_name, Some("debezium_slot".to_string()));
        assert_eq!(config.snapshot_mode, SnapshotMode::Never);
    }

    #[test]
    fn test_config_mysql() {
        let mut m = base_config_map();
        m.insert("database.type".to_string(), "mysql".to_string());
        m.insert("database.port".to_string(), "3306".to_string());
        let config = DebeziumConfig::from_config_map(&m).unwrap();
        assert_eq!(config.database_type, DatabaseType::MySQL);
        assert_eq!(config.port, 3306);
    }

    #[test]
    fn test_config_mongodb() {
        let mut m = base_config_map();
        m.insert("database.type".to_string(), "mongodb".to_string());
        m.insert("database.port".to_string(), "27017".to_string());
        let config = DebeziumConfig::from_config_map(&m).unwrap();
        assert_eq!(config.database_type, DatabaseType::MongoDB);
    }

    // ---------------------------------------------------------------
    // Config parsing - validation errors
    // ---------------------------------------------------------------

    #[test]
    fn test_config_missing_database_type() {
        let mut m = base_config_map();
        m.remove("database.type");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("database.type"));
    }

    #[test]
    fn test_config_invalid_database_type() {
        let mut m = base_config_map();
        m.insert("database.type".to_string(), "oracle".to_string());
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_hostname() {
        let mut m = base_config_map();
        m.remove("database.hostname");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_empty_hostname() {
        let mut m = base_config_map();
        m.insert("database.hostname".to_string(), "  ".to_string());
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_port() {
        let mut m = base_config_map();
        m.remove("database.port");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_port() {
        let mut m = base_config_map();
        m.insert("database.port".to_string(), "not_a_port".to_string());
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_dbname() {
        let mut m = base_config_map();
        m.remove("database.dbname");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_user() {
        let mut m = base_config_map();
        m.remove("database.user");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_password() {
        let mut m = base_config_map();
        m.remove("database.password");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_tables() {
        let mut m = base_config_map();
        m.remove("table.include.list");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_empty_tables() {
        let mut m = base_config_map();
        m.insert("table.include.list".to_string(), " , , ".to_string());
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_server_name() {
        let mut m = base_config_map();
        m.remove("database.server.name");
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_empty_server_name() {
        let mut m = base_config_map();
        m.insert("database.server.name".to_string(), "  ".to_string());
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_snapshot_mode() {
        let mut m = base_config_map();
        m.insert("snapshot.mode".to_string(), "partial".to_string());
        let result = DebeziumConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // DatabaseType
    // ---------------------------------------------------------------

    #[test]
    fn test_database_type_from_str() {
        assert_eq!(
            DatabaseType::from_str_config("postgresql").unwrap(),
            DatabaseType::PostgreSQL
        );
        assert_eq!(
            DatabaseType::from_str_config("postgres").unwrap(),
            DatabaseType::PostgreSQL
        );
        assert_eq!(
            DatabaseType::from_str_config("pg").unwrap(),
            DatabaseType::PostgreSQL
        );
        assert_eq!(
            DatabaseType::from_str_config("POSTGRESQL").unwrap(),
            DatabaseType::PostgreSQL
        );
        assert_eq!(
            DatabaseType::from_str_config("mysql").unwrap(),
            DatabaseType::MySQL
        );
        assert_eq!(
            DatabaseType::from_str_config("MongoDB").unwrap(),
            DatabaseType::MongoDB
        );
        assert_eq!(
            DatabaseType::from_str_config("mongo").unwrap(),
            DatabaseType::MongoDB
        );
    }

    #[test]
    fn test_database_type_display() {
        assert_eq!(format!("{}", DatabaseType::PostgreSQL), "postgresql");
        assert_eq!(format!("{}", DatabaseType::MySQL), "mysql");
        assert_eq!(format!("{}", DatabaseType::MongoDB), "mongodb");
    }

    // ---------------------------------------------------------------
    // SnapshotMode
    // ---------------------------------------------------------------

    #[test]
    fn test_snapshot_mode_from_str() {
        assert_eq!(
            SnapshotMode::from_str_config("initial").unwrap(),
            SnapshotMode::Initial
        );
        assert_eq!(
            SnapshotMode::from_str_config("NEVER").unwrap(),
            SnapshotMode::Never
        );
        assert_eq!(
            SnapshotMode::from_str_config("Always").unwrap(),
            SnapshotMode::Always
        );
    }

    #[test]
    fn test_snapshot_mode_display() {
        assert_eq!(format!("{}", SnapshotMode::Initial), "initial");
        assert_eq!(format!("{}", SnapshotMode::Never), "never");
        assert_eq!(format!("{}", SnapshotMode::Always), "always");
    }

    // ---------------------------------------------------------------
    // CdcOperation
    // ---------------------------------------------------------------

    #[test]
    fn test_cdc_operation_display() {
        assert_eq!(format!("{}", CdcOperation::Create), "c");
        assert_eq!(format!("{}", CdcOperation::Update), "u");
        assert_eq!(format!("{}", CdcOperation::Delete), "d");
        assert_eq!(format!("{}", CdcOperation::Snapshot), "r");
    }

    // ---------------------------------------------------------------
    // CdcEvent
    // ---------------------------------------------------------------

    #[test]
    fn test_cdc_event_to_source_record_create() {
        let event = CdcEvent {
            operation: CdcOperation::Create,
            table: "public.users".to_string(),
            before: None,
            after: Some(serde_json::json!({"id": 1, "name": "Alice"})),
            timestamp: 1_700_000_000_000,
            transaction_id: Some("tx-123".to_string()),
        };

        let record = event.to_source_record().unwrap();
        assert_eq!(record.key, Some(Bytes::from("public.users")));
        assert_eq!(record.timestamp, Some(1_700_000_000_000));
        assert!(record.partition.is_none());

        // The value should be valid JSON containing the event fields
        let parsed: serde_json::Value =
            serde_json::from_slice(&record.value).unwrap();
        assert_eq!(parsed["operation"], "Create");
        assert_eq!(parsed["table"], "public.users");
        assert!(parsed["before"].is_null());
        assert_eq!(parsed["after"]["name"], "Alice");
        assert_eq!(parsed["transaction_id"], "tx-123");
    }

    #[test]
    fn test_cdc_event_to_source_record_update() {
        let event = CdcEvent {
            operation: CdcOperation::Update,
            table: "public.users".to_string(),
            before: Some(serde_json::json!({"id": 1, "name": "Alice"})),
            after: Some(serde_json::json!({"id": 1, "name": "Bob"})),
            timestamp: 1_700_000_001_000,
            transaction_id: None,
        };

        let record = event.to_source_record().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_slice(&record.value).unwrap();
        assert_eq!(parsed["operation"], "Update");
        assert_eq!(parsed["before"]["name"], "Alice");
        assert_eq!(parsed["after"]["name"], "Bob");
        assert!(parsed["transaction_id"].is_null());
    }

    #[test]
    fn test_cdc_event_to_source_record_delete() {
        let event = CdcEvent {
            operation: CdcOperation::Delete,
            table: "public.orders".to_string(),
            before: Some(serde_json::json!({"id": 42, "status": "pending"})),
            after: None,
            timestamp: 1_700_000_002_000,
            transaction_id: Some("tx-456".to_string()),
        };

        let record = event.to_source_record().unwrap();
        assert_eq!(record.key, Some(Bytes::from("public.orders")));

        let parsed: serde_json::Value =
            serde_json::from_slice(&record.value).unwrap();
        assert_eq!(parsed["operation"], "Delete");
        assert_eq!(parsed["before"]["id"], 42);
        assert!(parsed["after"].is_null());
    }

    #[test]
    fn test_cdc_event_to_source_record_snapshot() {
        let event = CdcEvent {
            operation: CdcOperation::Snapshot,
            table: "public.products".to_string(),
            before: None,
            after: Some(serde_json::json!({"id": 10, "name": "Widget"})),
            timestamp: 1_700_000_003_000,
            transaction_id: None,
        };

        let record = event.to_source_record().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_slice(&record.value).unwrap();
        assert_eq!(parsed["operation"], "Snapshot");
        assert_eq!(parsed["after"]["name"], "Widget");
    }

    #[test]
    fn test_cdc_event_serialization_roundtrip() {
        let event = CdcEvent {
            operation: CdcOperation::Create,
            table: "t".to_string(),
            before: None,
            after: Some(serde_json::json!({"x": 1})),
            timestamp: 100,
            transaction_id: Some("tx".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CdcEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.operation, CdcOperation::Create);
        assert_eq!(deserialized.table, "t");
        assert_eq!(deserialized.timestamp, 100);
    }

    // ---------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------

    #[test]
    fn test_new_connector() {
        let m = base_config_map();
        let connector = DebeziumSourceConnector::new("cdc-src", &m).unwrap();
        assert_eq!(connector.name(), "cdc-src");
        assert_eq!(connector.connector_state(), ConnectorState::Stopped);
    }

    #[test]
    fn test_new_connector_missing_config() {
        let m = HashMap::new();
        let result = DebeziumSourceConnector::new("bad", &m);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_config() {
        let config = DebeziumConfig {
            database_type: DatabaseType::MySQL,
            hostname: "db.example.com".to_string(),
            port: 3306,
            database: "shop".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            tables: vec!["orders".to_string()],
            server_name: "shop-db".to_string(),
            slot_name: None,
            snapshot_mode: SnapshotMode::Never,
        };
        let connector = DebeziumSourceConnector::with_config("mysql-cdc", config);
        assert_eq!(connector.name(), "mysql-cdc");
        assert_eq!(connector.config().database_type, DatabaseType::MySQL);
        assert_eq!(connector.config().snapshot_mode, SnapshotMode::Never);
    }

    // ---------------------------------------------------------------
    // Lifecycle - start / poll / stop
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_start_transitions_to_running() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Stopped);

        connector.start().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Running);
    }

    #[tokio::test]
    async fn test_start_when_already_running_fails() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        let result = connector.start().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_poll_returns_empty_when_no_events() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        let records = connector.poll().await.unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_poll_when_not_running_fails() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();

        let result = connector.poll().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_poll_returns_enqueued_events() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        let event = CdcEvent {
            operation: CdcOperation::Create,
            table: "public.users".to_string(),
            before: None,
            after: Some(serde_json::json!({"id": 1})),
            timestamp: 1_700_000_000_000,
            transaction_id: None,
        };
        connector.enqueue_event(&event).unwrap();

        let records = connector.poll().await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key, Some(Bytes::from("public.users")));

        // Buffer should be drained
        let records = connector.poll().await.unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_stop_transitions_to_stopped() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        connector.stop().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Stopped);
    }

    #[tokio::test]
    async fn test_stop_when_already_stopped_fails() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();

        let result = connector.stop().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stop_clears_buffer() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        let event = CdcEvent {
            operation: CdcOperation::Create,
            table: "t".to_string(),
            before: None,
            after: Some(serde_json::json!({})),
            timestamp: 0,
            transaction_id: None,
        };
        connector.enqueue_event(&event).unwrap();

        connector.stop().await.unwrap();
        assert!(connector.buffer.is_empty());
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let m = base_config_map();
        let mut connector = DebeziumSourceConnector::new("lifecycle", &m).unwrap();

        // Start
        connector.start().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Running);

        // Enqueue and poll
        let event = CdcEvent {
            operation: CdcOperation::Update,
            table: "public.users".to_string(),
            before: Some(serde_json::json!({"name": "old"})),
            after: Some(serde_json::json!({"name": "new"})),
            timestamp: 1_700_000_000_000,
            transaction_id: Some("tx-1".to_string()),
        };
        connector.enqueue_event(&event).unwrap();
        let records = connector.poll().await.unwrap();
        assert_eq!(records.len(), 1);

        // Stop
        connector.stop().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Stopped);

        // Restart
        connector.start().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Running);

        connector.stop().await.unwrap();
    }

    // ---------------------------------------------------------------
    // Config clone and debug
    // ---------------------------------------------------------------

    #[test]
    fn test_config_clone() {
        let m = base_config_map();
        let config = DebeziumConfig::from_config_map(&m).unwrap();
        let cloned = config.clone();
        assert_eq!(cloned.hostname, config.hostname);
        assert_eq!(cloned.port, config.port);
        assert_eq!(cloned.database_type, config.database_type);
        assert_eq!(cloned.tables, config.tables);
    }

    #[test]
    fn test_config_debug() {
        let m = base_config_map();
        let config = DebeziumConfig::from_config_map(&m).unwrap();
        let debug = format!("{:?}", config);
        assert!(debug.contains("localhost"));
        assert!(debug.contains("5432"));
        assert!(debug.contains("PostgreSQL"));
    }
}
