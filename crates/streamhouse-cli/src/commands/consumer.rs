//! Consumer group management commands
//!
//! These commands use the REST API to manage consumer groups:
//! - List consumer groups
//! - Get consumer group details
//! - Reset offsets
//! - Seek to timestamp
//! - Delete consumer group

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::rest_client::RestClient;

#[derive(Subcommand)]
pub enum ConsumerCommands {
    /// List all consumer groups
    List,
    /// Get consumer group details
    Get {
        /// Consumer group ID
        group_id: String,
    },
    /// Get consumer group lag
    Lag {
        /// Consumer group ID
        group_id: String,
    },
    /// Reset consumer group offsets
    Reset {
        /// Consumer group ID
        group_id: String,
        /// Reset strategy: earliest, latest, specific
        #[arg(short, long, default_value = "earliest")]
        strategy: String,
        /// Topic (optional, resets all topics if not specified)
        #[arg(short, long)]
        topic: Option<String>,
        /// Partition (optional, resets all partitions if not specified)
        #[arg(short, long)]
        partition: Option<u32>,
        /// Specific offset (required when strategy = specific)
        #[arg(short, long)]
        offset: Option<u64>,
    },
    /// Seek consumer group to a timestamp
    Seek {
        /// Consumer group ID
        group_id: String,
        /// Topic to seek
        #[arg(short, long)]
        topic: String,
        /// Timestamp (ISO 8601 format, e.g., 2026-01-15T10:00:00Z)
        #[arg(long)]
        timestamp: String,
        /// Partition (optional, seeks all partitions if not specified)
        #[arg(short, long)]
        partition: Option<u32>,
    },
    /// Delete a consumer group
    Delete {
        /// Consumer group ID
        group_id: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

// API Response types

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupInfo {
    pub group_id: String,
    pub topics: Vec<String>,
    pub total_lag: i64,
    pub partition_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupDetail {
    pub group_id: String,
    pub offsets: Vec<ConsumerOffsetInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerOffsetInfo {
    pub topic: String,
    pub partition_id: u32,
    pub committed_offset: u64,
    pub high_watermark: u64,
    pub lag: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupLag {
    pub group_id: String,
    pub total_lag: i64,
    pub partition_count: usize,
    pub topics: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResetOffsetsRequest {
    strategy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    partition: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ResetOffsetsResponse {
    pub success: bool,
    pub partitions_reset: usize,
    pub details: Vec<ResetOffsetDetail>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetOffsetDetail {
    pub topic: String,
    pub partition: u32,
    pub old_offset: u64,
    pub new_offset: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeekToTimestampRequest {
    topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    partition: Option<u32>,
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SeekToTimestampResponse {
    pub success: bool,
    pub partitions_updated: usize,
    pub details: Vec<SeekOffsetDetail>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SeekOffsetDetail {
    pub topic: String,
    pub partition: u32,
    pub old_offset: u64,
    pub new_offset: u64,
    pub timestamp_found: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DeleteConsumerGroupResponse {
    pub success: bool,
    pub group_id: String,
    pub partitions_deleted: usize,
}

/// Handle consumer group commands
pub async fn handle_consumer_command(command: ConsumerCommands, api_url: &str) -> Result<()> {
    let client = RestClient::new(api_url);

    match command {
        ConsumerCommands::List => {
            let groups: Vec<ConsumerGroupInfo> = client
                .get("/api/v1/consumer-groups")
                .await
                .context("Failed to list consumer groups")?;

            if groups.is_empty() {
                println!("No consumer groups found");
            } else {
                println!("Consumer Groups ({}):", groups.len());
                println!();
                println!(
                    "{:<30} {:<10} {:<15} Topics",
                    "Group ID", "Lag", "Partitions"
                );
                println!("{}", "-".repeat(80));
                for group in groups {
                    let lag_str = if group.total_lag > 0 {
                        format!("{}", group.total_lag)
                    } else {
                        "0 (caught up)".to_string()
                    };
                    println!(
                        "{:<30} {:<10} {:<15} {}",
                        group.group_id,
                        lag_str,
                        group.partition_count,
                        group.topics.join(", ")
                    );
                }
            }
        }

        ConsumerCommands::Get { group_id } => {
            let detail: ConsumerGroupDetail = client
                .get(&format!(
                    "/api/v1/consumer-groups/{}",
                    urlencoding::encode(&group_id)
                ))
                .await
                .context("Failed to get consumer group")?;

            println!("Consumer Group: {}", detail.group_id);
            println!();

            if detail.offsets.is_empty() {
                println!("No offsets committed");
            } else {
                println!(
                    "{:<20} {:<10} {:<15} {:<15} {:<10}",
                    "Topic", "Partition", "Committed", "High Watermark", "Lag"
                );
                println!("{}", "-".repeat(80));
                for offset in detail.offsets {
                    let lag_str = if offset.lag > 0 {
                        format!("{}", offset.lag)
                    } else {
                        "0".to_string()
                    };
                    println!(
                        "{:<20} {:<10} {:<15} {:<15} {:<10}",
                        offset.topic,
                        offset.partition_id,
                        offset.committed_offset,
                        offset.high_watermark,
                        lag_str
                    );
                }
            }
        }

        ConsumerCommands::Lag { group_id } => {
            let lag: ConsumerGroupLag = client
                .get(&format!(
                    "/api/v1/consumer-groups/{}/lag",
                    urlencoding::encode(&group_id)
                ))
                .await
                .context("Failed to get consumer group lag")?;

            println!("Consumer Group: {}", lag.group_id);
            println!("  Total Lag: {}", lag.total_lag);
            println!("  Partitions: {}", lag.partition_count);
            println!("  Topics: {}", lag.topics.join(", "));
        }

        ConsumerCommands::Reset {
            group_id,
            strategy,
            topic,
            partition,
            offset,
        } => {
            // Validate strategy
            let strategy_lower = strategy.to_lowercase();
            if !["earliest", "latest", "specific"].contains(&strategy_lower.as_str()) {
                anyhow::bail!(
                    "Invalid strategy: {}. Must be one of: earliest, latest, specific",
                    strategy
                );
            }

            if strategy_lower == "specific" && offset.is_none() {
                anyhow::bail!("--offset is required when using 'specific' strategy");
            }

            let request = ResetOffsetsRequest {
                strategy: strategy_lower,
                topic,
                partition,
                offset,
            };

            let response: ResetOffsetsResponse = client
                .post(
                    &format!(
                        "/api/v1/consumer-groups/{}/reset",
                        urlencoding::encode(&group_id)
                    ),
                    &request,
                )
                .await
                .context("Failed to reset offsets")?;

            println!("Offsets reset for consumer group: {}", group_id);
            println!("  Partitions reset: {}", response.partitions_reset);
            println!();

            if !response.details.is_empty() {
                println!(
                    "{:<20} {:<10} {:<15} {:<15}",
                    "Topic", "Partition", "Old Offset", "New Offset"
                );
                println!("{}", "-".repeat(60));
                for detail in response.details {
                    println!(
                        "{:<20} {:<10} {:<15} {:<15}",
                        detail.topic, detail.partition, detail.old_offset, detail.new_offset
                    );
                }
            }
        }

        ConsumerCommands::Seek {
            group_id,
            topic,
            timestamp,
            partition,
        } => {
            // Parse timestamp
            let timestamp_ms = chrono::DateTime::parse_from_rfc3339(&timestamp)
                .or_else(|_| {
                    // Try parsing as naive datetime
                    chrono::NaiveDateTime::parse_from_str(&timestamp, "%Y-%m-%dT%H:%M:%S").map(
                        |dt| {
                            dt.and_local_timezone(chrono::Utc)
                                .single()
                                .unwrap()
                                .fixed_offset()
                        },
                    )
                })
                .context("Invalid timestamp format. Use ISO 8601 (e.g., 2026-01-15T10:00:00Z)")?
                .timestamp_millis();

            let request = SeekToTimestampRequest {
                topic: topic.clone(),
                partition,
                timestamp: timestamp_ms,
            };

            let response: SeekToTimestampResponse = client
                .post(
                    &format!(
                        "/api/v1/consumer-groups/{}/seek",
                        urlencoding::encode(&group_id)
                    ),
                    &request,
                )
                .await
                .context("Failed to seek to timestamp")?;

            println!(
                "Seeked consumer group {} to timestamp {}",
                group_id, timestamp
            );
            println!("  Partitions updated: {}", response.partitions_updated);
            println!();

            if !response.details.is_empty() {
                println!(
                    "{:<20} {:<10} {:<15} {:<15}",
                    "Topic", "Partition", "Old Offset", "New Offset"
                );
                println!("{}", "-".repeat(60));
                for detail in response.details {
                    println!(
                        "{:<20} {:<10} {:<15} {:<15}",
                        detail.topic, detail.partition, detail.old_offset, detail.new_offset
                    );
                }
            }
        }

        ConsumerCommands::Delete { group_id, force } => {
            if !force {
                println!(
                    "Are you sure you want to delete consumer group '{}'?",
                    group_id
                );
                println!("This will remove all committed offsets and cannot be undone.");
                println!("Use --force to skip this confirmation.");
                return Ok(());
            }

            let response: DeleteConsumerGroupResponse = client
                .delete_json(&format!(
                    "/api/v1/consumer-groups/{}",
                    urlencoding::encode(&group_id)
                ))
                .await
                .context("Failed to delete consumer group")?;

            println!("Consumer group deleted: {}", response.group_id);
            println!("  Partitions deleted: {}", response.partitions_deleted);
        }
    }

    Ok(())
}
