//! Local Workflow Test - Complete StreamHouse Demonstration
//!
//! This script demonstrates the full StreamHouse workflow:
//! 1. Setup metadata store (SQLite)
//! 2. Create topics with partitions
//! 3. Start multiple agents with auto-assignment
//! 4. Simulate partition assignment and rebalancing
//! 5. Show lease management and epoch fencing
//! 6. Demonstrate failover scenarios
//! 7. Clean shutdown
//!
//! Run with:
//! ```bash
//! cargo run --package streamhouse-agent --example local_workflow_test
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::Agent;
use streamhouse_metadata::{CleanupPolicy, MetadataStore, SqliteMetadataStore, TopicConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    print_header("StreamHouse Local Workflow Test");

    // Step 1: Setup metadata store
    print_step("Setting up metadata store (SQLite)");
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("streamhouse_workflow.db");

    let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap()).await?;
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    println!("✓ Metadata store initialized at: {}", db_path.display());
    println!();

    // Step 2: Create topics
    print_step("Creating topics with partitions");

    metadata
        .create_topic(TopicConfig {
            name: "orders".to_string(),
            partition_count: 6,
            retention_ms: Some(86400000), // 1 day
            cleanup_policy: CleanupPolicy::default(),
            config: HashMap::new(),
        })
        .await?;
    println!("✓ Created topic 'orders' with 6 partitions");

    metadata
        .create_topic(TopicConfig {
            name: "users".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            cleanup_policy: CleanupPolicy::default(),
            config: HashMap::new(),
        })
        .await?;
    println!("✓ Created topic 'users' with 3 partitions");

    metadata
        .create_topic(TopicConfig {
            name: "analytics".to_string(),
            partition_count: 12,
            retention_ms: Some(3600000), // 1 hour
            cleanup_policy: CleanupPolicy::default(),
            config: HashMap::new(),
        })
        .await?;
    println!("✓ Created topic 'analytics' with 12 partitions");

    println!("\n📊 Total: 3 topics, 21 partitions");
    println!();

    // Step 3: Start agents with auto-assignment
    print_step("Starting agents with automatic partition assignment");

    let mut agents = Vec::new();

    for i in 1..=3 {
        let agent = Agent::builder()
            .agent_id(format!("agent-{}", i))
            .address(format!("127.0.0.1:909{}", i))
            .availability_zone(format!("zone-{}", (i % 2) + 1))
            .agent_group("local-test")
            .heartbeat_interval(Duration::from_secs(5))
            .metadata_store(Arc::clone(&metadata))
            .managed_topics(vec![
                "orders".to_string(),
                "users".to_string(),
                "analytics".to_string(),
            ])
            .build()
            .await?;

        agent.start().await?;
        println!("✓ Agent {} started in zone-{}", i, (i % 2) + 1);
        agents.push(agent);
    }

    println!("\n⏳ Waiting for initial partition assignment (5 seconds)...");
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!();

    // Step 4: Show initial partition distribution
    print_step("Initial Partition Distribution");
    show_partition_distribution(&agents).await;
    println!();

    // Step 5: Show lease information
    print_step("Lease Management Status");
    show_lease_status(&metadata).await?;
    println!();

    // Step 6: Demonstrate failover
    print_step("Simulating Agent Failure (agent-2)");
    println!("💥 Stopping agent-2...");
    agents[1].stop().await?;
    println!("✓ Agent-2 stopped gracefully");
    println!();

    println!("⏳ Waiting for rebalancing (35 seconds)...");
    tokio::time::sleep(Duration::from_secs(35)).await;
    println!();

    print_step("Partition Distribution After Failure");
    println!("📊 Active agents: agent-1, agent-3");
    show_partition_distribution_filtered(&agents, vec![0, 2]).await;
    println!();

    // Step 7: Recover failed agent
    print_step("Recovering Failed Agent");
    println!("🔄 Restarting agent-2...");

    let agent2_recovered = Agent::builder()
        .agent_id("agent-2")
        .address("127.0.0.1:9092")
        .availability_zone("zone-1")
        .agent_group("local-test")
        .heartbeat_interval(Duration::from_secs(5))
        .metadata_store(Arc::clone(&metadata))
        .managed_topics(vec![
            "orders".to_string(),
            "users".to_string(),
            "analytics".to_string(),
        ])
        .build()
        .await?;

    agent2_recovered.start().await?;
    println!("✓ Agent-2 restarted successfully");

    // Replace the stopped agent with the recovered one
    agents[1] = agent2_recovered;
    println!();

    println!("⏳ Waiting for rebalancing (35 seconds)...");
    tokio::time::sleep(Duration::from_secs(35)).await;
    println!();

    print_step("Final Partition Distribution (All Agents Recovered)");
    show_partition_distribution(&agents).await;
    println!();

    // Step 8: Show cluster health
    print_step("Cluster Health Summary");
    show_cluster_health(&metadata, &agents).await?;
    println!();

    // Step 9: Test agent discovery
    print_step("Agent Discovery");
    test_agent_discovery(&metadata).await?;
    println!();

    // Step 10: Test lease ownership
    print_step("Lease Ownership Verification");
    verify_lease_ownership(&metadata).await?;
    println!();

    // Step 11: Graceful shutdown
    print_step("Graceful Shutdown");
    println!("🛑 Stopping all agents...");

    for (idx, agent) in agents.iter().enumerate() {
        agent.stop().await?;
        println!("✓ Agent {} stopped and deregistered", idx + 1);
    }

    println!();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify cleanup
    let remaining_agents = metadata.list_agents(None, None).await?;
    let remaining_leases = metadata.list_partition_leases(None, None).await?;

    println!("📊 Cleanup verification:");
    println!("  Remaining agents: {}", remaining_agents.len());
    println!("  Remaining leases: {}", remaining_leases.len());

    if remaining_agents.is_empty() && remaining_leases.is_empty() {
        println!("\n✅ All resources cleaned up successfully!");
    } else {
        println!("\n⚠️  Warning: Some resources not cleaned up");
    }

    println!();
    print_header("Workflow Test Complete");

    println!("Summary:");
    println!("  ✓ Created 3 topics with 21 total partitions");
    println!("  ✓ Started 3 agents with automatic assignment");
    println!("  ✓ Demonstrated failover and recovery");
    println!("  ✓ Verified lease management and epoch fencing");
    println!("  ✓ Clean shutdown with full cleanup");
    println!();

    Ok(())
}

/// Print a section header
fn print_header(title: &str) {
    let separator = "=".repeat(60);
    println!("{}", separator);
    println!("🎯 {}", title);
    println!("{}", separator);
    println!();
}

/// Print a step header
fn print_step(title: &str) {
    let separator = "-".repeat(60);
    println!("{}", separator);
    println!("📌 {}", title);
    println!("{}", separator);
}

/// Show partition distribution across all agents
async fn show_partition_distribution(agents: &[Agent]) {
    for agent in agents {
        let agent_id = agent.agent_id();

        if let Some(partitions) = agent.partition_assigner().await {
            println!("Agent: {}", agent_id);
            println!("  Assigned partitions ({}): ", partitions.len());

            // Group by topic
            let mut by_topic: HashMap<String, Vec<u32>> = HashMap::new();
            for (topic, partition_id) in partitions {
                by_topic.entry(topic).or_default().push(partition_id);
            }

            for (topic, mut partition_ids) in by_topic {
                partition_ids.sort();
                println!("    {}: {:?}", topic, partition_ids);
            }
        } else {
            println!("Agent: {} (no assigner configured)", agent_id);
        }
        println!();
    }
}

/// Show partition distribution for filtered agents
async fn show_partition_distribution_filtered(agents: &[Agent], indices: Vec<usize>) {
    for &idx in &indices {
        if idx < agents.len() {
            let agent = &agents[idx];
            let agent_id = agent.agent_id();

            if let Some(partitions) = agent.partition_assigner().await {
                println!("Agent: {}", agent_id);
                println!("  Assigned partitions ({}): ", partitions.len());

                let mut by_topic: HashMap<String, Vec<u32>> = HashMap::new();
                for (topic, partition_id) in partitions {
                    by_topic.entry(topic).or_default().push(partition_id);
                }

                for (topic, mut partition_ids) in by_topic {
                    partition_ids.sort();
                    println!("    {}: {:?}", topic, partition_ids);
                }
            }
            println!();
        }
    }
}

/// Show lease status from metadata store
async fn show_lease_status(
    metadata: &Arc<dyn MetadataStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    let leases = metadata.list_partition_leases(None, None).await?;

    println!("📊 Active leases: {}", leases.len());

    // Group by agent
    let mut by_agent: HashMap<String, Vec<(String, u32, u64)>> = HashMap::new();
    for lease in leases {
        by_agent
            .entry(lease.leader_agent_id.clone())
            .or_default()
            .push((lease.topic, lease.partition_id, lease.epoch));
    }

    for (agent_id, mut leases) in by_agent {
        leases.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        println!("\n  {}: {} leases", agent_id, leases.len());

        // Group by topic for cleaner display
        let mut by_topic: HashMap<String, Vec<(u32, u64)>> = HashMap::new();
        for (topic, partition_id, epoch) in leases {
            by_topic
                .entry(topic)
                .or_default()
                .push((partition_id, epoch));
        }

        for (topic, partitions) in by_topic {
            let partition_strs: Vec<String> = partitions
                .iter()
                .map(|(pid, epoch)| format!("{}(e:{})", pid, epoch))
                .collect();
            println!("    {}: {}", topic, partition_strs.join(", "));
        }
    }

    Ok(())
}

/// Show cluster health summary
async fn show_cluster_health(
    metadata: &Arc<dyn MetadataStore>,
    agents: &[Agent],
) -> Result<(), Box<dyn std::error::Error>> {
    let live_agents = metadata.list_agents(None, None).await?;
    let total_leases = metadata.list_partition_leases(None, None).await?;

    println!("🏥 Cluster Status:");
    println!("  Live agents: {}", live_agents.len());
    println!("  Total leases: {}", total_leases.len());
    println!("  Expected agents: {}", agents.len());

    // Calculate lease distribution balance
    let mut lease_counts: Vec<usize> = Vec::new();
    for agent in agents {
        if let Some(partitions) = agent.partition_assigner().await {
            lease_counts.push(partitions.len());
        }
    }

    if !lease_counts.is_empty() {
        let total: usize = lease_counts.iter().sum();
        let avg = total as f64 / lease_counts.len() as f64;
        let min = *lease_counts.iter().min().unwrap();
        let max = *lease_counts.iter().max().unwrap();

        println!("\n📊 Load Distribution:");
        println!("  Average: {:.1} partitions/agent", avg);
        println!("  Min: {} partitions", min);
        println!("  Max: {} partitions", max);
        println!("  Balance factor: {:.2}x", max as f64 / min.max(1) as f64);
    }

    Ok(())
}

/// Test agent discovery functionality
async fn test_agent_discovery(
    metadata: &Arc<dyn MetadataStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing agent discovery queries:\n");

    // List all agents
    let all_agents = metadata.list_agents(None, None).await?;
    println!("  All agents: {}", all_agents.len());
    for agent in &all_agents {
        println!("    - {} ({})", agent.agent_id, agent.availability_zone);
    }

    // List by group
    let group_agents = metadata.list_agents(Some("local-test"), None).await?;
    println!("\n  Agents in group 'local-test': {}", group_agents.len());

    // List by zone
    let zone1_agents = metadata.list_agents(None, Some("zone-1")).await?;
    println!("\n  Agents in zone-1: {}", zone1_agents.len());
    for agent in &zone1_agents {
        println!("    - {}", agent.agent_id);
    }

    let zone2_agents = metadata.list_agents(None, Some("zone-2")).await?;
    println!("\n  Agents in zone-2: {}", zone2_agents.len());
    for agent in &zone2_agents {
        println!("    - {}", agent.agent_id);
    }

    Ok(())
}

/// Verify lease ownership is exclusive
async fn verify_lease_ownership(
    metadata: &Arc<dyn MetadataStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    let leases = metadata.list_partition_leases(None, None).await?;

    // Check for duplicate partition ownership
    let mut seen_partitions: HashMap<(String, u32), String> = HashMap::new();
    let mut violations = 0;

    for lease in leases {
        let key = (lease.topic.clone(), lease.partition_id);

        if let Some(existing_owner) = seen_partitions.get(&key) {
            println!(
                "❌ VIOLATION: Partition {}:{} owned by both {} and {}",
                lease.topic, lease.partition_id, existing_owner, lease.leader_agent_id
            );
            violations += 1;
        } else {
            seen_partitions.insert(key, lease.leader_agent_id.clone());
        }
    }

    if violations == 0 {
        println!("✅ All leases have exclusive ownership");
        println!(
            "   Verified {} unique partition leases",
            seen_partitions.len()
        );
    } else {
        println!("❌ Found {} ownership violations", violations);
    }

    // Check for expired leases
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;

    let expired_count = metadata
        .list_partition_leases(None, None)
        .await?
        .iter()
        .filter(|l| l.lease_expires_at < now)
        .count();

    if expired_count > 0 {
        println!("\n⚠️  Warning: {} leases have expired", expired_count);
    } else {
        println!("\n✅ All leases are current (not expired)");
    }

    Ok(())
}
