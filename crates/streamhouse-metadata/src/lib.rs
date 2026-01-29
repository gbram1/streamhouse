//! StreamHouse Metadata Store
//!
//! This crate implements the metadata tracking system - the "brain" that knows where
//! everything is stored and tracks consumer progress.
//!
//! ## Purpose
//!
//! While segments store actual event data in S3, the metadata store tracks:
//! - **Topics**: What streams exist and their configuration (partition count, retention)
//! - **Partitions**: Division of topics and their current high watermark (latest offset)
//! - **Segments**: Which S3 files contain which offset ranges
//! - **Consumer Groups**: Which offsets each consumer group has processed
//!
//! ## Why Do We Need This?
//!
//! Without metadata, simple questions become impossible to answer efficiently:
//! - "Give me records from offset 5000" → Which S3 file?
//! - "What's the latest offset?" → Must list all S3 files
//! - "Where did my consumer leave off?" → No tracking
//!
//! With metadata, these queries are **instant** (< 1ms).
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐
//! │   Producer   │
//! └──────┬───────┘
//!        │ writes
//!        ▼
//! ┌──────────────┐     ┌─────────────────┐
//! │      S3      │ ←──→│ Metadata Store  │ ◄── You are here
//! │  (Segments)  │     │ (SQLite/Postgres)│
//! └──────────────┘     └────────┬────────┘
//!                               │ queries
//!                      ┌────────┴─────────┐
//!                      │    Consumer      │
//!                      └──────────────────┘
//! ```
//!
//! ## Usage Example
//!
//! ```ignore
//! use streamhouse_metadata::{SqliteMetadataStore, MetadataStore, TopicConfig};
//!
//! // Create store
//! let store = SqliteMetadataStore::new("metadata.db").await?;
//!
//! // Create a topic
//! store.create_topic(TopicConfig {
//!     name: "orders".to_string(),
//!     partition_count: 3,
//!     retention_ms: Some(7 * 24 * 60 * 60 * 1000), // 7 days
//!     config: HashMap::new(),
//! }).await?;
//!
//! // Register a segment after writing to S3
//! store.add_segment(SegmentInfo {
//!     id: "orders-0-0".to_string(),
//!     topic: "orders".to_string(),
//!     partition_id: 0,
//!     base_offset: 0,
//!     end_offset: 99_999,
//!     record_count: 100_000,
//!     size_bytes: 67_108_864, // 64MB
//!     s3_bucket: "streamhouse".to_string(),
//!     s3_key: "orders/0/seg_0.bin".to_string(),
//!     created_at: now_ms(),
//! }).await?;
//!
//! // Find which segment contains offset 50,000
//! let segment = store.find_segment_for_offset("orders", 0, 50_000).await?.unwrap();
//! println!("Download: s3://{}/{}", segment.s3_bucket, segment.s3_key);
//!
//! // Track consumer progress
//! store.commit_offset("analytics", "orders", 0, 50_000, None).await?;
//! ```
//!
//! ## Performance
//!
//! ### SQLite (Phase 1)
//! - **Reads**: 100K+ queries/sec
//! - **Writes**: 50K+ inserts/sec (with transactions)
//! - **Latency**: < 1ms for indexed queries
//!
//! ### Query Performance
//! - Get topic by name: **< 100µs**
//! - Find segment for offset: **< 1ms** (indexed)
//! - Get consumer offset: **< 100µs** (primary key)
//!
//! ## Implementation Details
//!
//! ### Database Backend
//! - **Phase 1**: SQLite (embedded, zero-config, single-node)
//! - **Phase 4**: PostgreSQL or FoundationDB (distributed)
//!
//! ### Schema Design
//! - Foreign keys for referential integrity (cascade deletes)
//! - Indexes on all query patterns
//! - CHECK constraints for data validation
//! - Timestamps as i64 (milliseconds since epoch)
//! - JSON config fields for flexibility
//!
//! ### Thread Safety
//! - SQLx connection pool handles concurrent access
//! - ACID transactions ensure consistency
//! - Safe to share across async tasks via Arc<>

pub mod cached_store;
pub mod error;
pub mod store;
pub mod types;

#[cfg(feature = "postgres")]
pub mod postgres;

pub use cached_store::{CacheConfig, CacheMetrics, CachedMetadataStore};
pub use error::{MetadataError, Result};
pub use store::SqliteMetadataStore;
pub use types::*;

#[cfg(feature = "postgres")]
pub use postgres::PostgresMetadataStore;

use async_trait::async_trait;

/// Metadata store trait - abstracts over different storage backends.
///
/// This trait defines the core interface for all metadata operations in StreamHouse.
/// It can be implemented by different backends (SQLite, PostgreSQL, FoundationDB, etc.)
/// while maintaining a consistent API for the rest of the system.
///
/// ## Implementations
///
/// - **SqliteMetadataStore**: Phase 1 implementation using SQLite (single-node)
/// - **PostgresMetadataStore**: Phase 4 implementation using PostgreSQL (distributed)
///
/// ## Thread Safety
///
/// All implementations must be Send + Sync, allowing safe sharing across async tasks
/// via Arc<dyn MetadataStore>.
///
/// ## Error Handling
///
/// All methods return `Result<T>` which is `Result<T, MetadataError>`. Common errors:
/// - `TopicNotFound`: Requested topic doesn't exist
/// - `TopicAlreadyExists`: Duplicate topic creation
/// - `PartitionNotFound`: Invalid partition ID
/// - `DatabaseError`: Underlying database failure
///
/// ## Examples
///
/// ```ignore
/// use streamhouse_metadata::{MetadataStore, SqliteMetadataStore, TopicConfig};
/// use std::sync::Arc;
///
/// // Create store
/// let store: Arc<dyn MetadataStore> = Arc::new(
///     SqliteMetadataStore::new("metadata.db").await?
/// );
///
/// // Use trait methods
/// store.create_topic(TopicConfig {
///     name: "orders".to_string(),
///     partition_count: 3,
///     retention_ms: Some(86400000),
///     config: HashMap::new(),
/// }).await?;
/// ```
#[async_trait]
pub trait MetadataStore: Send + Sync {
    // ============================================================
    // TOPIC OPERATIONS
    // ============================================================

    /// Create a new topic with the specified configuration.
    ///
    /// This operation is atomic: either the topic and all its partitions are created,
    /// or nothing is created (transaction semantics).
    ///
    /// # Arguments
    ///
    /// * `config` - Topic configuration including name, partition count, and retention
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - `TopicAlreadyExists`: Topic with this name already exists
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// store.create_topic(TopicConfig {
    ///     name: "orders".to_string(),
    ///     partition_count: 3,
    ///     retention_ms: Some(7 * 24 * 60 * 60 * 1000), // 7 days
    ///     config: HashMap::new(),
    /// }).await?;
    /// ```
    async fn create_topic(&self, config: TopicConfig) -> Result<()>;

    /// Delete a topic and all its associated data.
    ///
    /// This is a destructive operation that removes:
    /// - The topic metadata
    /// - All partitions (via cascade delete)
    /// - All segments (via cascade delete)
    /// - Consumer group offsets for this topic (via cascade delete)
    ///
    /// Note: This does NOT delete segment files from S3. Use a separate cleanup process.
    ///
    /// # Arguments
    ///
    /// * `name` - Topic name to delete
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - `TopicNotFound`: Topic doesn't exist
    /// - `DatabaseError`: Database operation failed
    async fn delete_topic(&self, name: &str) -> Result<()>;

    /// Get metadata for a specific topic.
    ///
    /// # Arguments
    ///
    /// * `name` - Topic name
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Topic))` if topic exists
    /// - `Ok(None)` if topic not found
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Very fast (< 100µs) as this is a primary key lookup.
    async fn get_topic(&self, name: &str) -> Result<Option<Topic>>;

    /// List all topics in the system.
    ///
    /// # Returns
    ///
    /// Vector of all topics, sorted by name.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Fast for small systems (< 10ms for 1000 topics).
    /// For large systems, consider pagination (future enhancement).
    async fn list_topics(&self) -> Result<Vec<Topic>>;

    // ============================================================
    // PARTITION OPERATIONS
    // ============================================================

    /// Get metadata for a specific partition.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID (0-indexed, must be < partition_count)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Partition))` if partition exists
    /// - `Ok(None)` if partition not found
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Very fast (< 100µs) as this uses composite index on (topic, partition_id).
    async fn get_partition(&self, topic: &str, partition_id: u32) -> Result<Option<Partition>>;

    /// Update the high watermark (latest offset) for a partition.
    ///
    /// The high watermark indicates the next offset that will be assigned to a new record.
    /// This is updated after each successful write to the partition.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `offset` - New high watermark value
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - `PartitionNotFound`: Partition doesn't exist
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Note
    ///
    /// This operation is not currently atomic with segment writes in Phase 5.1.
    /// In Phase 5.2+ with agent coordination, this will be part of the write transaction.
    async fn update_high_watermark(
        &self,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<()>;

    /// List all partitions for a topic.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    ///
    /// # Returns
    ///
    /// Vector of all partitions for the topic, sorted by partition_id.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Fast (< 1ms for topics with 1000 partitions).
    async fn list_partitions(&self, topic: &str) -> Result<Vec<Partition>>;

    // ============================================================
    // SEGMENT OPERATIONS
    // ============================================================

    /// Register a new segment after writing it to S3.
    ///
    /// Called by writers after successfully uploading a segment file to S3.
    /// This metadata enables consumers to find which S3 file contains a given offset.
    ///
    /// # Arguments
    ///
    /// * `segment` - Segment metadata including S3 location and offset range
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed (e.g., duplicate segment ID)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// store.add_segment(SegmentInfo {
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
    /// }).await?;
    /// ```
    async fn add_segment(&self, segment: SegmentInfo) -> Result<()>;

    /// Get all segments for a partition, ordered by base_offset.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    ///
    /// # Returns
    ///
    /// Vector of all segments for the partition, sorted by base_offset (oldest first).
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Fast (< 10ms for partitions with 1000 segments) due to index on (topic, partition_id).
    async fn get_segments(&self, topic: &str, partition_id: u32) -> Result<Vec<SegmentInfo>>;

    /// Find which segment contains a specific offset.
    ///
    /// This is the core query for consumers: "I want to read offset 50,000, which S3 file has it?"
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `offset` - Offset to search for
    ///
    /// # Returns
    ///
    /// - `Ok(Some(SegmentInfo))` if a segment contains this offset
    /// - `Ok(None)` if offset hasn't been written yet or offset is before earliest segment
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Very fast (< 1ms) due to index on (topic, partition_id, base_offset, end_offset).
    /// Uses optimized SQL query with range filter: `base_offset <= offset AND end_offset >= offset`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Consumer wants to read offset 50,000
    /// if let Some(segment) = store.find_segment_for_offset("orders", 0, 50_000).await? {
    ///     // Download from S3: s3://{segment.s3_bucket}/{segment.s3_key}
    ///     println!("Download: s3://{}/{}", segment.s3_bucket, segment.s3_key);
    /// }
    /// ```
    async fn find_segment_for_offset(
        &self,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<Option<SegmentInfo>>;

    /// Delete segments with end_offset before the specified offset.
    ///
    /// Used for implementing retention policies (delete old data).
    /// Note: This only deletes metadata; S3 cleanup must be done separately.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `before_offset` - Delete segments where end_offset < before_offset
    ///
    /// # Returns
    ///
    /// Number of segments deleted.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Delete segments older than offset 100,000
    /// let deleted = store.delete_segments_before("orders", 0, 100_000).await?;
    /// println!("Deleted {} segment metadata entries", deleted);
    /// // Now delete corresponding S3 files...
    /// ```
    async fn delete_segments_before(
        &self,
        topic: &str,
        partition_id: u32,
        before_offset: u64,
    ) -> Result<u64>;

    // ============================================================
    // CONSUMER GROUP OPERATIONS
    // ============================================================

    /// Ensure a consumer group exists, creating it if necessary.
    ///
    /// This is an idempotent operation: safe to call multiple times.
    /// Typically called before first offset commit for a new consumer group.
    ///
    /// # Arguments
    ///
    /// * `group_id` - Consumer group identifier (e.g., "analytics-team")
    ///
    /// # Returns
    ///
    /// `Ok(())` whether group was created or already existed.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    async fn ensure_consumer_group(&self, group_id: &str) -> Result<()>;

    /// Commit (save) the last processed offset for a consumer group.
    ///
    /// This allows consumers to resume from where they left off after a restart.
    /// The operation is idempotent: calling multiple times with the same offset
    /// just updates the timestamp.
    ///
    /// # Arguments
    ///
    /// * `group_id` - Consumer group identifier
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `offset` - Last successfully processed offset + 1 (next offset to consume)
    /// * `metadata` - Optional application-specific metadata (e.g., checkpoint info)
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Semantics
    ///
    /// The committed offset represents "next offset to consume", not "last consumed offset".
    /// If you've processed offsets 0-99, commit offset 100.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Consumer processed records 0-99, commit offset 100 (next to read)
    /// store.commit_offset("analytics", "orders", 0, 100, None).await?;
    ///
    /// // Later: resume from offset 100
    /// let offset = store.get_committed_offset("analytics", "orders", 0).await?;
    /// // offset == Some(100)
    /// ```
    async fn commit_offset(
        &self,
        group_id: &str,
        topic: &str,
        partition_id: u32,
        offset: u64,
        metadata: Option<String>,
    ) -> Result<()>;

    /// Get the committed offset for a consumer group's partition.
    ///
    /// # Arguments
    ///
    /// * `group_id` - Consumer group identifier
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    ///
    /// # Returns
    ///
    /// - `Ok(Some(offset))` if this group has committed an offset for this partition
    /// - `Ok(None)` if no offset has been committed (new consumer or partition)
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Very fast (< 100µs) as this uses composite primary key (group_id, topic, partition_id).
    async fn get_committed_offset(
        &self,
        group_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<u64>>;

    /// Get all committed offsets for a consumer group across all topics and partitions.
    ///
    /// Useful for monitoring consumer lag and progress.
    ///
    /// # Arguments
    ///
    /// * `group_id` - Consumer group identifier
    ///
    /// # Returns
    ///
    /// Vector of all offsets committed by this group, sorted by (topic, partition_id).
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Fast (< 10ms for groups consuming 100 partitions).
    async fn get_consumer_offsets(&self, group_id: &str) -> Result<Vec<ConsumerOffset>>;

    /// List all consumer groups that have committed offsets.
    ///
    /// Returns a list of unique consumer group IDs that have at least one committed offset.
    /// This is used by the REST API to enumerate all active consumer groups.
    ///
    /// # Returns
    ///
    /// A vector of consumer group IDs (strings), sorted alphabetically.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Performance
    ///
    /// Fast (< 10ms for thousands of groups) as this uses a DISTINCT query.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let groups = store.list_consumer_groups().await?;
    /// for group_id in groups {
    ///     println!("Consumer group: {}", group_id);
    /// }
    /// ```
    async fn list_consumer_groups(&self) -> Result<Vec<String>>;

    /// Delete a consumer group and all its committed offsets.
    ///
    /// This is useful for cleaning up old consumer groups that are no longer active.
    ///
    /// # Arguments
    ///
    /// * `group_id` - Consumer group identifier
    ///
    /// # Returns
    ///
    /// `Ok(())` even if the group doesn't exist (idempotent).
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Note
    ///
    /// This cascades to delete all offset entries for the group.
    async fn delete_consumer_group(&self, group_id: &str) -> Result<()>;

    // ============================================================
    // AGENT OPERATIONS (Phase 4: Multi-Agent Architecture)
    // ============================================================
    //
    // These methods enable stateless agents to coordinate via the metadata store.
    // Agents use heartbeats to maintain liveness and leases to coordinate partition ownership.

    /// Register a new agent or update an existing agent's heartbeat.
    ///
    /// This is called when an agent starts up and periodically (every 30 seconds) to
    /// maintain liveness. If the agent already exists, only the last_heartbeat timestamp
    /// is updated.
    ///
    /// # Arguments
    ///
    /// * `agent` - Agent information including ID, address, zone, and heartbeat timestamp
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Agent startup
    /// store.register_agent(AgentInfo {
    ///     agent_id: "agent-us-east-1a-001".to_string(),
    ///     address: "10.0.1.5:9090".to_string(),
    ///     availability_zone: "us-east-1a".to_string(),
    ///     agent_group: "prod".to_string(),
    ///     last_heartbeat: now_ms(),
    ///     started_at: now_ms(),
    ///     metadata: HashMap::new(),
    /// }).await?;
    ///
    /// // Periodic heartbeat (every 30s)
    /// store.register_agent(agent_info).await?;
    /// ```
    async fn register_agent(&self, agent: AgentInfo) -> Result<()>;

    /// Get information about a specific agent.
    ///
    /// # Arguments
    ///
    /// * `agent_id` - Agent identifier
    ///
    /// # Returns
    ///
    /// - `Ok(Some(AgentInfo))` if agent exists (regardless of heartbeat freshness)
    /// - `Ok(None)` if agent not found
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Note
    ///
    /// This returns the agent even if its heartbeat is stale. Use `list_agents()` to
    /// filter for only healthy agents.
    async fn get_agent(&self, agent_id: &str) -> Result<Option<AgentInfo>>;

    /// List all healthy agents, optionally filtered by agent group or availability zone.
    ///
    /// Only returns agents with a heartbeat within the last 60 seconds (considered alive).
    /// Agents without recent heartbeats are excluded as they may be dead or partitioned.
    ///
    /// # Arguments
    ///
    /// * `agent_group` - Filter by agent group (e.g., "prod", "staging"), or None for all
    /// * `availability_zone` - Filter by AZ (e.g., "us-east-1a"), or None for all
    ///
    /// # Returns
    ///
    /// Vector of healthy agents matching the filters, sorted by agent_id.
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Get all healthy agents in the "prod" group
    /// let agents = store.list_agents(Some("prod"), None).await?;
    ///
    /// // Get all healthy agents in us-east-1a
    /// let agents = store.list_agents(None, Some("us-east-1a")).await?;
    ///
    /// // Get all healthy agents
    /// let agents = store.list_agents(None, None).await?;
    /// ```
    async fn list_agents(
        &self,
        agent_group: Option<&str>,
        availability_zone: Option<&str>,
    ) -> Result<Vec<AgentInfo>>;

    /// Deregister an agent (called during graceful shutdown).
    ///
    /// Removes the agent from the metadata store. This is optional; agents will
    /// automatically be considered dead after 60 seconds without heartbeat.
    ///
    /// # Arguments
    ///
    /// * `agent_id` - Agent identifier
    ///
    /// # Returns
    ///
    /// `Ok(())` even if agent doesn't exist (idempotent).
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    async fn deregister_agent(&self, agent_id: &str) -> Result<()>;

    /// Acquire a leadership lease for a partition.
    ///
    /// This implements distributed leadership election using lease-based coordination.
    /// The operation uses compare-and-swap semantics to ensure only one agent can hold
    /// the lease at a time.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `agent_id` - Agent requesting the lease
    /// * `lease_duration_ms` - How long the lease should last (typically 30-60 seconds)
    ///
    /// # Returns
    ///
    /// The granted lease on success, including the epoch (incremented on each leadership change).
    ///
    /// # Errors
    ///
    /// - `ConflictError`: Another agent currently holds the lease
    /// - `NotFoundError`: Lease acquisition failed unexpectedly
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Semantics
    ///
    /// The lease is granted if:
    /// 1. No lease exists for this partition, OR
    /// 2. The existing lease has expired, OR
    /// 3. The same agent holds the lease (renewal)
    ///
    /// Otherwise, returns `ConflictError`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Agent tries to become leader for partition orders/0
    /// match store.acquire_partition_lease("orders", 0, "agent-001", 30_000).await {
    ///     Ok(lease) => {
    ///         println!("Became leader with epoch {}", lease.epoch);
    ///         // Now safe to write to this partition
    ///     }
    ///     Err(MetadataError::ConflictError(_)) => {
    ///         println!("Another agent holds the lease, backing off...");
    ///     }
    ///     Err(e) => return Err(e),
    /// }
    /// ```
    async fn acquire_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
        agent_id: &str,
        lease_duration_ms: i64,
    ) -> Result<PartitionLease>;

    /// Get the current lease for a partition, if any.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    ///
    /// # Returns
    ///
    /// - `Ok(Some(PartitionLease))` if an active lease exists (not expired)
    /// - `Ok(None)` if no lease exists or the lease has expired
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Note
    ///
    /// This automatically filters out expired leases by checking lease_expires_at > now.
    async fn get_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<PartitionLease>>;

    /// Release a partition leadership lease.
    ///
    /// Called during graceful shutdown to allow another agent to immediately acquire
    /// the lease instead of waiting for expiration.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `agent_id` - Agent releasing the lease (must be current leader)
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - `NotFoundError`: No lease exists or lease is held by a different agent
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Note
    ///
    /// Only succeeds if the lease is currently held by the specified agent.
    /// If another agent holds the lease, returns `NotFoundError`.
    async fn release_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
        agent_id: &str,
    ) -> Result<()>;

    /// List all active partition leases, optionally filtered by topic or agent.
    ///
    /// Useful for monitoring leadership distribution and debugging coordination issues.
    ///
    /// # Arguments
    ///
    /// * `topic` - Filter by topic name, or None for all topics
    /// * `agent_id` - Filter by agent ID, or None for all agents
    ///
    /// # Returns
    ///
    /// Vector of active leases (not expired) matching the filters, sorted by (topic, partition_id).
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Get all leases for the "orders" topic
    /// let leases = store.list_partition_leases(Some("orders"), None).await?;
    ///
    /// // Get all leases held by agent-001
    /// let leases = store.list_partition_leases(None, Some("agent-001")).await?;
    ///
    /// // Get all active leases in the system
    /// let leases = store.list_partition_leases(None, None).await?;
    /// ```
    async fn list_partition_leases(
        &self,
        topic: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<PartitionLease>>;
}
