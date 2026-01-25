//! Error types for StreamHouse Agent.
//!
//! This module defines all error types that can occur during agent operations.
//!
//! ## Error Categories
//!
//! ### Lifecycle Errors
//! - `NotStarted`: Agent operation called before `start()`
//! - `AlreadyStarted`: Attempted to start an already-running agent
//!
//! ### Lease Errors
//! - `LeaseHeldByOther`: Tried to write to partition owned by another agent
//! - `LeaseExpired`: Agent's lease expired (another agent may have taken over)
//! - `StaleEpoch`: Write rejected due to stale epoch (fencing token)
//!
//! ### Wrapped Errors
//! - `Metadata`: Metadata store operation failed
//! - `Storage`: Storage layer operation failed
//! - `Io`: I/O operation failed
//! - `Join`: Background task join failed
//!
//! ## Usage
//!
//! All agent operations return `Result<T>` which is aliased to `Result<T, AgentError>`.

use thiserror::Error;

/// Convenience type alias for `Result<T, AgentError>`.
pub type Result<T> = std::result::Result<T, AgentError>;

/// Comprehensive error type for StreamHouse agent operations.
///
/// This enum covers all possible errors that can occur when running an agent.
/// Each variant includes context to help with debugging.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Agent operation called before `start()`.
    ///
    /// ## Causes
    /// - Calling `stop()` before `start()`
    /// - Attempting to write data before agent is started
    ///
    /// ## Resolution
    /// - Call `agent.start().await?` before other operations
    #[error("Agent not started")]
    NotStarted,

    /// Attempted to start an already-running agent.
    ///
    /// ## Causes
    /// - Calling `start()` multiple times without `stop()`
    ///
    /// ## Resolution
    /// - Check agent state before calling `start()`
    /// - Call `stop()` before restarting
    #[error("Agent already started")]
    AlreadyStarted,

    /// Tried to write to a partition owned by another agent.
    ///
    /// ## Causes
    /// - Another agent acquired the lease before this agent
    /// - Lease expired and another agent took over
    /// - Split-brain scenario (network partition)
    ///
    /// ## Resolution
    /// - This is a transient error - the producer should retry with another agent
    /// - The current agent should release the lease and try to acquire a different partition
    #[error("Lease held by another agent: {0}")]
    LeaseHeldByOther(String),

    /// Agent's lease for this partition has expired.
    ///
    /// ## Causes
    /// - Heartbeat failed (agent crashed, network partition, etc.)
    /// - Agent was too slow to renew lease
    /// - Another agent may have taken over
    ///
    /// ## Resolution
    /// - Stop writing to this partition immediately
    /// - Try to re-acquire the lease
    /// - If another agent holds the lease, release it gracefully
    #[error("Lease expired for partition {topic}/{partition}")]
    LeaseExpired { topic: String, partition: u32 },

    /// Write rejected due to stale epoch (fencing token).
    ///
    /// ## Causes
    /// - This agent was the leader but lost the lease
    /// - Another agent became leader and incremented the epoch
    /// - This agent is a "zombie" from a previous leadership term
    ///
    /// ## Resolution
    /// - Stop writing to this partition immediately
    /// - Release the lease
    /// - Re-acquire the lease to get the new epoch
    #[error("Stale epoch: expected {expected}, got {actual}")]
    StaleEpoch { expected: u64, actual: u64 },

    /// Metadata store operation failed.
    ///
    /// ## Causes
    /// - Database connection lost
    /// - Query timeout
    /// - Schema mismatch
    ///
    /// ## Resolution
    /// - Check metadata store logs
    /// - Verify database is running and accessible
    #[error("Metadata error: {0}")]
    Metadata(#[from] streamhouse_metadata::MetadataError),

    /// Storage layer operation failed.
    ///
    /// ## Causes
    /// - S3/MinIO connection failure
    /// - Disk full
    /// - Segment write error
    ///
    /// ## Resolution
    /// - Check storage logs
    /// - Verify S3/MinIO is accessible
    #[error("Storage error: {0}")]
    Storage(String),

    /// I/O operation failed.
    ///
    /// ## Causes
    /// - File system error
    /// - Network error
    ///
    /// ## Resolution
    /// - Check system logs
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Background task join failed.
    ///
    /// ## Causes
    /// - Background task panicked
    /// - Task was cancelled unexpectedly
    ///
    /// ## Resolution
    /// - Check logs for panic messages
    /// - This usually indicates a bug
    #[error("Join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}
