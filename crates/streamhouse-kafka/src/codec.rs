//! Kafka protocol frame codec
//!
//! Handles the length-prefixed framing of Kafka protocol messages.
//!
//! Frame format:
//! ```text
//! +------------------+------------------+
//! | Length (4 bytes) | Payload          |
//! +------------------+------------------+
//! ```

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::error::{KafkaError, KafkaResult};

/// Maximum frame size (100MB)
const MAX_FRAME_SIZE: usize = 100 * 1024 * 1024;

/// Kafka protocol frame codec
///
/// Handles encoding and decoding of length-prefixed Kafka protocol frames.
pub struct KafkaCodec {
    /// Maximum allowed frame size
    max_frame_size: usize,
}

impl Default for KafkaCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl KafkaCodec {
    pub fn new() -> Self {
        Self {
            max_frame_size: MAX_FRAME_SIZE,
        }
    }

    pub fn with_max_frame_size(max_frame_size: usize) -> Self {
        Self { max_frame_size }
    }
}

impl Decoder for KafkaCodec {
    type Item = BytesMut;
    type Error = KafkaError;

    fn decode(&mut self, src: &mut BytesMut) -> KafkaResult<Option<Self::Item>> {
        // Need at least 4 bytes for the length prefix
        if src.len() < 4 {
            return Ok(None);
        }

        // Read length without consuming
        let length = (&src[..4]).get_i32() as usize;

        // Validate frame size
        if length > self.max_frame_size {
            return Err(KafkaError::Protocol(format!(
                "Frame size {} exceeds maximum {}",
                length, self.max_frame_size
            )));
        }

        // Check if we have the full frame
        let total_length = 4 + length;
        if src.len() < total_length {
            // Reserve capacity for the full frame
            src.reserve(total_length - src.len());
            return Ok(None);
        }

        // Consume the length prefix
        src.advance(4);

        // Extract the payload
        let payload = src.split_to(length);

        Ok(Some(payload))
    }
}

impl Encoder<BytesMut> for KafkaCodec {
    type Error = KafkaError;

    fn encode(&mut self, item: BytesMut, dst: &mut BytesMut) -> KafkaResult<()> {
        let length = item.len();

        if length > self.max_frame_size {
            return Err(KafkaError::Protocol(format!(
                "Frame size {} exceeds maximum {}",
                length, self.max_frame_size
            )));
        }

        // Write length prefix
        dst.reserve(4 + length);
        dst.put_i32(length as i32);
        dst.extend_from_slice(&item);

        Ok(())
    }
}

/// Request header structure
#[derive(Debug, Clone)]
pub struct RequestHeader {
    pub api_key: i16,
    pub api_version: i16,
    pub correlation_id: i32,
    pub client_id: Option<String>,
}

impl RequestHeader {
    /// Parse request header from bytes
    pub fn parse(buf: &mut BytesMut) -> KafkaResult<Self> {
        if buf.len() < 8 {
            return Err(KafkaError::Protocol("Request header too short".to_string()));
        }

        let api_key = buf.get_i16();
        let api_version = buf.get_i16();
        let correlation_id = buf.get_i32();

        // Parse client_id (nullable string)
        let client_id = parse_nullable_string(buf)?;

        Ok(RequestHeader {
            api_key,
            api_version,
            correlation_id,
            client_id,
        })
    }
}

/// Response header structure
#[derive(Debug, Clone)]
pub struct ResponseHeader {
    pub correlation_id: i32,
}

impl ResponseHeader {
    pub fn new(correlation_id: i32) -> Self {
        Self { correlation_id }
    }

    /// Encode response header to bytes
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_i32(self.correlation_id);
    }
}

/// Parse a nullable string (int16 length + bytes, -1 for null)
pub fn parse_nullable_string(buf: &mut BytesMut) -> KafkaResult<Option<String>> {
    if buf.len() < 2 {
        return Err(KafkaError::Protocol(
            "Buffer too short for string".to_string(),
        ));
    }

    let length = buf.get_i16();

    if length < 0 {
        return Ok(None);
    }

    let length = length as usize;
    if buf.len() < length {
        return Err(KafkaError::Protocol(format!(
            "Buffer too short for string of length {}",
            length
        )));
    }

    let bytes = buf.split_to(length);
    let s = String::from_utf8(bytes.to_vec())
        .map_err(|e| KafkaError::Protocol(format!("Invalid UTF-8 in string: {}", e)))?;

    Ok(Some(s))
}

/// Parse a compact nullable string (unsigned varint length + bytes)
pub fn parse_compact_nullable_string(buf: &mut BytesMut) -> KafkaResult<Option<String>> {
    let length = parse_unsigned_varint(buf)?;

    if length == 0 {
        return Ok(None);
    }

    let length = length as usize - 1; // Compact strings use length + 1

    if buf.len() < length {
        return Err(KafkaError::Protocol(format!(
            "Buffer too short for compact string of length {}",
            length
        )));
    }

    let bytes = buf.split_to(length);
    let s = String::from_utf8(bytes.to_vec())
        .map_err(|e| KafkaError::Protocol(format!("Invalid UTF-8 in compact string: {}", e)))?;

    Ok(Some(s))
}

/// Parse a string (int16 length + bytes)
pub fn parse_string(buf: &mut BytesMut) -> KafkaResult<String> {
    parse_nullable_string(buf)?
        .ok_or_else(|| KafkaError::Protocol("Expected non-null string".to_string()))
}

/// Parse a compact string (unsigned varint length + bytes)
pub fn parse_compact_string(buf: &mut BytesMut) -> KafkaResult<String> {
    parse_compact_nullable_string(buf)?
        .ok_or_else(|| KafkaError::Protocol("Expected non-null compact string".to_string()))
}

/// Parse nullable bytes (int32 length + bytes, -1 for null)
pub fn parse_nullable_bytes(buf: &mut BytesMut) -> KafkaResult<Option<Vec<u8>>> {
    if buf.len() < 4 {
        return Err(KafkaError::Protocol(
            "Buffer too short for bytes".to_string(),
        ));
    }

    let length = buf.get_i32();

    if length < 0 {
        return Ok(None);
    }

    let length = length as usize;
    if buf.len() < length {
        return Err(KafkaError::Protocol(format!(
            "Buffer too short for bytes of length {}",
            length
        )));
    }

    let bytes = buf.split_to(length).to_vec();
    Ok(Some(bytes))
}

/// Parse compact nullable bytes (unsigned varint length + bytes)
pub fn parse_compact_nullable_bytes(buf: &mut BytesMut) -> KafkaResult<Option<Vec<u8>>> {
    let length = parse_unsigned_varint(buf)?;

    if length == 0 {
        return Ok(None);
    }

    let length = length as usize - 1;

    if buf.len() < length {
        return Err(KafkaError::Protocol(format!(
            "Buffer too short for compact bytes of length {}",
            length
        )));
    }

    let bytes = buf.split_to(length).to_vec();
    Ok(Some(bytes))
}

/// Parse an array (int32 count + elements)
pub fn parse_array<T, F>(buf: &mut BytesMut, parse_element: F) -> KafkaResult<Vec<T>>
where
    F: Fn(&mut BytesMut) -> KafkaResult<T>,
{
    if buf.len() < 4 {
        return Err(KafkaError::Protocol(
            "Buffer too short for array".to_string(),
        ));
    }

    let count = buf.get_i32();

    if count < 0 {
        return Ok(vec![]);
    }

    let count = count as usize;
    let mut elements = Vec::with_capacity(count);

    for _ in 0..count {
        elements.push(parse_element(buf)?);
    }

    Ok(elements)
}

/// Parse a compact array (unsigned varint count + elements)
pub fn parse_compact_array<T, F>(buf: &mut BytesMut, parse_element: F) -> KafkaResult<Vec<T>>
where
    F: Fn(&mut BytesMut) -> KafkaResult<T>,
{
    let count = parse_unsigned_varint(buf)?;

    if count == 0 {
        return Ok(vec![]);
    }

    let count = count as usize - 1;
    let mut elements = Vec::with_capacity(count);

    for _ in 0..count {
        elements.push(parse_element(buf)?);
    }

    Ok(elements)
}

/// Parse an unsigned varint
pub fn parse_unsigned_varint(buf: &mut BytesMut) -> KafkaResult<u64> {
    let mut result: u64 = 0;
    let mut shift = 0;

    loop {
        if buf.is_empty() {
            return Err(KafkaError::Protocol(
                "Buffer too short for varint".to_string(),
            ));
        }

        let byte = buf.get_u8();
        result |= ((byte & 0x7F) as u64) << shift;

        if byte & 0x80 == 0 {
            break;
        }

        shift += 7;
        if shift >= 64 {
            return Err(KafkaError::Protocol("Varint too long".to_string()));
        }
    }

    Ok(result)
}

/// Parse a signed varint (zigzag encoded)
pub fn parse_signed_varint(buf: &mut BytesMut) -> KafkaResult<i64> {
    let unsigned = parse_unsigned_varint(buf)?;
    // Zigzag decode
    Ok(((unsigned >> 1) as i64) ^ (-((unsigned & 1) as i64)))
}

/// Encode a nullable string
pub fn encode_nullable_string(buf: &mut BytesMut, s: Option<&str>) {
    match s {
        Some(s) => {
            buf.put_i16(s.len() as i16);
            buf.extend_from_slice(s.as_bytes());
        }
        None => {
            buf.put_i16(-1);
        }
    }
}

/// Encode a compact nullable string
pub fn encode_compact_nullable_string(buf: &mut BytesMut, s: Option<&str>) {
    match s {
        Some(s) => {
            encode_unsigned_varint(buf, (s.len() + 1) as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        None => {
            encode_unsigned_varint(buf, 0);
        }
    }
}

/// Encode a string
pub fn encode_string(buf: &mut BytesMut, s: &str) {
    buf.put_i16(s.len() as i16);
    buf.extend_from_slice(s.as_bytes());
}

/// Encode a compact string
pub fn encode_compact_string(buf: &mut BytesMut, s: &str) {
    encode_unsigned_varint(buf, (s.len() + 1) as u64);
    buf.extend_from_slice(s.as_bytes());
}

/// Encode nullable bytes
pub fn encode_nullable_bytes(buf: &mut BytesMut, bytes: Option<&[u8]>) {
    match bytes {
        Some(b) => {
            buf.put_i32(b.len() as i32);
            buf.extend_from_slice(b);
        }
        None => {
            buf.put_i32(-1);
        }
    }
}

/// Encode compact nullable bytes
pub fn encode_compact_nullable_bytes(buf: &mut BytesMut, bytes: Option<&[u8]>) {
    match bytes {
        Some(b) => {
            encode_unsigned_varint(buf, (b.len() + 1) as u64);
            buf.extend_from_slice(b);
        }
        None => {
            encode_unsigned_varint(buf, 0);
        }
    }
}

/// Encode an unsigned varint
pub fn encode_unsigned_varint(buf: &mut BytesMut, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;

        if value != 0 {
            byte |= 0x80;
        }

        buf.put_u8(byte);

        if value == 0 {
            break;
        }
    }
}

/// Encode a signed varint (zigzag encoded)
pub fn encode_signed_varint(buf: &mut BytesMut, value: i64) {
    // Zigzag encode
    let unsigned = ((value << 1) ^ (value >> 63)) as u64;
    encode_unsigned_varint(buf, unsigned);
}

/// Skip tagged fields (compact protocol)
pub fn skip_tagged_fields(buf: &mut BytesMut) -> KafkaResult<()> {
    let count = parse_unsigned_varint(buf)?;
    for _ in 0..count {
        let _tag = parse_unsigned_varint(buf)?;
        let size = parse_unsigned_varint(buf)? as usize;
        if buf.len() < size {
            return Err(KafkaError::Protocol(
                "Buffer too short for tagged field".to_string(),
            ));
        }
        buf.advance(size);
    }
    Ok(())
}

/// Encode empty tagged fields
pub fn encode_empty_tagged_fields(buf: &mut BytesMut) {
    encode_unsigned_varint(buf, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===============================================================
    // KafkaCodec frame-level encode/decode tests
    // ===============================================================

    #[test]
    fn test_codec_roundtrip_simple() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        let payload = BytesMut::from(&b"hello world"[..]);
        codec.encode(payload.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_codec_roundtrip_empty_payload() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        let payload = BytesMut::new();
        codec.encode(payload.clone(), &mut buf).unwrap();

        // Should have exactly 4 bytes (length prefix of 0)
        assert_eq!(buf.len(), 4);

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, payload);
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_codec_decode_incomplete_length() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        // Only 2 bytes, not enough for the 4-byte length prefix
        buf.put_u8(0);
        buf.put_u8(0);

        let result = codec.decode(&mut buf).unwrap();
        assert!(
            result.is_none(),
            "Should return None when not enough bytes for length"
        );
    }

    #[test]
    fn test_codec_decode_incomplete_payload() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        // Length prefix says 100 bytes, but we only provide 10
        buf.put_i32(100);
        buf.extend_from_slice(&[0u8; 10]);

        let result = codec.decode(&mut buf).unwrap();
        assert!(
            result.is_none(),
            "Should return None when payload is incomplete"
        );
    }

    #[test]
    fn test_codec_decode_frame_too_large() {
        let mut codec = KafkaCodec::with_max_frame_size(1024);
        let mut buf = BytesMut::new();

        // Length prefix says 2048 bytes, exceeds max
        buf.put_i32(2048);

        let result = codec.decode(&mut buf);
        assert!(result.is_err(), "Should error on oversized frame");
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("exceeds maximum"));
    }

    #[test]
    fn test_codec_encode_frame_too_large() {
        let mut codec = KafkaCodec::with_max_frame_size(16);
        let mut dst = BytesMut::new();

        // Payload larger than max
        let payload = BytesMut::from(&[0u8; 32][..]);
        let result = codec.encode(payload, &mut dst);
        assert!(result.is_err(), "Should error on oversized payload");
    }

    #[test]
    fn test_codec_multiple_frames() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        let p1 = BytesMut::from(&b"frame one"[..]);
        let p2 = BytesMut::from(&b"frame two"[..]);
        let p3 = BytesMut::from(&b"frame three"[..]);

        codec.encode(p1.clone(), &mut buf).unwrap();
        codec.encode(p2.clone(), &mut buf).unwrap();
        codec.encode(p3.clone(), &mut buf).unwrap();

        let d1 = codec.decode(&mut buf).unwrap().unwrap();
        let d2 = codec.decode(&mut buf).unwrap().unwrap();
        let d3 = codec.decode(&mut buf).unwrap().unwrap();

        assert_eq!(d1, p1);
        assert_eq!(d2, p2);
        assert_eq!(d3, p3);

        // Buffer should be empty now
        assert!(buf.is_empty());
    }

    #[test]
    fn test_codec_custom_max_frame_size() {
        let codec = KafkaCodec::with_max_frame_size(512);
        // Verify it was created (no public accessor, but we can test behavior)
        let mut c = codec;
        let mut buf = BytesMut::new();
        let payload = BytesMut::from(&[0u8; 256][..]);
        c.encode(payload, &mut buf).unwrap(); // 256 < 512, should succeed
    }

    #[test]
    fn test_codec_default() {
        let codec = KafkaCodec::default();
        let mut c = codec;
        let mut buf = BytesMut::new();
        let payload = BytesMut::from(&b"test"[..]);
        c.encode(payload, &mut buf).unwrap();
    }

    #[test]
    fn test_codec_encode_length_prefix_is_correct() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        let payload = BytesMut::from(&b"12345"[..]);
        codec.encode(payload, &mut buf).unwrap();

        // First 4 bytes should be the length = 5
        let length = (&buf[..4]).get_i32();
        assert_eq!(length, 5);
        assert_eq!(&buf[4..], b"12345");
    }

    #[test]
    fn test_codec_roundtrip_binary_data() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        // Binary data with all byte values
        let payload_data: Vec<u8> = (0..=255).collect();
        let payload = BytesMut::from(&payload_data[..]);

        codec.encode(payload.clone(), &mut buf).unwrap();
        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, payload);
    }

    // ===============================================================
    // RequestHeader tests
    // ===============================================================

    #[test]
    fn test_request_header_parse_basic() {
        let mut buf = BytesMut::new();
        buf.put_i16(3); // api_key = Metadata
        buf.put_i16(1); // api_version = 1
        buf.put_i32(42); // correlation_id = 42
                         // client_id = "my-client"
        let client_id = "my-client";
        buf.put_i16(client_id.len() as i16);
        buf.extend_from_slice(client_id.as_bytes());

        let header = RequestHeader::parse(&mut buf).unwrap();
        assert_eq!(header.api_key, 3);
        assert_eq!(header.api_version, 1);
        assert_eq!(header.correlation_id, 42);
        assert_eq!(header.client_id, Some("my-client".to_string()));
    }

    #[test]
    fn test_request_header_parse_null_client_id() {
        let mut buf = BytesMut::new();
        buf.put_i16(0); // api_key = Produce
        buf.put_i16(0); // api_version = 0
        buf.put_i32(1); // correlation_id = 1
        buf.put_i16(-1); // null client_id

        let header = RequestHeader::parse(&mut buf).unwrap();
        assert_eq!(header.api_key, 0);
        assert_eq!(header.api_version, 0);
        assert_eq!(header.correlation_id, 1);
        assert_eq!(header.client_id, None);
    }

    #[test]
    fn test_request_header_parse_empty_client_id() {
        let mut buf = BytesMut::new();
        buf.put_i16(18); // api_key = ApiVersions
        buf.put_i16(3); // api_version = 3
        buf.put_i32(99); // correlation_id = 99
        buf.put_i16(0); // empty string client_id (length = 0)

        let header = RequestHeader::parse(&mut buf).unwrap();
        assert_eq!(header.api_key, 18);
        assert_eq!(header.api_version, 3);
        assert_eq!(header.correlation_id, 99);
        assert_eq!(header.client_id, Some("".to_string()));
    }

    #[test]
    fn test_request_header_parse_too_short() {
        let mut buf = BytesMut::new();
        buf.put_i16(0); // only 2 bytes, need at least 8

        let result = RequestHeader::parse(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_request_header_parse_negative_correlation() {
        let mut buf = BytesMut::new();
        buf.put_i16(1); // api_key
        buf.put_i16(0); // api_version
        buf.put_i32(-1); // correlation_id (negative is valid in protocol)
        buf.put_i16(-1); // null client_id

        let header = RequestHeader::parse(&mut buf).unwrap();
        assert_eq!(header.correlation_id, -1);
    }

    // ===============================================================
    // ResponseHeader tests
    // ===============================================================

    #[test]
    fn test_response_header_encode() {
        let header = ResponseHeader::new(42);
        let mut buf = BytesMut::new();
        header.encode(&mut buf);

        assert_eq!(buf.len(), 4);
        assert_eq!((&buf[..]).get_i32(), 42);
    }

    #[test]
    fn test_response_header_encode_zero() {
        let header = ResponseHeader::new(0);
        let mut buf = BytesMut::new();
        header.encode(&mut buf);
        assert_eq!((&buf[..]).get_i32(), 0);
    }

    #[test]
    fn test_response_header_encode_negative() {
        let header = ResponseHeader::new(-1);
        let mut buf = BytesMut::new();
        header.encode(&mut buf);
        assert_eq!((&buf[..]).get_i32(), -1);
    }

    #[test]
    fn test_response_header_encode_max() {
        let header = ResponseHeader::new(i32::MAX);
        let mut buf = BytesMut::new();
        header.encode(&mut buf);
        assert_eq!((&buf[..]).get_i32(), i32::MAX);
    }

    // ===============================================================
    // Nullable string encode/decode roundtrip tests
    // ===============================================================

    #[test]
    fn test_nullable_string_roundtrip_some() {
        let mut buf = BytesMut::new();
        encode_nullable_string(&mut buf, Some("hello kafka"));
        let result = parse_nullable_string(&mut buf).unwrap();
        assert_eq!(result, Some("hello kafka".to_string()));
    }

    #[test]
    fn test_nullable_string_roundtrip_none() {
        let mut buf = BytesMut::new();
        encode_nullable_string(&mut buf, None);

        // Should be exactly 2 bytes (i16 of -1)
        assert_eq!(buf.len(), 2);

        let result = parse_nullable_string(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_nullable_string_roundtrip_empty() {
        let mut buf = BytesMut::new();
        encode_nullable_string(&mut buf, Some(""));
        let result = parse_nullable_string(&mut buf).unwrap();
        assert_eq!(result, Some("".to_string()));
    }

    #[test]
    fn test_nullable_string_roundtrip_unicode() {
        let mut buf = BytesMut::new();
        let s = "hello world \u{1F600}";
        encode_nullable_string(&mut buf, Some(s));
        let result = parse_nullable_string(&mut buf).unwrap();
        assert_eq!(result, Some(s.to_string()));
    }

    #[test]
    fn test_parse_nullable_string_buffer_too_short_for_length() {
        let mut buf = BytesMut::new();
        buf.put_u8(0); // only 1 byte, need 2
        let result = parse_nullable_string(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_nullable_string_buffer_too_short_for_payload() {
        let mut buf = BytesMut::new();
        buf.put_i16(10); // says 10 bytes follow
        buf.extend_from_slice(b"short"); // only 5 bytes
        let result = parse_nullable_string(&mut buf);
        assert!(result.is_err());
    }

    // ===============================================================
    // Non-nullable string encode/decode roundtrip tests
    // ===============================================================

    #[test]
    fn test_string_roundtrip() {
        let mut buf = BytesMut::new();
        encode_string(&mut buf, "hello");
        assert_eq!(parse_string(&mut buf).unwrap(), "hello");
    }

    #[test]
    fn test_string_roundtrip_empty() {
        let mut buf = BytesMut::new();
        encode_string(&mut buf, "");
        assert_eq!(parse_string(&mut buf).unwrap(), "");
    }

    #[test]
    fn test_parse_string_null_errors() {
        // parse_string expects non-null; encoding a null should cause error
        let mut buf = BytesMut::new();
        encode_nullable_string(&mut buf, None);
        let result = parse_string(&mut buf);
        assert!(result.is_err());
    }

    // ===============================================================
    // Compact string encode/decode roundtrip tests
    // ===============================================================

    #[test]
    fn test_compact_nullable_string_roundtrip_some() {
        let mut buf = BytesMut::new();
        encode_compact_nullable_string(&mut buf, Some("compact test"));
        let result = parse_compact_nullable_string(&mut buf).unwrap();
        assert_eq!(result, Some("compact test".to_string()));
    }

    #[test]
    fn test_compact_nullable_string_roundtrip_none() {
        let mut buf = BytesMut::new();
        encode_compact_nullable_string(&mut buf, None);
        let result = parse_compact_nullable_string(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_compact_nullable_string_roundtrip_empty() {
        let mut buf = BytesMut::new();
        encode_compact_nullable_string(&mut buf, Some(""));
        let result = parse_compact_nullable_string(&mut buf).unwrap();
        assert_eq!(result, Some("".to_string()));
    }

    #[test]
    fn test_compact_string_roundtrip() {
        let mut buf = BytesMut::new();
        encode_compact_string(&mut buf, "compact non-null");
        let result = parse_compact_string(&mut buf).unwrap();
        assert_eq!(result, "compact non-null");
    }

    #[test]
    fn test_compact_string_roundtrip_empty() {
        let mut buf = BytesMut::new();
        encode_compact_string(&mut buf, "");
        let result = parse_compact_string(&mut buf).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_parse_compact_string_null_errors() {
        let mut buf = BytesMut::new();
        encode_compact_nullable_string(&mut buf, None);
        let result = parse_compact_string(&mut buf);
        assert!(result.is_err());
    }

    // ===============================================================
    // Nullable bytes encode/decode roundtrip tests
    // ===============================================================

    #[test]
    fn test_nullable_bytes_roundtrip_some() {
        let mut buf = BytesMut::new();
        let data = b"binary data \x00\x01\xFF";
        encode_nullable_bytes(&mut buf, Some(data));
        let result = parse_nullable_bytes(&mut buf).unwrap();
        assert_eq!(result, Some(data.to_vec()));
    }

    #[test]
    fn test_nullable_bytes_roundtrip_none() {
        let mut buf = BytesMut::new();
        encode_nullable_bytes(&mut buf, None);
        let result = parse_nullable_bytes(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_nullable_bytes_roundtrip_empty() {
        let mut buf = BytesMut::new();
        encode_nullable_bytes(&mut buf, Some(b""));
        let result = parse_nullable_bytes(&mut buf).unwrap();
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn test_parse_nullable_bytes_buffer_too_short_for_length() {
        let mut buf = BytesMut::new();
        buf.put_u8(0);
        buf.put_u8(0); // only 2 bytes, need 4 for i32 length
        let result = parse_nullable_bytes(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_nullable_bytes_buffer_too_short_for_payload() {
        let mut buf = BytesMut::new();
        buf.put_i32(100);
        buf.extend_from_slice(&[0u8; 10]); // only 10 bytes, need 100
        let result = parse_nullable_bytes(&mut buf);
        assert!(result.is_err());
    }

    // ===============================================================
    // Compact nullable bytes encode/decode roundtrip tests
    // ===============================================================

    #[test]
    fn test_compact_nullable_bytes_roundtrip_some() {
        let mut buf = BytesMut::new();
        let data = vec![1, 2, 3, 4, 5];
        encode_compact_nullable_bytes(&mut buf, Some(&data));
        let result = parse_compact_nullable_bytes(&mut buf).unwrap();
        assert_eq!(result, Some(data));
    }

    #[test]
    fn test_compact_nullable_bytes_roundtrip_none() {
        let mut buf = BytesMut::new();
        encode_compact_nullable_bytes(&mut buf, None);
        let result = parse_compact_nullable_bytes(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_compact_nullable_bytes_roundtrip_empty() {
        let mut buf = BytesMut::new();
        encode_compact_nullable_bytes(&mut buf, Some(b""));
        let result = parse_compact_nullable_bytes(&mut buf).unwrap();
        assert_eq!(result, Some(vec![]));
    }

    // ===============================================================
    // Unsigned varint encode/decode roundtrip tests
    // ===============================================================

    #[test]
    fn test_unsigned_varint_roundtrip_zero() {
        let mut buf = BytesMut::new();
        encode_unsigned_varint(&mut buf, 0);
        assert_eq!(buf.len(), 1); // Zero fits in 1 byte
        assert_eq!(parse_unsigned_varint(&mut buf).unwrap(), 0);
    }

    #[test]
    fn test_unsigned_varint_roundtrip_small() {
        for value in [1u64, 63, 127] {
            let mut buf = BytesMut::new();
            encode_unsigned_varint(&mut buf, value);
            assert_eq!(buf.len(), 1, "Value {} should encode in 1 byte", value);
            assert_eq!(parse_unsigned_varint(&mut buf).unwrap(), value);
        }
    }

    #[test]
    fn test_unsigned_varint_roundtrip_medium() {
        for value in [128u64, 255, 300, 16383] {
            let mut buf = BytesMut::new();
            encode_unsigned_varint(&mut buf, value);
            assert_eq!(buf.len(), 2, "Value {} should encode in 2 bytes", value);
            assert_eq!(parse_unsigned_varint(&mut buf).unwrap(), value);
        }
    }

    #[test]
    fn test_unsigned_varint_roundtrip_large() {
        let values = [16384u64, 2097151, 1_000_000, u64::MAX];
        for value in values {
            let mut buf = BytesMut::new();
            encode_unsigned_varint(&mut buf, value);
            let decoded = parse_unsigned_varint(&mut buf).unwrap();
            assert_eq!(decoded, value, "Roundtrip failed for {}", value);
        }
    }

    #[test]
    fn test_unsigned_varint_roundtrip_boundary_values() {
        // Test at byte boundaries: 2^7-1, 2^7, 2^14-1, 2^14, 2^21-1, 2^21
        let boundaries = [
            (0x7F, 1),
            (0x80, 2),
            (0x3FFF, 2),
            (0x4000, 3),
            (0x1FFFFF, 3),
            (0x200000, 4),
        ];
        for (value, expected_bytes) in boundaries {
            let mut buf = BytesMut::new();
            encode_unsigned_varint(&mut buf, value);
            assert_eq!(
                buf.len(),
                expected_bytes,
                "Value {} should encode in {} bytes, got {}",
                value,
                expected_bytes,
                buf.len()
            );
            assert_eq!(parse_unsigned_varint(&mut buf).unwrap(), value);
        }
    }

    #[test]
    fn test_parse_unsigned_varint_empty_buffer() {
        let mut buf = BytesMut::new();
        let result = parse_unsigned_varint(&mut buf);
        assert!(result.is_err());
    }

    // ===============================================================
    // Signed varint (zigzag) encode/decode roundtrip tests
    // ===============================================================

    #[test]
    fn test_signed_varint_roundtrip_zero() {
        let mut buf = BytesMut::new();
        encode_signed_varint(&mut buf, 0);
        assert_eq!(parse_signed_varint(&mut buf).unwrap(), 0);
    }

    #[test]
    fn test_signed_varint_roundtrip_positive() {
        for value in [1i64, 42, 127, 300, 10000, i64::MAX] {
            let mut buf = BytesMut::new();
            encode_signed_varint(&mut buf, value);
            let decoded = parse_signed_varint(&mut buf).unwrap();
            assert_eq!(decoded, value, "Roundtrip failed for {}", value);
        }
    }

    #[test]
    fn test_signed_varint_roundtrip_negative() {
        for value in [-1i64, -42, -128, -300, -10000, i64::MIN] {
            let mut buf = BytesMut::new();
            encode_signed_varint(&mut buf, value);
            let decoded = parse_signed_varint(&mut buf).unwrap();
            assert_eq!(decoded, value, "Roundtrip failed for {}", value);
        }
    }

    #[test]
    fn test_signed_varint_zigzag_encoding() {
        // Zigzag: 0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, 2 -> 4
        let mut buf = BytesMut::new();
        encode_signed_varint(&mut buf, 0);
        // Zigzag(0) = 0, unsigned varint of 0 = [0x00]
        assert_eq!(buf[0], 0x00);

        let mut buf = BytesMut::new();
        encode_signed_varint(&mut buf, -1);
        // Zigzag(-1) = 1, unsigned varint of 1 = [0x01]
        assert_eq!(buf[0], 0x01);

        let mut buf = BytesMut::new();
        encode_signed_varint(&mut buf, 1);
        // Zigzag(1) = 2, unsigned varint of 2 = [0x02]
        assert_eq!(buf[0], 0x02);

        let mut buf = BytesMut::new();
        encode_signed_varint(&mut buf, -2);
        // Zigzag(-2) = 3, unsigned varint of 3 = [0x03]
        assert_eq!(buf[0], 0x03);
    }

    // ===============================================================
    // Array parse tests
    // ===============================================================

    #[test]
    fn test_parse_array_of_i32() {
        let mut buf = BytesMut::new();
        buf.put_i32(3); // count = 3
        buf.put_i32(10);
        buf.put_i32(20);
        buf.put_i32(30);

        let result = parse_array(&mut buf, |b| {
            if b.len() < 4 {
                return Err(KafkaError::Protocol("too short".to_string()));
            }
            Ok(b.get_i32())
        })
        .unwrap();

        assert_eq!(result, vec![10, 20, 30]);
    }

    #[test]
    fn test_parse_array_empty() {
        let mut buf = BytesMut::new();
        buf.put_i32(0); // count = 0

        let result = parse_array(&mut buf, |b| Ok(b.get_i32())).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_array_negative_count_returns_empty() {
        let mut buf = BytesMut::new();
        buf.put_i32(-1); // null array

        let result = parse_array(&mut buf, |b| Ok(b.get_i32())).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_array_buffer_too_short() {
        let mut buf = BytesMut::new();
        buf.put_u8(0); // only 1 byte, need 4

        let result = parse_array(&mut buf, |b| Ok(b.get_i32()));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_array_of_strings() {
        let mut buf = BytesMut::new();
        buf.put_i32(2); // count = 2
        encode_string(&mut buf, "alpha");
        encode_string(&mut buf, "beta");

        let result = parse_array(&mut buf, |b| parse_string(b)).unwrap();
        assert_eq!(result, vec!["alpha".to_string(), "beta".to_string()]);
    }

    // ===============================================================
    // Compact array parse tests
    // ===============================================================

    #[test]
    fn test_parse_compact_array_basic() {
        let mut buf = BytesMut::new();
        // compact array count is varint(n+1), so for 2 elements, write varint(3)
        encode_unsigned_varint(&mut buf, 3);
        buf.put_i32(100);
        buf.put_i32(200);

        let result = parse_compact_array(&mut buf, |b| {
            if b.len() < 4 {
                return Err(KafkaError::Protocol("too short".to_string()));
            }
            Ok(b.get_i32())
        })
        .unwrap();

        assert_eq!(result, vec![100, 200]);
    }

    #[test]
    fn test_parse_compact_array_empty() {
        let mut buf = BytesMut::new();
        // compact array count = varint(0) means null/empty
        encode_unsigned_varint(&mut buf, 0);

        let result = parse_compact_array(&mut buf, |b| Ok(b.get_i32())).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_compact_array_single_element() {
        let mut buf = BytesMut::new();
        // 1 element = varint(2)
        encode_unsigned_varint(&mut buf, 2);
        buf.put_i32(42);

        let result = parse_compact_array(&mut buf, |b| {
            if b.len() < 4 {
                return Err(KafkaError::Protocol("too short".to_string()));
            }
            Ok(b.get_i32())
        })
        .unwrap();

        assert_eq!(result, vec![42]);
    }

    // ===============================================================
    // Tagged fields tests
    // ===============================================================

    #[test]
    fn test_skip_tagged_fields_empty() {
        let mut buf = BytesMut::new();
        encode_unsigned_varint(&mut buf, 0); // 0 tagged fields

        skip_tagged_fields(&mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_skip_tagged_fields_with_data() {
        let mut buf = BytesMut::new();
        encode_unsigned_varint(&mut buf, 2); // 2 tagged fields

        // Field 1: tag=0, size=3, data=[1,2,3]
        encode_unsigned_varint(&mut buf, 0);
        encode_unsigned_varint(&mut buf, 3);
        buf.extend_from_slice(&[1, 2, 3]);

        // Field 2: tag=1, size=2, data=[4,5]
        encode_unsigned_varint(&mut buf, 1);
        encode_unsigned_varint(&mut buf, 2);
        buf.extend_from_slice(&[4, 5]);

        skip_tagged_fields(&mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_skip_tagged_fields_buffer_too_short() {
        let mut buf = BytesMut::new();
        encode_unsigned_varint(&mut buf, 1); // 1 tagged field
        encode_unsigned_varint(&mut buf, 0); // tag=0
        encode_unsigned_varint(&mut buf, 100); // size=100
                                               // but no data follows

        let result = skip_tagged_fields(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_empty_tagged_fields() {
        let mut buf = BytesMut::new();
        encode_empty_tagged_fields(&mut buf);

        // Should just be varint(0) = single byte 0x00
        assert_eq!(buf.len(), 1);
        assert_eq!(buf[0], 0x00);
    }

    // ===============================================================
    // Multiple sequential encode/decode operations on same buffer
    // ===============================================================

    #[test]
    fn test_mixed_types_sequential() {
        let mut buf = BytesMut::new();

        // Encode a mix of types
        encode_string(&mut buf, "topic-name");
        buf.put_i32(42);
        encode_nullable_string(&mut buf, None);
        encode_nullable_bytes(&mut buf, Some(b"\xDE\xAD"));
        encode_signed_varint(&mut buf, -999);

        // Decode in same order
        assert_eq!(parse_string(&mut buf).unwrap(), "topic-name");
        assert_eq!(buf.get_i32(), 42);
        assert_eq!(parse_nullable_string(&mut buf).unwrap(), None);
        assert_eq!(
            parse_nullable_bytes(&mut buf).unwrap(),
            Some(vec![0xDE, 0xAD])
        );
        assert_eq!(parse_signed_varint(&mut buf).unwrap(), -999);

        assert!(buf.is_empty());
    }

    #[test]
    fn test_multiple_strings_sequential() {
        let mut buf = BytesMut::new();

        let strings = vec!["", "a", "hello", "world", "a longer string with spaces"];
        for s in &strings {
            encode_string(&mut buf, s);
        }
        for expected in &strings {
            let parsed = parse_string(&mut buf).unwrap();
            assert_eq!(&parsed, expected);
        }
        assert!(buf.is_empty());
    }

    #[test]
    fn test_many_varints_sequential() {
        let mut buf = BytesMut::new();
        let values: Vec<u64> = vec![0, 1, 127, 128, 16383, 16384, 2097151, 268435455, u64::MAX];

        for &v in &values {
            encode_unsigned_varint(&mut buf, v);
        }
        for &expected in &values {
            let parsed = parse_unsigned_varint(&mut buf).unwrap();
            assert_eq!(parsed, expected);
        }
        assert!(buf.is_empty());
    }

    // ===============================================================
    // Compact string with unicode / special characters
    // ===============================================================

    #[test]
    fn test_compact_string_unicode() {
        let mut buf = BytesMut::new();
        let s = "Caf\u{00E9} \u{2603} \u{1F4A9}";
        encode_compact_string(&mut buf, s);
        let result = parse_compact_string(&mut buf).unwrap();
        assert_eq!(result, s);
    }

    #[test]
    fn test_nullable_string_long() {
        let mut buf = BytesMut::new();
        let long_str: String = "x".repeat(10_000);
        encode_nullable_string(&mut buf, Some(&long_str));
        let result = parse_nullable_string(&mut buf).unwrap();
        assert_eq!(result, Some(long_str));
    }

    // ===============================================================
    // Edge case: parse_compact_nullable_string buffer too short
    // ===============================================================

    #[test]
    fn test_parse_compact_nullable_string_buffer_too_short() {
        let mut buf = BytesMut::new();
        // varint says length+1 = 100, but no data follows
        encode_unsigned_varint(&mut buf, 100);
        let result = parse_compact_nullable_string(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_compact_nullable_bytes_buffer_too_short() {
        let mut buf = BytesMut::new();
        encode_unsigned_varint(&mut buf, 50); // length+1=50, actual=49 bytes needed
        buf.extend_from_slice(&[0u8; 5]); // only 5 bytes
        let result = parse_compact_nullable_bytes(&mut buf);
        assert!(result.is_err());
    }

    // ===============================================================
    // Varint edge: too long
    // ===============================================================

    #[test]
    fn test_parse_unsigned_varint_too_long() {
        let mut buf = BytesMut::new();
        // Write 10+ continuation bytes (each with high bit set)
        for _ in 0..11 {
            buf.put_u8(0x80);
        }
        let result = parse_unsigned_varint(&mut buf);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("Varint too long"));
    }
}
