//! E2E tests using real PostgreSQL and MinIO containers via testcontainers.
//!
//! These tests validate the production storage path (Postgres metadata + S3 object store)
//! instead of the in-memory SQLite + local filesystem used by the default tests.
//!
//! Requires Docker to be running. Tests are skipped if Docker is not available.
//! Enable with: `cargo nextest run -p streamhouse-e2e --features postgres`

#![cfg(feature = "postgres")]

use serde_json::json;
use streamhouse_e2e::{BatchRecord, TestCluster};

async fn wait_for_flush() {
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
}

// ---------------------------------------------------------------------------
// Test: Full write-read path with Postgres + MinIO
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_postgres_minio_full_write_read_path() {
    streamhouse_e2e::init_tracing();

    let cluster = TestCluster::builder()
        .with_postgres()
        .with_minio()
        .start()
        .await
        .expect("failed to start cluster with postgres + minio");

    let client = cluster.rest_client();

    // Health check
    let health = client.health().await.expect("health check failed");
    assert_eq!(health["status"], "ok");

    // Create topic
    cluster
        .create_topic_with_leases("pg-test-topic", 2)
        .await
        .expect("create topic failed");

    // Produce records
    let record_count = 50;
    let mut records = Vec::new();
    for i in 0..record_count {
        records.push(BatchRecord {
            key: Some(format!("key-{}", i)),
            value: json!({
                "index": i,
                "message": format!("postgres-minio-test-{}", i),
            })
            .to_string(),
            partition: Some(i % 2),
        });
    }

    let batch_resp = client
        .produce_batch("pg-test-topic", records)
        .await
        .expect("batch produce failed");
    assert_eq!(batch_resp.count, record_count as usize);

    // Wait for flush to S3 (MinIO)
    wait_for_flush().await;

    // Consume from both partitions and verify
    let mut total_consumed = 0;
    for partition in 0..2u32 {
        let resp = client
            .consume("pg-test-topic", partition, 0, 100)
            .await
            .expect("consume failed");
        total_consumed += resp.records.len();
    }
    assert_eq!(total_consumed, record_count as usize);

    // SQL query
    let sql_resp = client
        .sql_query("SELECT COUNT(*) as cnt FROM \"pg-test-topic\"")
        .await
        .expect("SQL query failed");
    assert_eq!(sql_resp.row_count, 1);
}

// ---------------------------------------------------------------------------
// Test: Topic CRUD with Postgres metadata
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_postgres_topic_crud() {
    streamhouse_e2e::init_tracing();

    let cluster = TestCluster::builder()
        .with_postgres()
        .start()
        .await
        .expect("failed to start cluster with postgres");

    let client = cluster.rest_client();

    // Create
    let topic = client
        .create_topic("crud-topic", 3)
        .await
        .expect("create topic failed");
    assert_eq!(topic.name, "crud-topic");
    assert_eq!(topic.partitions, 3);

    // Get
    let fetched = client
        .get_topic("crud-topic")
        .await
        .expect("get topic failed");
    assert_eq!(fetched.name, "crud-topic");

    // List
    let topics = client.list_topics().await.expect("list topics failed");
    assert!(topics.iter().any(|t| t.name == "crud-topic"));

    // Delete
    client
        .delete_topic("crud-topic")
        .await
        .expect("delete topic failed");

    let topics = client.list_topics().await.expect("list topics failed");
    assert!(!topics.iter().any(|t| t.name == "crud-topic"));
}

// ---------------------------------------------------------------------------
// Test: Schema validation with Postgres + MinIO
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_postgres_minio_schema_validation() {
    streamhouse_e2e::init_tracing();

    let cluster = TestCluster::builder()
        .with_postgres()
        .with_minio()
        .start()
        .await
        .expect("failed to start cluster");

    let client = cluster.rest_client();

    cluster
        .create_topic_with_leases("schema-pg-topic", 1)
        .await
        .expect("create topic failed");

    // Register a JSON schema
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name", "age"]
    });

    let schema_resp = client
        .register_schema("schema-pg-topic-value", &schema.to_string(), "JSON")
        .await
        .expect("register schema failed");
    assert!(schema_resp.id > 0);

    // Valid produce should succeed
    let valid_value = json!({"name": "Alice", "age": 30}).to_string();
    let resp = client
        .produce("schema-pg-topic", Some("k1"), &valid_value)
        .await;
    assert!(resp.is_ok());

    // Invalid produce should fail
    let invalid_value = json!({"name": "Bob"}).to_string(); // missing "age"
    let resp = client
        .produce("schema-pg-topic", Some("k2"), &invalid_value)
        .await;
    assert!(resp.is_err());
}

// ---------------------------------------------------------------------------
// Test: Multi-tenant isolation with Postgres
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_postgres_multi_tenant_isolation() {
    streamhouse_e2e::init_tracing();

    let cluster = TestCluster::builder()
        .with_postgres()
        .start()
        .await
        .expect("failed to start cluster");

    let client = cluster.rest_client();

    // Create two organizations (no auth — admin bootstrap)
    let org1 = client
        .create_org("Org Alpha", "org-alpha")
        .await
        .expect("create org1 failed");
    let org1_id = org1["id"].as_str().expect("org1 missing id");

    let org2 = client
        .create_org("Org Beta", "org-beta")
        .await
        .expect("create org2 failed");
    let org2_id = org2["id"].as_str().expect("org2 missing id");

    // Each org creates a topic with the same name using org ID headers
    let mut client1 = cluster.rest_client();
    client1.set_org_id(org1_id);
    let topic1 = client1
        .create_topic("shared-name", 1)
        .await
        .expect("org1 create topic failed");
    assert_eq!(topic1.name, "shared-name");

    let mut client2 = cluster.rest_client();
    client2.set_org_id(org2_id);
    let topic2 = client2
        .create_topic("shared-name", 1)
        .await
        .expect("org2 create topic failed");
    assert_eq!(topic2.name, "shared-name");

    // Org1's topics should not include org2's
    let org1_topics = client1.list_topics().await.expect("list org1 topics");
    let org2_topics = client2.list_topics().await.expect("list org2 topics");
    assert_eq!(org1_topics.len(), 1);
    assert_eq!(org2_topics.len(), 1);
}

// ---------------------------------------------------------------------------
// Test: Consumer groups with Postgres
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_postgres_consumer_group_offsets() {
    streamhouse_e2e::init_tracing();

    let cluster = TestCluster::builder()
        .with_postgres()
        .with_minio()
        .start()
        .await
        .expect("failed to start cluster");

    let client = cluster.rest_client();

    cluster
        .create_topic_with_leases("cg-pg-topic", 1)
        .await
        .expect("create topic failed");

    // Produce some records
    for i in 0..10 {
        client
            .produce(
                "cg-pg-topic",
                Some(&format!("k{}", i)),
                &format!("value-{}", i),
            )
            .await
            .expect("produce failed");
    }

    wait_for_flush().await;

    // Commit offset
    client
        .commit_offset("test-group", "cg-pg-topic", 0, 5)
        .await
        .expect("commit offset failed");

    // Verify consumer group exists
    let groups = client
        .list_consumer_groups()
        .await
        .expect("list groups failed");
    assert!(!groups.is_empty());
}
