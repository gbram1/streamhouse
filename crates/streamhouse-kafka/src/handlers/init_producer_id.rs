//! InitProducerId API handler (API Key 22)
//!
//! Initializes a producer ID for idempotent/transactional producing.
//! Returns a producer_id (i64) and producer_epoch (i16) that the client
//! uses to tag all subsequent produce requests.

use bytes::{Buf, BufMut, BytesMut};
use tracing::debug;

use crate::codec::{
    encode_empty_tagged_fields, parse_compact_nullable_string, parse_nullable_string,
    skip_tagged_fields, RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;
use streamhouse_metadata::InitProducerConfig;

/// Convert a UUID string producer ID to a Kafka-compatible i64.
///
/// Kafka clients expect a numeric i64 producer ID. We hash the UUID string
/// to produce a deterministic i64 value.
fn hash_producer_id(id: &str) -> i64 {
    // Use a simple FNV-1a-style hash to get a deterministic i64 from the UUID string
    let mut hash: u64 = 14695981039346656037;
    for byte in id.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    // Mask off the sign bit to ensure positive IDs (Kafka expects >= 0)
    (hash & 0x7FFF_FFFF_FFFF_FFFF) as i64
}

/// Handle InitProducerId request
pub async fn handle_init_producer_id(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request based on version
    let (transactional_id, transaction_timeout_ms) = if header.api_version >= 3 {
        // Compact protocol (v3+)
        let txn_id = parse_compact_nullable_string(body)?;
        let timeout_ms = body.get_i32();
        skip_tagged_fields(body)?;
        (txn_id, timeout_ms)
    } else {
        // Legacy protocol (v0-v2)
        let txn_id = parse_nullable_string(body)?;
        let timeout_ms = body.get_i32();
        (txn_id, timeout_ms)
    };

    debug!(
        "InitProducerId: transactional_id={:?}, timeout={}",
        transactional_id, transaction_timeout_ms
    );

    // Initialize the producer via the metadata store
    let config = InitProducerConfig {
        transactional_id: transactional_id.clone(),
        organization_id: None,
        timeout_ms: transaction_timeout_ms as u32,
        metadata: None,
    };

    let (error_code, producer_id, producer_epoch) = match state.metadata.init_producer(config).await
    {
        Ok(producer) => {
            let numeric_id = hash_producer_id(&producer.id);
            (
                ErrorCode::None,
                numeric_id,
                producer.epoch as i16,
            )
        }
        Err(e) => {
            tracing::warn!("InitProducerId failed: {}", e);
            (ErrorCode::UnknownServerError, -1i64, -1i16)
        }
    };

    // Build response
    let mut response = BytesMut::new();

    // throttle_time_ms (i32)
    response.put_i32(0);
    // error_code (i16)
    response.put_i16(error_code.as_i16());
    // producer_id (i64)
    response.put_i64(producer_id);
    // producer_epoch (i16)
    response.put_i16(producer_epoch);

    if header.api_version >= 3 {
        encode_empty_tagged_fields(&mut response);
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;

    #[test]
    fn test_hash_producer_id_deterministic() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let hash1 = hash_producer_id(id);
        let hash2 = hash_producer_id(id);
        assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    #[test]
    fn test_hash_producer_id_positive() {
        // All generated IDs should be non-negative (Kafka requirement)
        let ids = vec![
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "f47ac10b-58cc-4372-a567-0e02b2c3d479",
            "",
            "a",
        ];
        for id in ids {
            let hash = hash_producer_id(id);
            assert!(hash >= 0, "Hash for '{}' should be >= 0, got {}", id, hash);
        }
    }

    #[test]
    fn test_hash_producer_id_different_inputs() {
        let id1 = "550e8400-e29b-41d4-a716-446655440000";
        let id2 = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        assert_ne!(
            hash_producer_id(id1),
            hash_producer_id(id2),
            "Different UUIDs should produce different hashes"
        );
    }

    #[test]
    fn test_parse_legacy_request() {
        // Build a legacy (v0) InitProducerId request:
        // nullable_string: transactional_id = "my-txn"
        // i32: timeout_ms = 5000
        let mut buf = BytesMut::new();
        // Nullable string: length (i16) + bytes
        let txn_id = "my-txn";
        buf.put_i16(txn_id.len() as i16);
        buf.extend_from_slice(txn_id.as_bytes());
        // timeout_ms
        buf.put_i32(5000);

        // Parse using the same logic as the handler
        let txn = parse_nullable_string(&mut buf).unwrap();
        let timeout = buf.get_i32();

        assert_eq!(txn, Some("my-txn".to_string()));
        assert_eq!(timeout, 5000);
    }

    #[test]
    fn test_parse_legacy_request_null_transactional_id() {
        // Build a legacy request with null transactional_id
        let mut buf = BytesMut::new();
        // Nullable string: -1 means null
        buf.put_i16(-1);
        // timeout_ms
        buf.put_i32(10000);

        let txn = parse_nullable_string(&mut buf).unwrap();
        let timeout = buf.get_i32();

        assert_eq!(txn, None);
        assert_eq!(timeout, 10000);
    }

    #[test]
    fn test_response_encoding_legacy() {
        // Verify response structure for legacy protocol
        let mut response = BytesMut::new();
        response.put_i32(0); // throttle_time_ms
        response.put_i16(0); // error_code = None
        response.put_i64(12345); // producer_id
        response.put_i16(0); // producer_epoch

        // Verify we can read back correctly
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i16(), 0); // error_code
        assert_eq!(buf.get_i64(), 12345); // producer_id
        assert_eq!(buf.get_i16(), 0); // producer_epoch
    }

    #[test]
    fn test_response_encoding_error() {
        // Verify response structure for error case
        let mut response = BytesMut::new();
        response.put_i32(0); // throttle_time_ms
        response.put_i16(ErrorCode::UnknownServerError.as_i16()); // error_code
        response.put_i64(-1); // producer_id (invalid)
        response.put_i16(-1); // producer_epoch (invalid)

        let mut buf = response;
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), -1); // UnknownServerError
        assert_eq!(buf.get_i64(), -1);
        assert_eq!(buf.get_i16(), -1);
    }

    #[test]
    fn test_response_encoding_compact() {
        // Verify response structure for compact protocol (v3+)
        let mut response = BytesMut::new();
        response.put_i32(0); // throttle_time_ms
        response.put_i16(0); // error_code = None
        response.put_i64(99999); // producer_id
        response.put_i16(3); // producer_epoch
        // Tagged fields (empty = single byte 0)
        response.put_u8(0);

        let mut buf = response;
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), 0);
        assert_eq!(buf.get_i64(), 99999);
        assert_eq!(buf.get_i16(), 3);
        assert_eq!(buf.get_u8(), 0); // empty tagged fields
    }
}
