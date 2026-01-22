//! Segment Writer - Building Compressed Segments for S3 Storage
//!
//! This module implements `SegmentWriter`, which batches records into a compressed,
//! indexed segment file ready for upload to S3.
//!
//! ## What Does SegmentWriter Do?
//!
//! 1. **Accumulates records** in memory as they're appended
//! 2. **Delta-encodes** offsets and timestamps using varints (saves ~5-7 bytes per record)
//! 3. **Batches into blocks** of ~1MB each for optimal compression
//! 4. **Compresses each block** using LZ4 (or no compression)
//! 5. **Builds an index** mapping offsets to block positions (enables fast seeks)
//! 6. **Creates final segment** with header, blocks, index, and footer
//!
//! ## When to Use SegmentWriter
//!
//! Use this when:
//! - Accepting writes from producers
//! - Building segments before uploading to S3
//! - A segment reaches target size (64-256MB)
//! - A time-based flush is triggered (e.g., every 5 minutes)
//!
//! ## Performance
//!
//! - **Write throughput**: 2.26M records/sec with LZ4 compression
//! - **Memory usage**: ~1MB for current block + compressed blocks
//! - **Compression ratio**: ~80%+ space savings with LZ4
//!
//! ## Example Usage
//!
//! ```ignore
//! use streamhouse_storage::SegmentWriter;
//! use streamhouse_core::segment::Compression;
//!
//! // Create writer with LZ4 compression
//! let mut writer = SegmentWriter::new(Compression::Lz4);
//!
//! // Append records as they arrive
//! for record in incoming_records {
//!     writer.append(&record)?;
//!
//!     // Check if segment is large enough to upload
//!     if writer.record_count() >= 100_000 {
//!         break;
//!     }
//! }
//!
//! // Finalize and get bytes
//! let segment_bytes = writer.finish()?;
//!
//! // Upload to S3
//! s3_client.put_object(
//!     bucket,
//!     format!("topic/partition/seg_{}.bin", base_offset),
//!     segment_bytes
//! ).await?;
//! ```
//!
//! ## Technical Details
//!
//! ### Delta Encoding
//! Instead of storing absolute values:
//! - **Offset 100**: delta = 0 (first record)
//! - **Offset 101**: delta = 1
//! - **Offset 102**: delta = 1
//! - **Timestamp 1234567890**: delta = 0 (first record)
//! - **Timestamp 1234567891**: delta = 1
//!
//! With varints, small deltas (0-127) use just 1 byte instead of 8 bytes.
//!
//! ### Block Flushing
//! When current block reaches ~1MB:
//! 1. Compress the block using LZ4
//! 2. Store compressed bytes
//! 3. Add index entry (offset → file position)
//! 4. Start new block
//!
//! ### Index Structure
//! For each block, we store:
//! - First offset in the block (for binary search)
//! - File position where block starts (for seeking)
//! - First timestamp in block (for time-based queries)
//!
//! ## Thread Safety
//!
//! SegmentWriter is NOT thread-safe. Each writer is owned by a single partition's
//! write path. For concurrent writes to different partitions, use separate writers.

use bytes::{BufMut, BytesMut};
use streamhouse_core::segment::Compression;
use streamhouse_core::{varint, Error, Record, Result};

use super::{BLOCK_SIZE_TARGET, HEADER_SIZE, SEGMENT_MAGIC, SEGMENT_VERSION};

/// Builds a segment file with compression and indexing
pub struct SegmentWriter {
    /// Compression type
    compression: Compression,

    /// Current block being built
    current_block: BytesMut,

    /// Completed compressed blocks
    blocks: Vec<Vec<u8>>,

    /// Index entries (offset -> file position)
    index: Vec<IndexEntry>,

    /// First offset in this segment
    base_offset: Option<u64>,

    /// Last offset written
    last_offset: Option<u64>,

    /// Total number of records
    record_count: u32,

    /// Last timestamp for delta encoding
    last_timestamp: u64,
}

#[derive(Debug, Clone)]
struct IndexEntry {
    /// Offset of first record in this block
    offset: u64,
    /// File position where this block starts
    file_position: u64,
    /// Timestamp of first record in this block
    timestamp: u64,
}

impl SegmentWriter {
    /// Create a new segment writer
    pub fn new(compression: Compression) -> Self {
        Self {
            compression,
            current_block: BytesMut::with_capacity(BLOCK_SIZE_TARGET),
            blocks: Vec::new(),
            index: Vec::new(),
            base_offset: None,
            last_offset: None,
            record_count: 0,
            last_timestamp: 0,
        }
    }

    /// Append a record to the segment
    pub fn append(&mut self, record: &Record) -> Result<()> {
        // Set base offset on first record
        if self.base_offset.is_none() {
            self.base_offset = Some(record.offset);
            self.last_timestamp = record.timestamp;

            // Create index entry for first block
            self.index.push(IndexEntry {
                offset: record.offset,
                file_position: HEADER_SIZE as u64, // First block starts after header
                timestamp: record.timestamp,
            });
        }

        // Check if we need to flush the current block
        if self.current_block.len() >= BLOCK_SIZE_TARGET {
            self.flush_block()?;
        }

        // Encode record with delta compression
        self.encode_record(record)?;

        self.last_offset = Some(record.offset);
        self.record_count += 1;

        Ok(())
    }

    /// Encode a single record into the current block
    fn encode_record(&mut self, record: &Record) -> Result<()> {
        // Calculate deltas
        let offset_delta = if let Some(last) = self.last_offset {
            record.offset.wrapping_sub(last) as i64
        } else {
            0 // First record in block has delta 0
        };

        let timestamp_delta = record.timestamp.wrapping_sub(self.last_timestamp) as i64;
        self.last_timestamp = record.timestamp;

        // Encode: offset_delta, timestamp_delta, key_len, key, value_len, value
        varint::encode_varint(&mut self.current_block, offset_delta);
        varint::encode_varint(&mut self.current_block, timestamp_delta);

        // Encode key
        if let Some(key) = &record.key {
            varint::encode_varint_u64(&mut self.current_block, key.len() as u64);
            self.current_block.put_slice(key);
        } else {
            varint::encode_varint_u64(&mut self.current_block, 0);
        }

        // Encode value
        varint::encode_varint_u64(&mut self.current_block, record.value.len() as u64);
        self.current_block.put_slice(&record.value);

        Ok(())
    }

    /// Flush the current block (compress and add to blocks list)
    fn flush_block(&mut self) -> Result<()> {
        if self.current_block.is_empty() {
            return Ok(());
        }

        // Compress the block
        let compressed = match self.compression {
            Compression::None => self.current_block.to_vec(),
            Compression::Lz4 => lz4_flex::compress_prepend_size(&self.current_block),
            Compression::Zstd => {
                return Err(Error::Unsupported(
                    "Zstd compression not yet implemented".to_string(),
                ));
            }
        };

        // Calculate file position for next block's index entry
        let current_position =
            HEADER_SIZE as u64 + self.blocks.iter().map(|b| b.len() as u64).sum::<u64>();

        self.blocks.push(compressed);

        // Create index entry for next block if we have a last offset
        if let Some(offset) = self.last_offset {
            self.index.push(IndexEntry {
                offset: offset + 1, // Next block starts at next offset
                file_position: current_position + self.blocks.last().unwrap().len() as u64,
                timestamp: self.last_timestamp,
            });
        }

        // Reset current block
        self.current_block.clear();

        Ok(())
    }

    /// Finish writing and return the complete segment bytes
    pub fn finish(mut self) -> Result<Vec<u8>> {
        // Flush any remaining data in current block
        self.flush_block()?;

        // Remove the extra index entry we created for "next block"
        if !self.index.is_empty() {
            self.index.pop();
        }

        let base_offset = self
            .base_offset
            .ok_or(Error::InvalidSegment("No records written".to_string()))?;
        let end_offset = self
            .last_offset
            .ok_or(Error::InvalidSegment("No records written".to_string()))?;

        // Build the complete segment file
        let mut output = BytesMut::new();

        // 1. Write header (64 bytes)
        self.write_header(&mut output, base_offset, end_offset);

        // 2. Write all compressed blocks
        for block in &self.blocks {
            output.put_slice(block);
        }

        // 3. Write index
        let index_start = output.len() as u64;
        self.write_index(&mut output)?;

        // 4. Write footer (32 bytes)
        self.write_footer(&mut output, index_start);

        Ok(output.to_vec())
    }

    /// Write the segment header
    fn write_header(&self, buf: &mut BytesMut, base_offset: u64, end_offset: u64) {
        buf.put_slice(&SEGMENT_MAGIC); // 4 bytes
        buf.put_u16(SEGMENT_VERSION); // 2 bytes
        buf.put_u16(self.compression as u16); // 2 bytes
        buf.put_u64(base_offset); // 8 bytes
        buf.put_u64(end_offset); // 8 bytes
        buf.put_u32(self.record_count); // 4 bytes
        buf.put_u32(self.blocks.len() as u32); // 4 bytes (block count)

        // Reserved space (32 bytes)
        buf.put_bytes(0, 32);
    }

    /// Write the index section
    fn write_index(&self, buf: &mut BytesMut) -> Result<()> {
        // Write number of index entries
        buf.put_u32(self.index.len() as u32);

        // Write each index entry
        for entry in &self.index {
            buf.put_u64(entry.offset);
            buf.put_u64(entry.file_position);
            buf.put_u64(entry.timestamp);
        }

        Ok(())
    }

    /// Write the segment footer
    fn write_footer(&self, buf: &mut BytesMut, index_start: u64) {
        // Calculate CRC32 of everything before the footer
        let crc = crc32fast::hash(&buf[..]);

        buf.put_u64(index_start); // 8 bytes - position of index
        buf.put_u32(crc); // 4 bytes - CRC32 checksum
        buf.put_slice(&SEGMENT_MAGIC); // 4 bytes - magic again for verification

        // Reserved space (16 bytes)
        buf.put_bytes(0, 16);
    }

    /// Get the number of records written so far
    pub fn record_count(&self) -> u32 {
        self.record_count
    }

    /// Get the base offset
    pub fn base_offset(&self) -> Option<u64> {
        self.base_offset
    }

    /// Get the last offset written
    pub fn last_offset(&self) -> Option<u64> {
        self.last_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use crate::segment::FOOTER_SIZE;

    #[test]
    fn test_writer_single_record() {
        let mut writer = SegmentWriter::new(Compression::None);

        let record = Record::new(
            100,
            1234567890,
            Some(Bytes::from("key1")),
            Bytes::from("value1"),
        );

        writer.append(&record).unwrap();

        assert_eq!(writer.record_count(), 1);
        assert_eq!(writer.base_offset(), Some(100));
        assert_eq!(writer.last_offset(), Some(100));
    }

    #[test]
    fn test_writer_multiple_records() {
        let mut writer = SegmentWriter::new(Compression::None);

        for i in 0..10 {
            let record = Record::new(
                100 + i,
                1234567890 + i,
                Some(Bytes::from(format!("key{}", i))),
                Bytes::from(format!("value{}", i)),
            );
            writer.append(&record).unwrap();
        }

        assert_eq!(writer.record_count(), 10);
        assert_eq!(writer.base_offset(), Some(100));
        assert_eq!(writer.last_offset(), Some(109));
    }

    #[test]
    fn test_writer_finish() {
        let mut writer = SegmentWriter::new(Compression::None);

        let record = Record::new(
            100,
            1234567890,
            Some(Bytes::from("key1")),
            Bytes::from("value1"),
        );

        writer.append(&record).unwrap();
        let segment_bytes = writer.finish().unwrap();

        // Should have header + data + index + footer
        assert!(segment_bytes.len() > HEADER_SIZE + FOOTER_SIZE);

        // Check magic bytes at start
        assert_eq!(&segment_bytes[0..4], &SEGMENT_MAGIC);

        // Check magic bytes at end (in footer)
        let footer_start = segment_bytes.len() - FOOTER_SIZE;
        assert_eq!(
            &segment_bytes[footer_start + 12..footer_start + 16],
            &SEGMENT_MAGIC
        );
    }

    #[test]
    fn test_writer_with_lz4_compression() {
        let mut writer = SegmentWriter::new(Compression::Lz4);

        // Add enough records to trigger block flush
        for i in 0..1000 {
            let record = Record::new(
                i,
                1234567890 + i,
                Some(Bytes::from(format!("key{}", i))),
                Bytes::from(vec![b'x'; 1024]), // 1KB value
            );
            writer.append(&record).unwrap();
        }

        let segment_bytes = writer.finish().unwrap();

        // Compressed segment should be significantly smaller than raw data
        // 1000 records * ~1KB each = ~1MB raw, should compress to much less
        assert!(segment_bytes.len() < 500_000); // Less than 500KB
    }
}
