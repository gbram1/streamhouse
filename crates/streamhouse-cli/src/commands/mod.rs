//! Command handlers for streamctl
//!
//! This module contains handlers for different command categories:
//! - Schema: Schema registry operations
//! - Consumer: Consumer group management operations
//! - SQL: SQL query operations

pub mod consumer;
pub mod schema;
pub mod sql;

// Re-export for convenience
pub use consumer::ConsumerCommands;
pub use schema::SchemaCommands;
pub use sql::SqlCommands;
