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

// ============================================================================
// MULTI-TENANCY TYPES (Phase 21.5)
// ============================================================================

/// Organization/tenant account for multi-tenancy.
///
/// Organizations are the root of the tenant hierarchy. All other resources
/// (topics, partitions, consumer groups) belong to an organization.
///
/// # Fields
///
/// * `id` - Unique organization ID (UUID format)
/// * `name` - Human-readable organization name
/// * `slug` - URL-friendly identifier (lowercase alphanumeric with hyphens)
/// * `plan` - Billing plan (free, pro, enterprise)
/// * `status` - Account status (active, suspended, deleted)
/// * `created_at` - Creation timestamp (milliseconds since Unix epoch)
/// * `settings` - Organization-specific settings
///
/// # Examples
///
/// ```ignore
/// use streamhouse_metadata::Organization;
///
/// let org = Organization {
///     id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
///     name: "Acme Corporation".to_string(),
///     slug: "acme-corp".to_string(),
///     plan: OrganizationPlan::Pro,
///     status: OrganizationStatus::Active,
///     created_at: now_ms(),
///     settings: HashMap::new(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    /// Unique organization ID (UUID format)
    pub id: String,

    /// Human-readable organization name
    pub name: String,

    /// URL-friendly identifier (lowercase alphanumeric with hyphens)
    pub slug: String,

    /// Billing plan
    pub plan: OrganizationPlan,

    /// Account status
    pub status: OrganizationStatus,

    /// Creation timestamp (milliseconds since Unix epoch)
    pub created_at: i64,

    /// Organization-specific settings
    pub settings: HashMap<String, String>,
}

/// Organization billing plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrganizationPlan {
    /// Free tier with limited resources
    Free,
    /// Professional tier with higher limits
    Pro,
    /// Enterprise tier with custom limits
    Enterprise,
}

impl Default for OrganizationPlan {
    fn default() -> Self {
        OrganizationPlan::Free
    }
}

impl std::fmt::Display for OrganizationPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganizationPlan::Free => write!(f, "free"),
            OrganizationPlan::Pro => write!(f, "pro"),
            OrganizationPlan::Enterprise => write!(f, "enterprise"),
        }
    }
}

impl std::str::FromStr for OrganizationPlan {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "free" => Ok(OrganizationPlan::Free),
            "pro" => Ok(OrganizationPlan::Pro),
            "enterprise" => Ok(OrganizationPlan::Enterprise),
            _ => Err(format!("Unknown plan: {}", s)),
        }
    }
}

/// Organization account status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrganizationStatus {
    /// Organization is active and can use all features
    Active,
    /// Organization is suspended (e.g., payment issues)
    Suspended,
    /// Organization is deleted (soft delete)
    Deleted,
}

impl Default for OrganizationStatus {
    fn default() -> Self {
        OrganizationStatus::Active
    }
}

impl std::fmt::Display for OrganizationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganizationStatus::Active => write!(f, "active"),
            OrganizationStatus::Suspended => write!(f, "suspended"),
            OrganizationStatus::Deleted => write!(f, "deleted"),
        }
    }
}

impl std::str::FromStr for OrganizationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(OrganizationStatus::Active),
            "suspended" => Ok(OrganizationStatus::Suspended),
            "deleted" => Ok(OrganizationStatus::Deleted),
            _ => Err(format!("Unknown status: {}", s)),
        }
    }
}

/// Configuration for creating a new organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrganization {
    /// Human-readable organization name
    pub name: String,

    /// URL-friendly identifier (lowercase alphanumeric with hyphens)
    pub slug: String,

    /// Billing plan (defaults to Free)
    #[serde(default)]
    pub plan: OrganizationPlan,

    /// Organization-specific settings
    #[serde(default)]
    pub settings: HashMap<String, String>,
}

/// API key for programmatic access.
///
/// API keys are used to authenticate requests to the StreamHouse API.
/// Each key belongs to an organization and has specific permissions.
///
/// # Security
///
/// - The actual key is never stored; only the SHA-256 hash is stored
/// - The key prefix (first 12 chars) is stored for identification
/// - Keys can have an expiration date
/// - Keys can be scoped to specific topics
///
/// # Fields
///
/// * `id` - Unique key ID (UUID format)
/// * `organization_id` - Owning organization ID
/// * `name` - Human-readable key name (e.g., "Production API Key")
/// * `key_prefix` - First 12 chars of the key (e.g., "sk_live_abc1")
/// * `permissions` - List of allowed operations (read, write, admin)
/// * `scopes` - Optional list of allowed topic patterns
/// * `expires_at` - Optional expiration timestamp
/// * `last_used_at` - When the key was last used
/// * `created_at` - Key creation timestamp
///
/// # Examples
///
/// ```ignore
/// // Generate a new API key
/// let (key, api_key_record) = create_api_key(
///     "org-123",
///     "Production Key",
///     vec!["read", "write"],
/// );
///
/// // key = "sk_live_abc123..." (give this to the user)
/// // api_key_record.key_hash = SHA-256 of key (store this)
/// // api_key_record.key_prefix = "sk_live_abc1" (for identification)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique key ID (UUID format)
    pub id: String,

    /// Owning organization ID
    pub organization_id: String,

    /// Human-readable key name
    pub name: String,

    /// First 12 chars of the key for identification
    pub key_prefix: String,

    /// Allowed operations (read, write, admin)
    pub permissions: Vec<String>,

    /// Optional list of allowed topic patterns (empty = all topics)
    pub scopes: Vec<String>,

    /// Optional expiration timestamp (milliseconds since Unix epoch)
    pub expires_at: Option<i64>,

    /// When the key was last used (milliseconds since Unix epoch)
    pub last_used_at: Option<i64>,

    /// Key creation timestamp (milliseconds since Unix epoch)
    pub created_at: i64,

    /// User ID who created the key
    pub created_by: Option<String>,
}

/// Configuration for creating a new API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKey {
    /// Human-readable key name
    pub name: String,

    /// Allowed operations (defaults to ["read", "write"])
    #[serde(default = "default_permissions")]
    pub permissions: Vec<String>,

    /// Optional list of allowed topic patterns
    #[serde(default)]
    pub scopes: Vec<String>,

    /// Optional expiration (milliseconds from now)
    pub expires_in_ms: Option<i64>,
}

fn default_permissions() -> Vec<String> {
    vec!["read".to_string(), "write".to_string()]
}

/// Resource quotas for an organization.
///
/// Quotas limit resource usage per organization for billing tiers and abuse prevention.
/// Different plans have different quota limits.
///
/// # Quota Categories
///
/// - **Topics**: Maximum number of topics and partitions
/// - **Storage**: Maximum data stored and retention period
/// - **Throughput**: Maximum produce/consume bytes per second
/// - **Connections**: Maximum concurrent connections
///
/// # Examples
///
/// ```ignore
/// // Free tier quotas
/// let free_quotas = OrganizationQuota {
///     max_topics: 3,
///     max_partitions_per_topic: 4,
///     max_storage_bytes: 1_073_741_824, // 1 GB
///     max_produce_bytes_per_sec: 1_048_576, // 1 MB/s
///     ..Default::default()
/// };
///
/// // Check quota before creating topic
/// if current_topics >= quotas.max_topics {
///     return Err(QuotaExceeded::MaxTopics);
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationQuota {
    /// Organization ID
    pub organization_id: String,

    /// Maximum number of topics
    pub max_topics: i32,

    /// Maximum partitions per topic
    pub max_partitions_per_topic: i32,

    /// Maximum total partitions across all topics
    pub max_total_partitions: i32,

    /// Maximum storage in bytes
    pub max_storage_bytes: i64,

    /// Maximum retention in days
    pub max_retention_days: i32,

    /// Maximum produce throughput (bytes per second)
    pub max_produce_bytes_per_sec: i64,

    /// Maximum consume throughput (bytes per second)
    pub max_consume_bytes_per_sec: i64,

    /// Maximum requests per second
    pub max_requests_per_sec: i32,

    /// Maximum consumer groups
    pub max_consumer_groups: i32,

    /// Maximum schemas in registry
    pub max_schemas: i32,

    /// Maximum schema versions per subject
    pub max_schema_versions_per_subject: i32,

    /// Maximum concurrent connections
    pub max_connections: i32,
}

impl Default for OrganizationQuota {
    fn default() -> Self {
        // Free tier defaults
        Self {
            organization_id: String::new(),
            max_topics: 10,
            max_partitions_per_topic: 12,
            max_total_partitions: 100,
            max_storage_bytes: 10_737_418_240, // 10 GB
            max_retention_days: 7,
            max_produce_bytes_per_sec: 10_485_760, // 10 MB/s
            max_consume_bytes_per_sec: 52_428_800, // 50 MB/s
            max_requests_per_sec: 1000,
            max_consumer_groups: 50,
            max_schemas: 100,
            max_schema_versions_per_subject: 100,
            max_connections: 100,
        }
    }
}

/// Usage tracking for quota enforcement and billing.
///
/// Tracks resource usage per organization for rate limiting and billing.
///
/// # Fields
///
/// * `organization_id` - Organization ID
/// * `metric` - Metric name (e.g., "storage_bytes", "produce_bytes")
/// * `value` - Current value
/// * `period_start` - Start of the measurement period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationUsage {
    /// Organization ID
    pub organization_id: String,

    /// Metric name
    pub metric: String,

    /// Current value
    pub value: i64,

    /// Start of measurement period (milliseconds since Unix epoch)
    pub period_start: i64,
}

/// Default organization ID for backwards compatibility.
///
/// This is used for existing data that doesn't have an organization_id,
/// and for single-tenant deployments.
pub const DEFAULT_ORGANIZATION_ID: &str = "00000000-0000-0000-0000-000000000000";
