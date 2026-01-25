//! Lease Coordination Integration Tests
//!
//! Tests that verify partition lease management works correctly:
//! - Single agent can acquire and renew leases
//! - Multiple agents compete for same partition
//! - Epoch fencing prevents split-brain
//! - Lease expiration and failover
//! - Graceful lease release

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::{validate_epoch, Agent, AgentError};
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};

/// Helper function to create a test topic
async fn create_test_topic(metadata: &Arc<dyn MetadataStore>, topic: &str, partition_count: u32) {
    metadata
        .create_topic(TopicConfig {
            name: topic.to_string(),
            partition_count,
            retention_ms: Some(86400000), // 1 day
            config: HashMap::new(),
        })
        .await
        .unwrap();
}

/// Test that a single agent can acquire a lease
#[tokio::test]
async fn test_single_agent_acquires_lease() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("single_lease.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic
    create_test_topic(&metadata, "orders", 3).await;

    let agent = Agent::builder()
        .agent_id("agent-1")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Acquire lease for partition
    let epoch = agent
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    assert_eq!(epoch, 1, "First lease should have epoch 1");

    // Verify lease is cached
    let cached_epoch = agent.lease_manager().get_epoch("orders", 0).await;
    assert_eq!(cached_epoch, Some(1));

    // Verify lease in metadata store
    let lease = metadata
        .get_partition_lease("orders", 0)
        .await
        .unwrap()
        .expect("Lease should exist");

    assert_eq!(lease.topic, "orders");
    assert_eq!(lease.partition_id, 0);
    assert_eq!(lease.leader_agent_id, "agent-1");
    assert_eq!(lease.epoch, 1);

    agent.stop().await.unwrap();

    // Verify lease was released
    let lease_after_stop = metadata.get_partition_lease("orders", 0).await.unwrap();
    assert!(lease_after_stop.is_none(), "Lease should be released");
}

/// Test that lease is renewed automatically
#[tokio::test]
async fn test_lease_renewal() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("lease_renewal.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic
    create_test_topic(&metadata, "orders", 3).await;

    let agent = Agent::builder()
        .agent_id("agent-renewal")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Acquire lease
    let epoch = agent
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    assert_eq!(epoch, 1);

    // Get initial expiration time
    let lease1 = metadata
        .get_partition_lease("orders", 0)
        .await
        .unwrap()
        .unwrap();
    let first_expires_at = lease1.lease_expires_at;

    // Wait for renewal (renewal interval is 10s, but we can trigger it manually)
    tokio::time::sleep(Duration::from_millis(11_000)).await;

    // Check if lease was renewed (expiration extended)
    let lease2 = metadata
        .get_partition_lease("orders", 0)
        .await
        .unwrap()
        .unwrap();
    let second_expires_at = lease2.lease_expires_at;

    assert!(
        second_expires_at > first_expires_at,
        "Lease should be renewed: {} -> {}",
        first_expires_at,
        second_expires_at
    );

    agent.stop().await.unwrap();
}

/// Test that two agents cannot hold the same lease
#[tokio::test]
async fn test_two_agents_compete_for_lease() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("lease_competition.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic
    create_test_topic(&metadata, "orders", 3).await;

    // Start first agent and acquire lease
    let agent1 = Agent::builder()
        .agent_id("agent-1")
        .address("127.0.0.1:9091")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent1.start().await.unwrap();

    let epoch1 = agent1
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    assert_eq!(epoch1, 1);

    // Start second agent and try to acquire same lease
    let agent2 = Agent::builder()
        .agent_id("agent-2")
        .address("127.0.0.1:9092")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent2.start().await.unwrap();

    // Agent 2 should fail to acquire lease held by agent 1
    let result = agent2.lease_manager().ensure_lease("orders", 0).await;

    assert!(
        result.is_err(),
        "Agent 2 should not be able to acquire lease held by agent 1"
    );

    // Verify lease still held by agent 1
    let lease = metadata
        .get_partition_lease("orders", 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease.leader_agent_id, "agent-1");
    assert_eq!(lease.epoch, 1);

    agent1.stop().await.unwrap();
    agent2.stop().await.unwrap();
}

/// Test lease failover when agent stops
#[tokio::test]
async fn test_lease_failover_after_agent_stop() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("lease_failover.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic
    create_test_topic(&metadata, "orders", 3).await;

    // Agent 1 acquires lease
    let agent1 = Agent::builder()
        .agent_id("agent-1")
        .address("127.0.0.1:9091")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent1.start().await.unwrap();

    let epoch1 = agent1
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    assert_eq!(epoch1, 1);

    // Agent 1 stops (releases lease gracefully)
    agent1.stop().await.unwrap();

    // Verify lease is released
    let lease_after_stop = metadata.get_partition_lease("orders", 0).await.unwrap();
    assert!(lease_after_stop.is_none());

    // Agent 2 can now acquire lease
    let agent2 = Agent::builder()
        .agent_id("agent-2")
        .address("127.0.0.1:9092")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent2.start().await.unwrap();

    let epoch2 = agent2
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    // New lease should have been acquired
    assert!(epoch2 > 0, "Agent 2 should acquire lease");

    // Verify lease now held by agent 2
    let lease = metadata
        .get_partition_lease("orders", 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease.leader_agent_id, "agent-2");
    assert_eq!(lease.epoch, epoch2);

    agent2.stop().await.unwrap();
}

/// Test epoch validation prevents stale writes
#[tokio::test]
async fn test_epoch_fencing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("epoch_fencing.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic
    create_test_topic(&metadata, "orders", 3).await;

    let agent = Agent::builder()
        .agent_id("agent-1")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Acquire lease
    let epoch = agent
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    assert_eq!(epoch, 1);

    // Validate with correct epoch - should succeed
    let result = validate_epoch(agent.lease_manager(), "orders", 0, epoch).await;
    assert!(
        result.is_ok(),
        "Validation should succeed with correct epoch"
    );

    // Validate with wrong epoch - should fail
    let result = validate_epoch(agent.lease_manager(), "orders", 0, 999).await;
    assert!(
        matches!(result, Err(AgentError::StaleEpoch { .. })),
        "Validation should fail with wrong epoch"
    );

    agent.stop().await.unwrap();

    // After stop, validation should fail (lease released)
    let result = validate_epoch(agent.lease_manager(), "orders", 0, epoch).await;
    assert!(
        matches!(result, Err(AgentError::LeaseExpired { .. })),
        "Validation should fail after lease released"
    );
}

/// Test agent can hold multiple partition leases
#[tokio::test]
async fn test_multiple_partition_leases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("multiple_leases.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic
    create_test_topic(&metadata, "orders", 3).await;

    let agent = Agent::builder()
        .agent_id("agent-multi")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Acquire leases for multiple partitions
    let epoch0 = agent
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();
    let epoch1 = agent
        .lease_manager()
        .ensure_lease("orders", 1)
        .await
        .unwrap();
    let epoch2 = agent
        .lease_manager()
        .ensure_lease("orders", 2)
        .await
        .unwrap();

    assert_eq!(epoch0, 1);
    assert_eq!(epoch1, 1);
    assert_eq!(epoch2, 1);

    // Verify all leases in metadata store
    let leases = metadata
        .list_partition_leases(Some("orders"), Some("agent-multi"))
        .await
        .unwrap();

    assert_eq!(leases.len(), 3, "Should have 3 leases");

    // Verify active leases
    let active = agent.lease_manager().get_active_leases().await;
    assert_eq!(active.len(), 3);

    agent.stop().await.unwrap();

    // Verify all leases released
    let leases_after = metadata
        .list_partition_leases(Some("orders"), Some("agent-multi"))
        .await
        .unwrap();

    assert_eq!(leases_after.len(), 0, "All leases should be released");
}

/// Test lease cache works correctly
#[tokio::test]
async fn test_lease_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("lease_cache.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic
    create_test_topic(&metadata, "orders", 3).await;

    let agent = Agent::builder()
        .agent_id("agent-cache")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // First ensure_lease should hit metadata store
    let epoch1 = agent
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    assert_eq!(epoch1, 1);

    // Second ensure_lease should use cache (same epoch)
    let epoch2 = agent
        .lease_manager()
        .ensure_lease("orders", 0)
        .await
        .unwrap();

    assert_eq!(epoch2, 1, "Should return cached epoch");

    // get_epoch should return cached value
    let cached = agent.lease_manager().get_epoch("orders", 0).await;
    assert_eq!(cached, Some(1));

    agent.stop().await.unwrap();

    // After stop, cache should be cleared
    let cached_after = agent.lease_manager().get_epoch("orders", 0).await;
    assert_eq!(cached_after, None);
}

/// Test graceful shutdown releases all leases
#[tokio::test]
async fn test_graceful_shutdown_releases_all_leases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("graceful_shutdown.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create test topic with 5 partitions
    create_test_topic(&metadata, "orders", 5).await;

    let agent = Agent::builder()
        .agent_id("agent-shutdown")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Acquire multiple leases
    for partition_id in 0..5 {
        agent
            .lease_manager()
            .ensure_lease("orders", partition_id)
            .await
            .unwrap();
    }

    // Verify leases acquired
    let leases = metadata
        .list_partition_leases(Some("orders"), Some("agent-shutdown"))
        .await
        .unwrap();
    assert_eq!(leases.len(), 5);

    // Graceful stop should release all
    agent.stop().await.unwrap();

    // Verify all released
    let leases_after = metadata
        .list_partition_leases(Some("orders"), Some("agent-shutdown"))
        .await
        .unwrap();
    assert_eq!(leases_after.len(), 0);
}
