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

        // If starting a new block (after flush), create index entry and reset delta state
        if self.current_block.is_empty() && self.base_offset.is_some() && self.index.len() < self.blocks.len() + 1 {
            let file_position =
                HEADER_SIZE as u64 + self.blocks.iter().map(|b| b.len() as u64).sum::<u64>();
            self.index.push(IndexEntry {
                offset: record.offset,
                file_position,
                timestamp: record.timestamp,
            });
            // Reset delta encoding state so first record in new block has delta 0
            self.last_offset = None;
            self.last_timestamp = record.timestamp;
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

        self.blocks.push(compressed);

        // Reset current block - index entry for next block created in append()
        self.current_block.clear();

        Ok(())
    }

    /// Finish writing and return the complete segment bytes
    pub fn finish(mut self) -> Result<Vec<u8>> {
        // Flush any remaining data in current block
        self.flush_block()?;

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
    use crate::segment::FOOTER_SIZE;
    use bytes::Bytes;

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

    // ---------------------------------------------------------------
    // Empty segment edge case
    // ---------------------------------------------------------------

    #[test]
    fn test_writer_empty_segment_finish_fails() {
        let writer = SegmentWriter::new(Compression::None);
        // Finishing a segment with no records should error
        let result = writer.finish();
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // Roundtrip: write then read back with SegmentReader
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_none_compression_single() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        let record = Record::new(
            42,
            9_999_999,
            Some(Bytes::from("the-key")),
            Bytes::from("the-value"),
        );
        writer.append(&record).unwrap();
        let data = writer.finish().unwrap();

        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        assert_eq!(reader.record_count(), 1);
        assert_eq!(reader.base_offset(), 42);
        assert_eq!(reader.end_offset(), 42);

        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0], record);
    }

    #[test]
    fn test_roundtrip_lz4_compression_single() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::Lz4);
        let record = Record::new(
            0,
            1_000_000,
            Some(Bytes::from("k")),
            Bytes::from("v"),
        );
        writer.append(&record).unwrap();
        let data = writer.finish().unwrap();

        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0], record);
    }

    #[test]
    fn test_roundtrip_none_compression_many_records() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        let mut originals = Vec::new();
        for i in 0..150 {
            let record = Record::new(
                1000 + i,
                5_000_000 + i * 100,
                Some(Bytes::from(format!("key-{}", i))),
                Bytes::from(format!("value-payload-{}", i)),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }
        assert_eq!(writer.record_count(), 150);
        assert_eq!(writer.base_offset(), Some(1000));
        assert_eq!(writer.last_offset(), Some(1149));

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        assert_eq!(reader.record_count(), 150);
        assert_eq!(reader.base_offset(), 1000);
        assert_eq!(reader.end_offset(), 1149);

        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 150);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i], "mismatch at index {}", i);
        }
    }

    #[test]
    fn test_roundtrip_lz4_compression_many_records() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::Lz4);
        let mut originals = Vec::new();
        for i in 0..200 {
            let record = Record::new(
                i,
                1_000_000 + i * 10,
                Some(Bytes::from(format!("k{}", i))),
                Bytes::from(vec![b'A' + (i % 26) as u8; 512]),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 200);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i], "mismatch at index {}", i);
        }
    }

    // ---------------------------------------------------------------
    // read_from_offset roundtrips
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_read_from_offset_none() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        let mut originals = Vec::new();
        for i in 0..100 {
            let record = Record::new(
                500 + i,
                10_000 + i,
                Some(Bytes::from(format!("key{}", i))),
                Bytes::from(format!("val{}", i)),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        // Read from offset 550 (50th record onward)
        let records = reader.read_from_offset(550).unwrap();
        assert_eq!(records.len(), 50);
        assert_eq!(records[0].offset, 550);
        assert_eq!(records[49].offset, 599);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[50 + i]);
        }
    }

    #[test]
    fn test_roundtrip_read_from_offset_lz4() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::Lz4);
        let mut originals = Vec::new();
        for i in 0..100 {
            let record = Record::new(
                i,
                1_000 + i * 5,
                None,
                Bytes::from(format!("payload-{:04}", i)),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        // Read from offset 75
        let records = reader.read_from_offset(75).unwrap();
        assert_eq!(records.len(), 25);
        assert_eq!(records[0].offset, 75);
        assert_eq!(records[24].offset, 99);
    }

    #[test]
    fn test_roundtrip_read_from_offset_first_record() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..10 {
            let record = Record::new(i, i * 100, None, Bytes::from("data"));
            writer.append(&record).unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        // Reading from offset 0 should return all records
        let records = reader.read_from_offset(0).unwrap();
        assert_eq!(records.len(), 10);
    }

    #[test]
    fn test_roundtrip_read_from_offset_last_record() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..10 {
            let record = Record::new(i, i * 100, None, Bytes::from("data"));
            writer.append(&record).unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        // Reading from last offset should return 1 record
        let records = reader.read_from_offset(9).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].offset, 9);
    }

    #[test]
    fn test_roundtrip_read_from_offset_beyond_end() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..10 {
            let record = Record::new(i, i * 100, None, Bytes::from("data"));
            writer.append(&record).unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        // Reading beyond the last offset should return empty
        let records = reader.read_from_offset(100).unwrap();
        assert_eq!(records.len(), 0);
    }

    // ---------------------------------------------------------------
    // Records without keys
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_records_without_keys() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        let mut originals = Vec::new();
        for i in 0..50 {
            let record = Record::new(i, 1_000_000 + i, None, Bytes::from("no-key-value"));
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 50);
        for (i, rec) in records.iter().enumerate() {
            assert!(rec.key.is_none());
            assert_eq!(rec, &originals[i]);
        }
    }

    // ---------------------------------------------------------------
    // Mixed records (some with keys, some without)
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_mixed_keys() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::Lz4);
        let mut originals = Vec::new();
        for i in 0..100 {
            let key = if i % 3 == 0 {
                Some(Bytes::from(format!("key-{}", i)))
            } else {
                None
            };
            let record = Record::new(i, 5_000 + i, key, Bytes::from(format!("v{}", i)));
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 100);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i], "mismatch at index {}", i);
        }
    }

    // ---------------------------------------------------------------
    // Large records (test block flushing with big payloads)
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_large_values_trigger_block_flush() {
        use crate::segment::reader::SegmentReader;

        // Each record is 128KB, so ~8 records should fill a 1MB block
        let mut writer = SegmentWriter::new(Compression::None);
        let mut originals = Vec::new();
        for i in 0..20 {
            let value = vec![(i as u8).wrapping_mul(7); 128 * 1024]; // 128KB
            let record = Record::new(
                i as u64,
                1_000_000 + i as u64,
                Some(Bytes::from(format!("big-{}", i))),
                Bytes::from(value),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 20);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i], "mismatch at record {}", i);
        }
    }

    #[test]
    fn test_roundtrip_large_values_lz4_block_flush() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::Lz4);
        let mut originals = Vec::new();
        for i in 0..20 {
            // Highly compressible: same byte repeated
            let value = vec![0xAB; 128 * 1024];
            let record = Record::new(
                i as u64,
                2_000_000 + i as u64,
                None,
                Bytes::from(value),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();

        // With LZ4, highly compressible data should be much smaller
        let raw_data_size = 20 * 128 * 1024; // ~2.5MB
        assert!(
            data.len() < raw_data_size / 2,
            "compressed size {} should be much less than raw {}",
            data.len(),
            raw_data_size
        );

        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 20);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i]);
        }
    }

    // ---------------------------------------------------------------
    // Header / footer structure tests
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_header_structure() {
        let mut writer = SegmentWriter::new(Compression::Lz4);
        for i in 0..5 {
            let record = Record::new(i + 10, 1000 + i, None, Bytes::from("hello"));
            writer.append(&record).unwrap();
        }
        let data = writer.finish().unwrap();

        // Check magic bytes
        assert_eq!(&data[0..4], &SEGMENT_MAGIC);

        // Check version (bytes 4..6, big-endian u16)
        let version = u16::from_be_bytes([data[4], data[5]]);
        assert_eq!(version, super::super::SEGMENT_VERSION);

        // Check compression (bytes 6..8, big-endian u16)
        let compression = u16::from_be_bytes([data[6], data[7]]);
        assert_eq!(compression, Compression::Lz4 as u16);

        // Check base offset (bytes 8..16, big-endian u64)
        let base_offset =
            u64::from_be_bytes(data[8..16].try_into().unwrap());
        assert_eq!(base_offset, 10);

        // Check end offset (bytes 16..24, big-endian u64)
        let end_offset =
            u64::from_be_bytes(data[16..24].try_into().unwrap());
        assert_eq!(end_offset, 14);

        // Check record count (bytes 24..28, big-endian u32)
        let record_count =
            u32::from_be_bytes(data[24..28].try_into().unwrap());
        assert_eq!(record_count, 5);
    }

    #[test]
    fn test_segment_footer_magic_bytes() {
        let mut writer = SegmentWriter::new(Compression::None);
        let record = Record::new(0, 0, None, Bytes::from("x"));
        writer.append(&record).unwrap();
        let data = writer.finish().unwrap();

        let footer_start = data.len() - FOOTER_SIZE;
        // Magic bytes in footer at offset +12..+16
        assert_eq!(
            &data[footer_start + 12..footer_start + 16],
            &SEGMENT_MAGIC
        );
    }

    #[test]
    fn test_segment_crc_integrity() {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..10 {
            writer
                .append(&Record::new(i, i * 10, None, Bytes::from("payload")))
                .unwrap();
        }
        let data = writer.finish().unwrap();

        let footer_start = data.len() - FOOTER_SIZE;
        // CRC32 is at footer offset +8..+12
        let stored_crc =
            u32::from_be_bytes(data[footer_start + 8..footer_start + 12].try_into().unwrap());
        let calculated_crc = crc32fast::hash(&data[..footer_start]);
        assert_eq!(stored_crc, calculated_crc);
    }

    // ---------------------------------------------------------------
    // Zstd should fail (unsupported)
    // ---------------------------------------------------------------

    #[test]
    fn test_writer_zstd_unsupported() {
        let mut writer = SegmentWriter::new(Compression::Zstd);
        // Write enough to trigger a block flush
        for i in 0..2000 {
            let record = Record::new(
                i,
                1000 + i,
                None,
                Bytes::from(vec![0u8; 1024]),
            );
            // Append might succeed until flush is triggered
            let _ = writer.append(&record);
        }
        // finish() should fail because Zstd is not implemented
        let result = writer.finish();
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // record_count, base_offset, last_offset accessors
    // ---------------------------------------------------------------

    #[test]
    fn test_writer_accessors_initial_state() {
        let writer = SegmentWriter::new(Compression::None);
        assert_eq!(writer.record_count(), 0);
        assert_eq!(writer.base_offset(), None);
        assert_eq!(writer.last_offset(), None);
    }

    #[test]
    fn test_writer_accessors_after_multiple_appends() {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..5 {
            let record = Record::new(100 + i, 5000 + i, None, Bytes::from("x"));
            writer.append(&record).unwrap();

            assert_eq!(writer.record_count(), (i + 1) as u32);
            assert_eq!(writer.base_offset(), Some(100));
            assert_eq!(writer.last_offset(), Some(100 + i));
        }
    }

    // ---------------------------------------------------------------
    // Segment size comparison: None vs Lz4
    // ---------------------------------------------------------------

    #[test]
    fn test_lz4_produces_smaller_segment_than_none() {
        let records: Vec<Record> = (0..100)
            .map(|i| {
                Record::new(
                    i,
                    1_000_000 + i,
                    Some(Bytes::from("repeated-key")),
                    Bytes::from(vec![b'Z'; 2048]), // 2KB of highly compressible data
                )
            })
            .collect();

        let mut writer_none = SegmentWriter::new(Compression::None);
        let mut writer_lz4 = SegmentWriter::new(Compression::Lz4);

        for rec in &records {
            writer_none.append(rec).unwrap();
            writer_lz4.append(rec).unwrap();
        }

        let data_none = writer_none.finish().unwrap();
        let data_lz4 = writer_lz4.finish().unwrap();

        assert!(
            data_lz4.len() < data_none.len(),
            "LZ4 ({}) should be smaller than None ({})",
            data_lz4.len(),
            data_none.len()
        );
    }

    // ---------------------------------------------------------------
    // Empty value records
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_empty_values() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        let mut originals = Vec::new();
        for i in 0..10 {
            let record = Record::new(i, i * 100, None, Bytes::new());
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 10);
        for (i, rec) in records.iter().enumerate() {
            assert!(rec.value.is_empty());
            assert_eq!(rec, &originals[i]);
        }
    }

    // ---------------------------------------------------------------
    // Non-sequential offsets
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_non_sequential_offsets() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::None);
        let offsets = [10, 20, 30, 50, 100, 200, 500];
        let mut originals = Vec::new();
        for &offset in &offsets {
            let record = Record::new(offset, 1000 + offset, None, Bytes::from("gap"));
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), offsets.len());
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i]);
        }
    }

    // ---------------------------------------------------------------
    // Stress test: 500 records with Lz4 roundtrip
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_500_records_lz4() {
        use crate::segment::reader::SegmentReader;

        let mut writer = SegmentWriter::new(Compression::Lz4);
        let mut originals = Vec::new();
        for i in 0..500 {
            let record = Record::new(
                i,
                1_000_000_000 + i * 1000,
                Some(Bytes::from(format!("user-{}", i % 50))),
                Bytes::from(format!("event-data-{}-{}", i, "x".repeat((i % 100) as usize))),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        assert_eq!(reader.record_count(), 500);
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 500);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i], "mismatch at index {}", i);
        }
    }
}
