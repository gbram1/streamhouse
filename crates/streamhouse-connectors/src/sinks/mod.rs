//! Sink connector implementations.
//!
//! This module contains concrete sink connector implementations for writing
//! StreamHouse records to external systems.

pub mod elasticsearch;
pub mod s3;

// The Postgres sink is only available when the `postgres` feature is enabled.
#[cfg(feature = "postgres")]
pub mod postgres;

pub use elasticsearch::ElasticsearchSinkConnector;
pub use s3::S3SinkConnector;

#[cfg(feature = "postgres")]
pub use postgres::PostgresSinkConnector;
