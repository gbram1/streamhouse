//! Command handlers for streamctl
//!
//! This module contains handlers for different command categories:
//! - Schema: Schema registry operations
//! - Group: Consumer group operations

pub mod schema;

// Re-export for convenience
pub use schema::SchemaCommands;
