//! Configuration types for the StreamHouse connectors framework.
//!
//! Defines the configuration schema used to declare, configure, and manage
//! both source and sink connectors.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default maximum number of tasks for a connector.
fn default_tasks_max() -> usize {
    1
}

/// Top-level configuration for a connector instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    /// Unique name identifying this connector instance.
    pub name: String,

    /// Whether this connector is a source or a sink.
    pub connector_type: ConnectorType,

    /// Fully-qualified class/identifier for the connector implementation.
    pub connector_class: String,

    /// Topics this connector reads from (sink) or writes to (source).
    pub topics: Vec<String>,

    /// Maximum number of parallel tasks. Defaults to 1.
    #[serde(default = "default_tasks_max")]
    pub tasks_max: usize,

    /// Arbitrary key-value configuration passed to the connector implementation.
    #[serde(default)]
    pub config: HashMap<String, String>,
}

/// Connector direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorType {
    Source,
    Sink,
}

/// Runtime state of a connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorState {
    Running,
    Paused,
    Stopped,
    Failed,
}

impl std::fmt::Display for ConnectorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectorType::Source => write!(f, "source"),
            ConnectorType::Sink => write!(f, "sink"),
        }
    }
}

impl std::fmt::Display for ConnectorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectorState::Running => write!(f, "RUNNING"),
            ConnectorState::Paused => write!(f, "PAUSED"),
            ConnectorState::Stopped => write!(f, "STOPPED"),
            ConnectorState::Failed => write!(f, "FAILED"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // ConnectorConfig serialization / deserialization
    // ---------------------------------------------------------------

    #[test]
    fn test_config_roundtrip_json() {
        let mut extra = HashMap::new();
        extra.insert("s3.bucket".to_string(), "my-bucket".to_string());

        let config = ConnectorConfig {
            name: "test-sink".to_string(),
            connector_type: ConnectorType::Sink,
            connector_class: "com.streamhouse.S3Sink".to_string(),
            topics: vec!["events".to_string(), "logs".to_string()],
            tasks_max: 4,
            config: extra,
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: ConnectorConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.name, "test-sink");
        assert_eq!(deserialized.connector_type, ConnectorType::Sink);
        assert_eq!(deserialized.connector_class, "com.streamhouse.S3Sink");
        assert_eq!(deserialized.topics, vec!["events", "logs"]);
        assert_eq!(deserialized.tasks_max, 4);
        assert_eq!(
            deserialized.config.get("s3.bucket").unwrap(),
            "my-bucket"
        );
    }

    #[test]
    fn test_config_defaults_tasks_max() {
        let json = r#"{
            "name": "my-source",
            "connector_type": "Source",
            "connector_class": "PgSource",
            "topics": ["t1"]
        }"#;
        let config: ConnectorConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.tasks_max, 1);
        assert!(config.config.is_empty());
    }

    #[test]
    fn test_config_empty_topics() {
        let json = r#"{
            "name": "empty",
            "connector_type": "Sink",
            "connector_class": "NullSink",
            "topics": []
        }"#;
        let config: ConnectorConfig = serde_json::from_str(json).expect("deserialize");
        assert!(config.topics.is_empty());
    }

    #[test]
    fn test_config_with_extra_config_map() {
        let json = r#"{
            "name": "es-sink",
            "connector_type": "Sink",
            "connector_class": "EsSink",
            "topics": ["events"],
            "tasks_max": 2,
            "config": {
                "connection.url": "http://localhost:9200",
                "index.name": "events-index",
                "batch.size": "500"
            }
        }"#;
        let config: ConnectorConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.config.len(), 3);
        assert_eq!(
            config.config.get("connection.url").unwrap(),
            "http://localhost:9200"
        );
    }

    // ---------------------------------------------------------------
    // ConnectorType
    // ---------------------------------------------------------------

    #[test]
    fn test_connector_type_display() {
        assert_eq!(format!("{}", ConnectorType::Source), "source");
        assert_eq!(format!("{}", ConnectorType::Sink), "sink");
    }

    #[test]
    fn test_connector_type_eq() {
        assert_eq!(ConnectorType::Source, ConnectorType::Source);
        assert_eq!(ConnectorType::Sink, ConnectorType::Sink);
        assert_ne!(ConnectorType::Source, ConnectorType::Sink);
    }

    #[test]
    fn test_connector_type_clone() {
        let t = ConnectorType::Source;
        let cloned = t;
        assert_eq!(t, cloned);
    }

    #[test]
    fn test_connector_type_serde_roundtrip() {
        let source_json = serde_json::to_string(&ConnectorType::Source).unwrap();
        let sink_json = serde_json::to_string(&ConnectorType::Sink).unwrap();

        assert_eq!(
            serde_json::from_str::<ConnectorType>(&source_json).unwrap(),
            ConnectorType::Source
        );
        assert_eq!(
            serde_json::from_str::<ConnectorType>(&sink_json).unwrap(),
            ConnectorType::Sink
        );
    }

    // ---------------------------------------------------------------
    // ConnectorState
    // ---------------------------------------------------------------

    #[test]
    fn test_connector_state_display() {
        assert_eq!(format!("{}", ConnectorState::Running), "RUNNING");
        assert_eq!(format!("{}", ConnectorState::Paused), "PAUSED");
        assert_eq!(format!("{}", ConnectorState::Stopped), "STOPPED");
        assert_eq!(format!("{}", ConnectorState::Failed), "FAILED");
    }

    #[test]
    fn test_connector_state_eq() {
        assert_eq!(ConnectorState::Running, ConnectorState::Running);
        assert_ne!(ConnectorState::Running, ConnectorState::Paused);
        assert_ne!(ConnectorState::Stopped, ConnectorState::Failed);
    }

    #[test]
    fn test_connector_state_serde_roundtrip() {
        let states = vec![
            ConnectorState::Running,
            ConnectorState::Paused,
            ConnectorState::Stopped,
            ConnectorState::Failed,
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: ConnectorState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, deserialized);
        }
    }

    #[test]
    fn test_connector_state_debug() {
        let debug = format!("{:?}", ConnectorState::Running);
        assert!(debug.contains("Running"));
    }

    // ---------------------------------------------------------------
    // ConnectorConfig edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_config_clone() {
        let config = ConnectorConfig {
            name: "clone-test".to_string(),
            connector_type: ConnectorType::Source,
            connector_class: "Cls".to_string(),
            topics: vec!["t1".to_string()],
            tasks_max: 1,
            config: HashMap::new(),
        };
        let cloned = config.clone();
        assert_eq!(cloned.name, "clone-test");
        assert_eq!(cloned.connector_type, ConnectorType::Source);
    }

    #[test]
    fn test_config_debug() {
        let config = ConnectorConfig {
            name: "debug-test".to_string(),
            connector_type: ConnectorType::Sink,
            connector_class: "Cls".to_string(),
            topics: vec![],
            tasks_max: 1,
            config: HashMap::new(),
        };
        let debug = format!("{:?}", config);
        assert!(debug.contains("debug-test"));
        assert!(debug.contains("Sink"));
    }
}
