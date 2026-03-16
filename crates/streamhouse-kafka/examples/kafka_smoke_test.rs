//! Kafka Protocol Smoke Test — All Optimizations
//!
//! Tests the Kafka protocol with all 5 performance optimizations:
//!   1. Batching — 100 records per produce request
//!   2. WAL batch mode — group commit, fewer fsyncs
//!   3. Parallelism — multiple connections producing concurrently
//!   4. Compression — LZ4 compressed RecordBatches
//!   5. Writer pool background flush — async segment uploads
//!
//! Run with:
//!   cargo run --release -p streamhouse-kafka --example kafka_smoke_test
//!
//! No external dependencies (no kcat, no Docker, no running server).

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Buf, BufMut, BytesMut};
use object_store::memory::InMemory;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};
use streamhouse_storage::{SegmentCache, WriteConfig, WriterPool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

use streamhouse_kafka::server::{KafkaServer, KafkaServerConfig};

const NUM_TOPICS: usize = 10;
const MESSAGES_PER_TOPIC: usize = 1_000;
const BATCH_SIZE: usize = 100;
const TOTAL_MESSAGES: usize = NUM_TOPICS * MESSAGES_PER_TOPIC;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,streamhouse_kafka=warn,streamhouse_storage=warn")
        .init();

    println!("=== Kafka Protocol Smoke Test ===");
    println!("    {NUM_TOPICS} topics x {MESSAGES_PER_TOPIC} msgs = {TOTAL_MESSAGES} total");
    println!("    Batch: {BATCH_SIZE} records/request | Compression: LZ4");
    println!("    Parallelism: {NUM_TOPICS} concurrent connections\n");

    // ── 1. Setup ─────────────────────────────────────────────────────────
    let metadata = Arc::new(SqliteMetadataStore::new_in_memory().await?);
    let object_store: Arc<dyn object_store::ObjectStore> = Arc::new(InMemory::new());

    let temp_dir = tempfile::tempdir()?;
    let cache = Arc::new(SegmentCache::new(
        temp_dir.path().join("cache"),
        64 * 1024 * 1024,
    )?);

    // Optimization #2: WAL with batch mode enabled
    let wal_dir = temp_dir.path().join("wal");
    std::fs::create_dir_all(&wal_dir)?;

    let config = WriteConfig {
        segment_max_size: 4 * 1024 * 1024,
        segment_max_age_ms: 60_000,
        s3_bucket: "test".to_string(),
        s3_region: "local".to_string(),
        block_size_target: 4096,
        wal_config: Some(streamhouse_storage::wal::WALConfig {
            directory: wal_dir,
            sync_policy: streamhouse_storage::wal::SyncPolicy::Interval {
                interval: Duration::from_millis(100),
            },
            max_size_bytes: 1024 * 1024 * 1024,
            batch_enabled: true, // group commit
            batch_max_records: 1000,
            batch_max_bytes: 4 * 1024 * 1024,
            batch_max_age_ms: 10, // flush batch after 10ms
            agent_id: None,
        }),
        ..Default::default()
    };

    let writer_pool = Arc::new(WriterPool::new(
        metadata.clone(),
        object_store.clone(),
        config,
    ));

    // Optimization #5: background flush every 2 seconds
    let _flush_handle = Arc::clone(&writer_pool).start_background_flush(Duration::from_secs(2));

    // Create topics
    let topic_names: Vec<String> = (0..NUM_TOPICS).map(|i| format!("topic-{i}")).collect();
    for name in &topic_names {
        metadata
            .create_topic(TopicConfig {
                name: name.clone(),
                partition_count: 1,
                retention_ms: Some(86_400_000),
                cleanup_policy: Default::default(),
                config: Default::default(),
            })
            .await?;
    }
    println!("[setup] Created {NUM_TOPICS} topics");

    // Start Kafka server
    let kafka_config = KafkaServerConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        node_id: 0,
        advertised_host: "127.0.0.1".to_string(),
        advertised_port: 0,
    };

    let bound = KafkaServer::bind(
        kafka_config,
        metadata.clone(),
        writer_pool.clone(),
        cache,
        object_store,
    )
    .await?;

    let addr = bound.local_addr()?;
    println!("[setup] Kafka server on {addr}\n");

    tokio::spawn(async move {
        if let Err(e) = bound.run().await {
            eprintln!("server error: {e}");
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // ── 2. Produce — parallel connections, batched, LZ4 compressed ───────
    // Optimization #3: one connection per topic, all producing concurrently
    let batches_per_topic = MESSAGES_PER_TOPIC / BATCH_SIZE;

    println!("[produce] Sending {TOTAL_MESSAGES} messages ({NUM_TOPICS} parallel connections)...");

    let produce_start = Instant::now();
    let mut join_set = JoinSet::new();

    for (topic_idx, topic) in topic_names.iter().enumerate() {
        let topic = topic.clone();
        let addr = addr;

        join_set.spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();

            // Handshake
            let req = build_api_versions_request(0);
            send_request(&mut stream, &req).await.unwrap();
            let _ = recv_response(&mut stream).await.unwrap();

            let mut errors = 0u64;
            let mut corr_id: i32 = 1;

            for batch_idx in 0..batches_per_topic {
                let base = batch_idx * BATCH_SIZE;

                let records: Vec<(Vec<u8>, Vec<u8>)> = (0..BATCH_SIZE)
                    .map(|i| {
                        let msg_idx = base + i;
                        let key = format!("key-{topic_idx}-{msg_idx}").into_bytes();
                        let value = format!(
                            r#"{{"topic":"{}","seq":{},"data":"payload-{:06}"}}"#,
                            topic, msg_idx, msg_idx
                        )
                        .into_bytes();
                        (key, value)
                    })
                    .collect();

                // Optimization #1 + #4: batched produce with LZ4 compression
                let req = build_batched_produce_request(corr_id, &topic, 0, &records);
                send_request(&mut stream, &req).await.unwrap();
                let resp = recv_response(&mut stream).await.unwrap();

                if parse_produce_error(&resp) != 0 {
                    errors += 1;
                }
                corr_id += 1;
            }

            (topic, errors)
        });
    }

    let mut total_errors = 0u64;
    while let Some(result) = join_set.join_next().await {
        let (topic, errors) = result?;
        if errors > 0 {
            eprintln!("  {topic}: {errors} errors");
        }
        total_errors += errors;
    }

    let produce_elapsed = produce_start.elapsed();
    let produce_rate = TOTAL_MESSAGES as f64 / produce_elapsed.as_secs_f64();

    println!(
        "[produce] Done: {TOTAL_MESSAGES} messages in {:.3}s ({:.0} msg/s) errors={total_errors}",
        produce_elapsed.as_secs_f64(),
        produce_rate,
    );
    assert_eq!(total_errors, 0, "Expected zero produce errors");

    // ── 3. Flush ─────────────────────────────────────────────────────────
    print!("[flush]   Flushing to storage...");
    let flush_start = Instant::now();
    writer_pool.flush_all().await?;
    println!(" done in {:.3}s", flush_start.elapsed().as_secs_f64());

    // ── 4. Consume — parallel connections ────────────────────────────────
    println!("\n[consume] Reading back from all {NUM_TOPICS} topics (parallel)...");
    let consume_start = Instant::now();
    let mut consume_set = JoinSet::new();

    for topic in topic_names.iter() {
        let topic = topic.clone();
        let addr = addr;

        consume_set.spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();

            // Handshake
            let req = build_api_versions_request(0);
            send_request(&mut stream, &req).await.unwrap();
            let _ = recv_response(&mut stream).await.unwrap();

            let mut topic_consumed = 0usize;
            let mut offset: i64 = 0;
            let mut corr_id: i32 = 1;

            loop {
                let req = build_fetch_request(corr_id, &topic, 0, offset);
                corr_id += 1;
                send_request(&mut stream, &req).await.unwrap();
                let resp = recv_response(&mut stream).await.unwrap();

                let records = parse_fetch_response(&resp);
                if records.is_empty() {
                    break;
                }

                for (key, _value) in &records {
                    if let Some(k) = key {
                        let k_str = String::from_utf8_lossy(k);
                        assert!(k_str.starts_with("key-"), "unexpected key: {k_str}");
                    }
                    topic_consumed += 1;
                    offset += 1;
                }
            }

            (topic, topic_consumed)
        });
    }

    let mut total_consumed = 0usize;
    while let Some(result) = consume_set.join_next().await {
        let (topic, count) = result?;
        println!("  {topic}: {count} records");
        total_consumed += count;
    }

    let consume_elapsed = consume_start.elapsed();
    let consume_rate = total_consumed as f64 / consume_elapsed.as_secs_f64();

    println!(
        "[consume] Done: {total_consumed} messages in {:.3}s ({:.0} msg/s)",
        consume_elapsed.as_secs_f64(),
        consume_rate
    );

    // ── 5. Summary ───────────────────────────────────────────────────────
    println!("\n=== Results ===");
    println!("  Produced:  {TOTAL_MESSAGES}");
    println!("  Consumed:  {total_consumed}");
    println!("  Errors:    {total_errors}");
    println!(
        "  Produce:   {:.3}s ({:.0} msg/s)",
        produce_elapsed.as_secs_f64(),
        produce_rate
    );
    println!(
        "  Consume:   {:.3}s ({:.0} msg/s)",
        consume_elapsed.as_secs_f64(),
        consume_rate
    );

    assert_eq!(
        total_consumed, TOTAL_MESSAGES,
        "Expected to consume all {TOTAL_MESSAGES} messages, got {total_consumed}"
    );

    println!("\n=== All checks passed! ===");

    // ── 6. SASL Authentication Test ─────────────────────────────────────
    println!("\n=== SASL Authentication Test ===");

    // Create an organization and API key
    let org = metadata
        .create_organization(streamhouse_metadata::CreateOrganization {
            name: "Test Org".to_string(),
            slug: "test-org".to_string(),
            plan: streamhouse_metadata::OrganizationPlan::default(),
            settings: Default::default(),
            clerk_id: None,
        })
        .await?;
    println!("[sasl] Created org: id={}", org.id);

    let (raw_key, key_hash, key_prefix) = streamhouse_metadata::tenant::generate_api_key("sk_test");
    let _api_key = metadata
        .create_api_key(
            &org.id,
            streamhouse_metadata::CreateApiKey {
                name: "smoke-test-key".to_string(),
                permissions: vec!["read".to_string(), "write".to_string()],
                scopes: vec![],
                expires_in_ms: None,
            },
            &key_hash,
            &key_prefix,
        )
        .await?;
    println!("[sasl] Created API key: prefix={}", key_prefix);

    // Create a topic scoped to this org
    let sasl_topic = "sasl-test-topic";
    metadata
        .create_topic_for_org(
            &org.id,
            TopicConfig {
                name: sasl_topic.to_string(),
                partition_count: 1,
                retention_ms: Some(86_400_000),
                cleanup_policy: Default::default(),
                config: Default::default(),
            },
        )
        .await?;
    println!("[sasl] Created org-scoped topic: {sasl_topic}");

    // Connect with SASL authentication flow
    {
        let mut stream = TcpStream::connect(addr).await?;

        // Step 1: ApiVersions
        let req = build_api_versions_request(0);
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;
        println!("[sasl] ApiVersions OK (response {} bytes)", resp.len());

        // Step 2: SaslHandshake — request PLAIN mechanism
        let req = build_sasl_handshake_request(1, "PLAIN");
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;
        let handshake_error = parse_sasl_handshake_error(&resp);
        assert_eq!(handshake_error, 0, "SaslHandshake should succeed for PLAIN");
        println!("[sasl] SaslHandshake OK (PLAIN accepted)");

        // Step 3: SaslAuthenticate — send API key as SASL/PLAIN credentials
        let req = build_sasl_authenticate_request(2, &raw_key, &raw_key);
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;
        let auth_error = parse_sasl_authenticate_error(&resp);
        assert_eq!(
            auth_error, 0,
            "SaslAuthenticate should succeed with valid key"
        );
        println!(
            "[sasl] SaslAuthenticate OK (authenticated as org={})",
            org.id
        );

        // Step 4: Produce a message (should use the authenticated org context)
        let records: Vec<(Vec<u8>, Vec<u8>)> = vec![(b"sasl-key".to_vec(), b"sasl-value".to_vec())];
        let req = build_batched_produce_request(3, sasl_topic, 0, &records);
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;
        let produce_err = parse_produce_error(&resp);
        assert_eq!(produce_err, 0, "Produce with SASL auth should succeed");
        println!("[sasl] Produce OK (1 message to {sasl_topic})");

        // Step 5: Flush and fetch back
        writer_pool.flush_all().await?;

        let req = build_fetch_request(4, sasl_topic, 0, 0);
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;
        let records = parse_fetch_response(&resp);
        assert_eq!(records.len(), 1, "Should fetch 1 record back");
        let (ref key, ref value) = records[0];
        assert_eq!(key.as_deref(), Some(b"sasl-key".as_ref()));
        assert_eq!(value, b"sasl-value");
        println!("[sasl] Fetch OK (1 record verified)");
    }

    // Test unsupported SASL mechanism
    {
        let mut stream = TcpStream::connect(addr).await?;
        let req = build_api_versions_request(0);
        send_request(&mut stream, &req).await?;
        let _ = recv_response(&mut stream).await?;

        let req = build_sasl_handshake_request(1, "SCRAM-SHA-256");
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;
        let err = parse_sasl_handshake_error(&resp);
        assert_eq!(
            err, 33,
            "Should get UnsupportedSaslMechanism (33) for SCRAM"
        );
        println!("[sasl] Unsupported mechanism rejected correctly");
    }

    // Test invalid credentials
    {
        let mut stream = TcpStream::connect(addr).await?;
        let req = build_api_versions_request(0);
        send_request(&mut stream, &req).await?;
        let _ = recv_response(&mut stream).await?;

        let req = build_sasl_handshake_request(1, "PLAIN");
        send_request(&mut stream, &req).await?;
        let _ = recv_response(&mut stream).await?;

        let req = build_sasl_authenticate_request(
            2,
            "sk_test_invalid_key_000000000000000000000000000000000000",
            "sk_test_invalid_key_000000000000000000000000000000000000",
        );
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;
        let err = parse_sasl_authenticate_error(&resp);
        assert_eq!(
            err, 58,
            "Should get SaslAuthenticationFailed (58) for bad key"
        );
        println!("[sasl] Invalid credentials rejected correctly");
    }

    println!("\n=== SASL Auth: All checks passed! ===");

    Ok(())
}

// ─── Wire protocol helpers ───────────────────────────────────────────────────

async fn send_request(stream: &mut TcpStream, payload: &[u8]) -> anyhow::Result<()> {
    let mut frame = BytesMut::new();
    frame.put_i32(payload.len() as i32);
    frame.extend_from_slice(payload);
    stream.write_all(&frame).await?;
    Ok(())
}

async fn recv_response(stream: &mut TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = i32::from_be_bytes(len_buf) as usize;
    let mut response = vec![0u8; len];
    stream.read_exact(&mut response).await?;
    Ok(response)
}

fn parse_produce_error(resp: &[u8]) -> i16 {
    let mut buf = BytesMut::from(resp);
    let _correlation_id = buf.get_i32();
    let _topic_count = buf.get_i32();
    let name_len = buf.get_i16() as usize;
    buf.advance(name_len);
    let _part_count = buf.get_i32();
    let _partition = buf.get_i32();
    buf.get_i16()
}

fn parse_fetch_response(resp: &[u8]) -> Vec<(Option<Vec<u8>>, Vec<u8>)> {
    let mut buf = BytesMut::from(resp);
    if buf.remaining() < 8 {
        return vec![];
    }

    let _correlation_id = buf.get_i32();
    let topic_count = buf.get_i32();
    if topic_count == 0 {
        return vec![];
    }

    let name_len = buf.get_i16() as usize;
    buf.advance(name_len);
    let part_count = buf.get_i32();
    if part_count == 0 {
        return vec![];
    }

    let _partition = buf.get_i32();
    let _error_code = buf.get_i16();
    let _high_watermark = buf.get_i64();

    let records_len = buf.get_i32();
    if records_len <= 0 {
        return vec![];
    }

    let record_data = buf.split_to(records_len as usize);
    parse_record_batch(&record_data)
}

fn parse_record_batch(data: &[u8]) -> Vec<(Option<Vec<u8>>, Vec<u8>)> {
    let mut buf = BytesMut::from(data);
    let mut all_records = Vec::new();

    while buf.remaining() >= 61 {
        let _base_offset = buf.get_i64();
        let batch_length = buf.get_i32() as usize;

        if buf.remaining() < batch_length {
            break;
        }

        let mut batch_buf = buf.split_to(batch_length);

        let _partition_leader_epoch = batch_buf.get_i32();
        let magic = batch_buf.get_i8();
        if magic != 2 {
            break;
        }

        let _crc = batch_buf.get_u32();
        let _attributes = batch_buf.get_i16();
        let _last_offset_delta = batch_buf.get_i32();
        let _first_timestamp = batch_buf.get_i64();
        let _max_timestamp = batch_buf.get_i64();
        let _producer_id = batch_buf.get_i64();
        let _producer_epoch = batch_buf.get_i16();
        let _base_sequence = batch_buf.get_i32();
        let record_count = batch_buf.get_i32();

        for _ in 0..record_count {
            let _length = read_varint(&mut batch_buf);
            let _attrs = batch_buf.get_i8();
            let _ts_delta = read_varint(&mut batch_buf);
            let _off_delta = read_varint(&mut batch_buf);

            let key_len = read_varint(&mut batch_buf);
            let key = if key_len >= 0 {
                Some(batch_buf.split_to(key_len as usize).to_vec())
            } else {
                None
            };

            let val_len = read_varint(&mut batch_buf);
            let value = if val_len >= 0 {
                batch_buf.split_to(val_len as usize).to_vec()
            } else {
                vec![]
            };

            let header_count = read_varint(&mut batch_buf);
            for _ in 0..header_count {
                let hk_len = read_varint(&mut batch_buf) as usize;
                batch_buf.advance(hk_len);
                let hv_len = read_varint(&mut batch_buf);
                if hv_len >= 0 {
                    batch_buf.advance(hv_len as usize);
                }
            }

            all_records.push((key, value));
        }
    }

    all_records
}

fn build_api_versions_request(correlation_id: i32) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(18);
    buf.put_i16(0);
    buf.put_i32(correlation_id);
    buf.put_i16(6);
    buf.extend_from_slice(b"smoke1");
    buf.to_vec()
}

/// Build a Produce request with multiple records in one LZ4-compressed RecordBatch
fn build_batched_produce_request(
    correlation_id: i32,
    topic: &str,
    partition: i32,
    records: &[(Vec<u8>, Vec<u8>)],
) -> Vec<u8> {
    let mut buf = BytesMut::new();

    buf.put_i16(0); // api_key = Produce
    buf.put_i16(0); // api_version = 0
    buf.put_i32(correlation_id);
    buf.put_i16(6);
    buf.extend_from_slice(b"smoke1");

    buf.put_i16(1); // acks
    buf.put_i32(30_000); // timeout

    buf.put_i32(1); // 1 topic
    buf.put_i16(topic.len() as i16);
    buf.extend_from_slice(topic.as_bytes());

    buf.put_i32(1); // 1 partition
    buf.put_i32(partition);

    // Optimization #4: LZ4-compressed record batch
    let record_batch = build_lz4_record_batch(records);
    buf.put_i32(record_batch.len() as i32);
    buf.extend_from_slice(&record_batch);

    buf.to_vec()
}

/// Build a Kafka v2 RecordBatch with LZ4 compression
fn build_lz4_record_batch(records: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    // Encode all individual records (uncompressed first)
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

    // Optimization #4: LZ4 compress the records
    let compressed = lz4_flex::compress_prepend_size(&raw_records);

    // Build batch header
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
    // attributes: bits 0-2 = compression type (3 = LZ4)
    batch.put_i16(3); // LZ4 compression
    batch.put_i32((records.len() - 1) as i32); // last_offset_delta
    batch.put_i64(timestamp);
    batch.put_i64(timestamp);
    batch.put_i64(-1); // producer_id
    batch.put_i16(-1); // producer_epoch
    batch.put_i32(-1); // base_sequence
    batch.put_i32(records.len() as i32); // record_count

    batch.extend_from_slice(&compressed);

    // Fill in batch length
    let batch_length = (batch.len() - 12) as i32;
    batch[length_pos..length_pos + 4].copy_from_slice(&batch_length.to_be_bytes());

    // Fill in CRC32C
    let crc = crc32c::crc32c(&batch[crc_start..]);
    batch[crc_pos..crc_pos + 4].copy_from_slice(&crc.to_be_bytes());

    batch.to_vec()
}

fn build_fetch_request(correlation_id: i32, topic: &str, partition: i32, offset: i64) -> Vec<u8> {
    let mut buf = BytesMut::new();

    buf.put_i16(1); // api_key = Fetch
    buf.put_i16(0); // api_version = 0
    buf.put_i32(correlation_id);
    buf.put_i16(6);
    buf.extend_from_slice(b"smoke1");

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

fn read_varint(buf: &mut BytesMut) -> i64 {
    let mut result: i64 = 0;
    let mut shift = 0;
    loop {
        let byte = buf.get_u8();
        result |= ((byte & 0x7F) as i64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    (result >> 1) ^ -(result & 1)
}

// ─── SASL protocol helpers ──────────────────────────────────────────────────

/// Build a SaslHandshake request (API Key 17, version 0)
fn build_sasl_handshake_request(correlation_id: i32, mechanism: &str) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(17); // api_key = SaslHandshake
    buf.put_i16(0); // api_version = 0
    buf.put_i32(correlation_id);
    buf.put_i16(6);
    buf.extend_from_slice(b"smoke1"); // client_id

    // mechanism string
    buf.put_i16(mechanism.len() as i16);
    buf.extend_from_slice(mechanism.as_bytes());

    buf.to_vec()
}

/// Build a SaslAuthenticate request (API Key 36, version 0)
fn build_sasl_authenticate_request(correlation_id: i32, username: &str, password: &str) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(36); // api_key = SaslAuthenticate
    buf.put_i16(0); // api_version = 0
    buf.put_i32(correlation_id);
    buf.put_i16(6);
    buf.extend_from_slice(b"smoke1"); // client_id

    // SASL/PLAIN auth_bytes: \0username\0password
    let mut auth_bytes = Vec::new();
    auth_bytes.push(0); // empty authzid
    auth_bytes.extend_from_slice(username.as_bytes());
    auth_bytes.push(0);
    auth_bytes.extend_from_slice(password.as_bytes());

    buf.put_i32(auth_bytes.len() as i32);
    buf.extend_from_slice(&auth_bytes);

    buf.to_vec()
}

/// Parse error_code from SaslHandshake response
fn parse_sasl_handshake_error(resp: &[u8]) -> i16 {
    let mut buf = BytesMut::from(resp);
    let _correlation_id = buf.get_i32();
    buf.get_i16() // error_code
}

/// Parse error_code from SaslAuthenticate response
fn parse_sasl_authenticate_error(resp: &[u8]) -> i16 {
    let mut buf = BytesMut::from(resp);
    let _correlation_id = buf.get_i32();
    buf.get_i16() // error_code
}
