//! Segment Reader - Reading and Decompressing Segments from S3
//!
//! This module implements `SegmentReader`, which reads compressed segment files
//! downloaded from S3 and converts them back into records.
//!
//! ## What Does SegmentReader Do?
//!
//! 1. **Validates the segment** (magic bytes, version, CRC32 checksum)
//! 2. **Parses the header** to get metadata (compression, offsets, record count)
//! 3. **Reads the index** for fast offset-based seeks
//! 4. **Decompresses blocks** on-demand as records are read
//! 5. **Decodes delta-encoded records** back to absolute offsets/timestamps
//! 6. **Returns records** either all at once or from a specific offset
//!
//! ## When to Use SegmentReader
//!
//! Use this when:
//! - Consumers request records from a specific offset
//! - Need to read historical data from S3
//! - Rebuilding state after a crash
//! - Running SQL queries over segments (DataFusion integration)
//!
//! ## Performance
//!
//! - **Read throughput**: 3.10M records/sec with LZ4 decompression
//! - **Seek time**: 640µs to read from 90% offset (skips first 90% of data)
//! - **Memory usage**: Only decompresses needed blocks (not entire segment)
//!
//! ## Example Usage
//!
//! ### Reading All Records
//! ```ignore
//! use streamhouse_storage::SegmentReader;
//! use bytes::Bytes;
//!
//! // Download segment from S3
//! let segment_bytes = s3_client
//!     .get_object(bucket, key)
//!     .await?;
//!
//! // Create reader
//! let reader = SegmentReader::new(Bytes::from(segment_bytes))?;
//!
//! println!("Segment contains {} records", reader.record_count());
//! println!("Offsets {} to {}", reader.base_offset(), reader.end_offset());
//!
//! // Read all records
//! let records = reader.read_all()?;
//! ```
//!
//! ### Reading from Specific Offset
//! ```ignore
//! // Consumer wants records starting from offset 5000
//! let records = reader.read_from_offset(5000)?;
//!
//! // Only decompresses blocks containing offset 5000 and later
//! // Skips earlier blocks entirely (efficient!)
//! for record in records {
//!     process_record(record);
//! }
//! ```
//!
//! ## Technical Details
//!
//! ### Validation Process
//! 1. Check file size is at least header + footer
//! 2. Verify magic bytes at start ("STRM")
//! 3. Check version is supported (currently v1)
//! 4. Read footer to get index position
//! 5. Verify CRC32 checksum of entire file
//! 6. Verify magic bytes at end (footer)
//! 7. Parse index entries
//!
//! ### Delta Decoding
//! Records are stored as deltas, so we need to reconstruct:
//! - Start with base offset from first record
//! - Add each delta to get absolute offset: `current_offset += delta`
//! - Same for timestamps
//!
//! ### Block-Level Decompression
//! We don't decompress the entire segment at once:
//! - Use index to find which block contains requested offset
//! - Only decompress that block and subsequent blocks
//! - Saves CPU and memory for offset-based reads
//!
//! ### Binary Search for Seeks
//! When reading from offset X:
//! 1. Binary search the index to find the block containing offset X
//! 2. Might need to go back one block (since first record's delta could span blocks)
//! 3. Start decompressing from that block
//! 4. Skip records until we hit offset X
//!
//! ## Error Handling
//!
//! Reader validates everything and returns errors for:
//! - `InvalidMagic`: File doesn't start/end with "STRM"
//! - `UnsupportedVersion`: Segment created by newer version
//! - `CrcMismatch`: Data corruption detected
//! - `InvalidSegment`: Malformed data (truncated, invalid varints, etc.)
//! - `Decompression`: LZ4 decompression failed (likely corruption)
//!
//! ## Thread Safety
//!
//! SegmentReader is NOT thread-safe (contains internal cursors).
//! For concurrent reads, create multiple readers (the segment bytes can be shared via Bytes::clone()).

use bytes::{Buf, Bytes};
use streamhouse_core::segment::Compression;
use streamhouse_core::{varint, Error, Record, Result};

use super::{FOOTER_SIZE, HEADER_SIZE, SEGMENT_MAGIC, SEGMENT_VERSION};

/// Reads records from a segment file
pub struct SegmentReader {
    /// The complete segment data
    data: Bytes,

    /// Segment header information
    header: SegmentHeader,

    /// Index entries for fast offset lookup
    index: Vec<IndexEntry>,

    /// Position of the index in the file
    index_position: u64,
}

#[derive(Debug, Clone)]
struct SegmentHeader {
    #[allow(dead_code)]
    version: u16,
    compression: Compression,
    base_offset: u64,
    end_offset: u64,
    record_count: u32,
    #[allow(dead_code)]
    block_count: u32,
}

#[derive(Debug, Clone)]
struct IndexEntry {
    offset: u64,
    file_position: u64,
    timestamp: u64,
}

impl SegmentReader {
    /// Open a segment file for reading
    pub fn new(data: Bytes) -> Result<Self> {
        if data.len() < HEADER_SIZE + FOOTER_SIZE {
            return Err(Error::InvalidSegment("Segment too small".to_string()));
        }

        // Read and validate header
        let header = Self::read_header(&data)?;

        // Read footer to get index position
        let footer_start = data.len() - FOOTER_SIZE;
        let index_position = Self::read_footer(&data, footer_start)?;

        // Read index
        let index = Self::read_index(&data, index_position as usize)?;

        Ok(Self {
            data,
            header,
            index,
            index_position,
        })
    }

    /// Read and validate the segment header
    fn read_header(data: &Bytes) -> Result<SegmentHeader> {
        let mut cursor = &data[..HEADER_SIZE];

        // Check magic bytes
        let mut magic = [0u8; 4];
        cursor.copy_to_slice(&mut magic);
        if magic != SEGMENT_MAGIC {
            return Err(Error::InvalidMagic);
        }

        let version = cursor.get_u16();
        if version != SEGMENT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }

        let compression_value = cursor.get_u16();
        let compression = Compression::try_from(compression_value)?;

        let base_offset = cursor.get_u64();
        let end_offset = cursor.get_u64();
        let record_count = cursor.get_u32();
        let block_count = cursor.get_u32();

        Ok(SegmentHeader {
            version,
            compression,
            base_offset,
            end_offset,
            record_count,
            block_count,
        })
    }

    /// Read and validate the footer
    fn read_footer(data: &Bytes, footer_start: usize) -> Result<u64> {
        let mut cursor = &data[footer_start..];

        let index_position = cursor.get_u64();

        // Verify CRC
        let stored_crc = cursor.get_u32();
        let calculated_crc = crc32fast::hash(&data[..footer_start]);

        if stored_crc != calculated_crc {
            return Err(Error::CrcMismatch);
        }

        // Check magic bytes again
        let mut magic = [0u8; 4];
        cursor.copy_to_slice(&mut magic);
        if magic != SEGMENT_MAGIC {
            return Err(Error::InvalidMagic);
        }

        Ok(index_position)
    }

    /// Read the index section
    fn read_index(data: &Bytes, index_start: usize) -> Result<Vec<IndexEntry>> {
        let mut cursor = &data[index_start..];

        let entry_count = cursor.get_u32() as usize;
        let mut index = Vec::with_capacity(entry_count);

        for _ in 0..entry_count {
            index.push(IndexEntry {
                offset: cursor.get_u64(),
                file_position: cursor.get_u64(),
                timestamp: cursor.get_u64(),
            });
        }

        Ok(index)
    }

    /// Read all records from the segment
    pub fn read_all(&self) -> Result<Vec<Record>> {
        let mut records = Vec::with_capacity(self.header.record_count as usize);

        // Read each block
        for i in 0..self.index.len() {
            let block_records = self.read_block(i)?;
            records.extend(block_records);
        }

        Ok(records)
    }

    /// Read a specific block by index
    fn read_block(&self, block_index: usize) -> Result<Vec<Record>> {
        if block_index >= self.index.len() {
            return Err(Error::InvalidSegment(format!(
                "Block index {} out of range",
                block_index
            )));
        }

        let entry = &self.index[block_index];
        let block_start = entry.file_position as usize;

        // Determine block end
        let block_end = if block_index + 1 < self.index.len() {
            self.index[block_index + 1].file_position as usize
        } else {
            self.index_position as usize
        };

        // Extract and decompress block
        let block_data = self.data.slice(block_start..block_end);
        let decompressed = self.decompress_block(&block_data)?;

        // Decode records from the block
        self.decode_block(&decompressed, entry.offset, entry.timestamp)
    }

    /// Decompress a block
    fn decompress_block(&self, block_data: &Bytes) -> Result<Bytes> {
        match self.header.compression {
            Compression::None => Ok(block_data.clone()),
            Compression::Lz4 => {
                let decompressed = lz4_flex::decompress_size_prepended(block_data)
                    .map_err(|e| Error::Decompression(e.to_string()))?;
                Ok(Bytes::from(decompressed))
            }
            Compression::Zstd => Err(Error::Unsupported(
                "Zstd compression not yet implemented".to_string(),
            )),
        }
    }

    /// Decode all records from a decompressed block
    fn decode_block(
        &self,
        data: &Bytes,
        base_offset: u64,
        base_timestamp: u64,
    ) -> Result<Vec<Record>> {
        let mut records = Vec::new();
        let mut cursor = data.as_ref();

        let mut current_offset = base_offset;
        let mut current_timestamp = base_timestamp;

        while cursor.has_remaining() {
            // Decode offset delta
            let offset_delta = varint::decode_varint(&mut cursor);
            current_offset = current_offset.wrapping_add(offset_delta as u64);

            // Decode timestamp delta
            let timestamp_delta = varint::decode_varint(&mut cursor);
            current_timestamp = current_timestamp.wrapping_add(timestamp_delta as u64);

            // Decode key
            let key_len = varint::decode_varint_u64(&mut cursor) as usize;
            let key = if key_len > 0 {
                if cursor.remaining() < key_len {
                    return Err(Error::InvalidSegment(
                        "Unexpected end of block reading key".to_string(),
                    ));
                }
                let key_data = Bytes::copy_from_slice(&cursor[..key_len]);
                cursor.advance(key_len);
                Some(key_data)
            } else {
                None
            };

            // Decode value
            let value_len = varint::decode_varint_u64(&mut cursor) as usize;
            if cursor.remaining() < value_len {
                return Err(Error::InvalidSegment(
                    "Unexpected end of block reading value".to_string(),
                ));
            }
            let value = Bytes::copy_from_slice(&cursor[..value_len]);
            cursor.advance(value_len);

            records.push(Record::new(current_offset, current_timestamp, key, value));
        }

        Ok(records)
    }

    /// Get the base offset of this segment
    pub fn base_offset(&self) -> u64 {
        self.header.base_offset
    }

    /// Get the end offset of this segment
    pub fn end_offset(&self) -> u64 {
        self.header.end_offset
    }

    /// Get the total number of records in this segment
    pub fn record_count(&self) -> u32 {
        self.header.record_count
    }

    /// Read records starting from a specific offset
    pub fn read_from_offset(&self, start_offset: u64) -> Result<Vec<Record>> {
        // Find the block containing this offset using binary search
        let block_index = self.find_block_for_offset(start_offset);

        let mut records = Vec::new();

        // Read from the found block onwards
        for i in block_index..self.index.len() {
            let block_records = self.read_block(i)?;

            // Filter records by offset
            for record in block_records {
                if record.offset >= start_offset {
                    records.push(record);
                }
            }
        }

        Ok(records)
    }

    /// Find which block contains the given offset (binary search)
    fn find_block_for_offset(&self, offset: u64) -> usize {
        if self.index.is_empty() {
            return 0;
        }

        // Binary search to find the right block
        let mut left = 0;
        let mut right = self.index.len();

        while left < right {
            let mid = (left + right) / 2;

            if self.index[mid].offset <= offset {
                left = mid + 1;
            } else {
                right = mid;
            }
        }

        // Go back one block to ensure we don't miss the record
        if left > 0 {
            left - 1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::writer::SegmentWriter;

    #[test]
    fn test_roundtrip_single_record() {
        let mut writer = SegmentWriter::new(Compression::None);

        let original = Record::new(
            100,
            1234567890,
            Some(Bytes::from("key1")),
            Bytes::from("value1"),
        );

        writer.append(&original).unwrap();
        let segment_bytes = writer.finish().unwrap();

        // Read it back
        let reader = SegmentReader::new(Bytes::from(segment_bytes)).unwrap();
        let records = reader.read_all().unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0], original);
    }

    #[test]
    fn test_roundtrip_multiple_records() {
        let mut writer = SegmentWriter::new(Compression::None);

        let mut originals = Vec::new();
        for i in 0..10 {
            let record = Record::new(
                100 + i,
                1234567890 + i * 1000,
                Some(Bytes::from(format!("key{}", i))),
                Bytes::from(format!("value{}", i)),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let segment_bytes = writer.finish().unwrap();

        // Read it back
        let reader = SegmentReader::new(Bytes::from(segment_bytes)).unwrap();

        assert_eq!(reader.base_offset(), 100);
        assert_eq!(reader.end_offset(), 109);
        assert_eq!(reader.record_count(), 10);

        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 10);

        for (i, record) in records.iter().enumerate() {
            assert_eq!(record, &originals[i]);
        }
    }

    #[test]
    fn test_roundtrip_with_lz4() {
        let mut writer = SegmentWriter::new(Compression::Lz4);

        let mut originals = Vec::new();
        for i in 0..100 {
            let record = Record::new(
                i,
                1234567890 + i,
                Some(Bytes::from(format!("key{}", i))),
                Bytes::from(vec![b'x'; 1024]), // Compressible data
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let segment_bytes = writer.finish().unwrap();

        // Read it back
        let reader = SegmentReader::new(Bytes::from(segment_bytes)).unwrap();
        let records = reader.read_all().unwrap();

        assert_eq!(records.len(), 100);

        for (i, record) in records.iter().enumerate() {
            assert_eq!(record, &originals[i]);
        }
    }

    #[test]
    fn test_read_from_offset() {
        let mut writer = SegmentWriter::new(Compression::None);

        for i in 100..200 {
            let record = Record::new(
                i,
                1234567890 + i,
                Some(Bytes::from(format!("key{}", i))),
                Bytes::from(format!("value{}", i)),
            );
            writer.append(&record).unwrap();
        }

        let segment_bytes = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(segment_bytes)).unwrap();

        // Read from offset 150
        let records = reader.read_from_offset(150).unwrap();

        assert_eq!(records.len(), 50);
        assert_eq!(records[0].offset, 150);
        assert_eq!(records[49].offset, 199);
    }

    #[test]
    fn test_invalid_magic() {
        let mut bad_data = vec![0u8; 100];
        bad_data[0..4].copy_from_slice(b"BADM");

        let result = SegmentReader::new(Bytes::from(bad_data));
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // Reader error cases
    // ---------------------------------------------------------------

    #[test]
    fn test_reader_segment_too_small() {
        // Segment smaller than HEADER_SIZE + FOOTER_SIZE should fail
        let tiny = vec![0u8; 10];
        let result = SegmentReader::new(Bytes::from(tiny));
        assert!(result.is_err());
    }

    #[test]
    fn test_reader_exactly_min_size_invalid() {
        // Exactly HEADER_SIZE + FOOTER_SIZE but invalid content
        let data = vec![0u8; HEADER_SIZE + FOOTER_SIZE];
        let result = SegmentReader::new(Bytes::from(data));
        assert!(result.is_err()); // Invalid magic
    }

    #[test]
    fn test_reader_corrupted_crc() {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..5 {
            writer
                .append(&Record::new(i, 1000 + i, None, Bytes::from("test")))
                .unwrap();
        }
        let mut data = writer.finish().unwrap();

        // Corrupt a byte in the middle of the data (not header, not footer)
        let mid = data.len() / 2;
        data[mid] ^= 0xFF;

        let result = SegmentReader::new(Bytes::from(data));
        assert!(result.is_err()); // CRC mismatch
    }

    #[test]
    fn test_reader_corrupted_footer_magic() {
        let mut writer = SegmentWriter::new(Compression::None);
        writer
            .append(&Record::new(0, 0, None, Bytes::from("x")))
            .unwrap();
        let mut data = writer.finish().unwrap();

        // Corrupt footer magic bytes (at footer_start + 12..16)
        // But CRC is computed over everything before footer, so we'd get CRC
        // mismatch first. Let's corrupt just the footer CRC byte to match
        // then corrupt magic.
        let footer_start = data.len() - FOOTER_SIZE;
        // Corrupt footer magic (bytes 12..16 within footer)
        data[footer_start + 12] = 0x00;

        // This will fail because CRC covers everything before footer,
        // and we changed the footer area which is after the CRC check boundary.
        // Actually footer magic is checked AFTER CRC, and CRC only covers data
        // before the footer. So let's set up a valid CRC but bad footer magic.
        // The CRC in the footer covers data[..footer_start].
        // We corrupt footer magic but not the data, so CRC should pass but magic fails.
        let mut data2 = writer_and_finish_helper(5);
        let footer_start2 = data2.len() - FOOTER_SIZE;
        // Overwrite just the footer magic with wrong bytes
        data2[footer_start2 + 12] = b'B';
        data2[footer_start2 + 13] = b'A';
        data2[footer_start2 + 14] = b'D';
        data2[footer_start2 + 15] = b'!';

        let result = SegmentReader::new(Bytes::from(data2));
        assert!(result.is_err());
    }

    /// Helper: write N records with Compression::None and return segment bytes
    fn writer_and_finish_helper(n: u64) -> Vec<u8> {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..n {
            writer
                .append(&Record::new(i, 1000 + i, None, Bytes::from("data")))
                .unwrap();
        }
        writer.finish().unwrap()
    }

    // ---------------------------------------------------------------
    // Reader accessor methods
    // ---------------------------------------------------------------

    #[test]
    fn test_reader_accessors() {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 50..60 {
            writer
                .append(&Record::new(i, 9000 + i, None, Bytes::from("val")))
                .unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        assert_eq!(reader.base_offset(), 50);
        assert_eq!(reader.end_offset(), 59);
        assert_eq!(reader.record_count(), 10);
    }

    // ---------------------------------------------------------------
    // read_from_offset edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_read_from_offset_before_base() {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 100..110 {
            writer
                .append(&Record::new(i, 5000 + i, None, Bytes::from("v")))
                .unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        // Reading from an offset before the base should still return all records
        let records = reader.read_from_offset(0).unwrap();
        assert_eq!(records.len(), 10);
        assert_eq!(records[0].offset, 100);
    }

    #[test]
    fn test_read_from_offset_exact_end() {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..10 {
            writer
                .append(&Record::new(i, 1000 + i, None, Bytes::from("v")))
                .unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        // Read from last offset
        let records = reader.read_from_offset(9).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].offset, 9);
    }

    #[test]
    fn test_read_from_offset_past_end() {
        let mut writer = SegmentWriter::new(Compression::None);
        for i in 0..10 {
            writer
                .append(&Record::new(i, 1000 + i, None, Bytes::from("v")))
                .unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        let records = reader.read_from_offset(10).unwrap();
        assert_eq!(records.len(), 0);
    }

    #[test]
    fn test_read_from_offset_midpoint_lz4() {
        let mut writer = SegmentWriter::new(Compression::Lz4);
        let mut originals = Vec::new();
        for i in 0..50 {
            let record = Record::new(
                i,
                2_000_000 + i * 100,
                Some(Bytes::from(format!("k{}", i))),
                Bytes::from(format!("v{}", i)),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        let records = reader.read_from_offset(25).unwrap();
        assert_eq!(records.len(), 25);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[25 + i]);
        }
    }

    // ---------------------------------------------------------------
    // Roundtrip with large number of records (stress)
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_1000_records_none() {
        let mut writer = SegmentWriter::new(Compression::None);
        let mut originals = Vec::new();
        for i in 0..1000u64 {
            let record = Record::new(
                i,
                1_000_000_000 + i,
                Some(Bytes::from(format!("key-{:04}", i))),
                Bytes::from(format!("value-{}", i)),
            );
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        assert_eq!(reader.record_count(), 1000);
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 1000);
        // Spot-check first, last, and middle
        assert_eq!(records[0], originals[0]);
        assert_eq!(records[500], originals[500]);
        assert_eq!(records[999], originals[999]);
    }

    // ---------------------------------------------------------------
    // Read all then read_from_offset consistency
    // ---------------------------------------------------------------

    #[test]
    fn test_read_all_consistent_with_read_from_offset() {
        let mut writer = SegmentWriter::new(Compression::Lz4);
        for i in 0..100 {
            writer
                .append(&Record::new(
                    i,
                    5000 + i,
                    None,
                    Bytes::from(format!("data-{}", i)),
                ))
                .unwrap();
        }
        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();

        let all = reader.read_all().unwrap();
        let from_start = reader.read_from_offset(0).unwrap();
        assert_eq!(all.len(), from_start.len());
        for (a, b) in all.iter().zip(from_start.iter()) {
            assert_eq!(a, b);
        }
    }

    // ---------------------------------------------------------------
    // Binary data as values
    // ---------------------------------------------------------------

    #[test]
    fn test_roundtrip_binary_data() {
        let mut writer = SegmentWriter::new(Compression::None);
        let mut originals = Vec::new();
        for i in 0..20 {
            let binary_val: Vec<u8> = (0..256).map(|b| ((b + i) % 256) as u8).collect();
            let record = Record::new(i as u64, 9000 + i as u64, None, Bytes::from(binary_val));
            originals.push(record.clone());
            writer.append(&record).unwrap();
        }

        let data = writer.finish().unwrap();
        let reader = SegmentReader::new(Bytes::from(data)).unwrap();
        let records = reader.read_all().unwrap();
        assert_eq!(records.len(), 20);
        for (i, rec) in records.iter().enumerate() {
            assert_eq!(rec, &originals[i]);
        }
    }
}
