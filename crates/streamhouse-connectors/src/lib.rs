//! StreamHouse Connectors Framework
//!
//! Provides a framework for building source and sink connectors that integrate
//! StreamHouse with external systems. Includes concrete implementations for
//! S3, PostgreSQL, and Elasticsearch sinks, as well as Kafka and Debezium CDC
//! source connectors.
//!
//! ## Architecture
//!
//! - **Traits**: `SinkConnector` and `SourceConnector` define the connector interface.
//! - **Config**: `ConnectorConfig` provides a unified configuration schema.
//! - **Runtime**: `ConnectorRuntime` manages connector lifecycles as background tasks.
//! - **Sinks**: Ready-to-use sink implementations for S3, Postgres, and Elasticsearch.
//! - **Sources**: Source connector implementations for Kafka and Debezium CDC.
//!
//! ## Feature Flags
//!
//! - `postgres` - Enables the PostgreSQL sink connector (requires `sqlx` with postgres).

pub mod config;
pub mod error;
pub mod runtime;
pub mod sinks;
pub mod sources;
pub mod traits;

// Re-export key types at crate root for convenience.
pub use config::{ConnectorConfig, ConnectorState, ConnectorType};
pub use error::{ConnectorError, Result};
pub use runtime::ConnectorRuntime;
pub use sinks::ElasticsearchSinkConnector;
pub use sinks::S3SinkConnector;
pub use sources::DebeziumSourceConnector;
pub use sources::KafkaSourceConnector;
pub use traits::{SinkConnector, SinkRecord, SourceConnector, SourceRecord};

#[cfg(feature = "postgres")]
pub use sinks::PostgresSinkConnector;
