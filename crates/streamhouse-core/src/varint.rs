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
}
