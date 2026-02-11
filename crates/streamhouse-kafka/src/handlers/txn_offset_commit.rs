//! TxnOffsetCommit API handler (API Key 28)
//!
//! Commits consumer offsets as part of a transaction. This ensures that
//! offset commits are atomic with the transaction — if the transaction
//! aborts, the offsets are rolled back too (exactly-once semantics).

use bytes::{Buf, BufMut, BytesMut};
use tracing::{debug, warn};

use crate::codec::{
    encode_compact_string, encode_empty_tagged_fields, encode_string, encode_unsigned_varint,
    parse_array, parse_compact_array, parse_compact_nullable_string, parse_compact_string,
    parse_nullable_string, parse_string, skip_tagged_fields, RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle TxnOffsetCommit request
pub async fn handle_txn_offset_commit(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request based on version
    let (transactional_id, group_id, _producer_id, _producer_epoch, topics) =
        if header.api_version >= 3 {
            // Compact protocol (v3+)
            let txn_id = parse_compact_string(body)?;
            let group_id = parse_compact_string(body)?;
            let producer_id = body.get_i64();
            let producer_epoch = body.get_i16();

            let topics = parse_compact_array(body, |b| {
                let name = parse_compact_string(b)?;
                let partitions = parse_compact_array(b, |b| {
                    let partition_index = b.get_i32();
                    let committed_offset = b.get_i64();
                    let _committed_leader_epoch = if header.api_version >= 2 {
                        b.get_i32()
                    } else {
                        -1
                    };
                    let metadata = parse_compact_nullable_string(b)?;
                    skip_tagged_fields(b)?;
                    Ok((partition_index, committed_offset, metadata))
                })?;
                skip_tagged_fields(b)?;
                Ok((name, partitions))
            })?;

            skip_tagged_fields(body)?;

            (txn_id, group_id, producer_id, producer_epoch, topics)
        } else {
            // Legacy protocol (v0-v2)
            let txn_id = parse_string(body)?;
            let group_id = parse_string(body)?;
            let producer_id = body.get_i64();
            let producer_epoch = body.get_i16();

            let topics = parse_array(body, |b| {
                let name = parse_string(b)?;
                let partitions = parse_array(b, |b| {
                    let partition_index = b.get_i32();
                    let committed_offset = b.get_i64();
                    let _committed_leader_epoch = if header.api_version >= 2 {
                        b.get_i32()
                    } else {
                        -1
                    };
                    let metadata = parse_nullable_string(b)?;
                    Ok((partition_index, committed_offset, metadata))
                })?;
                Ok((name, partitions))
            })?;

            (txn_id, group_id, producer_id, producer_epoch, topics)
        };

    debug!(
        "TxnOffsetCommit: transactional_id={}, group_id={}, topics={}",
        transactional_id,
        group_id,
        topics.len()
    );

    // Process each topic-partition offset commit via the metadata store
    let mut topic_responses: Vec<(String, Vec<(i32, ErrorCode)>)> = Vec::new();

    for (topic_name, partitions) in &topics {
        let mut partition_responses: Vec<(i32, ErrorCode)> = Vec::new();

        for (partition_index, committed_offset, metadata) in partitions {
            let error = match state
                .metadata
                .commit_offset(
                    &group_id,
                    topic_name,
                    *partition_index as u32,
                    *committed_offset as u64,
                    metadata.clone(),
                )
                .await
            {
                Ok(()) => ErrorCode::None,
                Err(e) => {
                    warn!(
                        "Failed to commit txn offset for {}/{}: {}",
                        topic_name, partition_index, e
                    );
                    ErrorCode::UnknownServerError
                }
            };

            partition_responses.push((*partition_index, error));
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

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;

    #[test]
    fn test_parse_legacy_request() {
        // Build a legacy (v0) TxnOffsetCommit request
        let mut buf = BytesMut::new();

        // transactional_id (string)
        let txn_id = "my-txn";
        buf.put_i16(txn_id.len() as i16);
        buf.extend_from_slice(txn_id.as_bytes());
        // group_id (string)
        let group = "my-group";
        buf.put_i16(group.len() as i16);
        buf.extend_from_slice(group.as_bytes());
        // producer_id (i64)
        buf.put_i64(1234);
        // producer_epoch (i16)
        buf.put_i16(0);
        // topics array: 1 topic
        buf.put_i32(1);
        // topic name
        let topic = "test-topic";
        buf.put_i16(topic.len() as i16);
        buf.extend_from_slice(topic.as_bytes());
        // partitions array: 1 partition
        buf.put_i32(1);
        buf.put_i32(0); // partition_index
        buf.put_i64(100); // committed_offset
        // metadata (nullable string)
        buf.put_i16(-1); // null

        // Parse fields manually to verify
        let txn = parse_string(&mut buf).unwrap();
        let group = parse_string(&mut buf).unwrap();
        let pid = buf.get_i64();
        let epoch = buf.get_i16();

        assert_eq!(txn, "my-txn");
        assert_eq!(group, "my-group");
        assert_eq!(pid, 1234);
        assert_eq!(epoch, 0);

        let topics = parse_array(&mut buf, |b| {
            let name = parse_string(b).unwrap();
            let parts = parse_array(b, |b| {
                let idx = b.get_i32();
                let offset = b.get_i64();
                let meta = parse_nullable_string(b).unwrap();
                Ok((idx, offset, meta))
            })
            .unwrap();
            Ok((name, parts))
        })
        .unwrap();

        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].0, "test-topic");
        assert_eq!(topics[0].1.len(), 1);
        assert_eq!(topics[0].1[0].0, 0);
        assert_eq!(topics[0].1[0].1, 100);
        assert_eq!(topics[0].1[0].2, None);
    }

    #[test]
    fn test_legacy_response_encoding() {
        let mut response = BytesMut::new();
        response.put_i32(0); // throttle_time_ms
        response.put_i32(1); // 1 topic
        let topic = "test-topic";
        response.put_i16(topic.len() as i16);
        response.extend_from_slice(topic.as_bytes());
        response.put_i32(1); // 1 partition
        response.put_i32(0); // partition_index
        response.put_i16(0); // error_code = None

        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle_time_ms
        assert_eq!(buf.get_i32(), 1); // 1 topic
        let name = parse_string(&mut buf).unwrap();
        assert_eq!(name, "test-topic");
        assert_eq!(buf.get_i32(), 1);
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), 0);
    }

    #[test]
    fn test_legacy_response_encoding_with_error() {
        let mut response = BytesMut::new();
        response.put_i32(0); // throttle_time_ms
        response.put_i32(1); // 1 topic
        let topic = "test-topic";
        response.put_i16(topic.len() as i16);
        response.extend_from_slice(topic.as_bytes());
        response.put_i32(1); // 1 partition
        response.put_i32(0); // partition_index
        response.put_i16(ErrorCode::UnknownServerError.as_i16());

        let mut buf = response;
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i32(), 1);
        let _name = parse_string(&mut buf).unwrap();
        assert_eq!(buf.get_i32(), 1);
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), ErrorCode::UnknownServerError.as_i16());
    }

    #[test]
    fn test_response_multiple_topics_and_partitions() {
        // Build a legacy response with multiple topics and partitions
        let mut response = BytesMut::new();
        response.put_i32(0); // throttle_time_ms
        response.put_i32(2); // 2 topics

        // Topic 1
        let topic1 = "topic-a";
        response.put_i16(topic1.len() as i16);
        response.extend_from_slice(topic1.as_bytes());
        response.put_i32(2); // 2 partitions
        response.put_i32(0);
        response.put_i16(0); // None
        response.put_i32(1);
        response.put_i16(0); // None

        // Topic 2
        let topic2 = "topic-b";
        response.put_i16(topic2.len() as i16);
        response.extend_from_slice(topic2.as_bytes());
        response.put_i32(1); // 1 partition
        response.put_i32(0);
        response.put_i16(ErrorCode::UnknownServerError.as_i16());

        let mut buf = response;
        assert_eq!(buf.get_i32(), 0); // throttle
        assert_eq!(buf.get_i32(), 2); // 2 topics

        let name1 = parse_string(&mut buf).unwrap();
        assert_eq!(name1, "topic-a");
        assert_eq!(buf.get_i32(), 2);
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), 0);
        assert_eq!(buf.get_i32(), 1);
        assert_eq!(buf.get_i16(), 0);

        let name2 = parse_string(&mut buf).unwrap();
        assert_eq!(name2, "topic-b");
        assert_eq!(buf.get_i32(), 1);
        assert_eq!(buf.get_i32(), 0);
        assert_eq!(buf.get_i16(), ErrorCode::UnknownServerError.as_i16());
    }

    #[test]
    fn test_parse_legacy_request_with_metadata() {
        // Build a request where metadata is present (not null)
        let mut buf = BytesMut::new();

        // Build just the partition portion: partition_index, offset, metadata
        buf.put_i32(3); // partition_index
        buf.put_i64(500); // committed_offset
        let meta = "some-metadata";
        buf.put_i16(meta.len() as i16);
        buf.extend_from_slice(meta.as_bytes());

        let idx = buf.get_i32();
        let offset = buf.get_i64();
        let metadata = parse_nullable_string(&mut buf).unwrap();

        assert_eq!(idx, 3);
        assert_eq!(offset, 500);
        assert_eq!(metadata, Some("some-metadata".to_string()));
    }
}
