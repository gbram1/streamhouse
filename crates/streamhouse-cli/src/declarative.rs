//! Declarative configuration types and diff engine for stm
//!
//! Supports `streamhouse.yaml` with topics, targets, pipelines, and connectors.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Top-level declarative config (streamhouse.yaml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamHouseConfig {
    #[serde(default)]
    pub topics: Vec<TopicConfig>,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
    #[serde(default)]
    pub pipelines: Vec<PipelineConfig>,
    #[serde(default)]
    pub connectors: Vec<ConnectorConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    pub name: String,
    #[serde(default = "default_partitions")]
    pub partitions: u32,
}

fn default_partitions() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub sql: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub connector_type: String,
    pub class: String,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

impl StreamHouseConfig {
    /// Load config from a YAML file
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
        let config: StreamHouseConfig = serde_yaml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("Failed to parse YAML: {}", e))?;
        Ok(config)
    }
}

/// Represents the current state fetched from the API
#[derive(Debug, Default)]
pub struct CurrentState {
    pub topics: Vec<String>,
    pub targets: Vec<String>,
    pub pipelines: Vec<String>,
    pub connectors: Vec<String>,
}

/// A single change in the diff plan
#[derive(Debug)]
pub struct DiffAction {
    pub operation: DiffOp,
    pub resource_type: &'static str,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOp {
    Create,
    Update,
    Delete,
}

impl fmt::Display for DiffOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffOp::Create => write!(f, "+"),
            DiffOp::Update => write!(f, "~"),
            DiffOp::Delete => write!(f, "-"),
        }
    }
}

/// Compute the diff between desired config and current state
pub fn compute_diff(config: &StreamHouseConfig, current: &CurrentState) -> Vec<DiffAction> {
    let mut actions = Vec::new();

    // Topics
    for topic in &config.topics {
        if current.topics.iter().any(|t| t == &topic.name) {
            // Topic exists — we don't update topics (can't change partitions via API easily)
        } else {
            actions.push(DiffAction {
                operation: DiffOp::Create,
                resource_type: "topic",
                name: topic.name.clone(),
                detail: format!("{} partitions", topic.partitions),
            });
        }
    }

    // Targets
    for target in &config.targets {
        if current.targets.iter().any(|t| t == &target.name) {
            // Target exists — could diff config but for now skip
        } else {
            actions.push(DiffAction {
                operation: DiffOp::Create,
                resource_type: "target",
                name: target.name.clone(),
                detail: format!("type: {}", target.target_type),
            });
        }
    }

    // Connectors
    for connector in &config.connectors {
        if current.connectors.iter().any(|c| c == &connector.name) {
            // Connector exists
        } else {
            actions.push(DiffAction {
                operation: DiffOp::Create,
                resource_type: "connector",
                name: connector.name.clone(),
                detail: format!("{} {}", connector.connector_type, connector.class),
            });
        }
    }

    // Pipelines
    for pipeline in &config.pipelines {
        if current.pipelines.iter().any(|p| p == &pipeline.name) {
            // Pipeline exists — could diff SQL but for now skip
        } else {
            actions.push(DiffAction {
                operation: DiffOp::Create,
                resource_type: "pipeline",
                name: pipeline.name.clone(),
                detail: format!("{} -> {}", pipeline.source, pipeline.target),
            });
        }
    }

    actions
}

/// Compute what needs to be destroyed (reverse of apply)
pub fn compute_destroy(config: &StreamHouseConfig, current: &CurrentState) -> Vec<DiffAction> {
    let mut actions = Vec::new();

    // Destroy in reverse dependency order: pipelines -> connectors -> targets -> topics
    for pipeline in &config.pipelines {
        if current.pipelines.iter().any(|p| p == &pipeline.name) {
            actions.push(DiffAction {
                operation: DiffOp::Delete,
                resource_type: "pipeline",
                name: pipeline.name.clone(),
                detail: format!("{} -> {}", pipeline.source, pipeline.target),
            });
        }
    }

    for connector in &config.connectors {
        if current.connectors.iter().any(|c| c == &connector.name) {
            actions.push(DiffAction {
                operation: DiffOp::Delete,
                resource_type: "connector",
                name: connector.name.clone(),
                detail: format!("{} {}", connector.connector_type, connector.class),
            });
        }
    }

    for target in &config.targets {
        if current.targets.iter().any(|t| t == &target.name) {
            actions.push(DiffAction {
                operation: DiffOp::Delete,
                resource_type: "target",
                name: target.name.clone(),
                detail: format!("type: {}", target.target_type),
            });
        }
    }

    for topic in &config.topics {
        if current.topics.iter().any(|t| t == &topic.name) {
            actions.push(DiffAction {
                operation: DiffOp::Delete,
                resource_type: "topic",
                name: topic.name.clone(),
                detail: format!("{} partitions", topic.partitions),
            });
        }
    }

    actions
}

/// Print the diff plan in terraform-style output
pub fn print_plan(actions: &[DiffAction]) {
    let creates = actions
        .iter()
        .filter(|a| a.operation == DiffOp::Create)
        .count();
    let updates = actions
        .iter()
        .filter(|a| a.operation == DiffOp::Update)
        .count();
    let deletes = actions
        .iter()
        .filter(|a| a.operation == DiffOp::Delete)
        .count();

    println!(
        "Plan: {} to create, {} to update, {} to destroy",
        creates, updates, deletes
    );
    println!();

    for action in actions {
        let symbol = match action.operation {
            DiffOp::Create => "+",
            DiffOp::Update => "~",
            DiffOp::Delete => "-",
        };
        println!(
            "  {} {}: {} ({})",
            symbol, action.resource_type, action.name, action.detail
        );
    }
}
