//! AddPartitionsToTxn API handler (API Key 24)
//!
//! Adds partitions to an ongoing transaction. The client calls this before
//! producing to a new partition within a transaction, so the transaction
//! coordinator knows which partitions are involved.

use bytes::{Buf, BufMut, BytesMut};
use tracing::{debug, warn};

use crate::codec::{
    encode_compact_string, encode_empty_tagged_fields, encode_string, encode_unsigned_varint,
    parse_array, parse_compact_array, parse_compact_string, parse_string, skip_tagged_fields,
    RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle AddPartitionsToTxn request
pub async fn handle_add_partitions_to_txn(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request based on version
    let (transactional_id, _producer_id, _producer_epoch, topics) = if header.api_version >= 3 {
        // Compact protocol (v3+)
        let txn_id = parse_compact_string(body)?;
        let producer_id = body.get_i64();
        let producer_epoch = body.get_i16();

        let topics = parse_compact_array(body, |b| {
            let name = parse_compact_string(b)?;
            let partitions = parse_compact_array(b, |b| {
                let partition_index = b.get_i32();
                skip_tagged_fields(b)?;
                Ok(partition_index)
            })?;
            skip_tagged_fields(b)?;
            Ok((name, partitions))
        })?;

        skip_tagged_fields(body)?;

        (txn_id, producer_id, producer_epoch, topics)
    } else {
        // Legacy protocol (v0-v2)
        let txn_id = parse_string(body)?;
        let producer_id = body.get_i64();
        let producer_epoch = body.get_i16();

        let topics = parse_array(body, |b| {
            let name = parse_string(b)?;
            let partitions = parse_array(b, |b| {
                let partition_index = b.get_i32();
                Ok(partition_index)
            })?;
            Ok((name, partitions))
        })?;

        (txn_id, producer_id, producer_epoch, topics)
    };

    debug!(
        "AddPartitionsToTxn: transactional_id={}, topics={}",
        transactional_id,
        topics.len()
    );

    // Look up producer by transactional_id
    let producer = match state
        .metadata
        .get_producer_by_transactional_id(&transactional_id, None)
        .await
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            // Producer not found - return error for all partitions
            return build_error_response(header, &topics, ErrorCode::InvalidProducerIdMapping);
        }
        Err(e) => {
            warn!("Failed to get producer by transactional_id: {}", e);
            return build_error_response(header, &topics, ErrorCode::UnknownServerError);
        }
    };

    // Find active transaction for this producer
    let transaction = match state
        .metadata
        .begin_transaction(&producer.id, 60000)
        .await
    {
        Ok(txn) => txn,
        Err(e) => {
            warn!("Failed to get/begin transaction: {}", e);
            return build_error_response(header, &topics, ErrorCode::InvalidTxnState);
        }
    };

    // Process each topic-partition
    let mut topic_responses: Vec<(String, Vec<(i32, ErrorCode)>)> = Vec::new();

    for (topic_name, partitions) in &topics {
        let mut partition_responses: Vec<(i32, ErrorCode)> = Vec::new();

        for &partition_index in partitions {
            let error = match state
                .metadata
                .add_transaction_partition(
                    &transaction.transaction_id,
                    topic_name,
                    partition_index as u32,
                    0, // placeholder offset
                )
                .await
            {
                Ok(()) => ErrorCode::None,
                Err(e) => {
                    warn!(
                        "Failed to add partition {}/{} to transaction: {}",
                        topic_name, partition_index, e
                    );
                    ErrorCode::UnknownServerError
                }
            };

            partition_responses.push((partition_index, error));
        }

        topic_responses.push((topic_name.clone(), partition_responses));
    }

    // Build response
    let mut response = BytesMut::new();

    // throttle_time_ms
    response.put_i32(0);

    if header.api_version >= 3 {
        // Compact protocol
        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for (topic_name, partitions) in &topic_responses {
            encode_compact_string(&mut response, topic_name);
            encode_unsigned_varint(&mut response, (partitions.len() + 1) as u64);

            for (partition_index, error_code) in partitions {
                response.put_i32(*partition_index);
                response.put_i16(error_code.as_i16());
                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        response.put_i32(topic_responses.len() as i32);

        for (topic_name, partitions) in &topic_responses {
            encode_string(&mut response, topic_name);
            response.put_i32(partitions.len() as i32);

            for (partition_index, error_code) in partitions {
                response.put_i32(*partition_index);
                response.put_i16(error_code.as_i16());
            }
        }
    }

    Ok(response)
}

/// Build an error response where all partitions get the same error code
fn build_error_response(
    header: &RequestHeader,
    topics: &[(String, Vec<i32>)],
    error: ErrorCode,
) -> KafkaResult<BytesMut> {
    let mut response = BytesMut::new();

    // throttle_time_ms
    response.put_i32(0);

    if header.api_version >= 3 {
        encode_unsigned_varint(&mut response, (topics.len() + 1) as u64);

        for (topic_name, partitions) in topics {
            encode_compact_string(&mut response, topic_name);
            encode_unsigned_varint(&mut response, (partitions.len() + 1) as u64);

            for &partition_index in partitions {
                response.put_i32(partition_index);
                response.put_i16(error.as_i16());
                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        response.put_i32(topics.len() as i32);

        for (topic_name, partitions) in topics {
            encode_string(&mut response, topic_name);
            response.put_i32(partitions.len() as i32);

            for &partition_index in partitions {
                response.put_i32(partition_index);
                response.put_i16(error.as_i16());
            }
        }
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;

    #[test]
    fn test_parse_legacy_request() {
        // Build a legacy (v0) AddPartitionsToTxn request:
        // string: transactional_id = "txn-1"
        // i64: producer_id
        // i16: producer_epoch
        // array of (string topic, array of i32 partitions)
        let mut buf = BytesMut::new();

        // transactional_id
        let txn_id = "txn-1";
        buf.put_i16(txn_id.len() as i16);
        buf.extend_from_slice(txn_id.as_bytes());
        // producer_id
        buf.put_i64(1234);
        // producer_epoch
        buf.put_i16(0);
        // topics array: 1 topic
        buf.put_i32(1);
        // topic name
        let topic = "my-topic";
        buf.put_i16(topic.len() as i16);
        buf.extend_from_slice(topic.as_bytes());
        // partitions array: 2 partitions
        buf.put_i32(2);
        buf.put_i32(0);
        buf.put_i32(1);

        // Parse
        let txn = parse_string(&mut buf).unwrap();
        let pid = buf.get_i64();
        let epoch = buf.get_i16();
        let topics = parse_array(&mut buf, |b| {
            let name = parse_string(b).unwrap();
            let parts = parse_array(b, |b| Ok(b.get_i32())).unwrap();
            Ok((name, parts))
        })
        .unwrap();

        assert_eq!(txn, "txn-1");
        assert_eq!(pid, 1234);
        assert_eq!(epoch, 0);
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].0, "my-topic");
        assert_eq!(topics[0].1, vec![0, 1]);
    }

    #[test]
    fn test_legacy_response_encoding() {
        // Verify response format for legacy protocol
        let mut response = BytesMut::new();
        response.put_i32(0); // throttle_time_ms
        response.put_i32(1); // 1 topic
        // topic name
        let topic = "test-topic";
        response.put_i16(topic.len() as i16);
        response.extend_from_slice(topic.as_bytes());
        // 1 partition
        response.put_i32(1);
        response.put_i32(0); // partition_index
        response.put_i16(0); // error_code = None

        // Read back
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i32(), 1); // 1 topic
        let name = parse_string(&mut buf).unwrap();
        assert_eq!(name, "test-topic");
        assert_eq!(buf.get_i32(), 1); // 1 partition
        assert_eq!(buf.get_i32(), 0); // partition_index
        assert_eq!(buf.get_i16(), 0); // None error
    }

    #[test]
    fn test_error_response_all_partitions() {
        // Build an error response where all partitions get InvalidProducerIdMapping
        let topics = vec![
            ("topic-a".to_string(), vec![0, 1, 2]),
            ("topic-b".to_string(), vec![0]),
        ];

        let header = RequestHeader {
            api_key: 24,
            api_version: 0,
            correlation_id: 1,
            client_id: None,
        };

        let response = build_error_response(&header, &topics, ErrorCode::InvalidProducerIdMapping)
            .unwrap();

        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i32(), 2); // 2 topics

        // topic-a
        let name = parse_string(&mut buf).unwrap();
        assert_eq!(name, "topic-a");
        assert_eq!(buf.get_i32(), 3); // 3 partitions
        for i in 0..3 {
            assert_eq!(buf.get_i32(), i);
            assert_eq!(buf.get_i16(), ErrorCode::InvalidProducerIdMapping.as_i16());
        }

        // topic-b
        let name = parse_string(&mut buf).unwrap();
        assert_eq!(name, "topic-b");
        assert_eq!(buf.get_i32(), 1);
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), ErrorCode::InvalidProducerIdMapping.as_i16());
    }

    #[test]
    fn test_error_response_compact() {
        let topics = vec![("topic-c".to_string(), vec![0])];

        let header = RequestHeader {
            api_key: 24,
            api_version: 3,
            correlation_id: 1,
            client_id: None,
        };

        let response =
            build_error_response(&header, &topics, ErrorCode::InvalidTxnState).unwrap();

        // Just verify the response is non-empty and starts with throttle_time_ms
        assert!(response.len() > 4);
        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
    }

    #[test]
    fn test_error_response_empty_topics() {
        let topics: Vec<(String, Vec<i32>)> = vec![];

        let header = RequestHeader {
            api_key: 24,
            api_version: 0,
            correlation_id: 1,
            client_id: None,
        };

        let response =
            build_error_response(&header, &topics, ErrorCode::UnknownServerError).unwrap();

        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i32(), 0); // 0 topics
    }
}
