//! Topic management API handlers
//!
//! - CreateTopics (API 19)
//! - DeleteTopics (API 20)

use bytes::{Buf, BufMut, BytesMut};
use std::collections::HashMap;
use tracing::debug;

use streamhouse_metadata::TopicConfig;

use crate::codec::{
    encode_compact_nullable_string, encode_compact_string, encode_empty_tagged_fields,
    encode_nullable_string, encode_string, encode_unsigned_varint, parse_array,
    parse_compact_array, parse_compact_nullable_string, parse_compact_string,
    parse_nullable_string, parse_string, RequestHeader, skip_tagged_fields,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle CreateTopics request (API 19)
pub async fn handle_create_topics(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (topics, timeout_ms, validate_only) = if header.api_version >= 5 {
        // Compact protocol
        let topics = parse_compact_array(body, |b| {
            let name = parse_compact_string(b)?;
            let num_partitions = b.get_i32();
            let replication_factor = b.get_i16();
            let _assignments = parse_compact_array(b, |b| {
                let partition_index = b.get_i32();
                let broker_ids = parse_compact_array(b, |b| Ok(b.get_i32()))?;
                skip_tagged_fields(b)?;
                Ok((partition_index, broker_ids))
            })?;
            let _configs = parse_compact_array(b, |b| {
                let key = parse_compact_string(b)?;
                let value = parse_compact_nullable_string(b)?;
                skip_tagged_fields(b)?;
                Ok((key, value))
            })?;
            skip_tagged_fields(b)?;
            Ok(CreateTopicRequest {
                name,
                num_partitions,
                replication_factor,
            })
        })?;

        let timeout_ms = body.get_i32();
        let validate_only = if header.api_version >= 1 {
            body.get_i8() != 0
        } else {
            false
        };

        skip_tagged_fields(body)?;

        (topics, timeout_ms, validate_only)
    } else {
        // Legacy protocol
        let topics = parse_array(body, |b| {
            let name = parse_string(b)?;
            let num_partitions = b.get_i32();
            let replication_factor = b.get_i16();
            let _assignments = parse_array(b, |b| {
                let partition_index = b.get_i32();
                let broker_ids = parse_array(b, |b| Ok(b.get_i32()))?;
                Ok((partition_index, broker_ids))
            })?;
            let _configs = parse_array(b, |b| {
                let key = parse_string(b)?;
                let value = parse_nullable_string(b)?;
                Ok((key, value))
            })?;
            Ok(CreateTopicRequest {
                name,
                num_partitions,
                replication_factor,
            })
        })?;

        let timeout_ms = body.get_i32();
        let validate_only = if header.api_version >= 1 {
            body.get_i8() != 0
        } else {
            false
        };

        (topics, timeout_ms, validate_only)
    };

    debug!(
        "CreateTopics: topics={:?}, timeout={}, validate_only={}",
        topics.iter().map(|t| &t.name).collect::<Vec<_>>(),
        timeout_ms,
        validate_only
    );

    // Create topics
    let mut topic_responses = Vec::new();

    for topic in &topics {
        let response = if validate_only {
            // Just validate, don't create
            CreateTopicResponse {
                name: topic.name.clone(),
                error_code: ErrorCode::None,
                error_message: None,
                num_partitions: topic.num_partitions,
                replication_factor: topic.replication_factor,
            }
        } else {
            // Actually create the topic
            create_topic(state, topic).await
        };
        topic_responses.push(response);
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 5 {
        // Compact protocol
        response.put_i32(0); // throttle time

        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for topic in &topic_responses {
            encode_compact_string(&mut response, &topic.name);

            // Topic ID (v7+)
            if header.api_version >= 7 {
                response.put_slice(&[0u8; 16]);
            }

            response.put_i16(topic.error_code.as_i16());
            encode_compact_nullable_string(&mut response, topic.error_message.as_deref());

            // Topic config error (v5+)
            if header.api_version >= 5 {
                response.put_i32(topic.num_partitions);
                response.put_i16(topic.replication_factor);

                // Configs (empty)
                encode_unsigned_varint(&mut response, 1);
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

        for topic in &topic_responses {
            encode_string(&mut response, &topic.name);
            response.put_i16(topic.error_code.as_i16());

            if header.api_version >= 1 {
                encode_nullable_string(&mut response, topic.error_message.as_deref());
            }

            if header.api_version >= 5 {
                response.put_i32(topic.num_partitions);
                response.put_i16(topic.replication_factor);
                response.put_i32(0); // empty configs
            }
        }
    }

    Ok(response)
}

struct CreateTopicRequest {
    name: String,
    num_partitions: i32,
    replication_factor: i16,
}

struct CreateTopicResponse {
    name: String,
    error_code: ErrorCode,
    error_message: Option<String>,
    num_partitions: i32,
    replication_factor: i16,
}

async fn create_topic(state: &KafkaServerState, request: &CreateTopicRequest) -> CreateTopicResponse {
    // Default partition count to 1 if not specified
    let num_partitions = if request.num_partitions <= 0 {
        1
    } else {
        request.num_partitions
    };

    // Create topic config
    let config = TopicConfig {
        name: request.name.clone(),
        partition_count: num_partitions as u32,
        retention_ms: None,
        config: HashMap::new(),
    };

    // Create topic in metadata store
    match state.metadata.create_topic(config).await {
        Ok(_) => CreateTopicResponse {
            name: request.name.clone(),
            error_code: ErrorCode::None,
            error_message: None,
            num_partitions,
            replication_factor: 1, // We don't support replication yet
        },
        Err(e) => {
            let error_str = e.to_string();
            let error_code = if error_str.contains("already exists") {
                ErrorCode::TopicAlreadyExists
            } else {
                ErrorCode::UnknownServerError
            };
            CreateTopicResponse {
                name: request.name.clone(),
                error_code,
                error_message: Some(error_str),
                num_partitions: -1,
                replication_factor: -1,
            }
        }
    }
}

/// Handle DeleteTopics request (API 20)
pub async fn handle_delete_topics(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (topics, timeout_ms) = if header.api_version >= 4 {
        // Compact protocol
        let topics = parse_compact_array(body, |b| {
            let name = if header.api_version >= 6 {
                parse_compact_nullable_string(b)?
            } else {
                Some(parse_compact_string(b)?)
            };

            // Topic ID (v6+)
            let _topic_id = if header.api_version >= 6 {
                let mut uuid = [0u8; 16];
                uuid.copy_from_slice(&b.split_to(16));
                Some(uuid)
            } else {
                None
            };

            skip_tagged_fields(b)?;
            Ok(name)
        })?;

        let timeout_ms = body.get_i32();
        skip_tagged_fields(body)?;

        (topics, timeout_ms)
    } else {
        // Legacy protocol
        let topics = parse_array(body, |b| {
            Ok(Some(parse_string(b)?))
        })?;
        let timeout_ms = body.get_i32();
        (topics, timeout_ms)
    };

    // Filter out None values
    let topic_names: Vec<String> = topics.into_iter().flatten().collect();

    debug!("DeleteTopics: topics={:?}, timeout={}", topic_names, timeout_ms);

    // Delete topics
    let mut topic_responses = Vec::new();

    for name in &topic_names {
        let error_code = match state.metadata.delete_topic(name).await {
            Ok(_) => ErrorCode::None,
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("not found") {
                    ErrorCode::UnknownTopicOrPartition
                } else {
                    ErrorCode::UnknownServerError
                }
            }
        };
        topic_responses.push((name.clone(), error_code));
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 4 {
        // Compact protocol
        response.put_i32(0); // throttle time

        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for (name, error_code) in &topic_responses {
            encode_compact_nullable_string(&mut response, Some(name));

            // Topic ID (v6+)
            if header.api_version >= 6 {
                response.put_slice(&[0u8; 16]);
            }

            response.put_i16(error_code.as_i16());
            encode_compact_nullable_string(&mut response, None); // error_message
            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            response.put_i32(0); // throttle time
        }

        response.put_i32(topic_responses.len() as i32);

        for (name, error_code) in &topic_responses {
            encode_string(&mut response, name);
            response.put_i16(error_code.as_i16());

            if header.api_version >= 5 {
                encode_nullable_string(&mut response, None); // error_message
            }
        }
    }

    Ok(response)
}
