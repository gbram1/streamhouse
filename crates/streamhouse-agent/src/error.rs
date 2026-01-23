//! Error types for StreamHouse Agent

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AgentError>;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Agent not started")]
    NotStarted,

    #[error("Agent already started")]
    AlreadyStarted,

    #[error("Lease held by another agent: {0}")]
    LeaseHeldByOther(String),

    #[error("Lease expired for partition {topic}/{partition}")]
    LeaseExpired { topic: String, partition: u32 },

    #[error("Stale epoch: expected {expected}, got {actual}")]
    StaleEpoch { expected: i64, actual: i64 },

    #[error("Metadata error: {0}")]
    Metadata(#[from] streamhouse_metadata::Error),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}
