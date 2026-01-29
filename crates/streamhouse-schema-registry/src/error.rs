//! Schema Registry Error Types

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SchemaError>;

#[derive(Error, Debug)]
pub enum SchemaError {
    #[error("Schema not found: {subject} version {version}")]
    SchemaNotFound { subject: String, version: i32 },

    #[error("Subject not found: {0}")]
    SubjectNotFound(String),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),

    #[error("Schema compatibility error: {0}")]
    IncompatibleSchema(String),

    #[error("Schema already exists with ID {0}")]
    SchemaAlreadyExists(i32),

    #[error("Invalid schema format: {0}")]
    InvalidFormat(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}
