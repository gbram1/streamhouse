//! AddOffsetsToTxn API handler (API Key 25)
//!
//! Registers a consumer group's `__consumer_offsets` topic partition
//! into an ongoing transaction. This is called by transactional consumers
//! that want to commit offsets as part of a transaction (exactly-once semantics).

use bytes::{Buf, BufMut, BytesMut};
use tracing::{debug, warn};

use crate::codec::{
    encode_empty_tagged_fields, parse_compact_string, parse_string, skip_tagged_fields,
    RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle AddOffsetsToTxn request
pub async fn handle_add_offsets_to_txn(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request based on version
    let (transactional_id, _producer_id, _producer_epoch, group_id) = if header.api_version >= 3 {
        // Compact protocol (v3+)
        let txn_id = parse_compact_string(body)?;
        let producer_id = body.get_i64();
        let producer_epoch = body.get_i16();
        let group_id = parse_compact_string(body)?;
        skip_tagged_fields(body)?;
        (txn_id, producer_id, producer_epoch, group_id)
    } else {
        // Legacy protocol (v0-v2)
        let txn_id = parse_string(body)?;
        let producer_id = body.get_i64();
        let producer_epoch = body.get_i16();
        let group_id = parse_string(body)?;
        (txn_id, producer_id, producer_epoch, group_id)
    };

    debug!(
        "AddOffsetsToTxn: transactional_id={}, group_id={}",
        transactional_id, group_id
    );

    // Look up producer by transactional_id
    let producer = match state
        .metadata
        .get_producer_by_transactional_id(&transactional_id, None)
        .await
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return build_response(header, ErrorCode::InvalidProducerIdMapping);
        }
        Err(e) => {
            warn!("Failed to get producer by transactional_id: {}", e);
            return build_response(header, ErrorCode::UnknownServerError);
        }
    };

    // Begin/get the active transaction for this producer
    let transaction = match state
        .metadata
        .begin_transaction(&producer.id, 60000)
        .await
    {
        Ok(txn) => txn,
        Err(e) => {
            warn!("Failed to get/begin transaction: {}", e);
            return build_response(header, ErrorCode::InvalidTxnState);
        }
    };

    // Register the __consumer_offsets topic into the transaction.
    // We use partition 0 as a simple hash of the group_id.
    // In a full implementation, this would hash group_id to a specific
    // __consumer_offsets partition.
    let offsets_partition = simple_group_partition(&group_id);

    let error_code = match state
        .metadata
        .add_transaction_partition(
            &transaction.transaction_id,
            "__consumer_offsets",
            offsets_partition,
            0, // placeholder offset
        )
        .await
    {
        Ok(()) => ErrorCode::None,
        Err(e) => {
            warn!(
                "Failed to add __consumer_offsets to transaction: {}",
                e
            );
            ErrorCode::UnknownServerError
        }
    };

    build_response(header, error_code)
}

/// Compute a simple partition index for a consumer group.
/// Uses a basic hash to map group_id to a partition number.
fn simple_group_partition(group_id: &str) -> u32 {
    let mut hash: u32 = 0;
    for byte in group_id.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as u32);
    }
    hash % 50 // Assume 50 __consumer_offsets partitions
}

fn build_response(header: &RequestHeader, error_code: ErrorCode) -> KafkaResult<BytesMut> {
    let mut response = BytesMut::new();

    // throttle_time_ms
    response.put_i32(0);
    // error_code
    response.put_i16(error_code.as_i16());

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
    fn test_parse_legacy_request() {
        // Build a legacy (v0) AddOffsetsToTxn request
        let mut buf = BytesMut::new();

        // transactional_id (string)
        let txn_id = "my-txn";
        buf.put_i16(txn_id.len() as i16);
        buf.extend_from_slice(txn_id.as_bytes());
        // producer_id (i64)
        buf.put_i64(1234);
        // producer_epoch (i16)
        buf.put_i16(0);
        // group_id (string)
        let group = "my-group";
        buf.put_i16(group.len() as i16);
        buf.extend_from_slice(group.as_bytes());

        // Parse
        let txn = parse_string(&mut buf).unwrap();
        let pid = buf.get_i64();
        let epoch = buf.get_i16();
        let gid = parse_string(&mut buf).unwrap();

        assert_eq!(txn, "my-txn");
        assert_eq!(pid, 1234);
        assert_eq!(epoch, 0);
        assert_eq!(gid, "my-group");
    }

    #[test]
    fn test_response_encoding_legacy() {
        let header = RequestHeader {
            api_key: 25,
            api_version: 0,
            correlation_id: 1,
            client_id: None,
        };

        let response = build_response(&header, ErrorCode::None).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i16(), 0); // error_code = None
        assert!(buf.is_empty()); // no tagged fields for legacy
    }

    #[test]
    fn test_response_encoding_compact() {
        let header = RequestHeader {
            api_key: 25,
            api_version: 3,
            correlation_id: 1,
            client_id: None,
        };

        let response = build_response(&header, ErrorCode::None).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i16(), 0); // error_code = None
        assert_eq!(buf.get_u8(), 0); // empty tagged fields
    }

    #[test]
    fn test_response_encoding_error() {
        let header = RequestHeader {
            api_key: 25,
            api_version: 0,
            correlation_id: 1,
            client_id: None,
        };

        let response =
            build_response(&header, ErrorCode::InvalidProducerIdMapping).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), ErrorCode::InvalidProducerIdMapping.as_i16());
    }

    #[test]
    fn test_simple_group_partition_deterministic() {
        let p1 = simple_group_partition("my-group");
        let p2 = simple_group_partition("my-group");
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_simple_group_partition_within_range() {
        let groups = vec!["group-a", "group-b", "group-c", "my-consumer-group"];
        for group in groups {
            let p = simple_group_partition(group);
            assert!(p < 50, "Partition {} should be < 50 for group {}", p, group);
        }
    }

    #[test]
    fn test_simple_group_partition_different_groups() {
        // Different groups should generally map to different partitions
        // (not guaranteed but very likely for different strings)
        let p1 = simple_group_partition("group-a");
        let p2 = simple_group_partition("group-b");
        // We just test they're valid, not necessarily different
        assert!(p1 < 50);
        assert!(p2 < 50);
    }
}
