//! Error types for StreamHouse client operations.
//!
//! This module defines all possible errors that can occur during Producer and Consumer
//! operations. Errors are categorized by source (agent, metadata, storage, etc.) to
//! make debugging easier.
//!
//! ## Error Handling Strategy
//!
//! - **Retriable errors**: `AgentConnectionFailed`, `Timeout`
//! - **Client errors**: `TopicNotFound`, `InvalidPartition`, `ConfigError`
//! - **Transient errors**: `NoAgentsAvailable`
//! - **Fatal errors**: `MetadataError`, `StorageError`, `Internal`
//!
//! ## Examples
//!
//! ```ignore
//! use streamhouse_client::{Producer, ClientError};
//!
//! match producer.send("topic", None, b"data", None).await {
//!     Ok(result) => println!("Success: {:?}", result),
//!     Err(ClientError::TopicNotFound(topic)) => {
//!         eprintln!("Topic '{}' does not exist", topic);
//!     }
//!     Err(ClientError::NoAgentsAvailable(group)) => {
//!         eprintln!("No healthy agents in group '{}'", group);
//!     }
//!     Err(e) => eprintln!("Error: {}", e),
//! }
//! ```

use thiserror::Error;

/// Convenience type alias for `Result<T, ClientError>`.
///
/// This is the standard Result type used throughout the client library.
/// All public APIs return this type for consistent error handling.
pub type Result<T> = std::result::Result<T, ClientError>;

/// Comprehensive error type for StreamHouse client operations.
///
/// This enum covers all possible errors that can occur when using the Producer
/// or Consumer APIs. Each variant includes context to help with debugging.
///
/// ## Error Categories
///
/// - **Configuration**: `ConfigError`
/// - **Discovery**: `NoAgentsAvailable`, `TopicNotFound`
/// - **Validation**: `InvalidPartition`
/// - **Communication**: `AgentConnectionFailed`, `AgentError`, `Timeout`
/// - **Processing**: `SerializationError`, `CompressionError`
/// - **Backend**: `MetadataError`, `StorageError`
/// - **Unknown**: `Internal`
#[derive(Debug, Error)]
pub enum ClientError {
    /// No agents available in the specified group.
    ///
    /// This error occurs when the producer/consumer cannot find any healthy agents
    /// in the configured agent group. Agents are considered healthy if they've sent
    /// a heartbeat within the last 60 seconds.
    ///
    /// ## Causes
    /// - All agents in the group are down
    /// - Agent group name is incorrect
    /// - No agents have registered yet
    ///
    /// ## Resolution
    /// - Verify agent group name matches deployed agents
    /// - Check that at least one agent is running and healthy
    /// - Wait for agents to register (first heartbeat)
    #[error("No agents available in group '{0}'")]
    NoAgentsAvailable(String),

    /// Topic does not exist in the metadata store.
    ///
    /// This error occurs when trying to produce/consume from a non-existent topic.
    ///
    /// ## Causes
    /// - Topic name is misspelled
    /// - Topic hasn't been created yet
    /// - Topic was deleted
    ///
    /// ## Resolution
    /// - Create the topic using `metadata_store.create_topic()`
    /// - Verify the topic name is correct
    #[error("Topic '{0}' does not exist")]
    TopicNotFound(String),

    /// Partition ID is out of valid range for the topic.
    ///
    /// This error occurs when explicitly specifying a partition that doesn't exist.
    /// The error includes the invalid partition ID, topic name, and maximum valid partition.
    ///
    /// ## Causes
    /// - Partition ID >= partition_count
    /// - Negative partition ID (impossible with u32, but shown for completeness)
    ///
    /// ## Resolution
    /// - Use partition IDs in range [0, partition_count)
    /// - Let the producer auto-select partition (pass None)
    #[error("Partition {0} does not exist for topic '{1}' (max: {2})")]
    InvalidPartition(u32, String, u32),

    /// Failed to establish connection with an agent.
    ///
    /// This error occurs when the client cannot connect to an agent's gRPC endpoint.
    /// Contains agent ID, address, and underlying error message.
    ///
    /// ## Causes (Phase 5.2+)
    /// - Agent is down
    /// - Network partition
    /// - Firewall blocking connection
    /// - Incorrect agent address
    ///
    /// ## Resolution
    /// - Verify agent is reachable at the address
    /// - Check network connectivity
    /// - The producer will automatically retry with another agent
    #[error("Failed to connect to agent {0} at {1}: {2}")]
    AgentConnectionFailed(String, String, String),

    /// Agent returned an error response.
    ///
    /// This error occurs when the agent successfully processed the request but
    /// returned an error (e.g., partition is full, lease not held).
    ///
    /// ## Causes (Phase 5.2+)
    /// - Agent doesn't hold lease for partition
    /// - Agent is shutting down
    /// - Disk full on agent
    /// - Rate limit exceeded
    ///
    /// ## Resolution
    /// - The producer will automatically retry with the correct agent
    /// - Check agent logs for details
    #[error("Agent {0} returned error: {1}")]
    AgentError(String, String),

    /// Metadata store operation failed.
    ///
    /// This error wraps underlying metadata store errors (PostgreSQL, SQLite, etc.).
    /// Automatically converted from `MetadataError` via the `#[from]` attribute.
    ///
    /// ## Causes
    /// - Database connection lost
    /// - Query timeout
    /// - Schema mismatch
    /// - Disk full
    ///
    /// ## Resolution
    /// - Check metadata store logs
    /// - Verify database is running and accessible
    /// - Check disk space
    #[error("Metadata store error: {0}")]
    MetadataError(#[from] streamhouse_metadata::MetadataError),

    /// Storage layer operation failed.
    ///
    /// This error wraps storage errors (S3, segment write/read failures).
    /// Contains the error message as a string.
    ///
    /// ## Causes
    /// - S3/MinIO connection failure
    /// - Bucket doesn't exist
    /// - Insufficient permissions
    /// - Segment corruption
    ///
    /// ## Resolution
    /// - Verify S3/MinIO is accessible
    /// - Check bucket exists and has correct permissions
    /// - Check storage logs for details
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Record serialization failed.
    ///
    /// This error occurs when the client cannot serialize the record data.
    ///
    /// ## Causes (Phase 5.2+)
    /// - Invalid data format
    /// - Serialization buffer overflow
    ///
    /// ## Resolution
    /// - Validate record data before sending
    /// - Check record size limits
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Compression operation failed.
    ///
    /// This error occurs when LZ4 compression fails.
    ///
    /// ## Causes
    /// - Corrupted data
    /// - Compression buffer overflow
    /// - LZ4 library error
    ///
    /// ## Resolution
    /// - Disable compression and retry
    /// - Check data validity
    #[error("Compression error: {0}")]
    CompressionError(String),

    /// Invalid client configuration.
    ///
    /// This error occurs when the Producer/Consumer is misconfigured.
    ///
    /// ## Causes
    /// - Required fields missing (e.g., metadata_store)
    /// - Invalid parameter values (e.g., negative timeout)
    ///
    /// ## Resolution
    /// - Review builder configuration
    /// - Ensure all required fields are set
    /// - Validate parameter ranges
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Operation exceeded configured timeout.
    ///
    /// This error occurs when a request takes longer than the configured timeout.
    ///
    /// ## Causes
    /// - Agent is slow to respond
    /// - Network latency
    /// - Agent is overloaded
    /// - Timeout configured too low
    ///
    /// ## Resolution
    /// - Increase request_timeout in builder
    /// - Check agent performance
    /// - Check network latency
    #[error("Operation timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Internal error that shouldn't normally occur.
    ///
    /// This error indicates a bug in the client library or an unexpected state.
    ///
    /// ## Causes
    /// - Programming error
    /// - Race condition
    /// - Invalid internal state
    ///
    /// ## Resolution
    /// - Report as a bug with full error message
    /// - Check for library updates
    #[error("Internal error: {0}")]
    Internal(String),
}
