//! Error types for StreamHouse client operations

use thiserror::Error;

/// Result type for client operations
pub type Result<T> = std::result::Result<T, ClientError>;

/// Errors that can occur during client operations
#[derive(Debug, Error)]
pub enum ClientError {
    /// No agents available in the specified group
    #[error("No agents available in group '{0}'")]
    NoAgentsAvailable(String),

    /// Topic does not exist
    #[error("Topic '{0}' does not exist")]
    TopicNotFound(String),

    /// Partition ID out of range
    #[error("Partition {0} does not exist for topic '{1}' (max: {2})")]
    InvalidPartition(u32, String, u32),

    /// Failed to connect to agent
    #[error("Failed to connect to agent {0} at {1}: {2}")]
    AgentConnectionFailed(String, String, String),

    /// Agent returned an error
    #[error("Agent {0} returned error: {1}")]
    AgentError(String, String),

    /// Metadata store operation failed
    #[error("Metadata store error: {0}")]
    MetadataError(#[from] streamhouse_metadata::MetadataError),

    /// Storage operation failed
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Compression error
    #[error("Compression error: {0}")]
    CompressionError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Timeout error
    #[error("Operation timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}
