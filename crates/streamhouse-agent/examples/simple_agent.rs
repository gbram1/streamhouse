//! Simple Agent Example
//!
//! Demonstrates starting a StreamHouse agent with heartbeat mechanism.
//!
//! Run with: cargo run --example simple_agent

use std::sync::Arc;
use std::time::Duration;
use streamhouse_agent::Agent;
use streamhouse_metadata::{MetadataStore, SqliteMetadataStore};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting StreamHouse Agent example");

    // Setup metadata store (SQLite for demo)
    let metadata = SqliteMetadataStore::new("/tmp/streamhouse_agent_example.db").await?;
    let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

    // Build agent
    let agent = Agent::builder()
        .agent_id("agent-example-1")
        .address("127.0.0.1:9090")
        .availability_zone("local")
        .agent_group("example")
        .heartbeat_interval(Duration::from_secs(5)) // Fast heartbeat for demo
        .metadata_store(metadata.clone())
        .build()
        .await?;

    info!("Agent built: {}", agent.agent_id());

    // Start agent (registers + begins heartbeat)
    agent.start().await?;

    info!("Agent started - heartbeat every 5 seconds");
    info!("Press Ctrl+C to stop");

    // List agents to verify registration
    tokio::time::sleep(Duration::from_secs(1)).await;
    let agents = metadata.list_agents(None, None).await?;
    info!("Registered agents: {:?}", agents.len());
    for a in agents {
        info!("  - {} at {}", a.agent_id, a.address);
    }

    // Run for 30 seconds
    info!("Running for 30 seconds...");
    tokio::time::sleep(Duration::from_secs(30)).await;

    // Graceful shutdown
    info!("Shutting down agent...");
    agent.stop().await?;

    // Verify deregistration
    let agents = metadata.list_agents(None, None).await?;
    info!("Agents after shutdown: {:?}", agents.len());

    info!("Agent stopped successfully");

    Ok(())
}
