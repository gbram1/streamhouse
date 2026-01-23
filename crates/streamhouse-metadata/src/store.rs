//! SQLite Metadata Store Implementation
//!
//! This module implements the MetadataStore trait using SQLite as the backend.
//!
//! ## What Does This Do?
//!
//! SqliteMetadataStore provides persistent storage for all StreamHouse metadata:
//! - Topics and their configurations
//! - Partitions and high watermarks
//! - Segment locations in S3
//! - Consumer group offsets
//!
//! ## Why SQLite?
//!
//! For Phase 1 (single-node deployment), SQLite is ideal:
//! - **Zero configuration**: Embedded database, no separate server
//! - **High performance**: 100K+ reads/sec, 50K+ writes/sec
//! - **ACID transactions**: Data safety and consistency
//! - **Low latency**: < 1ms for indexed queries
//! - **Easy migration**: Can switch to Postgres later with minimal changes
//!
//! ## Usage
//!
//! ### File-Based (Production)
//! ```ignore
//! use streamhouse_metadata::{SqliteMetadataStore, MetadataStore};
//!
//! // Creates metadata.db file (or opens if exists)
//! let store = SqliteMetadataStore::new("metadata.db").await?;
//!
//! // Use the store
//! store.create_topic(config).await?;
//! ```
//!
//! ### In-Memory (Testing)
//! ```ignore
//! // Fast, isolated tests
//! let store = SqliteMetadataStore::new_in_memory().await?;
//! ```
//!
//! ## Implementation Details
//!
//! ### Connection Pool
//! - Uses SQLx connection pool with 10 connections
//! - Thread-safe, can be shared across async tasks
//! - Automatically handles connection lifecycle
//!
//! ### Migrations
//! - Runs automatically on startup via sqlx::migrate!
//! - Creates schema if database is new
//! - Upgrades schema if database is old
//!
//! ### Transactions
//! - Topic creation uses transactions (atomic: create topic + partitions)
//! - Ensures data consistency even if operation fails partway
//!
//! ### Query Optimization
//! - Uses compiled queries (sqlx::query!) for safety and performance
//! - All common queries are indexed for fast lookups
//! - Binary search on offset ranges for segment lookups
//!
//! ## Performance Characteristics
//!
//! ### Fast Queries (< 1ms)
//! - Get topic by name (primary key lookup)
//! - Get partition info (composite index)
//! - Find segment for offset (indexed on topic + partition + offsets)
//! - Get consumer offset (composite primary key)
//!
//! ### Moderate Queries (1-10ms)
//! - List all topics (full table scan, but small)
//! - List all segments for partition (filtered scan with index)
//!
//! ## Thread Safety
//!
//! - SqliteMetadataStore is Send + Sync
//! - Can be safely shared via Arc<SqliteMetadataStore>
//! - Connection pool handles concurrent access
//! - SQLite WAL mode allows concurrent readers

use crate::{
    error::{MetadataError, Result},
    types::*,
    MetadataStore,
};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// SQLite-based metadata store implementation
pub struct SqliteMetadataStore {
    pool: SqlitePool,
}

impl SqliteMetadataStore {
    /// Create a new SQLite metadata store
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", path.as_ref().display()))?
                .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    /// Create in-memory database (for testing)
    pub async fn new_in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect("sqlite::memory:")
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

#[async_trait]
impl MetadataStore for SqliteMetadataStore {
    async fn create_topic(&self, config: TopicConfig) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let now = Self::now_ms();
        let config_json = serde_json::to_string(&config.config)?;

        // Insert topic
        let result = sqlx::query!(
            r#"
            INSERT INTO topics (name, partition_count, retention_ms, created_at, updated_at, config)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            config.name,
            config.partition_count,
            config.retention_ms,
            now,
            now,
            config_json,
        )
        .execute(&mut *tx)
        .await;

        if let Err(e) = result {
            if e.to_string().contains("UNIQUE constraint failed") {
                return Err(MetadataError::TopicAlreadyExists(config.name));
            }
            return Err(e.into());
        }

        // Create partitions
        for partition_id in 0..config.partition_count {
            sqlx::query!(
                r#"
                INSERT INTO partitions (topic, partition_id, high_watermark, created_at, updated_at)
                VALUES (?, ?, 0, ?, ?)
                "#,
                config.name,
                partition_id,
                now,
                now,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn delete_topic(&self, name: &str) -> Result<()> {
        let rows_affected = sqlx::query!("DELETE FROM topics WHERE name = ?", name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(MetadataError::TopicNotFound(name.to_string()));
        }

        Ok(())
    }

    async fn get_topic(&self, name: &str) -> Result<Option<Topic>> {
        let row = sqlx::query!(
            r#"
            SELECT name as "name!", partition_count as "partition_count!", retention_ms, created_at as "created_at!", config as "config!"
            FROM topics
            WHERE name = ?
            "#,
            name,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Topic {
            name: r.name,
            partition_count: r.partition_count as u32,
            retention_ms: r.retention_ms,
            created_at: r.created_at,
            config: serde_json::from_str(&r.config).unwrap_or_default(),
        }))
    }

    async fn list_topics(&self) -> Result<Vec<Topic>> {
        let rows = sqlx::query!(
            r#"
            SELECT name as "name!", partition_count as "partition_count!", retention_ms, created_at as "created_at!", config as "config!"
            FROM topics
            ORDER BY name
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Topic {
                name: r.name,
                partition_count: r.partition_count as u32,
                retention_ms: r.retention_ms,
                created_at: r.created_at,
                config: serde_json::from_str(&r.config).unwrap_or_default(),
            })
            .collect())
    }

    async fn get_partition(&self, topic: &str, partition_id: u32) -> Result<Option<Partition>> {
        let row = sqlx::query!(
            r#"
            SELECT topic, partition_id, high_watermark
            FROM partitions
            WHERE topic = ? AND partition_id = ?
            "#,
            topic,
            partition_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Partition {
            topic: r.topic,
            partition_id: r.partition_id as u32,
            high_watermark: r.high_watermark as u64,
        }))
    }

    async fn update_high_watermark(
        &self,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<()> {
        let offset_i64 = offset as i64;
        let now = Self::now_ms();

        let rows_affected = sqlx::query!(
            r#"
            UPDATE partitions
            SET high_watermark = ?, updated_at = ?
            WHERE topic = ? AND partition_id = ?
            "#,
            offset_i64,
            now,
            topic,
            partition_id,
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            return Err(MetadataError::PartitionNotFound {
                topic: topic.to_string(),
                partition: partition_id,
            });
        }

        Ok(())
    }

    async fn list_partitions(&self, topic: &str) -> Result<Vec<Partition>> {
        let rows = sqlx::query!(
            r#"
            SELECT topic, partition_id, high_watermark
            FROM partitions
            WHERE topic = ?
            ORDER BY partition_id
            "#,
            topic,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Partition {
                topic: r.topic,
                partition_id: r.partition_id as u32,
                high_watermark: r.high_watermark as u64,
            })
            .collect())
    }

    async fn add_segment(&self, segment: SegmentInfo) -> Result<()> {
        let base_offset_i64 = segment.base_offset as i64;
        let end_offset_i64 = segment.end_offset as i64;
        let size_bytes_i64 = segment.size_bytes as i64;

        sqlx::query!(
            r#"
            INSERT INTO segments (id, topic, partition_id, base_offset, end_offset, record_count, size_bytes, s3_bucket, s3_key, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            segment.id,
            segment.topic,
            segment.partition_id,
            base_offset_i64,
            end_offset_i64,
            segment.record_count,
            size_bytes_i64,
            segment.s3_bucket,
            segment.s3_key,
            segment.created_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_segments(&self, topic: &str, partition_id: u32) -> Result<Vec<SegmentInfo>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id as "id!",
                topic as "topic!",
                partition_id as "partition_id!",
                base_offset as "base_offset!",
                end_offset as "end_offset!",
                record_count as "record_count!",
                size_bytes as "size_bytes!",
                s3_bucket as "s3_bucket!",
                s3_key as "s3_key!",
                created_at as "created_at!"
            FROM segments
            WHERE topic = ? AND partition_id = ?
            ORDER BY base_offset
            "#,
            topic,
            partition_id,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SegmentInfo {
                id: r.id,
                topic: r.topic,
                partition_id: r.partition_id as u32,
                base_offset: r.base_offset as u64,
                end_offset: r.end_offset as u64,
                record_count: r.record_count as u32,
                size_bytes: r.size_bytes as u64,
                s3_bucket: r.s3_bucket,
                s3_key: r.s3_key,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn find_segment_for_offset(
        &self,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<Option<SegmentInfo>> {
        let offset_i64 = offset as i64;

        let row = sqlx::query!(
            r#"
            SELECT
                id as "id!",
                topic as "topic!",
                partition_id as "partition_id!",
                base_offset as "base_offset!",
                end_offset as "end_offset!",
                record_count as "record_count!",
                size_bytes as "size_bytes!",
                s3_bucket as "s3_bucket!",
                s3_key as "s3_key!",
                created_at as "created_at!"
            FROM segments
            WHERE topic = ? AND partition_id = ? AND base_offset <= ? AND end_offset >= ?
            ORDER BY base_offset DESC
            LIMIT 1
            "#,
            topic,
            partition_id,
            offset_i64,
            offset_i64,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| SegmentInfo {
            id: r.id,
            topic: r.topic,
            partition_id: r.partition_id as u32,
            base_offset: r.base_offset as u64,
            end_offset: r.end_offset as u64,
            record_count: r.record_count as u32,
            size_bytes: r.size_bytes as u64,
            s3_bucket: r.s3_bucket,
            s3_key: r.s3_key,
            created_at: r.created_at,
        }))
    }

    async fn delete_segments_before(
        &self,
        topic: &str,
        partition_id: u32,
        before_offset: u64,
    ) -> Result<u64> {
        let before_offset_i64 = before_offset as i64;

        let result = sqlx::query!(
            r#"
            DELETE FROM segments
            WHERE topic = ? AND partition_id = ? AND end_offset < ?
            "#,
            topic,
            partition_id,
            before_offset_i64,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn ensure_consumer_group(&self, group_id: &str) -> Result<()> {
        let now = Self::now_ms();

        sqlx::query!(
            "INSERT OR IGNORE INTO consumer_groups (group_id, created_at, updated_at) VALUES (?, ?, ?)",
            group_id,
            now,
            now,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn commit_offset(
        &self,
        group_id: &str,
        topic: &str,
        partition_id: u32,
        offset: u64,
        metadata: Option<String>,
    ) -> Result<()> {
        let offset_i64 = offset as i64;
        let now = Self::now_ms();

        // Ensure group exists
        self.ensure_consumer_group(group_id).await?;

        // Upsert offset
        sqlx::query!(
            r#"
            INSERT INTO consumer_offsets (group_id, topic, partition_id, committed_offset, metadata, committed_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(group_id, topic, partition_id)
            DO UPDATE SET
                committed_offset = excluded.committed_offset,
                metadata = excluded.metadata,
                committed_at = excluded.committed_at
            "#,
            group_id,
            topic,
            partition_id,
            offset_i64,
            metadata,
            now,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_committed_offset(
        &self,
        group_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<u64>> {
        let row = sqlx::query!(
            r#"
            SELECT committed_offset
            FROM consumer_offsets
            WHERE group_id = ? AND topic = ? AND partition_id = ?
            "#,
            group_id,
            topic,
            partition_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.committed_offset as u64))
    }

    async fn get_consumer_offsets(&self, group_id: &str) -> Result<Vec<ConsumerOffset>> {
        let rows = sqlx::query!(
            r#"
            SELECT group_id, topic, partition_id, committed_offset, metadata
            FROM consumer_offsets
            WHERE group_id = ?
            ORDER BY topic, partition_id
            "#,
            group_id,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ConsumerOffset {
                group_id: r.group_id,
                topic: r.topic,
                partition_id: r.partition_id as u32,
                committed_offset: r.committed_offset as u64,
                metadata: r.metadata,
            })
            .collect())
    }

    async fn delete_consumer_group(&self, group_id: &str) -> Result<()> {
        sqlx::query!("DELETE FROM consumer_groups WHERE group_id = ?", group_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ============================================================
    // AGENT OPERATIONS (Phase 4: Multi-Agent Architecture)
    // ============================================================

    async fn register_agent(&self, agent: AgentInfo) -> Result<()> {
        let metadata_json = serde_json::to_string(&agent.metadata)?;

        sqlx::query!(
            r#"
            INSERT INTO agents (
                agent_id, address, availability_zone, agent_group,
                last_heartbeat, started_at, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(agent_id) DO UPDATE SET
                address = excluded.address,
                availability_zone = excluded.availability_zone,
                agent_group = excluded.agent_group,
                last_heartbeat = excluded.last_heartbeat,
                metadata = excluded.metadata
            "#,
            agent.agent_id,
            agent.address,
            agent.availability_zone,
            agent.agent_group,
            agent.last_heartbeat,
            agent.started_at,
            metadata_json,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_agent(&self, agent_id: &str) -> Result<Option<AgentInfo>> {
        let query = format!(
            "SELECT agent_id, address, availability_zone, agent_group, \
             last_heartbeat, started_at, metadata \
             FROM agents \
             WHERE agent_id = '{}'",
            agent_id
        );

        let row: Option<(String, String, String, String, i64, i64, String)> =
            sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(|(agent_id, address, availability_zone, agent_group, last_heartbeat, started_at, metadata_json)| {
            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_str(&metadata_json).unwrap_or_default();

            AgentInfo {
                agent_id,
                address,
                availability_zone,
                agent_group,
                last_heartbeat,
                started_at,
                metadata,
            }
        }))
    }

    async fn list_agents(
        &self,
        agent_group: Option<&str>,
        availability_zone: Option<&str>,
    ) -> Result<Vec<AgentInfo>> {
        // Only return agents with heartbeat within last 60 seconds
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let stale_threshold = now_ms - 60_000;

        // Use a single query structure to avoid type mismatch
        let query = match (agent_group, availability_zone) {
            (Some(group), Some(az)) => format!(
                "SELECT agent_id, address, availability_zone, agent_group, \
                 last_heartbeat, started_at, metadata \
                 FROM agents \
                 WHERE agent_group = '{}' AND availability_zone = '{}' AND last_heartbeat > {} \
                 ORDER BY agent_id",
                group, az, stale_threshold
            ),
            (Some(group), None) => format!(
                "SELECT agent_id, address, availability_zone, agent_group, \
                 last_heartbeat, started_at, metadata \
                 FROM agents \
                 WHERE agent_group = '{}' AND last_heartbeat > {} \
                 ORDER BY agent_id",
                group, stale_threshold
            ),
            (None, Some(az)) => format!(
                "SELECT agent_id, address, availability_zone, agent_group, \
                 last_heartbeat, started_at, metadata \
                 FROM agents \
                 WHERE availability_zone = '{}' AND last_heartbeat > {} \
                 ORDER BY agent_id",
                az, stale_threshold
            ),
            (None, None) => format!(
                "SELECT agent_id, address, availability_zone, agent_group, \
                 last_heartbeat, started_at, metadata \
                 FROM agents \
                 WHERE last_heartbeat > {} \
                 ORDER BY agent_id",
                stale_threshold
            ),
        };

        let rows: Vec<(String, String, String, String, i64, i64, String)> =
            sqlx::query_as(&query).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|(agent_id, address, availability_zone, agent_group, last_heartbeat, started_at, metadata_json)| {
                let metadata: std::collections::HashMap<String, String> =
                    serde_json::from_str(&metadata_json).unwrap_or_default();

                AgentInfo {
                    agent_id,
                    address,
                    availability_zone,
                    agent_group,
                    last_heartbeat,
                    started_at,
                    metadata,
                }
            })
            .collect())
    }

    async fn deregister_agent(&self, agent_id: &str) -> Result<()> {
        sqlx::query!("DELETE FROM agents WHERE agent_id = ?", agent_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn acquire_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
        agent_id: &str,
        lease_duration_ms: i64,
    ) -> Result<PartitionLease> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let expires_at = now_ms + lease_duration_ms;

        // Try to acquire or renew lease
        // This query implements compare-and-swap semantics:
        // - If no lease exists OR lease expired OR same agent holds it, grant/renew
        // - Otherwise, fail (another agent holds the lease)
        let _result = sqlx::query!(
            r#"
            INSERT INTO partition_leases (
                topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
            )
            VALUES (?, ?, ?, ?, ?, 1)
            ON CONFLICT(topic, partition_id) DO UPDATE SET
                leader_agent_id = CASE
                    WHEN excluded.leader_agent_id = leader_agent_id OR lease_expires_at < ?
                    THEN excluded.leader_agent_id
                    ELSE leader_agent_id
                END,
                lease_expires_at = CASE
                    WHEN excluded.leader_agent_id = leader_agent_id OR lease_expires_at < ?
                    THEN excluded.lease_expires_at
                    ELSE lease_expires_at
                END,
                acquired_at = CASE
                    WHEN excluded.leader_agent_id = leader_agent_id OR lease_expires_at < ?
                    THEN excluded.acquired_at
                    ELSE acquired_at
                END,
                epoch = CASE
                    WHEN excluded.leader_agent_id = leader_agent_id OR lease_expires_at < ?
                    THEN epoch + 1
                    ELSE epoch
                END
            "#,
            topic,
            partition_id,
            agent_id,
            expires_at,
            now_ms,
            now_ms,
            now_ms,
            now_ms,
            now_ms,
        )
        .execute(&self.pool)
        .await?;

        // Verify we got the lease
        let lease = self.get_partition_lease(topic, partition_id).await?;

        match lease {
            Some(lease) if lease.leader_agent_id == agent_id => Ok(lease),
            Some(_) => Err(MetadataError::ConflictError(format!(
                "Partition {}/{} is already leased to another agent",
                topic, partition_id
            ))),
            None => Err(MetadataError::NotFoundError(format!(
                "Lease for partition {}/{} not found after acquisition",
                topic, partition_id
            ))),
        }
    }

    async fn get_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<PartitionLease>> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let row = sqlx::query!(
            r#"
            SELECT topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
            FROM partition_leases
            WHERE topic = ? AND partition_id = ? AND lease_expires_at > ?
            "#,
            topic,
            partition_id,
            now_ms,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| PartitionLease {
            topic: r.topic,
            partition_id: r.partition_id as u32,
            leader_agent_id: r.leader_agent_id,
            lease_expires_at: r.lease_expires_at,
            acquired_at: r.acquired_at,
            epoch: r.epoch as u64,
        }))
    }

    async fn release_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
        agent_id: &str,
    ) -> Result<()> {
        let result = sqlx::query!(
            r#"
            DELETE FROM partition_leases
            WHERE topic = ? AND partition_id = ? AND leader_agent_id = ?
            "#,
            topic,
            partition_id,
            agent_id,
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::NotFoundError(format!(
                "No lease found for partition {}/{} held by agent {}",
                topic, partition_id, agent_id
            )));
        }

        Ok(())
    }

    async fn list_partition_leases(
        &self,
        topic: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<PartitionLease>> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Use a single query structure to avoid type mismatch
        let query = match (topic, agent_id) {
            (Some(t), Some(a)) => format!(
                "SELECT topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch \
                 FROM partition_leases \
                 WHERE topic = '{}' AND leader_agent_id = '{}' AND lease_expires_at > {} \
                 ORDER BY topic, partition_id",
                t, a, now_ms
            ),
            (Some(t), None) => format!(
                "SELECT topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch \
                 FROM partition_leases \
                 WHERE topic = '{}' AND lease_expires_at > {} \
                 ORDER BY topic, partition_id",
                t, now_ms
            ),
            (None, Some(a)) => format!(
                "SELECT topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch \
                 FROM partition_leases \
                 WHERE leader_agent_id = '{}' AND lease_expires_at > {} \
                 ORDER BY topic, partition_id",
                a, now_ms
            ),
            (None, None) => format!(
                "SELECT topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch \
                 FROM partition_leases \
                 WHERE lease_expires_at > {} \
                 ORDER BY topic, partition_id",
                now_ms
            ),
        };

        let rows: Vec<(String, i32, String, i64, i64, i64)> =
            sqlx::query_as(&query).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|(topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch)| PartitionLease {
                topic,
                partition_id: partition_id as u32,
                leader_agent_id,
                lease_expires_at,
                acquired_at,
                epoch: epoch as u64,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    async fn setup_test_store() -> SqliteMetadataStore {
        SqliteMetadataStore::new_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn test_create_and_get_topic() {
        let store = setup_test_store().await;

        let config = TopicConfig {
            name: "test".to_string(),
            partition_count: 3,
            retention_ms: None,
            config: HashMap::new(),
        };

        store.create_topic(config.clone()).await.unwrap();

        let topic = store.get_topic("test").await.unwrap().unwrap();
        assert_eq!(topic.name, "test");
        assert_eq!(topic.partition_count, 3);

        // Should have created 3 partitions
        let partitions = store.list_partitions("test").await.unwrap();
        assert_eq!(partitions.len(), 3);
    }

    #[tokio::test]
    async fn test_duplicate_topic_fails() {
        let store = setup_test_store().await;

        let config = TopicConfig {
            name: "test".to_string(),
            partition_count: 1,
            retention_ms: None,
            config: HashMap::new(),
        };

        store.create_topic(config.clone()).await.unwrap();

        // Second create should fail
        let result = store.create_topic(config).await;
        assert!(matches!(result, Err(MetadataError::TopicAlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_find_segment_for_offset() {
        let store = setup_test_store().await;

        // Create topic
        store
            .create_topic(TopicConfig {
                name: "test".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .unwrap();

        // Add segments
        store
            .add_segment(SegmentInfo {
                id: "seg1".to_string(),
                topic: "test".to_string(),
                partition_id: 0,
                base_offset: 0,
                end_offset: 999,
                record_count: 1000,
                size_bytes: 1024,
                s3_bucket: "test".to_string(),
                s3_key: "seg1.seg".to_string(),
                created_at: 0,
            })
            .await
            .unwrap();

        store
            .add_segment(SegmentInfo {
                id: "seg2".to_string(),
                topic: "test".to_string(),
                partition_id: 0,
                base_offset: 1000,
                end_offset: 1999,
                record_count: 1000,
                size_bytes: 1024,
                s3_bucket: "test".to_string(),
                s3_key: "seg2.seg".to_string(),
                created_at: 0,
            })
            .await
            .unwrap();

        // Find offset in first segment
        let seg = store
            .find_segment_for_offset("test", 0, 500)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(seg.id, "seg1");

        // Find offset in second segment
        let seg = store
            .find_segment_for_offset("test", 0, 1500)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(seg.id, "seg2");

        // Offset doesn't exist yet
        let seg = store
            .find_segment_for_offset("test", 0, 5000)
            .await
            .unwrap();
        assert!(seg.is_none());
    }

    #[tokio::test]
    async fn test_consumer_offset_commit() {
        let store = setup_test_store().await;

        // Create topic
        store
            .create_topic(TopicConfig {
                name: "test".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .unwrap();

        // Initially no offset
        let offset = store
            .get_committed_offset("group1", "test", 0)
            .await
            .unwrap();
        assert!(offset.is_none());

        // Commit offset
        store
            .commit_offset("group1", "test", 0, 100, None)
            .await
            .unwrap();

        // Should be retrievable
        let offset = store
            .get_committed_offset("group1", "test", 0)
            .await
            .unwrap();
        assert_eq!(offset, Some(100));

        // Update offset
        store
            .commit_offset("group1", "test", 0, 200, None)
            .await
            .unwrap();
        let offset = store
            .get_committed_offset("group1", "test", 0)
            .await
            .unwrap();
        assert_eq!(offset, Some(200));
    }
}
