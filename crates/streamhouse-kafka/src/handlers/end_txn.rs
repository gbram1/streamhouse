//! EndTxn API handler (API Key 26)
//!
//! Commits or aborts an ongoing transaction. When committing, writes COMMIT
//! markers to all enrolled partitions and advances the Last Stable Offset (LSO).
//! When aborting, writes ABORT markers instead.

use bytes::{Buf, BufMut, BytesMut};
use tracing::{debug, warn};

use crate::codec::{
    encode_empty_tagged_fields, parse_compact_string, parse_string, skip_tagged_fields,
    RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;
use streamhouse_metadata::{TransactionMarker, TransactionMarkerType};

/// Handle EndTxn request
pub async fn handle_end_txn(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request based on version
    let (transactional_id, _producer_id, _producer_epoch, committed) = if header.api_version >= 3 {
        // Compact protocol (v3+)
        let txn_id = parse_compact_string(body)?;
        let producer_id = body.get_i64();
        let producer_epoch = body.get_i16();
        let committed = body.get_i8() != 0;
        skip_tagged_fields(body)?;
        (txn_id, producer_id, producer_epoch, committed)
    } else {
        // Legacy protocol (v0-v2)
        let txn_id = parse_string(body)?;
        let producer_id = body.get_i64();
        let producer_epoch = body.get_i16();
        let committed = body.get_i8() != 0;
        (txn_id, producer_id, producer_epoch, committed)
    };

    debug!(
        "EndTxn: transactional_id={}, committed={}",
        transactional_id, committed
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

    // Find the active transaction for this producer.
    // We get the transaction by beginning one (which returns existing if ongoing).
    let transaction = match state
        .metadata
        .begin_transaction(&producer.id, 60000)
        .await
    {
        Ok(txn) => txn,
        Err(e) => {
            warn!("Failed to get transaction for producer: {}", e);
            return build_response(header, ErrorCode::InvalidTxnState);
        }
    };

    // Get all partitions enrolled in this transaction
    let partitions = match state
        .metadata
        .get_transaction_partitions(&transaction.transaction_id)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to get transaction partitions: {}", e);
            return build_response(header, ErrorCode::UnknownServerError);
        }
    };

    let now = chrono::Utc::now().timestamp_millis();

    if committed {
        // Commit the transaction
        match state
            .metadata
            .commit_transaction(&transaction.transaction_id)
            .await
        {
            Ok(_commit_ts) => {}
            Err(e) => {
                warn!("Failed to commit transaction: {}", e);
                return build_response(header, ErrorCode::UnknownServerError);
            }
        }

        // Write COMMIT markers to all enrolled partitions and advance LSO
        for partition in &partitions {
            let marker = TransactionMarker {
                transaction_id: transaction.transaction_id.clone(),
                topic: partition.topic.clone(),
                partition_id: partition.partition_id,
                offset: partition.last_offset + 1,
                marker_type: TransactionMarkerType::Commit,
                timestamp: now,
            };

            if let Err(e) = state.metadata.add_transaction_marker(marker).await {
                warn!(
                    "Failed to write COMMIT marker for {}/{}: {}",
                    partition.topic, partition.partition_id, e
                );
            }

            // Advance the Last Stable Offset
            if let Err(e) = state
                .metadata
                .update_last_stable_offset(
                    &partition.topic,
                    partition.partition_id,
                    partition.last_offset + 1,
                )
                .await
            {
                warn!(
                    "Failed to advance LSO for {}/{}: {}",
                    partition.topic, partition.partition_id, e
                );
            }
        }
    } else {
        // Abort the transaction
        match state
            .metadata
            .abort_transaction(&transaction.transaction_id)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                warn!("Failed to abort transaction: {}", e);
                return build_response(header, ErrorCode::UnknownServerError);
            }
        }

        // Write ABORT markers to all enrolled partitions
        for partition in &partitions {
            let marker = TransactionMarker {
                transaction_id: transaction.transaction_id.clone(),
                topic: partition.topic.clone(),
                partition_id: partition.partition_id,
                offset: partition.last_offset + 1,
                marker_type: TransactionMarkerType::Abort,
                timestamp: now,
            };

            if let Err(e) = state.metadata.add_transaction_marker(marker).await {
                warn!(
                    "Failed to write ABORT marker for {}/{}: {}",
                    partition.topic, partition.partition_id, e
                );
            }
        }
    }

    build_response(header, ErrorCode::None)
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
    fn test_parse_legacy_request_commit() {
        // Build a legacy (v0) EndTxn request with committed=true
        let mut buf = BytesMut::new();

        // transactional_id (string)
        let txn_id = "my-txn";
        buf.put_i16(txn_id.len() as i16);
        buf.extend_from_slice(txn_id.as_bytes());
        // producer_id (i64)
        buf.put_i64(1234);
        // producer_epoch (i16)
        buf.put_i16(0);
        // committed (i8) = true
        buf.put_i8(1);

        // Parse
        let txn = parse_string(&mut buf).unwrap();
        let pid = buf.get_i64();
        let epoch = buf.get_i16();
        let committed = buf.get_i8() != 0;

        assert_eq!(txn, "my-txn");
        assert_eq!(pid, 1234);
        assert_eq!(epoch, 0);
        assert!(committed);
    }

    #[test]
    fn test_parse_legacy_request_abort() {
        // Build a legacy (v0) EndTxn request with committed=false
        let mut buf = BytesMut::new();

        let txn_id = "my-txn";
        buf.put_i16(txn_id.len() as i16);
        buf.extend_from_slice(txn_id.as_bytes());
        buf.put_i64(1234);
        buf.put_i16(0);
        buf.put_i8(0); // committed = false

        let _txn = parse_string(&mut buf).unwrap();
        let _pid = buf.get_i64();
        let _epoch = buf.get_i16();
        let committed = buf.get_i8() != 0;

        assert!(!committed);
    }

    #[test]
    fn test_response_encoding_legacy_success() {
        let header = RequestHeader {
            api_key: 26,
            api_version: 0,
            correlation_id: 1,
            client_id: None,
        };

        let response = build_response(&header, ErrorCode::None).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i16(), 0); // error_code = None
        assert!(buf.is_empty());
    }

    #[test]
    fn test_response_encoding_compact_success() {
        let header = RequestHeader {
            api_key: 26,
            api_version: 3,
            correlation_id: 1,
            client_id: None,
        };

        let response = build_response(&header, ErrorCode::None).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i16(), 0); // error_code = None
        assert_eq!(buf.get_u8(), 0); // empty tagged fields
        assert!(buf.is_empty());
    }

    #[test]
    fn test_response_encoding_error() {
        let header = RequestHeader {
            api_key: 26,
            api_version: 0,
            correlation_id: 1,
            client_id: None,
        };

        let response = build_response(&header, ErrorCode::InvalidTxnState).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), ErrorCode::InvalidTxnState.as_i16());
    }

    #[test]
    fn test_response_encoding_producer_not_found() {
        let header = RequestHeader {
            api_key: 26,
            api_version: 2,
            correlation_id: 42,
            client_id: Some("test-client".to_string()),
        };

        let response =
            build_response(&header, ErrorCode::InvalidProducerIdMapping).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), ErrorCode::InvalidProducerIdMapping.as_i16());
    }

    #[test]
    fn test_response_encoding_unknown_server_error() {
        let header = RequestHeader {
            api_key: 26,
            api_version: 1,
            correlation_id: 5,
            client_id: None,
        };

        let response = build_response(&header, ErrorCode::UnknownServerError).unwrap();
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), ErrorCode::UnknownServerError.as_i16());
    }
}
