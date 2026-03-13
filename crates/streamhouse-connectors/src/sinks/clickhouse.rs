//! ClickHouse Sink Connector
//!
//! Writes StreamHouse records to ClickHouse via the HTTP API. Records are
//! buffered and flushed as JSONEachRow inserts.
//!
//! ## Configuration
//!
//! | Key               | Description                        | Default                 |
//! |-------------------|------------------------------------|-------------------------|
//! | `connection.url`  | ClickHouse HTTP URL                | `http://localhost:8123` |
//! | `database`        | Target database                    | `default`               |
//! | `table`           | Target table                       | required                |
//! | `batch.size`      | Records per insert                 | `1000`                  |

use std::collections::HashMap;

use async_trait::async_trait;
use tracing;

use crate::error::{ConnectorError, Result};
use crate::traits::{SinkConnector, SinkRecord};

/// Parsed configuration for the ClickHouse sink.
#[derive(Debug, Clone)]
pub struct ClickHouseSinkConfig {
    pub connection_url: String,
    pub database: String,
    pub table: String,
    pub batch_size: usize,
}

impl ClickHouseSinkConfig {
    /// Parse a ClickHouseSinkConfig from a string key-value map.
    pub fn from_config_map(config: &HashMap<String, String>) -> Result<Self> {
        let connection_url = config
            .get("connection.url")
            .cloned()
            .unwrap_or_else(|| "http://localhost:8123".to_string());

        let database = config
            .get("database")
            .cloned()
            .unwrap_or_else(|| "default".to_string());

        let table = config
            .get("table")
            .ok_or_else(|| ConnectorError::ConfigError("missing required 'table'".to_string()))?
            .clone();

        let batch_size = config
            .get("batch.size")
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|e| ConnectorError::ConfigError(format!("invalid batch.size: {}", e)))
            })
            .transpose()?
            .unwrap_or(1000);

        Ok(ClickHouseSinkConfig {
            connection_url,
            database,
            table,
            batch_size,
        })
    }
}

/// ClickHouse Sink Connector implementation.
///
/// Accumulates records in a buffer and sends them to ClickHouse via the
/// HTTP API when the buffer reaches `batch_size` or when `flush()` is called.
pub struct ClickHouseSinkConnector {
    name: String,
    config: ClickHouseSinkConfig,
    client: Option<reqwest::Client>,
    buffer: Vec<SinkRecord>,
}

impl ClickHouseSinkConnector {
    /// Create a new ClickHouseSinkConnector with the given name and config map.
    pub fn new(name: &str, config_map: &HashMap<String, String>) -> Result<Self> {
        let config = ClickHouseSinkConfig::from_config_map(config_map)?;
        Ok(Self {
            name: name.to_string(),
            config,
            client: None,
            buffer: Vec::new(),
        })
    }

    /// Create with an already-parsed config.
    pub fn with_config(name: &str, config: ClickHouseSinkConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            client: None,
            buffer: Vec::new(),
        }
    }

    /// Build the JSONEachRow body for a ClickHouse INSERT.
    pub fn build_insert_body(records: &[SinkRecord]) -> Result<String> {
        let mut body = String::new();

        for record in records {
            let value_str = std::str::from_utf8(&record.value).map_err(|e| {
                ConnectorError::SerializationError(format!("invalid UTF-8 in record value: {}", e))
            })?;

            // Validate it's valid JSON
            let _: serde_json::Value = serde_json::from_str(value_str)?;

            body.push_str(value_str);
            body.push('\n');
        }

        Ok(body)
    }

    /// Send an INSERT request to ClickHouse.
    async fn send_insert(&self, body: &str) -> Result<()> {
        let client = self.client.as_ref().ok_or_else(|| {
            ConnectorError::ConnectionError("HTTP client not initialized".to_string())
        })?;

        let query = format!(
            "INSERT INTO {}.{} FORMAT JSONEachRow",
            self.config.database, self.config.table
        );

        let response = client
            .post(&self.config.connection_url)
            .query(&[("query", &query)])
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| {
                ConnectorError::ConnectionError(format!("ClickHouse request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(ConnectorError::SinkError(format!(
                "ClickHouse INSERT returned {}: {}",
                status, body_text
            )));
        }

        Ok(())
    }

    /// Flush the current buffer by building and sending an insert request.
    async fn flush_buffer(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let batch: Vec<SinkRecord> = self.buffer.drain(..).collect();
        let body = Self::build_insert_body(&batch)?;

        self.send_insert(&body).await?;

        tracing::debug!(
            connector = %self.name,
            records = batch.len(),
            "flushed insert to ClickHouse"
        );

        Ok(())
    }
}

#[async_trait]
impl SinkConnector for ClickHouseSinkConnector {
    async fn start(&mut self) -> Result<()> {
        if self.client.is_none() {
            self.client = Some(reqwest::Client::builder().build().map_err(|e| {
                ConnectorError::ConnectionError(format!("failed to build HTTP client: {}", e))
            })?);
        }
        tracing::info!(connector = %self.name, "ClickHouse sink connector started");
        Ok(())
    }

    async fn put(&mut self, records: &[SinkRecord]) -> Result<()> {
        self.buffer.extend(records.iter().cloned());

        while self.buffer.len() >= self.config.batch_size {
            let batch: Vec<SinkRecord> = self.buffer.drain(..self.config.batch_size).collect();
            let body = Self::build_insert_body(&batch)?;
            self.send_insert(&body).await?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.flush_buffer().await
    }

    async fn stop(&mut self) -> Result<()> {
        self.flush().await?;
        tracing::info!(connector = %self.name, "ClickHouse sink connector stopped");
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
        m.insert("table".to_string(), "events".to_string());
        m
    }

    fn sample_records(n: usize) -> Vec<SinkRecord> {
        (0..n)
            .map(|i| SinkRecord {
                topic: "events".to_string(),
                partition: 0,
                offset: i as u64,
                timestamp: 1_700_000_000_000 + (i as u64 * 1000),
                key: Some(Bytes::from(format!("key-{}", i))),
                value: Bytes::from(format!(
                    r#"{{"id":"doc-{}","name":"item-{}","count":{}}}"#,
                    i, i, i
                )),
            })
            .collect()
    }

    #[test]
    fn test_config_parse_minimal() {
        let m = base_config_map();
        let config = ClickHouseSinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.connection_url, "http://localhost:8123");
        assert_eq!(config.database, "default");
        assert_eq!(config.table, "events");
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn test_config_parse_all_options() {
        let mut m = base_config_map();
        m.insert(
            "connection.url".to_string(),
            "http://clickhouse:8123".to_string(),
        );
        m.insert("database".to_string(), "analytics".to_string());
        m.insert("batch.size".to_string(), "5000".to_string());

        let config = ClickHouseSinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.connection_url, "http://clickhouse:8123");
        assert_eq!(config.database, "analytics");
        assert_eq!(config.batch_size, 5000);
    }

    #[test]
    fn test_config_missing_table() {
        let m = HashMap::new();
        let result = ClickHouseSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_batch_size() {
        let mut m = base_config_map();
        m.insert("batch.size".to_string(), "not_a_number".to_string());
        let result = ClickHouseSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_insert_body() {
        let records = sample_records(2);
        let body = ClickHouseSinkConnector::build_insert_body(&records).unwrap();

        let lines: Vec<&str> = body.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        let doc0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(doc0["id"], "doc-0");
    }

    #[test]
    fn test_build_insert_body_empty() {
        let body = ClickHouseSinkConnector::build_insert_body(&[]).unwrap();
        assert!(body.is_empty());
    }

    #[test]
    fn test_build_insert_body_invalid_json() {
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("not json"),
        }];
        let result = ClickHouseSinkConnector::build_insert_body(&records);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_connector() {
        let m = base_config_map();
        let connector = ClickHouseSinkConnector::new("ch-sink", &m).unwrap();
        assert_eq!(connector.name(), "ch-sink");
        assert!(connector.buffer.is_empty());
        assert!(connector.client.is_none());
    }

    #[test]
    fn test_with_config() {
        let config = ClickHouseSinkConfig {
            connection_url: "http://localhost:8123".to_string(),
            database: "default".to_string(),
            table: "events".to_string(),
            batch_size: 500,
        };
        let connector = ClickHouseSinkConnector::with_config("ch", config);
        assert_eq!(connector.name(), "ch");
        assert_eq!(connector.config.batch_size, 500);
    }

    #[tokio::test]
    async fn test_start_creates_client() {
        let config = ClickHouseSinkConfig {
            connection_url: "http://localhost:8123".to_string(),
            database: "default".to_string(),
            table: "events".to_string(),
            batch_size: 1000,
        };
        let mut connector = ClickHouseSinkConnector::with_config("test", config);
        assert!(connector.client.is_none());

        connector.start().await.unwrap();
        assert!(connector.client.is_some());
    }
}
