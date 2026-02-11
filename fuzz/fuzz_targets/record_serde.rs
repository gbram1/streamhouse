#![no_main]

use bytes::Bytes;
use libfuzzer_sys::fuzz_target;
use streamhouse_core::Record;
use streamhouse_core::varint::{decode_varint, decode_varint_u64};

fuzz_target!(|data: &[u8]| {
    // Fuzz record deserialization and varint decoding.
    // Tests handling of:
    // - Malformed JSON
    // - Invalid varint sequences
    // - Huge values
    // - Missing fields

    // Try JSON deserialization
    let _ = serde_json::from_slice::<Record>(data);

    // Try varint decoding (critical for segment delta encoding)
    if !data.is_empty() {
        let mut cursor = &data[..];
        let _ = decode_varint(&mut cursor);
    }
    if !data.is_empty() {
        let mut cursor = &data[..];
        let _ = decode_varint_u64(&mut cursor);
    }

    // If we have enough bytes, try constructing a record and round-tripping it
    if data.len() >= 16 {
        let offset = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let timestamp = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let value = Bytes::copy_from_slice(&data[16..]);
        let record = Record::new(offset, timestamp, None, value);

        // Round-trip through JSON
        if let Ok(json) = serde_json::to_vec(&record) {
            let decoded: Record = serde_json::from_slice(&json).unwrap();
            assert_eq!(record.offset, decoded.offset);
            assert_eq!(record.timestamp, decoded.timestamp);
        }
    }
});
