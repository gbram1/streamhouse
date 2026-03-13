//! Pipeline management commands
//!
//! These commands use the REST API to manage pipeline targets and pipelines.

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::rest_client::RestClient;

#[derive(Subcommand)]
pub enum PipelineCommands {
    /// Pipeline target management
    Target {
        #[command(subcommand)]
        command: TargetCommands,
    },
    /// Create a new pipeline
    Create {
        /// Pipeline name
        name: String,
        /// Source topic to consume from
        #[arg(long)]
        source: String,
        /// Target name to sink to
        #[arg(long)]
        target: String,
        /// Optional SQL transform (e.g. "SELECT id, name FROM records WHERE status = 'active'")
        #[arg(long)]
        transform: Option<String>,
    },
    /// List all pipelines
    List,
    /// Get pipeline details
    Get {
        /// Pipeline name
        name: String,
    },
    /// Start a pipeline
    Start {
        /// Pipeline name
        name: String,
    },
    /// Stop a pipeline
    Stop {
        /// Pipeline name
        name: String,
    },
    /// Delete a pipeline
    Delete {
        /// Pipeline name
        name: String,
    },
    /// Validate a SQL transform
    Validate {
        /// SQL query to validate
        sql: String,
    },
}

#[derive(Subcommand)]
pub enum TargetCommands {
    /// Create a new pipeline target
    Create {
        /// Target name
        name: String,
        /// Target type (postgres, clickhouse, elasticsearch, s3)
        #[arg(long, alias = "type")]
        target_type: String,
        /// Connection URL
        #[arg(long)]
        url: String,
        /// Table name (for postgres/clickhouse)
        #[arg(long)]
        table: Option<String>,
        /// Database name (for clickhouse)
        #[arg(long)]
        database: Option<String>,
        /// Insert mode: insert or upsert (for postgres)
        #[arg(long, default_value = "insert")]
        insert_mode: String,
    },
    /// List all pipeline targets
    List,
    /// Get pipeline target details
    Get {
        /// Target name
        name: String,
    },
    /// Delete a pipeline target
    Delete {
        /// Target name
        name: String,
    },
}

// API request/response types

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTargetRequest {
    name: String,
    target_type: String,
    connection_config: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetResponse {
    id: String,
    name: String,
    target_type: String,
    connection_config: HashMap<String, String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatePipelineRequest {
    name: String,
    source_topic: String,
    target_id: String,
    transform_sql: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineResponse {
    id: String,
    name: String,
    source_topic: String,
    consumer_group: String,
    target_id: String,
    transform_sql: Option<String>,
    state: String,
    error_message: Option<String>,
    records_processed: i64,
    last_offset: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePipelineRequest {
    state: Option<String>,
    error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidateTransformRequest {
    sql: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateTransformResponse {
    valid: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    warnings: Vec<String>,
}

pub async fn handle_pipeline_command(command: PipelineCommands, api_url: &str) -> Result<()> {
    let client = RestClient::new(api_url);

    match command {
        PipelineCommands::Target { command } => handle_target_command(command, &client).await,
        PipelineCommands::Create {
            name,
            source,
            target,
            transform,
        } => {
            // Look up target by name to get its ID
            let target_resp: TargetResponse = client
                .get(&format!("/api/v1/pipeline-targets/{}", target))
                .await
                .context(format!("Target '{}' not found", target))?;

            let req = CreatePipelineRequest {
                name: name.clone(),
                source_topic: source,
                target_id: target_resp.id,
                transform_sql: transform,
            };

            let resp: PipelineResponse = client
                .post("/api/v1/pipelines", &req)
                .await
                .context("Failed to create pipeline")?;

            println!("Pipeline created:");
            print_pipeline(&resp);
            Ok(())
        }
        PipelineCommands::List => {
            let pipelines: Vec<PipelineResponse> = client
                .get("/api/v1/pipelines")
                .await
                .context("Failed to list pipelines")?;

            if pipelines.is_empty() {
                println!("No pipelines found");
            } else {
                println!("Pipelines ({}):", pipelines.len());
                for p in &pipelines {
                    println!(
                        "  {} [{}] {} -> target:{} ({} records)",
                        p.name, p.state, p.source_topic, p.target_id, p.records_processed
                    );
                }
            }
            Ok(())
        }
        PipelineCommands::Get { name } => {
            let resp: PipelineResponse = client
                .get(&format!("/api/v1/pipelines/{}", name))
                .await
                .context("Pipeline not found")?;
            print_pipeline(&resp);
            Ok(())
        }
        PipelineCommands::Start { name } => {
            let req = UpdatePipelineRequest {
                state: Some("running".to_string()),
                error_message: None,
            };
            let resp: PipelineResponse = client
                .patch(&format!("/api/v1/pipelines/{}", name), &req)
                .await
                .context("Failed to start pipeline")?;
            println!("Pipeline '{}' started (state: {})", resp.name, resp.state);
            Ok(())
        }
        PipelineCommands::Stop { name } => {
            let req = UpdatePipelineRequest {
                state: Some("stopped".to_string()),
                error_message: None,
            };
            let resp: PipelineResponse = client
                .patch(&format!("/api/v1/pipelines/{}", name), &req)
                .await
                .context("Failed to stop pipeline")?;
            println!("Pipeline '{}' stopped (state: {})", resp.name, resp.state);
            Ok(())
        }
        PipelineCommands::Delete { name } => {
            client
                .delete(&format!("/api/v1/pipelines/{}", name))
                .await
                .context("Failed to delete pipeline")?;
            println!("Pipeline '{}' deleted", name);
            Ok(())
        }
        PipelineCommands::Validate { sql } => {
            let req = ValidateTransformRequest { sql: sql.clone() };
            let resp: ValidateTransformResponse = client
                .post("/api/v1/transforms/validate", &req)
                .await
                .context("Failed to validate transform")?;

            if resp.valid {
                println!("Transform SQL is valid");
            } else {
                println!("Transform SQL is invalid");
                if let Some(error) = resp.error {
                    println!("  Error: {}", error);
                }
            }
            for warning in &resp.warnings {
                println!("  Warning: {}", warning);
            }
            Ok(())
        }
    }
}

async fn handle_target_command(command: TargetCommands, client: &RestClient) -> Result<()> {
    match command {
        TargetCommands::Create {
            name,
            target_type,
            url,
            table,
            database,
            insert_mode,
        } => {
            let mut config = HashMap::new();
            config.insert("connection_url".to_string(), url);

            match target_type.as_str() {
                "postgres" => {
                    if let Some(t) = table {
                        config.insert("table_name".to_string(), t);
                    }
                    config.insert("insert_mode".to_string(), insert_mode);
                }
                "clickhouse" => {
                    if let Some(t) = table {
                        config.insert("table".to_string(), t);
                    }
                    if let Some(db) = database {
                        config.insert("database".to_string(), db);
                    }
                }
                "elasticsearch" => {
                    if let Some(t) = table {
                        config.insert("index".to_string(), t);
                    }
                }
                "s3" => {
                    if let Some(t) = table {
                        config.insert("prefix".to_string(), t);
                    }
                }
                _ => {}
            }

            let req = CreateTargetRequest {
                name: name.clone(),
                target_type,
                connection_config: config,
            };

            let resp: TargetResponse = client
                .post("/api/v1/pipeline-targets", &req)
                .await
                .context("Failed to create target")?;

            println!("Target created:");
            print_target(&resp);
        }
        TargetCommands::List => {
            let targets: Vec<TargetResponse> = client
                .get("/api/v1/pipeline-targets")
                .await
                .context("Failed to list targets")?;

            if targets.is_empty() {
                println!("No pipeline targets found");
            } else {
                println!("Pipeline targets ({}):", targets.len());
                for t in &targets {
                    println!("  {} [{}] {}", t.name, t.target_type, t.connection_config.get("connection_url").unwrap_or(&"".to_string()));
                }
            }
        }
        TargetCommands::Get { name } => {
            let resp: TargetResponse = client
                .get(&format!("/api/v1/pipeline-targets/{}", name))
                .await
                .context("Target not found")?;
            print_target(&resp);
        }
        TargetCommands::Delete { name } => {
            client
                .delete(&format!("/api/v1/pipeline-targets/{}", name))
                .await
                .context("Failed to delete target")?;
            println!("Target '{}' deleted", name);
        }
    }
    Ok(())
}

fn print_target(t: &TargetResponse) {
    println!("  Name:    {}", t.name);
    println!("  ID:      {}", t.id);
    println!("  Type:    {}", t.target_type);
    println!("  Config:");
    for (k, v) in &t.connection_config {
        println!("    {}: {}", k, v);
    }
    println!("  Created: {}", t.created_at);
}

fn print_pipeline(p: &PipelineResponse) {
    println!("  Name:       {}", p.name);
    println!("  ID:         {}", p.id);
    println!("  Source:      {}", p.source_topic);
    println!("  Group:       {}", p.consumer_group);
    println!("  Target:      {}", p.target_id);
    println!("  State:       {}", p.state);
    if let Some(ref sql) = p.transform_sql {
        println!("  Transform:   {}", sql);
    }
    if let Some(ref err) = p.error_message {
        println!("  Error:       {}", err);
    }
    println!("  Records:     {}", p.records_processed);
    if let Some(offset) = p.last_offset {
        println!("  Last offset: {}", offset);
    }
    println!("  Created:     {}", p.created_at);
}
