//! SQL error types

use thiserror::Error;

/// SQL execution errors
#[derive(Debug, Error)]
pub enum SqlError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    #[error("Materialized view not found: {0}")]
    ViewNotFound(String),

    #[error("Materialized view already exists: {0}")]
    ViewAlreadyExists(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Timeout: query exceeded {0}ms limit")]
    Timeout(u64),

    #[error("Result too large: {0} rows exceeds limit of {1}")]
    ResultTooLarge(usize, usize),

    #[error("Metadata error: {0}")]
    MetadataError(#[from] streamhouse_metadata::MetadataError),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Arrow error: {0}")]
    ArrowError(String),

    #[error("DataFusion error: {0}")]
    DataFusionError(String),
}
