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

pub use cached_store::{CachedMetadataStore, CacheConfig, CacheMetrics};
pub use error::{MetadataError, Result};
pub use store::SqliteMetadataStore;
pub use types::*;

#[cfg(feature = "postgres")]
pub use postgres::PostgresMetadataStore;

use async_trait::async_trait;

/// Metadata store trait - abstracts over different storage backends
#[async_trait]
pub trait MetadataStore: Send + Sync {
    // TOPIC OPERATIONS
    async fn create_topic(&self, config: TopicConfig) -> Result<()>;
    async fn delete_topic(&self, name: &str) -> Result<()>;
    async fn get_topic(&self, name: &str) -> Result<Option<Topic>>;
    async fn list_topics(&self) -> Result<Vec<Topic>>;

    // PARTITION OPERATIONS
    async fn get_partition(&self, topic: &str, partition_id: u32) -> Result<Option<Partition>>;
    async fn update_high_watermark(
        &self,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<()>;
    async fn list_partitions(&self, topic: &str) -> Result<Vec<Partition>>;

    // SEGMENT OPERATIONS
    async fn add_segment(&self, segment: SegmentInfo) -> Result<()>;
    async fn get_segments(&self, topic: &str, partition_id: u32) -> Result<Vec<SegmentInfo>>;
    async fn find_segment_for_offset(
        &self,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<Option<SegmentInfo>>;
    async fn delete_segments_before(
        &self,
        topic: &str,
        partition_id: u32,
        before_offset: u64,
    ) -> Result<u64>;

    // CONSUMER GROUP OPERATIONS
    async fn ensure_consumer_group(&self, group_id: &str) -> Result<()>;
    async fn commit_offset(
        &self,
        group_id: &str,
        topic: &str,
        partition_id: u32,
        offset: u64,
        metadata: Option<String>,
    ) -> Result<()>;
    async fn get_committed_offset(
        &self,
        group_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<u64>>;
    async fn get_consumer_offsets(&self, group_id: &str) -> Result<Vec<ConsumerOffset>>;
    async fn delete_consumer_group(&self, group_id: &str) -> Result<()>;

    // AGENT OPERATIONS (Phase 4: Multi-Agent Architecture)
    // These methods enable stateless agents to coordinate via the metadata store

    /// Register a new agent or update existing agent's heartbeat
    ///
    /// Called when an agent starts up and periodically (every 30s) to maintain liveness.
    /// If agent already exists, updates last_heartbeat timestamp.
    async fn register_agent(&self, agent: AgentInfo) -> Result<()>;

    /// Get information about a specific agent
    async fn get_agent(&self, agent_id: &str) -> Result<Option<AgentInfo>>;

    /// List all agents, optionally filtered by agent group or availability zone
    ///
    /// Returns only agents with heartbeat within last 60 seconds (considered alive).
    async fn list_agents(
        &self,
        agent_group: Option<&str>,
        availability_zone: Option<&str>,
    ) -> Result<Vec<AgentInfo>>;

    /// Remove an agent (called on graceful shutdown)
    async fn deregister_agent(&self, agent_id: &str) -> Result<()>;

    /// Acquire leadership lease for a partition
    ///
    /// Implements compare-and-swap semantics:
    /// - If no lease exists OR existing lease expired OR existing lease held by same agent,
    ///   grant/renew the lease
    /// - Otherwise, return error (another agent holds the lease)
    ///
    /// Returns the new lease on success.
    async fn acquire_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
        agent_id: &str,
        lease_duration_ms: i64,
    ) -> Result<PartitionLease>;

    /// Get current lease for a partition (if any)
    ///
    /// Returns None if no lease exists or lease has expired.
    async fn get_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<PartitionLease>>;

    /// Release leadership lease for a partition
    ///
    /// Called during graceful shutdown. Only succeeds if the lease is held by the given agent.
    async fn release_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
        agent_id: &str,
    ) -> Result<()>;

    /// List all partition leases, optionally filtered by topic or agent
    ///
    /// Useful for monitoring and debugging leadership distribution.
    async fn list_partition_leases(
        &self,
        topic: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<PartitionLease>>;
}
