//! StreamHouse CLI (streamctl)
//!
//! Command-line tool for interacting with StreamHouse servers via gRPC.
//!
//! ## Overview
//!
//! `streamctl` provides an ergonomic interface to StreamHouse operations:
//! - **Topic management**: Create, list, get, and delete topics
//! - **Record production**: Send records with optional keys
//! - **Record consumption**: Read records from specific offsets
//! - **Offset management**: Commit and retrieve consumer group offsets
//!
//! ## Installation
//!
//! ```bash
//! # Build from source
//! cargo build --release -p streamhouse-cli
//!
//! # Binary will be at ./target/release/streamctl
//! ```
//!
//! ## Quick Start
//!
//! ```bash
//! # Connect to server (default: localhost:9090)
//! export STREAMHOUSE_ADDR=http://localhost:9090
//!
//! # Create a topic
//! streamctl topic create orders --partitions 3
//!
//! # Produce a record
//! streamctl produce orders --partition 0 --value '{"amount": 99.99}'
//!
//! # Consume records
//! streamctl consume orders --partition 0 --offset 0
//!
//! # Manage consumer offsets
//! streamctl offset commit --group analytics --topic orders --partition 0 --offset 42
//! streamctl offset get --group analytics --topic orders --partition 0
//! ```
//!
//! ## Configuration
//!
//! The CLI uses environment variables for configuration:
//! - `STREAMHOUSE_ADDR`: Server address (default: http://localhost:9090)
//!
//! ## Architecture
//!
//! The CLI uses:
//! - **clap**: For argument parsing and help generation
//! - **tonic**: For gRPC client communication
//! - **anyhow**: For ergonomic error handling
//! - **serde_json**: For JSON pretty-printing of consumed values
//!
//! ## Error Handling
//!
//! All commands return meaningful error messages:
//! - Connection errors include server address
//! - gRPC errors include status codes and messages
//! - Invalid arguments show usage help

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;

// Include generated protobuf code from build.rs
// This provides StreamHouseClient and all message types
pub mod pb {
    tonic::include_proto!("streamhouse");
}

mod auth;
mod commands;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod format;
mod repl;
mod rest_client;

use pb::stream_house_client::StreamHouseClient;

#[derive(Parser)]
#[command(name = "streamctl")]
#[command(about = "StreamHouse command-line tool", long_about = None)]
struct Cli {
    /// Server address (gRPC)
    #[arg(
        short,
        long,
        env = "STREAMHOUSE_ADDR",
        default_value = "http://localhost:9090"
    )]
    server: String,

    /// Schema Registry URL (REST)
    #[arg(
        long,
        env = "SCHEMA_REGISTRY_URL",
        default_value = "http://localhost:8081"
    )]
    schema_registry_url: String,

    /// REST API URL
    #[arg(
        long,
        env = "STREAMHOUSE_API_URL",
        default_value = "http://localhost:8080"
    )]
    api_url: String,

    /// API key for authentication
    #[arg(
        short = 'k',
        long = "api-key",
        env = "STREAMHOUSE_API_KEY",
        global = true
    )]
    api_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Topic management commands
    Topic {
        #[command(subcommand)]
        command: TopicCommands,
    },
    /// Schema registry commands
    Schema {
        #[command(subcommand)]
        command: commands::SchemaCommands,
    },
    /// Consumer group management commands
    Consumer {
        #[command(subcommand)]
        command: commands::ConsumerCommands,
    },
    /// SQL query commands
    Sql {
        #[command(subcommand)]
        command: commands::SqlCommands,
    },
    /// Produce records to a topic
    Produce {
        /// Topic name
        topic: String,
        /// Partition number
        #[arg(short, long)]
        partition: u32,
        /// Record key (optional)
        #[arg(short, long)]
        key: Option<String>,
        /// Record value (JSON string)
        #[arg(short, long, required_unless_present_any = ["file", "stdin"])]
        value: Option<String>,
        /// Skip client-side schema validation
        #[arg(long, default_value = "false")]
        skip_validation: bool,
        /// File containing JSON records (one per line) for batch produce
        #[arg(short, long)]
        file: Option<String>,
        /// Read JSON records from stdin for batch produce
        #[arg(long)]
        stdin: bool,
    },
    /// Consume records from a topic
    Consume {
        /// Topic name
        topic: String,
        /// Partition number
        #[arg(short, long)]
        partition: u32,
        /// Starting offset
        #[arg(short, long, default_value = "0")]
        offset: u64,
        /// Maximum number of records to consume
        #[arg(short, long)]
        limit: Option<u32>,
    },
    /// Consumer offset management
    Offset {
        #[command(subcommand)]
        command: OffsetCommands,
    },
    /// Pipeline management (targets, pipelines, transforms)
    Pipeline {
        #[command(subcommand)]
        command: commands::PipelineCommands,
    },
    /// Authentication management
    Auth {
        #[command(subcommand)]
        command: commands::auth::AuthCommands,
    },
    /// Connector management
    Connector {
        #[command(subcommand)]
        command: commands::ConnectorCommands,
    },
    /// Organization management
    Org {
        #[command(subcommand)]
        command: commands::OrgCommands,
    },
    /// Metrics and monitoring
    Metrics {
        #[command(subcommand)]
        command: commands::MetricsCommands,
    },
    /// Check server health
    Health,
}

#[derive(Subcommand)]
pub enum TopicCommands {
    /// Create a new topic
    Create {
        /// Topic name
        name: String,
        /// Number of partitions
        #[arg(short, long, default_value = "1")]
        partitions: u32,
        /// Retention period in milliseconds
        #[arg(short, long)]
        retention_ms: Option<u64>,
    },
    /// List all topics
    List,
    /// Get topic information
    Get {
        /// Topic name
        name: String,
    },
    /// Delete a topic
    Delete {
        /// Topic name
        name: String,
    },
    /// Get topic partition details
    Partitions {
        /// Topic name
        name: String,
    },
    /// Read messages from a topic via REST API
    Messages {
        /// Topic name
        name: String,
        /// Partition number
        #[arg(short, long, default_value = "0")]
        partition: u32,
        /// Starting offset
        #[arg(short, long, default_value = "0")]
        offset: u64,
        /// Maximum number of messages
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
}

#[derive(Subcommand)]
pub enum OffsetCommands {
    /// Commit consumer group offset
    Commit {
        /// Consumer group name
        #[arg(short, long)]
        group: String,
        /// Topic name
        #[arg(short, long)]
        topic: String,
        /// Partition number
        #[arg(short, long)]
        partition: u32,
        /// Offset to commit
        #[arg(short, long)]
        offset: u64,
    },
    /// Get committed offset for a consumer group
    Get {
        /// Consumer group name
        #[arg(short, long)]
        group: String,
        /// Topic name
        #[arg(short, long)]
        topic: String,
        /// Partition number
        #[arg(short, long)]
        partition: u32,
    },
}

/// Main entry point for the CLI.
///
/// This function:
/// 1. Parses command-line arguments using clap
/// 2. Establishes a gRPC connection to the server
/// 3. Either enters REPL mode (if no subcommand) or dispatches to command handler
/// 4. Returns errors with context for better debugging
#[tokio::main]
async fn main() -> Result<()> {
    // Check if running in interactive mode (no arguments)
    let args: Vec<String> = std::env::args().collect();
    let is_interactive = args.len() == 1 || (args.len() == 3 && args[1] == "--server");

    if is_interactive {
        // Interactive REPL mode
        let server = if args.len() == 3 {
            args[2].clone()
        } else {
            std::env::var("STREAMHOUSE_ADDR")
                .unwrap_or_else(|_| "http://localhost:9090".to_string())
        };

        let schema_registry_url = std::env::var("SCHEMA_REGISTRY_URL")
            .unwrap_or_else(|_| "http://localhost:8081".to_string());

        let api_url = std::env::var("STREAMHOUSE_API_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        let channel = Channel::from_shared(server.clone())
            .context("Invalid server address")?
            .connect()
            .await
            .context("Failed to connect to server")?;

        let client = StreamHouseClient::new(channel);
        let mut repl = repl::Repl::new(client, schema_registry_url, api_url)?;
        repl.run().await?;
    } else {
        // Traditional CLI mode
        let mut cli = Cli::parse();

        // If no --api-key flag or env var, try to load from stored auth
        if cli.api_key.is_none() {
            if let Ok(manager) = auth::AuthManager::new() {
                if let Some(key) = manager.active_api_key() {
                    cli.api_key = Some(key.to_string());
                }
                // Also use stored server URL if not overridden
                if let Some(url) = manager.server_url() {
                    if cli.api_url == "http://localhost:8080" {
                        cli.api_url = url.to_string();
                    }
                }
            }
        }

        // Helper to lazily connect gRPC only when needed
        let connect_grpc = || async {
            let channel = Channel::from_shared(cli.server.clone())
                .context("Invalid server address")?
                .connect()
                .await
                .context("Failed to connect to gRPC server")?;
            Ok::<_, anyhow::Error>(StreamHouseClient::new(channel))
        };

        match cli.command {
            // --- Commands that need gRPC ---
            Commands::Topic { command } => match command {
                TopicCommands::Partitions { ref name } => {
                    handle_topic_partitions(
                        &cli.api_url,
                        cli.api_key.as_deref(),
                        name,
                    )
                    .await?
                }
                TopicCommands::Messages {
                    ref name,
                    partition,
                    offset,
                    limit,
                } => {
                    handle_topic_messages(
                        &cli.api_url,
                        cli.api_key.as_deref(),
                        name,
                        partition,
                        offset,
                        limit,
                    )
                    .await?
                }
                TopicCommands::List => {
                    handle_topic_list_rest(
                        &cli.api_url,
                        cli.api_key.as_deref(),
                    )
                    .await?
                }
                _ => {
                    let mut client = connect_grpc().await?;
                    handle_topic_command(&mut client, command).await?
                }
            },
            Commands::Produce {
                topic,
                partition,
                key,
                value,
                skip_validation,
                file,
                stdin,
            } => {
                if file.is_some() || stdin {
                    handle_batch_produce(
                        &cli.api_url,
                        cli.api_key.as_deref(),
                        &topic,
                        partition,
                        key.as_deref(),
                        file.as_deref(),
                        stdin,
                    )
                    .await?
                } else if let Some(value) = value {
                    let mut client = connect_grpc().await?;
                    let registry_url = if skip_validation {
                        None
                    } else {
                        Some(cli.schema_registry_url.as_str())
                    };
                    handle_produce(&mut client, topic, partition, key, value, registry_url).await?
                } else {
                    anyhow::bail!("Either --value, --file, or --stdin must be provided");
                }
            }
            Commands::Consume {
                topic,
                partition,
                offset,
                limit,
            } => {
                let mut client = connect_grpc().await?;
                handle_consume(&mut client, topic, partition, offset, limit).await?
            }
            Commands::Offset { command } => {
                let mut client = connect_grpc().await?;
                handle_offset_command(&mut client, command).await?
            }

            // --- Commands that only need REST (no gRPC) ---
            Commands::Schema { command } => {
                commands::schema::handle_schema_command(command, &cli.schema_registry_url).await?
            }
            Commands::Consumer { command } => {
                commands::consumer::handle_consumer_command(command, &cli.api_url).await?
            }
            Commands::Sql { command } => {
                commands::sql::handle_sql_command(command, &cli.api_url).await?
            }
            Commands::Pipeline { command } => {
                commands::pipeline::handle_pipeline_command(command, &cli.api_url).await?
            }
            Commands::Auth { command } => {
                commands::auth::handle_auth_command(
                    command,
                    &cli.api_url,
                    cli.api_key.as_deref(),
                )
                .await?
            }
            Commands::Connector { command } => {
                commands::connector::handle_connector_command(
                    command,
                    &cli.api_url,
                    cli.api_key.as_deref(),
                )
                .await?
            }
            Commands::Org { command } => {
                commands::org::handle_org_command(
                    command,
                    &cli.api_url,
                    cli.api_key.as_deref(),
                )
                .await?
            }
            Commands::Metrics { command } => {
                commands::metrics::handle_metrics_command(
                    command,
                    &cli.api_url,
                    cli.api_key.as_deref(),
                )
                .await?
            }
            Commands::Health => {
                commands::metrics::handle_health_command(
                    &cli.api_url,
                    cli.api_key.as_deref(),
                )
                .await?
            }
        }
    }

    Ok(())
}

/// Handles all topic-related commands.
///
/// This function dispatches to the appropriate topic operation:
/// - Create: Creates a new topic with specified partitions
/// - List: Lists all topics in the system
/// - Get: Retrieves information about a specific topic
/// - Delete: Removes a topic and all its data
///
/// All operations use the gRPC client to communicate with the server.
pub async fn handle_topic_command(
    client: &mut StreamHouseClient<Channel>,
    command: TopicCommands,
) -> Result<()> {
    match command {
        TopicCommands::Create {
            name,
            partitions,
            retention_ms,
        } => {
            let request = pb::CreateTopicRequest {
                name: name.clone(),
                partition_count: partitions,
                retention_ms,
                config: std::collections::HashMap::new(),
            };

            let response = client
                .create_topic(request)
                .await
                .context("Failed to create topic")?;

            let data = response.into_inner();
            println!("✅ Topic created:");
            println!("  Name: {}", data.topic_id);
            println!("  Partitions: {}", data.partition_count);
        }
        TopicCommands::List => {
            let request = pb::ListTopicsRequest {};
            let response = client
                .list_topics(request)
                .await
                .context("Failed to list topics")?;

            let data = response.into_inner();
            if data.topics.is_empty() {
                println!("No topics found");
            } else {
                println!("Topics ({}):", data.topics.len());
                for topic in data.topics {
                    println!("  - {} ({} partitions)", topic.name, topic.partition_count);
                }
            }
        }
        TopicCommands::Get { name } => {
            let request = pb::GetTopicRequest { name: name.clone() };
            let response = client
                .get_topic(request)
                .await
                .context("Failed to get topic")?;

            let data = response.into_inner();
            if let Some(topic) = data.topic {
                println!("Topic: {}", topic.name);
                println!("  Partitions: {}", topic.partition_count);
                if let Some(retention) = topic.retention_ms {
                    println!("  Retention: {}ms", retention);
                }
            } else {
                println!("Topic not found: {}", name);
            }
        }
        TopicCommands::Delete { name } => {
            let request = pb::DeleteTopicRequest { name: name.clone() };
            client
                .delete_topic(request)
                .await
                .context("Failed to delete topic")?;

            println!("✅ Topic deleted: {}", name);
        }
        TopicCommands::Partitions { name } | TopicCommands::Messages { name, .. } => {
            // These are handled via REST below, but we need the api_url which
            // isn't available here. We'll handle this in main() dispatch instead.
            // This arm should not be reached.
            anyhow::bail!(
                "Topic {} subcommand should be handled via REST dispatch",
                name
            );
        }
    }

    Ok(())
}

/// Produces a single record to a topic partition.
///
/// ## Arguments
/// - `topic`: Name of the topic to produce to
/// - `partition`: Partition number (0-indexed)
/// - `key`: Optional record key (used for ordering/compaction)
/// - `value`: Record value (typically JSON string)
///
/// ## Returns
/// Prints the offset and timestamp of the produced record.
///
/// ## Example
/// ```bash
/// streamctl produce orders --partition 0 --value '{"amount": 99.99}'
/// ```
pub async fn handle_produce(
    client: &mut StreamHouseClient<Channel>,
    topic: String,
    partition: u32,
    key: Option<String>,
    value: String,
    schema_registry_url: Option<&str>,
) -> Result<()> {
    // Client-side schema validation (if registry URL provided)
    if let Some(registry_url) = schema_registry_url {
        if let Err(msg) = validate_value_client_side(registry_url, &topic, &value).await {
            anyhow::bail!("Schema validation failed: {}", msg);
        }
    }

    let key_bytes = key.map(|k| k.into_bytes()).unwrap_or_default();
    let value_bytes = value.into_bytes();

    let request = pb::ProduceRequest {
        topic: topic.clone(),
        partition,
        key: key_bytes,
        value: value_bytes,
        headers: std::collections::HashMap::new(),
    };

    let response = client
        .produce(request)
        .await
        .context("Failed to produce record")?;

    let data = response.into_inner();
    println!("✅ Record produced:");
    println!("  Topic: {}", topic);
    println!("  Partition: {}", partition);
    println!("  Offset: {}", data.offset);
    println!("  Timestamp: {}", data.timestamp);

    Ok(())
}

/// Validate a value against the schema registered for `{topic}-value` via REST API.
/// Returns Ok(()) if no schema is registered or validation passes.
async fn validate_value_client_side(
    registry_url: &str,
    topic: &str,
    value: &str,
) -> Result<(), String> {
    let subject = format!("{}-value", topic);
    let url = format!("{}/subjects/{}/versions/latest", registry_url, subject);

    let http_client = reqwest::Client::new();
    let resp = match http_client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => return Ok(()), // Can't reach registry → skip validation
    };

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(()); // No schema registered → skip
    }

    if !resp.status().is_success() {
        return Ok(()); // Registry error → skip (server-side will catch it)
    }

    let schema_resp: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let schema_type = schema_resp
        .get("schemaType")
        .and_then(|v| v.as_str())
        .unwrap_or("AVRO");

    let schema_str = schema_resp
        .get("schema")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'schema' field in response")?;

    match schema_type {
        "JSON" => validate_json_schema_client(schema_str, value),
        "AVRO" => validate_avro_schema_client(schema_str, value),
        _ => Ok(()), // Protobuf etc — not implemented
    }
}

fn validate_json_schema_client(schema_str: &str, value: &str) -> Result<(), String> {
    let schema_value: serde_json::Value =
        serde_json::from_str(schema_str).map_err(|e| format!("Invalid schema: {}", e))?;
    let instance: serde_json::Value =
        serde_json::from_str(value).map_err(|e| format!("Value is not valid JSON: {}", e))?;
    let validator =
        jsonschema::validator_for(&schema_value).map_err(|e| format!("Schema error: {}", e))?;
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{}", errors.join("; ")))
    }
}

fn validate_avro_schema_client(schema_str: &str, value: &str) -> Result<(), String> {
    let schema = apache_avro::Schema::parse_str(schema_str)
        .map_err(|e| format!("Invalid Avro schema: {}", e))?;
    let json_value: serde_json::Value =
        serde_json::from_str(value).map_err(|e| format!("Value is not valid JSON: {}", e))?;
    let avro_value = apache_avro::types::Value::from(json_value);
    if avro_value.resolve(&schema).is_ok() {
        Ok(())
    } else {
        Err("Value does not conform to Avro schema".to_string())
    }
}

/// Consumes records from a topic partition.
///
/// ## Arguments
/// - `topic`: Name of the topic to consume from
/// - `partition`: Partition number to read
/// - `offset`: Starting offset (0 for beginning)
/// - `limit`: Maximum number of records to fetch (default: 100)
///
/// ## Behavior
/// - Fetches records starting at the specified offset
/// - Attempts to pretty-print JSON values
/// - Shows binary data with byte count for non-UTF8 values
/// - Displays headers if present
///
/// ## Example
/// ```bash
/// streamctl consume orders --partition 0 --offset 0 --limit 10
/// ```
pub async fn handle_consume(
    client: &mut StreamHouseClient<Channel>,
    topic: String,
    partition: u32,
    offset: u64,
    limit: Option<u32>,
) -> Result<()> {
    let request = pb::ConsumeRequest {
        topic: topic.clone(),
        partition,
        offset,
        max_records: limit.unwrap_or(100),
        consumer_group: None,
    };

    let response = client
        .consume(request)
        .await
        .context("Failed to consume records")?;

    let data = response.into_inner();

    println!(
        "📥 Consuming from {}:{} starting at offset {}",
        topic, partition, offset
    );
    println!();

    if data.records.is_empty() {
        println!("No records found");
    } else {
        for (idx, record) in data.records.iter().enumerate() {
            println!(
                "Record {} (offset: {}, timestamp: {})",
                idx + 1,
                record.offset,
                record.timestamp
            );

            if !record.key.is_empty() {
                match String::from_utf8(record.key.clone()) {
                    Ok(key) => println!("  Key: {}", key),
                    Err(_) => println!("  Key: {} bytes (binary)", record.key.len()),
                }
            }

            match String::from_utf8(record.value.clone()) {
                Ok(value) => {
                    // Try to pretty-print as JSON
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&value) {
                        println!("  Value: {}", serde_json::to_string_pretty(&json)?);
                    } else {
                        println!("  Value: {}", value);
                    }
                }
                Err(_) => println!("  Value: {} bytes (binary)", record.value.len()),
            }

            if !record.headers.is_empty() {
                println!("  Headers:");
                for (k, v) in &record.headers {
                    println!("    {}: {}", k, v);
                }
            }

            println!();
        }

        println!("✅ Consumed {} records", data.records.len());
        if data.has_more {
            println!("(More records available, use --limit to fetch more)");
        }
    }

    Ok(())
}

/// Handles consumer offset operations.
///
/// This function manages consumer group offsets:
/// - Commit: Stores the current processing position for a consumer group
/// - Get: Retrieves the last committed offset for a consumer group
///
/// ## Consumer Groups
/// Consumer groups track their progress through a topic partition.
/// Multiple instances of the same group coordinate to avoid duplicate processing.
///
/// ## Example
/// ```bash
/// # Mark that we've processed up to offset 100
/// streamctl offset commit --group analytics --topic orders --partition 0 --offset 100
///
/// # Later, retrieve where we left off
/// streamctl offset get --group analytics --topic orders --partition 0
/// ```
pub async fn handle_offset_command(
    client: &mut StreamHouseClient<Channel>,
    command: OffsetCommands,
) -> Result<()> {
    match command {
        OffsetCommands::Commit {
            group,
            topic,
            partition,
            offset,
        } => {
            let request = pb::CommitOffsetRequest {
                consumer_group: group.clone(),
                topic: topic.clone(),
                partition,
                offset,
                metadata: None,
            };

            client
                .commit_offset(request)
                .await
                .context("Failed to commit offset")?;

            println!("✅ Offset committed:");
            println!("  Consumer group: {}", group);
            println!("  Topic: {}", topic);
            println!("  Partition: {}", partition);
            println!("  Offset: {}", offset);
        }
        OffsetCommands::Get {
            group,
            topic,
            partition,
        } => {
            let request = pb::GetOffsetRequest {
                consumer_group: group.clone(),
                topic: topic.clone(),
                partition,
            };

            let response = client
                .get_offset(request)
                .await
                .context("Failed to get offset")?;

            let data = response.into_inner();
            println!("Consumer group: {}", group);
            println!("  Topic: {}", topic);
            println!("  Partition: {}", partition);
            println!("  Committed offset: {}", data.offset);
        }
    }

    Ok(())
}

// --- REST-based topic commands ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct PartitionInfo {
    #[serde(default)]
    topic: Option<String>,
    partition_id: u32,
    #[serde(default)]
    high_watermark: u64,
    #[serde(default)]
    low_watermark: u64,
    #[serde(default)]
    leader_agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TopicMessage {
    offset: u64,
    #[serde(default)]
    partition: u32,
    #[serde(default)]
    timestamp: Option<u64>,
    #[serde(default)]
    key: Option<String>,
    value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TopicListEntry {
    name: String,
    partitions: u32,
    #[serde(default)]
    replication_factor: u32,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    message_count: i64,
    #[serde(default)]
    size_bytes: i64,
}

async fn handle_topic_partitions(
    api_url: &str,
    api_key: Option<&str>,
    topic: &str,
) -> Result<()> {
    let client = rest_client::RestClient::with_api_key(api_url, api_key.map(String::from));
    let partitions: Vec<PartitionInfo> = client
        .get(&format!("/api/v1/topics/{}/partitions", topic))
        .await
        .context("Failed to get topic partitions")?;

    if partitions.is_empty() {
        println!("No partitions found for topic '{}'", topic);
    } else {
        println!("Partitions for topic '{}' ({}):", topic, partitions.len());
        println!(
            "{:<12} {:<15} {:<15}",
            "Partition", "Low WM", "High WM"
        );
        println!("{}", "-".repeat(42));
        for p in &partitions {
            println!(
                "{:<12} {:<15} {:<15}",
                p.partition_id, p.low_watermark, p.high_watermark
            );
        }
    }

    Ok(())
}

async fn handle_topic_list_rest(
    api_url: &str,
    api_key: Option<&str>,
) -> Result<()> {
    let client = rest_client::RestClient::with_api_key(api_url, api_key.map(String::from));
    let topics: Vec<TopicListEntry> = client
        .get("/api/v1/topics")
        .await
        .context("Failed to list topics")?;

    if topics.is_empty() {
        println!("No topics found");
    } else {
        println!("Topics ({}):", topics.len());
        println!(
            "{:<25} {:<12} {:<12} {:<12}",
            "Name", "Partitions", "Messages", "Size (bytes)"
        );
        println!("{}", "-".repeat(61));
        for t in &topics {
            println!(
                "{:<25} {:<12} {:<12} {:<12}",
                t.name, t.partitions, t.message_count, t.size_bytes
            );
        }
    }

    Ok(())
}

async fn handle_topic_messages(
    api_url: &str,
    api_key: Option<&str>,
    topic: &str,
    partition: u32,
    offset: u64,
    limit: u32,
) -> Result<()> {
    let client = rest_client::RestClient::with_api_key(api_url, api_key.map(String::from));
    let messages: Vec<TopicMessage> = client
        .get(&format!(
            "/api/v1/topics/{}/messages?partition={}&offset={}&limit={}",
            topic, partition, offset, limit
        ))
        .await
        .context("Failed to get topic messages")?;

    if messages.is_empty() {
        println!("No messages found");
    } else {
        println!(
            "Messages from {}:{} (offset {}):",
            topic, partition, offset
        );
        println!();
        for msg in &messages {
            println!("Offset: {}", msg.offset);
            if let Some(ts) = msg.timestamp {
                println!("  Timestamp: {}", ts);
            }
            if let Some(ref key) = msg.key {
                println!("  Key: {}", key);
            }
            // value comes as a JSON string from the API, try to parse and pretty-print
            if let serde_json::Value::String(ref s) = msg.value {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                    println!(
                        "  Value: {}",
                        serde_json::to_string_pretty(&parsed)
                            .unwrap_or_else(|_| s.clone())
                    );
                } else {
                    println!("  Value: {}", s);
                }
            } else {
                println!(
                    "  Value: {}",
                    serde_json::to_string_pretty(&msg.value)
                        .unwrap_or_else(|_| msg.value.to_string())
                );
            }
            println!();
        }
        println!("{} message(s) returned", messages.len());
    }

    Ok(())
}

// --- Batch produce via REST ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchProduceRequest {
    topic: String,
    partition: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    records: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchProduceResponse {
    count: usize,
    #[serde(default)]
    first_offset: Option<u64>,
    #[serde(default)]
    last_offset: Option<u64>,
}

async fn handle_batch_produce(
    api_url: &str,
    api_key: Option<&str>,
    topic: &str,
    partition: u32,
    key: Option<&str>,
    file: Option<&str>,
    from_stdin: bool,
) -> Result<()> {
    let input = if let Some(path) = file {
        std::fs::read_to_string(path).context("Failed to read file")?
    } else if from_stdin {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read from stdin")?;
        buf
    } else {
        anyhow::bail!("Either --file or --stdin must be specified for batch produce");
    };

    let records: Vec<serde_json::Value> = input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("Invalid JSON record"))
        .collect::<Result<Vec<_>>>()?;

    if records.is_empty() {
        println!("No records to produce");
        return Ok(());
    }

    let client = rest_client::RestClient::with_api_key(api_url, api_key.map(String::from));
    let req = BatchProduceRequest {
        topic: topic.to_string(),
        partition,
        key: key.map(String::from),
        records,
    };

    let resp: BatchProduceResponse = client
        .post("/api/v1/produce/batch", &req)
        .await
        .context("Failed to batch produce")?;

    println!("✅ Batch produce complete:");
    println!("  Topic:     {}", topic);
    println!("  Partition: {}", partition);
    println!("  Records:   {}", resp.count);
    if let Some(first) = resp.first_offset {
        println!("  First offset: {}", first);
    }
    if let Some(last) = resp.last_offset {
        println!("  Last offset:  {}", last);
    }

    Ok(())
}
