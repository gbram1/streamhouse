//! Kafka Source Connector
//!
//! Reads records from an Apache Kafka cluster and produces them into StreamHouse.
//! This implementation provides the configuration parsing, validation, and lifecycle
//! framework; the actual Kafka consumer integration (e.g. via `rdkafka`) can be
//! plugged in later.
//!
//! ## Configuration
//!
//! | Key                   | Description                                    | Default     |
//! |-----------------------|------------------------------------------------|-------------|
//! | `bootstrap.servers`   | Kafka broker addresses                         | required    |
//! | `topics`              | Comma-separated list of topics to subscribe to | required    |
//! | `group.id`            | Consumer group ID                              | required    |
//! | `auto.offset.reset`   | Where to start consuming: `earliest`/`latest`  | `earliest`  |
//! | `max.poll.records`    | Maximum records returned per poll              | `500`       |
//! | `poll.timeout.ms`     | Poll timeout in milliseconds                   | `1000`      |
//! | `security.protocol`   | Security protocol: `PLAINTEXT`, `SASL_SSL`, etc | `PLAINTEXT` |
//! | `sasl.mechanism`      | SASL mechanism (e.g. `PLAIN`, `SCRAM-SHA-256`) | (none)      |
//! | `sasl.username`       | SASL username                                  | (none)      |
//! | `sasl.password`       | SASL password                                  | (none)      |

use std::collections::HashMap;

use async_trait::async_trait;
use tracing;

use crate::config::ConnectorState;
use crate::error::{ConnectorError, Result};
use crate::traits::{SourceConnector, SourceRecord};

/// Parsed configuration for the Kafka source connector.
#[derive(Debug, Clone)]
pub struct KafkaSourceConfig {
    /// Comma-separated list of Kafka broker addresses.
    pub bootstrap_servers: String,
    /// Topics to subscribe to.
    pub topics: Vec<String>,
    /// Consumer group ID.
    pub group_id: String,
    /// Auto offset reset policy: `"earliest"` or `"latest"`.
    pub auto_offset_reset: String,
    /// Maximum number of records returned per poll.
    pub max_poll_records: usize,
    /// Poll timeout in milliseconds.
    pub poll_timeout_ms: u64,
    /// Security protocol (e.g. `PLAINTEXT`, `SASL_SSL`).
    pub security_protocol: String,
    /// SASL mechanism (e.g. `PLAIN`, `SCRAM-SHA-256`).
    pub sasl_mechanism: Option<String>,
    /// SASL username.
    pub sasl_username: Option<String>,
    /// SASL password.
    pub sasl_password: Option<String>,
}

impl KafkaSourceConfig {
    /// Parse a `KafkaSourceConfig` from a string key-value map.
    ///
    /// Required keys: `bootstrap.servers`, `topics`, `group.id`.
    pub fn from_config_map(config: &HashMap<String, String>) -> Result<Self> {
        let bootstrap_servers = config
            .get("bootstrap.servers")
            .ok_or_else(|| {
                ConnectorError::ConfigError(
                    "missing required 'bootstrap.servers'".to_string(),
                )
            })?
            .clone();

        if bootstrap_servers.trim().is_empty() {
            return Err(ConnectorError::ConfigError(
                "'bootstrap.servers' must not be empty".to_string(),
            ));
        }

        let topics_raw = config
            .get("topics")
            .ok_or_else(|| {
                ConnectorError::ConfigError("missing required 'topics'".to_string())
            })?
            .clone();

        let topics: Vec<String> = topics_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if topics.is_empty() {
            return Err(ConnectorError::ConfigError(
                "'topics' must contain at least one topic".to_string(),
            ));
        }

        let group_id = config
            .get("group.id")
            .ok_or_else(|| {
                ConnectorError::ConfigError("missing required 'group.id'".to_string())
            })?
            .clone();

        if group_id.trim().is_empty() {
            return Err(ConnectorError::ConfigError(
                "'group.id' must not be empty".to_string(),
            ));
        }

        let auto_offset_reset = config
            .get("auto.offset.reset")
            .cloned()
            .unwrap_or_else(|| "earliest".to_string());

        // Validate auto.offset.reset
        match auto_offset_reset.as_str() {
            "earliest" | "latest" => {}
            other => {
                return Err(ConnectorError::ConfigError(format!(
                    "invalid auto.offset.reset '{}': must be 'earliest' or 'latest'",
                    other
                )));
            }
        }

        let max_poll_records = config
            .get("max.poll.records")
            .map(|s| {
                s.parse::<usize>().map_err(|e| {
                    ConnectorError::ConfigError(format!("invalid max.poll.records: {}", e))
                })
            })
            .transpose()?
            .unwrap_or(500);

        let poll_timeout_ms = config
            .get("poll.timeout.ms")
            .map(|s| {
                s.parse::<u64>().map_err(|e| {
                    ConnectorError::ConfigError(format!("invalid poll.timeout.ms: {}", e))
                })
            })
            .transpose()?
            .unwrap_or(1000);

        let security_protocol = config
            .get("security.protocol")
            .cloned()
            .unwrap_or_else(|| "PLAINTEXT".to_string());

        let sasl_mechanism = config.get("sasl.mechanism").cloned();
        let sasl_username = config.get("sasl.username").cloned();
        let sasl_password = config.get("sasl.password").cloned();

        Ok(KafkaSourceConfig {
            bootstrap_servers,
            topics,
            group_id,
            auto_offset_reset,
            max_poll_records,
            poll_timeout_ms,
            security_protocol,
            sasl_mechanism,
            sasl_username,
            sasl_password,
        })
    }
}

/// Kafka Source Connector implementation.
///
/// Provides the lifecycle framework for consuming records from Kafka.
/// The actual Kafka consumer (e.g. via `rdkafka`) is not included in this
/// crate; the `poll()` method returns an empty vec as a placeholder.
pub struct KafkaSourceConnector {
    name: String,
    config: KafkaSourceConfig,
    state: ConnectorState,
    buffer: Vec<SourceRecord>,
}

impl KafkaSourceConnector {
    /// Create a new `KafkaSourceConnector` with the given name and config map.
    pub fn new(name: &str, config_map: &HashMap<String, String>) -> Result<Self> {
        let config = KafkaSourceConfig::from_config_map(config_map)?;
        Ok(Self {
            name: name.to_string(),
            config,
            state: ConnectorState::Stopped,
            buffer: Vec::new(),
        })
    }

    /// Create with an already-parsed config.
    pub fn with_config(name: &str, config: KafkaSourceConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            state: ConnectorState::Stopped,
            buffer: Vec::new(),
        }
    }

    /// Return a reference to the parsed configuration.
    pub fn config(&self) -> &KafkaSourceConfig {
        &self.config
    }

    /// Return the current connector state.
    pub fn connector_state(&self) -> ConnectorState {
        self.state
    }
}

#[async_trait]
impl SourceConnector for KafkaSourceConnector {
    async fn start(&mut self) -> Result<()> {
        if self.state == ConnectorState::Running {
            return Err(ConnectorError::SourceError(
                "connector is already running".to_string(),
            ));
        }

        // @Note this would create a Kafka consumer,
        // subscribe to the configured topics, and begin consuming.
        tracing::info!(
            connector = %self.name,
            bootstrap_servers = %self.config.bootstrap_servers,
            topics = ?self.config.topics,
            group_id = %self.config.group_id,
            "Kafka source connector started"
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

        // @Note this would poll the Kafka consumer
        // for up to `max_poll_records` records with a timeout of
        // `poll_timeout_ms` milliseconds.
        //
        // For now, drain any records that have been manually pushed
        // into the buffer (useful for testing), or return empty.
        let records: Vec<SourceRecord> = self.buffer.drain(..).collect();
        Ok(records)
    }

    async fn stop(&mut self) -> Result<()> {
        if self.state == ConnectorState::Stopped {
            return Err(ConnectorError::SourceError(
                "connector is already stopped".to_string(),
            ));
        }

        // @Note this would commit offsets and close
        // the Kafka consumer.
        tracing::info!(connector = %self.name, "Kafka source connector stopped");

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
    use bytes::Bytes;

    /// Helper to create a minimal valid config map.
    fn base_config_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(
            "bootstrap.servers".to_string(),
            "localhost:9092".to_string(),
        );
        m.insert("topics".to_string(), "events,logs".to_string());
        m.insert("group.id".to_string(), "streamhouse-cg".to_string());
        m
    }

    // ---------------------------------------------------------------
    // Config parsing - valid configurations
    // ---------------------------------------------------------------

    #[test]
    fn test_config_parse_minimal() {
        let m = base_config_map();
        let config = KafkaSourceConfig::from_config_map(&m).unwrap();
        assert_eq!(config.bootstrap_servers, "localhost:9092");
        assert_eq!(config.topics, vec!["events", "logs"]);
        assert_eq!(config.group_id, "streamhouse-cg");
        assert_eq!(config.auto_offset_reset, "earliest");
        assert_eq!(config.max_poll_records, 500);
        assert_eq!(config.poll_timeout_ms, 1000);
        assert_eq!(config.security_protocol, "PLAINTEXT");
        assert!(config.sasl_mechanism.is_none());
        assert!(config.sasl_username.is_none());
        assert!(config.sasl_password.is_none());
    }

    #[test]
    fn test_config_parse_all_options() {
        let mut m = base_config_map();
        m.insert("auto.offset.reset".to_string(), "latest".to_string());
        m.insert("max.poll.records".to_string(), "1000".to_string());
        m.insert("poll.timeout.ms".to_string(), "5000".to_string());
        m.insert("security.protocol".to_string(), "SASL_SSL".to_string());
        m.insert("sasl.mechanism".to_string(), "PLAIN".to_string());
        m.insert("sasl.username".to_string(), "admin".to_string());
        m.insert("sasl.password".to_string(), "secret".to_string());

        let config = KafkaSourceConfig::from_config_map(&m).unwrap();
        assert_eq!(config.auto_offset_reset, "latest");
        assert_eq!(config.max_poll_records, 1000);
        assert_eq!(config.poll_timeout_ms, 5000);
        assert_eq!(config.security_protocol, "SASL_SSL");
        assert_eq!(config.sasl_mechanism, Some("PLAIN".to_string()));
        assert_eq!(config.sasl_username, Some("admin".to_string()));
        assert_eq!(config.sasl_password, Some("secret".to_string()));
    }

    #[test]
    fn test_config_single_topic() {
        let mut m = base_config_map();
        m.insert("topics".to_string(), "single-topic".to_string());
        let config = KafkaSourceConfig::from_config_map(&m).unwrap();
        assert_eq!(config.topics, vec!["single-topic"]);
    }

    #[test]
    fn test_config_topics_with_whitespace() {
        let mut m = base_config_map();
        m.insert("topics".to_string(), " a , b , c ".to_string());
        let config = KafkaSourceConfig::from_config_map(&m).unwrap();
        assert_eq!(config.topics, vec!["a", "b", "c"]);
    }

    // ---------------------------------------------------------------
    // Config parsing - validation errors
    // ---------------------------------------------------------------

    #[test]
    fn test_config_missing_bootstrap_servers() {
        let mut m = base_config_map();
        m.remove("bootstrap.servers");
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("bootstrap.servers"));
    }

    #[test]
    fn test_config_empty_bootstrap_servers() {
        let mut m = base_config_map();
        m.insert("bootstrap.servers".to_string(), "  ".to_string());
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_topics() {
        let mut m = base_config_map();
        m.remove("topics");
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("topics"));
    }

    #[test]
    fn test_config_empty_topics() {
        let mut m = base_config_map();
        m.insert("topics".to_string(), "  ,  , ".to_string());
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_group_id() {
        let mut m = base_config_map();
        m.remove("group.id");
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("group.id"));
    }

    #[test]
    fn test_config_empty_group_id() {
        let mut m = base_config_map();
        m.insert("group.id".to_string(), "  ".to_string());
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_auto_offset_reset() {
        let mut m = base_config_map();
        m.insert("auto.offset.reset".to_string(), "middle".to_string());
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("auto.offset.reset"));
    }

    #[test]
    fn test_config_invalid_max_poll_records() {
        let mut m = base_config_map();
        m.insert("max.poll.records".to_string(), "not_a_number".to_string());
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_poll_timeout() {
        let mut m = base_config_map();
        m.insert("poll.timeout.ms".to_string(), "abc".to_string());
        let result = KafkaSourceConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------

    #[test]
    fn test_new_connector() {
        let m = base_config_map();
        let connector = KafkaSourceConnector::new("kafka-src", &m).unwrap();
        assert_eq!(connector.name(), "kafka-src");
        assert_eq!(connector.connector_state(), ConnectorState::Stopped);
        assert!(connector.buffer.is_empty());
    }

    #[test]
    fn test_new_connector_missing_config() {
        let m = HashMap::new();
        let result = KafkaSourceConnector::new("bad", &m);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_config() {
        let config = KafkaSourceConfig {
            bootstrap_servers: "broker:9092".to_string(),
            topics: vec!["t1".to_string()],
            group_id: "g1".to_string(),
            auto_offset_reset: "latest".to_string(),
            max_poll_records: 100,
            poll_timeout_ms: 2000,
            security_protocol: "PLAINTEXT".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        };
        let connector = KafkaSourceConnector::with_config("named", config);
        assert_eq!(connector.name(), "named");
        assert_eq!(connector.config().max_poll_records, 100);
        assert_eq!(connector.config().poll_timeout_ms, 2000);
    }

    // ---------------------------------------------------------------
    // Lifecycle - start / poll / stop
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_start_transitions_to_running() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Stopped);

        connector.start().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Running);
    }

    #[tokio::test]
    async fn test_start_when_already_running_fails() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        let result = connector.start().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_poll_returns_empty_when_no_records() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        let records = connector.poll().await.unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_poll_when_not_running_fails() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();

        let result = connector.poll().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_poll_drains_buffer() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        // Manually push records into the buffer (simulating a real consumer)
        connector.buffer.push(SourceRecord {
            key: Some(Bytes::from("key-1")),
            value: Bytes::from(r#"{"msg":"hello"}"#),
            timestamp: Some(1_700_000_000_000),
            partition: Some(0),
        });
        connector.buffer.push(SourceRecord {
            key: None,
            value: Bytes::from(r#"{"msg":"world"}"#),
            timestamp: Some(1_700_000_001_000),
            partition: Some(1),
        });

        let records = connector.poll().await.unwrap();
        assert_eq!(records.len(), 2);
        assert!(connector.buffer.is_empty());

        // Subsequent poll should return empty
        let records = connector.poll().await.unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_stop_transitions_to_stopped() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        connector.stop().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Stopped);
    }

    #[tokio::test]
    async fn test_stop_when_already_stopped_fails() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();

        let result = connector.stop().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stop_clears_buffer() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("test", &m).unwrap();
        connector.start().await.unwrap();

        connector.buffer.push(SourceRecord {
            key: None,
            value: Bytes::from("data"),
            timestamp: None,
            partition: None,
        });

        connector.stop().await.unwrap();
        assert!(connector.buffer.is_empty());
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let m = base_config_map();
        let mut connector = KafkaSourceConnector::new("lifecycle-test", &m).unwrap();

        // Start
        connector.start().await.unwrap();
        assert_eq!(connector.connector_state(), ConnectorState::Running);

        // Poll (empty)
        let records = connector.poll().await.unwrap();
        assert!(records.is_empty());

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
        let config = KafkaSourceConfig::from_config_map(&m).unwrap();
        let cloned = config.clone();
        assert_eq!(cloned.bootstrap_servers, config.bootstrap_servers);
        assert_eq!(cloned.topics, config.topics);
        assert_eq!(cloned.group_id, config.group_id);
    }

    #[test]
    fn test_config_debug() {
        let m = base_config_map();
        let config = KafkaSourceConfig::from_config_map(&m).unwrap();
        let debug = format!("{:?}", config);
        assert!(debug.contains("localhost:9092"));
        assert!(debug.contains("events"));
        assert!(debug.contains("streamhouse-cg"));
    }
}
