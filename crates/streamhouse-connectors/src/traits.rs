//! Connector traits for the StreamHouse connectors framework.
//!
//! Defines the core `SinkConnector` and `SourceConnector` traits that all
//! connector implementations must satisfy, along with the record types
//! exchanged between the runtime and connector implementations.

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::Result;

/// A record delivered to a sink connector for writing to an external system.
///
/// This is a simplified representation of a consumed record, stripped of
/// schema-related fields to keep sink implementations decoupled from the
/// schema registry.
#[derive(Debug, Clone)]
pub struct SinkRecord {
    /// Topic the record was consumed from.
    pub topic: String,
    /// Partition within the topic.
    pub partition: u32,
    /// Offset within the partition.
    pub offset: u64,
    /// Timestamp in milliseconds since epoch.
    pub timestamp: u64,
    /// Optional record key.
    pub key: Option<Bytes>,
    /// Record value (payload).
    pub value: Bytes,
}

/// A record produced by a source connector for writing into StreamHouse.
#[derive(Debug, Clone)]
pub struct SourceRecord {
    /// Optional record key.
    pub key: Option<Bytes>,
    /// Record value (payload).
    pub value: Bytes,
    /// Optional timestamp in milliseconds since epoch.
    pub timestamp: Option<u64>,
    /// Optional target partition.
    pub partition: Option<u32>,
}

/// Trait that all sink connectors must implement.
///
/// A sink connector consumes records from StreamHouse topics and writes them
/// to an external system (S3, Postgres, Elasticsearch, etc).
#[async_trait]
pub trait SinkConnector: Send + Sync {
    /// Initialize the connector and establish connections.
    async fn start(&mut self) -> Result<()>;

    /// Accept a batch of records for writing.
    ///
    /// Implementations may buffer records internally and defer the actual
    /// write until [`flush`] is called.
    async fn put(&mut self, records: &[SinkRecord]) -> Result<()>;

    /// Flush any buffered records to the external system.
    async fn flush(&mut self) -> Result<()>;

    /// Gracefully shut down the connector, flushing remaining data.
    async fn stop(&mut self) -> Result<()>;

    /// Return the unique name of this connector instance.
    fn name(&self) -> &str;
}

/// Trait that all source connectors must implement.
///
/// A source connector reads records from an external system and produces
/// them into StreamHouse topics.
#[async_trait]
pub trait SourceConnector: Send + Sync {
    /// Initialize the connector and establish connections.
    async fn start(&mut self) -> Result<()>;

    /// Poll for new records from the external source.
    ///
    /// Returns an empty vec when no new records are available.
    async fn poll(&mut self) -> Result<Vec<SourceRecord>>;

    /// Gracefully shut down the connector.
    async fn stop(&mut self) -> Result<()>;

    /// Return the unique name of this connector instance.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // SinkRecord
    // ---------------------------------------------------------------

    #[test]
    fn test_sink_record_construction() {
        let rec = SinkRecord {
            topic: "events".to_string(),
            partition: 0,
            offset: 42,
            timestamp: 1_700_000_000_000,
            key: Some(Bytes::from("user-123")),
            value: Bytes::from(r#"{"action":"click"}"#),
        };
        assert_eq!(rec.topic, "events");
        assert_eq!(rec.partition, 0);
        assert_eq!(rec.offset, 42);
        assert_eq!(rec.timestamp, 1_700_000_000_000);
        assert_eq!(rec.key, Some(Bytes::from("user-123")));
        assert_eq!(rec.value, Bytes::from(r#"{"action":"click"}"#));
    }

    #[test]
    fn test_sink_record_no_key() {
        let rec = SinkRecord {
            topic: "t".to_string(),
            partition: 1,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("data"),
        };
        assert!(rec.key.is_none());
    }

    #[test]
    fn test_sink_record_clone() {
        let rec = SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 1,
            timestamp: 2,
            key: Some(Bytes::from("k")),
            value: Bytes::from("v"),
        };
        let cloned = rec.clone();
        assert_eq!(cloned.topic, rec.topic);
        assert_eq!(cloned.offset, rec.offset);
        assert_eq!(cloned.key, rec.key);
        assert_eq!(cloned.value, rec.value);
    }

    #[test]
    fn test_sink_record_debug() {
        let rec = SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::new(),
        };
        let debug = format!("{:?}", rec);
        assert!(debug.contains("SinkRecord"));
    }

    // ---------------------------------------------------------------
    // SourceRecord
    // ---------------------------------------------------------------

    #[test]
    fn test_source_record_construction() {
        let rec = SourceRecord {
            key: Some(Bytes::from("key")),
            value: Bytes::from("value"),
            timestamp: Some(1_700_000_000_000),
            partition: Some(3),
        };
        assert_eq!(rec.key, Some(Bytes::from("key")));
        assert_eq!(rec.value, Bytes::from("value"));
        assert_eq!(rec.timestamp, Some(1_700_000_000_000));
        assert_eq!(rec.partition, Some(3));
    }

    #[test]
    fn test_source_record_minimal() {
        let rec = SourceRecord {
            key: None,
            value: Bytes::from("payload"),
            timestamp: None,
            partition: None,
        };
        assert!(rec.key.is_none());
        assert!(rec.timestamp.is_none());
        assert!(rec.partition.is_none());
    }

    #[test]
    fn test_source_record_clone() {
        let rec = SourceRecord {
            key: Some(Bytes::from("k")),
            value: Bytes::from("v"),
            timestamp: Some(100),
            partition: Some(0),
        };
        let cloned = rec.clone();
        assert_eq!(cloned.key, rec.key);
        assert_eq!(cloned.value, rec.value);
        assert_eq!(cloned.timestamp, rec.timestamp);
        assert_eq!(cloned.partition, rec.partition);
    }

    #[test]
    fn test_source_record_debug() {
        let rec = SourceRecord {
            key: None,
            value: Bytes::new(),
            timestamp: None,
            partition: None,
        };
        let debug = format!("{:?}", rec);
        assert!(debug.contains("SourceRecord"));
    }

    // ---------------------------------------------------------------
    // Trait object safety (compile-time verification)
    // ---------------------------------------------------------------

    // These tests verify that the traits are object-safe by constructing
    // trait object references. If the traits were not object-safe, these
    // would fail to compile.

    struct MockSink;

    #[async_trait]
    impl SinkConnector for MockSink {
        async fn start(&mut self) -> Result<()> {
            Ok(())
        }
        async fn put(&mut self, _records: &[SinkRecord]) -> Result<()> {
            Ok(())
        }
        async fn flush(&mut self) -> Result<()> {
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            Ok(())
        }
        fn name(&self) -> &str {
            "mock-sink"
        }
    }

    struct MockSource;

    #[async_trait]
    impl SourceConnector for MockSource {
        async fn start(&mut self) -> Result<()> {
            Ok(())
        }
        async fn poll(&mut self) -> Result<Vec<SourceRecord>> {
            Ok(vec![])
        }
        async fn stop(&mut self) -> Result<()> {
            Ok(())
        }
        fn name(&self) -> &str {
            "mock-source"
        }
    }

    #[test]
    fn test_sink_connector_object_safety() {
        let sink = MockSink;
        let _: &dyn SinkConnector = &sink;
    }

    #[test]
    fn test_source_connector_object_safety() {
        let source = MockSource;
        let _: &dyn SourceConnector = &source;
    }

    #[tokio::test]
    async fn test_mock_sink_lifecycle() {
        let mut sink = MockSink;
        sink.start().await.unwrap();
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("test"),
        }];
        sink.put(&records).await.unwrap();
        sink.flush().await.unwrap();
        sink.stop().await.unwrap();
        assert_eq!(sink.name(), "mock-sink");
    }

    #[tokio::test]
    async fn test_mock_source_lifecycle() {
        let mut source = MockSource;
        source.start().await.unwrap();
        let records = source.poll().await.unwrap();
        assert!(records.is_empty());
        source.stop().await.unwrap();
        assert_eq!(source.name(), "mock-source");
    }
}
