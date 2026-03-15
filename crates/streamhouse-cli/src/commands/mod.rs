//! Command handlers for streamctl
//!
//! This module contains handlers for different command categories:
//! - Schema: Schema registry operations
//! - Consumer: Consumer group management operations
//! - SQL: SQL query operations
//! - Pipeline: Pipeline management operations
//! - Connector: Connector management operations
//! - Org: Organization and API key management
//! - Metrics: System metrics and health
//! - Init: Project scaffolding
//! - Status: Cluster status dashboard
//! - Apply/Destroy: Declarative config management
//! - Logs: Pipeline log tailing

pub mod apply;
pub mod auth;
pub mod connector;
pub mod consumer;
pub mod init;
pub mod logs;
pub mod metrics;
pub mod org;
pub mod pipeline;
pub mod schema;
pub mod sql;
pub mod status;

// Re-export for convenience
pub use connector::ConnectorCommands;
pub use consumer::ConsumerCommands;
pub use metrics::MetricsCommands;
pub use org::OrgCommands;
pub use pipeline::PipelineCommands;
pub use schema::SchemaCommands;
pub use sql::SqlCommands;
