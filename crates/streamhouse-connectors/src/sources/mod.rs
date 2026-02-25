//! Source connector implementations.
//!
//! This module contains concrete source connector implementations for reading
//! records from external systems and producing them into StreamHouse.

pub mod debezium;
pub mod kafka;

pub use debezium::DebeziumSourceConnector;
pub use kafka::KafkaSourceConnector;
