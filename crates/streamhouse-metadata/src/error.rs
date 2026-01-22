//! Metadata Error Types
//!
//! This module defines all error types that can occur during metadata operations.
//!
//! ## Error Categories
//!
//! ### Topic Errors
//! - `TopicNotFound`: Requested topic doesn't exist
//! - `TopicAlreadyExists`: Trying to create a topic that already exists
//!
//! ### Partition Errors
//! - `PartitionNotFound`: Requested partition doesn't exist for the topic
//!
//! ### Database Errors
//! - `DatabaseError`: SQLite/database operation failed (connection, query, etc.)
//!
//! ### Data Errors
//! - `InvalidOffset`: Offset value is invalid or out of range
//! - `SerializationError`: Failed to serialize/deserialize JSON config
//!
//! ## Usage
//!
//! All metadata store operations return `Result<T>` which is aliased to
//! `Result<T, MetadataError>`. This allows clean error propagation with `?`.
//!
//! ```ignore
//! use streamhouse_metadata::{MetadataStore, Result};
//!
//! async fn example(store: &impl MetadataStore) -> Result<()> {
//!     // Database errors automatically convert
//!     let topic = store.get_topic("orders").await?;
//!
//!     // Can check for specific errors
//!     match store.create_topic(config).await {
//!         Ok(()) => println!("Created!"),
//!         Err(MetadataError::TopicAlreadyExists(name)) => {
//!             println!("Topic {} already exists", name);
//!         }
//!         Err(e) => return Err(e),
//!     }
//!
//!     Ok(())
//! }
//! ```

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MetadataError>;

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    #[error("Topic already exists: {0}")]
    TopicAlreadyExists(String),

    #[error("Partition not found: {topic}/{partition}")]
    PartitionNotFound { topic: String, partition: u32 },

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Invalid offset: {0}")]
    InvalidOffset(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Migration error: {0}")]
    MigrationError(String),
}

impl From<sqlx::migrate::MigrateError> for MetadataError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        MetadataError::MigrationError(e.to_string())
    }
}
