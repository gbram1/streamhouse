//! Offset-related API handlers
//!
//! - ListOffsets (API 2): Get earliest/latest offsets
//! - OffsetCommit (API 8): Commit consumer offsets
//! - OffsetFetch (API 9): Fetch committed offsets

use bytes::{Buf, BufMut, BytesMut};
use tracing::debug;

use crate::codec::{
    encode_compact_nullable_string, encode_compact_string, encode_empty_tagged_fields,
    encode_nullable_string, encode_string, encode_unsigned_varint, parse_array,
    parse_compact_array, parse_compact_nullable_string, parse_compact_string,
    parse_nullable_string, parse_string, RequestHeader, skip_tagged_fields,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle ListOffsets request (API 2)
pub async fn handle_list_offsets(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let topics = if header.api_version >= 6 {
        // Compact protocol
        let _replica_id = body.get_i32();
        let _isolation_level = body.get_i8();

        parse_compact_array(body, |b| {
            let name = parse_compact_string(b)?;
            let partitions = parse_compact_array(b, |b| {
                let partition_index = b.get_i32();
                let _current_leader_epoch = b.get_i32();
                let timestamp = b.get_i64();
                skip_tagged_fields(b)?;
                Ok((partition_index, timestamp))
            })?;
            skip_tagged_fields(b)?;
            Ok((name, partitions))
        })?
    } else {
        // Legacy protocol
        let _replica_id = body.get_i32();

        let _isolation_level = if header.api_version >= 2 {
            body.get_i8()
        } else {
            0
        };

        parse_array(body, |b| {
            let name = parse_string(b)?;
            let partitions = parse_array(b, |b| {
                let partition_index = b.get_i32();
                let _current_leader_epoch = if header.api_version >= 4 {
                    b.get_i32()
                } else {
                    -1
                };
                let timestamp = b.get_i64();
                Ok((partition_index, timestamp))
            })?;
            Ok((name, partitions))
        })?
    };

    debug!("ListOffsets: topics={}", topics.len());

    // Get offsets
    let mut topic_responses = Vec::new();

    for (topic_name, partitions) in topics {
        let mut partition_responses = Vec::new();

        for (partition_index, timestamp) in partitions {
            let response = get_partition_offset(
                state,
                &topic_name,
                partition_index as u32,
                timestamp,
            )
            .await;
            partition_responses.push(response);
        }

        topic_responses.push((topic_name, partition_responses));
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 6 {
        // Compact protocol
        response.put_i32(0); // throttle time

        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for (topic_name, partitions) in &topic_responses {
            encode_compact_string(&mut response, topic_name);
            encode_unsigned_varint(&mut response, (partitions.len() + 1) as u64);

            for p in partitions {
                response.put_i32(p.partition_index);
                response.put_i16(p.error_code.as_i16());
                response.put_i64(p.timestamp);
                response.put_i64(p.offset);

                if header.api_version >= 4 {
                    response.put_i32(p.leader_epoch);
                }

                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 2 {
            response.put_i32(0); // throttle time
        }

        response.put_i32(topic_responses.len() as i32);

        for (topic_name, partitions) in &topic_responses {
            encode_string(&mut response, topic_name);
            response.put_i32(partitions.len() as i32);

            for p in partitions {
                response.put_i32(p.partition_index);
                response.put_i16(p.error_code.as_i16());

                if header.api_version == 0 {
                    // v0 returns array of offsets
                    response.put_i32(1);
                    response.put_i64(p.offset);
                } else {
                    response.put_i64(p.timestamp);
                    response.put_i64(p.offset);

                    if header.api_version >= 4 {
                        response.put_i32(p.leader_epoch);
                    }
                }
            }
        }
    }

    Ok(response)
}

struct ListOffsetsPartitionResponse {
    partition_index: i32,
    error_code: ErrorCode,
    timestamp: i64,
    offset: i64,
    leader_epoch: i32,
}

async fn get_partition_offset(
    state: &KafkaServerState,
    topic_name: &str,
    partition_id: u32,
    timestamp: i64,
) -> ListOffsetsPartitionResponse {
    // Get partition info
    let partition = match state.metadata.get_partition(topic_name, partition_id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return ListOffsetsPartitionResponse {
                partition_index: partition_id as i32,
                error_code: ErrorCode::UnknownTopicOrPartition,
                timestamp: -1,
                offset: -1,
                leader_epoch: -1,
            };
        }
        Err(_) => {
            return ListOffsetsPartitionResponse {
                partition_index: partition_id as i32,
                error_code: ErrorCode::UnknownServerError,
                timestamp: -1,
                offset: -1,
                leader_epoch: -1,
            };
        }
    };

    // Determine offset based on timestamp
    // -1 = latest, -2 = earliest
    let offset = match timestamp {
        -1 => partition.high_watermark as i64,
        -2 => 0, // Earliest is always 0 for now
        ts => {
            // TODO: Implement timestamp-based lookup
            partition.high_watermark as i64
        }
    };

    ListOffsetsPartitionResponse {
        partition_index: partition_id as i32,
        error_code: ErrorCode::None,
        timestamp: chrono::Utc::now().timestamp_millis(),
        offset,
        leader_epoch: 0,
    }
}

/// Handle OffsetCommit request (API 8)
pub async fn handle_offset_commit(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (group_id, generation_id, member_id, topics) = if header.api_version >= 8 {
        // Compact protocol
        let group_id = parse_compact_string(body)?;
        let generation_id = body.get_i32();
        let member_id = parse_compact_string(body)?;
        let _group_instance_id = parse_compact_nullable_string(body)?;

        let topics = parse_compact_array(body, |b| {
            let name = parse_compact_string(b)?;
            let partitions = parse_compact_array(b, |b| {
                let partition_index = b.get_i32();
                let committed_offset = b.get_i64();
                let _committed_leader_epoch = b.get_i32();
                let metadata = parse_compact_nullable_string(b)?;
                skip_tagged_fields(b)?;
                Ok((partition_index, committed_offset, metadata))
            })?;
            skip_tagged_fields(b)?;
            Ok((name, partitions))
        })?;

        skip_tagged_fields(body)?;

        (group_id, generation_id, member_id, topics)
    } else {
        // Legacy protocol
        let group_id = parse_string(body)?;

        let generation_id = if header.api_version >= 1 {
            body.get_i32()
        } else {
            -1
        };

        let member_id = if header.api_version >= 1 {
            parse_string(body)?
        } else {
            String::new()
        };

        let _retention_time = if header.api_version >= 2 && header.api_version <= 4 {
            body.get_i64()
        } else {
            -1
        };

        let _group_instance_id = if header.api_version >= 7 {
            parse_nullable_string(body)?
        } else {
            None
        };

        let topics = parse_array(body, |b| {
            let name = parse_string(b)?;
            let partitions = parse_array(b, |b| {
                let partition_index = b.get_i32();
                let committed_offset = b.get_i64();

                let _committed_leader_epoch = if header.api_version >= 6 {
                    b.get_i32()
                } else {
                    -1
                };

                let _timestamp = if header.api_version == 1 {
                    b.get_i64()
                } else {
                    -1
                };

                let metadata = parse_nullable_string(b)?;

                Ok((partition_index, committed_offset, metadata))
            })?;
            Ok((name, partitions))
        })?;

        (group_id, generation_id, member_id, topics)
    };

    debug!("OffsetCommit: group={}, topics={}", group_id, topics.len());

    // Commit offsets
    let mut topic_responses = Vec::new();

    for (topic_name, partitions) in topics {
        let mut partition_responses = Vec::new();

        for (partition_index, offset, metadata) in partitions {
            let error = commit_offset(
                state,
                &group_id,
                &topic_name,
                partition_index as u32,
                offset as u64,
                metadata,
            )
            .await;

            partition_responses.push((partition_index, error));
        }

        topic_responses.push((topic_name, partition_responses));
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 8 {
        // Compact protocol
        response.put_i32(0); // throttle time

        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for (topic_name, partitions) in &topic_responses {
            encode_compact_string(&mut response, topic_name);
            encode_unsigned_varint(&mut response, (partitions.len() + 1) as u64);

            for (partition_index, error) in partitions {
                response.put_i32(*partition_index);
                response.put_i16(error.as_i16());
                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 3 {
            response.put_i32(0); // throttle time
        }

        response.put_i32(topic_responses.len() as i32);

        for (topic_name, partitions) in &topic_responses {
            encode_string(&mut response, topic_name);
            response.put_i32(partitions.len() as i32);

            for (partition_index, error) in partitions {
                response.put_i32(*partition_index);
                response.put_i16(error.as_i16());
            }
        }
    }

    Ok(response)
}

async fn commit_offset(
    state: &KafkaServerState,
    group_id: &str,
    topic: &str,
    partition: u32,
    offset: u64,
    metadata: Option<String>,
) -> ErrorCode {
    match state
        .metadata
        .commit_offset(group_id, topic, partition, offset, metadata)
        .await
    {
        Ok(_) => ErrorCode::None,
        Err(_) => ErrorCode::UnknownServerError,
    }
}

/// Handle OffsetFetch request (API 9)
pub async fn handle_offset_fetch(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (group_id, topics) = if header.api_version >= 8 {
        // Compact protocol - v8+ can fetch all groups at once
        let groups = parse_compact_array(body, |b| {
            let group_id = parse_compact_string(b)?;
            let topics = if header.api_version >= 8 {
                parse_compact_array(b, |b| {
                    let name = parse_compact_string(b)?;
                    let partitions = parse_compact_array(b, |b| {
                        let partition_index = b.get_i32();
                        skip_tagged_fields(b)?;
                        Ok(partition_index)
                    })?;
                    skip_tagged_fields(b)?;
                    Ok((name, partitions))
                })?
            } else {
                vec![]
            };
            skip_tagged_fields(b)?;
            Ok((group_id, topics))
        })?;

        let _require_stable = body.get_i8();
        skip_tagged_fields(body)?;

        // Take first group for simplicity
        groups.into_iter().next().unwrap_or_default()
    } else if header.api_version >= 6 {
        // Compact protocol v6-7
        let group_id = parse_compact_string(body)?;
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

        let _require_stable = if header.api_version >= 7 {
            body.get_i8()
        } else {
            0
        };

        skip_tagged_fields(body)?;

        (group_id, topics)
    } else {
        // Legacy protocol
        let group_id = parse_string(body)?;

        let topics = if header.api_version >= 2 {
            // v2+ can have null topics (fetch all)
            let has_topics = body.len() >= 4 && (&body[..4]).get_i32() >= 0;
            if has_topics {
                parse_array(body, |b| {
                    let name = parse_string(b)?;
                    let partitions = parse_array(b, |b| Ok(b.get_i32()))?;
                    Ok((name, partitions))
                })?
            } else {
                body.get_i32(); // consume the -1
                vec![]
            }
        } else {
            parse_array(body, |b| {
                let name = parse_string(b)?;
                let partitions = parse_array(b, |b| Ok(b.get_i32()))?;
                Ok((name, partitions))
            })?
        };

        (group_id, topics)
    };

    debug!("OffsetFetch: group={}, topics={}", group_id, topics.len());

    // Fetch offsets
    let mut topic_responses = Vec::new();

    for (topic_name, partitions) in topics {
        let mut partition_responses = Vec::new();

        for partition_index in partitions {
            let (offset, metadata, error) = fetch_offset(
                state,
                &group_id,
                &topic_name,
                partition_index as u32,
            )
            .await;

            partition_responses.push((partition_index, offset, metadata, error));
        }

        topic_responses.push((topic_name, partition_responses));
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 8 {
        // Compact protocol v8+
        response.put_i32(0); // throttle time

        // Groups array
        encode_unsigned_varint(&mut response, 2); // 1 group + 1

        encode_compact_string(&mut response, &group_id);

        // Topics
        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for (topic_name, partitions) in &topic_responses {
            encode_compact_string(&mut response, topic_name);
            encode_unsigned_varint(&mut response, (partitions.len() + 1) as u64);

            for (partition_index, offset, metadata, error) in partitions {
                response.put_i32(*partition_index);
                response.put_i64(*offset);
                response.put_i32(-1); // leader epoch
                encode_compact_nullable_string(&mut response, metadata.as_deref());
                response.put_i16(error.as_i16());
                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        response.put_i16(ErrorCode::None.as_i16()); // group error
        encode_empty_tagged_fields(&mut response);

        encode_empty_tagged_fields(&mut response);
    } else if header.api_version >= 6 {
        // Compact protocol v6-7
        response.put_i32(0); // throttle time

        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for (topic_name, partitions) in &topic_responses {
            encode_compact_string(&mut response, topic_name);
            encode_unsigned_varint(&mut response, (partitions.len() + 1) as u64);

            for (partition_index, offset, metadata, error) in partitions {
                response.put_i32(*partition_index);
                response.put_i64(*offset);
                response.put_i32(-1); // leader epoch
                encode_compact_nullable_string(&mut response, metadata.as_deref());
                response.put_i16(error.as_i16());
                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        response.put_i16(ErrorCode::None.as_i16()); // error code

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 3 {
            response.put_i32(0); // throttle time
        }

        response.put_i32(topic_responses.len() as i32);

        for (topic_name, partitions) in &topic_responses {
            encode_string(&mut response, topic_name);
            response.put_i32(partitions.len() as i32);

            for (partition_index, offset, metadata, error) in partitions {
                response.put_i32(*partition_index);
                response.put_i64(*offset);

                if header.api_version >= 5 {
                    response.put_i32(-1); // leader epoch
                }

                encode_nullable_string(&mut response, metadata.as_deref());
                response.put_i16(error.as_i16());
            }
        }

        if header.api_version >= 2 {
            response.put_i16(ErrorCode::None.as_i16()); // error code
        }
    }

    Ok(response)
}

async fn fetch_offset(
    state: &KafkaServerState,
    group_id: &str,
    topic: &str,
    partition: u32,
) -> (i64, Option<String>, ErrorCode) {
    match state.metadata.get_committed_offset(group_id, topic, partition).await {
        Ok(Some(offset)) => (offset as i64, None, ErrorCode::None),
        Ok(None) => (-1, None, ErrorCode::None), // No committed offset
        Err(_) => (-1, None, ErrorCode::UnknownServerError),
    }
}
