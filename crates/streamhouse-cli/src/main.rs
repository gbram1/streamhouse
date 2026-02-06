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
use tonic::transport::Channel;

// Include generated protobuf code from build.rs
// This provides StreamHouseClient and all message types
pub mod pb {
    tonic::include_proto!("streamhouse");
}

mod commands;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod format;
mod repl;
#[allow(dead_code)]
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
        #[arg(short, long)]
        value: String,
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
        let cli = Cli::parse();

        let channel = Channel::from_shared(cli.server.clone())
            .context("Invalid server address")?
            .connect()
            .await
            .context("Failed to connect to server")?;

        let mut client = StreamHouseClient::new(channel);

        match cli.command {
            Commands::Topic { command } => handle_topic_command(&mut client, command).await?,
            Commands::Schema { command } => {
                commands::schema::handle_schema_command(command, &cli.schema_registry_url).await?
            }
            Commands::Produce {
                topic,
                partition,
                key,
                value,
            } => handle_produce(&mut client, topic, partition, key, value).await?,
            Commands::Consume {
                topic,
                partition,
                offset,
                limit,
            } => handle_consume(&mut client, topic, partition, offset, limit).await?,
            Commands::Offset { command } => handle_offset_command(&mut client, command).await?,
            Commands::Consumer { command } => {
                commands::consumer::handle_consumer_command(command, &cli.api_url).await?
            }
            Commands::Sql { command } => {
                commands::sql::handle_sql_command(command, &cli.api_url).await?
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
) -> Result<()> {
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
