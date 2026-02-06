//! Variable-length Integer Encoding (Varint)
//!
//! This module provides efficient variable-length encoding for integers using two techniques:
//!
//! ## Varint Encoding
//! Instead of always using 8 bytes for a u64, varints use only as many bytes as needed:
//! - Small numbers (0-127) use just 1 byte
//! - Larger numbers use 2-9 bytes depending on magnitude
//! - Each byte uses 7 bits for data and 1 bit as a "continuation" flag
//!
//! ## ZigZag Encoding (for signed integers)
//! Maps signed integers to unsigned so small negative numbers are also efficient:
//! - 0 → 0, -1 → 1, 1 → 2, -2 → 3, 2 → 4, etc.
//! - This means -1 encodes to 1 byte instead of 8 bytes
//!
//! ## Why This Matters for StreamHouse
//! We use varints for delta encoding in segments:
//! - Offsets are sequential (100, 101, 102...) so deltas are small (0, 1, 1...)
//! - Timestamps are usually close together, so deltas are small
//! - This can save 5-7 bytes per record, significantly reducing storage costs
//!
//! ## Usage
//! ```ignore
//! let mut buf = BytesMut::new();
//! encode_varint(&mut buf, -42);  // Encodes efficiently
//! let value = decode_varint(&mut buf.as_ref());  // Returns -42
//! ```

use bytes::{Buf, BufMut};

/// Encode a signed integer as a varint (ZigZag encoding)
pub fn encode_varint(buf: &mut impl BufMut, value: i64) {
    // ZigZag encoding: maps signed integers to unsigned
    // 0 => 0, -1 => 1, 1 => 2, -2 => 3, 2 => 4, etc.
    let unsigned = ((value << 1) ^ (value >> 63)) as u64;

    encode_varint_u64(buf, unsigned);
}

/// Encode an unsigned integer as a varint
pub fn encode_varint_u64(buf: &mut impl BufMut, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;

        if value != 0 {
            byte |= 0x80; // Set continuation bit
        }

        buf.put_u8(byte);

        if value == 0 {
            break;
        }
    }
}

/// Decode a varint to a signed integer
pub fn decode_varint(buf: &mut impl Buf) -> i64 {
    let unsigned = decode_varint_u64(buf);

    // ZigZag decoding
    let value = (unsigned >> 1) as i64;
    if (unsigned & 1) != 0 {
        !value
    } else {
        value
    }
}

/// Decode a varint to an unsigned integer
pub fn decode_varint_u64(buf: &mut impl Buf) -> u64 {
    let mut value: u64 = 0;
    let mut shift = 0;

    loop {
        let byte = buf.get_u8();
        value |= ((byte & 0x7F) as u64) << shift;

        if (byte & 0x80) == 0 {
            break;
        }

        shift += 7;

        if shift >= 64 {
            panic!("Varint too large");
        }
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_varint_small_positive() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 5);

        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, 5);
    }

    #[test]
    fn test_varint_small_negative() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, -5);

        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, -5);
    }

    #[test]
    fn test_varint_zero() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 0);

        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, 0);
    }

    #[test]
    fn test_varint_large_positive() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 1_000_000);

        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, 1_000_000);
    }

    #[test]
    fn test_varint_large_negative() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, -1_000_000);

        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, -1_000_000);
    }

    #[test]
    fn test_varint_u64() {
        let mut buf = BytesMut::new();
        encode_varint_u64(&mut buf, 12345);

        let mut cursor = buf.as_ref();
        let decoded = decode_varint_u64(&mut cursor);
        assert_eq!(decoded, 12345);
    }

    #[test]
    fn test_varint_compression() {
        // Small numbers should use fewer bytes (ZigZag encoding doubles values)
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 0);
        assert_eq!(buf.len(), 1); // Should use only 1 byte

        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 1);
        assert_eq!(buf.len(), 1); // Should use only 1 byte (encodes as 2)

        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 63);
        assert_eq!(buf.len(), 1); // Should use only 1 byte (encodes as 126)

        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 64);
        assert_eq!(buf.len(), 2); // Should use 2 bytes (encodes as 128)
    }

    // ---------------------------------------------------------------
    // Signed varint: extreme values (i64 boundaries)
    // ---------------------------------------------------------------

    #[test]
    fn test_varint_i64_max() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, i64::MAX);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, i64::MAX);
    }

    #[test]
    fn test_varint_i64_min() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, i64::MIN);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, i64::MIN);
    }

    #[test]
    fn test_varint_negative_one() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, -1);
        // ZigZag: -1 -> 1, which fits in 1 byte
        assert_eq!(buf.len(), 1);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, -1);
    }

    #[test]
    fn test_varint_one() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, 1);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, 1);
    }

    #[test]
    fn test_varint_i64_min_plus_one() {
        let val = i64::MIN + 1;
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, val);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_varint_i64_max_minus_one() {
        let val = i64::MAX - 1;
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, val);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint(&mut cursor);
        assert_eq!(decoded, val);
    }

    // ---------------------------------------------------------------
    // Unsigned varint: extreme values (u64 boundaries)
    // ---------------------------------------------------------------

    #[test]
    fn test_varint_u64_zero() {
        let mut buf = BytesMut::new();
        encode_varint_u64(&mut buf, 0);
        assert_eq!(buf.len(), 1);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint_u64(&mut cursor);
        assert_eq!(decoded, 0);
    }

    #[test]
    fn test_varint_u64_one() {
        let mut buf = BytesMut::new();
        encode_varint_u64(&mut buf, 1);
        assert_eq!(buf.len(), 1);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint_u64(&mut cursor);
        assert_eq!(decoded, 1);
    }

    #[test]
    fn test_varint_u64_max() {
        let mut buf = BytesMut::new();
        encode_varint_u64(&mut buf, u64::MAX);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint_u64(&mut cursor);
        assert_eq!(decoded, u64::MAX);
    }

    #[test]
    fn test_varint_u64_127() {
        // 127 = 0x7F, max value that fits in 1 byte
        let mut buf = BytesMut::new();
        encode_varint_u64(&mut buf, 127);
        assert_eq!(buf.len(), 1);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint_u64(&mut cursor);
        assert_eq!(decoded, 127);
    }

    #[test]
    fn test_varint_u64_128() {
        // 128 = 0x80, first value that requires 2 bytes
        let mut buf = BytesMut::new();
        encode_varint_u64(&mut buf, 128);
        assert_eq!(buf.len(), 2);
        let mut cursor = buf.as_ref();
        let decoded = decode_varint_u64(&mut cursor);
        assert_eq!(decoded, 128);
    }

    #[test]
    fn test_varint_u64_power_of_two_boundaries() {
        // Test values at each 7-bit boundary: 2^7, 2^14, 2^21, 2^28, ...
        let boundaries = [
            (1u64 << 7, 2),   // 128 -> 2 bytes
            (1u64 << 14, 3),  // 16384 -> 3 bytes
            (1u64 << 21, 4),  // 2097152 -> 4 bytes
            (1u64 << 28, 5),  // 268435456 -> 5 bytes
            (1u64 << 35, 6),  // -> 6 bytes
            (1u64 << 42, 7),  // -> 7 bytes
            (1u64 << 49, 8),  // -> 8 bytes
            (1u64 << 56, 9),  // -> 9 bytes
            (1u64 << 63, 10), // -> 10 bytes
        ];
        for (value, expected_bytes) in boundaries {
            let mut buf = BytesMut::new();
            encode_varint_u64(&mut buf, value);
            assert_eq!(
                buf.len(),
                expected_bytes,
                "Value {} should encode to {} bytes, got {}",
                value,
                expected_bytes,
                buf.len()
            );
            let mut cursor = buf.as_ref();
            let decoded = decode_varint_u64(&mut cursor);
            assert_eq!(decoded, value);
        }
    }

    // ---------------------------------------------------------------
    // Multiple sequential varints in the same buffer
    // ---------------------------------------------------------------

    #[test]
    fn test_multiple_signed_varints_sequential() {
        let values: Vec<i64> = vec![0, 1, -1, 42, -42, 1_000_000, -1_000_000, i64::MAX, i64::MIN];
        let mut buf = BytesMut::new();
        for &v in &values {
            encode_varint(&mut buf, v);
        }
        let mut cursor = buf.as_ref();
        for &expected in &values {
            let decoded = decode_varint(&mut cursor);
            assert_eq!(decoded, expected);
        }
        // All bytes should have been consumed
        assert_eq!(cursor.len(), 0, "Buffer should be fully consumed");
    }

    #[test]
    fn test_multiple_unsigned_varints_sequential() {
        let values: Vec<u64> = vec![0, 1, 127, 128, 255, 256, 16383, 16384, u64::MAX];
        let mut buf = BytesMut::new();
        for &v in &values {
            encode_varint_u64(&mut buf, v);
        }
        let mut cursor = buf.as_ref();
        for &expected in &values {
            let decoded = decode_varint_u64(&mut cursor);
            assert_eq!(decoded, expected);
        }
        assert_eq!(cursor.len(), 0, "Buffer should be fully consumed");
    }

    #[test]
    fn test_many_sequential_small_varints() {
        let mut buf = BytesMut::new();
        for i in 0i64..1000 {
            encode_varint(&mut buf, i);
        }
        let mut cursor = buf.as_ref();
        for i in 0i64..1000 {
            let decoded = decode_varint(&mut cursor);
            assert_eq!(decoded, i);
        }
        assert_eq!(cursor.len(), 0);
    }

    #[test]
    fn test_alternating_signed_unsigned_sequential() {
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, -100);
        encode_varint_u64(&mut buf, 200);
        encode_varint(&mut buf, -300);
        encode_varint_u64(&mut buf, 400);

        let mut cursor = buf.as_ref();
        assert_eq!(decode_varint(&mut cursor), -100);
        assert_eq!(decode_varint_u64(&mut cursor), 200);
        assert_eq!(decode_varint(&mut cursor), -300);
        assert_eq!(decode_varint_u64(&mut cursor), 400);
        assert_eq!(cursor.len(), 0);
    }

    // ---------------------------------------------------------------
    // ZigZag encoding properties
    // ---------------------------------------------------------------

    #[test]
    fn test_zigzag_encoding_pattern() {
        // ZigZag maps:  0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, 2 -> 4, ...
        // We can verify by checking the unsigned encoding matches expected ZigZag values
        let pairs: Vec<(i64, u64)> = vec![
            (0, 0),
            (-1, 1),
            (1, 2),
            (-2, 3),
            (2, 4),
            (-3, 5),
            (3, 6),
            (2147483647, 4294967294),  // i32::MAX
            (-2147483648, 4294967295), // i32::MIN
        ];
        for (signed, expected_unsigned) in pairs {
            let zigzag = ((signed << 1) ^ (signed >> 63)) as u64;
            assert_eq!(
                zigzag, expected_unsigned,
                "ZigZag({}) should be {}, got {}",
                signed, expected_unsigned, zigzag
            );
        }
    }

    #[test]
    fn test_zigzag_small_negatives_are_compact() {
        // Small negative numbers should use few bytes thanks to ZigZag
        for val in [-1i64, -2, -3, -10, -63] {
            let mut buf = BytesMut::new();
            encode_varint(&mut buf, val);
            assert!(
                buf.len() <= 2,
                "Small negative {} should encode to at most 2 bytes, got {}",
                val,
                buf.len()
            );
        }
    }

    // ---------------------------------------------------------------
    // Byte-size expectations for unsigned varints
    // ---------------------------------------------------------------

    #[test]
    fn test_varint_u64_max_byte_count() {
        // u64::MAX = 2^64 - 1 should use 10 bytes
        let mut buf = BytesMut::new();
        encode_varint_u64(&mut buf, u64::MAX);
        assert_eq!(buf.len(), 10, "u64::MAX should encode to 10 bytes");
    }

    #[test]
    fn test_varint_i64_max_byte_count() {
        // i64::MAX zigzag-encodes to u64::MAX - 1 = 2^64 - 2
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, i64::MAX);
        assert_eq!(buf.len(), 10, "i64::MAX should encode to 10 bytes");
    }

    #[test]
    fn test_varint_i64_min_byte_count() {
        // i64::MIN zigzag-encodes to u64::MAX = 2^64 - 1
        let mut buf = BytesMut::new();
        encode_varint(&mut buf, i64::MIN);
        assert_eq!(buf.len(), 10, "i64::MIN should encode to 10 bytes");
    }

    // ---------------------------------------------------------------
    // Roundtrip: various notable values
    // ---------------------------------------------------------------

    #[test]
    fn test_varint_roundtrip_notable_signed_values() {
        let values: Vec<i64> = vec![
            0,
            1,
            -1,
            63,
            -64,
            64,
            -65,
            127,
            128,
            -128,
            -129,
            255,
            256,
            -256,
            i32::MAX as i64,
            i32::MIN as i64,
            i64::MAX,
            i64::MIN,
            i64::MAX / 2,
            i64::MIN / 2,
        ];
        for &val in &values {
            let mut buf = BytesMut::new();
            encode_varint(&mut buf, val);
            let mut cursor = buf.as_ref();
            let decoded = decode_varint(&mut cursor);
            assert_eq!(decoded, val, "Failed roundtrip for signed value {}", val);
        }
    }

    #[test]
    fn test_varint_roundtrip_notable_unsigned_values() {
        let values: Vec<u64> = vec![
            0,
            1,
            127,
            128,
            255,
            256,
            16383,
            16384,
            u32::MAX as u64,
            (u32::MAX as u64) + 1,
            u64::MAX / 2,
            u64::MAX - 1,
            u64::MAX,
        ];
        for &val in &values {
            let mut buf = BytesMut::new();
            encode_varint_u64(&mut buf, val);
            let mut cursor = buf.as_ref();
            let decoded = decode_varint_u64(&mut cursor);
            assert_eq!(decoded, val, "Failed roundtrip for unsigned value {}", val);
        }
    }

    // ---------------------------------------------------------------
    // Delta encoding simulation (typical StreamHouse use case)
    // ---------------------------------------------------------------

    #[test]
    fn test_delta_encoding_offsets() {
        // Simulate sequential offsets: 1000, 1001, 1002, ...
        // Deltas: 0, 1, 1, 1, ...
        let offsets: Vec<u64> = (1000..1100).collect();
        let mut deltas = Vec::new();
        deltas.push(offsets[0] as i64);
        for i in 1..offsets.len() {
            deltas.push((offsets[i] as i64) - (offsets[i - 1] as i64));
        }

        let mut buf = BytesMut::new();
        for &d in &deltas {
            encode_varint(&mut buf, d);
        }

        // Most deltas are 1, which should be very compact
        // First delta is 1000 (larger), rest are 1
        let mut cursor = buf.as_ref();
        for &expected in &deltas {
            let decoded = decode_varint(&mut cursor);
            assert_eq!(decoded, expected);
        }
        assert_eq!(cursor.len(), 0);
    }

    #[test]
    fn test_delta_encoding_timestamps() {
        // Simulate close timestamps with small negative and positive deltas
        let deltas: Vec<i64> = vec![1_700_000_000_000, 5, -2, 10, 0, -1, 3, 100, -50];
        let mut buf = BytesMut::new();
        for &d in &deltas {
            encode_varint(&mut buf, d);
        }
        let mut cursor = buf.as_ref();
        for &expected in &deltas {
            let decoded = decode_varint(&mut cursor);
            assert_eq!(decoded, expected);
        }
        assert_eq!(cursor.len(), 0);
    }
}
