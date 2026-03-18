//! Metrics and status commands
//!
//! These commands use the REST API to query system metrics, agent status, and health.

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::Deserialize;

use crate::rest_client::RestClient;

#[derive(Subcommand)]
pub enum MetricsCommands {
    /// Get metrics overview
    Overview,
    /// Get throughput metrics
    Throughput,
    /// Get latency metrics
    Latency,
    /// Get error metrics
    Errors,
    /// Get storage metrics
    Storage,
    /// List all agents
    Agents,
    /// Get agent details
    Agent {
        /// Agent ID
        id: String,
    },
}

#[derive(Debug, Deserialize)]
struct MetricsSnapshot {
    #[serde(default)]
    topics_count: i64,
    #[serde(default)]
    agents_count: i64,
    #[serde(default)]
    partitions_count: i64,
    #[serde(default)]
    total_messages: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThroughputMetric {
    topic: String,
    #[serde(default)]
    partition: Option<u32>,
    records_per_second: f64,
    bytes_per_second: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LatencyMetric {
    operation: String,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    #[serde(default)]
    avg_ms: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorMetric {
    error_type: String,
    count: i64,
    #[serde(default)]
    last_occurred: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageMetricsResponse {
    total_bytes: i64,
    #[serde(default)]
    topics: Vec<TopicStorageMetric>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TopicStorageMetric {
    topic: String,
    size_bytes: i64,
    partition_count: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentMetricsResponse {
    id: String,
    #[serde(default)]
    hostname: Option<String>,
    state: String,
    #[serde(default)]
    topics_assigned: Vec<String>,
    #[serde(default)]
    uptime_seconds: i64,
    #[serde(default)]
    records_processed: i64,
    #[serde(default)]
    last_heartbeat: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: String,
    #[serde(default)]
    version: Option<String>,
}

/// Handle metrics commands
pub async fn handle_metrics_command(
    command: MetricsCommands,
    api_url: &str,
    api_key: Option<&str>,
    org_id: Option<&str>,
) -> Result<()> {
    let client = RestClient::with_org(api_url, api_key.map(String::from), org_id.map(String::from));

    match command {
        MetricsCommands::Overview => {
            let snapshot: MetricsSnapshot = client
                .get("/api/v1/metrics")
                .await
                .context("Failed to get metrics overview")?;

            println!("Metrics Overview:");
            println!("  Topics:           {}", snapshot.topics_count);
            println!("  Partitions:       {}", snapshot.partitions_count);
            println!("  Agents:           {}", snapshot.agents_count);
            println!("  Total messages:   {}", snapshot.total_messages);
        }
        MetricsCommands::Throughput => {
            let metrics: Vec<ThroughputMetric> = client
                .get("/api/v1/metrics/throughput")
                .await
                .context("Failed to get throughput metrics")?;

            if metrics.is_empty() {
                println!("No throughput data available");
            } else {
                println!("Throughput Metrics:");
                println!(
                    "{:<25} {:<10} {:<15} {:<15}",
                    "Topic", "Partition", "Records/s", "Bytes/s"
                );
                println!("{}", "-".repeat(65));
                for m in &metrics {
                    println!(
                        "{:<25} {:<10} {:<15.1} {:<15.1}",
                        m.topic,
                        m.partition
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "all".to_string()),
                        m.records_per_second,
                        m.bytes_per_second
                    );
                }
            }
        }
        MetricsCommands::Latency => {
            let metrics: Vec<LatencyMetric> = client
                .get("/api/v1/metrics/latency")
                .await
                .context("Failed to get latency metrics")?;

            if metrics.is_empty() {
                println!("No latency data available");
            } else {
                println!("Latency Metrics (ms):");
                println!(
                    "{:<20} {:<10} {:<10} {:<10} {:<10}",
                    "Operation", "p50", "p95", "p99", "avg"
                );
                println!("{}", "-".repeat(60));
                for m in &metrics {
                    println!(
                        "{:<20} {:<10.2} {:<10.2} {:<10.2} {:<10.2}",
                        m.operation, m.p50_ms, m.p95_ms, m.p99_ms, m.avg_ms
                    );
                }
            }
        }
        MetricsCommands::Errors => {
            let metrics: Vec<ErrorMetric> = client
                .get("/api/v1/metrics/errors")
                .await
                .context("Failed to get error metrics")?;

            if metrics.is_empty() {
                println!("No errors recorded");
            } else {
                println!("Error Metrics:");
                println!(
                    "{:<30} {:<10} {:<25}",
                    "Error Type", "Count", "Last Occurred"
                );
                println!("{}", "-".repeat(65));
                for m in &metrics {
                    println!(
                        "{:<30} {:<10} {:<25}",
                        m.error_type,
                        m.count,
                        m.last_occurred.as_deref().unwrap_or("n/a"),
                    );
                }
            }
        }
        MetricsCommands::Storage => {
            let storage: StorageMetricsResponse = client
                .get("/api/v1/metrics/storage")
                .await
                .context("Failed to get storage metrics")?;

            println!("Storage Metrics:");
            println!("  Total: {} bytes", storage.total_bytes);
            if !storage.topics.is_empty() {
                println!();
                println!(
                    "{:<25} {:<15} {:<10}",
                    "Topic", "Size (bytes)", "Partitions"
                );
                println!("{}", "-".repeat(50));
                for t in &storage.topics {
                    println!(
                        "{:<25} {:<15} {:<10}",
                        t.topic, t.size_bytes, t.partition_count
                    );
                }
            }
        }
        MetricsCommands::Agents => {
            let agents: Vec<AgentMetricsResponse> = client
                .get("/api/v1/agents")
                .await
                .context("Failed to list agents")?;

            if agents.is_empty() {
                println!("No agents found");
            } else {
                println!("Agents ({}):", agents.len());
                println!(
                    "{:<36} {:<15} {:<10} {:<15} {:<10}",
                    "ID", "Hostname", "State", "Records", "Uptime(s)"
                );
                println!("{}", "-".repeat(86));
                for a in &agents {
                    println!(
                        "{:<36} {:<15} {:<10} {:<15} {:<10}",
                        a.id,
                        a.hostname.as_deref().unwrap_or("unknown"),
                        a.state,
                        a.records_processed,
                        a.uptime_seconds,
                    );
                }
            }
        }
        MetricsCommands::Agent { id } => {
            let agent: AgentMetricsResponse = client
                .get(&format!("/api/v1/agents/{}", id))
                .await
                .context("Agent not found")?;

            println!("Agent:");
            println!("  ID:               {}", agent.id);
            if let Some(ref host) = agent.hostname {
                println!("  Hostname:         {}", host);
            }
            println!("  State:            {}", agent.state);
            println!("  Records processed: {}", agent.records_processed);
            println!("  Uptime:           {}s", agent.uptime_seconds);
            if !agent.topics_assigned.is_empty() {
                println!("  Topics:           {}", agent.topics_assigned.join(", "));
            }
            if let Some(ref hb) = agent.last_heartbeat {
                println!("  Last heartbeat:   {}", hb);
            }
        }
    }

    Ok(())
}

/// Handle the top-level health command
pub async fn handle_health_command(api_url: &str, api_key: Option<&str>) -> Result<()> {
    let client = RestClient::with_api_key(api_url, api_key.map(String::from));

    let health: HealthResponse = client
        .get("/health")
        .await
        .context("Failed to reach server")?;

    println!("Server health: {}", health.status);
    if let Some(version) = health.version {
        println!("Version: {}", version);
    }

    Ok(())
}
