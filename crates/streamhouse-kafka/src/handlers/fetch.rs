//! Fetch API handler (API Key 1)
//!
//! Reads records from topic partitions.

use bytes::{Buf, BufMut, BytesMut};
use tracing::debug;

use streamhouse_storage::PartitionReader;

use crate::codec::{
    encode_compact_nullable_bytes, encode_compact_string, encode_empty_tagged_fields,
    encode_nullable_bytes, encode_string, encode_unsigned_varint, parse_array, parse_compact_array,
    parse_compact_string, parse_string, skip_tagged_fields, RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;

/// Handle Fetch request
pub async fn handle_fetch(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse request
    let (max_wait_ms, min_bytes, max_bytes, topics) = if header.api_version >= 12 {
        // Compact protocol
        // Skip cluster ID for v12+
        if header.api_version >= 15 {
            let _cluster_id = parse_compact_string(body).ok();
        }

        let _replica_id = body.get_i32();
        let max_wait_ms = body.get_i32();
        let min_bytes = body.get_i32();
        let max_bytes = body.get_i32();
        let _isolation_level = body.get_i8();
        let _session_id = body.get_i32();
        let _session_epoch = body.get_i32();

        let topics = parse_compact_array(body, |b| {
            let name = parse_compact_string(b)?;
            let partitions = parse_compact_array(b, |b| {
                let partition = b.get_i32();
                let _current_leader_epoch = b.get_i32();
                let fetch_offset = b.get_i64();
                let _last_fetched_epoch = if header.api_version >= 12 {
                    b.get_i32()
                } else {
                    -1
                };
                let _log_start_offset = b.get_i64();
                let partition_max_bytes = b.get_i32();
                skip_tagged_fields(b)?;
                Ok(FetchPartition {
                    partition,
                    fetch_offset,
                    partition_max_bytes,
                })
            })?;
            skip_tagged_fields(b)?;
            Ok((name, partitions))
        })?;

        // Forgotten topics
        let _forgotten = parse_compact_array(body, |b| {
            let _name = parse_compact_string(b)?;
            let _partitions = parse_compact_array(b, |b| Ok(b.get_i32()))?;
            skip_tagged_fields(b)?;
            Ok(())
        })?;

        // Rack ID
        let _rack_id = parse_compact_string(body).ok();

        skip_tagged_fields(body)?;

        (max_wait_ms, min_bytes, max_bytes, topics)
    } else {
        // Legacy protocol
        let _replica_id = body.get_i32();
        let max_wait_ms = body.get_i32();
        let min_bytes = body.get_i32();

        let max_bytes = if header.api_version >= 3 {
            body.get_i32()
        } else {
            i32::MAX
        };

        let _isolation_level = if header.api_version >= 4 {
            body.get_i8()
        } else {
            0
        };

        let _session_id = if header.api_version >= 7 {
            body.get_i32()
        } else {
            0
        };

        let _session_epoch = if header.api_version >= 7 {
            body.get_i32()
        } else {
            -1
        };

        let topics = parse_array(body, |b| {
            let name = parse_string(b)?;
            let partitions = parse_array(b, |b| {
                let partition = b.get_i32();
                let _current_leader_epoch = if header.api_version >= 9 {
                    b.get_i32()
                } else {
                    -1
                };
                let fetch_offset = b.get_i64();
                let _log_start_offset = if header.api_version >= 5 {
                    b.get_i64()
                } else {
                    -1
                };
                let partition_max_bytes = b.get_i32();
                Ok(FetchPartition {
                    partition,
                    fetch_offset,
                    partition_max_bytes,
                })
            })?;
            Ok((name, partitions))
        })?;

        (max_wait_ms, min_bytes, max_bytes, topics)
    };

    debug!(
        "Fetch: max_wait={}, min_bytes={}, max_bytes={}, topics={}",
        max_wait_ms,
        min_bytes,
        max_bytes,
        topics.len()
    );

    // Fetch records
    let mut topic_responses = Vec::new();

    for (topic_name, partitions) in topics {
        let mut partition_responses = Vec::new();

        for fp in partitions {
            let response = fetch_partition(
                state,
                &topic_name,
                fp.partition as u32,
                fp.fetch_offset as u64,
                fp.partition_max_bytes as usize,
            )
            .await;
            partition_responses.push(response);
        }

        topic_responses.push(FetchTopicResponse {
            name: topic_name,
            partitions: partition_responses,
        });
    }

    // Build response
    let mut response = BytesMut::new();

    if header.api_version >= 12 {
        // Compact protocol

        // Throttle time
        response.put_i32(0);

        // Error code
        response.put_i16(ErrorCode::None.as_i16());

        // Session ID
        response.put_i32(0);

        // Responses array
        encode_unsigned_varint(&mut response, (topic_responses.len() + 1) as u64);

        for topic in &topic_responses {
            encode_compact_string(&mut response, &topic.name);

            encode_unsigned_varint(&mut response, (topic.partitions.len() + 1) as u64);

            for partition in &topic.partitions {
                response.put_i32(partition.partition_index);
                response.put_i16(partition.error_code.as_i16());
                response.put_i64(partition.high_watermark);
                response.put_i64(partition.last_stable_offset);
                response.put_i64(partition.log_start_offset);

                // Diverging epoch (v12+)
                if header.api_version >= 12 {
                    response.put_i32(-1); // epoch
                    response.put_i64(-1); // end_offset
                }

                // Current leader (v12+)
                if header.api_version >= 12 {
                    response.put_i32(-1); // leader_id
                    response.put_i32(-1); // leader_epoch
                }

                // Snapshot ID (v12+)
                if header.api_version >= 12 {
                    response.put_i64(-1); // end_offset
                    response.put_i32(-1); // epoch
                }

                // Aborted transactions (compact array)
                encode_unsigned_varint(&mut response, 1);

                // Preferred read replica
                response.put_i32(-1);

                // Records
                encode_compact_nullable_bytes(&mut response, partition.records.as_deref());

                encode_empty_tagged_fields(&mut response);
            }

            encode_empty_tagged_fields(&mut response);
        }

        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol

        // Throttle time (v1+)
        if header.api_version >= 1 {
            response.put_i32(0);
        }

        // Error code (v7+)
        if header.api_version >= 7 {
            response.put_i16(ErrorCode::None.as_i16());
        }

        // Session ID (v7+)
        if header.api_version >= 7 {
            response.put_i32(0);
        }

        // Responses array
        response.put_i32(topic_responses.len() as i32);

        for topic in &topic_responses {
            encode_string(&mut response, &topic.name);
            response.put_i32(topic.partitions.len() as i32);

            for partition in &topic.partitions {
                response.put_i32(partition.partition_index);
                response.put_i16(partition.error_code.as_i16());
                response.put_i64(partition.high_watermark);

                // Last stable offset (v4+)
                if header.api_version >= 4 {
                    response.put_i64(partition.last_stable_offset);
                }

                // Log start offset (v5+)
                if header.api_version >= 5 {
                    response.put_i64(partition.log_start_offset);
                }

                // Aborted transactions (v4+)
                if header.api_version >= 4 {
                    response.put_i32(-1); // null array (no transactions)
                }

                // Preferred read replica (v11+)
                if header.api_version >= 11 {
                    response.put_i32(-1);
                }

                // Records
                encode_nullable_bytes(&mut response, partition.records.as_deref());
            }
        }
    }

    Ok(response)
}

struct FetchPartition {
    partition: i32,
    fetch_offset: i64,
    partition_max_bytes: i32,
}

struct FetchTopicResponse {
    name: String,
    partitions: Vec<FetchPartitionResponse>,
}

struct FetchPartitionResponse {
    partition_index: i32,
    error_code: ErrorCode,
    high_watermark: i64,
    last_stable_offset: i64,
    log_start_offset: i64,
    records: Option<Vec<u8>>,
}

async fn fetch_partition(
    state: &KafkaServerState,
    topic_name: &str,
    partition_id: u32,
    offset: u64,
    max_bytes: usize,
) -> FetchPartitionResponse {
    // Get partition info
    let partition_info = match state.metadata.get_partition(topic_name, partition_id).await {
        Ok(Some(info)) => info,
        Ok(None) => {
            return FetchPartitionResponse {
                partition_index: partition_id as i32,
                error_code: ErrorCode::UnknownTopicOrPartition,
                high_watermark: -1,
                last_stable_offset: -1,
                log_start_offset: 0,
                records: None,
            };
        }
        Err(_) => {
            return FetchPartitionResponse {
                partition_index: partition_id as i32,
                error_code: ErrorCode::UnknownServerError,
                high_watermark: -1,
                last_stable_offset: -1,
                log_start_offset: 0,
                records: None,
            };
        }
    };

    let high_watermark = partition_info.high_watermark as i64;

    // Read records from storage
    let records = read_records(state, topic_name, partition_id, offset, max_bytes)
        .await
        .ok();

    FetchPartitionResponse {
        partition_index: partition_id as i32,
        error_code: ErrorCode::None,
        high_watermark,
        last_stable_offset: high_watermark,
        log_start_offset: 0,
        records,
    }
}

async fn read_records(
    state: &KafkaServerState,
    topic_name: &str,
    partition_id: u32,
    offset: u64,
    max_bytes: usize,
) -> KafkaResult<Vec<u8>> {
    // Create a PartitionReader to read from storage
    let reader = PartitionReader::new(
        topic_name.to_string(),
        partition_id,
        state.metadata.clone(),
        state.object_store.clone(),
        state.segment_cache.clone(),
    );

    // Estimate max records based on max_bytes (assume ~100 bytes per record average)
    let max_records = (max_bytes / 100).clamp(1, 10000);

    // Read records from storage
    let read_result = match reader.read(offset, max_records).await {
        Ok(result) => result,
        Err(e) => {
            debug!("Failed to read records from storage: {}", e);
            return Ok(vec![]);
        }
    };

    if read_result.records.is_empty() {
        return Ok(vec![]);
    }

    // Build Kafka RecordBatch from the records
    let base_offset = read_result
        .records
        .first()
        .map(|r| r.offset)
        .unwrap_or(offset);
    let base_timestamp = read_result
        .records
        .first()
        .map(|r| r.timestamp as i64)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let record_batch =
        build_record_batch_from_records(base_offset, base_timestamp, &read_result.records)?;

    Ok(record_batch)
}

fn build_record_batch_from_records(
    base_offset: u64,
    base_timestamp: i64,
    records: &[streamhouse_core::Record],
) -> KafkaResult<Vec<u8>> {
    if records.is_empty() {
        return Ok(vec![]);
    }

    let mut batch = BytesMut::new();

    // RecordBatch header
    batch.put_i64(base_offset as i64);

    // Placeholder for batch length
    let length_pos = batch.len();
    batch.put_i32(0);

    batch.put_i32(0); // partition_leader_epoch
    batch.put_i8(2); // magic (v2 format)

    // Placeholder for CRC
    let crc_pos = batch.len();
    batch.put_u32(0);

    let crc_start = batch.len();

    // Find max timestamp for the batch
    let max_timestamp = records
        .iter()
        .map(|r| r.timestamp as i64)
        .max()
        .unwrap_or(base_timestamp);

    batch.put_i16(0); // attributes (no compression)
    batch.put_i32((records.len() - 1) as i32); // last_offset_delta
    batch.put_i64(base_timestamp); // first_timestamp
    batch.put_i64(max_timestamp); // max_timestamp
    batch.put_i64(-1); // producer_id
    batch.put_i16(-1); // producer_epoch
    batch.put_i32(-1); // base_sequence
    batch.put_i32(records.len() as i32); // record_count

    // Encode each record
    for record in records {
        let offset_delta = (record.offset - base_offset) as i32;
        let timestamp_delta = (record.timestamp as i64) - base_timestamp;
        encode_record_with_deltas(
            &mut batch,
            offset_delta,
            timestamp_delta,
            record.key.as_ref().map(|k| k.as_ref()),
            &record.value,
        );
    }

    // Calculate and update batch length
    let batch_length = batch.len() - 12; // Exclude base_offset and batch_length fields
    let length_bytes = (batch_length as i32).to_be_bytes();
    batch[length_pos..length_pos + 4].copy_from_slice(&length_bytes);

    // Calculate CRC32C (Castagnoli) - Kafka uses this variant
    let crc = crc32c::crc32c(&batch[crc_start..]);
    let crc_bytes = crc.to_be_bytes();
    batch[crc_pos..crc_pos + 4].copy_from_slice(&crc_bytes);

    Ok(batch.to_vec())
}

fn encode_record_with_deltas(
    batch: &mut BytesMut,
    offset_delta: i32,
    timestamp_delta: i64,
    key: Option<&[u8]>,
    value: &[u8],
) {
    // Build record content first to get length
    let mut record = BytesMut::new();

    record.put_i8(0); // attributes

    // Timestamp delta (varint)
    encode_varint(&mut record, timestamp_delta);

    // Offset delta (varint)
    encode_varint(&mut record, offset_delta as i64);

    // Key (varlen bytes)
    match key {
        Some(k) => {
            encode_varint(&mut record, k.len() as i64);
            record.extend_from_slice(k);
        }
        None => {
            encode_varint(&mut record, -1);
        }
    }

    // Value (varlen bytes)
    encode_varint(&mut record, value.len() as i64);
    record.extend_from_slice(value);

    // Headers (empty array)
    encode_varint(&mut record, 0);

    // Write length-prefixed record
    encode_varint(batch, record.len() as i64);
    batch.extend_from_slice(&record);
}

fn encode_varint(buf: &mut BytesMut, value: i64) {
    // ZigZag encode
    let mut v = ((value << 1) ^ (value >> 63)) as u64;

    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;

        if v != 0 {
            byte |= 0x80;
        }

        buf.put_u8(byte);

        if v == 0 {
            break;
        }
    }
}
