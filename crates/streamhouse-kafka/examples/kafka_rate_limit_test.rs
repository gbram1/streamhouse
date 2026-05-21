//! Kafka Rate Limit Test
//!
//! Demonstrates rate limiting on the Kafka protocol by:
//!   1. Creating an org with a very low request rate limit (10 req/s)
//!   2. Producing many batches rapidly
//!   3. Observing throttle_time_ms > 0 in produce responses
//!
//! Kafka protocol uses throttle_time_ms for backpressure (not rejection).
//! Messages still go through, but the client is told to slow down.
//!
//! Run with:
//!   cargo run --release -p streamhouse-kafka --example kafka_rate_limit_test

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Buf, BufMut, BytesMut};
use object_store::memory::InMemory;
use streamhouse_metadata::{
    MetadataStore, OrganizationPlan, OrganizationQuota, QuotaEnforcer, SqliteMetadataStore,
    TopicConfig,
};
use streamhouse_storage::{SegmentCache, WriteConfig, WriterPool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use streamhouse_kafka::server::{KafkaServerConfig, KafkaServerState};

/// Very low rate limits to trigger throttling quickly
const REQUEST_RATE_LIMIT: i32 = 10; // 10 req/s (burst = 20)
const PRODUCE_BYTE_RATE: i64 = 1024; // 1 KB/s (burst = 2 KB)
const NUM_BATCHES: usize = 50; // Send 50 produce requests rapidly

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            "info,streamhouse_kafka=debug,streamhouse_metadata=debug,streamhouse_storage=warn",
        )
        .init();

    println!("=== Kafka Rate Limit Test ===");
    println!(
        "    Request limit: {} req/s (burst: {})",
        REQUEST_RATE_LIMIT,
        REQUEST_RATE_LIMIT * 2
    );
    println!(
        "    Produce limit: {} bytes/s (burst: {})",
        PRODUCE_BYTE_RATE,
        PRODUCE_BYTE_RATE * 2
    );
    println!(
        "    Sending:       {} produce requests as fast as possible\n",
        NUM_BATCHES
    );

    // ── Setup ────────────────────────────────────────────────────────────
    let metadata: Arc<dyn MetadataStore> = Arc::new(SqliteMetadataStore::new_in_memory().await?);
    let object_store: Arc<dyn object_store::ObjectStore> = Arc::new(InMemory::new());

    let temp_dir = tempfile::tempdir()?;
    let cache = Arc::new(SegmentCache::new(
        temp_dir.path().join("cache"),
        64 * 1024 * 1024,
    )?);

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
            batch_enabled: true,
            batch_max_records: 1000,
            batch_max_bytes: 4 * 1024 * 1024,
            batch_max_age_ms: 10,
            agent_id: None,
        }),
        ..Default::default()
    };

    let writer_pool = Arc::new(WriterPool::new(
        metadata.clone(),
        object_store.clone(),
        config,
    ));

    let _flush_handle = Arc::clone(&writer_pool).start_background_flush(Duration::from_secs(2));

    // Create topic (unauthenticated connections use default org)
    let topic_name = "rate-limit-test";
    metadata
        .create_topic(TopicConfig {
            name: topic_name.to_string(),
            partition_count: 1,
            retention_ms: Some(86_400_000),
            cleanup_policy: Default::default(),
            config: Default::default(),
        })
        .await?;
    println!("[setup] Created topic: {topic_name}");

    // Create QuotaEnforcer and pre-seed with low limits
    let enforcer: Arc<QuotaEnforcer<dyn MetadataStore>> =
        Arc::new(QuotaEnforcer::new(metadata.clone()));

    // Pre-seed the enforcer by making a check with low-limit quotas.
    // This creates the rate limiter for the default org with our custom limits.
    let low_quota_ctx = streamhouse_metadata::tenant::TenantContext {
        organization: streamhouse_metadata::Organization {
            id: streamhouse_metadata::DEFAULT_ORGANIZATION_ID.to_string(),
            name: "default".to_string(),
            slug: "default".to_string(),
            plan: OrganizationPlan::Free,
            status: streamhouse_metadata::OrganizationStatus::Active,
            created_at: 0,
            settings: std::collections::HashMap::new(),
            external_id: None,
            deployment_mode: Default::default(),
        },
        api_key: None,
        quota: OrganizationQuota {
            organization_id: streamhouse_metadata::DEFAULT_ORGANIZATION_ID.to_string(),
            max_topics: 10,
            max_partitions_per_topic: 12,
            max_total_partitions: 100,
            max_storage_bytes: 10_737_418_240,
            max_retention_days: 7,
            max_produce_bytes_per_sec: PRODUCE_BYTE_RATE,
            max_consume_bytes_per_sec: 52_428_800,
            max_requests_per_sec: REQUEST_RATE_LIMIT,
            max_consumer_groups: 50,
            max_schemas: 100,
            max_schema_versions_per_subject: 100,
            max_connections: 100,
        },
        is_default: false,
    };

    // Seed the enforcer — this creates the rate limiter with our low limits
    let _ = enforcer.check_request(&low_quota_ctx, None).await;
    println!(
        "[setup] QuotaEnforcer seeded with low limits ({}req/s, {}B/s)",
        REQUEST_RATE_LIMIT, PRODUCE_BYTE_RATE
    );

    // Build KafkaServerState manually (to inject the quota_enforcer)
    let kafka_config = KafkaServerConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        node_id: 0,
        advertised_host: "127.0.0.1".to_string(),
        advertised_port: 0,
    };

    let group_coordinator = Arc::new(streamhouse_kafka::coordinator::GroupCoordinator::new(
        metadata.clone(),
    ));

    let state = Arc::new(KafkaServerState {
        config: kafka_config.clone(),
        metadata: metadata.clone(),
        writer_pool: writer_pool.clone(),
        segment_cache: cache,
        object_store,
        group_coordinator,
        tenant_resolver: None, // No SASL for this test
        quota_enforcer: Some(enforcer.clone()),
        active_connections: Arc::new(dashmap::DashMap::new()),
    });

    // Bind and run the server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    println!("[setup] Kafka server on {addr}\n");

    let server_state = state.clone();
    tokio::spawn(async move {
        loop {
            if let Ok((stream, peer_addr)) = listener.accept().await {
                let state = server_state.clone();
                tokio::spawn(async move {
                    if let Err(e) = streamhouse_kafka::server::handle_connection_public(
                        stream, peer_addr, state,
                    )
                    .await
                    {
                        // ConnectionClosed is expected
                        let _ = e;
                    }
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // ── Produce rapidly and observe throttle_time_ms ──────────────────────
    let mut stream = TcpStream::connect(addr).await?;

    // Handshake
    let req = build_api_versions_request(0);
    send_request(&mut stream, &req).await?;
    let _ = recv_response(&mut stream).await?;

    println!(
        "[produce] Sending {} produce requests as fast as possible...\n",
        NUM_BATCHES
    );
    println!(
        "  {:>5}  {:>12}  {:>15}  {:>10}",
        "Batch", "Error Code", "Throttle (ms)", "Status"
    );
    println!(
        "  {:>5}  {:>12}  {:>15}  {:>10}",
        "-----", "----------", "-------------", "------"
    );

    let mut throttled_count = 0;
    let mut total_throttle_ms = 0i32;
    let produce_start = Instant::now();

    for batch_idx in 0..NUM_BATCHES {
        let records: Vec<(Vec<u8>, Vec<u8>)> = (0..10)
            .map(|i| {
                let key = format!("key-{batch_idx}-{i}").into_bytes();
                let value = format!(
                    r#"{{"batch":{},"seq":{},"data":"rate-limit-test-payload-{:06}"}}"#,
                    batch_idx,
                    i,
                    batch_idx * 10 + i
                )
                .into_bytes();
                (key, value)
            })
            .collect();

        // Use v1 produce (which includes throttle_time_ms in response)
        let req = build_produce_v1_request(batch_idx as i32 + 1, topic_name, 0, &records);
        send_request(&mut stream, &req).await?;
        let resp = recv_response(&mut stream).await?;

        let (error_code, throttle_ms) = parse_produce_v1_response(&resp);

        let status = if throttle_ms > 0 {
            throttled_count += 1;
            total_throttle_ms += throttle_ms;
            "THROTTLED"
        } else if error_code != 0 {
            "ERROR"
        } else {
            "ok"
        };

        // Print every batch (since there are only 50)
        println!(
            "  {:>5}  {:>12}  {:>15}  {:>10}",
            batch_idx + 1,
            error_code,
            throttle_ms,
            status
        );
    }

    let produce_elapsed = produce_start.elapsed();

    // ── Summary ───────────────────────────────────────────────────────────
    println!("\n=== Results ===");
    println!("  Total batches:     {}", NUM_BATCHES);
    println!("  Total records:     {}", NUM_BATCHES * 10);
    println!("  Elapsed:           {:.3}s", produce_elapsed.as_secs_f64());
    println!(
        "  Request rate:      {:.0} req/s",
        NUM_BATCHES as f64 / produce_elapsed.as_secs_f64()
    );
    println!(
        "  Throttled:         {}/{} batches",
        throttled_count, NUM_BATCHES
    );
    println!("  Total throttle:    {} ms", total_throttle_ms);
    println!(
        "  Rate limit:        {} req/s (burst {})",
        REQUEST_RATE_LIMIT,
        REQUEST_RATE_LIMIT * 2
    );

    if throttled_count > 0 {
        println!(
            "\n  Rate limiting is working! Kafka clients would back off for {}ms total.",
            total_throttle_ms
        );
    } else {
        println!("\n  No throttling observed (requests may not have exceeded burst capacity).");
        println!(
            "  Burst allows {} requests before throttling kicks in.",
            REQUEST_RATE_LIMIT * 2
        );
    }

    println!("\n=== Done ===");

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

fn build_api_versions_request(correlation_id: i32) -> Vec<u8> {
    let mut buf = BytesMut::new();
    buf.put_i16(18); // api_key = ApiVersions
    buf.put_i16(0); // version
    buf.put_i32(correlation_id);
    buf.put_i16(8);
    buf.extend_from_slice(b"ratelim1");
    buf.to_vec()
}

/// Build a Produce request at version 1 (which includes throttle_time_ms in response)
fn build_produce_v1_request(
    correlation_id: i32,
    topic: &str,
    partition: i32,
    records: &[(Vec<u8>, Vec<u8>)],
) -> Vec<u8> {
    let mut buf = BytesMut::new();

    buf.put_i16(0); // api_key = Produce
    buf.put_i16(1); // api_version = 1 (includes throttle_time_ms in response)
    buf.put_i32(correlation_id);
    buf.put_i16(8);
    buf.extend_from_slice(b"ratelim1");

    buf.put_i16(1); // acks
    buf.put_i32(30_000); // timeout

    buf.put_i32(1); // 1 topic
    buf.put_i16(topic.len() as i16);
    buf.extend_from_slice(topic.as_bytes());

    buf.put_i32(1); // 1 partition
    buf.put_i32(partition);

    let record_batch = build_record_batch(records);
    buf.put_i32(record_batch.len() as i32);
    buf.extend_from_slice(&record_batch);

    buf.to_vec()
}

/// Build a v2 RecordBatch (no compression for simplicity)
fn build_record_batch(records: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
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
    batch.put_i16(0); // attributes (no compression)
    batch.put_i32((records.len() - 1) as i32); // last_offset_delta
    batch.put_i64(timestamp);
    batch.put_i64(timestamp);
    batch.put_i64(-1); // producer_id
    batch.put_i16(-1); // producer_epoch
    batch.put_i32(-1); // base_sequence
    batch.put_i32(records.len() as i32);

    batch.extend_from_slice(&raw_records);

    let batch_length = (batch.len() - 12) as i32;
    batch[length_pos..length_pos + 4].copy_from_slice(&batch_length.to_be_bytes());

    let crc = crc32c::crc32c(&batch[crc_start..]);
    batch[crc_pos..crc_pos + 4].copy_from_slice(&crc.to_be_bytes());

    batch.to_vec()
}

/// Parse a v1 produce response: returns (error_code, throttle_time_ms)
fn parse_produce_v1_response(resp: &[u8]) -> (i16, i32) {
    let mut buf = BytesMut::from(resp);
    let _correlation_id = buf.get_i32();

    // topics array
    let _topic_count = buf.get_i32();
    let name_len = buf.get_i16() as usize;
    buf.advance(name_len);

    // partitions array
    let _part_count = buf.get_i32();
    let _partition = buf.get_i32();
    let error_code = buf.get_i16();
    let _base_offset = buf.get_i64();

    // v2+ has log_append_time
    // v1 does NOT have log_append_time — throttle_time_ms comes right after base_offset
    // Actually for v1: [topics][partitions: partition_index, error_code, base_offset][throttle_time_ms]
    // But there might be remaining partition fields... let's just read throttle_time from the end

    // For v1 response format:
    // correlation_id (4) + topic_count (4) + topic_name (2+len) + partition_count (4)
    // + partition_index (4) + error_code (2) + base_offset (8) + throttle_time_ms (4)
    let throttle_time_ms = if buf.remaining() >= 4 {
        buf.get_i32()
    } else {
        0
    };

    (error_code, throttle_time_ms)
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
