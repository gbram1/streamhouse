//! Produce API handler (API Key 0)
//!
//! Writes records to topic partitions.

use bytes::{Buf, BufMut, BytesMut};
use tracing::{debug, warn};

use crate::codec::{
    encode_compact_string, encode_empty_tagged_fields, encode_string, encode_unsigned_varint,
    parse_array, parse_compact_array, parse_compact_nullable_bytes, parse_compact_string,
    parse_nullable_bytes, parse_nullable_string, parse_string, skip_tagged_fields, RequestHeader,
};
use crate::error::{ErrorCode, KafkaError, KafkaResult};
use crate::server::KafkaServerState;
use crate::types::CompressionType;

/// Handle Produce request
pub async fn handle_produce(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request based on version
    let (acks, timeout_ms, topics) = if header.api_version >= 9 {
        // Compact protocol (v9+)
        if body.len() < 2 {
            return Err(KafkaError::InvalidRequest("Body too short".to_string()));
        }

        // Transactional ID (nullable compact string)
        let _transactional_id = parse_compact_string(body).ok();

        let acks = body.get_i16();
        let timeout_ms = body.get_i32();

        let topics = parse_compact_array(body, |b| {
            let name = parse_compact_string(b)?;
            let partitions = parse_compact_array(b, |b| {
                let index = b.get_i32();
                let records = parse_compact_nullable_bytes(b)?;
                skip_tagged_fields(b)?;
                Ok((index, records))
            })?;
            skip_tagged_fields(b)?;
            Ok((name, partitions))
        })?;

        skip_tagged_fields(body)?;

        (acks, timeout_ms, topics)
    } else {
        // Legacy protocol (v0-v8)
        let transactional_id = if header.api_version >= 3 {
            parse_nullable_string(body)?
        } else {
            None
        };

        if body.len() < 6 {
            return Err(KafkaError::InvalidRequest("Body too short".to_string()));
        }

        let acks = body.get_i16();
        let timeout_ms = body.get_i32();

        let topics = parse_array(body, |b| {
            let name = parse_string(b)?;
            let partitions = parse_array(b, |b| {
                let index = b.get_i32();
                let records = parse_nullable_bytes(b)?;
                Ok((index, records))
            })?;
            Ok((name, partitions))
        })?;

        (acks, timeout_ms, topics)
    };

    debug!(
        "Produce: acks={}, timeout={}, topics={}",
        acks,
        timeout_ms,
        topics.len()
    );

    // Process each topic
    let mut topic_responses: Vec<TopicProduceResponse> = Vec::new();

    for (topic_name, partitions) in topics {
        let mut partition_responses: Vec<PartitionProduceResponse> = Vec::new();

        for (partition_index, records) in partitions {
            let response = match records {
                Some(record_data) => {
                    process_records(state, &topic_name, partition_index as u32, record_data).await
                }
                None => PartitionProduceResponse {
                    partition_index,
                    error_code: ErrorCode::InvalidRecord,
                    base_offset: -1,
                    log_append_time: -1,
                    log_start_offset: 0,
                },
            };
            partition_responses.push(response);
        }

        topic_responses.push(TopicProduceResponse {
            name: topic_name,
            partitions: partition_responses,
        });
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 9 {
        // Compact protocol
        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for topic in &topic_responses {
            encode_compact_string(&mut response, &topic.name);
            encode_unsigned_varint(&mut response, (topic.partitions.len() + 1) as u64);

            for partition in &topic.partitions {
                response.put_i32(partition.partition_index);
                response.put_i16(partition.error_code.as_i16());
                response.put_i64(partition.base_offset);
                response.put_i64(partition.log_append_time);
                response.put_i64(partition.log_start_offset);

                // Record errors (empty array)
                encode_unsigned_varint(&mut response, 1);

                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        // Throttle time
        response.put_i32(0);

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol
        response.put_i32(topic_responses.len() as i32);

        for topic in &topic_responses {
            encode_string(&mut response, &topic.name);
            response.put_i32(topic.partitions.len() as i32);

            for partition in &topic.partitions {
                response.put_i32(partition.partition_index);
                response.put_i16(partition.error_code.as_i16());
                response.put_i64(partition.base_offset);

                if header.api_version >= 2 {
                    response.put_i64(partition.log_append_time);
                }

                if header.api_version >= 5 {
                    response.put_i64(partition.log_start_offset);
                }

                // Record errors (v8+)
                if header.api_version >= 8 {
                    response.put_i32(0);
                }
            }
        }

        // Throttle time (v1+)
        if header.api_version >= 1 {
            response.put_i32(0);
        }
    }

    Ok(response)
}

struct TopicProduceResponse {
    name: String,
    partitions: Vec<PartitionProduceResponse>,
}

struct PartitionProduceResponse {
    partition_index: i32,
    error_code: ErrorCode,
    base_offset: i64,
    log_append_time: i64,
    log_start_offset: i64,
}

async fn process_records(
    state: &KafkaServerState,
    topic_name: &str,
    partition_id: u32,
    record_data: Vec<u8>,
) -> PartitionProduceResponse {
    // Parse RecordBatch
    let records = match parse_record_batch(&record_data) {
        Ok(records) => records,
        Err(e) => {
            warn!("Failed to parse record batch: {}", e);
            return PartitionProduceResponse {
                partition_index: partition_id as i32,
                error_code: ErrorCode::CorruptMessage,
                base_offset: -1,
                log_append_time: -1,
                log_start_offset: 0,
            };
        }
    };

    // Get writer from pool
    let writer = match state.writer_pool.get_writer(topic_name, partition_id).await {
        Ok(w) => w,
        Err(e) => {
            warn!("Failed to get writer: {}", e);
            return PartitionProduceResponse {
                partition_index: partition_id as i32,
                error_code: ErrorCode::KafkaStorageError,
                base_offset: -1,
                log_append_time: -1,
                log_start_offset: 0,
            };
        }
    };

    let mut writer_guard = writer.lock().await;

    // Write records
    let mut base_offset = 0i64;
    let timestamp = chrono::Utc::now().timestamp_millis() as u64;

    for record in records {
        let value = bytes::Bytes::from(record.value);
        let key = record.key.map(bytes::Bytes::from);

        match writer_guard.append(key, value, timestamp).await {
            Ok(offset) => {
                if base_offset == 0 {
                    base_offset = offset as i64;
                }
            }
            Err(e) => {
                warn!("Failed to write record: {}", e);
                return PartitionProduceResponse {
                    partition_index: partition_id as i32,
                    error_code: ErrorCode::KafkaStorageError,
                    base_offset: -1,
                    log_append_time: -1,
                    log_start_offset: 0,
                };
            }
        }
    }

    PartitionProduceResponse {
        partition_index: partition_id as i32,
        error_code: ErrorCode::None,
        base_offset,
        log_append_time: timestamp as i64,
        log_start_offset: 0,
    }
}

struct ParsedRecord {
    key: Option<Vec<u8>>,
    value: Vec<u8>,
    headers: Vec<(String, Vec<u8>)>,
}

/// Parse a Kafka RecordBatch
fn parse_record_batch(data: &[u8]) -> KafkaResult<Vec<ParsedRecord>> {
    if data.len() < 61 {
        return Err(KafkaError::Protocol("RecordBatch too short".to_string()));
    }

    let mut buf = BytesMut::from(data);

    // RecordBatch header
    let _base_offset = buf.get_i64();
    let batch_length = buf.get_i32() as usize;
    let _partition_leader_epoch = buf.get_i32();
    let magic = buf.get_i8();

    if magic != 2 {
        return Err(KafkaError::Protocol(format!(
            "Unsupported magic byte: {}",
            magic
        )));
    }

    let _crc = buf.get_u32();
    let attributes = buf.get_i16();
    let _last_offset_delta = buf.get_i32();
    let _first_timestamp = buf.get_i64();
    let _max_timestamp = buf.get_i64();
    let _producer_id = buf.get_i64();
    let _producer_epoch = buf.get_i16();
    let _base_sequence = buf.get_i32();
    let record_count = buf.get_i32();

    // Get compression type from attributes
    let compression = CompressionType::from_attributes(attributes);

    // Decompress if needed
    let record_data = if compression != CompressionType::None {
        decompress(&buf, compression)?
    } else {
        buf.to_vec()
    };

    let mut record_buf = BytesMut::from(&record_data[..]);

    // Parse individual records
    let mut records = Vec::with_capacity(record_count as usize);

    for _ in 0..record_count {
        let record = parse_record(&mut record_buf)?;
        records.push(record);
    }

    Ok(records)
}

fn parse_record(buf: &mut BytesMut) -> KafkaResult<ParsedRecord> {
    // Length (varint)
    let _length = parse_varint(buf)?;

    // Attributes (int8)
    let _attributes = buf.get_i8();

    // Timestamp delta (varint)
    let _timestamp_delta = parse_varint(buf)?;

    // Offset delta (varint)
    let _offset_delta = parse_varint(buf)?;

    // Key (varlen bytes)
    let key_length = parse_varint(buf)?;
    let key = if key_length >= 0 {
        let len = key_length as usize;
        Some(buf.split_to(len).to_vec())
    } else {
        None
    };

    // Value (varlen bytes)
    let value_length = parse_varint(buf)?;
    let value = if value_length >= 0 {
        let len = value_length as usize;
        buf.split_to(len).to_vec()
    } else {
        vec![]
    };

    // Headers (array)
    let header_count = parse_varint(buf)?;
    let mut headers = Vec::new();

    for _ in 0..header_count {
        let key_len = parse_varint(buf)? as usize;
        let key = String::from_utf8(buf.split_to(key_len).to_vec())
            .map_err(|e| KafkaError::Protocol(format!("Invalid header key: {}", e)))?;

        let value_len = parse_varint(buf)?;
        let value = if value_len >= 0 {
            buf.split_to(value_len as usize).to_vec()
        } else {
            vec![]
        };

        headers.push((key, value));
    }

    Ok(ParsedRecord {
        key,
        value,
        headers,
    })
}

fn parse_varint(buf: &mut BytesMut) -> KafkaResult<i64> {
    let mut result: i64 = 0;
    let mut shift = 0;

    loop {
        if buf.is_empty() {
            return Err(KafkaError::Protocol(
                "Buffer too short for varint".to_string(),
            ));
        }

        let byte = buf.get_u8();
        result |= ((byte & 0x7F) as i64) << shift;

        if byte & 0x80 == 0 {
            break;
        }

        shift += 7;
        if shift >= 64 {
            return Err(KafkaError::Protocol("Varint too long".to_string()));
        }
    }

    // ZigZag decode
    Ok((result >> 1) ^ -(result & 1))
}

fn decompress(data: &BytesMut, compression: CompressionType) -> KafkaResult<Vec<u8>> {
    match compression {
        CompressionType::None => Ok(data.to_vec()),
        CompressionType::Gzip => {
            use flate2::read::GzDecoder;
            use std::io::Read;
            let mut decoder = GzDecoder::new(&data[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| KafkaError::Compression(e.to_string()))?;
            Ok(decompressed)
        }
        CompressionType::Snappy => {
            let decompressed = snap::raw::Decoder::new()
                .decompress_vec(data)
                .map_err(|e| KafkaError::Compression(e.to_string()))?;
            Ok(decompressed)
        }
        CompressionType::Lz4 => {
            let decompressed = lz4_flex::decompress_size_prepended(data)
                .map_err(|e| KafkaError::Compression(e.to_string()))?;
            Ok(decompressed)
        }
        CompressionType::Zstd => {
            let decompressed =
                zstd::decode_all(&data[..]).map_err(|e| KafkaError::Compression(e.to_string()))?;
            Ok(decompressed)
        }
    }
}
