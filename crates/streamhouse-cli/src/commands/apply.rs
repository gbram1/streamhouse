//! Declarative apply/destroy commands
//!
//! `streamctl apply` reads streamhouse.yaml, diffs against current state, and applies changes.
//! `streamctl destroy` reads streamhouse.yaml and tears down all declared resources.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::declarative::{
    compute_destroy, compute_diff, print_plan, CurrentState, DiffOp, StreamHouseConfig,
};
use crate::rest_client::RestClient;

// --- API response types needed for fetching current state ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TopicEntry {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetEntry {
    name: String,
    #[allow(dead_code)]
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineEntry {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorEntry {
    name: String,
}

// --- API request types for creating resources ---

#[derive(Debug, Serialize)]
struct CreateTopicRequest {
    name: String,
    partitions: u32,
    replication_factor: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTargetRequest {
    name: String,
    target_type: String,
    connection_config: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatePipelineRequest {
    name: String,
    source_topic: String,
    target_id: String,
    transform_sql: Option<String>,
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
struct TargetResponse {
    id: String,
    name: String,
}

/// Fetch current state from the API
async fn fetch_current_state(client: &RestClient) -> Result<CurrentState> {
    let (topics_result, targets_result, pipelines_result, connectors_result) = tokio::join!(
        client.get::<Vec<TopicEntry>>("/api/v1/topics"),
        client.get::<Vec<TargetEntry>>("/api/v1/pipeline-targets"),
        client.get::<Vec<PipelineEntry>>("/api/v1/pipelines"),
        client.get::<Vec<ConnectorEntry>>("/api/v1/connectors"),
    );

    Ok(CurrentState {
        topics: topics_result
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.name)
            .collect(),
        targets: targets_result
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.name)
            .collect(),
        pipelines: pipelines_result
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.name)
            .collect(),
        connectors: connectors_result
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.name)
            .collect(),
    })
}

/// Handle `streamctl apply`
pub async fn handle_apply(
    file: Option<&str>,
    yes: bool,
    api_url: &str,
    api_key: Option<&str>,
    org_id: Option<&str>,
) -> Result<()> {
    let config_path = file.unwrap_or("streamhouse.yaml");
    let path = Path::new(config_path);

    let config = StreamHouseConfig::load(path)?;
    let client = RestClient::with_org(
        api_url,
        api_key.map(String::from),
        org_id.map(String::from),
    );

    let current = fetch_current_state(&client)
        .await
        .context("Failed to fetch current state")?;

    let actions = compute_diff(&config, &current);

    if actions.is_empty() {
        println!("No changes needed. Infrastructure is up to date.");
        return Ok(());
    }

    print_plan(&actions);
    println!();

    if !yes {
        let confirm = dialoguer::Confirm::new()
            .with_prompt("Apply these changes?")
            .default(false)
            .interact()
            .context("Failed to read confirmation")?;

        if !confirm {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!();

    // Execute in dependency order: topics -> targets -> connectors -> pipelines
    for action in &actions {
        if action.operation != DiffOp::Create {
            continue;
        }
        match action.resource_type {
            "topic" => {
                let topic_config = config.topics.iter().find(|t| t.name == action.name).unwrap();
                let req = CreateTopicRequest {
                    name: topic_config.name.clone(),
                    partitions: topic_config.partitions,
                    replication_factor: 1,
                };
                let _: serde_json::Value = client
                    .post("/api/v1/topics", &req)
                    .await
                    .context(format!("Failed to create topic '{}'", action.name))?;
                println!("  + topic: {}", action.name);
            }
            _ => {}
        }
    }

    for action in &actions {
        if action.operation != DiffOp::Create || action.resource_type != "target" {
            continue;
        }
        let target_config = config.targets.iter().find(|t| t.name == action.name).unwrap();
        let req = CreateTargetRequest {
            name: target_config.name.clone(),
            target_type: target_config.target_type.clone(),
            connection_config: target_config.config.clone(),
        };
        let _: serde_json::Value = client
            .post("/api/v1/pipeline-targets", &req)
            .await
            .context(format!("Failed to create target '{}'", action.name))?;
        println!("  + target: {}", action.name);
    }

    for action in &actions {
        if action.operation != DiffOp::Create || action.resource_type != "connector" {
            continue;
        }
        let conn_config = config
            .connectors
            .iter()
            .find(|c| c.name == action.name)
            .unwrap();
        let config_map = if conn_config.config.is_empty() {
            None
        } else {
            Some(conn_config.config.clone())
        };
        let req = CreateConnectorRequest {
            name: conn_config.name.clone(),
            connector_type: conn_config.connector_type.clone(),
            connector_class: conn_config.class.clone(),
            topics: conn_config.topics.clone(),
            config: config_map,
        };
        let _: serde_json::Value = client
            .post("/api/v1/connectors", &req)
            .await
            .context(format!("Failed to create connector '{}'", action.name))?;
        println!("  + connector: {}", action.name);
    }

    for action in &actions {
        if action.operation != DiffOp::Create || action.resource_type != "pipeline" {
            continue;
        }
        let pipeline_config = config
            .pipelines
            .iter()
            .find(|p| p.name == action.name)
            .unwrap();

        // Look up target ID by name
        let target_resp: TargetResponse = client
            .get(&format!(
                "/api/v1/pipeline-targets/{}",
                pipeline_config.target
            ))
            .await
            .context(format!(
                "Target '{}' not found for pipeline '{}'",
                pipeline_config.target, action.name
            ))?;

        let req = CreatePipelineRequest {
            name: pipeline_config.name.clone(),
            source_topic: pipeline_config.source.clone(),
            target_id: target_resp.id,
            transform_sql: pipeline_config.sql.clone(),
        };
        let _: serde_json::Value = client
            .post("/api/v1/pipelines", &req)
            .await
            .context(format!("Failed to create pipeline '{}'", action.name))?;
        println!("  + pipeline: {}", action.name);
    }

    println!();
    println!(
        "Apply complete. {} resource(s) created.",
        actions.iter().filter(|a| a.operation == DiffOp::Create).count()
    );

    Ok(())
}

/// Handle `streamctl destroy`
pub async fn handle_destroy(
    file: Option<&str>,
    yes: bool,
    api_url: &str,
    api_key: Option<&str>,
    org_id: Option<&str>,
) -> Result<()> {
    let config_path = file.unwrap_or("streamhouse.yaml");
    let path = Path::new(config_path);

    let config = StreamHouseConfig::load(path)?;
    let client = RestClient::with_org(
        api_url,
        api_key.map(String::from),
        org_id.map(String::from),
    );

    let current = fetch_current_state(&client)
        .await
        .context("Failed to fetch current state")?;

    let actions = compute_destroy(&config, &current);

    if actions.is_empty() {
        println!("Nothing to destroy. No matching resources found.");
        return Ok(());
    }

    print_plan(&actions);
    println!();

    if !yes {
        let confirm = dialoguer::Confirm::new()
            .with_prompt("Destroy these resources?")
            .default(false)
            .interact()
            .context("Failed to read confirmation")?;

        if !confirm {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!();

    // Execute in reverse dependency order (already ordered by compute_destroy):
    // pipelines -> connectors -> targets -> topics
    for action in &actions {
        if action.operation != DiffOp::Delete {
            continue;
        }
        let path = match action.resource_type {
            "pipeline" => format!("/api/v1/pipelines/{}", action.name),
            "connector" => format!("/api/v1/connectors/{}", action.name),
            "target" => format!("/api/v1/pipeline-targets/{}", action.name),
            "topic" => format!("/api/v1/topics/{}", action.name),
            _ => continue,
        };

        match client.delete(&path).await {
            Ok(()) => println!("  - {}: {}", action.resource_type, action.name),
            Err(e) => eprintln!(
                "  ! Failed to delete {} '{}': {}",
                action.resource_type, action.name, e
            ),
        }
    }

    println!();
    println!(
        "Destroy complete. {} resource(s) removed.",
        actions.len()
    );

    Ok(())
}
