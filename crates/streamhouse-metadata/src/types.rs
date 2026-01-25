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

/// Configuration for creating a new topic.
///
/// This struct is passed to `MetadataStore::create_topic()` to define a new topic's
/// configuration. All fields are required except retention_ms.
///
/// # Fields
///
/// * `name` - Topic name (must be unique, alphanumeric with hyphens/underscores recommended)
/// * `partition_count` - Number of partitions (must be > 0, cannot be changed after creation)
/// * `retention_ms` - Optional retention period in milliseconds (None = infinite retention)
/// * `config` - Additional configuration parameters (flexible key-value pairs for future extensions)
///
/// # Examples
///
/// ```ignore
/// use streamhouse_metadata::TopicConfig;
/// use std::collections::HashMap;
///
/// // Topic with 3 partitions, 7-day retention
/// let config = TopicConfig {
///     name: "orders".to_string(),
///     partition_count: 3,
///     retention_ms: Some(7 * 24 * 60 * 60 * 1000), // 7 days
///     config: HashMap::new(),
/// };
///
/// // Topic with infinite retention
/// let config = TopicConfig {
///     name: "audit_log".to_string(),
///     partition_count: 1,
///     retention_ms: None, // Keep forever
///     config: HashMap::new(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    /// Topic name (unique identifier)
    pub name: String,

    /// Number of partitions (determines parallelism)
    pub partition_count: u32,

    /// Retention period in milliseconds (None = infinite)
    pub retention_ms: Option<i64>,

    /// Additional configuration parameters
    pub config: HashMap<String, String>,
}

/// Information about an existing topic.
///
/// This is returned by `MetadataStore::get_topic()` and `list_topics()`.
/// It includes all the configuration from `TopicConfig` plus the creation timestamp.
///
/// # Fields
///
/// * `name` - Topic name
/// * `partition_count` - Number of partitions
/// * `retention_ms` - Retention period in milliseconds (None = infinite)
/// * `created_at` - Topic creation timestamp (milliseconds since Unix epoch)
/// * `config` - Additional configuration parameters
///
/// # Examples
///
/// ```ignore
/// let topic = store.get_topic("orders").await?.unwrap();
/// println!("Topic {} has {} partitions", topic.name, topic.partition_count);
/// println!("Created at: {}", topic.created_at);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    /// Topic name
    pub name: String,

    /// Number of partitions
    pub partition_count: u32,

    /// Retention period in milliseconds (None = infinite)
    pub retention_ms: Option<i64>,

    /// Creation timestamp (milliseconds since Unix epoch)
    pub created_at: i64,

    /// Additional configuration parameters
    pub config: HashMap<String, String>,
}

/// Information about a partition within a topic.
///
/// Partitions are the unit of parallelism in StreamHouse. Each partition is an ordered,
/// append-only log with a monotonically increasing offset.
///
/// # Fields
///
/// * `topic` - Parent topic name
/// * `partition_id` - Partition ID (0-indexed, range: [0, partition_count))
/// * `high_watermark` - Next offset to be assigned (latest offset + 1)
///
/// # High Watermark Semantics
///
/// The high watermark is the **next** offset that will be assigned to a new record,
/// not the last written offset. If records 0-99 have been written, high_watermark = 100.
///
/// # Examples
///
/// ```ignore
/// let partition = store.get_partition("orders", 0).await?.unwrap();
/// println!("Partition {}/{} has {} records (offsets 0-{})",
///     partition.topic,
///     partition.partition_id,
///     partition.high_watermark,
///     partition.high_watermark - 1
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    /// Parent topic name
    pub topic: String,

    /// Partition ID (0-indexed)
    pub partition_id: u32,

    /// Next offset to be assigned (high watermark)
    pub high_watermark: u64,
}

/// Metadata about a segment file stored in S3.
///
/// Segments are immutable files containing a contiguous range of records for a partition.
/// This metadata enables consumers to find which S3 file contains a given offset.
///
/// # Fields
///
/// * `id` - Unique segment identifier (e.g., "orders-0-1000")
/// * `topic` - Parent topic name
/// * `partition_id` - Partition ID
/// * `base_offset` - First offset in this segment (inclusive)
/// * `end_offset` - Last offset in this segment (inclusive)
/// * `record_count` - Number of records in the segment
/// * `size_bytes` - Segment file size in bytes
/// * `s3_bucket` - S3 bucket name (e.g., "streamhouse")
/// * `s3_key` - S3 object key (e.g., "orders/0/seg_1000.bin")
/// * `created_at` - Segment creation timestamp (milliseconds since Unix epoch)
///
/// # Offset Range
///
/// The offset range [base_offset, end_offset] is **inclusive** on both ends.
/// For example, if base_offset=1000 and end_offset=1999, the segment contains
/// offsets 1000, 1001, ..., 1999 (2000 records total if record_count=2000).
///
/// # Examples
///
/// ```ignore
/// use streamhouse_metadata::SegmentInfo;
///
/// let segment = SegmentInfo {
///     id: "orders-0-1000".to_string(),
///     topic: "orders".to_string(),
///     partition_id: 0,
///     base_offset: 1000,
///     end_offset: 1999,
///     record_count: 1000,
///     size_bytes: 67_108_864,  // 64MB
///     s3_bucket: "streamhouse".to_string(),
///     s3_key: "orders/0/seg_1000.bin".to_string(),
///     created_at: now_ms(),
/// };
///
/// // To read this segment from S3:
/// // aws s3 cp s3://streamhouse/orders/0/seg_1000.bin /tmp/segment.bin
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentInfo {
    /// Unique segment identifier
    pub id: String,

    /// Parent topic name
    pub topic: String,

    /// Partition ID
    pub partition_id: u32,

    /// First offset in segment (inclusive)
    pub base_offset: u64,

    /// Last offset in segment (inclusive)
    pub end_offset: u64,

    /// Number of records in segment
    pub record_count: u32,

    /// Segment file size in bytes
    pub size_bytes: u64,

    /// S3 bucket name
    pub s3_bucket: String,

    /// S3 object key
    pub s3_key: String,

    /// Creation timestamp (milliseconds since Unix epoch)
    pub created_at: i64,
}

/// Consumer group offset tracking.
///
/// Tracks where a consumer group left off for a specific topic partition.
/// This allows consumers to resume from their last position after a restart.
///
/// # Fields
///
/// * `group_id` - Consumer group identifier (e.g., "analytics-team")
/// * `topic` - Topic name
/// * `partition_id` - Partition ID
/// * `committed_offset` - Last committed offset (next offset to consume)
/// * `metadata` - Optional application-specific metadata (e.g., checkpoint info, consumer state)
///
/// # Offset Semantics
///
/// `committed_offset` represents the **next** offset to consume, not the last consumed offset.
/// If you've successfully processed offsets 0-99, commit offset 100.
///
/// # Examples
///
/// ```ignore
/// // Get all offsets for a consumer group
/// let offsets = store.get_consumer_offsets("analytics").await?;
/// for offset in offsets {
///     println!("Consumer {} should resume from {}/{} at offset {}",
///         offset.group_id,
///         offset.topic,
///         offset.partition_id,
///         offset.committed_offset
///     );
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerOffset {
    /// Consumer group identifier
    pub group_id: String,

    /// Topic name
    pub topic: String,

    /// Partition ID
    pub partition_id: u32,

    /// Next offset to consume
    pub committed_offset: u64,

    /// Optional application-specific metadata
    pub metadata: Option<String>,
}

/// Agent registration information (Phase 4).
///
/// Represents a stateless StreamHouse agent that can handle produce/consume requests.
/// Agents register themselves on startup and send periodic heartbeats to maintain liveness.
///
/// # Lifecycle
///
/// 1. **Startup**: Agent calls `register_agent()` with its address and zone
/// 2. **Heartbeat**: Every 30s, agent calls `register_agent()` to update last_heartbeat
/// 3. **Liveness**: Agents with heartbeat > 60s ago are considered dead
/// 4. **Shutdown**: Agent optionally calls `deregister_agent()` for graceful shutdown
///
/// # Fields
///
/// * `agent_id` - Unique agent identifier (e.g., "agent-us-east-1a-001")
/// * `address` - Agent's gRPC address (e.g., "10.0.1.5:9090")
/// * `availability_zone` - Cloud availability zone (e.g., "us-east-1a")
/// * `agent_group` - Agent group for network isolation (e.g., "prod", "staging")
/// * `last_heartbeat` - Last heartbeat timestamp (milliseconds since Unix epoch)
/// * `started_at` - Agent startup timestamp (milliseconds since Unix epoch)
/// * `metadata` - Additional metadata (version, capabilities, config, etc.)
///
/// # Examples
///
/// ```ignore
/// use streamhouse_metadata::AgentInfo;
/// use std::collections::HashMap;
///
/// let agent = AgentInfo {
///     agent_id: "agent-us-east-1a-001".to_string(),
///     address: "10.0.1.5:9090".to_string(),
///     availability_zone: "us-east-1a".to_string(),
///     agent_group: "prod".to_string(),
///     last_heartbeat: now_ms(),
///     started_at: now_ms(),
///     metadata: HashMap::from([
///         ("version".to_string(), "1.0.0".to_string()),
///         ("hostname".to_string(), "ip-10-0-1-5".to_string()),
///     ]),
/// };
///
/// store.register_agent(agent).await?;
/// ```
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

    /// Last heartbeat timestamp (milliseconds since Unix epoch)
    pub last_heartbeat: i64,

    /// Agent startup timestamp (milliseconds since Unix epoch)
    pub started_at: i64,

    /// Additional metadata (version, capabilities, etc.)
    pub metadata: HashMap<String, String>,
}

/// Partition leadership lease (Phase 4).
///
/// Represents which agent is currently the leader for a partition.
/// Uses lease-based leadership with automatic expiration for fault tolerance.
///
/// # Lease-Based Leadership
///
/// StreamHouse uses leases to coordinate which agent can write to each partition:
/// - Only the agent holding the lease can write to the partition
/// - Leases automatically expire if not renewed (typically 30-60 seconds)
/// - If an agent crashes, its leases expire and others can take over
/// - Prevents split-brain scenarios (two agents writing to same partition)
///
/// # Epoch
///
/// The epoch increments every time leadership changes. This can be used to:
/// - Detect stale writes from old leaders
/// - Implement fencing tokens to prevent zombie agents
/// - Track leadership stability for monitoring
///
/// # Fields
///
/// * `topic` - Topic name
/// * `partition_id` - Partition ID
/// * `leader_agent_id` - Current leader agent ID
/// * `lease_expires_at` - Lease expiration timestamp (milliseconds since Unix epoch)
/// * `acquired_at` - When this lease was first acquired (milliseconds since Unix epoch)
/// * `epoch` - Lease epoch (increments on each leadership change)
///
/// # Examples
///
/// ```ignore
/// // Agent acquires lease
/// let lease = store.acquire_partition_lease(
///     "orders",
///     0,
///     "agent-001",
///     30_000, // 30 second lease
/// ).await?;
///
/// println!("Acquired lease for {}/{} with epoch {}",
///     lease.topic, lease.partition_id, lease.epoch);
///
/// // Agent must renew before lease_expires_at
/// // Otherwise another agent can take over
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionLease {
    /// Topic name
    pub topic: String,

    /// Partition ID
    pub partition_id: u32,

    /// Current leader agent ID
    pub leader_agent_id: String,

    /// Lease expiration timestamp (milliseconds since Unix epoch)
    pub lease_expires_at: i64,

    /// Lease acquisition timestamp (milliseconds since Unix epoch)
    pub acquired_at: i64,

    /// Lease epoch (increments on each leadership change)
    pub epoch: u64,
}
