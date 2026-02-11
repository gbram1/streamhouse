#![no_main]

use bytes::BytesMut;
use libfuzzer_sys::fuzz_target;
use streamhouse_kafka::codec::*;
use tokio_util::codec::Decoder;

fuzz_target!(|data: &[u8]| {
    // Fuzz the Kafka frame codec with arbitrary bytes.
    // Tests handling of:
    // - Invalid length prefixes
    // - Oversized frames (>100MB)
    // - Truncated frames
    // - Zero-length frames
    let mut codec = KafkaCodec::new();
    let mut buf = BytesMut::from(data);

    // Try decoding frames
    loop {
        match codec.decode(&mut buf) {
            Ok(Some(frame)) => {
                // Successfully decoded a frame — try parsing as request header
                let mut frame_copy = frame.clone();
                if frame_copy.len() >= 8 {
                    let _ = RequestHeader::parse(&mut frame_copy);
                }
            }
            Ok(None) => break,  // Need more data
            Err(_) => break,    // Invalid data
        }
    }

    // Also fuzz the individual protocol parsing functions
    let mut buf2 = BytesMut::from(data);
    let _ = parse_nullable_string(&mut buf2);

    let mut buf3 = BytesMut::from(data);
    let _ = parse_compact_nullable_string(&mut buf3);

    let mut buf4 = BytesMut::from(data);
    let _ = parse_string(&mut buf4);

    let mut buf5 = BytesMut::from(data);
    let _ = parse_nullable_bytes(&mut buf5);

    let mut buf6 = BytesMut::from(data);
    let _ = parse_unsigned_varint(&mut buf6);

    let mut buf7 = BytesMut::from(data);
    let _ = parse_signed_varint(&mut buf7);

    let mut buf8 = BytesMut::from(data);
    let _ = skip_tagged_fields(&mut buf8);
});
