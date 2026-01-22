//! Error Types for StreamHouse
//!
//! This module defines all error types that can occur in StreamHouse operations.
//!
//! ## Error Categories
//!
//! ### I/O Errors
//! - File system operations
//! - Network operations
//! - S3 operations
//!
//! ### Data Integrity Errors
//! - `InvalidMagic`: Segment file doesn't start with expected magic bytes ("STRM")
//! - `CrcMismatch`: Data corruption detected via checksum
//! - `InvalidSegment`: Malformed segment data
//!
//! ### Version/Compatibility Errors
//! - `InvalidVersion`: Segment version number is malformed
//! - `UnsupportedVersion`: Segment was created by a newer version we don't support
//! - `InvalidCompression`: Unknown compression type ID
//!
//! ### Compression Errors
//! - `CompressionError`: Failed to compress data
//! - `Decompression`: Failed to decompress data (likely corruption)
//!
//! ### Query Errors
//! - `OffsetNotFound`: Requested offset doesn't exist in segment
//!
//! ### Feature Errors
//! - `Unsupported`: Feature not yet implemented (e.g., Zstd compression)
//!
//! ## Usage
//! All functions in StreamHouse return `Result<T>` which is aliased to `Result<T, Error>`.
//! This allows using `?` operator for error propagation.
//!
//! ## Example
//! ```ignore
//! use streamhouse_core::{Result, Error};
//!
//! fn read_segment(path: &str) -> Result<Vec<Record>> {
//!     // I/O errors automatically convert via #[from]
//!     let data = std::fs::read(path)?;
//!
//!     // Check magic bytes
//!     if &data[0..4] != b"STRM" {
//!         return Err(Error::InvalidMagic);
//!     }
//!
//!     Ok(vec![])
//! }
//! ```

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid magic bytes")]
    InvalidMagic,

    #[error("Invalid version: {0}")]
    InvalidVersion(u16),

    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u16),

    #[error("Invalid compression type: {0}")]
    InvalidCompression(u16),

    #[error("CRC mismatch")]
    CrcMismatch,

    #[error("Offset not found: {0}")]
    OffsetNotFound(u64),

    #[error("Invalid segment: {0}")]
    InvalidSegment(String),

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Decompression error: {0}")]
    Decompression(String),

    #[error("Unsupported feature: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, Error>;
