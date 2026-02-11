//! Edge-case tests for the segment format, varint encoding, and record types.

use bytes::Bytes;
use streamhouse_core::varint::{decode_varint, decode_varint_u64, encode_varint, encode_varint_u64};
use streamhouse_core::Record;

// ---------------------------------------------------------------
// Varint encoding round-trip
// ---------------------------------------------------------------

#[test]
fn varint_roundtrip_zero() {
    let mut buf = Vec::new();
    encode_varint(&mut buf, 0);
    let decoded = decode_varint(&mut &buf[..]);
    assert_eq!(decoded, 0);
}

#[test]
fn varint_roundtrip_positive_small() {
    for val in 1..=127i64 {
        let mut buf = Vec::new();
        encode_varint(&mut buf, val);
        let decoded = decode_varint(&mut &buf[..]);
        assert_eq!(decoded, val, "failed for value {val}");
    }
}

#[test]
fn varint_roundtrip_negative() {
    for val in [-1i64, -2, -128, -256, -1000, -i64::MAX] {
        let mut buf = Vec::new();
        encode_varint(&mut buf, val);
        let decoded = decode_varint(&mut &buf[..]);
        assert_eq!(decoded, val, "failed for value {val}");
    }
}

#[test]
fn varint_roundtrip_large_values() {
    let values = [
        128i64,
        255,
        256,
        16383,
        16384,
        2_097_151,
        268_435_455,
        i64::MAX,
        i64::MIN + 1,
    ];
    for val in values {
        let mut buf = Vec::new();
        encode_varint(&mut buf, val);
        let decoded = decode_varint(&mut &buf[..]);
        assert_eq!(decoded, val, "failed for value {val}");
    }
}

#[test]
fn varint_u64_roundtrip() {
    let values = [0u64, 1, 127, 128, 255, 16383, 16384, u64::MAX / 2, u64::MAX];
    for val in values {
        let mut buf = Vec::new();
        encode_varint_u64(&mut buf, val);
        let decoded = decode_varint_u64(&mut &buf[..]);
        assert_eq!(decoded, val, "failed for value {val}");
    }
}

#[test]
fn varint_encoding_size() {
    // Small values should encode in 1 byte
    let mut buf = Vec::new();
    encode_varint_u64(&mut buf, 0);
    assert_eq!(buf.len(), 1);

    let mut buf = Vec::new();
    encode_varint_u64(&mut buf, 127);
    assert_eq!(buf.len(), 1);

    // 128 should need 2 bytes
    let mut buf = Vec::new();
    encode_varint_u64(&mut buf, 128);
    assert_eq!(buf.len(), 2);
}

#[test]
fn varint_zigzag_symmetry() {
    // -1 and 0 should both encode small via zigzag
    let mut buf_neg1 = Vec::new();
    encode_varint(&mut buf_neg1, -1);

    let mut buf_zero = Vec::new();
    encode_varint(&mut buf_zero, 0);

    // Both should be single byte
    assert_eq!(buf_neg1.len(), 1);
    assert_eq!(buf_zero.len(), 1);
}

// ---------------------------------------------------------------
// Record construction edge cases
// ---------------------------------------------------------------

#[test]
fn record_empty_value() {
    let record = Record::new(0, 0, None, Bytes::new());
    assert_eq!(record.value.len(), 0);
    assert_eq!(record.estimated_size(), 16); // offset(8) + timestamp(8)
}

#[test]
fn record_max_offset_and_timestamp() {
    let record = Record::new(u64::MAX, u64::MAX, None, Bytes::from("data"));
    assert_eq!(record.offset, u64::MAX);
    assert_eq!(record.timestamp, u64::MAX);
}

#[test]
fn record_large_value() {
    let big_value = vec![0xABu8; 1_000_000]; // 1MB
    let record = Record::new(0, 0, None, Bytes::from(big_value));
    assert_eq!(record.value.len(), 1_000_000);
    assert_eq!(record.estimated_size(), 16 + 1_000_000);
}

#[test]
fn record_with_key() {
    let record = Record::new(
        42,
        1000,
        Some(Bytes::from("user-123")),
        Bytes::from("payload"),
    );
    assert_eq!(record.estimated_size(), 16 + 8 + 7); // 16 + key_len + value_len
}

#[test]
fn record_serde_roundtrip() {
    let record = Record::new(
        100,
        1234567890000,
        Some(Bytes::from("key")),
        Bytes::from(r#"{"event":"click"}"#),
    );

    let json = serde_json::to_string(&record).unwrap();
    let decoded: Record = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.offset, record.offset);
    assert_eq!(decoded.timestamp, record.timestamp);
    assert_eq!(decoded.key, record.key);
    assert_eq!(decoded.value, record.value);
}

#[test]
fn record_serde_no_key() {
    let record = Record::new(0, 0, None, Bytes::from("val"));
    let json = serde_json::to_string(&record).unwrap();
    let decoded: Record = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.key, None);
}

#[test]
fn record_equality() {
    let r1 = Record::new(1, 100, Some(Bytes::from("k")), Bytes::from("v"));
    let r2 = Record::new(1, 100, Some(Bytes::from("k")), Bytes::from("v"));
    assert_eq!(r1, r2);
}

#[test]
fn record_inequality_different_offset() {
    let r1 = Record::new(1, 100, None, Bytes::from("v"));
    let r2 = Record::new(2, 100, None, Bytes::from("v"));
    assert_ne!(r1, r2);
}

// ---------------------------------------------------------------
// SegmentInfo serialization
// ---------------------------------------------------------------

#[test]
fn segment_info_roundtrip() {
    let info = streamhouse_core::SegmentInfo {
        id: "seg-001".to_string(),
        topic: "orders".to_string(),
        partition_id: 0,
        base_offset: 0,
        end_offset: 999,
        record_count: 1000,
        size_bytes: 524288,
        s3_bucket: "data".to_string(),
        s3_key: "orders/0/seg-001.bin".to_string(),
        created_at: 1700000000000,
    };

    let json = serde_json::to_string(&info).unwrap();
    let decoded: streamhouse_core::SegmentInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, info);
}

// ---------------------------------------------------------------
// Varint edge cases that could cause issues in segments
// ---------------------------------------------------------------

#[test]
fn varint_sequential_deltas() {
    // Simulate delta encoding of sequential offsets (0, 1, 2, ...)
    // Delta = 1 for each
    let mut buf = Vec::new();
    for _ in 0..1000 {
        encode_varint(&mut buf, 1); // delta of 1
    }

    let mut cursor = &buf[..];
    for _ in 0..1000 {
        let delta = decode_varint(&mut cursor);
        assert_eq!(delta, 1);
    }
    assert!(cursor.is_empty());
}

#[test]
fn varint_mixed_positive_negative_deltas() {
    // Timestamp deltas can be negative if records arrive out of order
    let deltas = vec![100i64, -5, 200, -1, 0, 50, -100];
    let mut buf = Vec::new();
    for &d in &deltas {
        encode_varint(&mut buf, d);
    }

    let mut cursor = &buf[..];
    for &expected in &deltas {
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, expected);
    }
}

#[test]
fn varint_u64_max_value_roundtrip() {
    let mut buf = Vec::new();
    encode_varint_u64(&mut buf, u64::MAX);
    let decoded = decode_varint_u64(&mut &buf[..]);
    assert_eq!(decoded, u64::MAX);
    assert_eq!(buf.len(), 10); // u64::MAX requires 10 bytes in varint encoding
}
