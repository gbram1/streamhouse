//! Status dashboard command
//!
//! `streamctl status` shows a single-pane overview of the StreamHouse cluster.

use anyhow::Result;
use serde::Deserialize;

use crate::rest_client::RestClient;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: String,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TopicEntry {
    name: String,
    partitions: u32,
    #[serde(default)]
    message_count: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineEntry {
    name: String,
    source_topic: String,
    target_id: String,
    state: String,
    #[serde(default)]
    records_processed: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorEntry {
    name: String,
    connector_type: String,
    #[serde(default)]
    connector_class: String,
    state: String,
    #[serde(default)]
    topics: Vec<String>,
}

pub async fn handle_status(
    api_url: &str,
    api_key: Option<&str>,
    org_id: Option<&str>,
) -> Result<()> {
    let client = RestClient::with_org(api_url, api_key.map(String::from), org_id.map(String::from));

    // Fire all requests in parallel
    let (health_result, topics_result, pipelines_result, connectors_result) = tokio::join!(
        client.get::<HealthResponse>("/health"),
        client.get::<Vec<TopicEntry>>("/api/v1/topics"),
        client.get::<Vec<PipelineEntry>>("/api/v1/pipelines"),
        client.get::<Vec<ConnectorEntry>>("/api/v1/connectors"),
    );

    // Header
    println!("StreamHouse Status");

    // Server info
    match health_result {
        Ok(health) => {
            let version_str = health
                .version
                .as_deref()
                .map(|v| format!(", {}", v))
                .unwrap_or_default();
            println!("  Server:  {} ({}{})", api_url, health.status, version_str);
        }
        Err(e) => {
            println!("  Server:  {} (unreachable: {})", api_url, e);
        }
    }

    if let Some(org) = org_id {
        println!("  Org:     {}", org);
    }
    println!();

    // Topics
    match topics_result {
        Ok(topics) => {
            if topics.is_empty() {
                println!("Topics (0)");
            } else {
                println!("Topics ({})", topics.len());
                for t in &topics {
                    println!(
                        "  {:<25} {} partitions   {:>8} messages",
                        t.name, t.partitions, t.message_count
                    );
                }
            }
        }
        Err(e) => println!("Topics: error fetching ({})", e),
    }
    println!();

    // Pipelines
    match pipelines_result {
        Ok(pipelines) => {
            if pipelines.is_empty() {
                println!("Pipelines (0)");
            } else {
                println!("Pipelines ({})", pipelines.len());
                for p in &pipelines {
                    println!(
                        "  {:<25} {:<10} {} -> {}   {} records",
                        p.name, p.state, p.source_topic, p.target_id, p.records_processed
                    );
                }
            }
        }
        Err(e) => println!("Pipelines: error fetching ({})", e),
    }
    println!();

    // Connectors
    match connectors_result {
        Ok(connectors) => {
            if connectors.is_empty() {
                println!("Connectors (0)");
            } else {
                println!("Connectors ({})", connectors.len());
                for c in &connectors {
                    println!(
                        "  {:<25} {:<10} {} {} -> {}",
                        c.name,
                        c.state,
                        c.connector_type,
                        c.connector_class,
                        c.topics.join(", ")
                    );
                }
            }
        }
        Err(e) => println!("Connectors: error fetching ({})", e),
    }

    Ok(())
}
