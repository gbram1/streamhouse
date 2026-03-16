//! Kafka binary wire protocol helpers for the load test.
//! Extracted from kafka_smoke_test.rs.

#![allow(dead_code)]

use bytes::{Buf, BufMut, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub async fn send_request(stream: &mut TcpStream, payload: &[u8]) -> anyhow::Result<()> {
    let mut frame = BytesMut::new();
    frame.put_i32(payload.len() as i32);
    frame.extend_from_slice(payload);
    stream.write_all(&frame).await?;
    Ok(())
}

pub async fn recv_response(stream: &mut TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = i32::from_be_bytes(len_buf) as usize;
    let mut response = vec![0u8; len];
    stream.read_exact(&mut response).await?;
    Ok(response)
}

pub fn build_api_versions_request(correlation_id: i32, client_id: &str) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(18); // api_key = ApiVersions
    buf.put_i16(0);
    buf.put_i32(correlation_id);
    buf.put_i16(client_id.len() as i16);
    buf.extend_from_slice(client_id.as_bytes());
    buf.to_vec()
}

pub fn build_batched_produce_request(
    correlation_id: i32,
    client_id: &str,
    topic: &str,
    partition: i32,
    records: &[(Vec<u8>, Vec<u8>)],
) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(0); // api_key = Produce
    buf.put_i16(0);
    buf.put_i32(correlation_id);
    buf.put_i16(client_id.len() as i16);
    buf.extend_from_slice(client_id.as_bytes());
    buf.put_i16(1); // acks
    buf.put_i32(30_000); // timeout

    buf.put_i32(1); // 1 topic
    buf.put_i16(topic.len() as i16);
    buf.extend_from_slice(topic.as_bytes());

    buf.put_i32(1); // 1 partition
    buf.put_i32(partition);

    let record_batch = build_lz4_record_batch(records);
    buf.put_i32(record_batch.len() as i32);
    buf.extend_from_slice(&record_batch);

    buf.to_vec()
}

pub fn build_fetch_request(
    correlation_id: i32,
    client_id: &str,
    topic: &str,
    partition: i32,
    offset: i64,
) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(1); // api_key = Fetch
    buf.put_i16(0);
    buf.put_i32(correlation_id);
    buf.put_i16(client_id.len() as i16);
    buf.extend_from_slice(client_id.as_bytes());
    buf.put_i32(-1); // replica_id
    buf.put_i32(5000); // max_wait_ms
    buf.put_i32(1); // min_bytes

    buf.put_i32(1); // 1 topic
    buf.put_i16(topic.len() as i16);
    buf.extend_from_slice(topic.as_bytes());
    buf.put_i32(1); // 1 partition
    buf.put_i32(partition);
    buf.put_i64(offset);
    buf.put_i32(10 * 1024 * 1024); // 10MB max

    buf.to_vec()
}

pub fn build_sasl_handshake_request(
    correlation_id: i32,
    client_id: &str,
    mechanism: &str,
) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(17); // SaslHandshake
    buf.put_i16(0);
    buf.put_i32(correlation_id);
    buf.put_i16(client_id.len() as i16);
    buf.extend_from_slice(client_id.as_bytes());
    buf.put_i16(mechanism.len() as i16);
    buf.extend_from_slice(mechanism.as_bytes());
    buf.to_vec()
}

pub fn build_sasl_authenticate_request(
    correlation_id: i32,
    client_id: &str,
    username: &str,
    password: &str,
) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(36); // SaslAuthenticate
    buf.put_i16(0);
    buf.put_i32(correlation_id);
    buf.put_i16(client_id.len() as i16);
    buf.extend_from_slice(client_id.as_bytes());

    let mut auth_bytes = Vec::new();
    auth_bytes.push(0);
    auth_bytes.extend_from_slice(username.as_bytes());
    auth_bytes.push(0);
    auth_bytes.extend_from_slice(password.as_bytes());

    buf.put_i32(auth_bytes.len() as i32);
    buf.extend_from_slice(&auth_bytes);
    buf.to_vec()
}

pub fn parse_produce_error(resp: &[u8]) -> i16 {
    let mut buf = BytesMut::from(resp);
    let _correlation_id = buf.get_i32();
    let _topic_count = buf.get_i32();
    let name_len = buf.get_i16() as usize;
    buf.advance(name_len);
    let _part_count = buf.get_i32();
    let _partition = buf.get_i32();
    buf.get_i16()
}

pub fn parse_sasl_handshake_error(resp: &[u8]) -> i16 {
    let mut buf = BytesMut::from(resp);
    let _correlation_id = buf.get_i32();
    buf.get_i16()
}

pub fn parse_sasl_authenticate_error(resp: &[u8]) -> i16 {
    let mut buf = BytesMut::from(resp);
    let _correlation_id = buf.get_i32();
    buf.get_i16()
}

fn build_lz4_record_batch(records: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut raw_records = BytesMut::new();
    for (i, (key, value)) in records.iter().enumerate() {
        let mut record = BytesMut::new();
        record.put_i8(0); // attributes
        write_varint(&mut record, 0); // timestamp_delta
        write_varint(&mut record, i as i64); // offset_delta
        write_varint(&mut record, key.len() as i64);
        record.extend_from_slice(key);
        write_varint(&mut record, value.len() as i64);
        record.extend_from_slice(value);
        write_varint(&mut record, 0); // headers count
        write_varint(&mut raw_records, record.len() as i64);
        raw_records.extend_from_slice(&record);
    }

    let compressed = lz4_flex::compress_prepend_size(&raw_records);

    let mut batch = BytesMut::new();
    batch.put_i64(0); // base_offset
    let length_pos = batch.len();
    batch.put_i32(0); // placeholder batch length
    batch.put_i32(0); // partition_leader_epoch
    batch.put_i8(2); // magic = 2
    let crc_pos = batch.len();
    batch.put_u32(0); // placeholder CRC
    let crc_start = batch.len();

    let timestamp = chrono::Utc::now().timestamp_millis();
    batch.put_i16(3); // LZ4 compression
    batch.put_i32((records.len() - 1) as i32); // last_offset_delta
    batch.put_i64(timestamp);
    batch.put_i64(timestamp);
    batch.put_i64(-1); // producer_id
    batch.put_i16(-1); // producer_epoch
    batch.put_i32(-1); // base_sequence
    batch.put_i32(records.len() as i32);
    batch.extend_from_slice(&compressed);

    let batch_length = (batch.len() - 12) as i32;
    batch[length_pos..length_pos + 4].copy_from_slice(&batch_length.to_be_bytes());

    let crc = crc32c::crc32c(&batch[crc_start..]);
    batch[crc_pos..crc_pos + 4].copy_from_slice(&crc.to_be_bytes());

    batch.to_vec()
}

fn write_varint(buf: &mut BytesMut, value: i64) {
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
