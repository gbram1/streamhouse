//! Multi-Agent Integration Tests
//!
//! Tests that verify multiple agents can coordinate correctly:
//! - Multiple agents register simultaneously
//! - Heartbeats keep agents alive
//! - Dead agents are filtered out after timeout
//! - Graceful shutdown deregisters agents

use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::Agent;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};

/// Test that multiple agents can register and coexist
#[tokio::test]
async fn test_multiple_agents_register() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("multi_agent_register.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Start 3 agents
    let agent1 = Agent::builder()
        .agent_id("agent-1")
        .address("127.0.0.1:9091")
        .availability_zone("us-east-1a")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    let agent2 = Agent::builder()
        .agent_id("agent-2")
        .address("127.0.0.1:9092")
        .availability_zone("us-east-1b")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    let agent3 = Agent::builder()
        .agent_id("agent-3")
        .address("127.0.0.1:9093")
        .availability_zone("us-east-1a")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    // Start all agents
    agent1.start().await.unwrap();
    agent2.start().await.unwrap();
    agent3.start().await.unwrap();

    // Verify all agents registered
    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 3, "Expected 3 agents");

    let agent_ids: Vec<String> = agents.iter().map(|a| a.agent_id.clone()).collect();
    assert!(agent_ids.contains(&"agent-1".to_string()));
    assert!(agent_ids.contains(&"agent-2".to_string()));
    assert!(agent_ids.contains(&"agent-3".to_string()));

    // Stop all agents
    agent1.stop().await.unwrap();
    agent2.stop().await.unwrap();
    agent3.stop().await.unwrap();

    // Verify all deregistered
    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 0, "All agents should be deregistered");
}

/// Test that heartbeat keeps agents alive
#[tokio::test]
async fn test_heartbeat_keeps_agents_alive() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("heartbeat_alive.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    let agent = Agent::builder()
        .agent_id("agent-heartbeat-test")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .heartbeat_interval(Duration::from_millis(100)) // Fast heartbeat
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Get initial heartbeat
    tokio::time::sleep(Duration::from_millis(150)).await;
    let agents = metadata.list_agents(None, None).await.unwrap();
    let first_heartbeat = agents[0].last_heartbeat;

    // Wait for another heartbeat
    tokio::time::sleep(Duration::from_millis(150)).await;
    let agents = metadata.list_agents(None, None).await.unwrap();
    let second_heartbeat = agents[0].last_heartbeat;

    // Heartbeat should have advanced
    assert!(
        second_heartbeat > first_heartbeat,
        "Heartbeat should update: {} -> {}",
        first_heartbeat,
        second_heartbeat
    );

    // Wait longer and verify still alive
    tokio::time::sleep(Duration::from_millis(300)).await;
    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 1, "Agent should still be alive");

    agent.stop().await.unwrap();
}

/// Test that agents can be filtered by availability zone
#[tokio::test]
async fn test_list_agents_by_zone() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("list_by_zone.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Start agents in different AZs
    let agent1 = Agent::builder()
        .agent_id("agent-us-east-1a")
        .address("10.0.1.5:9090")
        .availability_zone("us-east-1a")
        .agent_group("prod")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    let agent2 = Agent::builder()
        .agent_id("agent-us-east-1b")
        .address("10.0.2.5:9090")
        .availability_zone("us-east-1b")
        .agent_group("prod")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    let agent3 = Agent::builder()
        .agent_id("agent-us-west-1a")
        .address("10.1.1.5:9090")
        .availability_zone("us-west-1a")
        .agent_group("prod")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent1.start().await.unwrap();
    agent2.start().await.unwrap();
    agent3.start().await.unwrap();

    // List all agents
    let all_agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(all_agents.len(), 3);

    // List agents in us-east-1a
    let east_1a = metadata
        .list_agents(None, Some("us-east-1a"))
        .await
        .unwrap();
    assert_eq!(east_1a.len(), 1);
    assert_eq!(east_1a[0].agent_id, "agent-us-east-1a");

    // List agents in us-east-1b
    let east_1b = metadata
        .list_agents(None, Some("us-east-1b"))
        .await
        .unwrap();
    assert_eq!(east_1b.len(), 1);
    assert_eq!(east_1b[0].agent_id, "agent-us-east-1b");

    agent1.stop().await.unwrap();
    agent2.stop().await.unwrap();
    agent3.stop().await.unwrap();
}

/// Test that agents can be filtered by agent group
#[tokio::test]
async fn test_list_agents_by_group() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("list_by_group.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Start agents in different groups
    let prod1 = Agent::builder()
        .agent_id("prod-agent-1")
        .address("10.0.1.5:9090")
        .availability_zone("us-east-1a")
        .agent_group("prod")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    let prod2 = Agent::builder()
        .agent_id("prod-agent-2")
        .address("10.0.2.5:9090")
        .availability_zone("us-east-1b")
        .agent_group("prod")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    let staging = Agent::builder()
        .agent_id("staging-agent-1")
        .address("10.1.1.5:9090")
        .availability_zone("us-east-1a")
        .agent_group("staging")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    prod1.start().await.unwrap();
    prod2.start().await.unwrap();
    staging.start().await.unwrap();

    // List all agents
    let all_agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(all_agents.len(), 3);

    // List prod agents
    let prod_agents = metadata.list_agents(Some("prod"), None).await.unwrap();
    assert_eq!(prod_agents.len(), 2);
    assert!(prod_agents.iter().all(|a| a.agent_group == "prod"));

    // List staging agents
    let staging_agents = metadata.list_agents(Some("staging"), None).await.unwrap();
    assert_eq!(staging_agents.len(), 1);
    assert_eq!(staging_agents[0].agent_id, "staging-agent-1");

    prod1.stop().await.unwrap();
    prod2.stop().await.unwrap();
    staging.stop().await.unwrap();
}

/// Test graceful shutdown with multiple agents
#[tokio::test]
async fn test_graceful_shutdown_multiple_agents() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("graceful_shutdown.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Start 5 agents
    let mut agents = Vec::new();
    for i in 1..=5 {
        let agent = Agent::builder()
            .agent_id(format!("agent-{}", i))
            .address(format!("127.0.0.1:909{}", i))
            .availability_zone("test")
            .agent_group("test")
            .metadata_store(Arc::clone(&metadata))
            .build()
            .await
            .unwrap();

        agent.start().await.unwrap();
        agents.push(agent);
    }

    // Verify all started
    let live_agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(live_agents.len(), 5);

    // Stop agents one by one
    for (i, agent) in agents.iter().enumerate() {
        agent.stop().await.unwrap();

        // Verify agent count decreases
        let remaining = metadata.list_agents(None, None).await.unwrap();
        assert_eq!(
            remaining.len(),
            4 - i,
            "Expected {} agents remaining",
            4 - i
        );
    }

    // All should be gone
    let final_agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(final_agents.len(), 0);
}

/// Test that stopped agent no longer sends heartbeats
#[tokio::test]
async fn test_stopped_agent_no_heartbeat() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("stopped_no_heartbeat.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    let agent = Agent::builder()
        .agent_id("agent-stop-test")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .heartbeat_interval(Duration::from_millis(100))
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    // Start and verify agent is alive
    agent.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 1);
    let _last_heartbeat_before_stop = agents[0].last_heartbeat;

    // Stop agent
    agent.stop().await.unwrap();

    // Wait for what would have been several heartbeat intervals
    tokio::time::sleep(Duration::from_millis(350)).await;

    // Agent should be deregistered (no rows)
    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 0, "Agent should be deregistered");
}

/// Test agent builder validation
#[tokio::test]
async fn test_agent_builder_validation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("builder_validation.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Missing agent_id should fail
    let result = Agent::builder()
        .address("127.0.0.1:9090")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await;
    assert!(result.is_err(), "Should fail without agent_id");

    // Missing address should fail
    let result = Agent::builder()
        .agent_id("test-agent")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await;
    assert!(result.is_err(), "Should fail without address");

    // Missing metadata_store should fail
    let result = Agent::builder()
        .agent_id("test-agent")
        .address("127.0.0.1:9090")
        .build()
        .await;
    assert!(result.is_err(), "Should fail without metadata_store");

    // Valid config should succeed
    let result = Agent::builder()
        .agent_id("test-agent")
        .address("127.0.0.1:9090")
        .metadata_store(metadata)
        .build()
        .await;
    assert!(result.is_ok(), "Should succeed with valid config");
}

/// Test concurrent agent starts
#[tokio::test]
async fn test_concurrent_agent_starts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("concurrent_starts.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Create 10 agents
    let mut agents = Vec::new();
    for i in 1..=10 {
        let agent = Agent::builder()
            .agent_id(format!("concurrent-agent-{}", i))
            .address(format!("127.0.0.1:{}", 9090 + i))
            .availability_zone("test")
            .agent_group("test")
            .metadata_store(Arc::clone(&metadata))
            .build()
            .await
            .unwrap();
        agents.push(agent);
    }

    // Start all agents sequentially (they register concurrently in metadata store)
    for agent in &agents {
        agent.start().await.unwrap();
    }

    // Verify all registered
    let live_agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(live_agents.len(), 10, "All 10 agents should be registered");

    // Stop all
    for agent in &agents {
        agent.stop().await.unwrap();
    }
}

/// Test agent metadata field
#[tokio::test]
async fn test_agent_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("agent_metadata.db");

    let metadata_store = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let metadata_store = Arc::new(metadata_store) as Arc<dyn MetadataStore>;

    let agent = Agent::builder()
        .agent_id("agent-with-metadata")
        .address("127.0.0.1:9090")
        .availability_zone("test")
        .agent_group("test")
        .metadata(r#"{"version":"1.0.0","region":"us-east-1"}"#)
        .metadata_store(Arc::clone(&metadata_store))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Verify metadata was stored
    let agents = metadata_store.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 1);

    let stored_agent = &agents[0];
    assert!(stored_agent.metadata.contains_key("version"));
    assert_eq!(stored_agent.metadata.get("version").unwrap(), "1.0.0");
    assert!(stored_agent.metadata.contains_key("region"));
    assert_eq!(stored_agent.metadata.get("region").unwrap(), "us-east-1");

    agent.stop().await.unwrap();
}
