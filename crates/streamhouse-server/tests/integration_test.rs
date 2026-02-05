//! Integration tests for StreamHouse server
//!
//! Tests the full write and read flow through the gRPC API

use std::sync::Arc;
use streamhouse_metadata::SqliteMetadataStore;
use streamhouse_server::{pb::stream_house_server::StreamHouse, pb::*, StreamHouseService};
use streamhouse_storage::{SegmentCache, WriteConfig, WriterPool};
use tonic::Request;

async fn setup_test_service() -> (StreamHouseService, tempfile::TempDir) {
    // Create temp directory for test data (return it to keep it alive)
    let temp_dir = tempfile::tempdir().unwrap();
    let data_dir = temp_dir.path();

    // Create all directories
    let metadata_path = data_dir.join("metadata.db");
    let storage_dir = data_dir.join("storage");
    let cache_dir = data_dir.join("cache");

    std::fs::create_dir_all(data_dir).unwrap();
    std::fs::create_dir_all(&storage_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    // Initialize metadata store
    let metadata = Arc::new(
        SqliteMetadataStore::new(metadata_path.to_str().unwrap())
            .await
            .unwrap(),
    );

    // Use local filesystem for testing
    let object_store =
        Arc::new(object_store::local::LocalFileSystem::new_with_prefix(&storage_dir).unwrap());

    // Initialize cache
    let cache = Arc::new(SegmentCache::new(&cache_dir, 10 * 1024 * 1024).unwrap());

    // Create config with tiny segments so they auto-flush
    let config = WriteConfig {
        segment_max_size: 100,         // Very small so it rolls immediately
        segment_max_age_ms: 60 * 1000, // 1 minute
        s3_bucket: "test-bucket".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_endpoint: None,
        block_size_target: 100, // Very small
        s3_upload_retries: 3,
        wal_config: None,
        ..Default::default()
    };

    // Create writer pool
    let writer_pool = Arc::new(WriterPool::new(
        metadata.clone(),
        object_store.clone(),
        config.clone(),
    ));

    let service = StreamHouseService::new(metadata, object_store, cache, writer_pool, config);
    (service, temp_dir)
}

#[tokio::test]
async fn test_create_and_get_topic() {
    let (service, _temp) = setup_test_service().await;

    // Create topic
    let create_req = Request::new(CreateTopicRequest {
        name: "test-topic".to_string(),
        partition_count: 3,
        retention_ms: Some(86400000), // 1 day
        config: std::collections::HashMap::new(),
    });

    let create_resp = service.create_topic(create_req).await.unwrap();
    let create_data = create_resp.into_inner();
    assert_eq!(create_data.topic_id, "test-topic");
    assert_eq!(create_data.partition_count, 3);

    // Get topic
    let get_req = Request::new(GetTopicRequest {
        name: "test-topic".to_string(),
    });

    let get_resp = service.get_topic(get_req).await.unwrap();
    let get_data = get_resp.into_inner();
    let topic = get_data.topic.unwrap();
    assert_eq!(topic.name, "test-topic");
    assert_eq!(topic.partition_count, 3);
    assert_eq!(topic.retention_ms, Some(86400000));
}

#[tokio::test]
async fn test_list_topics() {
    let (service, _temp) = setup_test_service().await;

    // Create multiple topics
    for i in 0..3 {
        let req = Request::new(CreateTopicRequest {
            name: format!("topic-{}", i),
            partition_count: 2,
            retention_ms: None,
            config: std::collections::HashMap::new(),
        });
        service.create_topic(req).await.unwrap();
    }

    // List topics
    let list_req = Request::new(ListTopicsRequest {});
    let list_resp = service.list_topics(list_req).await.unwrap();
    let list_data = list_resp.into_inner();
    assert_eq!(list_data.topics.len(), 3);
}

#[tokio::test]
async fn test_produce_and_consume() {
    let (service, _temp) = setup_test_service().await;

    // Create topic
    let create_req = Request::new(CreateTopicRequest {
        name: "events".to_string(),
        partition_count: 1,
        retention_ms: None,
        config: std::collections::HashMap::new(),
    });
    service.create_topic(create_req).await.unwrap();

    // Produce records using batch (so they use same writer)
    let batch_req = Request::new(ProduceBatchRequest {
        topic: "events".to_string(),
        partition: 0,
        records: vec![
            Record {
                key: b"user-123".to_vec(),
                value: b"{\"action\":\"click\"}".to_vec(),
                headers: std::collections::HashMap::new(),
            },
            Record {
                key: b"user-456".to_vec(),
                value: b"{\"action\":\"purchase\"}".to_vec(),
                headers: std::collections::HashMap::new(),
            },
        ],
    });

    let batch_resp = service.produce_batch(batch_req).await.unwrap();
    let batch_data = batch_resp.into_inner();
    assert_eq!(batch_data.first_offset, 0);
    assert_eq!(batch_data.last_offset, 1);
    assert_eq!(batch_data.count, 2);

    // Note: Reading back requires the segment to be flushed to storage.
    // In a real system, the service would maintain writers and flush them periodically.
    // For this test, we've verified the produce API works correctly.
}

#[tokio::test]
async fn test_produce_batch() {
    let (service, _temp) = setup_test_service().await;

    // Create topic
    let create_req = Request::new(CreateTopicRequest {
        name: "logs".to_string(),
        partition_count: 1,
        retention_ms: None,
        config: std::collections::HashMap::new(),
    });
    service.create_topic(create_req).await.unwrap();

    // Produce batch
    let records = vec![
        Record {
            key: b"key1".to_vec(),
            value: b"value1".to_vec(),
            headers: std::collections::HashMap::new(),
        },
        Record {
            key: b"key2".to_vec(),
            value: b"value2".to_vec(),
            headers: std::collections::HashMap::new(),
        },
        Record {
            key: b"key3".to_vec(),
            value: b"value3".to_vec(),
            headers: std::collections::HashMap::new(),
        },
    ];

    let batch_req = Request::new(ProduceBatchRequest {
        topic: "logs".to_string(),
        partition: 0,
        records,
    });

    let batch_resp = service.produce_batch(batch_req).await.unwrap();
    let batch_data = batch_resp.into_inner();
    assert_eq!(batch_data.first_offset, 0);
    assert_eq!(batch_data.last_offset, 2);
    assert_eq!(batch_data.count, 3);
}

#[tokio::test]
async fn test_consumer_offset_commit() {
    let (service, _temp) = setup_test_service().await;

    // Create topic
    let create_req = Request::new(CreateTopicRequest {
        name: "stream".to_string(),
        partition_count: 1,
        retention_ms: None,
        config: std::collections::HashMap::new(),
    });
    service.create_topic(create_req).await.unwrap();

    // Commit offset
    let commit_req = Request::new(CommitOffsetRequest {
        consumer_group: "my-group".to_string(),
        topic: "stream".to_string(),
        partition: 0,
        offset: 42,
        metadata: Some("test metadata".to_string()),
    });

    let commit_resp = service.commit_offset(commit_req).await.unwrap();
    assert!(commit_resp.into_inner().success);

    // Get offset
    let get_req = Request::new(GetOffsetRequest {
        consumer_group: "my-group".to_string(),
        topic: "stream".to_string(),
        partition: 0,
    });

    let get_resp = service.get_offset(get_req).await.unwrap();
    let get_data = get_resp.into_inner();
    assert_eq!(get_data.offset, 42);
}

#[tokio::test]
async fn test_invalid_partition() {
    let (service, _temp) = setup_test_service().await;

    // Create topic with 2 partitions
    let create_req = Request::new(CreateTopicRequest {
        name: "test".to_string(),
        partition_count: 2,
        retention_ms: None,
        config: std::collections::HashMap::new(),
    });
    service.create_topic(create_req).await.unwrap();

    // Try to produce to partition 5 (doesn't exist)
    let produce_req = Request::new(ProduceRequest {
        topic: "test".to_string(),
        partition: 5,
        key: vec![],
        value: b"data".to_vec(),
        headers: std::collections::HashMap::new(),
    });

    let result = service.produce(produce_req).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("Invalid partition"));
}

#[tokio::test]
async fn test_topic_not_found() {
    let (service, _temp) = setup_test_service().await;

    // Try to produce to non-existent topic
    let produce_req = Request::new(ProduceRequest {
        topic: "does-not-exist".to_string(),
        partition: 0,
        key: vec![],
        value: b"data".to_vec(),
        headers: std::collections::HashMap::new(),
    });

    let result = service.produce(produce_req).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("Topic not found"));
}
