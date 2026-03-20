//! Full write path E2E tests -- tests the complete production flow through each protocol.
//!
//! Each test follows: create topic -> produce -> flush -> consume -> verify integrity
//! Tests run against a real server with real storage (not mocked).

use std::collections::HashSet;

use serde_json::json;
use streamhouse_e2e::{BatchRecord, TestCluster};

/// Helper: generate a batch of JSON records with checksums for integrity verification.
fn generate_records(count: usize, prefix: &str) -> Vec<serde_json::Value> {
    (0..count)
        .map(|i| {
            let payload = format!("{}-payload-{}", prefix, i);
            let checksum = format!("{:x}", md5_simple(payload.as_bytes()));
            json!({
                "index": i,
                "key": format!("{}-key-{}", prefix, i),
                "payload": payload,
                "checksum": checksum,
            })
        })
        .collect()
}

/// Minimal MD5-like hash for checksums (uses a simple FNV-1a for test purposes).
fn md5_simple(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Helper: verify checksum integrity of a consumed record.
fn verify_record_checksum(value: &serde_json::Value) -> bool {
    let payload = value["payload"].as_str().unwrap_or("");
    let expected_checksum = format!("{:x}", md5_simple(payload.as_bytes()));
    value["checksum"].as_str().unwrap_or("") == expected_checksum
}

/// Wait for data to flush to storage so it becomes readable via consume/SQL.
async fn wait_for_flush() {
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
}

// ---------------------------------------------------------------------------
// Test: REST full write-read path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rest_full_write_read_path() {
    let cluster = TestCluster::start().await.unwrap();
    let client = cluster.rest_client();

    // Step 1: Create organization
    let org = client
        .create_org("e2e-rest-org", "e2e-rest-org")
        .await
        .expect("Failed to create organization");
    let org_id = org["id"].as_str().expect("org should have an id");

    // Step 2: Create API key for the org
    let _api_key = client
        .create_api_key(org_id, "e2e-test-key")
        .await
        .expect("Failed to create API key");

    // Step 3: Create topic with 4 partitions
    let topic_name = "rest-e2e";
    let topic = client
        .create_topic(topic_name, 4)
        .await
        .expect("Failed to create topic");
    assert_eq!(topic.name, topic_name, "Topic name should match");
    assert_eq!(topic.partitions, 4, "Topic should have 4 partitions");

    // Step 4: Register JSON schema for the topic
    let schema_def = json!({
        "type": "object",
        "properties": {
            "index": { "type": "integer" },
            "key": { "type": "string" },
            "payload": { "type": "string" },
            "checksum": { "type": "string" }
        },
        "required": ["index", "payload", "checksum"]
    });
    let schema_result = client
        .register_schema(
            &format!("{}-value", topic_name),
            &schema_def.to_string(),
            "JSON",
        )
        .await
        .expect("Failed to register schema");
    assert!(
        schema_result.id > 0,
        "Schema registration should return an id"
    );

    // Step 5: Produce 200 records via REST batch
    let records = generate_records(200, "rest");
    let batch_records: Vec<BatchRecord> = records
        .iter()
        .enumerate()
        .map(|(i, r)| BatchRecord {
            key: Some(format!("rest-key-{}", i)),
            value: r.to_string(),
            partition: None,
        })
        .collect();

    let produce_result = client
        .produce_batch(topic_name, batch_records)
        .await
        .expect("Failed to produce batch");
    assert_eq!(
        produce_result.count, 200,
        "Batch produce should acknowledge 200 records"
    );

    // Step 6: Wait for flush
    wait_for_flush().await;

    // Step 7: Consume all records back via REST (across all 4 partitions)
    let mut all_consumed: Vec<streamhouse_e2e::ConsumedRecord> = Vec::new();
    for partition in 0..4u32 {
        let mut offset = 0u64;
        loop {
            let response = client
                .consume(topic_name, partition, offset, 100)
                .await
                .expect("Failed to consume");

            if response.records.is_empty() {
                break;
            }

            let next_offset = response.next_offset;
            all_consumed.extend(response.records);

            if next_offset <= offset {
                break;
            }
            offset = next_offset;
        }
    }

    // Step 8: Verify count
    assert_eq!(
        all_consumed.len(),
        200,
        "Should consume exactly 200 records, got {}",
        all_consumed.len()
    );

    // Step 9: Verify JSON parsing and checksums
    let mut seen_indices: HashSet<u64> = HashSet::new();
    for record in &all_consumed {
        let parsed: serde_json::Value =
            serde_json::from_str(&record.value).expect("value should be valid JSON");
        assert!(
            verify_record_checksum(&parsed),
            "Checksum mismatch for record with index {}",
            parsed["index"]
        );
        let idx = parsed["index"].as_u64().expect("index should be a number");
        assert!(
            seen_indices.insert(idx),
            "Duplicate record detected at index {}",
            idx
        );
    }

    // Step 10: SQL COUNT(*) should match
    let sql_result = client
        .sql_query(&format!("SELECT COUNT(*) FROM \"{}\"", topic_name))
        .await
        .expect("SQL query failed");
    let row_count = sql_result.rows[0][0]
        .as_u64()
        .or_else(|| {
            sql_result.rows[0][0]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
        })
        .expect("SQL COUNT should return a number");
    assert_eq!(row_count, 200, "SQL COUNT(*) should return 200");

    // Step 11: Check metrics show the writes
    let metrics = client.get_metrics().await.expect("Failed to get metrics");
    assert!(
        metrics["topics_count"].as_u64().unwrap_or(0) >= 1,
        "Metrics should show at least 1 topic"
    );
    assert!(
        metrics["total_messages"].as_u64().unwrap_or(0) >= 200,
        "Metrics should show at least 200 total messages"
    );

    // Step 12: Cleanup
    client
        .delete_topic(topic_name)
        .await
        .expect("Failed to delete topic");

    // Verify deletion
    let topics = client.list_topics().await.expect("Failed to list topics");
    let topic_names: Vec<&str> = topics.iter().map(|t| t.name.as_str()).collect();
    assert!(
        !topic_names.contains(&topic_name),
        "Topic should be deleted"
    );
}

// ---------------------------------------------------------------------------
// Test: gRPC full write-read path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_grpc_full_write_read_path() {
    let cluster = TestCluster::start().await.unwrap();
    let mut grpc = cluster.grpc_client().await.unwrap();

    // Step 1: Create topic via gRPC
    let topic_name = "grpc-e2e";
    let create_resp = grpc
        .create_topic(streamhouse_proto::streamhouse::CreateTopicRequest {
            name: topic_name.to_string(),
            partition_count: 2,
            retention_ms: None,
            config: Default::default(),
        })
        .await
        .expect("gRPC CreateTopic failed");
    let create_resp = create_resp.into_inner();
    assert_eq!(
        create_resp.partition_count, 2,
        "gRPC topic should have 2 partitions"
    );

    // Step 2: Produce 200 records via gRPC ProduceBatch
    let records: Vec<streamhouse_proto::streamhouse::Record> = (0..200)
        .map(|i| {
            let value = json!({
                "index": i,
                "payload": format!("grpc-payload-{}", i),
            });
            streamhouse_proto::streamhouse::Record {
                key: format!("grpc-key-{}", i).into_bytes(),
                value: value.to_string().into_bytes(),
                headers: Default::default(),
            }
        })
        .collect();

    let batch_resp = grpc
        .produce_batch(streamhouse_proto::streamhouse::ProduceBatchRequest {
            topic: topic_name.to_string(),
            partition: 0,
            records,
            producer_id: None,
            producer_epoch: None,
            base_sequence: None,
            transaction_id: None,
            ack_mode: 0,
        })
        .await
        .expect("gRPC ProduceBatch failed");
    let batch_resp = batch_resp.into_inner();
    assert_eq!(
        batch_resp.count, 200,
        "gRPC batch should acknowledge 200 records"
    );

    // Step 3: Wait for flush
    wait_for_flush().await;

    // Step 4: Consume via gRPC
    let mut all_records = Vec::new();
    let mut offset = 0u64;
    loop {
        let consume_resp = grpc
            .consume(streamhouse_proto::streamhouse::ConsumeRequest {
                topic: topic_name.to_string(),
                partition: 0,
                offset,
                max_records: 100,
                consumer_group: None,
            })
            .await
            .expect("gRPC Consume failed");
        let consume_resp = consume_resp.into_inner();

        if consume_resp.records.is_empty() {
            break;
        }

        offset = consume_resp
            .records
            .last()
            .map(|r| r.offset + 1)
            .unwrap_or(offset);
        all_records.extend(consume_resp.records);

        if !consume_resp.has_more {
            break;
        }
    }

    // Step 5: Verify count and data integrity
    assert_eq!(
        all_records.len(),
        200,
        "Should consume 200 records via gRPC, got {}",
        all_records.len()
    );

    for record in &all_records {
        let value: serde_json::Value =
            serde_json::from_slice(&record.value).expect("gRPC record value should be valid JSON");
        assert!(
            value["index"].as_u64().is_some(),
            "Each record should have an index field"
        );
    }

    // Step 6: Cleanup via gRPC
    let delete_resp = grpc
        .delete_topic(streamhouse_proto::streamhouse::DeleteTopicRequest {
            name: topic_name.to_string(),
        })
        .await
        .expect("gRPC DeleteTopic failed");
    assert!(
        delete_resp.into_inner().success,
        "gRPC delete should succeed"
    );
}

// ---------------------------------------------------------------------------
// Test: Cross-protocol -- REST produce, gRPC consume
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cross_protocol_rest_produce_grpc_consume() {
    let cluster = TestCluster::start().await.unwrap();
    let client = cluster.rest_client();
    let mut grpc = cluster.grpc_client().await.unwrap();

    let topic_name = "cross-rest-grpc";

    // Create topic via REST
    client
        .create_topic(topic_name, 1)
        .await
        .expect("Failed to create topic");

    // Produce 100 records via REST
    let batch_records: Vec<BatchRecord> = (0..100)
        .map(|i| BatchRecord {
            key: Some(format!("cross-key-{}", i)),
            value: json!({"index": i, "source": "rest"}).to_string(),
            partition: Some(0),
        })
        .collect();

    client
        .produce_batch(topic_name, batch_records)
        .await
        .expect("Failed to produce via REST");

    wait_for_flush().await;

    // Consume via gRPC
    let mut all_records = Vec::new();
    let mut offset = 0u64;
    loop {
        let resp = grpc
            .consume(streamhouse_proto::streamhouse::ConsumeRequest {
                topic: topic_name.to_string(),
                partition: 0,
                offset,
                max_records: 100,
                consumer_group: None,
            })
            .await
            .expect("gRPC consume failed");
        let resp = resp.into_inner();

        if resp.records.is_empty() {
            break;
        }
        offset = resp.records.last().map(|r| r.offset + 1).unwrap_or(offset);
        all_records.extend(resp.records);

        if !resp.has_more {
            break;
        }
    }

    assert_eq!(
        all_records.len(),
        100,
        "gRPC should consume all 100 records produced via REST, got {}",
        all_records.len()
    );

    // Verify content
    for record in &all_records {
        let value: serde_json::Value =
            serde_json::from_slice(&record.value).expect("Record value should be valid JSON");
        assert_eq!(
            value["source"].as_str().unwrap(),
            "rest",
            "Records should originate from REST"
        );
    }

    // Cleanup
    client
        .delete_topic(topic_name)
        .await
        .expect("Failed to delete topic");
}

// ---------------------------------------------------------------------------
// Test: Cross-protocol -- gRPC produce, REST consume
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cross_protocol_grpc_produce_rest_consume() {
    let cluster = TestCluster::start().await.unwrap();
    let client = cluster.rest_client();
    let mut grpc = cluster.grpc_client().await.unwrap();

    let topic_name = "cross-grpc-rest";

    // Create topic via REST
    client
        .create_topic(topic_name, 1)
        .await
        .expect("Failed to create topic");

    // Produce 100 records via gRPC
    let records: Vec<streamhouse_proto::streamhouse::Record> = (0..100)
        .map(|i| {
            let value = json!({"index": i, "source": "grpc"});
            streamhouse_proto::streamhouse::Record {
                key: format!("grpc-key-{}", i).into_bytes(),
                value: value.to_string().into_bytes(),
                headers: Default::default(),
            }
        })
        .collect();

    grpc.produce_batch(streamhouse_proto::streamhouse::ProduceBatchRequest {
        topic: topic_name.to_string(),
        partition: 0,
        records,
        producer_id: None,
        producer_epoch: None,
        base_sequence: None,
        transaction_id: None,
        ack_mode: 0,
    })
    .await
    .expect("gRPC produce failed");

    wait_for_flush().await;

    // Consume via REST
    let mut all_consumed: Vec<streamhouse_e2e::ConsumedRecord> = Vec::new();
    let mut offset = 0u64;
    loop {
        let response = client
            .consume(topic_name, 0, offset, 100)
            .await
            .expect("REST consume failed");

        if response.records.is_empty() {
            break;
        }

        let next_offset = response.next_offset;
        all_consumed.extend(response.records);

        if next_offset <= offset {
            break;
        }
        offset = next_offset;
    }

    assert_eq!(
        all_consumed.len(),
        100,
        "REST should consume all 100 records produced via gRPC, got {}",
        all_consumed.len()
    );

    // Verify content
    for record in &all_consumed {
        let parsed: serde_json::Value = serde_json::from_str(&record.value).unwrap();
        assert_eq!(
            parsed["source"].as_str().unwrap(),
            "grpc",
            "Records should originate from gRPC"
        );
    }

    // Cleanup
    client
        .delete_topic(topic_name)
        .await
        .expect("Failed to delete topic");
}

// ---------------------------------------------------------------------------
// Test: Schema-enforced produce
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_schema_enforced_produce() {
    let cluster = TestCluster::start().await.unwrap();
    let client = cluster.rest_client();

    let topic_name = "schema-enforced";

    // Create topic
    client
        .create_topic(topic_name, 1)
        .await
        .expect("Failed to create topic");

    // Register JSON schema with required fields
    let schema_def = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name", "age"]
    });

    client
        .register_schema(
            &format!("{}-value", topic_name),
            &schema_def.to_string(),
            "JSON",
        )
        .await
        .expect("Failed to register schema");

    // Produce valid message -- should succeed
    let valid_msg = json!({"name": "Alice", "age": 30}).to_string();
    let produce_ok = client.produce(topic_name, Some("key-1"), &valid_msg).await;
    assert!(
        produce_ok.is_ok(),
        "Producing a valid message should succeed"
    );

    // Produce invalid message (missing required field "age") -- should fail
    let invalid_msg = json!({"name": "Bob"}).to_string();
    let produce_err = client
        .produce(topic_name, Some("key-2"), &invalid_msg)
        .await;
    assert!(
        produce_err.is_err(),
        "Producing an invalid message (missing required field) should fail"
    );

    wait_for_flush().await;

    // Consume -- only the valid message should be present
    let response = client
        .consume(topic_name, 0, 0, 100)
        .await
        .expect("Consume failed");

    assert_eq!(
        response.records.len(),
        1,
        "Only the valid message should be present, got {} records",
        response.records.len()
    );

    let parsed: serde_json::Value = serde_json::from_str(&response.records[0].value).unwrap();
    assert_eq!(
        parsed["name"].as_str().unwrap(),
        "Alice",
        "The consumed record should be the valid message"
    );

    // Cleanup
    client
        .delete_topic(topic_name)
        .await
        .expect("Failed to delete topic");
}

// ---------------------------------------------------------------------------
// Test: Consumer group offsets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_consumer_group_offsets() {
    let cluster = TestCluster::start().await.unwrap();
    let client = cluster.rest_client();

    let topic_name = "consumer-group-test";
    let group_id = "test-group";

    // Create topic and produce 100 records
    client
        .create_topic(topic_name, 1)
        .await
        .expect("Failed to create topic");

    let batch_records: Vec<BatchRecord> = (0..100)
        .map(|i| BatchRecord {
            key: Some(format!("cg-key-{}", i)),
            value: json!({"index": i}).to_string(),
            partition: Some(0),
        })
        .collect();

    client
        .produce_batch(topic_name, batch_records)
        .await
        .expect("Failed to produce batch");

    wait_for_flush().await;

    // Commit offset at 50 for group "test-group"
    let commit_result = client
        .commit_offset(group_id, topic_name, 0, 50)
        .await
        .expect("Failed to commit offset");
    assert!(
        commit_result["success"].as_bool().unwrap_or(false),
        "Offset commit should succeed"
    );

    // Get committed offset via raw request
    let (_status, group_detail) = client
        .get_raw(&format!("/api/v1/consumer-groups/{}", group_id))
        .await
        .expect("Failed to get consumer group");

    let offsets = group_detail["offsets"].as_array().unwrap();
    let partition_0_offset = offsets
        .iter()
        .find(|o| o["partitionId"].as_u64().unwrap_or(u64::MAX) == 0)
        .expect("Should have offset info for partition 0");
    assert_eq!(
        partition_0_offset["committedOffset"].as_u64().unwrap(),
        50,
        "Committed offset should be 50"
    );

    // Consume from offset 50 -- should get remaining records
    let response = client
        .consume(topic_name, 0, 50, 100)
        .await
        .expect("Consume from offset 50 failed");

    assert_eq!(
        response.records.len(),
        50,
        "Consuming from offset 50 should yield 50 records, got {}",
        response.records.len()
    );

    // Verify the first consumed record starts at the right index
    let first_value: serde_json::Value = serde_json::from_str(&response.records[0].value).unwrap();
    assert_eq!(
        first_value["index"].as_u64().unwrap(),
        50,
        "First record after offset 50 should have index 50"
    );

    // Cleanup
    client
        .delete_topic(topic_name)
        .await
        .expect("Failed to delete topic");
}

// ---------------------------------------------------------------------------
// Test: SQL queries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sql_queries() {
    let cluster = TestCluster::start().await.unwrap();
    let client = cluster.rest_client();

    let topic_name = "sql-test";

    // Create topic
    client
        .create_topic(topic_name, 2)
        .await
        .expect("Failed to create topic");

    // Produce 500 records with structured data: {category: A/B/C, price: float, quantity: int}
    let categories = ["A", "B", "C"];
    let batch_records: Vec<BatchRecord> = (0..500)
        .map(|i| {
            let category = categories[i % 3];
            let price = 10.0 + (i as f64 * 0.5);
            let quantity = (i % 50) + 1;
            BatchRecord {
                key: Some(format!("sql-key-{}", i)),
                value: json!({
                    "category": category,
                    "price": price,
                    "quantity": quantity,
                })
                .to_string(),
                partition: None,
            }
        })
        .collect();

    // Produce in batches of 100
    for chunk in batch_records.chunks(100) {
        client
            .produce_batch(topic_name, chunk.to_vec())
            .await
            .expect("Failed to produce batch");
    }

    wait_for_flush().await;

    // Test 1: SELECT COUNT(*) should return 500
    let count_result = client
        .sql_query(&format!("SELECT COUNT(*) FROM \"{}\"", topic_name))
        .await
        .expect("SQL COUNT query failed");
    let total_count = count_result.rows[0][0]
        .as_u64()
        .or_else(|| {
            count_result.rows[0][0]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
        })
        .expect("COUNT should return a number");
    assert_eq!(total_count, 500, "SQL COUNT(*) should return 500");

    // Test 2: SELECT * LIMIT 10 should return exactly 10 rows
    let limit_result = client
        .sql_query(&format!("SELECT * FROM \"{}\" LIMIT 10", topic_name))
        .await
        .expect("SQL LIMIT query failed");
    assert_eq!(
        limit_result.row_count, 10,
        "SELECT * LIMIT 10 should return 10 rows"
    );

    // Test 3: WHERE filter on category
    let where_result = client
        .sql_query(&format!(
            "SELECT COUNT(*) FROM \"{}\" WHERE json_extract(value, '$.category') = 'A'",
            topic_name
        ))
        .await
        .expect("SQL WHERE query failed");
    let category_a_count = where_result.rows[0][0]
        .as_u64()
        .or_else(|| {
            where_result.rows[0][0]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
        })
        .expect("Filtered COUNT should return a number");
    // With 500 records and 3 categories (round-robin), category A should get ~167 records
    assert!(
        (165..=168).contains(&category_a_count),
        "Category A should have ~167 records, got {}",
        category_a_count
    );

    // Test 4: GROUP BY category
    let group_result = client
        .sql_query(&format!(
            "SELECT json_extract(value, '$.category') as category, COUNT(*) as cnt FROM \"{}\" GROUP BY json_extract(value, '$.category')",
            topic_name
        ))
        .await
        .expect("SQL GROUP BY query failed");
    assert_eq!(
        group_result.rows.len(),
        3,
        "GROUP BY category should yield 3 groups, got {}",
        group_result.rows.len()
    );

    // Cleanup
    client
        .delete_topic(topic_name)
        .await
        .expect("Failed to delete topic");
}

// ---------------------------------------------------------------------------
// Test: Ordering guarantees within a single partition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ordering_guarantees() {
    let cluster = TestCluster::start().await.unwrap();
    let client = cluster.rest_client();

    let topic_name = "ordering-test";

    // Create topic with 1 partition to guarantee total ordering
    client
        .create_topic(topic_name, 1)
        .await
        .expect("Failed to create topic");

    // Produce 100 records sequentially with keys "seq-0" through "seq-99"
    for i in 0..100u64 {
        let value = json!({"sequence": i}).to_string();
        client
            .produce(topic_name, Some(&format!("seq-{}", i)), &value)
            .await
            .unwrap_or_else(|_| panic!("Failed to produce record {}", i));
    }

    wait_for_flush().await;

    // Consume all from partition 0
    let mut all_records: Vec<streamhouse_e2e::ConsumedRecord> = Vec::new();
    let mut offset = 0u64;
    loop {
        let response = client
            .consume(topic_name, 0, offset, 100)
            .await
            .expect("Consume failed");

        if response.records.is_empty() {
            break;
        }

        let next_offset = response.next_offset;
        all_records.extend(response.records);

        if next_offset <= offset {
            break;
        }
        offset = next_offset;
    }

    assert_eq!(
        all_records.len(),
        100,
        "Should consume all 100 records, got {}",
        all_records.len()
    );

    // Verify offsets are monotonically increasing
    let mut prev_offset: Option<u64> = None;
    for record in &all_records {
        let current_offset = record.offset;
        if let Some(prev) = prev_offset {
            assert!(
                current_offset > prev,
                "Offsets should be monotonically increasing: {} should be > {}",
                current_offset,
                prev
            );
        }
        prev_offset = Some(current_offset);
    }

    // Verify order matches produce order
    for (i, record) in all_records.iter().enumerate() {
        let parsed: serde_json::Value = serde_json::from_str(&record.value).unwrap();
        let sequence = parsed["sequence"].as_u64().unwrap();
        assert_eq!(
            sequence, i as u64,
            "Record at position {} should have sequence {}, got {}",
            i, i, sequence
        );
    }

    // Cleanup
    client
        .delete_topic(topic_name)
        .await
        .expect("Failed to delete topic");
}

// ---------------------------------------------------------------------------
// Test: Multi-tenant isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multi_tenant_isolation() {
    let cluster = TestCluster::start().await.unwrap();

    // Create two organizations via the admin-style client (no auth)
    let client = cluster.rest_client();
    let org_alpha = client
        .create_org("org-alpha", "org-alpha")
        .await
        .expect("Failed to create org-alpha");
    let alpha_id = org_alpha["id"]
        .as_str()
        .expect("org-alpha should have an id")
        .to_string();

    let org_beta = client
        .create_org("org-beta", "org-beta")
        .await
        .expect("Failed to create org-beta");
    let beta_id = org_beta["id"]
        .as_str()
        .expect("org-beta should have an id")
        .to_string();

    // Create org-scoped clients using X-Organization-Id header (dev/no-auth mode)
    let mut alpha_client = cluster.rest_client();
    alpha_client.set_org_id(&alpha_id);
    let mut beta_client = cluster.rest_client();
    beta_client.set_org_id(&beta_id);

    // Create topic "shared" in both orgs — same name, different namespace
    let alpha_topic = alpha_client
        .create_topic("shared", 1)
        .await
        .expect("Failed to create topic in org-alpha");
    assert_eq!(alpha_topic.name, "shared");

    let beta_topic = beta_client
        .create_topic("shared", 1)
        .await
        .expect("Failed to create topic in org-beta -- should succeed for different org");
    assert_eq!(beta_topic.name, "shared");

    // Produce to org-alpha's "shared"
    let alpha_msg = json!({"org": "alpha", "secret": "alpha-data"}).to_string();
    alpha_client
        .produce("shared", Some("alpha-key"), &alpha_msg)
        .await
        .expect("Failed to produce to org-alpha's topic");

    wait_for_flush().await;

    // Consume from org-beta's "shared" -- should be empty or not contain alpha's data
    let beta_response = beta_client
        .consume("shared", 0, 0, 100)
        .await
        .expect("Failed to consume from org-beta's topic");

    for record in &beta_response.records {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&record.value) {
            assert_ne!(
                parsed["org"].as_str().unwrap_or(""),
                "alpha",
                "org-beta should NOT see org-alpha's data"
            );
            assert_ne!(
                parsed["secret"].as_str().unwrap_or(""),
                "alpha-data",
                "org-beta should NOT see org-alpha's secret data"
            );
        }
    }

    // Verify org-alpha CAN see its own data
    let alpha_response = alpha_client
        .consume("shared", 0, 0, 100)
        .await
        .expect("Failed to consume from org-alpha's topic");

    assert!(
        !alpha_response.records.is_empty(),
        "org-alpha should see its own data"
    );

    let alpha_value: serde_json::Value =
        serde_json::from_str(&alpha_response.records[0].value).unwrap();
    assert_eq!(
        alpha_value["org"].as_str().unwrap(),
        "alpha",
        "org-alpha should see its own records"
    );

    // Cleanup
    alpha_client
        .delete_topic("shared")
        .await
        .expect("Failed to cleanup alpha topic");
    beta_client
        .delete_topic("shared")
        .await
        .expect("Failed to cleanup beta topic");
}
