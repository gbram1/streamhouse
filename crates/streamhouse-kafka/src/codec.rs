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
            return Err(KafkaError::Protocol(
                "Request header too short".to_string(),
            ));
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
        return Err(KafkaError::Protocol("Buffer too short for string".to_string()));
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
    parse_nullable_string(buf)?.ok_or_else(|| KafkaError::Protocol("Expected non-null string".to_string()))
}

/// Parse a compact string (unsigned varint length + bytes)
pub fn parse_compact_string(buf: &mut BytesMut) -> KafkaResult<String> {
    parse_compact_nullable_string(buf)?.ok_or_else(|| KafkaError::Protocol("Expected non-null compact string".to_string()))
}

/// Parse nullable bytes (int32 length + bytes, -1 for null)
pub fn parse_nullable_bytes(buf: &mut BytesMut) -> KafkaResult<Option<Vec<u8>>> {
    if buf.len() < 4 {
        return Err(KafkaError::Protocol("Buffer too short for bytes".to_string()));
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
        return Err(KafkaError::Protocol("Buffer too short for array".to_string()));
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
            return Err(KafkaError::Protocol("Buffer too short for varint".to_string()));
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
            return Err(KafkaError::Protocol("Buffer too short for tagged field".to_string()));
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

    #[test]
    fn test_codec_roundtrip() {
        let mut codec = KafkaCodec::new();
        let mut buf = BytesMut::new();

        let payload = BytesMut::from(&b"hello world"[..]);
        codec.encode(payload.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_varint_roundtrip() {
        let mut buf = BytesMut::new();
        encode_unsigned_varint(&mut buf, 300);
        assert_eq!(parse_unsigned_varint(&mut buf).unwrap(), 300);
    }

    #[test]
    fn test_string_roundtrip() {
        let mut buf = BytesMut::new();
        encode_string(&mut buf, "hello");
        assert_eq!(parse_string(&mut buf).unwrap(), "hello");
    }
}
