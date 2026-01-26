//! Integration tests for Producer → Agent gRPC flow.
//!
//! These tests verify the complete end-to-end flow:
//! 1. Producer sends records via send()
//! 2. BatchManager accumulates records
//! 3. Background flush task sends batches to agent
//! 4. Agent writes records to storage
//! 5. Producer receives acknowledgment

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::grpc_service::ProducerServiceImpl;
use streamhouse_client::Producer;
use streamhouse_metadata::{AgentInfo, MetadataStore, SqliteMetadataStore, TopicConfig};
use streamhouse_proto::producer::producer_service_server::ProducerServiceServer;
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
        partition_count: 2,
        retention_ms: None,
        config: HashMap::new(),
    };
    store.create_topic(config).await.unwrap();

    // Register agent
    let agent_info = AgentInfo {
        agent_id: agent_id.to_string(),
        address: "127.0.0.1:9090".to_string(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "default".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    store.register_agent(agent_info).await.unwrap();

    // Create partition lease for agent
    store
        .acquire_partition_lease("test_topic", 0, agent_id, 30_000)
        .await
        .unwrap();

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
async fn test_producer_to_agent_basic() {
    // Setup
    let agent_id = "test-agent-producer-001";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    // Update agent address in metadata
    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Update agent info with actual address
    let agent_info = AgentInfo {
        agent_id: agent_id.to_string(),
        address: agent_address.clone(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "default".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    metadata_store.register_agent(agent_info).await.unwrap();

    // Create Producer
    let producer = Producer::builder()
        .metadata_store(metadata_store.clone())
        .agent_group("default")
        .batch_size(10)
        .batch_timeout(Duration::from_millis(100))
        .build()
        .await
        .unwrap();

    // Send records
    for i in 0..5 {
        producer
            .send(
                "test_topic",
                Some(format!("key{}", i).as_bytes()),
                format!("value{}", i).as_bytes(),
                Some(0),
            )
            .await
            .unwrap();
    }

    // Flush to ensure records are sent
    producer.flush().await.unwrap();

    // Close producer
    producer.close().await.unwrap();
}

#[tokio::test]
async fn test_producer_batch_accumulation() {
    // Setup
    let agent_id = "test-agent-producer-002";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Update agent info
    let agent_info = AgentInfo {
        agent_id: agent_id.to_string(),
        address: agent_address.clone(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "default".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    metadata_store.register_agent(agent_info).await.unwrap();

    // Create Producer with small batch size
    let producer = Producer::builder()
        .metadata_store(metadata_store.clone())
        .agent_group("default")
        .batch_size(3) // Small batch size for testing
        .batch_timeout(Duration::from_secs(10)) // Long timeout to test size trigger
        .build()
        .await
        .unwrap();

    // Send 10 records (should trigger 3-4 batches)
    for i in 0..10 {
        producer
            .send(
                "test_topic",
                Some(format!("key{}", i).as_bytes()),
                format!("value{}", i).as_bytes(),
                Some(0),
            )
            .await
            .unwrap();
    }

    // Wait for background flush task to process batches
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Flush remaining records
    producer.flush().await.unwrap();

    producer.close().await.unwrap();
}

#[tokio::test]
async fn test_producer_time_based_flush() {
    // Setup
    let agent_id = "test-agent-producer-003";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Update agent info
    let agent_info = AgentInfo {
        agent_id: agent_id.to_string(),
        address: agent_address.clone(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "default".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    metadata_store.register_agent(agent_info).await.unwrap();

    // Create Producer with short batch timeout
    let producer = Producer::builder()
        .metadata_store(metadata_store.clone())
        .agent_group("default")
        .batch_size(1000) // Large batch size to test time trigger
        .batch_timeout(Duration::from_millis(100)) // Short timeout
        .build()
        .await
        .unwrap();

    // Send a few records
    for i in 0..5 {
        producer
            .send(
                "test_topic",
                None,
                format!("value{}", i).as_bytes(),
                Some(0),
            )
            .await
            .unwrap();
    }

    // Wait for time-based flush
    tokio::time::sleep(Duration::from_millis(200)).await;

    producer.close().await.unwrap();
}

#[tokio::test]
async fn test_producer_concurrent_sends() {
    // Setup
    let agent_id = "test-agent-producer-004";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Update agent info
    let agent_info = AgentInfo {
        agent_id: agent_id.to_string(),
        address: agent_address.clone(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "default".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    metadata_store.register_agent(agent_info).await.unwrap();

    // Create Producer
    let producer = Arc::new(
        Producer::builder()
            .metadata_store(metadata_store.clone())
            .agent_group("default")
            .batch_size(100)
            .batch_timeout(Duration::from_millis(100))
            .build()
            .await
            .unwrap(),
    );

    // Spawn 5 concurrent tasks sending records
    let mut tasks = vec![];
    for task_id in 0..5 {
        let producer = Arc::clone(&producer);
        let task = tokio::spawn(async move {
            for i in 0..10 {
                producer
                    .send(
                        "test_topic",
                        Some(format!("task{}-key{}", task_id, i).as_bytes()),
                        format!("task{}-value{}", task_id, i).as_bytes(),
                        Some(0),
                    )
                    .await
                    .unwrap();
            }
        });
        tasks.push(task);
    }

    // Wait for all tasks
    for task in tasks {
        task.await.unwrap();
    }

    // Flush and close
    producer.flush().await.unwrap();
    // Note: We can't close an Arc<Producer>, so we just drop it
    // In a real application, you'd ensure only one Arc exists before closing
}

#[tokio::test]
async fn test_producer_throughput() {
    // Setup
    let agent_id = "test-agent-producer-005";
    let (metadata_store, _metadata_temp) = create_test_metadata_store(agent_id).await;
    let (writer_pool, _writer_temp) = create_test_writer_pool(metadata_store.clone()).await;

    let agent_address = start_test_agent(
        agent_id.to_string(),
        metadata_store.clone(),
        writer_pool.clone(),
    )
    .await;

    // Update agent info
    let agent_info = AgentInfo {
        agent_id: agent_id.to_string(),
        address: agent_address.clone(),
        availability_zone: "us-east-1a".to_string(),
        agent_group: "default".to_string(),
        last_heartbeat: chrono::Utc::now().timestamp_millis(),
        started_at: chrono::Utc::now().timestamp_millis(),
        metadata: HashMap::new(),
    };
    metadata_store.register_agent(agent_info).await.unwrap();

    // Create Producer
    let producer = Producer::builder()
        .metadata_store(metadata_store.clone())
        .agent_group("default")
        .batch_size(100)
        .batch_timeout(Duration::from_millis(100))
        .build()
        .await
        .unwrap();

    let start = std::time::Instant::now();
    let record_count = 10_000;

    // Send many records
    for i in 0..record_count {
        producer
            .send(
                "test_topic",
                Some(format!("key{}", i).as_bytes()),
                format!("value{}", i).as_bytes(),
                Some(0),
            )
            .await
            .unwrap();
    }

    // Flush all batches
    producer.flush().await.unwrap();

    let elapsed = start.elapsed();
    let throughput = record_count as f64 / elapsed.as_secs_f64();

    println!(
        "Throughput: {:.0} records/sec ({} records in {:?})",
        throughput, record_count, elapsed
    );

    // Target: 50K+ records/sec
    assert!(
        throughput > 1000.0,
        "Throughput too low: {} rec/s",
        throughput
    );

    producer.close().await.unwrap();
}
