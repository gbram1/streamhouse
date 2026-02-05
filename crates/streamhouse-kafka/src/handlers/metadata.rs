//! Metadata API handler (API Key 3)
//!
//! Returns information about topics, partitions, and brokers.

use bytes::{Buf, BufMut, BytesMut};
use tracing::{debug, info};

use crate::codec::{
    encode_compact_nullable_string, encode_compact_string, encode_empty_tagged_fields,
    encode_nullable_string, encode_string, encode_unsigned_varint, parse_array,
    parse_compact_array, parse_compact_nullable_string, parse_nullable_string, RequestHeader,
    skip_tagged_fields,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle Metadata request
pub async fn handle_metadata(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let topics = if header.api_version >= 9 {
        // Compact protocol
        parse_compact_array(body, |b| parse_compact_nullable_string(b).map(|s| s.unwrap_or_default()))?
    } else {
        // Legacy protocol
        if header.api_version >= 1 {
            parse_array(body, |b| parse_nullable_string(b).map(|s| s.unwrap_or_default()))?
        } else {
            parse_array(body, |b| {
                let len = b.get_i16() as usize;
                let s = String::from_utf8(b.split_to(len).to_vec()).unwrap_or_default();
                Ok(s)
            })?
        }
    };

    // Parse allow_auto_topic_creation (v4+)
    let allow_auto_topic_creation = if header.api_version >= 4 && body.remaining() >= 1 {
        body.get_i8() != 0
    } else {
        true // Default to true for older versions
    };

    // Skip remaining fields
    if header.api_version >= 9 {
        skip_tagged_fields(body)?;
    }

    debug!("Metadata request for topics: {:?}, auto_create: {}", topics, allow_auto_topic_creation);

    // Get topic metadata from store
    let mut all_topics = state.metadata.list_topics().await
        .map_err(|e| crate::error::KafkaError::MetadataStore(e.to_string()))?;

    // Auto-create topics if requested and allowed
    if allow_auto_topic_creation && !topics.is_empty() {
        for topic_name in &topics {
            if !all_topics.iter().any(|t| &t.name == topic_name) {
                // Auto-create the topic with 1 partition
                info!("Auto-creating topic: {}", topic_name);
                let config = streamhouse_metadata::TopicConfig {
                    name: topic_name.clone(),
                    partition_count: 1,
                    retention_ms: None,
                    cleanup_policy: Default::default(),
                    config: std::collections::HashMap::new(),
                };
                if let Err(e) = state.metadata.create_topic(config).await {
                    debug!("Failed to auto-create topic {}: {}", topic_name, e);
                } else {
                    // Refresh the topics list
                    if let Ok(refreshed) = state.metadata.list_topics().await {
                        all_topics = refreshed;
                    }
                }
            }
        }
    }

    // Filter topics if specific ones requested
    let requested_topics: Vec<_> = if topics.is_empty() {
        // Return all topics
        all_topics
    } else {
        // Return only requested topics
        all_topics
            .into_iter()
            .filter(|t| topics.contains(&t.name))
            .collect()
    };

    // Build response
    let mut response = BytesMut::new();
    let node_id = state.config.node_id;
    let host = &state.config.advertised_host;
    let port = state.config.advertised_port;

    if header.api_version >= 9 {
        // Compact protocol (v9+)

        // Throttle time
        response.put_i32(0);

        // Brokers array (compact)
        encode_unsigned_varint(&mut response, 2); // 1 broker + 1
        response.put_i32(node_id);
        encode_compact_string(&mut response, host);
        response.put_i32(port);
        encode_compact_nullable_string(&mut response, None); // rack
        encode_empty_tagged_fields(&mut response);

        // Cluster ID
        encode_compact_nullable_string(&mut response, Some("streamhouse"));

        // Controller ID
        response.put_i32(node_id);

        // Topics array (compact)
        encode_unsigned_varint(&mut response, (requested_topics.len() + 1) as u64);
        for topic in &requested_topics {
            response.put_i16(ErrorCode::None.as_i16());
            encode_compact_string(&mut response, &topic.name);

            // Topic ID (v10+)
            if header.api_version >= 10 {
                // UUID - use zeros for now
                response.put_slice(&[0u8; 16]);
            }

            // Is internal
            response.put_u8(0);

            // Partitions array (compact)
            encode_unsigned_varint(&mut response, (topic.partition_count + 1) as u64);
            for partition_id in 0..topic.partition_count {
                response.put_i16(ErrorCode::None.as_i16());
                response.put_i32(partition_id as i32);
                response.put_i32(node_id); // leader

                // Leader epoch (v7+)
                if header.api_version >= 7 {
                    response.put_i32(0);
                }

                // Replica nodes (compact array)
                encode_unsigned_varint(&mut response, 2); // 1 + 1
                response.put_i32(node_id);

                // ISR nodes (compact array)
                encode_unsigned_varint(&mut response, 2); // 1 + 1
                response.put_i32(node_id);

                // Offline replicas (compact array, v5+)
                encode_unsigned_varint(&mut response, 1); // 0 + 1

                encode_empty_tagged_fields(&mut response);
            }

            // Topic authorized operations (v8+)
            if header.api_version >= 8 {
                response.put_i32(-2147483648); // Unknown
            }

            encode_empty_tagged_fields(&mut response);
        }

        // Add topics that weren't found
        for topic_name in &topics {
            if !requested_topics.iter().any(|t| &t.name == topic_name) {
                response.put_i16(ErrorCode::UnknownTopicOrPartition.as_i16());
                encode_compact_string(&mut response, topic_name);
                if header.api_version >= 10 {
                    response.put_slice(&[0u8; 16]);
                }
                response.put_u8(0);
                encode_unsigned_varint(&mut response, 1); // Empty partitions
                if header.api_version >= 8 {
                    response.put_i32(-2147483648);
                }
                encode_empty_tagged_fields(&mut response);
            }
        }

        // Cluster authorized operations (v8+)
        if header.api_version >= 8 {
            response.put_i32(-2147483648); // Unknown
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol (v0-v8)

        // Throttle time (v3+)
        if header.api_version >= 3 {
            response.put_i32(0);
        }

        // Brokers array
        response.put_i32(1);
        response.put_i32(node_id);
        encode_string(&mut response, host);
        response.put_i32(port);
        if header.api_version >= 1 {
            encode_nullable_string(&mut response, None); // rack
        }

        // Cluster ID (v2+)
        if header.api_version >= 2 {
            encode_nullable_string(&mut response, Some("streamhouse"));
        }

        // Controller ID (v1+)
        if header.api_version >= 1 {
            response.put_i32(node_id);
        }

        // Count unknown topics
        let unknown_topics: Vec<_> = topics
            .iter()
            .filter(|t| !requested_topics.iter().any(|rt| &rt.name == *t))
            .collect();

        // Topics array - include both found and not-found topics
        let total_count = requested_topics.len() + unknown_topics.len();
        response.put_i32(total_count as i32);

        // First, write found topics
        for topic in &requested_topics {
            response.put_i16(ErrorCode::None.as_i16());
            encode_string(&mut response, &topic.name);

            if header.api_version >= 1 {
                response.put_u8(0); // is_internal
            }

            // Partitions array
            response.put_i32(topic.partition_count as i32);
            for partition_id in 0..topic.partition_count {
                response.put_i16(ErrorCode::None.as_i16());
                response.put_i32(partition_id as i32);
                response.put_i32(node_id); // leader

                if header.api_version >= 7 {
                    response.put_i32(0); // leader epoch
                }

                // Replicas
                response.put_i32(1);
                response.put_i32(node_id);

                // ISR
                response.put_i32(1);
                response.put_i32(node_id);

                // Offline replicas (v5+)
                if header.api_version >= 5 {
                    response.put_i32(0);
                }
            }

            // Topic authorized operations (v8)
            if header.api_version >= 8 {
                response.put_i32(-2147483648);
            }
        }

        // Then, write error responses for unknown topics
        for topic_name in &unknown_topics {
            response.put_i16(ErrorCode::UnknownTopicOrPartition.as_i16());
            encode_string(&mut response, topic_name);

            if header.api_version >= 1 {
                response.put_u8(0); // is_internal
            }

            // Empty partitions array
            response.put_i32(0);

            // Topic authorized operations (v8)
            if header.api_version >= 8 {
                response.put_i32(-2147483648);
            }
        }

        // Cluster authorized operations (v8)
        if header.api_version >= 8 {
            response.put_i32(-2147483648);
        }
    }

    Ok(response)
}
