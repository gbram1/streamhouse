#![no_main]

use bytes::Bytes;
use libfuzzer_sys::fuzz_target;
use streamhouse_storage::SegmentReader;

fuzz_target!(|data: &[u8]| {
    // Feed arbitrary bytes to the segment reader.
    // The reader should handle all malformed inputs gracefully:
    // - Invalid magic bytes
    // - Truncated headers/footers
    // - Bad CRC32 checksums
    // - Invalid compression data
    // - Corrupted delta-encoded varints
    // - Malformed index entries
    let bytes = Bytes::copy_from_slice(data);

    if let Ok(reader) = SegmentReader::new(bytes) {
        // If parsing succeeded, try reading records
        let _ = reader.read_all();
        let _ = reader.record_count();
        let _ = reader.base_offset();
        let _ = reader.end_offset();

        // Try offset-based reads with various offsets
        let _ = reader.read_from_offset(0);
        let _ = reader.read_from_offset(reader.base_offset());
        if reader.end_offset() > 0 {
            let _ = reader.read_from_offset(reader.end_offset() - 1);
        }
        let _ = reader.read_from_offset(u64::MAX);
    }
});
