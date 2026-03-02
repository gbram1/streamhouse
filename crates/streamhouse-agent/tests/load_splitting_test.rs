//! Multi-Agent Load Splitting Integration Test
//!
//! Verifies the core multi-agent partition distribution behavior:
//! 1. Partitions are distributed across multiple agents via consistent hashing
//! 2. Every partition is assigned to exactly one agent (no gaps, no overlaps)
//! 3. When an agent dies, survivors absorb its partitions
//! 4. When a new agent joins, partitions rebalance to include it
//!
//! Uses manual PartitionAssigner setup (not the Agent builder) with a fast
//! rebalance interval (500ms) to keep the test under 10 seconds.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::{LeaseManager, PartitionAssigner};
use streamhouse_metadata::{AgentInfo, CleanupPolicy, MetadataStore, SqliteMetadataStore, TopicConfig};

/// Helper: create a shared SQLite metadata store
async fn setup() -> (Arc<dyn MetadataStore>, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("load_split.db");
    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    (Arc::new(metadata) as Arc<dyn MetadataStore>, temp_dir)
}

/// Helper: create a topic with N partitions
async fn create_topic(metadata: &Arc<dyn MetadataStore>, name: &str, partitions: u32) {
    metadata
        .create_topic(TopicConfig {
            name: name.to_string(),
            partition_count: partitions,
            retention_ms: Some(86400000),
            cleanup_policy: CleanupPolicy::default(),
            config: HashMap::new(),
        })
        .await
        .unwrap();
}

/// Helper: register an agent in the metadata store (each gets a unique address)
async fn register_agent(metadata: &Arc<dyn MetadataStore>, agent_id: &str, group: &str) {
    use std::sync::atomic::{AtomicU16, Ordering};
    static PORT: AtomicU16 = AtomicU16::new(9090);
    let port = PORT.fetch_add(1, Ordering::Relaxed);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    metadata
        .register_agent(AgentInfo {
            agent_id: agent_id.to_string(),
            address: format!("127.0.0.1:{}", port),
            availability_zone: "test".to_string(),
            agent_group: group.to_string(),
            last_heartbeat: now_ms,
            started_at: now_ms,
            metadata: HashMap::new(),
        })
        .await
        .unwrap();
}

/// A lightweight test agent: lease manager + partition assigner (no heartbeat/gRPC)
struct TestAgent {
    agent_id: String,
    lease_manager: Arc<LeaseManager>,
    assigner: PartitionAssigner,
}

impl TestAgent {
    fn new(
        agent_id: &str,
        group: &str,
        metadata: &Arc<dyn MetadataStore>,
        topics: Vec<String>,
    ) -> Self {
        let lease_manager = Arc::new(LeaseManager::new(
            agent_id.to_string(),
            Arc::clone(metadata),
        ));

        let assigner = PartitionAssigner::new(
            agent_id.to_string(),
            group.to_string(),
            Arc::clone(metadata),
            Arc::clone(&lease_manager),
            topics,
        )
        .with_rebalance_interval(Duration::from_millis(500));

        Self {
            agent_id: agent_id.to_string(),
            lease_manager,
            assigner,
        }
    }

    async fn start(&self) {
        self.lease_manager.start_renewal_task().await.unwrap();
        self.assigner.start().await.unwrap();
    }

    async fn stop(&self) {
        self.assigner.stop().await.unwrap();
        self.lease_manager.stop_renewal_task().await.unwrap();
    }

    async fn assigned(&self) -> Vec<(String, u32)> {
        self.assigner.get_assigned_partitions().await
    }
}

/// Collect all assigned partitions across agents and verify correctness
fn verify_full_coverage(
    assignments: &[(&str, Vec<(String, u32)>)],
    topic: &str,
    partition_count: u32,
) {
    let mut all: Vec<(String, u32)> = Vec::new();
    for (agent_id, partitions) in assignments {
        println!("  {} → {:?}", agent_id, partitions);
        all.extend(partitions.iter().cloned());
    }

    // No duplicates
    let unique: HashSet<(String, u32)> = all.iter().cloned().collect();
    assert_eq!(
        unique.len(),
        all.len(),
        "Duplicate partition assignments detected"
    );

    // Full coverage
    assert_eq!(
        unique.len(),
        partition_count as usize,
        "Expected {} partitions assigned, got {}",
        partition_count,
        unique.len()
    );

    // All partitions accounted for
    for pid in 0..partition_count {
        assert!(
            unique.contains(&(topic.to_string(), pid)),
            "Partition {} not assigned to any agent",
            pid
        );
    }
}

/// Test 1: Three agents split 6 partitions evenly
#[tokio::test]
async fn test_three_agents_split_partitions() {
    let (metadata, _dir) = setup().await;
    let topic = "orders";
    let group = "test";
    let topics = vec![topic.to_string()];

    // Setup: 6 partitions, 3 agents
    create_topic(&metadata, topic, 6).await;
    register_agent(&metadata, "agent-1", group).await;
    register_agent(&metadata, "agent-2", group).await;
    register_agent(&metadata, "agent-3", group).await;

    // Create and start assigners (all see 3 agents on initial rebalance)
    let a1 = TestAgent::new("agent-1", group, &metadata, topics.clone());
    let a2 = TestAgent::new("agent-2", group, &metadata, topics.clone());
    let a3 = TestAgent::new("agent-3", group, &metadata, topics.clone());

    a1.start().await;
    a2.start().await;
    a3.start().await;

    // Wait for initial rebalance to settle
    tokio::time::sleep(Duration::from_secs(2)).await;

    let p1 = a1.assigned().await;
    let p2 = a2.assigned().await;
    let p3 = a3.assigned().await;

    println!("=== Three agents, 6 partitions ===");
    let assignments = vec![
        ("agent-1", p1.clone()),
        ("agent-2", p2.clone()),
        ("agent-3", p3.clone()),
    ];
    verify_full_coverage(&assignments, topic, 6);

    // Each agent should have at least 1 partition
    assert!(!p1.is_empty(), "Agent 1 should have partitions");
    assert!(!p2.is_empty(), "Agent 2 should have partitions");
    assert!(!p3.is_empty(), "Agent 3 should have partitions");

    // Cleanup
    a1.stop().await;
    a2.stop().await;
    a3.stop().await;
}

/// Test 2: Agent failure triggers rebalance — survivors absorb orphaned partitions
#[tokio::test]
async fn test_agent_failure_rebalance() {
    let (metadata, _dir) = setup().await;
    let topic = "events";
    let group = "test";
    let topics = vec![topic.to_string()];

    // Setup: 6 partitions, 3 agents
    create_topic(&metadata, topic, 6).await;
    register_agent(&metadata, "agent-a", group).await;
    register_agent(&metadata, "agent-b", group).await;
    register_agent(&metadata, "agent-c", group).await;

    let a = TestAgent::new("agent-a", group, &metadata, topics.clone());
    let b = TestAgent::new("agent-b", group, &metadata, topics.clone());
    let c = TestAgent::new("agent-c", group, &metadata, topics.clone());

    a.start().await;
    b.start().await;
    c.start().await;

    // Wait for initial assignment
    tokio::time::sleep(Duration::from_secs(2)).await;

    let pa = a.assigned().await;
    let pb = b.assigned().await;
    let pc = c.assigned().await;
    println!("=== Before failure (3 agents) ===");
    verify_full_coverage(
        &[("agent-a", pa), ("agent-b", pb), ("agent-c", pc.clone())],
        topic,
        6,
    );

    let agent_c_had = pc.len();
    println!("Agent C had {} partitions before dying", agent_c_had);

    // Kill agent-c: stop assigner (releases leases) + deregister
    c.stop().await;
    metadata.deregister_agent("agent-c").await.unwrap();

    // Wait for surviving agents to detect topology change and rebalance
    tokio::time::sleep(Duration::from_secs(3)).await;

    let pa2 = a.assigned().await;
    let pb2 = b.assigned().await;

    println!("=== After agent-c failure (2 agents) ===");
    verify_full_coverage(&[("agent-a", pa2.clone()), ("agent-b", pb2.clone())], topic, 6);

    // Both surviving agents should have partitions
    assert!(!pa2.is_empty(), "Agent A should have partitions after rebalance");
    assert!(!pb2.is_empty(), "Agent B should have partitions after rebalance");

    // Total should still be 6
    assert_eq!(pa2.len() + pb2.len(), 6);

    // Cleanup
    a.stop().await;
    b.stop().await;
}

/// Test 3: New agent joins — partitions rebalance to include it
#[tokio::test]
async fn test_new_agent_joins_rebalance() {
    let (metadata, _dir) = setup().await;
    let topic = "logs";
    let group = "test";
    let topics = vec![topic.to_string()];

    // Start with 2 agents, 6 partitions
    create_topic(&metadata, topic, 6).await;
    register_agent(&metadata, "agent-x", group).await;
    register_agent(&metadata, "agent-y", group).await;

    let x = TestAgent::new("agent-x", group, &metadata, topics.clone());
    let y = TestAgent::new("agent-y", group, &metadata, topics.clone());

    x.start().await;
    y.start().await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    let px = x.assigned().await;
    let py = y.assigned().await;
    println!("=== Before join (2 agents) ===");
    verify_full_coverage(&[("agent-x", px.clone()), ("agent-y", py.clone())], topic, 6);

    // Both should have ~3 each
    assert_eq!(px.len() + py.len(), 6);

    // New agent joins
    register_agent(&metadata, "agent-z", group).await;
    let z = TestAgent::new("agent-z", group, &metadata, topics.clone());
    z.start().await;

    // Wait for all assigners to detect topology change and rebalance
    tokio::time::sleep(Duration::from_secs(3)).await;

    let px2 = x.assigned().await;
    let py2 = y.assigned().await;
    let pz2 = z.assigned().await;

    println!("=== After agent-z joins (3 agents) ===");
    verify_full_coverage(
        &[
            ("agent-x", px2.clone()),
            ("agent-y", py2.clone()),
            ("agent-z", pz2.clone()),
        ],
        topic,
        6,
    );

    // New agent should have picked up some partitions
    assert!(
        !pz2.is_empty(),
        "New agent Z should have received partitions after rebalance"
    );

    // Cleanup
    x.stop().await;
    y.stop().await;
    z.stop().await;
}

/// Test 4: Multiple topics — partitions from all topics are distributed
#[tokio::test]
async fn test_multi_topic_distribution() {
    let (metadata, _dir) = setup().await;
    let group = "test";

    // Create 2 topics with different partition counts
    create_topic(&metadata, "orders", 4).await;
    create_topic(&metadata, "events", 3).await;

    register_agent(&metadata, "agent-1", group).await;
    register_agent(&metadata, "agent-2", group).await;

    let topics = vec!["orders".to_string(), "events".to_string()];
    let a1 = TestAgent::new("agent-1", group, &metadata, topics.clone());
    let a2 = TestAgent::new("agent-2", group, &metadata, topics.clone());

    a1.start().await;
    a2.start().await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    let p1 = a1.assigned().await;
    let p2 = a2.assigned().await;

    println!("=== Multi-topic distribution ===");
    println!("  agent-1 → {:?}", p1);
    println!("  agent-2 → {:?}", p2);

    // Total should be 4 + 3 = 7
    let total = p1.len() + p2.len();
    assert_eq!(total, 7, "All 7 partitions should be assigned, got {}", total);

    // Both agents should have partitions
    assert!(!p1.is_empty(), "Agent 1 should have partitions");
    assert!(!p2.is_empty(), "Agent 2 should have partitions");

    // No duplicates across all partitions
    let mut all: HashSet<(String, u32)> = HashSet::new();
    for p in p1.iter().chain(p2.iter()) {
        assert!(all.insert(p.clone()), "Duplicate: {:?}", p);
    }

    // Both topics fully covered
    for pid in 0..4 {
        assert!(all.contains(&("orders".to_string(), pid)));
    }
    for pid in 0..3 {
        assert!(all.contains(&("events".to_string(), pid)));
    }

    a1.stop().await;
    a2.stop().await;
}

/// Test 5: Single agent takes all partitions, then scales out
#[tokio::test]
async fn test_scale_from_one_to_three() {
    let (metadata, _dir) = setup().await;
    let topic = "metrics";
    let group = "test";
    let topics = vec![topic.to_string()];

    // Start with 1 agent
    create_topic(&metadata, topic, 6).await;
    register_agent(&metadata, "agent-solo", group).await;

    let solo = TestAgent::new("agent-solo", group, &metadata, topics.clone());
    solo.start().await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    let p_solo = solo.assigned().await;
    println!("=== Single agent ===");
    println!("  agent-solo → {:?}", p_solo);
    assert_eq!(p_solo.len(), 6, "Solo agent should own all 6 partitions");

    // Scale to 2
    register_agent(&metadata, "agent-two", group).await;
    let two = TestAgent::new("agent-two", group, &metadata, topics.clone());
    two.start().await;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let p_solo2 = solo.assigned().await;
    let p_two = two.assigned().await;
    println!("=== Scaled to 2 agents ===");
    verify_full_coverage(
        &[("agent-solo", p_solo2.clone()), ("agent-two", p_two.clone())],
        topic,
        6,
    );
    assert!(!p_two.is_empty(), "Agent two should have partitions");

    // Scale to 3
    register_agent(&metadata, "agent-three", group).await;
    let three = TestAgent::new("agent-three", group, &metadata, topics.clone());
    three.start().await;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let p_solo3 = solo.assigned().await;
    let p_two2 = two.assigned().await;
    let p_three = three.assigned().await;
    println!("=== Scaled to 3 agents ===");
    verify_full_coverage(
        &[
            ("agent-solo", p_solo3.clone()),
            ("agent-two", p_two2.clone()),
            ("agent-three", p_three.clone()),
        ],
        topic,
        6,
    );
    assert!(!p_three.is_empty(), "Agent three should have partitions");

    // Cleanup
    solo.stop().await;
    two.stop().await;
    three.stop().await;
}
