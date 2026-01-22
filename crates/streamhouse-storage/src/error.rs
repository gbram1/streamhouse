//! Storage Error Types
//!
//! This module defines all error types that can occur during storage operations.
//!
//! ## Error Categories
//!
//! ### Topic/Partition Errors
//! - `TopicNotFound`: Attempted to write to a topic that doesn't exist
//! - `PartitionNotFound`: Partition doesn't exist for the topic
//!
//! ### S3 Errors
//! - `S3UploadFailed`: Failed to upload segment to S3 after retries
//! - `ObjectStoreError`: Low-level object store operation failed
//!
//! ### Segment Errors
//! - `SegmentError`: Error during segment creation or finalization
//!
//! ### Metadata Errors
//! - `MetadataError`: Metadata store operation failed
//!
//! ## Usage
//!
//! All storage operations return `Result<T>` which is aliased to
//! `Result<T, Error>`. This allows clean error propagation with `?`.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    #[error("Partition not found: {topic}/{partition}")]
    PartitionNotFound { topic: String, partition: u32 },

    #[error("S3 upload failed: {0}")]
    S3UploadFailed(String),

    #[error("Metadata error: {0}")]
    MetadataError(#[from] streamhouse_metadata::MetadataError),

    #[error("Object store error: {0}")]
    ObjectStoreError(#[from] object_store::Error),

    #[error("Segment write error: {0}")]
    SegmentError(String),
}
