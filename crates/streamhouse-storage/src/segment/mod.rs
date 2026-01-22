//! Segment Storage Format
//!
//! This module implements the binary file format for storing records in S3-compatible storage.
//!
//! ## Segment File Structure
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ Header (64 bytes)                                           │
//! │ - Magic bytes: "STRM" (4 bytes)                            │
//! │ - Version: 1 (2 bytes)                                      │
//! │ - Compression: None/Lz4/Zstd (2 bytes)                     │
//! │ - Base offset (8 bytes)                                     │
//! │ - End offset (8 bytes)                                      │
//! │ - Record count (4 bytes)                                    │
//! │ - Block count (4 bytes)                                     │
//! │ - Reserved (32 bytes)                                       │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Block 1 (compressed, ~1MB target)                           │
//! │ - Delta-encoded records with varint compression             │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Block 2 (compressed, ~1MB target)                           │
//! ├─────────────────────────────────────────────────────────────┤
//! │ ...                                                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Index                                                       │
//! │ - Entry count (4 bytes)                                     │
//! │ - For each block:                                           │
//! │   * First offset in block (8 bytes)                         │
//! │   * File position (8 bytes)                                 │
//! │   * First timestamp in block (8 bytes)                      │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Footer (32 bytes)                                           │
//! │ - Index position (8 bytes)                                  │
//! │ - CRC32 checksum (4 bytes)                                  │
//! │ - Magic bytes: "STRM" again (4 bytes)                      │
//! │ - Reserved (16 bytes)                                       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Block Format (Uncompressed)
//!
//! Each block contains delta-encoded records:
//! ```text
//! Record 1:
//!   - Offset delta (varint, signed) - difference from last offset
//!   - Timestamp delta (varint, signed) - difference from last timestamp
//!   - Key length (varint, unsigned)
//!   - Key bytes (if key_length > 0)
//!   - Value length (varint, unsigned)
//!   - Value bytes
//! Record 2:
//!   ...
//! ```
//!
//! ## Why This Design?
//!
//! ### Block-based Compression
//! - Blocks are ~1MB for good compression ratios
//! - Independent blocks allow parallel decompression
//! - Failed decompression only affects one block
//!
//! ### Delta Encoding + Varints
//! - Offsets are sequential (1, 2, 3...) so deltas are tiny (1, 1, 1...)
//! - Timestamps are close together, so deltas are small
//! - Varints make small numbers use 1 byte instead of 8
//! - Combined savings: ~5-7 bytes per record on metadata alone
//!
//! ### Index for Fast Seeks
//! - Binary search to find the right block for an offset
//! - No need to decompress earlier blocks
//! - Enables efficient "read from offset X" queries
//!
//! ### CRC32 Checksum
//! - Detects data corruption from S3/network
//! - Covers entire file except the footer itself
//!
//! ## Performance Characteristics
//!
//! - **Write**: 2.26M records/sec with LZ4 compression
//! - **Read**: 3.10M records/sec with LZ4 decompression
//! - **Seek**: 640µs to read from 90% offset (skips first 90%)
//! - **Compression**: ~80%+ space savings with LZ4
//!
//! ## Usage
//!
//! ### Writing a Segment
//! ```ignore
//! let mut writer = SegmentWriter::new(Compression::Lz4);
//!
//! for i in 0..10_000 {
//!     let record = Record::new(i, timestamp, key, value);
//!     writer.append(&record)?;
//! }
//!
//! let segment_bytes = writer.finish()?;
//! // Upload segment_bytes to S3
//! ```
//!
//! ### Reading a Segment
//! ```ignore
//! // Download from S3
//! let segment_bytes = s3_client.get_object(bucket, key).await?;
//!
//! let reader = SegmentReader::new(segment_bytes)?;
//!
//! // Read all records
//! let all_records = reader.read_all()?;
//!
//! // Or read from specific offset
//! let records = reader.read_from_offset(5000)?;
//! ```

mod writer;
mod reader;

pub use writer::SegmentWriter;
pub use reader::SegmentReader;

/// Magic bytes for segment files: "STRM"
pub const SEGMENT_MAGIC: [u8; 4] = [0x53, 0x54, 0x52, 0x4D];

/// Version number for the segment format
pub const SEGMENT_VERSION: u16 = 1;

/// Target block size (~1MB)
pub const BLOCK_SIZE_TARGET: usize = 1024 * 1024;

/// Segment header size (64 bytes)
pub const HEADER_SIZE: usize = 64;

/// Segment footer size (32 bytes)
pub const FOOTER_SIZE: usize = 32;
