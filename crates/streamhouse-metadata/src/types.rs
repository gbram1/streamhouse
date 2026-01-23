//! Metadata Type Definitions
//!
//! This module defines all the data structures used in the metadata store.
//!
//! ## Types Overview
//!
//! ### TopicConfig
//! Configuration for creating a new topic. Specifies how many partitions,
//! retention period, and other settings.
//!
//! ### Topic
//! Complete information about an existing topic, including when it was created.
//!
//! ### Partition
//! Information about a single partition within a topic. Tracks the high watermark
//! (latest offset written).
//!
//! ### SegmentInfo
//! Metadata about a segment file in S3. Contains the offset range it covers,
//! S3 location, size, and record count.
//!
//! ### ConsumerOffset
//! Tracks where a consumer group left off for a specific topic partition.
//! Allows consumers to resume from their last position after a restart.
//!
//! ## Design Decisions
//!
//! - All types are Serialize/Deserialize for storage and API responses
//! - Timestamps are i64 (milliseconds since epoch) for simplicity
//! - Offsets are u64 to support very large streams (18 quintillion records)
//! - Config is HashMap<String, String> for flexibility

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for creating a new topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    pub name: String,
    pub partition_count: u32,
    pub retention_ms: Option<i64>,
    pub config: HashMap<String, String>,
}

/// Information about an existing topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub name: String,
    pub partition_count: u32,
    pub retention_ms: Option<i64>,
    pub created_at: i64,
    pub config: HashMap<String, String>,
}

/// Information about a partition within a topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    pub topic: String,
    pub partition_id: u32,
    pub high_watermark: u64,
}

/// Metadata about a segment file stored in S3
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentInfo {
    pub id: String,
    pub topic: String,
    pub partition_id: u32,
    pub base_offset: u64,
    pub end_offset: u64,
    pub record_count: u32,
    pub size_bytes: u64,
    pub s3_bucket: String,
    pub s3_key: String,
    pub created_at: i64,
}

/// Consumer group offset tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerOffset {
    pub group_id: String,
    pub topic: String,
    pub partition_id: u32,
    pub committed_offset: u64,
    pub metadata: Option<String>,
}

/// Agent registration information (Phase 4)
///
/// Represents a stateless StreamHouse agent that can handle produce/consume requests.
/// Agents register themselves on startup and send periodic heartbeats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Unique agent identifier (e.g., "agent-us-east-1a-001")
    pub agent_id: String,

    /// Agent's gRPC address (e.g., "10.0.1.5:9090")
    pub address: String,

    /// Availability zone (e.g., "us-east-1a")
    pub availability_zone: String,

    /// Agent group for network isolation (e.g., "prod", "staging")
    pub agent_group: String,

    /// Last heartbeat timestamp (ms since epoch)
    pub last_heartbeat: i64,

    /// Agent startup timestamp (ms since epoch)
    pub started_at: i64,

    /// Additional metadata (version, capabilities, etc.)
    pub metadata: HashMap<String, String>,
}

/// Partition leadership lease (Phase 4)
///
/// Represents which agent is currently the leader for a partition.
/// Uses lease-based leadership with automatic expiration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionLease {
    /// Topic name
    pub topic: String,

    /// Partition ID
    pub partition_id: u32,

    /// Current leader agent ID
    pub leader_agent_id: String,

    /// Lease expiration timestamp (ms since epoch)
    pub lease_expires_at: i64,

    /// Lease acquisition timestamp (ms since epoch)
    pub acquired_at: i64,

    /// Lease epoch (increments on each leadership change)
    pub epoch: u64,
}
