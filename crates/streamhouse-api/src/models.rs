//! API models for REST endpoints

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Topic {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub created_at: String,
    #[serde(default)]
    pub message_count: u64,
    #[serde(default)]
    pub size_bytes: u64,
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupInfo {
    pub group_id: String,
    pub topics: Vec<String>,
    pub total_lag: i64,
    pub partition_count: usize,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupDetail {
    pub group_id: String,
    pub offsets: Vec<ConsumerOffsetInfo>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerOffsetInfo {
    pub topic: String,
    pub partition_id: u32,
    pub committed_offset: u64,
    pub high_watermark: u64,
    pub lag: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroupLag {
    pub group_id: String,
    pub total_lag: i64,
    pub partition_count: usize,
    pub topics: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorageMetricsResponse {
    pub total_size_bytes: u64,
    pub segment_count: u64,
    pub storage_by_topic: std::collections::HashMap<String, u64>,
    pub cache_size: u64,
    pub cache_hit_rate: f64,
    pub cache_evictions: u64,
    pub wal_size: u64,
    pub wal_uncommitted_entries: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThroughputMetric {
    pub timestamp: i64,
    pub messages_per_second: f64,
    pub bytes_per_second: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LatencyMetric {
    pub timestamp: i64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub avg: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMetric {
    pub timestamp: i64,
    pub error_rate: f64,
    pub error_count: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentMetricsResponse {
    pub agent_id: String,
    pub address: String,
    pub availability_zone: String,
    pub partition_count: usize,
    pub last_heartbeat: i64,
    pub uptime_ms: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TimeRangeParams {
    pub time_range: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageQueryParams {
    pub partition: Option<u32>,
    pub offset: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommitOffsetRequest {
    pub group_id: String,
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommitOffsetResponse {
    pub success: bool,
}

// Batch produce models
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchProduceRequest {
    pub topic: String,
    pub records: Vec<BatchRecord>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchProduceResponse {
    pub count: usize,
    pub offsets: Vec<BatchRecordResult>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchRecordResult {
    pub partition: u32,
    pub offset: u64,
}

// Consumer Actions models

/// Strategy for resetting consumer group offsets
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResetStrategy {
    /// Reset to the earliest available offset
    Earliest,
    /// Reset to the latest offset (skip all existing messages)
    Latest,
    /// Reset to a specific offset value
    Specific,
    /// Reset to the offset at or after a specific timestamp
    Timestamp,
}

/// Request to reset consumer group offsets
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetOffsetsRequest {
    /// Reset strategy to use
    pub strategy: ResetStrategy,
    /// Topic to reset (None = all topics for this group)
    pub topic: Option<String>,
    /// Partition to reset (None = all partitions)
    pub partition: Option<u32>,
    /// Specific offset (required when strategy = "specific")
    pub offset: Option<u64>,
    /// Unix epoch milliseconds (required when strategy = "timestamp")
    pub timestamp: Option<i64>,
}

/// Response after resetting offsets
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetOffsetsResponse {
    pub success: bool,
    pub partitions_reset: usize,
    pub details: Vec<ResetOffsetDetail>,
}

/// Detail of a single partition offset reset
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetOffsetDetail {
    pub topic: String,
    pub partition: u32,
    pub old_offset: u64,
    pub new_offset: u64,
}

/// Request to seek consumer group to a timestamp
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeekToTimestampRequest {
    /// Topic to seek
    pub topic: String,
    /// Partition to seek (None = all partitions)
    pub partition: Option<u32>,
    /// Unix epoch milliseconds to seek to
    pub timestamp: i64,
}

/// Response after seeking to timestamp
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeekToTimestampResponse {
    pub success: bool,
    pub partitions_updated: usize,
    pub details: Vec<SeekOffsetDetail>,
}

/// Detail of a single partition seek result
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeekOffsetDetail {
    pub topic: String,
    pub partition: u32,
    pub old_offset: u64,
    pub new_offset: u64,
    pub timestamp_found: i64,
}

/// Response after deleting a consumer group
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteConsumerGroupResponse {
    pub success: bool,
    pub group_id: String,
    pub partitions_deleted: usize,
}
