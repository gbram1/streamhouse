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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Helper: assert Display output contains a substring
    // ---------------------------------------------------------------

    fn assert_display_contains(err: &Error, expected: &str) {
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
    fn test_io_error_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = Error::Io(io_err);
        assert_display_contains(&err, "IO error");
        assert_display_contains(&err, "file missing");
    }

    #[test]
    fn test_invalid_magic_variant() {
        let err = Error::InvalidMagic;
        assert_display_contains(&err, "Invalid magic bytes");
    }

    #[test]
    fn test_invalid_version_variant() {
        let err = Error::InvalidVersion(99);
        assert_display_contains(&err, "Invalid version");
        assert_display_contains(&err, "99");
    }

    #[test]
    fn test_unsupported_version_variant() {
        let err = Error::UnsupportedVersion(5);
        assert_display_contains(&err, "Unsupported version");
        assert_display_contains(&err, "5");
    }

    #[test]
    fn test_invalid_compression_variant() {
        let err = Error::InvalidCompression(42);
        assert_display_contains(&err, "Invalid compression type");
        assert_display_contains(&err, "42");
    }

    #[test]
    fn test_crc_mismatch_variant() {
        let err = Error::CrcMismatch;
        assert_display_contains(&err, "CRC mismatch");
    }

    #[test]
    fn test_offset_not_found_variant() {
        let err = Error::OffsetNotFound(12345);
        assert_display_contains(&err, "Offset not found");
        assert_display_contains(&err, "12345");
    }

    #[test]
    fn test_invalid_segment_variant() {
        let err = Error::InvalidSegment("truncated data".to_string());
        assert_display_contains(&err, "Invalid segment");
        assert_display_contains(&err, "truncated data");
    }

    #[test]
    fn test_compression_error_variant() {
        let err = Error::CompressionError("lz4 failed".to_string());
        assert_display_contains(&err, "Compression error");
        assert_display_contains(&err, "lz4 failed");
    }

    #[test]
    fn test_decompression_variant() {
        let err = Error::Decompression("corrupt block".to_string());
        assert_display_contains(&err, "Decompression error");
        assert_display_contains(&err, "corrupt block");
    }

    #[test]
    fn test_unsupported_variant() {
        let err = Error::Unsupported("zstd not available".to_string());
        assert_display_contains(&err, "Unsupported feature");
        assert_display_contains(&err, "zstd not available");
    }

    // ---------------------------------------------------------------
    // From<io::Error> conversion
    // ---------------------------------------------------------------

    #[test]
    fn test_from_io_error_not_found() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let err: Error = io_err.into();
        assert_display_contains(&err, "IO error");
        assert_display_contains(&err, "no such file");
    }

    #[test]
    fn test_from_io_error_permission_denied() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: Error = Error::from(io_err);
        assert_display_contains(&err, "IO error");
        assert_display_contains(&err, "access denied");
    }

    #[test]
    fn test_from_io_error_connection_refused() {
        let io_err =
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
        let err: Error = io_err.into();
        assert_display_contains(&err, "connection refused");
    }

    #[test]
    fn test_from_io_error_in_result_context() {
        fn fallible() -> Result<()> {
            let io_err = std::io::Error::new(std::io::ErrorKind::Other, "something broke");
            Err(io_err)?
        }
        let result = fallible();
        assert!(result.is_err());
        assert_display_contains(&result.unwrap_err(), "something broke");
    }

    // ---------------------------------------------------------------
    // Display output format validation
    // ---------------------------------------------------------------

    #[test]
    fn test_display_invalid_version_format() {
        let err = Error::InvalidVersion(0);
        let msg = format!("{}", err);
        assert_eq!(msg, "Invalid version: 0");
    }

    #[test]
    fn test_display_unsupported_version_format() {
        let err = Error::UnsupportedVersion(3);
        let msg = format!("{}", err);
        assert_eq!(msg, "Unsupported version: 3");
    }

    #[test]
    fn test_display_invalid_compression_format() {
        let err = Error::InvalidCompression(7);
        let msg = format!("{}", err);
        assert_eq!(msg, "Invalid compression type: 7");
    }

    #[test]
    fn test_display_crc_mismatch_format() {
        let err = Error::CrcMismatch;
        let msg = format!("{}", err);
        assert_eq!(msg, "CRC mismatch");
    }

    #[test]
    fn test_display_invalid_magic_format() {
        let err = Error::InvalidMagic;
        let msg = format!("{}", err);
        assert_eq!(msg, "Invalid magic bytes");
    }

    #[test]
    fn test_display_offset_not_found_format() {
        let err = Error::OffsetNotFound(0);
        let msg = format!("{}", err);
        assert_eq!(msg, "Offset not found: 0");
    }

    #[test]
    fn test_display_invalid_segment_format() {
        let err = Error::InvalidSegment("bad header".to_string());
        let msg = format!("{}", err);
        assert_eq!(msg, "Invalid segment: bad header");
    }

    #[test]
    fn test_display_compression_error_format() {
        let err = Error::CompressionError("out of memory".to_string());
        let msg = format!("{}", err);
        assert_eq!(msg, "Compression error: out of memory");
    }

    #[test]
    fn test_display_decompression_format() {
        let err = Error::Decompression("bad frame".to_string());
        let msg = format!("{}", err);
        assert_eq!(msg, "Decompression error: bad frame");
    }

    #[test]
    fn test_display_unsupported_format() {
        let err = Error::Unsupported("snappy".to_string());
        let msg = format!("{}", err);
        assert_eq!(msg, "Unsupported feature: snappy");
    }

    // ---------------------------------------------------------------
    // Edge cases for numeric variants
    // ---------------------------------------------------------------

    #[test]
    fn test_invalid_version_max_u16() {
        let err = Error::InvalidVersion(u16::MAX);
        assert_display_contains(&err, &u16::MAX.to_string());
    }

    #[test]
    fn test_unsupported_version_max_u16() {
        let err = Error::UnsupportedVersion(u16::MAX);
        assert_display_contains(&err, &u16::MAX.to_string());
    }

    #[test]
    fn test_invalid_compression_max_u16() {
        let err = Error::InvalidCompression(u16::MAX);
        assert_display_contains(&err, &u16::MAX.to_string());
    }

    #[test]
    fn test_offset_not_found_zero() {
        let err = Error::OffsetNotFound(0);
        assert_display_contains(&err, "0");
    }

    #[test]
    fn test_offset_not_found_max_u64() {
        let err = Error::OffsetNotFound(u64::MAX);
        assert_display_contains(&err, &u64::MAX.to_string());
    }

    // ---------------------------------------------------------------
    // Edge cases for string variants
    // ---------------------------------------------------------------

    #[test]
    fn test_invalid_segment_empty_message() {
        let err = Error::InvalidSegment(String::new());
        let msg = format!("{}", err);
        assert_eq!(msg, "Invalid segment: ");
    }

    #[test]
    fn test_compression_error_empty_message() {
        let err = Error::CompressionError(String::new());
        let msg = format!("{}", err);
        assert_eq!(msg, "Compression error: ");
    }

    #[test]
    fn test_decompression_empty_message() {
        let err = Error::Decompression(String::new());
        let msg = format!("{}", err);
        assert_eq!(msg, "Decompression error: ");
    }

    #[test]
    fn test_unsupported_empty_message() {
        let err = Error::Unsupported(String::new());
        let msg = format!("{}", err);
        assert_eq!(msg, "Unsupported feature: ");
    }

    #[test]
    fn test_invalid_segment_long_message() {
        let long_msg = "x".repeat(10_000);
        let err = Error::InvalidSegment(long_msg.clone());
        assert_display_contains(&err, &long_msg);
    }

    // ---------------------------------------------------------------
    // Debug trait
    // ---------------------------------------------------------------

    #[test]
    fn test_debug_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let err = Error::Io(io_err);
        let debug = format!("{:?}", err);
        assert!(debug.contains("Io"));
    }

    #[test]
    fn test_debug_invalid_magic() {
        let err = Error::InvalidMagic;
        let debug = format!("{:?}", err);
        assert!(debug.contains("InvalidMagic"));
    }

    #[test]
    fn test_debug_invalid_version() {
        let err = Error::InvalidVersion(1);
        let debug = format!("{:?}", err);
        assert!(debug.contains("InvalidVersion"));
    }

    #[test]
    fn test_debug_unsupported_version() {
        let err = Error::UnsupportedVersion(2);
        let debug = format!("{:?}", err);
        assert!(debug.contains("UnsupportedVersion"));
    }

    #[test]
    fn test_debug_invalid_compression() {
        let err = Error::InvalidCompression(3);
        let debug = format!("{:?}", err);
        assert!(debug.contains("InvalidCompression"));
    }

    #[test]
    fn test_debug_crc_mismatch() {
        let err = Error::CrcMismatch;
        let debug = format!("{:?}", err);
        assert!(debug.contains("CrcMismatch"));
    }

    #[test]
    fn test_debug_offset_not_found() {
        let err = Error::OffsetNotFound(42);
        let debug = format!("{:?}", err);
        assert!(debug.contains("OffsetNotFound"));
    }

    #[test]
    fn test_debug_invalid_segment() {
        let err = Error::InvalidSegment("reason".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("InvalidSegment"));
    }

    #[test]
    fn test_debug_compression_error() {
        let err = Error::CompressionError("reason".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("CompressionError"));
    }

    #[test]
    fn test_debug_decompression() {
        let err = Error::Decompression("reason".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Decompression"));
    }

    #[test]
    fn test_debug_unsupported() {
        let err = Error::Unsupported("reason".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Unsupported"));
    }

    // ---------------------------------------------------------------
    // Error is std::error::Error (via thiserror)
    // ---------------------------------------------------------------

    #[test]
    fn test_error_is_std_error() {
        fn assert_std_error<E: std::error::Error>(_e: &E) {}
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let err = Error::Io(io_err);
        assert_std_error(&err);
    }

    #[test]
    fn test_io_error_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "inner");
        let err = Error::Io(io_err);
        // thiserror provides source() for #[from] variants
        let source = std::error::Error::source(&err);
        assert!(source.is_some());
    }

    #[test]
    fn test_non_io_errors_have_no_source() {
        let variants: Vec<Error> = vec![
            Error::InvalidMagic,
            Error::InvalidVersion(1),
            Error::UnsupportedVersion(1),
            Error::InvalidCompression(1),
            Error::CrcMismatch,
            Error::OffsetNotFound(1),
            Error::InvalidSegment("s".to_string()),
            Error::CompressionError("s".to_string()),
            Error::Decompression("s".to_string()),
            Error::Unsupported("s".to_string()),
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
    // Result type alias
    // ---------------------------------------------------------------

    #[test]
    fn test_result_ok() {
        let result: Result<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_err() {
        let result: Result<i32> = Err(Error::CrcMismatch);
        assert!(result.is_err());
    }

    #[test]
    fn test_result_question_mark_propagation() {
        fn inner() -> Result<()> {
            Err(Error::InvalidMagic)?;
            Ok(())
        }
        assert!(inner().is_err());
    }
}
