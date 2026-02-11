//! Error types for the StreamHouse connectors framework.
//!
//! Provides a unified error type for all connector operations including
//! configuration parsing, I/O, serialization, connectivity, and runtime errors.

use thiserror::Error;

/// Errors that can occur during connector operations.
#[derive(Debug, Error)]
pub enum ConnectorError {
    /// Invalid or missing configuration.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// I/O error (file, network, etc).
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization or deserialization failure.
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Failed to connect to an external system.
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Error in a sink connector.
    #[error("Sink error: {0}")]
    SinkError(String),

    /// Error in a source connector.
    #[error("Source error: {0}")]
    SourceError(String),

    /// Error in the connector runtime.
    #[error("Runtime error: {0}")]
    RuntimeError(String),
}

/// Result type alias for connector operations.
pub type Result<T> = std::result::Result<T, ConnectorError>;

impl From<serde_json::Error> for ConnectorError {
    fn from(e: serde_json::Error) -> Self {
        ConnectorError::SerializationError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_display_contains(err: &ConnectorError, expected: &str) {
        let msg = format!("{}", err);
        assert!(
            msg.contains(expected),
            "Expected display '{}' to contain '{}'",
            msg,
            expected
        );
    }

    // ---------------------------------------------------------------
    // Construction of every variant
    // ---------------------------------------------------------------

    #[test]
    fn test_config_error() {
        let err = ConnectorError::ConfigError("missing field 'name'".to_string());
        assert_display_contains(&err, "Configuration error");
        assert_display_contains(&err, "missing field 'name'");
    }

    #[test]
    fn test_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = ConnectorError::IoError(io_err);
        assert_display_contains(&err, "I/O error");
        assert_display_contains(&err, "file missing");
    }

    #[test]
    fn test_serialization_error() {
        let err = ConnectorError::SerializationError("invalid JSON".to_string());
        assert_display_contains(&err, "Serialization error");
        assert_display_contains(&err, "invalid JSON");
    }

    #[test]
    fn test_connection_error() {
        let err = ConnectorError::ConnectionError("connection refused".to_string());
        assert_display_contains(&err, "Connection error");
        assert_display_contains(&err, "connection refused");
    }

    #[test]
    fn test_sink_error() {
        let err = ConnectorError::SinkError("write failed".to_string());
        assert_display_contains(&err, "Sink error");
        assert_display_contains(&err, "write failed");
    }

    #[test]
    fn test_source_error() {
        let err = ConnectorError::SourceError("read failed".to_string());
        assert_display_contains(&err, "Source error");
        assert_display_contains(&err, "read failed");
    }

    #[test]
    fn test_runtime_error() {
        let err = ConnectorError::RuntimeError("task panicked".to_string());
        assert_display_contains(&err, "Runtime error");
        assert_display_contains(&err, "task panicked");
    }

    // ---------------------------------------------------------------
    // From conversions
    // ---------------------------------------------------------------

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: ConnectorError = io_err.into();
        assert_display_contains(&err, "I/O error");
        assert_display_contains(&err, "access denied");
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: ConnectorError = json_err.into();
        assert_display_contains(&err, "Serialization error");
    }

    // ---------------------------------------------------------------
    // Result alias
    // ---------------------------------------------------------------

    #[test]
    fn test_result_ok() {
        let result: Result<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_err() {
        let result: Result<i32> = Err(ConnectorError::ConfigError("bad".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_question_mark_propagation() {
        fn inner() -> Result<()> {
            Err(ConnectorError::RuntimeError("boom".to_string()))?;
            Ok(())
        }
        assert!(inner().is_err());
    }

    // ---------------------------------------------------------------
    // Debug trait
    // ---------------------------------------------------------------

    #[test]
    fn test_debug_config_error() {
        let err = ConnectorError::ConfigError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("ConfigError"));
    }

    #[test]
    fn test_debug_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let err = ConnectorError::IoError(io_err);
        let debug = format!("{:?}", err);
        assert!(debug.contains("IoError"));
    }

    #[test]
    fn test_debug_serialization_error() {
        let err = ConnectorError::SerializationError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("SerializationError"));
    }

    #[test]
    fn test_debug_connection_error() {
        let err = ConnectorError::ConnectionError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("ConnectionError"));
    }

    #[test]
    fn test_debug_sink_error() {
        let err = ConnectorError::SinkError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("SinkError"));
    }

    #[test]
    fn test_debug_source_error() {
        let err = ConnectorError::SourceError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("SourceError"));
    }

    #[test]
    fn test_debug_runtime_error() {
        let err = ConnectorError::RuntimeError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("RuntimeError"));
    }

    // ---------------------------------------------------------------
    // Error is std::error::Error
    // ---------------------------------------------------------------

    #[test]
    fn test_error_is_std_error() {
        fn assert_std_error<E: std::error::Error>(_e: &E) {}
        let err = ConnectorError::SinkError("test".to_string());
        assert_std_error(&err);
    }

    #[test]
    fn test_io_error_has_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "inner");
        let err = ConnectorError::IoError(io_err);
        let source = std::error::Error::source(&err);
        assert!(source.is_some());
    }

    #[test]
    fn test_non_io_errors_have_no_source() {
        let variants: Vec<ConnectorError> = vec![
            ConnectorError::ConfigError("s".to_string()),
            ConnectorError::SerializationError("s".to_string()),
            ConnectorError::ConnectionError("s".to_string()),
            ConnectorError::SinkError("s".to_string()),
            ConnectorError::SourceError("s".to_string()),
            ConnectorError::RuntimeError("s".to_string()),
        ];
        for err in &variants {
            assert!(
                std::error::Error::source(err).is_none(),
                "Expected no source for {:?}",
                err
            );
        }
    }

    // ---------------------------------------------------------------
    // Edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_empty_message() {
        let err = ConnectorError::ConfigError(String::new());
        let msg = format!("{}", err);
        assert_eq!(msg, "Configuration error: ");
    }

    #[test]
    fn test_long_message() {
        let long = "x".repeat(10_000);
        let err = ConnectorError::SinkError(long.clone());
        assert_display_contains(&err, &long);
    }
}
