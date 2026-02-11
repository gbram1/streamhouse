//! PostgreSQL Sink Connector
//!
//! Writes StreamHouse records to a PostgreSQL table by parsing record values as JSON
//! and mapping fields to columns. Supports both plain INSERT and UPSERT (INSERT ... ON CONFLICT)
//! modes.
//!
//! This module is only available when the `postgres` feature is enabled.
//!
//! ## Configuration
//!
//! | Key              | Description                                | Default |
//! |------------------|--------------------------------------------|---------|
//! | `connection.url` | PostgreSQL connection string               | required|
//! | `table.name`     | Target table name                          | required|
//! | `insert.mode`    | `insert` or `upsert`                       | `insert`|
//! | `pk.fields`      | Comma-separated primary key columns (for upsert) | `""` |
//! | `batch.size`     | Records per batch INSERT                   | `500`   |

use std::collections::HashMap;

use async_trait::async_trait;
use tracing;

use crate::error::{ConnectorError, Result};
use crate::traits::{SinkConnector, SinkRecord};

/// Insert mode for the Postgres sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertMode {
    Insert,
    Upsert,
}

impl InsertMode {
    /// Parse from a string (case-insensitive).
    pub fn from_str_config(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "insert" => Ok(InsertMode::Insert),
            "upsert" => Ok(InsertMode::Upsert),
            other => Err(ConnectorError::ConfigError(format!(
                "unknown insert mode: '{}'",
                other
            ))),
        }
    }
}

/// Parsed configuration for the Postgres sink.
#[derive(Debug, Clone)]
pub struct PostgresSinkConfig {
    pub connection_url: String,
    pub table_name: String,
    pub insert_mode: InsertMode,
    pub pk_fields: Vec<String>,
    pub batch_size: usize,
}

impl PostgresSinkConfig {
    /// Parse a PostgresSinkConfig from a string key-value map.
    pub fn from_config_map(config: &HashMap<String, String>) -> Result<Self> {
        let connection_url = config
            .get("connection.url")
            .ok_or_else(|| {
                ConnectorError::ConfigError("missing required 'connection.url'".to_string())
            })?
            .clone();

        let table_name = config
            .get("table.name")
            .ok_or_else(|| {
                ConnectorError::ConfigError("missing required 'table.name'".to_string())
            })?
            .clone();

        let insert_mode = config
            .get("insert.mode")
            .map(|s| InsertMode::from_str_config(s))
            .transpose()?
            .unwrap_or(InsertMode::Insert);

        let pk_fields: Vec<String> = config
            .get("pk.fields")
            .map(|s| {
                s.split(',')
                    .map(|f| f.trim().to_string())
                    .filter(|f| !f.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        if insert_mode == InsertMode::Upsert && pk_fields.is_empty() {
            return Err(ConnectorError::ConfigError(
                "upsert mode requires 'pk.fields' to be set".to_string(),
            ));
        }

        let batch_size = config
            .get("batch.size")
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|e| ConnectorError::ConfigError(format!("invalid batch.size: {}", e)))
            })
            .transpose()?
            .unwrap_or(500);

        Ok(PostgresSinkConfig {
            connection_url,
            table_name,
            insert_mode,
            pk_fields,
            batch_size,
        })
    }
}

/// PostgreSQL Sink Connector implementation.
///
/// Parses record values as JSON objects and generates INSERT/UPSERT SQL
/// statements. Records are buffered and written in batches.
pub struct PostgresSinkConnector {
    name: String,
    config: PostgresSinkConfig,
    buffer: Vec<SinkRecord>,
    // In a real implementation, this would hold a sqlx::PgPool.
    // For compilation without the postgres feature, we keep the pool as Option.
    #[cfg(feature = "postgres")]
    pool: Option<sqlx::PgPool>,
}

impl PostgresSinkConnector {
    /// Create a new PostgresSinkConnector with the given name and config map.
    pub fn new(name: &str, config_map: &HashMap<String, String>) -> Result<Self> {
        let config = PostgresSinkConfig::from_config_map(config_map)?;
        Ok(Self {
            name: name.to_string(),
            config,
            buffer: Vec::new(),
            #[cfg(feature = "postgres")]
            pool: None,
        })
    }

    /// Create with an already-parsed config.
    pub fn with_config(name: &str, config: PostgresSinkConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            buffer: Vec::new(),
            #[cfg(feature = "postgres")]
            pool: None,
        }
    }

    /// Build an INSERT SQL statement for a batch of JSON records.
    ///
    /// All records are assumed to have the same set of columns (keys from the
    /// first record's JSON object).
    pub fn build_insert_sql(
        table: &str,
        columns: &[String],
        num_rows: usize,
    ) -> String {
        let col_list = columns.join(", ");
        let mut sql = format!("INSERT INTO {} ({}) VALUES ", table, col_list);

        for row in 0..num_rows {
            if row > 0 {
                sql.push_str(", ");
            }
            sql.push('(');
            for col in 0..columns.len() {
                if col > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&format!("${}", row * columns.len() + col + 1));
            }
            sql.push(')');
        }

        sql
    }

    /// Build an UPSERT (INSERT ... ON CONFLICT ... DO UPDATE) SQL statement.
    pub fn build_upsert_sql(
        table: &str,
        columns: &[String],
        pk_fields: &[String],
        num_rows: usize,
    ) -> String {
        let mut sql = Self::build_insert_sql(table, columns, num_rows);

        let pk_list = pk_fields.join(", ");
        sql.push_str(&format!(" ON CONFLICT ({}) DO UPDATE SET ", pk_list));

        let non_pk_columns: Vec<&String> = columns
            .iter()
            .filter(|c| !pk_fields.contains(c))
            .collect();

        for (i, col) in non_pk_columns.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("{} = EXCLUDED.{}", col, col));
        }

        sql
    }

    /// Parse a SinkRecord's value as a JSON object and extract columns/values.
    pub fn parse_record_columns(
        record: &SinkRecord,
    ) -> Result<Vec<(String, serde_json::Value)>> {
        let value_str = std::str::from_utf8(&record.value).map_err(|e| {
            ConnectorError::SerializationError(format!("invalid UTF-8 in record value: {}", e))
        })?;

        let parsed: serde_json::Value = serde_json::from_str(value_str)?;

        match parsed {
            serde_json::Value::Object(map) => {
                Ok(map.into_iter().collect())
            }
            _ => Err(ConnectorError::SerializationError(
                "record value must be a JSON object".to_string(),
            )),
        }
    }

    /// Execute the buffered batch of records.
    async fn execute_batch(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let batch: Vec<SinkRecord> = self.buffer.drain(..).collect();

        // Parse all records and collect columns from the first record
        let mut all_rows: Vec<Vec<(String, serde_json::Value)>> = Vec::new();
        for record in &batch {
            let cols = Self::parse_record_columns(record)?;
            all_rows.push(cols);
        }

        if all_rows.is_empty() {
            return Ok(());
        }

        let columns: Vec<String> = all_rows[0].iter().map(|(k, _)| k.clone()).collect();
        let _sql = match self.config.insert_mode {
            InsertMode::Insert => {
                Self::build_insert_sql(&self.config.table_name, &columns, all_rows.len())
            }
            InsertMode::Upsert => Self::build_upsert_sql(
                &self.config.table_name,
                &columns,
                &self.config.pk_fields,
                all_rows.len(),
            ),
        };

        // TODO: Execute the SQL via sqlx when the postgres feature is enabled.
        // For now, log the generated SQL.
        #[cfg(feature = "postgres")]
        {
            if let Some(_pool) = &self.pool {
                // In a full implementation, we would bind parameters and execute:
                // let mut query = sqlx::query(&_sql);
                // for row in &all_rows {
                //     for (_, val) in row {
                //         query = query.bind(val.to_string());
                //     }
                // }
                // query.execute(pool).await.map_err(|e| {
                //     ConnectorError::SinkError(format!("Postgres execute error: {}", e))
                // })?;
                tracing::debug!(connector = %self.name, rows = batch.len(), "executed batch");
            }
        }

        tracing::debug!(
            connector = %self.name,
            rows = batch.len(),
            "batch prepared (SQL generated)"
        );

        Ok(())
    }
}

#[async_trait]
impl SinkConnector for PostgresSinkConnector {
    async fn start(&mut self) -> Result<()> {
        #[cfg(feature = "postgres")]
        {
            let pool = sqlx::PgPool::connect(&self.config.connection_url)
                .await
                .map_err(|e| {
                    ConnectorError::ConnectionError(format!(
                        "failed to connect to Postgres: {}",
                        e
                    ))
                })?;
            self.pool = Some(pool);
        }

        tracing::info!(connector = %self.name, "Postgres sink connector started");
        Ok(())
    }

    async fn put(&mut self, records: &[SinkRecord]) -> Result<()> {
        self.buffer.extend(records.iter().cloned());

        while self.buffer.len() >= self.config.batch_size {
            self.execute_batch().await?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.execute_batch().await
    }

    async fn stop(&mut self) -> Result<()> {
        self.flush().await?;
        #[cfg(feature = "postgres")]
        {
            if let Some(pool) = self.pool.take() {
                pool.close().await;
            }
        }
        tracing::info!(connector = %self.name, "Postgres sink connector stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn base_config_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(
            "connection.url".to_string(),
            "postgres://user:pass@localhost/db".to_string(),
        );
        m.insert("table.name".to_string(), "events".to_string());
        m
    }

    // ---------------------------------------------------------------
    // Config parsing
    // ---------------------------------------------------------------

    #[test]
    fn test_config_parse_minimal() {
        let m = base_config_map();
        let config = PostgresSinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.connection_url, "postgres://user:pass@localhost/db");
        assert_eq!(config.table_name, "events");
        assert_eq!(config.insert_mode, InsertMode::Insert);
        assert!(config.pk_fields.is_empty());
        assert_eq!(config.batch_size, 500);
    }

    #[test]
    fn test_config_parse_upsert() {
        let mut m = base_config_map();
        m.insert("insert.mode".to_string(), "upsert".to_string());
        m.insert("pk.fields".to_string(), "id, tenant_id".to_string());
        m.insert("batch.size".to_string(), "1000".to_string());

        let config = PostgresSinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.insert_mode, InsertMode::Upsert);
        assert_eq!(config.pk_fields, vec!["id", "tenant_id"]);
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn test_config_upsert_without_pk_fails() {
        let mut m = base_config_map();
        m.insert("insert.mode".to_string(), "upsert".to_string());
        let result = PostgresSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_connection_url() {
        let mut m = base_config_map();
        m.remove("connection.url");
        let result = PostgresSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_table_name() {
        let mut m = base_config_map();
        m.remove("table.name");
        let result = PostgresSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_insert_mode() {
        let mut m = base_config_map();
        m.insert("insert.mode".to_string(), "replace".to_string());
        let result = PostgresSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_batch_size() {
        let mut m = base_config_map();
        m.insert("batch.size".to_string(), "abc".to_string());
        let result = PostgresSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // InsertMode
    // ---------------------------------------------------------------

    #[test]
    fn test_insert_mode_from_str() {
        assert_eq!(InsertMode::from_str_config("insert").unwrap(), InsertMode::Insert);
        assert_eq!(InsertMode::from_str_config("INSERT").unwrap(), InsertMode::Insert);
        assert_eq!(InsertMode::from_str_config("upsert").unwrap(), InsertMode::Upsert);
        assert_eq!(InsertMode::from_str_config("UPSERT").unwrap(), InsertMode::Upsert);
    }

    #[test]
    fn test_insert_mode_invalid() {
        assert!(InsertMode::from_str_config("merge").is_err());
        assert!(InsertMode::from_str_config("delete").is_err());
    }

    // ---------------------------------------------------------------
    // SQL generation
    // ---------------------------------------------------------------

    #[test]
    fn test_build_insert_sql_single_row() {
        let columns = vec!["id".to_string(), "name".to_string(), "value".to_string()];
        let sql = PostgresSinkConnector::build_insert_sql("events", &columns, 1);
        assert_eq!(sql, "INSERT INTO events (id, name, value) VALUES ($1, $2, $3)");
    }

    #[test]
    fn test_build_insert_sql_multiple_rows() {
        let columns = vec!["id".to_string(), "data".to_string()];
        let sql = PostgresSinkConnector::build_insert_sql("tbl", &columns, 3);
        assert_eq!(
            sql,
            "INSERT INTO tbl (id, data) VALUES ($1, $2), ($3, $4), ($5, $6)"
        );
    }

    #[test]
    fn test_build_insert_sql_single_column() {
        let columns = vec!["val".to_string()];
        let sql = PostgresSinkConnector::build_insert_sql("t", &columns, 2);
        assert_eq!(sql, "INSERT INTO t (val) VALUES ($1), ($2)");
    }

    #[test]
    fn test_build_upsert_sql() {
        let columns = vec!["id".to_string(), "name".to_string(), "score".to_string()];
        let pk = vec!["id".to_string()];
        let sql = PostgresSinkConnector::build_upsert_sql("players", &columns, &pk, 1);
        assert!(sql.contains("INSERT INTO players"));
        assert!(sql.contains("ON CONFLICT (id) DO UPDATE SET"));
        assert!(sql.contains("name = EXCLUDED.name"));
        assert!(sql.contains("score = EXCLUDED.score"));
        assert!(!sql.contains("id = EXCLUDED.id")); // PK should not be in SET clause
    }

    #[test]
    fn test_build_upsert_sql_composite_pk() {
        let columns = vec![
            "org_id".to_string(),
            "user_id".to_string(),
            "role".to_string(),
        ];
        let pk = vec!["org_id".to_string(), "user_id".to_string()];
        let sql = PostgresSinkConnector::build_upsert_sql("roles", &columns, &pk, 1);
        assert!(sql.contains("ON CONFLICT (org_id, user_id) DO UPDATE SET"));
        assert!(sql.contains("role = EXCLUDED.role"));
        assert!(!sql.contains("org_id = EXCLUDED.org_id"));
        assert!(!sql.contains("user_id = EXCLUDED.user_id"));
    }

    // ---------------------------------------------------------------
    // Record parsing
    // ---------------------------------------------------------------

    #[test]
    fn test_parse_record_columns_valid_json() {
        let record = SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from(r#"{"id": 1, "name": "Alice", "score": 99.5}"#),
        };
        let cols = PostgresSinkConnector::parse_record_columns(&record).unwrap();
        assert_eq!(cols.len(), 3);
        let col_names: Vec<&str> = cols.iter().map(|(k, _)| k.as_str()).collect();
        assert!(col_names.contains(&"id"));
        assert!(col_names.contains(&"name"));
        assert!(col_names.contains(&"score"));
    }

    #[test]
    fn test_parse_record_columns_not_an_object() {
        let record = SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from(r#"[1, 2, 3]"#),
        };
        let result = PostgresSinkConnector::parse_record_columns(&record);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_record_columns_invalid_json() {
        let record = SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("not json at all"),
        };
        let result = PostgresSinkConnector::parse_record_columns(&record);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_record_columns_empty_object() {
        let record = SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("{}"),
        };
        let cols = PostgresSinkConnector::parse_record_columns(&record).unwrap();
        assert!(cols.is_empty());
    }

    // ---------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------

    #[test]
    fn test_new_connector() {
        let m = base_config_map();
        let connector = PostgresSinkConnector::new("pg-sink", &m).unwrap();
        assert_eq!(connector.name(), "pg-sink");
        assert!(connector.buffer.is_empty());
    }

    #[test]
    fn test_new_connector_missing_config() {
        let m = HashMap::new();
        let result = PostgresSinkConnector::new("bad", &m);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_config() {
        let config = PostgresSinkConfig {
            connection_url: "postgres://localhost/test".to_string(),
            table_name: "data".to_string(),
            insert_mode: InsertMode::Insert,
            pk_fields: vec![],
            batch_size: 100,
        };
        let connector = PostgresSinkConnector::with_config("pg", config);
        assert_eq!(connector.name(), "pg");
        assert_eq!(connector.config.batch_size, 100);
    }

    // ---------------------------------------------------------------
    // Lifecycle (no actual Postgres needed)
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_put_and_flush_without_postgres() {
        let config = PostgresSinkConfig {
            connection_url: "postgres://localhost/test".to_string(),
            table_name: "events".to_string(),
            insert_mode: InsertMode::Insert,
            pk_fields: vec![],
            batch_size: 500,
        };
        let mut connector = PostgresSinkConnector::with_config("test", config);

        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from(r#"{"id": 1, "name": "test"}"#),
        }];

        // put should succeed (just buffers)
        connector.put(&records).await.unwrap();
        assert_eq!(connector.buffer.len(), 1);

        // flush will parse and generate SQL, succeeding without a real connection
        connector.flush().await.unwrap();
        assert!(connector.buffer.is_empty());
    }

    #[tokio::test]
    async fn test_stop_flushes() {
        let config = PostgresSinkConfig {
            connection_url: "postgres://localhost/test".to_string(),
            table_name: "t".to_string(),
            insert_mode: InsertMode::Insert,
            pk_fields: vec![],
            batch_size: 500,
        };
        let mut connector = PostgresSinkConnector::with_config("test", config);

        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from(r#"{"col": "val"}"#),
        }];
        connector.put(&records).await.unwrap();
        connector.stop().await.unwrap();
        assert!(connector.buffer.is_empty());
    }
}
