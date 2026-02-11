//! Edge-case tests for the Kafka wire protocol codec and parsing functions.

use bytes::{BufMut, BytesMut};
use streamhouse_kafka::codec::*;
use tokio_util::codec::{Decoder, Encoder};

// ---------------------------------------------------------------
// Frame codec
// ---------------------------------------------------------------

#[test]
fn codec_decode_empty_buffer() {
    let mut codec = KafkaCodec::new();
    let mut buf = BytesMut::new();
    assert!(codec.decode(&mut buf).unwrap().is_none());
}

#[test]
fn codec_decode_incomplete_length_prefix() {
    let mut codec = KafkaCodec::new();
    let mut buf = BytesMut::from(&[0u8, 0, 0][..]);
    assert!(codec.decode(&mut buf).unwrap().is_none());
}

#[test]
fn codec_decode_incomplete_payload() {
    let mut codec = KafkaCodec::new();
    let mut buf = BytesMut::new();
    buf.put_i32(10); // length says 10 bytes
    buf.put_slice(&[1, 2, 3]); // only 3 bytes of payload
    assert!(codec.decode(&mut buf).unwrap().is_none());
}

#[test]
fn codec_decode_exact_frame() {
    let mut codec = KafkaCodec::new();
    let payload = b"hello kafka";
    let mut buf = BytesMut::new();
    buf.put_i32(payload.len() as i32);
    buf.put_slice(payload);

    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(&frame[..], payload);
    assert!(buf.is_empty());
}

#[test]
fn codec_decode_two_frames() {
    let mut codec = KafkaCodec::new();
    let mut buf = BytesMut::new();

    // Frame 1
    buf.put_i32(3);
    buf.put_slice(b"abc");
    // Frame 2
    buf.put_i32(2);
    buf.put_slice(b"xy");

    let f1 = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(&f1[..], b"abc");

    let f2 = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(&f2[..], b"xy");
}

#[test]
fn codec_decode_oversized_frame_rejected() {
    let mut codec = KafkaCodec::with_max_frame_size(100);
    let mut buf = BytesMut::new();
    buf.put_i32(200); // exceeds max of 100
    buf.put_slice(&[0u8; 200]);

    let result = codec.decode(&mut buf);
    assert!(result.is_err());
}

#[test]
fn codec_decode_zero_length_frame() {
    let mut codec = KafkaCodec::new();
    let mut buf = BytesMut::new();
    buf.put_i32(0); // zero-length payload

    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert!(frame.is_empty());
}

#[test]
fn codec_encode_roundtrip() {
    let mut codec = KafkaCodec::new();
    let payload = BytesMut::from(&b"test payload"[..]);
    let mut encoded = BytesMut::new();
    codec.encode(payload.clone(), &mut encoded).unwrap();

    let decoded = codec.decode(&mut encoded).unwrap().unwrap();
    assert_eq!(decoded, payload);
}

#[test]
fn codec_encode_oversized_rejected() {
    let mut codec = KafkaCodec::with_max_frame_size(5);
    let payload = BytesMut::from(&b"too large"[..]);
    let mut dst = BytesMut::new();
    assert!(codec.encode(payload, &mut dst).is_err());
}

// ---------------------------------------------------------------
// Request header parsing
// ---------------------------------------------------------------

#[test]
fn parse_request_header_minimal() {
    let mut buf = BytesMut::new();
    buf.put_i16(18); // api_key: ApiVersions
    buf.put_i16(0); // api_version: 0
    buf.put_i32(1); // correlation_id
    buf.put_i16(-1); // null client_id

    let header = RequestHeader::parse(&mut buf).unwrap();
    assert_eq!(header.api_key, 18);
    assert_eq!(header.api_version, 0);
    assert_eq!(header.correlation_id, 1);
    assert_eq!(header.client_id, None);
}

#[test]
fn parse_request_header_with_client_id() {
    let mut buf = BytesMut::new();
    buf.put_i16(0); // api_key: Produce
    buf.put_i16(3); // api_version
    buf.put_i32(42); // correlation_id
    // client_id: "my-app"
    buf.put_i16(6);
    buf.put_slice(b"my-app");

    let header = RequestHeader::parse(&mut buf).unwrap();
    assert_eq!(header.api_key, 0);
    assert_eq!(header.api_version, 3);
    assert_eq!(header.correlation_id, 42);
    assert_eq!(header.client_id, Some("my-app".to_string()));
}

#[test]
fn parse_request_header_too_short() {
    let mut buf = BytesMut::from(&[0u8; 4][..]);
    assert!(RequestHeader::parse(&mut buf).is_err());
}

// ---------------------------------------------------------------
// String parsing
// ---------------------------------------------------------------

#[test]
fn parse_nullable_string_null() {
    let mut buf = BytesMut::new();
    buf.put_i16(-1);
    assert_eq!(parse_nullable_string(&mut buf).unwrap(), None);
}

#[test]
fn parse_nullable_string_empty() {
    let mut buf = BytesMut::new();
    buf.put_i16(0);
    assert_eq!(
        parse_nullable_string(&mut buf).unwrap(),
        Some(String::new())
    );
}

#[test]
fn parse_nullable_string_content() {
    let mut buf = BytesMut::new();
    buf.put_i16(5);
    buf.put_slice(b"hello");
    assert_eq!(
        parse_nullable_string(&mut buf).unwrap(),
        Some("hello".to_string())
    );
}

#[test]
fn parse_string_truncated_buffer() {
    let mut buf = BytesMut::new();
    buf.put_i16(10); // says 10 bytes
    buf.put_slice(b"short"); // only 5 bytes
    assert!(parse_string(&mut buf).is_err());
}

#[test]
fn parse_nullable_string_buffer_too_short() {
    let mut buf = BytesMut::from(&[0u8][..]); // only 1 byte, need 2 for length
    assert!(parse_nullable_string(&mut buf).is_err());
}

// ---------------------------------------------------------------
// Compact string parsing
// ---------------------------------------------------------------

#[test]
fn parse_compact_nullable_string_null() {
    let mut buf = BytesMut::new();
    buf.put_u8(0); // varint 0 = null
    assert_eq!(parse_compact_nullable_string(&mut buf).unwrap(), None);
}

#[test]
fn parse_compact_nullable_string_empty() {
    let mut buf = BytesMut::new();
    buf.put_u8(1); // varint 1 = length 0 (compact uses length + 1)
    assert_eq!(
        parse_compact_nullable_string(&mut buf).unwrap(),
        Some(String::new())
    );
}

#[test]
fn parse_compact_nullable_string_content() {
    let mut buf = BytesMut::new();
    buf.put_u8(6); // varint 6 = length 5
    buf.put_slice(b"world");
    assert_eq!(
        parse_compact_nullable_string(&mut buf).unwrap(),
        Some("world".to_string())
    );
}

// ---------------------------------------------------------------
// Bytes parsing
// ---------------------------------------------------------------

#[test]
fn parse_nullable_bytes_null() {
    let mut buf = BytesMut::new();
    buf.put_i32(-1);
    assert_eq!(parse_nullable_bytes(&mut buf).unwrap(), None);
}

#[test]
fn parse_nullable_bytes_empty() {
    let mut buf = BytesMut::new();
    buf.put_i32(0);
    assert_eq!(
        parse_nullable_bytes(&mut buf).unwrap(),
        Some(Vec::<u8>::new())
    );
}

#[test]
fn parse_nullable_bytes_content() {
    let mut buf = BytesMut::new();
    buf.put_i32(4);
    buf.put_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(
        parse_nullable_bytes(&mut buf).unwrap(),
        Some(vec![0xDE, 0xAD, 0xBE, 0xEF])
    );
}

#[test]
fn parse_nullable_bytes_buffer_too_short() {
    let mut buf = BytesMut::from(&[0u8; 2][..]);
    assert!(parse_nullable_bytes(&mut buf).is_err());
}

// ---------------------------------------------------------------
// Varint parsing
// ---------------------------------------------------------------

#[test]
fn parse_unsigned_varint_single_byte() {
    let mut buf = BytesMut::new();
    buf.put_u8(42);
    assert_eq!(parse_unsigned_varint(&mut buf).unwrap(), 42);
}

#[test]
fn parse_unsigned_varint_two_bytes() {
    // 300 = 0b100101100
    // varint: 0xAC 0x02
    let mut buf = BytesMut::new();
    buf.put_u8(0xAC);
    buf.put_u8(0x02);
    assert_eq!(parse_unsigned_varint(&mut buf).unwrap(), 300);
}

#[test]
fn parse_signed_varint_negative() {
    // Encode -1 as zigzag varint = 1 as unsigned varint
    let mut buf = BytesMut::new();
    buf.put_u8(1);
    assert_eq!(parse_signed_varint(&mut buf).unwrap(), -1);
}

// ---------------------------------------------------------------
// Encode/decode roundtrips
// ---------------------------------------------------------------

#[test]
fn encode_decode_nullable_string_roundtrip() {
    let mut buf = BytesMut::new();
    encode_nullable_string(&mut buf, Some("test-client"));
    let decoded = parse_nullable_string(&mut buf).unwrap();
    assert_eq!(decoded, Some("test-client".to_string()));
}

#[test]
fn encode_decode_nullable_string_null_roundtrip() {
    let mut buf = BytesMut::new();
    encode_nullable_string(&mut buf, None);
    let decoded = parse_nullable_string(&mut buf).unwrap();
    assert_eq!(decoded, None);
}

#[test]
fn encode_decode_compact_string_roundtrip() {
    let mut buf = BytesMut::new();
    encode_compact_nullable_string(&mut buf, Some("compact"));
    let decoded = parse_compact_nullable_string(&mut buf).unwrap();
    assert_eq!(decoded, Some("compact".to_string()));
}

#[test]
fn encode_decode_nullable_bytes_roundtrip() {
    let mut buf = BytesMut::new();
    encode_nullable_bytes(&mut buf, Some(&[1, 2, 3, 4, 5]));
    let decoded = parse_nullable_bytes(&mut buf).unwrap();
    assert_eq!(decoded, Some(vec![1, 2, 3, 4, 5]));
}

#[test]
fn encode_decode_nullable_bytes_null_roundtrip() {
    let mut buf = BytesMut::new();
    encode_nullable_bytes(&mut buf, None);
    let decoded = parse_nullable_bytes(&mut buf).unwrap();
    assert_eq!(decoded, None);
}

// ---------------------------------------------------------------
// Response header
// ---------------------------------------------------------------

#[test]
fn response_header_encode() {
    let header = ResponseHeader::new(12345);
    let mut buf = BytesMut::new();
    header.encode(&mut buf);
    assert_eq!(buf.len(), 4);
    assert_eq!(buf.as_ref(), &12345i32.to_be_bytes());
}

// ---------------------------------------------------------------
// Tagged fields
// ---------------------------------------------------------------

#[test]
fn skip_empty_tagged_fields() {
    let mut buf = BytesMut::new();
    buf.put_u8(0); // 0 tagged fields
    skip_tagged_fields(&mut buf).unwrap();
    assert!(buf.is_empty());
}

#[test]
fn encode_empty_tagged_fields_format() {
    let mut buf = BytesMut::new();
    encode_empty_tagged_fields(&mut buf);
    assert_eq!(buf.len(), 1);
    assert_eq!(buf[0], 0);
}
