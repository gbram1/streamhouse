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

/// Topic cleanup policy.
///
/// Determines how old data is handled in a topic.
///
/// # Variants
///
/// * `Delete` - Delete old segments when retention time/size is exceeded (default)
/// * `Compact` - Keep only the latest value for each key (log compaction)
/// * `CompactAndDelete` - Compact first, then delete after retention period
///
/// # Examples
///
/// ```ignore
/// // User profiles topic - keep latest state per user
/// let config = TopicConfig {
///     name: "user_profiles".to_string(),
///     partition_count: 10,
///     cleanup_policy: CleanupPolicy::Compact,
///     ..Default::default()
/// };
///
/// // Event log - delete after 7 days
/// let config = TopicConfig {
///     name: "events".to_string(),
///     partition_count: 3,
///     cleanup_policy: CleanupPolicy::Delete,
///     retention_ms: Some(7 * 24 * 60 * 60 * 1000),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CleanupPolicy {
    /// Delete old segments based on retention time/size
    #[default]
    Delete,
    /// Keep only the latest value for each key
    Compact,
    /// Compact first, then delete after retention period
    #[serde(rename = "compact,delete")]
    CompactAndDelete,
}

impl CleanupPolicy {
    /// Parse cleanup policy from string (Kafka-compatible format)
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "compact" => CleanupPolicy::Compact,
            "compact,delete" | "delete,compact" => CleanupPolicy::CompactAndDelete,
            _ => CleanupPolicy::Delete,
        }
    }

    /// Convert to string (Kafka-compatible format)
    pub fn as_str(&self) -> &'static str {
        match self {
            CleanupPolicy::Delete => "delete",
            CleanupPolicy::Compact => "compact",
            CleanupPolicy::CompactAndDelete => "compact,delete",
        }
    }

    /// Check if compaction is enabled
    pub fn is_compacted(&self) -> bool {
        matches!(
            self,
            CleanupPolicy::Compact | CleanupPolicy::CompactAndDelete
        )
    }
}

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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopicConfig {
    /// Topic name (unique identifier)
    pub name: String,

    /// Number of partitions (determines parallelism)
    pub partition_count: u32,

    /// Retention period in milliseconds (None = infinite)
    pub retention_ms: Option<i64>,

    /// Cleanup policy (delete, compact, or compact,delete)
    #[serde(default)]
    pub cleanup_policy: CleanupPolicy,

    /// Additional configuration parameters
    #[serde(default)]
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

    /// Cleanup policy (delete, compact, or compact,delete)
    #[serde(default)]
    pub cleanup_policy: CleanupPolicy,

    /// Creation timestamp (milliseconds since Unix epoch)
    pub created_at: i64,

    /// Additional configuration parameters
    #[serde(default)]
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrganizationPlan {
    /// Free tier with limited resources
    #[default]
    Free,
    /// Professional tier with higher limits
    Pro,
    /// Enterprise tier with custom limits
    Enterprise,
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrganizationStatus {
    /// Organization is active and can use all features
    #[default]
    Active,
    /// Organization is suspended (e.g., payment issues)
    Suspended,
    /// Organization is deleted (soft delete)
    Deleted,
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

// ============================================================================
// Exactly-Once Semantics Types
// ============================================================================

/// Producer state for idempotent and transactional producers.
///
/// Each producer is assigned a unique ID and epoch. The epoch is incremented
/// on each InitProducer call, allowing detection of "zombie" producers.
///
/// # Idempotent Producers
///
/// When a producer sets `producer_id` and `base_sequence` in ProduceRequest,
/// the agent tracks the last sequence seen and rejects duplicates.
///
/// # Transactional Producers
///
/// If `transactional_id` is set, the producer can use transactions for
/// atomic multi-partition writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Producer {
    /// Unique producer ID (UUID).
    pub id: String,

    /// Optional organization ID for multi-tenant isolation.
    pub organization_id: Option<String>,

    /// Optional transactional ID for transactional producers.
    /// Must be unique per organization.
    pub transactional_id: Option<String>,

    /// Producer epoch (incremented on each InitProducer call).
    /// Used for fencing zombie producers.
    pub epoch: u32,

    /// Producer state.
    pub state: ProducerState,

    /// Creation timestamp (milliseconds since Unix epoch).
    pub created_at: i64,

    /// Last heartbeat timestamp (milliseconds since Unix epoch).
    pub last_heartbeat: i64,

    /// Optional metadata (client info, etc.).
    pub metadata: Option<HashMap<String, String>>,
}

/// Producer state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProducerState {
    /// Producer is active and can send records.
    #[default]
    Active,
    /// Producer has been fenced by a newer instance.
    Fenced,
    /// Producer has expired due to timeout.
    Expired,
}

impl std::fmt::Display for ProducerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProducerState::Active => write!(f, "active"),
            ProducerState::Fenced => write!(f, "fenced"),
            ProducerState::Expired => write!(f, "expired"),
        }
    }
}

impl std::str::FromStr for ProducerState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(ProducerState::Active),
            "fenced" => Ok(ProducerState::Fenced),
            "expired" => Ok(ProducerState::Expired),
            _ => Err(format!("Unknown producer state: {}", s)),
        }
    }
}

/// Sequence tracking for idempotent producers.
///
/// Tracks the last sequence number seen for each producer/topic/partition combo.
/// Used for deduplication - if a record's sequence <= last_sequence, it's a duplicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerSequence {
    /// Producer ID.
    pub producer_id: String,

    /// Topic name.
    pub topic: String,

    /// Partition ID.
    pub partition_id: u32,

    /// Last sequence number successfully processed.
    /// -1 means no records have been processed yet.
    pub last_sequence: i64,

    /// Last update timestamp (milliseconds since Unix epoch).
    pub updated_at: i64,
}

/// Configuration for creating/initializing a producer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitProducerConfig {
    /// Optional transactional ID.
    pub transactional_id: Option<String>,

    /// Optional organization ID.
    pub organization_id: Option<String>,

    /// Producer timeout in milliseconds.
    pub timeout_ms: u32,

    /// Optional metadata.
    pub metadata: Option<HashMap<String, String>>,
}

/// Transaction state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique transaction ID (UUID).
    pub transaction_id: String,

    /// Producer ID that owns this transaction.
    pub producer_id: String,

    /// Transaction state.
    pub state: TransactionState,

    /// Transaction timeout in milliseconds.
    pub timeout_ms: u32,

    /// Start timestamp (milliseconds since Unix epoch).
    pub started_at: i64,

    /// Last update timestamp (milliseconds since Unix epoch).
    pub updated_at: i64,

    /// Completion timestamp (milliseconds since Unix epoch).
    /// None if transaction is still ongoing.
    pub completed_at: Option<i64>,
}

/// Transaction state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    /// Transaction is ongoing, records being added.
    #[default]
    Ongoing,
    /// Transaction is preparing to commit (two-phase commit).
    Preparing,
    /// Transaction has been committed.
    Committed,
    /// Transaction has been aborted.
    Aborted,
}

impl std::fmt::Display for TransactionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionState::Ongoing => write!(f, "ongoing"),
            TransactionState::Preparing => write!(f, "preparing"),
            TransactionState::Committed => write!(f, "committed"),
            TransactionState::Aborted => write!(f, "aborted"),
        }
    }
}

impl std::str::FromStr for TransactionState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ongoing" => Ok(TransactionState::Ongoing),
            "preparing" => Ok(TransactionState::Preparing),
            "committed" => Ok(TransactionState::Committed),
            "aborted" => Ok(TransactionState::Aborted),
            _ => Err(format!("Unknown transaction state: {}", s)),
        }
    }
}

/// Partition participation in a transaction.
///
/// Tracks which partitions have records as part of a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionPartition {
    /// Transaction ID.
    pub transaction_id: String,

    /// Topic name.
    pub topic: String,

    /// Partition ID.
    pub partition_id: u32,

    /// First offset written in this transaction.
    pub first_offset: u64,

    /// Last offset written in this transaction (updated as records are added).
    pub last_offset: u64,
}

/// Transaction marker written to indicate commit/abort boundaries.
///
/// These are control records written to the partition log to indicate
/// transaction boundaries. Used by consumers in read-committed mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMarker {
    /// Transaction ID this marker belongs to.
    pub transaction_id: String,

    /// Topic name.
    pub topic: String,

    /// Partition ID.
    pub partition_id: u32,

    /// Offset of the marker record.
    pub offset: u64,

    /// Marker type.
    pub marker_type: TransactionMarkerType,

    /// Creation timestamp (milliseconds since Unix epoch).
    pub timestamp: i64,
}

/// Transaction marker type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionMarkerType {
    /// Transaction was committed.
    Commit,
    /// Transaction was aborted.
    Abort,
}

/// Consumer isolation level for read-committed support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IsolationLevel {
    /// Read all records, including uncommitted transaction records.
    #[default]
    ReadUncommitted,
    /// Only read committed records (skip uncommitted transaction records).
    ReadCommitted,
}

/// Consumer group configuration for exactly-once support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroupConfig {
    /// Consumer group ID.
    pub group_id: String,

    /// Isolation level for reads.
    pub isolation_level: IsolationLevel,

    /// Whether to enable auto-commit.
    pub enable_auto_commit: bool,

    /// Auto-commit interval in milliseconds.
    pub auto_commit_interval_ms: u32,

    /// Session timeout in milliseconds.
    pub session_timeout_ms: u32,

    /// Creation timestamp (milliseconds since Unix epoch).
    pub created_at: i64,

    /// Last update timestamp (milliseconds since Unix epoch).
    pub updated_at: i64,
}

impl Default for ConsumerGroupConfig {
    fn default() -> Self {
        Self {
            group_id: String::new(),
            isolation_level: IsolationLevel::ReadUncommitted,
            enable_auto_commit: true,
            auto_commit_interval_ms: 5000,
            session_timeout_ms: 30000,
            created_at: 0,
            updated_at: 0,
        }
    }
}

/// Last Stable Offset (LSO) for a partition.
///
/// The LSO is the highest offset where all transactions below are either
/// committed or aborted. Consumers in read-committed mode only see records
/// up to the LSO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionLSO {
    /// Topic name.
    pub topic: String,

    /// Partition ID.
    pub partition_id: u32,

    /// Last stable offset.
    pub last_stable_offset: u64,

    /// Last update timestamp (milliseconds since Unix epoch).
    pub updated_at: i64,
}

/// Ack mode for controlling durability vs latency trade-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AckMode {
    /// Ack after record is buffered in agent memory.
    /// Fast (~1ms) but data at risk until S3 flush (~30s window).
    #[default]
    Buffered,
    /// Ack after record is persisted to S3.
    /// Slower (~150ms) but zero data loss.
    Durable,
    /// Fire and forget - don't wait for any confirmation.
    /// Fastest but can lose data.
    None,
}

// ============================================================================
// Fast Leader Handoff Types
// ============================================================================

/// Reason for leadership change.
///
/// Used for tracking and metrics to understand why leadership changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaderChangeReason {
    /// Lease expired due to agent failure or network partition.
    LeaseExpired,
    /// Graceful handoff during shutdown or rolling deploy.
    GracefulHandoff,
    /// Agent crashed without graceful shutdown.
    AgentCrash,
    /// Rebalance triggered by partition reassignment.
    Rebalance,
    /// Initial lease acquisition (no previous leader).
    #[default]
    Initial,
}

impl std::fmt::Display for LeaderChangeReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeaderChangeReason::LeaseExpired => write!(f, "lease_expired"),
            LeaderChangeReason::GracefulHandoff => write!(f, "graceful_handoff"),
            LeaderChangeReason::AgentCrash => write!(f, "agent_crash"),
            LeaderChangeReason::Rebalance => write!(f, "rebalance"),
            LeaderChangeReason::Initial => write!(f, "initial"),
        }
    }
}

impl std::str::FromStr for LeaderChangeReason {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lease_expired" => Ok(LeaderChangeReason::LeaseExpired),
            "graceful_handoff" => Ok(LeaderChangeReason::GracefulHandoff),
            "agent_crash" => Ok(LeaderChangeReason::AgentCrash),
            "rebalance" => Ok(LeaderChangeReason::Rebalance),
            "initial" => Ok(LeaderChangeReason::Initial),
            _ => Err(format!("Unknown leader change reason: {}", s)),
        }
    }
}

/// State of a pending lease transfer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseTransferState {
    /// Transfer has been initiated but not yet accepted.
    #[default]
    Pending,
    /// Transfer has been accepted, waiting for data sync.
    Accepted,
    /// Data sync is complete, lease is being transferred.
    Completing,
    /// Transfer completed successfully.
    Completed,
    /// Transfer was rejected by the target agent.
    Rejected,
    /// Transfer timed out.
    TimedOut,
    /// Transfer failed for other reasons.
    Failed,
}

impl std::fmt::Display for LeaseTransferState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeaseTransferState::Pending => write!(f, "pending"),
            LeaseTransferState::Accepted => write!(f, "accepted"),
            LeaseTransferState::Completing => write!(f, "completing"),
            LeaseTransferState::Completed => write!(f, "completed"),
            LeaseTransferState::Rejected => write!(f, "rejected"),
            LeaseTransferState::TimedOut => write!(f, "timed_out"),
            LeaseTransferState::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for LeaseTransferState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(LeaseTransferState::Pending),
            "accepted" => Ok(LeaseTransferState::Accepted),
            "completing" => Ok(LeaseTransferState::Completing),
            "completed" => Ok(LeaseTransferState::Completed),
            "rejected" => Ok(LeaseTransferState::Rejected),
            "timed_out" => Ok(LeaseTransferState::TimedOut),
            "failed" => Ok(LeaseTransferState::Failed),
            _ => Err(format!("Unknown transfer state: {}", s)),
        }
    }
}

/// Pending lease transfer record.
///
/// Tracks an in-progress lease transfer between two agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseTransfer {
    /// Unique transfer ID (UUID).
    pub transfer_id: String,

    /// Topic name.
    pub topic: String,

    /// Partition ID.
    pub partition_id: u32,

    /// Source agent ID (current leader).
    pub from_agent_id: String,

    /// Target agent ID (incoming leader).
    pub to_agent_id: String,

    /// Lease epoch at transfer initiation.
    pub from_epoch: u64,

    /// Transfer state.
    pub state: LeaseTransferState,

    /// Reason for transfer.
    pub reason: LeaderChangeReason,

    /// Transfer initiation timestamp (milliseconds since Unix epoch).
    pub initiated_at: i64,

    /// Transfer completion timestamp (milliseconds since Unix epoch).
    /// None if transfer is still in progress.
    pub completed_at: Option<i64>,

    /// Transfer timeout (milliseconds since Unix epoch).
    pub timeout_at: i64,

    /// Last flushed offset (set when data sync completes).
    pub last_flushed_offset: Option<u64>,

    /// High watermark at transfer initiation.
    pub high_watermark: Option<u64>,

    /// Error message if transfer failed.
    pub error: Option<String>,
}

// ============================================================================
// Materialized View Types
// ============================================================================

/// Refresh mode for materialized views
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaterializedViewRefreshMode {
    /// Continuously update as new messages arrive
    #[default]
    Continuous,
    /// Refresh periodically on a schedule
    Periodic {
        /// Interval in milliseconds
        interval_ms: i64,
    },
    /// Only refresh when explicitly triggered
    Manual,
}

/// Status of a materialized view
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaterializedViewStatus {
    /// View is being initialized/bootstrapped
    #[default]
    Initializing,
    /// View is actively being maintained
    Running,
    /// View maintenance is paused
    Paused,
    /// View encountered an error
    Error,
}

/// Configuration for creating a materialized view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMaterializedView {
    /// View name (unique per organization)
    pub name: String,

    /// Source topic for the view
    pub source_topic: String,

    /// SQL query that defines the view
    pub query_sql: String,

    /// Refresh mode
    pub refresh_mode: MaterializedViewRefreshMode,

    /// Organization ID (for multi-tenancy)
    pub organization_id: Option<String>,
}

/// Materialized view definition and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedView {
    /// Unique identifier
    pub id: String,

    /// Organization ID (for multi-tenancy)
    pub organization_id: String,

    /// View name
    pub name: String,

    /// Source topic
    pub source_topic: String,

    /// SQL query that defines the view
    pub query_sql: String,

    /// Refresh mode
    pub refresh_mode: MaterializedViewRefreshMode,

    /// Current status
    pub status: MaterializedViewStatus,

    /// Error message (if status is Error)
    pub error_message: Option<String>,

    /// Number of rows in the view
    pub row_count: u64,

    /// Last refresh timestamp (milliseconds since epoch)
    pub last_refresh_at: Option<i64>,

    /// Created timestamp (milliseconds since epoch)
    pub created_at: i64,

    /// Updated timestamp (milliseconds since epoch)
    pub updated_at: i64,
}

/// Offset tracking for a materialized view partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedViewOffset {
    /// View ID
    pub view_id: String,

    /// Partition ID
    pub partition_id: u32,

    /// Last processed offset (exclusive - next offset to process)
    pub last_offset: u64,

    /// Last processed timestamp (milliseconds since epoch)
    pub last_processed_at: i64,
}

/// Aggregated data row for a materialized view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedViewData {
    /// View ID
    pub view_id: String,

    /// Aggregation key
    pub agg_key: String,

    /// Aggregated values
    pub agg_values: serde_json::Value,

    /// Window start timestamp (for windowed aggregations)
    pub window_start: Option<i64>,

    /// Window end timestamp (for windowed aggregations)
    pub window_end: Option<i64>,

    /// Last updated timestamp
    pub updated_at: i64,
}

/// Connector definition stored in metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorInfo {
    pub name: String,
    pub connector_type: String,
    pub connector_class: String,
    pub topics: Vec<String>,
    pub config: HashMap<String, String>,
    pub state: String,
    pub error_message: Option<String>,
    pub records_processed: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ========================================================================
    // CleanupPolicy Tests
    // ========================================================================

    #[test]
    fn test_cleanup_policy_from_str_compact() {
        assert_eq!(CleanupPolicy::from_str("compact"), CleanupPolicy::Compact);
    }

    #[test]
    fn test_cleanup_policy_from_str_compact_uppercase() {
        assert_eq!(CleanupPolicy::from_str("COMPACT"), CleanupPolicy::Compact);
    }

    #[test]
    fn test_cleanup_policy_from_str_compact_mixed_case() {
        assert_eq!(CleanupPolicy::from_str("Compact"), CleanupPolicy::Compact);
    }

    #[test]
    fn test_cleanup_policy_from_str_delete() {
        assert_eq!(CleanupPolicy::from_str("delete"), CleanupPolicy::Delete);
    }

    #[test]
    fn test_cleanup_policy_from_str_delete_uppercase() {
        assert_eq!(CleanupPolicy::from_str("DELETE"), CleanupPolicy::Delete);
    }

    #[test]
    fn test_cleanup_policy_from_str_compact_delete() {
        assert_eq!(
            CleanupPolicy::from_str("compact,delete"),
            CleanupPolicy::CompactAndDelete
        );
    }

    #[test]
    fn test_cleanup_policy_from_str_delete_compact() {
        assert_eq!(
            CleanupPolicy::from_str("delete,compact"),
            CleanupPolicy::CompactAndDelete
        );
    }

    #[test]
    fn test_cleanup_policy_from_str_unknown_defaults_to_delete() {
        assert_eq!(CleanupPolicy::from_str("unknown"), CleanupPolicy::Delete);
    }

    #[test]
    fn test_cleanup_policy_from_str_empty_defaults_to_delete() {
        assert_eq!(CleanupPolicy::from_str(""), CleanupPolicy::Delete);
    }

    #[test]
    fn test_cleanup_policy_from_str_garbage_defaults_to_delete() {
        assert_eq!(
            CleanupPolicy::from_str("not-a-policy"),
            CleanupPolicy::Delete
        );
    }

    #[test]
    fn test_cleanup_policy_as_str_delete() {
        assert_eq!(CleanupPolicy::Delete.as_str(), "delete");
    }

    #[test]
    fn test_cleanup_policy_as_str_compact() {
        assert_eq!(CleanupPolicy::Compact.as_str(), "compact");
    }

    #[test]
    fn test_cleanup_policy_as_str_compact_and_delete() {
        assert_eq!(CleanupPolicy::CompactAndDelete.as_str(), "compact,delete");
    }

    #[test]
    fn test_cleanup_policy_is_compacted_delete() {
        assert!(!CleanupPolicy::Delete.is_compacted());
    }

    #[test]
    fn test_cleanup_policy_is_compacted_compact() {
        assert!(CleanupPolicy::Compact.is_compacted());
    }

    #[test]
    fn test_cleanup_policy_is_compacted_compact_and_delete() {
        assert!(CleanupPolicy::CompactAndDelete.is_compacted());
    }

    #[test]
    fn test_cleanup_policy_default() {
        assert_eq!(CleanupPolicy::default(), CleanupPolicy::Delete);
    }

    #[test]
    fn test_cleanup_policy_roundtrip_from_str_as_str() {
        for policy in [
            CleanupPolicy::Delete,
            CleanupPolicy::Compact,
            CleanupPolicy::CompactAndDelete,
        ] {
            let s = policy.as_str();
            let parsed = CleanupPolicy::from_str(s);
            assert_eq!(parsed, policy, "Roundtrip failed for {:?}", policy);
        }
    }

    #[test]
    fn test_cleanup_policy_serde_roundtrip() {
        for policy in [
            CleanupPolicy::Delete,
            CleanupPolicy::Compact,
            CleanupPolicy::CompactAndDelete,
        ] {
            let json = serde_json::to_string(&policy).unwrap();
            let parsed: CleanupPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, policy, "Serde roundtrip failed for {:?}", policy);
        }
    }

    #[test]
    fn test_cleanup_policy_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&CleanupPolicy::Delete).unwrap(),
            "\"delete\""
        );
        assert_eq!(
            serde_json::to_string(&CleanupPolicy::Compact).unwrap(),
            "\"compact\""
        );
        assert_eq!(
            serde_json::to_string(&CleanupPolicy::CompactAndDelete).unwrap(),
            "\"compact,delete\""
        );
    }

    #[test]
    fn test_cleanup_policy_clone_and_copy() {
        let policy = CleanupPolicy::Compact;
        let cloned = policy.clone();
        let copied = policy;
        assert_eq!(policy, cloned);
        assert_eq!(policy, copied);
    }

    #[test]
    fn test_cleanup_policy_debug() {
        let debug_str = format!("{:?}", CleanupPolicy::Delete);
        assert_eq!(debug_str, "Delete");
    }

    // ========================================================================
    // OrganizationPlan Tests
    // ========================================================================

    #[test]
    fn test_organization_plan_display_free() {
        assert_eq!(OrganizationPlan::Free.to_string(), "free");
    }

    #[test]
    fn test_organization_plan_display_pro() {
        assert_eq!(OrganizationPlan::Pro.to_string(), "pro");
    }

    #[test]
    fn test_organization_plan_display_enterprise() {
        assert_eq!(OrganizationPlan::Enterprise.to_string(), "enterprise");
    }

    #[test]
    fn test_organization_plan_from_str_free() {
        assert_eq!(
            "free".parse::<OrganizationPlan>().unwrap(),
            OrganizationPlan::Free
        );
    }

    #[test]
    fn test_organization_plan_from_str_pro() {
        assert_eq!(
            "pro".parse::<OrganizationPlan>().unwrap(),
            OrganizationPlan::Pro
        );
    }

    #[test]
    fn test_organization_plan_from_str_enterprise() {
        assert_eq!(
            "enterprise".parse::<OrganizationPlan>().unwrap(),
            OrganizationPlan::Enterprise
        );
    }

    #[test]
    fn test_organization_plan_from_str_case_insensitive() {
        assert_eq!(
            "FREE".parse::<OrganizationPlan>().unwrap(),
            OrganizationPlan::Free
        );
        assert_eq!(
            "Pro".parse::<OrganizationPlan>().unwrap(),
            OrganizationPlan::Pro
        );
        assert_eq!(
            "ENTERPRISE".parse::<OrganizationPlan>().unwrap(),
            OrganizationPlan::Enterprise
        );
    }

    #[test]
    fn test_organization_plan_from_str_invalid() {
        let result = "invalid".parse::<OrganizationPlan>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown plan"));
        assert!(err.contains("invalid"));
    }

    #[test]
    fn test_organization_plan_from_str_empty() {
        let result = "".parse::<OrganizationPlan>();
        assert!(result.is_err());
    }

    #[test]
    fn test_organization_plan_default() {
        assert_eq!(OrganizationPlan::default(), OrganizationPlan::Free);
    }

    #[test]
    fn test_organization_plan_display_from_str_roundtrip() {
        for plan in [
            OrganizationPlan::Free,
            OrganizationPlan::Pro,
            OrganizationPlan::Enterprise,
        ] {
            let s = plan.to_string();
            let parsed: OrganizationPlan = s.parse().unwrap();
            assert_eq!(parsed, plan, "Roundtrip failed for {:?}", plan);
        }
    }

    #[test]
    fn test_organization_plan_serde_roundtrip() {
        for plan in [
            OrganizationPlan::Free,
            OrganizationPlan::Pro,
            OrganizationPlan::Enterprise,
        ] {
            let json = serde_json::to_string(&plan).unwrap();
            let parsed: OrganizationPlan = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, plan, "Serde roundtrip failed for {:?}", plan);
        }
    }

    #[test]
    fn test_organization_plan_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&OrganizationPlan::Free).unwrap(),
            "\"free\""
        );
        assert_eq!(
            serde_json::to_string(&OrganizationPlan::Pro).unwrap(),
            "\"pro\""
        );
        assert_eq!(
            serde_json::to_string(&OrganizationPlan::Enterprise).unwrap(),
            "\"enterprise\""
        );
    }

    #[test]
    fn test_organization_plan_equality() {
        assert_eq!(OrganizationPlan::Free, OrganizationPlan::Free);
        assert_ne!(OrganizationPlan::Free, OrganizationPlan::Pro);
        assert_ne!(OrganizationPlan::Pro, OrganizationPlan::Enterprise);
    }

    // ========================================================================
    // OrganizationStatus Tests
    // ========================================================================

    #[test]
    fn test_organization_status_display_active() {
        assert_eq!(OrganizationStatus::Active.to_string(), "active");
    }

    #[test]
    fn test_organization_status_display_suspended() {
        assert_eq!(OrganizationStatus::Suspended.to_string(), "suspended");
    }

    #[test]
    fn test_organization_status_display_deleted() {
        assert_eq!(OrganizationStatus::Deleted.to_string(), "deleted");
    }

    #[test]
    fn test_organization_status_from_str_active() {
        assert_eq!(
            "active".parse::<OrganizationStatus>().unwrap(),
            OrganizationStatus::Active
        );
    }

    #[test]
    fn test_organization_status_from_str_suspended() {
        assert_eq!(
            "suspended".parse::<OrganizationStatus>().unwrap(),
            OrganizationStatus::Suspended
        );
    }

    #[test]
    fn test_organization_status_from_str_deleted() {
        assert_eq!(
            "deleted".parse::<OrganizationStatus>().unwrap(),
            OrganizationStatus::Deleted
        );
    }

    #[test]
    fn test_organization_status_from_str_case_insensitive() {
        assert_eq!(
            "ACTIVE".parse::<OrganizationStatus>().unwrap(),
            OrganizationStatus::Active
        );
        assert_eq!(
            "Suspended".parse::<OrganizationStatus>().unwrap(),
            OrganizationStatus::Suspended
        );
        assert_eq!(
            "DELETED".parse::<OrganizationStatus>().unwrap(),
            OrganizationStatus::Deleted
        );
    }

    #[test]
    fn test_organization_status_from_str_invalid() {
        let result = "invalid".parse::<OrganizationStatus>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown status"));
        assert!(err.contains("invalid"));
    }

    #[test]
    fn test_organization_status_from_str_empty() {
        let result = "".parse::<OrganizationStatus>();
        assert!(result.is_err());
    }

    #[test]
    fn test_organization_status_default() {
        assert_eq!(OrganizationStatus::default(), OrganizationStatus::Active);
    }

    #[test]
    fn test_organization_status_display_from_str_roundtrip() {
        for status in [
            OrganizationStatus::Active,
            OrganizationStatus::Suspended,
            OrganizationStatus::Deleted,
        ] {
            let s = status.to_string();
            let parsed: OrganizationStatus = s.parse().unwrap();
            assert_eq!(parsed, status, "Roundtrip failed for {:?}", status);
        }
    }

    #[test]
    fn test_organization_status_serde_roundtrip() {
        for status in [
            OrganizationStatus::Active,
            OrganizationStatus::Suspended,
            OrganizationStatus::Deleted,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: OrganizationStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status, "Serde roundtrip failed for {:?}", status);
        }
    }

    #[test]
    fn test_organization_status_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&OrganizationStatus::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&OrganizationStatus::Suspended).unwrap(),
            "\"suspended\""
        );
        assert_eq!(
            serde_json::to_string(&OrganizationStatus::Deleted).unwrap(),
            "\"deleted\""
        );
    }

    // ========================================================================
    // ProducerState Tests
    // ========================================================================

    #[test]
    fn test_producer_state_display_active() {
        assert_eq!(ProducerState::Active.to_string(), "active");
    }

    #[test]
    fn test_producer_state_display_fenced() {
        assert_eq!(ProducerState::Fenced.to_string(), "fenced");
    }

    #[test]
    fn test_producer_state_display_expired() {
        assert_eq!(ProducerState::Expired.to_string(), "expired");
    }

    #[test]
    fn test_producer_state_from_str_active() {
        assert_eq!(
            "active".parse::<ProducerState>().unwrap(),
            ProducerState::Active
        );
    }

    #[test]
    fn test_producer_state_from_str_fenced() {
        assert_eq!(
            "fenced".parse::<ProducerState>().unwrap(),
            ProducerState::Fenced
        );
    }

    #[test]
    fn test_producer_state_from_str_expired() {
        assert_eq!(
            "expired".parse::<ProducerState>().unwrap(),
            ProducerState::Expired
        );
    }

    #[test]
    fn test_producer_state_from_str_case_insensitive() {
        assert_eq!(
            "ACTIVE".parse::<ProducerState>().unwrap(),
            ProducerState::Active
        );
        assert_eq!(
            "Fenced".parse::<ProducerState>().unwrap(),
            ProducerState::Fenced
        );
        assert_eq!(
            "EXPIRED".parse::<ProducerState>().unwrap(),
            ProducerState::Expired
        );
    }

    #[test]
    fn test_producer_state_from_str_invalid() {
        let result = "invalid".parse::<ProducerState>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown producer state"));
    }

    #[test]
    fn test_producer_state_default() {
        assert_eq!(ProducerState::default(), ProducerState::Active);
    }

    #[test]
    fn test_producer_state_display_from_str_roundtrip() {
        for state in [
            ProducerState::Active,
            ProducerState::Fenced,
            ProducerState::Expired,
        ] {
            let s = state.to_string();
            let parsed: ProducerState = s.parse().unwrap();
            assert_eq!(parsed, state, "Roundtrip failed for {:?}", state);
        }
    }

    #[test]
    fn test_producer_state_serde_roundtrip() {
        for state in [
            ProducerState::Active,
            ProducerState::Fenced,
            ProducerState::Expired,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: ProducerState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state, "Serde roundtrip failed for {:?}", state);
        }
    }

    #[test]
    fn test_producer_state_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&ProducerState::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&ProducerState::Fenced).unwrap(),
            "\"fenced\""
        );
        assert_eq!(
            serde_json::to_string(&ProducerState::Expired).unwrap(),
            "\"expired\""
        );
    }

    // ========================================================================
    // TransactionState Tests
    // ========================================================================

    #[test]
    fn test_transaction_state_display_ongoing() {
        assert_eq!(TransactionState::Ongoing.to_string(), "ongoing");
    }

    #[test]
    fn test_transaction_state_display_preparing() {
        assert_eq!(TransactionState::Preparing.to_string(), "preparing");
    }

    #[test]
    fn test_transaction_state_display_committed() {
        assert_eq!(TransactionState::Committed.to_string(), "committed");
    }

    #[test]
    fn test_transaction_state_display_aborted() {
        assert_eq!(TransactionState::Aborted.to_string(), "aborted");
    }

    #[test]
    fn test_transaction_state_from_str_ongoing() {
        assert_eq!(
            "ongoing".parse::<TransactionState>().unwrap(),
            TransactionState::Ongoing
        );
    }

    #[test]
    fn test_transaction_state_from_str_preparing() {
        assert_eq!(
            "preparing".parse::<TransactionState>().unwrap(),
            TransactionState::Preparing
        );
    }

    #[test]
    fn test_transaction_state_from_str_committed() {
        assert_eq!(
            "committed".parse::<TransactionState>().unwrap(),
            TransactionState::Committed
        );
    }

    #[test]
    fn test_transaction_state_from_str_aborted() {
        assert_eq!(
            "aborted".parse::<TransactionState>().unwrap(),
            TransactionState::Aborted
        );
    }

    #[test]
    fn test_transaction_state_from_str_case_insensitive() {
        assert_eq!(
            "ONGOING".parse::<TransactionState>().unwrap(),
            TransactionState::Ongoing
        );
        assert_eq!(
            "Preparing".parse::<TransactionState>().unwrap(),
            TransactionState::Preparing
        );
        assert_eq!(
            "COMMITTED".parse::<TransactionState>().unwrap(),
            TransactionState::Committed
        );
        assert_eq!(
            "ABORTED".parse::<TransactionState>().unwrap(),
            TransactionState::Aborted
        );
    }

    #[test]
    fn test_transaction_state_from_str_invalid() {
        let result = "invalid".parse::<TransactionState>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown transaction state"));
    }

    #[test]
    fn test_transaction_state_default() {
        assert_eq!(TransactionState::default(), TransactionState::Ongoing);
    }

    #[test]
    fn test_transaction_state_display_from_str_roundtrip() {
        for state in [
            TransactionState::Ongoing,
            TransactionState::Preparing,
            TransactionState::Committed,
            TransactionState::Aborted,
        ] {
            let s = state.to_string();
            let parsed: TransactionState = s.parse().unwrap();
            assert_eq!(parsed, state, "Roundtrip failed for {:?}", state);
        }
    }

    #[test]
    fn test_transaction_state_serde_roundtrip() {
        for state in [
            TransactionState::Ongoing,
            TransactionState::Preparing,
            TransactionState::Committed,
            TransactionState::Aborted,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: TransactionState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state, "Serde roundtrip failed for {:?}", state);
        }
    }

    #[test]
    fn test_transaction_state_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&TransactionState::Ongoing).unwrap(),
            "\"ongoing\""
        );
        assert_eq!(
            serde_json::to_string(&TransactionState::Preparing).unwrap(),
            "\"preparing\""
        );
        assert_eq!(
            serde_json::to_string(&TransactionState::Committed).unwrap(),
            "\"committed\""
        );
        assert_eq!(
            serde_json::to_string(&TransactionState::Aborted).unwrap(),
            "\"aborted\""
        );
    }

    // ========================================================================
    // LeaderChangeReason Tests
    // ========================================================================

    #[test]
    fn test_leader_change_reason_display_lease_expired() {
        assert_eq!(
            LeaderChangeReason::LeaseExpired.to_string(),
            "lease_expired"
        );
    }

    #[test]
    fn test_leader_change_reason_display_graceful_handoff() {
        assert_eq!(
            LeaderChangeReason::GracefulHandoff.to_string(),
            "graceful_handoff"
        );
    }

    #[test]
    fn test_leader_change_reason_display_agent_crash() {
        assert_eq!(LeaderChangeReason::AgentCrash.to_string(), "agent_crash");
    }

    #[test]
    fn test_leader_change_reason_display_rebalance() {
        assert_eq!(LeaderChangeReason::Rebalance.to_string(), "rebalance");
    }

    #[test]
    fn test_leader_change_reason_display_initial() {
        assert_eq!(LeaderChangeReason::Initial.to_string(), "initial");
    }

    #[test]
    fn test_leader_change_reason_from_str_all_variants() {
        assert_eq!(
            "lease_expired".parse::<LeaderChangeReason>().unwrap(),
            LeaderChangeReason::LeaseExpired
        );
        assert_eq!(
            "graceful_handoff".parse::<LeaderChangeReason>().unwrap(),
            LeaderChangeReason::GracefulHandoff
        );
        assert_eq!(
            "agent_crash".parse::<LeaderChangeReason>().unwrap(),
            LeaderChangeReason::AgentCrash
        );
        assert_eq!(
            "rebalance".parse::<LeaderChangeReason>().unwrap(),
            LeaderChangeReason::Rebalance
        );
        assert_eq!(
            "initial".parse::<LeaderChangeReason>().unwrap(),
            LeaderChangeReason::Initial
        );
    }

    #[test]
    fn test_leader_change_reason_from_str_case_insensitive() {
        assert_eq!(
            "LEASE_EXPIRED".parse::<LeaderChangeReason>().unwrap(),
            LeaderChangeReason::LeaseExpired
        );
        assert_eq!(
            "Graceful_Handoff".parse::<LeaderChangeReason>().unwrap(),
            LeaderChangeReason::GracefulHandoff
        );
    }

    #[test]
    fn test_leader_change_reason_from_str_invalid() {
        let result = "invalid".parse::<LeaderChangeReason>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown leader change reason"));
    }

    #[test]
    fn test_leader_change_reason_default() {
        assert_eq!(LeaderChangeReason::default(), LeaderChangeReason::Initial);
    }

    #[test]
    fn test_leader_change_reason_display_from_str_roundtrip() {
        for reason in [
            LeaderChangeReason::LeaseExpired,
            LeaderChangeReason::GracefulHandoff,
            LeaderChangeReason::AgentCrash,
            LeaderChangeReason::Rebalance,
            LeaderChangeReason::Initial,
        ] {
            let s = reason.to_string();
            let parsed: LeaderChangeReason = s.parse().unwrap();
            assert_eq!(parsed, reason, "Roundtrip failed for {:?}", reason);
        }
    }

    #[test]
    fn test_leader_change_reason_serde_roundtrip() {
        for reason in [
            LeaderChangeReason::LeaseExpired,
            LeaderChangeReason::GracefulHandoff,
            LeaderChangeReason::AgentCrash,
            LeaderChangeReason::Rebalance,
            LeaderChangeReason::Initial,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            let parsed: LeaderChangeReason = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, reason, "Serde roundtrip failed for {:?}", reason);
        }
    }

    #[test]
    fn test_leader_change_reason_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&LeaderChangeReason::LeaseExpired).unwrap(),
            "\"lease_expired\""
        );
        assert_eq!(
            serde_json::to_string(&LeaderChangeReason::GracefulHandoff).unwrap(),
            "\"graceful_handoff\""
        );
        assert_eq!(
            serde_json::to_string(&LeaderChangeReason::AgentCrash).unwrap(),
            "\"agent_crash\""
        );
        assert_eq!(
            serde_json::to_string(&LeaderChangeReason::Rebalance).unwrap(),
            "\"rebalance\""
        );
        assert_eq!(
            serde_json::to_string(&LeaderChangeReason::Initial).unwrap(),
            "\"initial\""
        );
    }

    // ========================================================================
    // LeaseTransferState Tests
    // ========================================================================

    #[test]
    fn test_lease_transfer_state_display_all_variants() {
        assert_eq!(LeaseTransferState::Pending.to_string(), "pending");
        assert_eq!(LeaseTransferState::Accepted.to_string(), "accepted");
        assert_eq!(LeaseTransferState::Completing.to_string(), "completing");
        assert_eq!(LeaseTransferState::Completed.to_string(), "completed");
        assert_eq!(LeaseTransferState::Rejected.to_string(), "rejected");
        assert_eq!(LeaseTransferState::TimedOut.to_string(), "timed_out");
        assert_eq!(LeaseTransferState::Failed.to_string(), "failed");
    }

    #[test]
    fn test_lease_transfer_state_from_str_all_variants() {
        assert_eq!(
            "pending".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Pending
        );
        assert_eq!(
            "accepted".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Accepted
        );
        assert_eq!(
            "completing".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Completing
        );
        assert_eq!(
            "completed".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Completed
        );
        assert_eq!(
            "rejected".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Rejected
        );
        assert_eq!(
            "timed_out".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::TimedOut
        );
        assert_eq!(
            "failed".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Failed
        );
    }

    #[test]
    fn test_lease_transfer_state_from_str_case_insensitive() {
        assert_eq!(
            "PENDING".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Pending
        );
        assert_eq!(
            "TIMED_OUT".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::TimedOut
        );
        assert_eq!(
            "Completed".parse::<LeaseTransferState>().unwrap(),
            LeaseTransferState::Completed
        );
    }

    #[test]
    fn test_lease_transfer_state_from_str_invalid() {
        let result = "invalid".parse::<LeaseTransferState>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown transfer state"));
    }

    #[test]
    fn test_lease_transfer_state_default() {
        assert_eq!(LeaseTransferState::default(), LeaseTransferState::Pending);
    }

    #[test]
    fn test_lease_transfer_state_display_from_str_roundtrip() {
        for state in [
            LeaseTransferState::Pending,
            LeaseTransferState::Accepted,
            LeaseTransferState::Completing,
            LeaseTransferState::Completed,
            LeaseTransferState::Rejected,
            LeaseTransferState::TimedOut,
            LeaseTransferState::Failed,
        ] {
            let s = state.to_string();
            let parsed: LeaseTransferState = s.parse().unwrap();
            assert_eq!(parsed, state, "Roundtrip failed for {:?}", state);
        }
    }

    #[test]
    fn test_lease_transfer_state_serde_roundtrip() {
        for state in [
            LeaseTransferState::Pending,
            LeaseTransferState::Accepted,
            LeaseTransferState::Completing,
            LeaseTransferState::Completed,
            LeaseTransferState::Rejected,
            LeaseTransferState::TimedOut,
            LeaseTransferState::Failed,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: LeaseTransferState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state, "Serde roundtrip failed for {:?}", state);
        }
    }

    #[test]
    fn test_lease_transfer_state_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&LeaseTransferState::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&LeaseTransferState::Accepted).unwrap(),
            "\"accepted\""
        );
        assert_eq!(
            serde_json::to_string(&LeaseTransferState::Completing).unwrap(),
            "\"completing\""
        );
        assert_eq!(
            serde_json::to_string(&LeaseTransferState::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&LeaseTransferState::Rejected).unwrap(),
            "\"rejected\""
        );
        assert_eq!(
            serde_json::to_string(&LeaseTransferState::TimedOut).unwrap(),
            "\"timed_out\""
        );
        assert_eq!(
            serde_json::to_string(&LeaseTransferState::Failed).unwrap(),
            "\"failed\""
        );
    }

    // ========================================================================
    // AckMode Tests
    // ========================================================================

    #[test]
    fn test_ack_mode_default() {
        assert_eq!(AckMode::default(), AckMode::Buffered);
    }

    #[test]
    fn test_ack_mode_serde_roundtrip() {
        for mode in [AckMode::Buffered, AckMode::Durable, AckMode::None] {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: AckMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode, "Serde roundtrip failed for {:?}", mode);
        }
    }

    #[test]
    fn test_ack_mode_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&AckMode::Buffered).unwrap(),
            "\"buffered\""
        );
        assert_eq!(
            serde_json::to_string(&AckMode::Durable).unwrap(),
            "\"durable\""
        );
        assert_eq!(serde_json::to_string(&AckMode::None).unwrap(), "\"none\"");
    }

    #[test]
    fn test_ack_mode_deserialize_from_json() {
        let buffered: AckMode = serde_json::from_str("\"buffered\"").unwrap();
        assert_eq!(buffered, AckMode::Buffered);

        let durable: AckMode = serde_json::from_str("\"durable\"").unwrap();
        assert_eq!(durable, AckMode::Durable);

        let none: AckMode = serde_json::from_str("\"none\"").unwrap();
        assert_eq!(none, AckMode::None);
    }

    #[test]
    fn test_ack_mode_deserialize_invalid() {
        let result: Result<AckMode, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_ack_mode_equality() {
        assert_eq!(AckMode::Buffered, AckMode::Buffered);
        assert_ne!(AckMode::Buffered, AckMode::Durable);
        assert_ne!(AckMode::Durable, AckMode::None);
    }

    #[test]
    fn test_ack_mode_clone_and_copy() {
        let mode = AckMode::Durable;
        let cloned = mode.clone();
        let copied = mode;
        assert_eq!(mode, cloned);
        assert_eq!(mode, copied);
    }

    // ========================================================================
    // IsolationLevel Tests
    // ========================================================================

    #[test]
    fn test_isolation_level_default() {
        assert_eq!(IsolationLevel::default(), IsolationLevel::ReadUncommitted);
    }

    #[test]
    fn test_isolation_level_serde_roundtrip() {
        for level in [
            IsolationLevel::ReadUncommitted,
            IsolationLevel::ReadCommitted,
        ] {
            let json = serde_json::to_string(&level).unwrap();
            let parsed: IsolationLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, level, "Serde roundtrip failed for {:?}", level);
        }
    }

    #[test]
    fn test_isolation_level_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&IsolationLevel::ReadUncommitted).unwrap(),
            "\"read_uncommitted\""
        );
        assert_eq!(
            serde_json::to_string(&IsolationLevel::ReadCommitted).unwrap(),
            "\"read_committed\""
        );
    }

    #[test]
    fn test_isolation_level_deserialize_from_json() {
        let uncommitted: IsolationLevel = serde_json::from_str("\"read_uncommitted\"").unwrap();
        assert_eq!(uncommitted, IsolationLevel::ReadUncommitted);

        let committed: IsolationLevel = serde_json::from_str("\"read_committed\"").unwrap();
        assert_eq!(committed, IsolationLevel::ReadCommitted);
    }

    #[test]
    fn test_isolation_level_deserialize_invalid() {
        let result: Result<IsolationLevel, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_isolation_level_equality() {
        assert_eq!(
            IsolationLevel::ReadUncommitted,
            IsolationLevel::ReadUncommitted
        );
        assert_ne!(
            IsolationLevel::ReadUncommitted,
            IsolationLevel::ReadCommitted
        );
    }

    // ========================================================================
    // TopicConfig Tests
    // ========================================================================

    #[test]
    fn test_topic_config_default() {
        let config = TopicConfig::default();
        assert_eq!(config.name, "");
        assert_eq!(config.partition_count, 0);
        assert_eq!(config.retention_ms, None);
        assert_eq!(config.cleanup_policy, CleanupPolicy::Delete);
        assert!(config.config.is_empty());
    }

    #[test]
    fn test_topic_config_serde_roundtrip() {
        let config = TopicConfig {
            name: "orders".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            cleanup_policy: CleanupPolicy::Compact,
            config: HashMap::from([
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
            ]),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: TopicConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "orders");
        assert_eq!(parsed.partition_count, 3);
        assert_eq!(parsed.retention_ms, Some(86400000));
        assert_eq!(parsed.cleanup_policy, CleanupPolicy::Compact);
        assert_eq!(parsed.config.len(), 2);
        assert_eq!(parsed.config.get("key1").unwrap(), "value1");
    }

    #[test]
    fn test_topic_config_serde_with_no_retention() {
        let config = TopicConfig {
            name: "audit_log".to_string(),
            partition_count: 1,
            retention_ms: None,
            cleanup_policy: CleanupPolicy::Delete,
            config: HashMap::new(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: TopicConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.retention_ms, None);
    }

    #[test]
    fn test_topic_config_serde_missing_optional_fields() {
        // JSON with only required fields; cleanup_policy and config have serde(default)
        let json = r#"{"name":"test","partition_count":1,"retention_ms":null}"#;
        let parsed: TopicConfig = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.partition_count, 1);
        assert_eq!(parsed.retention_ms, None);
        assert_eq!(parsed.cleanup_policy, CleanupPolicy::Delete);
        assert!(parsed.config.is_empty());
    }

    #[test]
    fn test_topic_config_serde_with_compact_and_delete() {
        let config = TopicConfig {
            name: "compacted_topic".to_string(),
            partition_count: 5,
            retention_ms: Some(3600000),
            cleanup_policy: CleanupPolicy::CompactAndDelete,
            config: HashMap::new(),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("compact,delete"));

        let parsed: TopicConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cleanup_policy, CleanupPolicy::CompactAndDelete);
    }

    #[test]
    fn test_topic_config_clone() {
        let config = TopicConfig {
            name: "orders".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            cleanup_policy: CleanupPolicy::Compact,
            config: HashMap::from([("key".to_string(), "val".to_string())]),
        };

        let cloned = config.clone();
        assert_eq!(cloned.name, "orders");
        assert_eq!(cloned.partition_count, 3);
        assert_eq!(cloned.config.get("key").unwrap(), "val");
    }

    // ========================================================================
    // ConsumerGroupConfig Tests
    // ========================================================================

    #[test]
    fn test_consumer_group_config_default() {
        let config = ConsumerGroupConfig::default();
        assert_eq!(config.group_id, "");
        assert_eq!(config.isolation_level, IsolationLevel::ReadUncommitted);
        assert!(config.enable_auto_commit);
        assert_eq!(config.auto_commit_interval_ms, 5000);
        assert_eq!(config.session_timeout_ms, 30000);
        assert_eq!(config.created_at, 0);
        assert_eq!(config.updated_at, 0);
    }

    #[test]
    fn test_consumer_group_config_default_auto_commit_enabled() {
        let config = ConsumerGroupConfig::default();
        assert!(config.enable_auto_commit);
    }

    #[test]
    fn test_consumer_group_config_default_auto_commit_interval() {
        let config = ConsumerGroupConfig::default();
        assert_eq!(config.auto_commit_interval_ms, 5000);
    }

    #[test]
    fn test_consumer_group_config_default_session_timeout() {
        let config = ConsumerGroupConfig::default();
        assert_eq!(config.session_timeout_ms, 30000);
    }

    #[test]
    fn test_consumer_group_config_serde_roundtrip() {
        let config = ConsumerGroupConfig {
            group_id: "analytics".to_string(),
            isolation_level: IsolationLevel::ReadCommitted,
            enable_auto_commit: false,
            auto_commit_interval_ms: 10000,
            session_timeout_ms: 60000,
            created_at: 1000,
            updated_at: 2000,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ConsumerGroupConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.group_id, "analytics");
        assert_eq!(parsed.isolation_level, IsolationLevel::ReadCommitted);
        assert!(!parsed.enable_auto_commit);
        assert_eq!(parsed.auto_commit_interval_ms, 10000);
        assert_eq!(parsed.session_timeout_ms, 60000);
        assert_eq!(parsed.created_at, 1000);
        assert_eq!(parsed.updated_at, 2000);
    }

    // ========================================================================
    // OrganizationQuota Tests
    // ========================================================================

    #[test]
    fn test_organization_quota_default() {
        let quota = OrganizationQuota::default();
        assert_eq!(quota.organization_id, "");
        assert_eq!(quota.max_topics, 10);
        assert_eq!(quota.max_partitions_per_topic, 12);
        assert_eq!(quota.max_total_partitions, 100);
        assert_eq!(quota.max_storage_bytes, 10_737_418_240); // 10 GB
        assert_eq!(quota.max_retention_days, 7);
        assert_eq!(quota.max_produce_bytes_per_sec, 10_485_760); // 10 MB/s
        assert_eq!(quota.max_consume_bytes_per_sec, 52_428_800); // 50 MB/s
        assert_eq!(quota.max_requests_per_sec, 1000);
        assert_eq!(quota.max_consumer_groups, 50);
        assert_eq!(quota.max_schemas, 100);
        assert_eq!(quota.max_schema_versions_per_subject, 100);
        assert_eq!(quota.max_connections, 100);
    }

    #[test]
    fn test_organization_quota_default_storage_is_10gb() {
        let quota = OrganizationQuota::default();
        assert_eq!(quota.max_storage_bytes, 10 * 1024 * 1024 * 1024); // 10 GB
    }

    #[test]
    fn test_organization_quota_default_produce_rate_is_10mb() {
        let quota = OrganizationQuota::default();
        assert_eq!(quota.max_produce_bytes_per_sec, 10 * 1024 * 1024); // 10 MB/s
    }

    #[test]
    fn test_organization_quota_default_consume_rate_is_50mb() {
        let quota = OrganizationQuota::default();
        assert_eq!(quota.max_consume_bytes_per_sec, 50 * 1024 * 1024); // 50 MB/s
    }

    #[test]
    fn test_organization_quota_serde_roundtrip() {
        let quota = OrganizationQuota {
            organization_id: "test-org".to_string(),
            max_topics: 50,
            max_partitions_per_topic: 24,
            max_total_partitions: 500,
            max_storage_bytes: 5_000_000_000,
            max_retention_days: 30,
            max_produce_bytes_per_sec: 50_000_000,
            max_consume_bytes_per_sec: 250_000_000,
            max_requests_per_sec: 5000,
            max_consumer_groups: 200,
            max_schemas: 500,
            max_schema_versions_per_subject: 200,
            max_connections: 500,
        };

        let json = serde_json::to_string(&quota).unwrap();
        let parsed: OrganizationQuota = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.organization_id, "test-org");
        assert_eq!(parsed.max_topics, 50);
        assert_eq!(parsed.max_partitions_per_topic, 24);
        assert_eq!(parsed.max_total_partitions, 500);
        assert_eq!(parsed.max_storage_bytes, 5_000_000_000);
        assert_eq!(parsed.max_retention_days, 30);
        assert_eq!(parsed.max_produce_bytes_per_sec, 50_000_000);
        assert_eq!(parsed.max_consume_bytes_per_sec, 250_000_000);
        assert_eq!(parsed.max_requests_per_sec, 5000);
        assert_eq!(parsed.max_consumer_groups, 200);
        assert_eq!(parsed.max_schemas, 500);
        assert_eq!(parsed.max_schema_versions_per_subject, 200);
        assert_eq!(parsed.max_connections, 500);
    }

    // ========================================================================
    // TransactionMarkerType Tests
    // ========================================================================

    #[test]
    fn test_transaction_marker_type_serde_roundtrip() {
        for marker_type in [TransactionMarkerType::Commit, TransactionMarkerType::Abort] {
            let json = serde_json::to_string(&marker_type).unwrap();
            let parsed: TransactionMarkerType = serde_json::from_str(&json).unwrap();
            assert_eq!(
                parsed, marker_type,
                "Serde roundtrip failed for {:?}",
                marker_type
            );
        }
    }

    #[test]
    fn test_transaction_marker_type_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&TransactionMarkerType::Commit).unwrap(),
            "\"commit\""
        );
        assert_eq!(
            serde_json::to_string(&TransactionMarkerType::Abort).unwrap(),
            "\"abort\""
        );
    }

    #[test]
    fn test_transaction_marker_type_equality() {
        assert_eq!(TransactionMarkerType::Commit, TransactionMarkerType::Commit);
        assert_ne!(TransactionMarkerType::Commit, TransactionMarkerType::Abort);
    }

    // ========================================================================
    // MaterializedViewRefreshMode Tests
    // ========================================================================

    #[test]
    fn test_materialized_view_refresh_mode_default() {
        assert_eq!(
            MaterializedViewRefreshMode::default(),
            MaterializedViewRefreshMode::Continuous
        );
    }

    #[test]
    fn test_materialized_view_refresh_mode_serde_continuous() {
        let mode = MaterializedViewRefreshMode::Continuous;
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: MaterializedViewRefreshMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MaterializedViewRefreshMode::Continuous);
    }

    #[test]
    fn test_materialized_view_refresh_mode_serde_periodic() {
        let mode = MaterializedViewRefreshMode::Periodic { interval_ms: 60000 };
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: MaterializedViewRefreshMode = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed,
            MaterializedViewRefreshMode::Periodic { interval_ms: 60000 }
        );
    }

    #[test]
    fn test_materialized_view_refresh_mode_serde_manual() {
        let mode = MaterializedViewRefreshMode::Manual;
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: MaterializedViewRefreshMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MaterializedViewRefreshMode::Manual);
    }

    // ========================================================================
    // MaterializedViewStatus Tests
    // ========================================================================

    #[test]
    fn test_materialized_view_status_default() {
        assert_eq!(
            MaterializedViewStatus::default(),
            MaterializedViewStatus::Initializing
        );
    }

    #[test]
    fn test_materialized_view_status_serde_roundtrip() {
        for status in [
            MaterializedViewStatus::Initializing,
            MaterializedViewStatus::Running,
            MaterializedViewStatus::Paused,
            MaterializedViewStatus::Error,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: MaterializedViewStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status, "Serde roundtrip failed for {:?}", status);
        }
    }

    // ========================================================================
    // DEFAULT_ORGANIZATION_ID Constant Test
    // ========================================================================

    #[test]
    fn test_default_organization_id() {
        assert_eq!(
            DEFAULT_ORGANIZATION_ID,
            "00000000-0000-0000-0000-000000000000"
        );
        // Should look like a nil UUID
        assert_eq!(DEFAULT_ORGANIZATION_ID.len(), 36);
    }

    // ========================================================================
    // Struct Serialization Tests (complex types)
    // ========================================================================

    #[test]
    fn test_topic_serde_roundtrip() {
        let topic = Topic {
            name: "orders".to_string(),
            partition_count: 3,
            retention_ms: Some(86400000),
            cleanup_policy: CleanupPolicy::Compact,
            created_at: 1700000000000,
            config: HashMap::from([("key".to_string(), "value".to_string())]),
        };

        let json = serde_json::to_string(&topic).unwrap();
        let parsed: Topic = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "orders");
        assert_eq!(parsed.partition_count, 3);
        assert_eq!(parsed.retention_ms, Some(86400000));
        assert_eq!(parsed.cleanup_policy, CleanupPolicy::Compact);
        assert_eq!(parsed.created_at, 1700000000000);
        assert_eq!(parsed.config.get("key").unwrap(), "value");
    }

    #[test]
    fn test_partition_serde_roundtrip() {
        let partition = Partition {
            topic: "orders".to_string(),
            partition_id: 0,
            high_watermark: 12345,
        };

        let json = serde_json::to_string(&partition).unwrap();
        let parsed: Partition = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.high_watermark, 12345);
    }

    #[test]
    fn test_segment_info_serde_roundtrip() {
        let segment = SegmentInfo {
            id: "orders-0-1000".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            base_offset: 1000,
            end_offset: 1999,
            record_count: 1000,
            size_bytes: 67_108_864,
            s3_bucket: "streamhouse".to_string(),
            s3_key: "orders/0/seg_1000.bin".to_string(),
            created_at: 1700000000000,
        };

        let json = serde_json::to_string(&segment).unwrap();
        let parsed: SegmentInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "orders-0-1000");
        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.base_offset, 1000);
        assert_eq!(parsed.end_offset, 1999);
        assert_eq!(parsed.record_count, 1000);
        assert_eq!(parsed.size_bytes, 67_108_864);
        assert_eq!(parsed.s3_bucket, "streamhouse");
        assert_eq!(parsed.s3_key, "orders/0/seg_1000.bin");
    }

    #[test]
    fn test_consumer_offset_serde_roundtrip() {
        let offset = ConsumerOffset {
            group_id: "analytics".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            committed_offset: 5000,
            metadata: Some("checkpoint-abc".to_string()),
        };

        let json = serde_json::to_string(&offset).unwrap();
        let parsed: ConsumerOffset = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.group_id, "analytics");
        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.committed_offset, 5000);
        assert_eq!(parsed.metadata, Some("checkpoint-abc".to_string()));
    }

    #[test]
    fn test_consumer_offset_serde_no_metadata() {
        let offset = ConsumerOffset {
            group_id: "analytics".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            committed_offset: 5000,
            metadata: None,
        };

        let json = serde_json::to_string(&offset).unwrap();
        let parsed: ConsumerOffset = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.metadata, None);
    }

    #[test]
    fn test_agent_info_serde_roundtrip() {
        let agent = AgentInfo {
            agent_id: "agent-us-east-1a-001".to_string(),
            address: "10.0.1.5:9090".to_string(),
            availability_zone: "us-east-1a".to_string(),
            agent_group: "prod".to_string(),
            last_heartbeat: 1700000000000,
            started_at: 1699999999000,
            metadata: HashMap::from([("version".to_string(), "1.0.0".to_string())]),
        };

        let json = serde_json::to_string(&agent).unwrap();
        let parsed: AgentInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.agent_id, "agent-us-east-1a-001");
        assert_eq!(parsed.address, "10.0.1.5:9090");
        assert_eq!(parsed.availability_zone, "us-east-1a");
        assert_eq!(parsed.agent_group, "prod");
        assert_eq!(parsed.last_heartbeat, 1700000000000);
        assert_eq!(parsed.started_at, 1699999999000);
        assert_eq!(parsed.metadata.get("version").unwrap(), "1.0.0");
    }

    #[test]
    fn test_partition_lease_serde_roundtrip() {
        let lease = PartitionLease {
            topic: "orders".to_string(),
            partition_id: 0,
            leader_agent_id: "agent-001".to_string(),
            lease_expires_at: 1700000030000,
            acquired_at: 1700000000000,
            epoch: 5,
        };

        let json = serde_json::to_string(&lease).unwrap();
        let parsed: PartitionLease = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.leader_agent_id, "agent-001");
        assert_eq!(parsed.lease_expires_at, 1700000030000);
        assert_eq!(parsed.acquired_at, 1700000000000);
        assert_eq!(parsed.epoch, 5);
    }

    #[test]
    fn test_organization_serde_roundtrip() {
        let org = Organization {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            name: "Acme Corporation".to_string(),
            slug: "acme-corp".to_string(),
            plan: OrganizationPlan::Pro,
            status: OrganizationStatus::Active,
            created_at: 1700000000000,
            settings: HashMap::from([("theme".to_string(), "dark".to_string())]),
        };

        let json = serde_json::to_string(&org).unwrap();
        let parsed: Organization = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(parsed.name, "Acme Corporation");
        assert_eq!(parsed.slug, "acme-corp");
        assert_eq!(parsed.plan, OrganizationPlan::Pro);
        assert_eq!(parsed.status, OrganizationStatus::Active);
        assert_eq!(parsed.settings.get("theme").unwrap(), "dark");
    }

    #[test]
    fn test_producer_serde_roundtrip() {
        let producer = Producer {
            id: "producer-001".to_string(),
            organization_id: Some("org-1".to_string()),
            transactional_id: Some("tx-writer-1".to_string()),
            epoch: 3,
            state: ProducerState::Active,
            created_at: 1700000000000,
            last_heartbeat: 1700000010000,
            metadata: Some(HashMap::from([("client".to_string(), "rust".to_string())])),
        };

        let json = serde_json::to_string(&producer).unwrap();
        let parsed: Producer = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "producer-001");
        assert_eq!(parsed.organization_id, Some("org-1".to_string()));
        assert_eq!(parsed.transactional_id, Some("tx-writer-1".to_string()));
        assert_eq!(parsed.epoch, 3);
        assert_eq!(parsed.state, ProducerState::Active);
        assert_eq!(parsed.created_at, 1700000000000);
        assert_eq!(parsed.last_heartbeat, 1700000010000);
    }

    #[test]
    fn test_transaction_serde_roundtrip() {
        let txn = Transaction {
            transaction_id: "txn-001".to_string(),
            producer_id: "producer-001".to_string(),
            state: TransactionState::Preparing,
            timeout_ms: 30000,
            started_at: 1700000000000,
            updated_at: 1700000010000,
            completed_at: None,
        };

        let json = serde_json::to_string(&txn).unwrap();
        let parsed: Transaction = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.transaction_id, "txn-001");
        assert_eq!(parsed.producer_id, "producer-001");
        assert_eq!(parsed.state, TransactionState::Preparing);
        assert_eq!(parsed.timeout_ms, 30000);
        assert_eq!(parsed.completed_at, None);
    }

    #[test]
    fn test_lease_transfer_serde_roundtrip() {
        let transfer = LeaseTransfer {
            transfer_id: "transfer-001".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            from_agent_id: "agent-001".to_string(),
            to_agent_id: "agent-002".to_string(),
            from_epoch: 5,
            state: LeaseTransferState::Accepted,
            reason: LeaderChangeReason::GracefulHandoff,
            initiated_at: 1700000000000,
            completed_at: None,
            timeout_at: 1700000030000,
            last_flushed_offset: Some(1000),
            high_watermark: Some(1001),
            error: None,
        };

        let json = serde_json::to_string(&transfer).unwrap();
        let parsed: LeaseTransfer = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.transfer_id, "transfer-001");
        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.from_agent_id, "agent-001");
        assert_eq!(parsed.to_agent_id, "agent-002");
        assert_eq!(parsed.from_epoch, 5);
        assert_eq!(parsed.state, LeaseTransferState::Accepted);
        assert_eq!(parsed.reason, LeaderChangeReason::GracefulHandoff);
        assert_eq!(parsed.last_flushed_offset, Some(1000));
        assert_eq!(parsed.high_watermark, Some(1001));
        assert_eq!(parsed.error, None);
    }

    #[test]
    fn test_lease_transfer_serde_with_error() {
        let transfer = LeaseTransfer {
            transfer_id: "transfer-002".to_string(),
            topic: "orders".to_string(),
            partition_id: 1,
            from_agent_id: "agent-001".to_string(),
            to_agent_id: "agent-003".to_string(),
            from_epoch: 2,
            state: LeaseTransferState::Failed,
            reason: LeaderChangeReason::AgentCrash,
            initiated_at: 1700000000000,
            completed_at: Some(1700000005000),
            timeout_at: 1700000030000,
            last_flushed_offset: None,
            high_watermark: None,
            error: Some("Agent crashed during transfer".to_string()),
        };

        let json = serde_json::to_string(&transfer).unwrap();
        let parsed: LeaseTransfer = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.state, LeaseTransferState::Failed);
        assert_eq!(
            parsed.error,
            Some("Agent crashed during transfer".to_string())
        );
        assert_eq!(parsed.completed_at, Some(1700000005000));
    }

    // ========================================================================
    // CreateOrganization Tests
    // ========================================================================

    #[test]
    fn test_create_organization_serde_roundtrip() {
        let create_org = CreateOrganization {
            name: "Test Org".to_string(),
            slug: "test-org".to_string(),
            plan: OrganizationPlan::Pro,
            settings: HashMap::from([("region".to_string(), "us-east-1".to_string())]),
        };

        let json = serde_json::to_string(&create_org).unwrap();
        let parsed: CreateOrganization = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "Test Org");
        assert_eq!(parsed.slug, "test-org");
        assert_eq!(parsed.plan, OrganizationPlan::Pro);
        assert_eq!(parsed.settings.get("region").unwrap(), "us-east-1");
    }

    #[test]
    fn test_create_organization_default_plan() {
        // When plan is missing from JSON, it defaults to Free
        let json = r#"{"name":"Test","slug":"test","settings":{}}"#;
        let parsed: CreateOrganization = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.plan, OrganizationPlan::Free);
    }

    // ========================================================================
    // CreateApiKey Tests
    // ========================================================================

    #[test]
    fn test_create_api_key_serde_roundtrip() {
        let create_key = CreateApiKey {
            name: "Production Key".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
            scopes: vec!["orders".to_string(), "events-*".to_string()],
            expires_in_ms: Some(86400000),
        };

        let json = serde_json::to_string(&create_key).unwrap();
        let parsed: CreateApiKey = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "Production Key");
        assert_eq!(parsed.permissions, vec!["read", "write"]);
        assert_eq!(parsed.scopes, vec!["orders", "events-*"]);
        assert_eq!(parsed.expires_in_ms, Some(86400000));
    }

    #[test]
    fn test_create_api_key_default_permissions() {
        // When permissions is missing, it defaults to ["read", "write"]
        let json = r#"{"name":"Test Key","scopes":[],"expires_in_ms":null}"#;
        let parsed: CreateApiKey = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.permissions, vec!["read", "write"]);
    }

    // ========================================================================
    // ApiKey Tests
    // ========================================================================

    #[test]
    fn test_api_key_serde_roundtrip() {
        let api_key = ApiKey {
            id: "key-001".to_string(),
            organization_id: "org-001".to_string(),
            name: "Production Key".to_string(),
            key_prefix: "sk_live_abc1".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
            scopes: vec!["orders".to_string()],
            expires_at: Some(1700000086400),
            last_used_at: Some(1700000000000),
            created_at: 1699999999000,
            created_by: Some("user-001".to_string()),
        };

        let json = serde_json::to_string(&api_key).unwrap();
        let parsed: ApiKey = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "key-001");
        assert_eq!(parsed.organization_id, "org-001");
        assert_eq!(parsed.name, "Production Key");
        assert_eq!(parsed.key_prefix, "sk_live_abc1");
        assert_eq!(parsed.permissions, vec!["read", "write"]);
        assert_eq!(parsed.scopes, vec!["orders"]);
        assert_eq!(parsed.expires_at, Some(1700000086400));
        assert_eq!(parsed.last_used_at, Some(1700000000000));
        assert_eq!(parsed.created_by, Some("user-001".to_string()));
    }

    // ========================================================================
    // ProducerSequence Tests
    // ========================================================================

    #[test]
    fn test_producer_sequence_serde_roundtrip() {
        let seq = ProducerSequence {
            producer_id: "producer-001".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            last_sequence: 99,
            updated_at: 1700000000000,
        };

        let json = serde_json::to_string(&seq).unwrap();
        let parsed: ProducerSequence = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.producer_id, "producer-001");
        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.last_sequence, 99);
    }

    #[test]
    fn test_producer_sequence_initial_value() {
        let seq = ProducerSequence {
            producer_id: "producer-001".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            last_sequence: -1,
            updated_at: 0,
        };

        assert_eq!(seq.last_sequence, -1);
    }

    // ========================================================================
    // OrganizationUsage Tests
    // ========================================================================

    #[test]
    fn test_organization_usage_serde_roundtrip() {
        let usage = OrganizationUsage {
            organization_id: "org-001".to_string(),
            metric: "storage_bytes".to_string(),
            value: 5_000_000_000,
            period_start: 1700000000000,
        };

        let json = serde_json::to_string(&usage).unwrap();
        let parsed: OrganizationUsage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.organization_id, "org-001");
        assert_eq!(parsed.metric, "storage_bytes");
        assert_eq!(parsed.value, 5_000_000_000);
        assert_eq!(parsed.period_start, 1700000000000);
    }

    // ========================================================================
    // PartitionLSO Tests
    // ========================================================================

    #[test]
    fn test_partition_lso_serde_roundtrip() {
        let lso = PartitionLSO {
            topic: "orders".to_string(),
            partition_id: 0,
            last_stable_offset: 500,
            updated_at: 1700000000000,
        };

        let json = serde_json::to_string(&lso).unwrap();
        let parsed: PartitionLSO = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.last_stable_offset, 500);
    }

    // ========================================================================
    // TransactionPartition Tests
    // ========================================================================

    #[test]
    fn test_transaction_partition_serde_roundtrip() {
        let tp = TransactionPartition {
            transaction_id: "txn-001".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            first_offset: 100,
            last_offset: 200,
        };

        let json = serde_json::to_string(&tp).unwrap();
        let parsed: TransactionPartition = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.transaction_id, "txn-001");
        assert_eq!(parsed.topic, "orders");
        assert_eq!(parsed.partition_id, 0);
        assert_eq!(parsed.first_offset, 100);
        assert_eq!(parsed.last_offset, 200);
    }

    // ========================================================================
    // TransactionMarker Tests
    // ========================================================================

    #[test]
    fn test_transaction_marker_serde_roundtrip() {
        let marker = TransactionMarker {
            transaction_id: "txn-001".to_string(),
            topic: "orders".to_string(),
            partition_id: 0,
            offset: 201,
            marker_type: TransactionMarkerType::Commit,
            timestamp: 1700000000000,
        };

        let json = serde_json::to_string(&marker).unwrap();
        let parsed: TransactionMarker = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.transaction_id, "txn-001");
        assert_eq!(parsed.marker_type, TransactionMarkerType::Commit);
        assert_eq!(parsed.offset, 201);
    }

    // ========================================================================
    // InitProducerConfig Tests
    // ========================================================================

    #[test]
    fn test_init_producer_config_serde_roundtrip() {
        let config = InitProducerConfig {
            transactional_id: Some("tx-writer-1".to_string()),
            organization_id: Some("org-1".to_string()),
            timeout_ms: 60000,
            metadata: Some(HashMap::from([(
                "client".to_string(),
                "rust-client".to_string(),
            )])),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: InitProducerConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.transactional_id, Some("tx-writer-1".to_string()));
        assert_eq!(parsed.organization_id, Some("org-1".to_string()));
        assert_eq!(parsed.timeout_ms, 60000);
    }

    #[test]
    fn test_init_producer_config_serde_minimal() {
        let config = InitProducerConfig {
            transactional_id: None,
            organization_id: None,
            timeout_ms: 30000,
            metadata: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: InitProducerConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.transactional_id, None);
        assert_eq!(parsed.organization_id, None);
        assert_eq!(parsed.timeout_ms, 30000);
        assert_eq!(parsed.metadata, None);
    }

    // ========================================================================
    // MaterializedView Tests
    // ========================================================================

    #[test]
    fn test_materialized_view_serde_roundtrip() {
        let view = MaterializedView {
            id: "view-001".to_string(),
            organization_id: "org-001".to_string(),
            name: "order_counts".to_string(),
            source_topic: "orders".to_string(),
            query_sql: "SELECT status, COUNT(*) as cnt FROM orders GROUP BY status".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Continuous,
            status: MaterializedViewStatus::Running,
            error_message: None,
            row_count: 42,
            last_refresh_at: Some(1700000000000),
            created_at: 1699999999000,
            updated_at: 1700000000000,
        };

        let json = serde_json::to_string(&view).unwrap();
        let parsed: MaterializedView = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "view-001");
        assert_eq!(parsed.name, "order_counts");
        assert_eq!(parsed.source_topic, "orders");
        assert_eq!(parsed.status, MaterializedViewStatus::Running);
        assert_eq!(parsed.row_count, 42);
    }

    #[test]
    fn test_materialized_view_with_error() {
        let view = MaterializedView {
            id: "view-002".to_string(),
            organization_id: "org-001".to_string(),
            name: "broken_view".to_string(),
            source_topic: "orders".to_string(),
            query_sql: "INVALID SQL".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Manual,
            status: MaterializedViewStatus::Error,
            error_message: Some("Parse error: unexpected token".to_string()),
            row_count: 0,
            last_refresh_at: None,
            created_at: 1699999999000,
            updated_at: 1700000000000,
        };

        let json = serde_json::to_string(&view).unwrap();
        let parsed: MaterializedView = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.status, MaterializedViewStatus::Error);
        assert_eq!(
            parsed.error_message,
            Some("Parse error: unexpected token".to_string())
        );
    }
}
