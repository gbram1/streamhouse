//! Integration tests for agent lifecycle management

use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::{Agent, AgentConfig};
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};

async fn create_metadata(name: &str) -> (Arc<dyn MetadataStore>, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join(format!("{}.db", name));
    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    (Arc::new(metadata) as Arc<dyn MetadataStore>, temp_dir)
}

#[tokio::test]
async fn test_agent_registers_on_start() {
    let (metadata, _dir) = create_metadata("register").await;

    let agent = Agent::builder()
        .agent_id("agent-register-1")
        .address("127.0.0.1:9090")
        .availability_zone("us-east-1a")
        .agent_group("test")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    // Verify agent appears in metadata store
    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent_id, "agent-register-1");
    assert_eq!(agents[0].address, "127.0.0.1:9090");
    assert_eq!(agents[0].availability_zone, "us-east-1a");
    assert_eq!(agents[0].agent_group, "test");

    agent.stop().await.unwrap();
}

#[tokio::test]
async fn test_agent_deregisters_on_stop() {
    let (metadata, _dir) = create_metadata("deregister").await;

    let agent = Agent::builder()
        .agent_id("agent-deregister-1")
        .address("127.0.0.1:9091")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();
    assert!(!metadata.list_agents(None, None).await.unwrap().is_empty());

    agent.stop().await.unwrap();

    // Agent should be removed from metadata
    let agents = metadata.list_agents(None, None).await.unwrap();
    assert!(agents.is_empty());
}

#[tokio::test]
async fn test_heartbeat_updates_timestamp() {
    let (metadata, _dir) = create_metadata("heartbeat").await;

    let agent = Agent::builder()
        .agent_id("agent-hb-1")
        .address("127.0.0.1:9092")
        .heartbeat_interval(Duration::from_millis(100))
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent.start().await.unwrap();

    let agents_before = metadata.list_agents(None, None).await.unwrap();
    let hb_before = agents_before[0].last_heartbeat;

    // Wait for at least one heartbeat cycle
    tokio::time::sleep(Duration::from_millis(250)).await;

    let agents_after = metadata.list_agents(None, None).await.unwrap();
    let hb_after = agents_after[0].last_heartbeat;

    // Heartbeat timestamp should have advanced
    assert!(hb_after > hb_before, "Heartbeat should update timestamp");

    agent.stop().await.unwrap();
}

#[tokio::test]
async fn test_agent_getters() {
    let (metadata, _dir) = create_metadata("getters").await;

    let agent = Agent::builder()
        .agent_id("agent-getters")
        .address("10.0.0.5:9090")
        .availability_zone("eu-west-1b")
        .agent_group("staging")
        .metadata_store(metadata)
        .build()
        .await
        .unwrap();

    assert_eq!(agent.agent_id(), "agent-getters");
    assert_eq!(agent.address(), "10.0.0.5:9090");
    assert_eq!(agent.availability_zone(), "eu-west-1b");
    assert_eq!(agent.agent_group(), "staging");
    assert!(!agent.is_started().await);
}

#[tokio::test]
async fn test_multiple_agents_same_store() {
    let (metadata, _dir) = create_metadata("multi").await;

    let agent1 = Agent::builder()
        .agent_id("agent-multi-1")
        .address("127.0.0.1:9001")
        .agent_group("prod")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    let agent2 = Agent::builder()
        .agent_id("agent-multi-2")
        .address("127.0.0.1:9002")
        .agent_group("prod")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    agent1.start().await.unwrap();
    agent2.start().await.unwrap();

    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 2);

    agent1.stop().await.unwrap();

    let agents = metadata.list_agents(None, None).await.unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent_id, "agent-multi-2");

    agent2.stop().await.unwrap();
}

// ============================================================================
// New integration tests
// ============================================================================

#[tokio::test]
async fn test_builder_requires_all_mandatory_fields() {
    let (metadata, _dir) = create_metadata("builder_validation").await;

    // Missing agent_id
    let res = Agent::builder()
        .address("127.0.0.1:9090")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await;
    assert!(res.is_err(), "should fail without agent_id");

    // Missing address
    let res = Agent::builder()
        .agent_id("agent-1")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await;
    assert!(res.is_err(), "should fail without address");

    // Missing metadata_store
    let res = Agent::builder()
        .agent_id("agent-1")
        .address("127.0.0.1:9090")
        .build()
        .await;
    assert!(res.is_err(), "should fail without metadata_store");

    // All fields present -- should succeed
    let res = Agent::builder()
        .agent_id("agent-1")
        .address("127.0.0.1:9090")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await;
    assert!(res.is_ok(), "should succeed with all mandatory fields");
}

#[test]
fn test_agent_config_defaults() {
    let config = AgentConfig::default();
    assert_eq!(config.agent_id, "");
    assert_eq!(config.address, "");
    assert_eq!(config.availability_zone, "default");
    assert_eq!(config.agent_group, "default");
    assert_eq!(config.heartbeat_interval, Duration::from_secs(20));
    assert_eq!(config.heartbeat_timeout, Duration::from_secs(60));
    assert!(config.metadata.is_none());
    assert!(config.managed_topics.is_empty());
}

#[tokio::test]
async fn test_config_with_all_fields_set() {
    let (metadata, _dir) = create_metadata("all_fields").await;

    let agent = Agent::builder()
        .agent_id("full-agent")
        .address("10.0.0.1:8080")
        .availability_zone("ap-southeast-1a")
        .agent_group("canary")
        .heartbeat_interval(Duration::from_secs(5))
        .heartbeat_timeout(Duration::from_secs(15))
        .metadata(r#"{"version":"4.2","env":"test"}"#)
        .managed_topics(vec!["orders".to_string(), "events".to_string()])
        .metadata_store(metadata)
        .build()
        .await
        .unwrap();

    assert_eq!(agent.agent_id(), "full-agent");
    assert_eq!(agent.address(), "10.0.0.1:8080");
    assert_eq!(agent.availability_zone(), "ap-southeast-1a");
    assert_eq!(agent.agent_group(), "canary");
    assert!(!agent.is_started().await);
}

#[tokio::test]
async fn test_agent_group_and_availability_zone_accessors() {
    let (metadata, _dir) = create_metadata("accessors").await;

    // With defaults
    let agent_default = Agent::builder()
        .agent_id("agent-default-az")
        .address("127.0.0.1:9090")
        .metadata_store(Arc::clone(&metadata))
        .build()
        .await
        .unwrap();

    assert_eq!(agent_default.availability_zone(), "default");
    assert_eq!(agent_default.agent_group(), "default");

    // With custom values
    let agent_custom = Agent::builder()
        .agent_id("agent-custom-az")
        .address("127.0.0.1:9091")
        .availability_zone("eu-central-1b")
        .agent_group("gpu-workers")
        .metadata_store(metadata)
        .build()
        .await
        .unwrap();

    assert_eq!(agent_custom.availability_zone(), "eu-central-1b");
    assert_eq!(agent_custom.agent_group(), "gpu-workers");
}
