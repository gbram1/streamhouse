//! API models for REST endpoints

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Topic {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTopicRequest {
    pub name: String,
    pub partitions: u32,
    #[serde(default = "default_replication_factor")]
    pub replication_factor: u32,
}

fn default_replication_factor() -> u32 {
    1
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Agent {
    pub agent_id: String,
    pub address: String,
    pub availability_zone: String,
    pub agent_group: String,
    pub last_heartbeat: i64,
    pub started_at: i64,
    pub active_leases: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Partition {
    pub topic: String,
    pub partition_id: u32,
    pub leader_agent_id: Option<String>,
    pub high_watermark: u64,
    pub low_watermark: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConsumerGroup {
    pub group_id: String,
    pub topic: String,
    pub members: u32,
    pub state: String,
    pub total_lag: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProduceRequest {
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProduceResponse {
    pub offset: u64,
    pub partition: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MetricsSnapshot {
    pub topics_count: u64,
    pub agents_count: u64,
    pub partitions_count: u64,
    pub total_messages: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeResponse {
    pub records: Vec<ConsumedRecord>,
    pub next_offset: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumedRecord {
    pub partition: u32,
    pub offset: u64,
    pub key: Option<String>,
    pub value: String,
    pub timestamp: i64,
}
