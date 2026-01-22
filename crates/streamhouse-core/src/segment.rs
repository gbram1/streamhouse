//! Segment Metadata and Configuration
//!
//! This module defines metadata structures for segments - the storage unit in StreamHouse.
//!
//! ## What is a Segment?
//! A segment is a immutable file stored in S3 containing a batch of records (similar to Kafka segments).
//! Think of it as a "chunk" of a partition that gets uploaded to S3 once it's full.
//!
//! ## Segment Lifecycle
//! 1. Records accumulate in memory (SegmentWriter)
//! 2. When segment reaches ~64-256MB, it's finalized
//! 3. Segment is uploaded to S3 as an immutable file
//! 4. Metadata is stored in SQLite for querying
//! 5. Readers can fetch and read segments from S3
//!
//! ## SegmentInfo
//! Contains all metadata needed to locate and understand a segment:
//! - Which topic/partition it belongs to
//! - Offset range it covers (base_offset to end_offset)
//! - Where it's stored in S3 (bucket + key)
//! - Size and record count
//! - When it was created
//!
//! ## Compression Types
//! - **None**: No compression (faster, but larger)
//! - **LZ4**: Fast compression with good ratio (~2x faster writes, 80%+ space savings)
//! - **Zstd**: Better compression but slower (not yet implemented)
//!
//! ## Example
//! ```ignore
//! let segment_info = SegmentInfo {
//!     id: "seg_123".to_string(),
//!     topic: "clickstream".to_string(),
//!     partition_id: 0,
//!     base_offset: 1000,
//!     end_offset: 1999,
//!     record_count: 1000,
//!     size_bytes: 5_242_880,  // ~5MB
//!     s3_bucket: "streamhouse-data".to_string(),
//!     s3_key: "clickstream/0/seg_123.bin".to_string(),
//!     created_at: 1234567890,
//! };
//! ```

use serde::{Deserialize, Serialize};

/// Information about a segment stored in S3
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentInfo {
    /// Unique segment ID
    pub id: String,

    /// Topic name
    pub topic: String,

    /// Partition ID
    pub partition_id: u32,

    /// First offset in this segment
    pub base_offset: u64,

    /// Last offset in this segment
    pub end_offset: u64,

    /// Number of records in segment
    pub record_count: u32,

    /// Size in bytes
    pub size_bytes: u64,

    /// S3 bucket name
    pub s3_bucket: String,

    /// S3 object key
    pub s3_key: String,

    /// Creation timestamp
    pub created_at: i64,
}

/// Compression type for segments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Compression {
    None = 0,
    Lz4 = 1,
    Zstd = 2,
}

impl TryFrom<u16> for Compression {
    type Error = crate::Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Compression::None),
            1 => Ok(Compression::Lz4),
            2 => Ok(Compression::Zstd),
            _ => Err(crate::Error::InvalidCompression(value)),
        }
    }
}
