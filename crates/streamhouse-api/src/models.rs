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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    // =========================================================================
    // Topic
    // =========================================================================

    #[test]
    fn test_topic_serialize_roundtrip() {
        let topic = Topic {
            name: "orders".to_string(),
            partitions: 8,
            replication_factor: 3,
            created_at: "2026-01-15T00:00:00Z".to_string(),
            message_count: 42_000,
            size_bytes: 1_048_576,
        };
        let json = serde_json::to_string(&topic).unwrap();
        let deserialized: Topic = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "orders");
        assert_eq!(deserialized.partitions, 8);
        assert_eq!(deserialized.replication_factor, 3);
        assert_eq!(deserialized.created_at, "2026-01-15T00:00:00Z");
        assert_eq!(deserialized.message_count, 42_000);
        assert_eq!(deserialized.size_bytes, 1_048_576);
    }

    #[test]
    fn test_topic_serde_defaults_for_optional_fields() {
        // message_count and size_bytes have #[serde(default)]
        let json = r#"{"name":"events","partitions":4,"replication_factor":1,"created_at":"2026-01-01T00:00:00Z"}"#;
        let topic: Topic = serde_json::from_str(json).unwrap();
        assert_eq!(topic.message_count, 0);
        assert_eq!(topic.size_bytes, 0);
    }

    #[test]
    fn test_topic_with_zero_values() {
        let topic = Topic {
            name: "".to_string(),
            partitions: 0,
            replication_factor: 0,
            created_at: "".to_string(),
            message_count: 0,
            size_bytes: 0,
        };
        let json = serde_json::to_string(&topic).unwrap();
        let deserialized: Topic = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.partitions, 0);
        assert_eq!(deserialized.message_count, 0);
    }

    #[test]
    fn test_topic_with_large_values() {
        let topic = Topic {
            name: "high-throughput".to_string(),
            partitions: 256,
            replication_factor: 5,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            message_count: u64::MAX,
            size_bytes: u64::MAX,
        };
        let json = serde_json::to_string(&topic).unwrap();
        let deserialized: Topic = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message_count, u64::MAX);
        assert_eq!(deserialized.size_bytes, u64::MAX);
    }

    // =========================================================================
    // CreateTopicRequest
    // =========================================================================

    #[test]
    fn test_create_topic_request_with_replication_factor() {
        let json = r#"{"name":"orders","partitions":4,"replication_factor":3}"#;
        let req: CreateTopicRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "orders");
        assert_eq!(req.partitions, 4);
        assert_eq!(req.replication_factor, 3);
    }

    #[test]
    fn test_create_topic_request_default_replication_factor() {
        let json = r#"{"name":"events","partitions":2}"#;
        let req: CreateTopicRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.replication_factor, 1);
    }

    #[test]
    fn test_create_topic_request_serialize_roundtrip() {
        let req = CreateTopicRequest {
            name: "logs".to_string(),
            partitions: 16,
            replication_factor: 2,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CreateTopicRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "logs");
        assert_eq!(deserialized.partitions, 16);
        assert_eq!(deserialized.replication_factor, 2);
    }

    #[test]
    fn test_create_topic_request_missing_name_fails() {
        let json = r#"{"partitions":4}"#;
        let result = serde_json::from_str::<CreateTopicRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_topic_request_missing_partitions_fails() {
        let json = r#"{"name":"test"}"#;
        let result = serde_json::from_str::<CreateTopicRequest>(json);
        assert!(result.is_err());
    }

    // =========================================================================
    // Agent
    // =========================================================================

    #[test]
    fn test_agent_serialize_roundtrip() {
        let agent = Agent {
            agent_id: "agent-001".to_string(),
            address: "10.0.0.1:9092".to_string(),
            availability_zone: "us-east-1a".to_string(),
            agent_group: "default".to_string(),
            last_heartbeat: 1700000000,
            started_at: 1699900000,
            active_leases: 5,
        };
        let json = serde_json::to_string(&agent).unwrap();
        let deserialized: Agent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_id, "agent-001");
        assert_eq!(deserialized.address, "10.0.0.1:9092");
        assert_eq!(deserialized.availability_zone, "us-east-1a");
        assert_eq!(deserialized.agent_group, "default");
        assert_eq!(deserialized.last_heartbeat, 1700000000);
        assert_eq!(deserialized.started_at, 1699900000);
        assert_eq!(deserialized.active_leases, 5);
    }

    #[test]
    fn test_agent_missing_field_fails() {
        let json = r#"{"agent_id":"a1","address":"x","availability_zone":"z"}"#;
        let result = serde_json::from_str::<Agent>(json);
        assert!(result.is_err());
    }

    // =========================================================================
    // Partition
    // =========================================================================

    #[test]
    fn test_partition_serialize_roundtrip() {
        let partition = Partition {
            topic: "orders".to_string(),
            partition_id: 3,
            leader_agent_id: Some("agent-001".to_string()),
            high_watermark: 10_000,
            low_watermark: 500,
        };
        let json = serde_json::to_string(&partition).unwrap();
        let deserialized: Partition = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topic, "orders");
        assert_eq!(deserialized.partition_id, 3);
        assert_eq!(deserialized.leader_agent_id, Some("agent-001".to_string()));
        assert_eq!(deserialized.high_watermark, 10_000);
        assert_eq!(deserialized.low_watermark, 500);
    }

    #[test]
    fn test_partition_with_null_leader() {
        let json = r#"{"topic":"events","partition_id":0,"leader_agent_id":null,"high_watermark":0,"low_watermark":0}"#;
        let partition: Partition = serde_json::from_str(json).unwrap();
        assert_eq!(partition.leader_agent_id, None);
    }

    #[test]
    fn test_partition_leader_omitted() {
        let json = r#"{"topic":"events","partition_id":0,"high_watermark":0,"low_watermark":0}"#;
        let partition: Partition = serde_json::from_str(json).unwrap();
        assert_eq!(partition.leader_agent_id, None);
    }

    // =========================================================================
    // ConsumerGroup
    // =========================================================================

    #[test]
    fn test_consumer_group_serialize_roundtrip() {
        let cg = ConsumerGroup {
            group_id: "cg-analytics".to_string(),
            topic: "clicks".to_string(),
            members: 4,
            state: "Stable".to_string(),
            total_lag: 128,
        };
        let json = serde_json::to_string(&cg).unwrap();
        let deserialized: ConsumerGroup = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.group_id, "cg-analytics");
        assert_eq!(deserialized.topic, "clicks");
        assert_eq!(deserialized.members, 4);
        assert_eq!(deserialized.state, "Stable");
        assert_eq!(deserialized.total_lag, 128);
    }

    #[test]
    fn test_consumer_group_negative_lag() {
        let cg = ConsumerGroup {
            group_id: "cg-1".to_string(),
            topic: "t".to_string(),
            members: 0,
            state: "Empty".to_string(),
            total_lag: -1,
        };
        let json = serde_json::to_string(&cg).unwrap();
        let deserialized: ConsumerGroup = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_lag, -1);
    }

    // =========================================================================
    // ProduceRequest
    // =========================================================================

    #[test]
    fn test_produce_request_full() {
        let req = ProduceRequest {
            topic: "orders".to_string(),
            key: Some("user-123".to_string()),
            value: r#"{"item":"widget"}"#.to_string(),
            partition: Some(2),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ProduceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topic, "orders");
        assert_eq!(deserialized.key, Some("user-123".to_string()));
        assert_eq!(deserialized.value, r#"{"item":"widget"}"#);
        assert_eq!(deserialized.partition, Some(2));
    }

    #[test]
    fn test_produce_request_minimal() {
        let json = r#"{"topic":"events","value":"hello"}"#;
        let req: ProduceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.topic, "events");
        assert_eq!(req.key, None);
        assert_eq!(req.value, "hello");
        assert_eq!(req.partition, None);
    }

    #[test]
    fn test_produce_request_skip_serializing_none() {
        let req = ProduceRequest {
            topic: "t".to_string(),
            key: None,
            value: "v".to_string(),
            partition: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("key"));
        assert!(!json.contains("partition"));
    }

    // =========================================================================
    // ProduceResponse
    // =========================================================================

    #[test]
    fn test_produce_response_roundtrip() {
        let resp = ProduceResponse {
            offset: 42,
            partition: 1,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ProduceResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.offset, 42);
        assert_eq!(deserialized.partition, 1);
    }

    // =========================================================================
    // MetricsSnapshot
    // =========================================================================

    #[test]
    fn test_metrics_snapshot_roundtrip() {
        let snapshot = MetricsSnapshot {
            topics_count: 10,
            agents_count: 3,
            partitions_count: 40,
            total_messages: 1_000_000,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topics_count, 10);
        assert_eq!(deserialized.agents_count, 3);
        assert_eq!(deserialized.partitions_count, 40);
        assert_eq!(deserialized.total_messages, 1_000_000);
    }

    // =========================================================================
    // HealthResponse
    // =========================================================================

    #[test]
    fn test_health_response_roundtrip() {
        let health = HealthResponse {
            status: "ok".to_string(),
        };
        let json = serde_json::to_string(&health).unwrap();
        let deserialized: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, "ok");
    }

    // =========================================================================
    // ErrorResponse
    // =========================================================================

    #[test]
    fn test_error_response_roundtrip() {
        let err = ErrorResponse {
            error: "not_found".to_string(),
            message: "Topic not found".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        let deserialized: ErrorResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.error, "not_found");
        assert_eq!(deserialized.message, "Topic not found");
    }

    // =========================================================================
    // ConsumeResponse / ConsumedRecord (camelCase)
    // =========================================================================

    #[test]
    fn test_consume_response_camel_case() {
        let resp = ConsumeResponse {
            records: vec![ConsumedRecord {
                partition: 0,
                offset: 100,
                key: Some("k1".to_string()),
                value: "val".to_string(),
                timestamp: 1700000000,
            }],
            next_offset: 101,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("nextOffset"));
        assert!(!json.contains("next_offset"));
    }

    #[test]
    fn test_consume_response_roundtrip() {
        let resp = ConsumeResponse {
            records: vec![
                ConsumedRecord {
                    partition: 0,
                    offset: 10,
                    key: None,
                    value: "hello".to_string(),
                    timestamp: 1700000000,
                },
                ConsumedRecord {
                    partition: 1,
                    offset: 20,
                    key: Some("key".to_string()),
                    value: "world".to_string(),
                    timestamp: 1700000001,
                },
            ],
            next_offset: 21,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ConsumeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.records.len(), 2);
        assert_eq!(deserialized.next_offset, 21);
        assert_eq!(deserialized.records[0].key, None);
        assert_eq!(deserialized.records[1].key, Some("key".to_string()));
    }

    #[test]
    fn test_consume_response_empty_records() {
        let resp = ConsumeResponse {
            records: vec![],
            next_offset: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ConsumeResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.records.is_empty());
    }

    // =========================================================================
    // ConsumerGroupInfo (camelCase)
    // =========================================================================

    #[test]
    fn test_consumer_group_info_camel_case() {
        let info = ConsumerGroupInfo {
            group_id: "g1".to_string(),
            topics: vec!["t1".to_string(), "t2".to_string()],
            total_lag: 50,
            partition_count: 8,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("groupId"));
        assert!(json.contains("totalLag"));
        assert!(json.contains("partitionCount"));
    }

    #[test]
    fn test_consumer_group_info_roundtrip() {
        let info = ConsumerGroupInfo {
            group_id: "analytics".to_string(),
            topics: vec!["orders".to_string(), "clicks".to_string()],
            total_lag: 1000,
            partition_count: 16,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ConsumerGroupInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.group_id, "analytics");
        assert_eq!(deserialized.topics.len(), 2);
        assert_eq!(deserialized.total_lag, 1000);
        assert_eq!(deserialized.partition_count, 16);
    }

    // =========================================================================
    // ConsumerGroupDetail / ConsumerOffsetInfo (camelCase)
    // =========================================================================

    #[test]
    fn test_consumer_group_detail_roundtrip() {
        let detail = ConsumerGroupDetail {
            group_id: "g1".to_string(),
            offsets: vec![ConsumerOffsetInfo {
                topic: "orders".to_string(),
                partition_id: 0,
                committed_offset: 500,
                high_watermark: 600,
                lag: 100,
            }],
        };
        let json = serde_json::to_string(&detail).unwrap();
        let deserialized: ConsumerGroupDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.group_id, "g1");
        assert_eq!(deserialized.offsets.len(), 1);
        assert_eq!(deserialized.offsets[0].committed_offset, 500);
        assert_eq!(deserialized.offsets[0].lag, 100);
    }

    #[test]
    fn test_consumer_offset_info_camel_case() {
        let info = ConsumerOffsetInfo {
            topic: "t".to_string(),
            partition_id: 0,
            committed_offset: 10,
            high_watermark: 20,
            lag: 10,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("partitionId"));
        assert!(json.contains("committedOffset"));
        assert!(json.contains("highWatermark"));
    }

    // =========================================================================
    // ConsumerGroupLag (camelCase)
    // =========================================================================

    #[test]
    fn test_consumer_group_lag_roundtrip() {
        let lag = ConsumerGroupLag {
            group_id: "g1".to_string(),
            total_lag: 500,
            partition_count: 4,
            topics: vec!["orders".to_string()],
        };
        let json = serde_json::to_string(&lag).unwrap();
        let deserialized: ConsumerGroupLag = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.group_id, "g1");
        assert_eq!(deserialized.total_lag, 500);
        assert_eq!(deserialized.partition_count, 4);
        assert_eq!(deserialized.topics, vec!["orders"]);
    }

    // =========================================================================
    // StorageMetricsResponse (camelCase + HashMap)
    // =========================================================================

    #[test]
    fn test_storage_metrics_response_roundtrip() {
        let mut storage_by_topic = std::collections::HashMap::new();
        storage_by_topic.insert("orders".to_string(), 1024u64);
        storage_by_topic.insert("events".to_string(), 2048u64);

        let resp = StorageMetricsResponse {
            total_size_bytes: 3072,
            segment_count: 10,
            storage_by_topic,
            cache_size: 512,
            cache_hit_rate: 0.95,
            cache_evictions: 3,
            wal_size: 100,
            wal_uncommitted_entries: 2,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("totalSizeBytes"));
        assert!(json.contains("segmentCount"));
        assert!(json.contains("storageByTopic"));
        assert!(json.contains("cacheHitRate"));
        assert!(json.contains("walUncommittedEntries"));

        let deserialized: StorageMetricsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_size_bytes, 3072);
        assert_eq!(deserialized.storage_by_topic.len(), 2);
        assert_eq!(deserialized.storage_by_topic["orders"], 1024);
        assert!((deserialized.cache_hit_rate - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_storage_metrics_empty_topic_map() {
        let resp = StorageMetricsResponse {
            total_size_bytes: 0,
            segment_count: 0,
            storage_by_topic: std::collections::HashMap::new(),
            cache_size: 0,
            cache_hit_rate: 0.0,
            cache_evictions: 0,
            wal_size: 0,
            wal_uncommitted_entries: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: StorageMetricsResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.storage_by_topic.is_empty());
    }

    // =========================================================================
    // ThroughputMetric (camelCase)
    // =========================================================================

    #[test]
    fn test_throughput_metric_camel_case_and_roundtrip() {
        let metric = ThroughputMetric {
            timestamp: 1700000000,
            messages_per_second: 5432.1,
            bytes_per_second: 1_234_567.89,
        };
        let json = serde_json::to_string(&metric).unwrap();
        assert!(json.contains("messagesPerSecond"));
        assert!(json.contains("bytesPerSecond"));

        let deserialized: ThroughputMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.timestamp, 1700000000);
        assert!((deserialized.messages_per_second - 5432.1).abs() < f64::EPSILON);
    }

    // =========================================================================
    // LatencyMetric (camelCase)
    // =========================================================================

    #[test]
    fn test_latency_metric_camel_case_and_roundtrip() {
        let metric = LatencyMetric {
            timestamp: 1700000000,
            p50: 1.5,
            p95: 10.2,
            p99: 50.0,
            avg: 3.7,
        };
        let json = serde_json::to_string(&metric).unwrap();
        // LatencyMetric fields are short enough to remain lowercase under camelCase
        let deserialized: LatencyMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.timestamp, 1700000000);
        assert!((deserialized.p50 - 1.5).abs() < f64::EPSILON);
        assert!((deserialized.p95 - 10.2).abs() < f64::EPSILON);
        assert!((deserialized.p99 - 50.0).abs() < f64::EPSILON);
        assert!((deserialized.avg - 3.7).abs() < f64::EPSILON);
    }

    // =========================================================================
    // ErrorMetric (camelCase)
    // =========================================================================

    #[test]
    fn test_error_metric_camel_case_and_roundtrip() {
        let metric = ErrorMetric {
            timestamp: 1700000000,
            error_rate: 0.02,
            error_count: 15,
        };
        let json = serde_json::to_string(&metric).unwrap();
        assert!(json.contains("errorRate"));
        assert!(json.contains("errorCount"));

        let deserialized: ErrorMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.error_count, 15);
        assert!((deserialized.error_rate - 0.02).abs() < f64::EPSILON);
    }

    // =========================================================================
    // AgentMetricsResponse (camelCase)
    // =========================================================================

    #[test]
    fn test_agent_metrics_response_camel_case_and_roundtrip() {
        let resp = AgentMetricsResponse {
            agent_id: "agent-1".to_string(),
            address: "10.0.0.1:9092".to_string(),
            availability_zone: "us-west-2a".to_string(),
            partition_count: 12,
            last_heartbeat: 1700000000,
            uptime_ms: 86_400_000,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("agentId"));
        assert!(json.contains("availabilityZone"));
        assert!(json.contains("partitionCount"));
        assert!(json.contains("lastHeartbeat"));
        assert!(json.contains("uptimeMs"));

        let deserialized: AgentMetricsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_id, "agent-1");
        assert_eq!(deserialized.partition_count, 12);
        assert_eq!(deserialized.uptime_ms, 86_400_000);
    }

    // =========================================================================
    // TimeRangeParams (Deserialize-only)
    // =========================================================================

    #[test]
    fn test_time_range_params_with_value() {
        let json = r#"{"time_range":"5m"}"#;
        let params: TimeRangeParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.time_range, Some("5m".to_string()));
    }

    #[test]
    fn test_time_range_params_empty() {
        let json = r#"{}"#;
        let params: TimeRangeParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.time_range, None);
    }

    #[test]
    fn test_time_range_params_null_value() {
        let json = r#"{"time_range":null}"#;
        let params: TimeRangeParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.time_range, None);
    }

    // =========================================================================
    // MessageQueryParams (Deserialize-only)
    // =========================================================================

    #[test]
    fn test_message_query_params_full() {
        let json = r#"{"partition":2,"offset":500,"limit":100}"#;
        let params: MessageQueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.partition, Some(2));
        assert_eq!(params.offset, Some(500));
        assert_eq!(params.limit, Some(100));
    }

    #[test]
    fn test_message_query_params_empty() {
        let json = r#"{}"#;
        let params: MessageQueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.partition, None);
        assert_eq!(params.offset, None);
        assert_eq!(params.limit, None);
    }

    #[test]
    fn test_message_query_params_partial() {
        let json = r#"{"limit":50}"#;
        let params: MessageQueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.partition, None);
        assert_eq!(params.offset, None);
        assert_eq!(params.limit, Some(50));
    }

    // =========================================================================
    // CommitOffsetRequest / CommitOffsetResponse (camelCase)
    // =========================================================================

    #[test]
    fn test_commit_offset_request_camel_case() {
        let json = r#"{"groupId":"g1","topic":"orders","partition":0,"offset":100}"#;
        let req: CommitOffsetRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.group_id, "g1");
        assert_eq!(req.topic, "orders");
        assert_eq!(req.partition, 0);
        assert_eq!(req.offset, 100);
    }

    #[test]
    fn test_commit_offset_request_roundtrip() {
        let req = CommitOffsetRequest {
            group_id: "cg-1".to_string(),
            topic: "events".to_string(),
            partition: 3,
            offset: 999,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CommitOffsetRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.group_id, "cg-1");
        assert_eq!(deserialized.offset, 999);
    }

    #[test]
    fn test_commit_offset_response_roundtrip() {
        let resp = CommitOffsetResponse { success: true };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: CommitOffsetResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.success);

        let resp_false = CommitOffsetResponse { success: false };
        let json = serde_json::to_string(&resp_false).unwrap();
        let deserialized: CommitOffsetResponse = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.success);
    }

    // =========================================================================
    // BatchProduceRequest / BatchRecord / BatchProduceResponse
    // =========================================================================

    #[test]
    fn test_batch_produce_request_roundtrip() {
        let req = BatchProduceRequest {
            topic: "orders".to_string(),
            records: vec![
                BatchRecord {
                    key: Some("k1".to_string()),
                    value: "v1".to_string(),
                    partition: Some(0),
                },
                BatchRecord {
                    key: None,
                    value: "v2".to_string(),
                    partition: None,
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: BatchProduceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topic, "orders");
        assert_eq!(deserialized.records.len(), 2);
        assert_eq!(deserialized.records[0].key, Some("k1".to_string()));
        assert_eq!(deserialized.records[1].key, None);
    }

    #[test]
    fn test_batch_record_skip_serializing_none() {
        let record = BatchRecord {
            key: None,
            value: "v".to_string(),
            partition: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(!json.contains("key"));
        assert!(!json.contains("partition"));
        assert!(json.contains("value"));
    }

    #[test]
    fn test_batch_produce_response_roundtrip() {
        let resp = BatchProduceResponse {
            count: 3,
            offsets: vec![
                BatchRecordResult {
                    partition: 0,
                    offset: 100,
                },
                BatchRecordResult {
                    partition: 0,
                    offset: 101,
                },
                BatchRecordResult {
                    partition: 1,
                    offset: 50,
                },
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: BatchProduceResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.count, 3);
        assert_eq!(deserialized.offsets.len(), 3);
        assert_eq!(deserialized.offsets[2].partition, 1);
        assert_eq!(deserialized.offsets[2].offset, 50);
    }

    // =========================================================================
    // ResetStrategy enum
    // =========================================================================

    #[test]
    fn test_reset_strategy_serialize_lowercase() {
        let earliest = ResetStrategy::Earliest;
        let json = serde_json::to_string(&earliest).unwrap();
        assert_eq!(json, r#""earliest""#);

        let latest = ResetStrategy::Latest;
        let json = serde_json::to_string(&latest).unwrap();
        assert_eq!(json, r#""latest""#);

        let specific = ResetStrategy::Specific;
        let json = serde_json::to_string(&specific).unwrap();
        assert_eq!(json, r#""specific""#);

        let timestamp = ResetStrategy::Timestamp;
        let json = serde_json::to_string(&timestamp).unwrap();
        assert_eq!(json, r#""timestamp""#);
    }

    #[test]
    fn test_reset_strategy_deserialize_lowercase() {
        let earliest: ResetStrategy = serde_json::from_str(r#""earliest""#).unwrap();
        assert!(matches!(earliest, ResetStrategy::Earliest));

        let latest: ResetStrategy = serde_json::from_str(r#""latest""#).unwrap();
        assert!(matches!(latest, ResetStrategy::Latest));

        let specific: ResetStrategy = serde_json::from_str(r#""specific""#).unwrap();
        assert!(matches!(specific, ResetStrategy::Specific));

        let timestamp: ResetStrategy = serde_json::from_str(r#""timestamp""#).unwrap();
        assert!(matches!(timestamp, ResetStrategy::Timestamp));
    }

    #[test]
    fn test_reset_strategy_invalid_value() {
        let result = serde_json::from_str::<ResetStrategy>(r#""invalid""#);
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_strategy_case_sensitive() {
        // rename_all = "lowercase" so "Earliest" should fail
        let result = serde_json::from_str::<ResetStrategy>(r#""Earliest""#);
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_strategy_clone() {
        let strategy = ResetStrategy::Latest;
        let cloned = strategy.clone();
        assert!(matches!(cloned, ResetStrategy::Latest));
    }

    // =========================================================================
    // ResetOffsetsRequest (camelCase, Deserialize-only)
    // =========================================================================

    #[test]
    fn test_reset_offsets_request_earliest() {
        let json = r#"{"strategy":"earliest"}"#;
        let req: ResetOffsetsRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.strategy, ResetStrategy::Earliest));
        assert_eq!(req.topic, None);
        assert_eq!(req.partition, None);
        assert_eq!(req.offset, None);
        assert_eq!(req.timestamp, None);
    }

    #[test]
    fn test_reset_offsets_request_specific_with_offset() {
        let json = r#"{"strategy":"specific","topic":"orders","partition":2,"offset":500}"#;
        let req: ResetOffsetsRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.strategy, ResetStrategy::Specific));
        assert_eq!(req.topic, Some("orders".to_string()));
        assert_eq!(req.partition, Some(2));
        assert_eq!(req.offset, Some(500));
    }

    #[test]
    fn test_reset_offsets_request_timestamp_with_ts() {
        let json = r#"{"strategy":"timestamp","topic":"events","timestamp":1700000000000}"#;
        let req: ResetOffsetsRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.strategy, ResetStrategy::Timestamp));
        assert_eq!(req.timestamp, Some(1700000000000));
    }

    // =========================================================================
    // ResetOffsetsResponse (camelCase, Serialize-only)
    // =========================================================================

    #[test]
    fn test_reset_offsets_response_serialize() {
        let resp = ResetOffsetsResponse {
            success: true,
            partitions_reset: 4,
            details: vec![
                ResetOffsetDetail {
                    topic: "orders".to_string(),
                    partition: 0,
                    old_offset: 100,
                    new_offset: 0,
                },
                ResetOffsetDetail {
                    topic: "orders".to_string(),
                    partition: 1,
                    old_offset: 200,
                    new_offset: 0,
                },
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("partitionsReset"));
        assert!(json.contains("oldOffset"));
        assert!(json.contains("newOffset"));

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["partitionsReset"], 4);
        assert_eq!(parsed["details"].as_array().unwrap().len(), 2);
    }

    // =========================================================================
    // SeekToTimestampRequest (camelCase, Deserialize-only)
    // =========================================================================

    #[test]
    fn test_seek_to_timestamp_request() {
        let json = r#"{"topic":"orders","partition":2,"timestamp":1700000000000}"#;
        let req: SeekToTimestampRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.topic, "orders");
        assert_eq!(req.partition, Some(2));
        assert_eq!(req.timestamp, 1700000000000);
    }

    #[test]
    fn test_seek_to_timestamp_request_all_partitions() {
        let json = r#"{"topic":"events","timestamp":1700000000000}"#;
        let req: SeekToTimestampRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.topic, "events");
        assert_eq!(req.partition, None);
    }

    #[test]
    fn test_seek_to_timestamp_request_missing_timestamp_fails() {
        let json = r#"{"topic":"events"}"#;
        let result = serde_json::from_str::<SeekToTimestampRequest>(json);
        assert!(result.is_err());
    }

    // =========================================================================
    // SeekToTimestampResponse / SeekOffsetDetail (camelCase, Serialize-only)
    // =========================================================================

    #[test]
    fn test_seek_to_timestamp_response_serialize() {
        let resp = SeekToTimestampResponse {
            success: true,
            partitions_updated: 2,
            details: vec![SeekOffsetDetail {
                topic: "orders".to_string(),
                partition: 0,
                old_offset: 100,
                new_offset: 75,
                timestamp_found: 1699999999000,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("partitionsUpdated"));
        assert!(json.contains("timestampFound"));
        assert!(json.contains("oldOffset"));
        assert!(json.contains("newOffset"));

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["partitionsUpdated"], 2);
    }

    // =========================================================================
    // DeleteConsumerGroupResponse (camelCase, Serialize-only)
    // =========================================================================

    #[test]
    fn test_delete_consumer_group_response_serialize() {
        let resp = DeleteConsumerGroupResponse {
            success: true,
            group_id: "cg-analytics".to_string(),
            partitions_deleted: 8,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("groupId"));
        assert!(json.contains("partitionsDeleted"));

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["groupId"], "cg-analytics");
        assert_eq!(parsed["partitionsDeleted"], 8);
    }

    #[test]
    fn test_delete_consumer_group_response_failure() {
        let resp = DeleteConsumerGroupResponse {
            success: false,
            group_id: "nonexistent".to_string(),
            partitions_deleted: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["partitionsDeleted"], 0);
    }

    // =========================================================================
    // Cross-model JSON value interop tests
    // =========================================================================

    #[test]
    fn test_produce_request_with_json_value() {
        let req = ProduceRequest {
            topic: "events".to_string(),
            key: Some("user-42".to_string()),
            value: serde_json::json!({"event": "click", "page": "/home"}).to_string(),
            partition: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ProduceRequest = serde_json::from_str(&json).unwrap();
        // The value can be parsed back as JSON
        let inner: serde_json::Value = serde_json::from_str(&deserialized.value).unwrap();
        assert_eq!(inner["event"], "click");
        assert_eq!(inner["page"], "/home");
    }

    #[test]
    fn test_models_reject_wrong_types() {
        // Passing a string where u32 is expected
        let json = r#"{"name":"test","partitions":"not_a_number","replication_factor":1}"#;
        let result = serde_json::from_str::<CreateTopicRequest>(json);
        assert!(result.is_err());

        // Passing a negative number where u64 is expected
        let json = r#"{"offset":-1,"partition":0}"#;
        let result = serde_json::from_str::<ProduceResponse>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_extra_fields_are_ignored() {
        // Serde by default ignores extra fields
        let json = r#"{"status":"ok","extra_field":"ignored","another":42}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ok");
    }

    // =========================================================================
    // Unicode and special character handling
    // =========================================================================

    #[test]
    fn test_topic_unicode_name() {
        let topic = Topic {
            name: "datos-espanoles-\u{00e9}".to_string(),
            partitions: 1,
            replication_factor: 1,
            created_at: "2026-01-01".to_string(),
            message_count: 0,
            size_bytes: 0,
        };
        let json = serde_json::to_string(&topic).unwrap();
        let deserialized: Topic = serde_json::from_str(&json).unwrap();
        assert!(deserialized.name.contains('\u{00e9}'));
    }

    #[test]
    fn test_produce_request_with_special_chars_in_value() {
        let req = ProduceRequest {
            topic: "t".to_string(),
            key: None,
            value: r#"{"msg":"hello \"world\"\nnewline"}"#.to_string(),
            partition: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ProduceRequest = serde_json::from_str(&json).unwrap();
        assert!(deserialized.value.contains("hello \\\"world\\\""));
    }
}
