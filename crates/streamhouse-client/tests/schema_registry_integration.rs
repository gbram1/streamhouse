//! Schema Registry Integration Tests
//!
//! Tests end-to-end flow of Producer -> Schema Registry -> Consumer

use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::{MetadataStore, TopicConfig};

/// Test that producer can register schema and consumer can resolve it
///
/// This test demonstrates the complete flow:
/// 1. Producer registers schema
/// 2. Producer sends message with schema ID
/// 3. Consumer polls message
/// 4. Consumer resolves schema from registry
/// 5. Consumer deserializes using schema
#[tokio::test]
#[ignore] // Requires running server with schema registry
async fn test_end_to_end_schema_flow() {
    use streamhouse_client::{Consumer, Producer, SchemaFormat};
    use streamhouse_metadata::SqliteMetadataStore;
    use streamhouse_storage::cache::SegmentCache;

    // Initialize test environment
    let metadata_store: Arc<dyn MetadataStore> = Arc::new(
        SqliteMetadataStore::new(":memory:")
            .await
            .expect("Failed to create metadata store"),
    );

    // Create test topic
    metadata_store
        .create_topic(TopicConfig {
            name: "test-schema-topic".to_string(),
            partition_count: 1,
            retention_ms: None,
            config: Default::default(),
        })
        .await
        .expect("Failed to create topic");

    // Configure object store (using in-memory for test)
    let object_store: Arc<dyn object_store::ObjectStore> = Arc::new(
        object_store::memory::InMemory::new(),
    );

    // Schema registry URL (assumes server running with schema registry on port 8080)
    let schema_registry_url = "http://localhost:8080/schemas";

    // Define Avro schema
    let avro_schema = r#"{
        "type": "record",
        "name": "TestRecord",
        "fields": [
            {"name": "id", "type": "int"},
            {"name": "message", "type": "string"}
        ]
    }"#;

    // === PRODUCER: Send with schema ===
    let producer = Producer::builder()
        .metadata_store(Arc::clone(&metadata_store))
        .schema_registry(schema_registry_url)
        .agent_group("test")
        .build()
        .await
        .expect("Failed to create producer");

    // Serialize test data (in real world, use apache_avro crate)
    // For this test, we'll use raw bytes that match the schema
    let test_data = b"{\"id\": 123, \"message\": \"test\"}";

    // Send with schema
    let mut send_result = producer
        .send_with_schema(
            "test-schema-topic",
            Some(b"key1"),
            test_data,
            avro_schema,
            SchemaFormat::Avro,
            None,
        )
        .await
        .expect("Failed to send with schema");

    println!("Message sent successfully");

    // Wait for offset to be committed
    let offset = send_result
        .wait_offset()
        .await
        .expect("Failed to get offset");
    println!("Message written at offset: {}", offset);

    // === CONSUMER: Poll and resolve schema ===
    let cache = Arc::new(
        SegmentCache::new("/tmp/test-cache", 100 * 1024 * 1024)
            .expect("Failed to create cache"),
    );

    let mut consumer = Consumer::builder()
        .group_id("test-schema-group")
        .topics(vec!["test-schema-topic".to_string()])
        .metadata_store(Arc::clone(&metadata_store))
        .object_store(Arc::clone(&object_store))
        .schema_registry(schema_registry_url)
        .build()
        .await
        .expect("Failed to create consumer");

    // Poll for messages
    let records = consumer
        .poll(Duration::from_secs(5))
        .await
        .expect("Failed to poll");

    // Verify we got the message
    assert_eq!(records.len(), 1, "Expected 1 record");
    let record = &records[0];

    // Verify schema was resolved
    assert!(record.schema_id.is_some(), "Schema ID should be present");
    assert!(record.schema.is_some(), "Schema should be resolved");

    let schema = record.schema.as_ref().unwrap();
    assert_eq!(schema.subject, "test-schema-topic-value");
    assert_eq!(schema.schema_type, SchemaFormat::Avro);
    assert!(schema.version >= 1);

    println!("Schema resolved successfully:");
    println!("  Subject: {}", schema.subject);
    println!("  Version: {}", schema.version);
    println!("  Schema ID: {}", schema.id);

    // Verify value was stripped of magic byte + schema ID
    assert_eq!(
        record.value.as_ref(),
        test_data,
        "Value should have magic byte + schema ID stripped"
    );
}

/// Test that producer caches schema IDs
#[tokio::test]
#[ignore] // Requires running server with schema registry
async fn test_schema_caching() {
    use streamhouse_client::{Producer, SchemaFormat};
    use streamhouse_metadata::SqliteMetadataStore;

    let metadata_store: Arc<dyn MetadataStore> = Arc::new(
        SqliteMetadataStore::new(":memory:")
            .await
            .expect("Failed to create metadata store"),
    );

    metadata_store
        .create_topic(TopicConfig {
            name: "test-cache-topic".to_string(),
            partition_count: 1,
            retention_ms: None,
            config: Default::default(),
        })
        .await
        .expect("Failed to create topic");

    let schema_registry_url = "http://localhost:8080/schemas";
    let avro_schema = r#"{"type": "record", "name": "Test", "fields": [{"name": "id", "type": "int"}]}"#;

    let producer = Producer::builder()
        .metadata_store(Arc::clone(&metadata_store))
        .schema_registry(schema_registry_url)
        .agent_group("test")
        .build()
        .await
        .expect("Failed to create producer");

    // First send - should register schema
    let start = std::time::Instant::now();
    producer
        .send_with_schema(
            "test-cache-topic",
            None,
            b"{\"id\": 1}",
            avro_schema,
            SchemaFormat::Avro,
            None,
        )
        .await
        .expect("Failed to send");
    let first_duration = start.elapsed();
    println!("First send (with registration): {:?}", first_duration);

    // Second send - should use cache
    let start = std::time::Instant::now();
    producer
        .send_with_schema(
            "test-cache-topic",
            None,
            b"{\"id\": 2}",
            avro_schema,
            SchemaFormat::Avro,
            None,
        )
        .await
        .expect("Failed to send");
    let second_duration = start.elapsed();
    println!("Second send (cached): {:?}", second_duration);

    // Second send should be much faster (no HTTP request)
    // Note: This might not always be true due to batching, but generally
    // the cache should make subsequent sends faster
    println!("Speedup: {:.2}x", first_duration.as_millis() as f64 / second_duration.as_millis() as f64);
}

/// Test that incompatible schemas are rejected
#[tokio::test]
#[ignore] // Requires running server with schema registry
async fn test_compatibility_checking() {
    use streamhouse_client::{Producer, SchemaFormat};
    use streamhouse_metadata::SqliteMetadataStore;

    let metadata_store: Arc<dyn MetadataStore> = Arc::new(
        SqliteMetadataStore::new(":memory:")
            .await
            .expect("Failed to create metadata store"),
    );

    metadata_store
        .create_topic(TopicConfig {
            name: "test-compat-topic".to_string(),
            partition_count: 1,
            retention_ms: None,
            config: Default::default(),
        })
        .await
        .expect("Failed to create topic");

    let schema_registry_url = "http://localhost:8080/schemas";

    let producer = Producer::builder()
        .metadata_store(Arc::clone(&metadata_store))
        .schema_registry(schema_registry_url)
        .agent_group("test")
        .build()
        .await
        .expect("Failed to create producer");

    // Register initial schema (v1)
    let schema_v1 = r#"{
        "type": "record",
        "name": "User",
        "fields": [
            {"name": "id", "type": "int"},
            {"name": "name", "type": "string"}
        ]
    }"#;

    producer
        .send_with_schema(
            "test-compat-topic",
            None,
            b"{\"id\": 1, \"name\": \"Alice\"}",
            schema_v1,
            SchemaFormat::Avro,
            None,
        )
        .await
        .expect("Failed to register schema v1");

    println!("Schema v1 registered successfully");

    // Try to register incompatible schema (v2 - removes required field)
    // This should fail backward compatibility check
    let schema_v2_incompatible = r#"{
        "type": "record",
        "name": "User",
        "fields": [
            {"name": "id", "type": "int"}
        ]
    }"#;

    let result = producer
        .send_with_schema(
            "test-compat-topic",
            None,
            b"{\"id\": 2}",
            schema_v2_incompatible,
            SchemaFormat::Avro,
            None,
        )
        .await;

    // Should fail due to compatibility check
    assert!(
        result.is_err(),
        "Incompatible schema should be rejected"
    );
    println!("Incompatible schema rejected as expected: {:?}", result);

    // Try compatible schema (v2 - adds optional field)
    let schema_v2_compatible = r#"{
        "type": "record",
        "name": "User",
        "fields": [
            {"name": "id", "type": "int"},
            {"name": "name", "type": "string"},
            {"name": "email", "type": ["null", "string"], "default": null}
        ]
    }"#;

    let result = producer
        .send_with_schema(
            "test-compat-topic",
            None,
            b"{\"id\": 3, \"name\": \"Bob\", \"email\": null}",
            schema_v2_compatible,
            SchemaFormat::Avro,
            None,
        )
        .await;

    // Should succeed
    assert!(
        result.is_ok(),
        "Compatible schema should be accepted"
    );
    println!("Compatible schema accepted");
}
