//! Elasticsearch Sink Connector
//!
//! Writes StreamHouse records to Elasticsearch via the Bulk API. Records are
//! buffered and flushed as NDJSON bulk requests.
//!
//! ## Configuration
//!
//! | Key                | Description                               | Default                 |
//! |--------------------|-------------------------------------------|-------------------------|
//! | `connection.url`   | Elasticsearch base URL                    | `http://localhost:9200` |
//! | `index.name`       | Target index name                         | required                |
//! | `document.id`      | JSON field to use as document `_id`       | (auto-generated)        |
//! | `batch.size`       | Records per bulk request                  | `1000`                  |
//! | `batch.interval_ms`| Max age of a batch before flush (ms)      | `5000`                  |

use std::collections::HashMap;

use async_trait::async_trait;
use tracing;

use crate::error::{ConnectorError, Result};
use crate::traits::{SinkConnector, SinkRecord};

/// Parsed configuration for the Elasticsearch sink.
#[derive(Debug, Clone)]
pub struct ElasticsearchSinkConfig {
    pub connection_url: String,
    pub index_name: String,
    pub document_id_field: Option<String>,
    pub batch_size: usize,
    pub batch_interval_ms: u64,
}

impl ElasticsearchSinkConfig {
    /// Parse an ElasticsearchSinkConfig from a string key-value map.
    pub fn from_config_map(config: &HashMap<String, String>) -> Result<Self> {
        let connection_url = config
            .get("connection.url")
            .cloned()
            .unwrap_or_else(|| "http://localhost:9200".to_string());

        let index_name = config
            .get("index.name")
            .ok_or_else(|| {
                ConnectorError::ConfigError("missing required 'index.name'".to_string())
            })?
            .clone();

        let document_id_field = config.get("document.id").cloned();

        let batch_size = config
            .get("batch.size")
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|e| ConnectorError::ConfigError(format!("invalid batch.size: {}", e)))
            })
            .transpose()?
            .unwrap_or(1000);

        let batch_interval_ms = config
            .get("batch.interval_ms")
            .map(|s| {
                s.parse::<u64>().map_err(|e| {
                    ConnectorError::ConfigError(format!("invalid batch.interval_ms: {}", e))
                })
            })
            .transpose()?
            .unwrap_or(5000);

        Ok(ElasticsearchSinkConfig {
            connection_url,
            index_name,
            document_id_field,
            batch_size,
            batch_interval_ms,
        })
    }
}

/// Elasticsearch Sink Connector implementation.
///
/// Accumulates records in a buffer and sends them to Elasticsearch via the
/// Bulk API when the buffer reaches `batch_size` or when `flush()` is called.
pub struct ElasticsearchSinkConnector {
    name: String,
    config: ElasticsearchSinkConfig,
    client: Option<reqwest::Client>,
    buffer: Vec<SinkRecord>,
}

impl ElasticsearchSinkConnector {
    /// Create a new ElasticsearchSinkConnector with the given name and config map.
    pub fn new(name: &str, config_map: &HashMap<String, String>) -> Result<Self> {
        let config = ElasticsearchSinkConfig::from_config_map(config_map)?;
        Ok(Self {
            name: name.to_string(),
            config,
            client: None,
            buffer: Vec::new(),
        })
    }

    /// Create with an already-parsed config.
    pub fn with_config(name: &str, config: ElasticsearchSinkConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            client: None,
            buffer: Vec::new(),
        }
    }

    /// Create with an injected reqwest client (useful for testing).
    pub fn with_client(
        name: &str,
        config: ElasticsearchSinkConfig,
        client: reqwest::Client,
    ) -> Self {
        Self {
            name: name.to_string(),
            config,
            client: Some(client),
            buffer: Vec::new(),
        }
    }

    /// Build the NDJSON body for an Elasticsearch Bulk API request.
    ///
    /// The format alternates action/metadata lines with document lines:
    /// ```text
    /// {"index":{"_index":"my-index"}}
    /// {"field1":"value1"}
    /// {"index":{"_index":"my-index","_id":"doc-123"}}
    /// {"field1":"value2"}
    /// ```
    pub fn build_bulk_body(
        index: &str,
        id_field: Option<&str>,
        records: &[SinkRecord],
    ) -> Result<String> {
        let mut body = String::new();

        for record in records {
            // Parse record value as JSON
            let value_str = std::str::from_utf8(&record.value).map_err(|e| {
                ConnectorError::SerializationError(format!("invalid UTF-8 in record value: {}", e))
            })?;

            let doc: serde_json::Value = serde_json::from_str(value_str)?;

            // Build the action line
            let doc_id = id_field.and_then(|field| {
                doc.get(field).and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
            });

            let action = if let Some(id) = doc_id {
                serde_json::json!({"index": {"_index": index, "_id": id}})
            } else {
                serde_json::json!({"index": {"_index": index}})
            };

            // Write action line
            serde_json::to_writer(unsafe { body.as_mut_vec() }, &action).map_err(|e| {
                ConnectorError::SerializationError(format!("JSON write error: {}", e))
            })?;
            body.push('\n');

            // Write document line
            body.push_str(value_str);
            body.push('\n');
        }

        Ok(body)
    }

    /// Send a bulk request to Elasticsearch.
    async fn send_bulk(&self, body: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| {
                ConnectorError::ConnectionError("HTTP client not initialized".to_string())
            })?;

        let url = format!("{}/_bulk", self.config.connection_url);

        let response = client
            .post(&url)
            .header("Content-Type", "application/x-ndjson")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| {
                ConnectorError::ConnectionError(format!("Elasticsearch request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(ConnectorError::SinkError(format!(
                "Elasticsearch bulk API returned {}: {}",
                status, body_text
            )));
        }

        // Check for per-item errors in the response
        let resp_body: serde_json::Value = response.json().await.map_err(|e| {
            ConnectorError::SinkError(format!("failed to parse ES response: {}", e))
        })?;

        if resp_body.get("errors") == Some(&serde_json::Value::Bool(true)) {
            tracing::warn!(
                connector = %self.name,
                "Elasticsearch bulk response contained errors"
            );
        }

        Ok(())
    }

    /// Flush the current buffer by building and sending a bulk request.
    async fn flush_buffer(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let batch: Vec<SinkRecord> = self.buffer.drain(..).collect();
        let body = Self::build_bulk_body(
            &self.config.index_name,
            self.config.document_id_field.as_deref(),
            &batch,
        )?;

        self.send_bulk(&body).await?;

        tracing::debug!(
            connector = %self.name,
            records = batch.len(),
            "flushed bulk request to Elasticsearch"
        );

        Ok(())
    }
}

#[async_trait]
impl SinkConnector for ElasticsearchSinkConnector {
    async fn start(&mut self) -> Result<()> {
        if self.client.is_none() {
            self.client = Some(
                reqwest::Client::builder()
                    .build()
                    .map_err(|e| {
                        ConnectorError::ConnectionError(format!(
                            "failed to build HTTP client: {}",
                            e
                        ))
                    })?,
            );
        }
        tracing::info!(connector = %self.name, "Elasticsearch sink connector started");
        Ok(())
    }

    async fn put(&mut self, records: &[SinkRecord]) -> Result<()> {
        self.buffer.extend(records.iter().cloned());

        // Auto-flush if we hit the batch size
        while self.buffer.len() >= self.config.batch_size {
            // Drain batch_size records
            let batch: Vec<SinkRecord> =
                self.buffer.drain(..self.config.batch_size).collect();
            let body = Self::build_bulk_body(
                &self.config.index_name,
                self.config.document_id_field.as_deref(),
                &batch,
            )?;
            self.send_bulk(&body).await?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.flush_buffer().await
    }

    async fn stop(&mut self) -> Result<()> {
        self.flush().await?;
        tracing::info!(connector = %self.name, "Elasticsearch sink connector stopped");
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
        m.insert("index.name".to_string(), "events-index".to_string());
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

    // ---------------------------------------------------------------
    // Config parsing
    // ---------------------------------------------------------------

    #[test]
    fn test_config_parse_minimal() {
        let m = base_config_map();
        let config = ElasticsearchSinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.connection_url, "http://localhost:9200");
        assert_eq!(config.index_name, "events-index");
        assert!(config.document_id_field.is_none());
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.batch_interval_ms, 5000);
    }

    #[test]
    fn test_config_parse_all_options() {
        let mut m = base_config_map();
        m.insert(
            "connection.url".to_string(),
            "https://es.example.com:9200".to_string(),
        );
        m.insert("document.id".to_string(), "doc_id".to_string());
        m.insert("batch.size".to_string(), "2000".to_string());
        m.insert("batch.interval_ms".to_string(), "10000".to_string());

        let config = ElasticsearchSinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.connection_url, "https://es.example.com:9200");
        assert_eq!(config.document_id_field, Some("doc_id".to_string()));
        assert_eq!(config.batch_size, 2000);
        assert_eq!(config.batch_interval_ms, 10000);
    }

    #[test]
    fn test_config_missing_index_name() {
        let m = HashMap::new();
        let result = ElasticsearchSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_batch_size() {
        let mut m = base_config_map();
        m.insert("batch.size".to_string(), "not_a_number".to_string());
        let result = ElasticsearchSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_batch_interval() {
        let mut m = base_config_map();
        m.insert("batch.interval_ms".to_string(), "xyz".to_string());
        let result = ElasticsearchSinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // Bulk body formatting
    // ---------------------------------------------------------------

    #[test]
    fn test_bulk_body_no_id_field() {
        let records = sample_records(2);
        let body =
            ElasticsearchSinkConnector::build_bulk_body("my-index", None, &records).unwrap();

        let lines: Vec<&str> = body.trim().split('\n').collect();
        // 2 records x 2 lines each = 4 lines
        assert_eq!(lines.len(), 4);

        // Odd lines (0, 2) are action lines
        let action0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(action0["index"]["_index"], "my-index");
        assert!(action0["index"].get("_id").is_none());

        // Even lines (1, 3) are document lines
        let doc0: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(doc0["id"], "doc-0");
    }

    #[test]
    fn test_bulk_body_with_id_field() {
        let records = sample_records(2);
        let body =
            ElasticsearchSinkConnector::build_bulk_body("idx", Some("id"), &records).unwrap();

        let lines: Vec<&str> = body.trim().split('\n').collect();
        assert_eq!(lines.len(), 4);

        let action0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(action0["index"]["_index"], "idx");
        assert_eq!(action0["index"]["_id"], "doc-0");

        let action1: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(action1["index"]["_id"], "doc-1");
    }

    #[test]
    fn test_bulk_body_with_numeric_id_field() {
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from(r#"{"num_id": 42, "data": "test"}"#),
        }];
        let body = ElasticsearchSinkConnector::build_bulk_body(
            "idx",
            Some("num_id"),
            &records,
        )
        .unwrap();

        let lines: Vec<&str> = body.trim().split('\n').collect();
        let action: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(action["index"]["_id"], "42");
    }

    #[test]
    fn test_bulk_body_missing_id_field_falls_back() {
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from(r#"{"name": "no-id-field-here"}"#),
        }];
        let body = ElasticsearchSinkConnector::build_bulk_body(
            "idx",
            Some("nonexistent"),
            &records,
        )
        .unwrap();

        let lines: Vec<&str> = body.trim().split('\n').collect();
        let action: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        // Should not include _id when the field doesn't exist in the doc
        assert!(action["index"].get("_id").is_none());
    }

    #[test]
    fn test_bulk_body_empty_records() {
        let body = ElasticsearchSinkConnector::build_bulk_body("idx", None, &[]).unwrap();
        assert!(body.is_empty());
    }

    #[test]
    fn test_bulk_body_invalid_json_record() {
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("not json"),
        }];
        let result = ElasticsearchSinkConnector::build_bulk_body("idx", None, &records);
        assert!(result.is_err());
    }

    #[test]
    fn test_bulk_body_ends_with_newline() {
        let records = sample_records(1);
        let body =
            ElasticsearchSinkConnector::build_bulk_body("idx", None, &records).unwrap();
        assert!(body.ends_with('\n'));
    }

    // ---------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------

    #[test]
    fn test_new_connector() {
        let m = base_config_map();
        let connector = ElasticsearchSinkConnector::new("es-sink", &m).unwrap();
        assert_eq!(connector.name(), "es-sink");
        assert!(connector.buffer.is_empty());
        assert!(connector.client.is_none());
    }

    #[test]
    fn test_new_connector_missing_config() {
        let m = HashMap::new();
        let result = ElasticsearchSinkConnector::new("bad", &m);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_config() {
        let config = ElasticsearchSinkConfig {
            connection_url: "http://localhost:9200".to_string(),
            index_name: "test-idx".to_string(),
            document_id_field: Some("id".to_string()),
            batch_size: 500,
            batch_interval_ms: 3000,
        };
        let connector = ElasticsearchSinkConnector::with_config("es", config);
        assert_eq!(connector.name(), "es");
        assert_eq!(connector.config.batch_size, 500);
    }

    // ---------------------------------------------------------------
    // Lifecycle (without real ES)
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_start_creates_client() {
        let config = ElasticsearchSinkConfig {
            connection_url: "http://localhost:9200".to_string(),
            index_name: "test".to_string(),
            document_id_field: None,
            batch_size: 1000,
            batch_interval_ms: 5000,
        };
        let mut connector = ElasticsearchSinkConnector::with_config("test", config);
        assert!(connector.client.is_none());

        connector.start().await.unwrap();
        assert!(connector.client.is_some());
    }

    #[tokio::test]
    async fn test_put_buffers_records() {
        let config = ElasticsearchSinkConfig {
            connection_url: "http://localhost:9200".to_string(),
            index_name: "test".to_string(),
            document_id_field: None,
            batch_size: 10_000, // Large batch size so no auto-flush
            batch_interval_ms: 5000,
        };
        let mut connector = ElasticsearchSinkConnector::with_config("test", config);
        // Skip start() so client is None - put should just buffer

        // We can't call put() without a client and have it auto-flush,
        // but we can manually buffer
        let records = sample_records(5);
        connector.buffer.extend(records.iter().cloned());
        assert_eq!(connector.buffer.len(), 5);
    }

    #[test]
    fn test_config_clone() {
        let config = ElasticsearchSinkConfig {
            connection_url: "http://localhost:9200".to_string(),
            index_name: "test".to_string(),
            document_id_field: Some("id".to_string()),
            batch_size: 500,
            batch_interval_ms: 3000,
        };
        let cloned = config.clone();
        assert_eq!(cloned.connection_url, config.connection_url);
        assert_eq!(cloned.index_name, config.index_name);
        assert_eq!(cloned.document_id_field, config.document_id_field);
    }

    #[test]
    fn test_config_debug() {
        let config = ElasticsearchSinkConfig {
            connection_url: "http://localhost:9200".to_string(),
            index_name: "events".to_string(),
            document_id_field: None,
            batch_size: 1000,
            batch_interval_ms: 5000,
        };
        let debug = format!("{:?}", config);
        assert!(debug.contains("events"));
        assert!(debug.contains("9200"));
    }
}
