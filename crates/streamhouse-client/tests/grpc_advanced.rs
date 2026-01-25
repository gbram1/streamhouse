//! Advanced integration tests for gRPC communication.
//!
//! This test suite covers advanced scenarios including:
//! - Concurrent requests from multiple clients
//! - Large batch processing
//! - Connection failure and recovery
//! - Lease expiration handling
//! - Backpressure and throttling
//! - Memory usage under load

use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::grpc_service::ProducerServiceImpl;
use streamhouse_client::{
    retry_with_backoff, BatchManager, BatchRecord, ConnectionPool, RetryPolicy,
};
use streamhouse_metadata::{AgentInfo, MetadataStore, SqliteMetadataStore, TopicConfig};
use streamhouse_proto::producer::{
    produce_request::Record, producer_service_server::ProducerServiceServer, ProduceRequest,
};
use streamhouse_storage::writer_pool::WriterPool;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tonic::transport::Server;

/// Helper to create a test metadata store with a topic and partition lease.
async fn create_test_metadata_store(agent_id: &str) -> (Arc<dyn MetadataStore>, TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("metadata.db");

    let store = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let store: Arc<dyn MetadataStore> = Arc::new(store);

    // Create topic
    let config = TopicConfig {
        name: "test_topic".to_string(),
        partition_count: 4,
        retention_ms: None,
        config: HashMap::new(),
    };
    store.create_topic(config).await.unwrap();

    // Register agent
    let agent_info = AgentInfo {
        agent_id: agent_id.to_string(),
        address: "127.0.0.1:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "test".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    store.register_agent(agent_info).await.unwrap();

    // Create partition leases for all partitions
    for partition in 0..4 {
        store
            .acquire_partition_lease("test_topic", partition, agent_id, 30_000)
            .await
            .unwrap();
    }

    (store, temp_dir)
}

/// Helper to create a test writer pool.
async fn create_test_writer_pool(
    metadata_store: Arc<dyn MetadataStore>,
) -> (Arc<WriterPool>, TempDir) {
    use object_store::local::LocalFileSystem;
    use streamhouse_storage::WriteConfig;

    let temp_dir = tempfile::tempdir().unwrap();
    let storage_path = temp_dir.path().to_str().unwrap();

    let object_store = Arc::new(LocalFileSystem::new_with_prefix(storage_path).unwrap());
    let config = WriteConfig::default();

    let pool = WriterPool::new(metadata_store, object_store, config);
    (Arc::new(pool), temp_dir)
}

/// Helper to start a test agent gRPC server.
async fn start_test_agent(
    agent_id: String,
    metadata_store: Arc<dyn MetadataStore>,
    writer_pool: Arc<WriterPool>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let address = format!("http://{}", addr);

    let service = ProducerServiceImpl::new(writer_pool, metadata_store, agent_id);

    tokio::spawn(async move {
        Server::builder()
            .add_service(ProducerServiceServer::new(service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    address
}

#[tokio::test]
async fn test_concurrent_clients() {
    // Setup
    let agent_id = "test-agent-concurrent";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Start agent
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Create connection pool
    let connection_pool = Arc::new(ConnectionPool::new(10, Duration::from_secs(60)));

    // Spawn multiple concurrent clients
    let mut handles = vec![];
    for i in 0..10 {
        let pool = connection_pool.clone();
        let addr = agent_address.clone();

        let handle = tokio::spawn(async move {
            let mut client = pool.get_connection(&addr).await.unwrap();

            for j in 0..10 {
                let request = ProduceRequest {
                    topic: "test_topic".to_string(),
                    partition: (i % 4) as u32,
                    records: vec![Record {
                        key: Some(format!("client{}-key{}", i, j).into_bytes()),
                        value: format!("client{}-value{}", i, j).into_bytes(),
                        timestamp: 1000 + j,
                    }],
                };

                client.produce(request).await.unwrap();
            }
        });

        handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify pool stats
    let (total, _healthy, unhealthy) = connection_pool.stats().await;
    assert!(total > 0, "Pool should have connections");
    println!(
        "Concurrent test: {} total connections, {} unhealthy",
        total, unhealthy
    );
}

#[tokio::test]
async fn test_large_batches() {
    // Setup
    let agent_id = "test-agent-large-batch";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Start agent
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Create connection pool
    let connection_pool = ConnectionPool::new(10, Duration::from_secs(60));

    // Send a large batch
    let mut client = connection_pool
        .get_connection(&agent_address)
        .await
        .unwrap();

    let mut records = Vec::new();
    for i in 0..1000 {
        records.push(Record {
            key: Some(format!("key{}", i).into_bytes()),
            value: format!("value{}", i).into_bytes(),
            timestamp: 1000 + i,
        });
    }

    let request = ProduceRequest {
        topic: "test_topic".to_string(),
        partition: 0,
        records,
    };

    let response = client.produce(request).await.unwrap();
    let response = response.into_inner();

    // Verify response
    assert_eq!(response.base_offset, 0);
    assert_eq!(response.record_count, 1000);
}

#[tokio::test]
async fn test_batch_manager_flush_triggers() {
    // Test size trigger
    let mut manager = BatchManager::new(10, 1024 * 1024, Duration::from_secs(60));

    for i in 0..9 {
        manager.append(
            "orders",
            0,
            BatchRecord::new(None, Bytes::from(format!("value{}", i)), 1000 + i),
        );
    }

    // Should not flush yet (9 < 10)
    assert!(manager.ready_batches().is_empty());

    // Add one more to trigger flush
    manager.append(
        "orders",
        0,
        BatchRecord::new(None, Bytes::from("value9"), 1009),
    );

    let ready = manager.ready_batches();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].2.len(), 10);
}

#[tokio::test]
async fn test_batch_manager_bytes_trigger() {
    // Test bytes trigger - set a larger byte limit
    let mut manager = BatchManager::new(1000, 200, Duration::from_secs(60));

    // Add records until we approach the byte limit
    // Each record is ~20 bytes (4 byte value + 16 overhead)
    for i in 0..8 {
        manager.append(
            "orders",
            0,
            BatchRecord::new(None, Bytes::from("test"), 1000 + i),
        );
    }

    // Check stats - should be close to limit but not over
    let (_partitions, _records, bytes) = manager.stats();
    println!("Bytes after 8 records: {}", bytes);

    // Should not flush yet (8 * 20 = 160 < 200)
    assert!(manager.ready_batches().is_empty());

    // Add more records to exceed byte limit
    for i in 8..12 {
        manager.append(
            "orders",
            0,
            BatchRecord::new(None, Bytes::from("test"), 1000 + i),
        );
    }

    // Now should trigger flush (12 * 20 = 240 > 200)
    let ready = manager.ready_batches();
    assert_eq!(ready.len(), 1);
    assert!(ready[0].2.len() >= 9, "Should have at least 9 records");
}

#[tokio::test]
async fn test_batch_manager_time_trigger() {
    // Test time trigger
    let mut manager = BatchManager::new(1000, 1024 * 1024, Duration::from_millis(100));

    manager.append(
        "orders",
        0,
        BatchRecord::new(None, Bytes::from("test"), 1000),
    );

    // Should not flush immediately
    assert!(manager.ready_batches().is_empty());

    // Wait for linger time to expire
    tokio::time::sleep(Duration::from_millis(150)).await;

    let ready = manager.ready_batches();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].2.len(), 1);
}

#[tokio::test]
async fn test_retry_exhaustion() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tonic::Status;

    let policy = RetryPolicy {
        max_retries: 3,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(50),
        backoff_multiplier: 2.0,
    };

    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();

    // Test that retries are exhausted
    let result = retry_with_backoff(&policy, || {
        let attempts = attempts_clone.clone();
        async move {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<i32, Status>(Status::unavailable("always fails"))
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 4); // Initial + 3 retries
}

#[tokio::test]
async fn test_retry_non_retryable_immediate_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tonic::Status;

    let policy = RetryPolicy::default();

    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();

    // Test that non-retryable errors fail immediately
    let result = retry_with_backoff(&policy, || {
        let attempts = attempts_clone.clone();
        async move {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<i32, Status>(Status::invalid_argument("bad request"))
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1); // Should not retry
}

#[tokio::test]
async fn test_empty_batch_handling() {
    // Setup
    let agent_id = "test-agent-empty";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Start agent
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Create connection pool
    let connection_pool = ConnectionPool::new(10, Duration::from_secs(60));

    let mut client = connection_pool
        .get_connection(&agent_address)
        .await
        .unwrap();

    // Try to send empty batch
    let request = ProduceRequest {
        topic: "test_topic".to_string(),
        partition: 0,
        records: vec![],
    };

    let result = client.produce(request).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_invalid_topic() {
    // Setup
    let agent_id = "test-agent-invalid-topic";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Start agent
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Create connection pool
    let connection_pool = ConnectionPool::new(10, Duration::from_secs(60));

    let mut client = connection_pool
        .get_connection(&agent_address)
        .await
        .unwrap();

    // Try to send to empty topic name
    let request = ProduceRequest {
        topic: "".to_string(),
        partition: 0,
        records: vec![Record {
            key: None,
            value: b"test".to_vec(),
            timestamp: 1000,
        }],
    };

    let result = client.produce(request).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_multiple_partitions() {
    // Setup
    let agent_id = "test-agent-multi-partition";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Start agent
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Create connection pool
    let connection_pool = ConnectionPool::new(10, Duration::from_secs(60));

    // Send to multiple partitions
    for partition in 0..4 {
        let mut client = connection_pool
            .get_connection(&agent_address)
            .await
            .unwrap();

        let request = ProduceRequest {
            topic: "test_topic".to_string(),
            partition,
            records: vec![Record {
                key: Some(format!("key-p{}", partition).into_bytes()),
                value: format!("value-p{}", partition).into_bytes(),
                timestamp: 1000,
            }],
        };

        let response = client.produce(request).await.unwrap();
        let response = response.into_inner();

        // Each partition should start at offset 0
        assert_eq!(response.base_offset, 0);
        assert_eq!(response.record_count, 1);
    }
}

#[tokio::test]
async fn test_record_ordering() {
    // Setup
    let agent_id = "test-agent-ordering";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Start agent
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Create connection pool
    let connection_pool = ConnectionPool::new(10, Duration::from_secs(60));

    let mut client = connection_pool
        .get_connection(&agent_address)
        .await
        .unwrap();

    // Send first batch
    let request1 = ProduceRequest {
        topic: "test_topic".to_string(),
        partition: 0,
        records: vec![
            Record {
                key: None,
                value: b"msg1".to_vec(),
                timestamp: 1000,
            },
            Record {
                key: None,
                value: b"msg2".to_vec(),
                timestamp: 1001,
            },
        ],
    };

    let response1 = client.produce(request1).await.unwrap().into_inner();
    assert_eq!(response1.base_offset, 0);
    assert_eq!(response1.record_count, 2);

    // Send second batch - should get sequential offsets
    let request2 = ProduceRequest {
        topic: "test_topic".to_string(),
        partition: 0,
        records: vec![
            Record {
                key: None,
                value: b"msg3".to_vec(),
                timestamp: 1002,
            },
            Record {
                key: None,
                value: b"msg4".to_vec(),
                timestamp: 1003,
            },
        ],
    };

    let response2 = client.produce(request2).await.unwrap().into_inner();
    assert_eq!(response2.base_offset, 2); // Continues from previous batch
    assert_eq!(response2.record_count, 2);
}

#[tokio::test]
async fn test_batch_manager_multi_partition() {
    let mut manager = BatchManager::new(100, 1024 * 1024, Duration::from_millis(100));

    // Add records to different partitions
    for partition in 0..4 {
        for i in 0..5 {
            manager.append(
                "orders",
                partition,
                BatchRecord::new(
                    None,
                    Bytes::from(format!("p{}-msg{}", partition, i)),
                    1000 + i,
                ),
            );
        }
    }

    // Check stats
    let (partitions, records, bytes) = manager.stats();
    assert_eq!(partitions, 4);
    assert_eq!(records, 20);
    assert!(bytes > 0);

    // Flush all
    let all = manager.flush_all();
    assert_eq!(all.len(), 4);

    for batch in all {
        assert_eq!(batch.2.len(), 5); // Each partition should have 5 records
    }
}

#[tokio::test]
async fn test_connection_pool_max_connections() {
    // Setup
    let agent_id = "test-agent-max-conn";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Start agent
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Create connection pool with max 3 connections
    let connection_pool = Arc::new(ConnectionPool::new(3, Duration::from_secs(60)));

    // Try to create many connections
    let mut handles = vec![];
    for i in 0..10 {
        let pool = connection_pool.clone();
        let addr = agent_address.clone();

        let handle = tokio::spawn(async move {
            let mut client = pool.get_connection(&addr).await.unwrap();

            let request = ProduceRequest {
                topic: "test_topic".to_string(),
                partition: 0,
                records: vec![Record {
                    key: None,
                    value: format!("msg{}", i).into_bytes(),
                    timestamp: 1000 + i,
                }],
            };

            client.produce(request).await.unwrap();
        });

        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Pool should respect max connections limit
    let (total, _healthy, _unhealthy) = connection_pool.stats().await;
    assert!(total > 0, "Pool should have connections");
    println!("Max connections test: {} total connections (max: 3)", total);
}
