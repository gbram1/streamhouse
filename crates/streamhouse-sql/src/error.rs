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

    #[error("Stream processing error: {0}")]
    StreamProcessingError(String),

    #[error("Checkpoint error: {0}")]
    CheckpointError(String),

    #[error("State store error: {0}")]
    StateStoreError(String),

    #[error("Watermark error: {0}")]
    WatermarkError(String),

    #[error("CDC error: {0}")]
    CdcError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_display() {
        let err = SqlError::ParseError("unexpected token".to_string());
        assert_eq!(err.to_string(), "Parse error: unexpected token");
    }

    #[test]
    fn test_topic_not_found_display() {
        let err = SqlError::TopicNotFound("orders".to_string());
        assert_eq!(err.to_string(), "Topic not found: orders");
    }

    #[test]
    fn test_view_not_found_display() {
        let err = SqlError::ViewNotFound("my_view".to_string());
        assert_eq!(err.to_string(), "Materialized view not found: my_view");
    }

    #[test]
    fn test_view_already_exists_display() {
        let err = SqlError::ViewAlreadyExists("my_view".to_string());
        assert_eq!(err.to_string(), "Materialized view already exists: my_view");
    }

    #[test]
    fn test_invalid_query_display() {
        let err = SqlError::InvalidQuery("missing FROM clause".to_string());
        assert_eq!(err.to_string(), "Invalid query: missing FROM clause");
    }

    #[test]
    fn test_unsupported_operation_display() {
        let err = SqlError::UnsupportedOperation("ALTER TABLE".to_string());
        assert_eq!(err.to_string(), "Unsupported operation: ALTER TABLE");
    }

    #[test]
    fn test_execution_error_display() {
        let err = SqlError::ExecutionError("division by zero".to_string());
        assert_eq!(err.to_string(), "Execution error: division by zero");
    }

    #[test]
    fn test_timeout_display() {
        let err = SqlError::Timeout(5000);
        assert_eq!(err.to_string(), "Timeout: query exceeded 5000ms limit");
    }

    #[test]
    fn test_result_too_large_display() {
        let err = SqlError::ResultTooLarge(50000, 10000);
        assert_eq!(
            err.to_string(),
            "Result too large: 50000 rows exceeds limit of 10000"
        );
    }

    #[test]
    fn test_storage_error_display() {
        let err = SqlError::StorageError("disk full".to_string());
        assert_eq!(err.to_string(), "Storage error: disk full");
    }

    #[test]
    fn test_arrow_error_display() {
        let err = SqlError::ArrowError("schema mismatch".to_string());
        assert_eq!(err.to_string(), "Arrow error: schema mismatch");
    }

    #[test]
    fn test_datafusion_error_display() {
        let err = SqlError::DataFusionError("plan error".to_string());
        assert_eq!(err.to_string(), "DataFusion error: plan error");
    }

    #[test]
    fn test_json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let sql_err: SqlError = json_err.into();
        assert!(matches!(sql_err, SqlError::JsonError(_)));
        assert!(sql_err.to_string().contains("JSON error:"));
    }
}
