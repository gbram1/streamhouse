//! Connector management commands
//!
//! These commands use the REST API to manage connectors:
//! - Create, list, get, delete, pause, resume connectors

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::rest_client::RestClient;

#[derive(Subcommand)]
pub enum ConnectorCommands {
    /// Create a new connector
    Create {
        /// Connector name
        name: String,
        /// Connector type (source or sink)
        #[arg(long, alias = "type")]
        connector_type: String,
        /// Connector class (e.g. kafka, postgres, s3)
        #[arg(long, alias = "class")]
        connector_class: String,
        /// Topics (comma-separated)
        #[arg(long)]
        topics: String,
        /// Configuration as JSON string
        #[arg(long)]
        config: Option<String>,
    },
    /// List all connectors
    List,
    /// Get connector details
    Get {
        /// Connector name
        name: String,
    },
    /// Delete a connector
    Delete {
        /// Connector name
        name: String,
    },
    /// Pause a connector
    Pause {
        /// Connector name
        name: String,
    },
    /// Resume a connector
    Resume {
        /// Connector name
        name: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateConnectorRequest {
    name: String,
    connector_type: String,
    connector_class: String,
    topics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorResponse {
    name: String,
    connector_type: String,
    connector_class: String,
    topics: Vec<String>,
    state: String,
    #[serde(default)]
    config: HashMap<String, serde_json::Value>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

/// Handle connector commands
pub async fn handle_connector_command(
    command: ConnectorCommands,
    api_url: &str,
    api_key: Option<&str>,
) -> Result<()> {
    let client = RestClient::with_api_key(api_url, api_key.map(String::from));

    match command {
        ConnectorCommands::Create {
            name,
            connector_type,
            connector_class,
            topics,
            config,
        } => {
            let config_map: Option<HashMap<String, serde_json::Value>> = match config {
                Some(json_str) => {
                    Some(serde_json::from_str(&json_str).context("Invalid config JSON")?)
                }
                None => None,
            };

            let req = CreateConnectorRequest {
                name: name.clone(),
                connector_type,
                connector_class,
                topics: topics.split(',').map(|s| s.trim().to_string()).collect(),
                config: config_map,
            };

            let resp: ConnectorResponse = client
                .post("/api/v1/connectors", &req)
                .await
                .context("Failed to create connector")?;

            println!("Connector created:");
            print_connector(&resp);
        }
        ConnectorCommands::List => {
            let connectors: Vec<ConnectorResponse> = client
                .get("/api/v1/connectors")
                .await
                .context("Failed to list connectors")?;

            if connectors.is_empty() {
                println!("No connectors found");
            } else {
                println!("Connectors ({}):", connectors.len());
                println!(
                    "{:<25} {:<10} {:<15} {:<10} Topics",
                    "Name", "Type", "Class", "State"
                );
                println!("{}", "-".repeat(80));
                for c in &connectors {
                    println!(
                        "{:<25} {:<10} {:<15} {:<10} {}",
                        c.name,
                        c.connector_type,
                        c.connector_class,
                        c.state,
                        c.topics.join(", ")
                    );
                }
            }
        }
        ConnectorCommands::Get { name } => {
            let resp: ConnectorResponse = client
                .get(&format!("/api/v1/connectors/{}", name))
                .await
                .context("Connector not found")?;
            print_connector(&resp);
        }
        ConnectorCommands::Delete { name } => {
            client
                .delete(&format!("/api/v1/connectors/{}", name))
                .await
                .context("Failed to delete connector")?;
            println!("Connector '{}' deleted", name);
        }
        ConnectorCommands::Pause { name } => {
            let resp: ConnectorResponse = client
                .put(&format!("/api/v1/connectors/{}/pause", name), &serde_json::json!({}))
                .await
                .context("Failed to pause connector")?;
            println!("Connector '{}' paused (state: {})", resp.name, resp.state);
        }
        ConnectorCommands::Resume { name } => {
            let resp: ConnectorResponse = client
                .put(&format!("/api/v1/connectors/{}/resume", name), &serde_json::json!({}))
                .await
                .context("Failed to resume connector")?;
            println!("Connector '{}' resumed (state: {})", resp.name, resp.state);
        }
    }

    Ok(())
}

fn print_connector(c: &ConnectorResponse) {
    println!("  Name:   {}", c.name);
    println!("  Type:   {}", c.connector_type);
    println!("  Class:  {}", c.connector_class);
    println!("  State:  {}", c.state);
    println!("  Topics: {}", c.topics.join(", "));
    if !c.config.is_empty() {
        println!("  Config:");
        for (k, v) in &c.config {
            println!("    {}: {}", k, v);
        }
    }
    if let Some(ref created) = c.created_at {
        println!("  Created: {}", created);
    }
}
