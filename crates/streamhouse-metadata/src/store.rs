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
use sqlx::Row;
use std::collections::HashMap;
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

    /// Helper to convert a SQLite row to ConnectorInfo
    fn row_to_connector_info(r: sqlx::sqlite::SqliteRow) -> Result<ConnectorInfo> {
        use sqlx::Row;

        let topics_str: String = r.get("topics");
        let config_str: String = r.get("config");

        let topics: Vec<String> = serde_json::from_str(&topics_str)?;
        let config: HashMap<String, String> = serde_json::from_str(&config_str)?;

        Ok(ConnectorInfo {
            name: r.get("name"),
            connector_type: r.get("connector_type"),
            connector_class: r.get("connector_class"),
            topics,
            config,
            state: r.get("state"),
            error_message: r.get("error_message"),
            records_processed: r.get("records_processed"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
    }

    /// Helper to convert a SQLite row to MaterializedView
    fn row_to_materialized_view(r: sqlx::sqlite::SqliteRow) -> MaterializedView {
        use sqlx::Row;

        let refresh_mode_str: String = r.get("refresh_mode");
        let interval_ms: Option<i64> = r.get("refresh_interval_ms");
        let refresh_mode = match refresh_mode_str.as_str() {
            "continuous" => MaterializedViewRefreshMode::Continuous,
            "periodic" => MaterializedViewRefreshMode::Periodic {
                interval_ms: interval_ms.unwrap_or(300000),
            },
            "manual" => MaterializedViewRefreshMode::Manual,
            _ => MaterializedViewRefreshMode::Continuous,
        };

        let status_str: String = r.get("status");
        let status = match status_str.as_str() {
            "initializing" => MaterializedViewStatus::Initializing,
            "running" => MaterializedViewStatus::Running,
            "paused" => MaterializedViewStatus::Paused,
            "error" => MaterializedViewStatus::Error,
            _ => MaterializedViewStatus::Initializing,
        };

        MaterializedView {
            id: r.get("id"),
            organization_id: r.get("organization_id"),
            name: r.get("name"),
            source_topic: r.get("source_topic"),
            query_sql: r.get("query_sql"),
            refresh_mode,
            status,
            error_message: r.get("error_message"),
            row_count: r.get::<i64, _>("row_count") as u64,
            last_refresh_at: r.get::<Option<i64>, _>("last_refresh_at"),
            created_at: r.get::<i64, _>("created_at"),
            updated_at: r.get::<i64, _>("updated_at"),
        }
    }
}

#[async_trait]
impl MetadataStore for SqliteMetadataStore {
    async fn create_topic(&self, config: TopicConfig) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let now = Self::now_ms();
        // Include cleanup_policy in the config map for storage
        let mut config_map = config.config.clone();
        config_map.insert(
            "cleanup.policy".to_string(),
            config.cleanup_policy.as_str().to_string(),
        );
        let config_json = serde_json::to_string(&config_map)?;

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

        Ok(row.map(|r| {
            let config: HashMap<String, String> =
                serde_json::from_str(&r.config).unwrap_or_default();
            let cleanup_policy = config
                .get("cleanup.policy")
                .map(|s| CleanupPolicy::from_str(s))
                .unwrap_or_default();
            Topic {
                name: r.name,
                partition_count: r.partition_count as u32,
                retention_ms: r.retention_ms,
                cleanup_policy,
                created_at: r.created_at,
                config,
            }
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
            .map(|r| {
                let config: HashMap<String, String> =
                    serde_json::from_str(&r.config).unwrap_or_default();
                let cleanup_policy = config
                    .get("cleanup.policy")
                    .map(|s| CleanupPolicy::from_str(s))
                    .unwrap_or_default();
                Topic {
                    name: r.name,
                    partition_count: r.partition_count as u32,
                    retention_ms: r.retention_ms,
                    cleanup_policy,
                    created_at: r.created_at,
                    config,
                }
            })
            .collect())
    }

    async fn list_topics_for_org(&self, org_id: &str) -> Result<Vec<Topic>> {
        let rows = sqlx::query!(
            r#"
            SELECT name as "name!", partition_count as "partition_count!", retention_ms, created_at as "created_at!", config as "config!"
            FROM topics
            WHERE organization_id = ?
            ORDER BY name
            "#,
            org_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let config: HashMap<String, String> =
                    serde_json::from_str(&r.config).unwrap_or_default();
                let cleanup_policy = config
                    .get("cleanup.policy")
                    .map(|s| CleanupPolicy::from_str(s))
                    .unwrap_or_default();
                Topic {
                    name: r.name,
                    partition_count: r.partition_count as u32,
                    retention_ms: r.retention_ms,
                    cleanup_policy,
                    created_at: r.created_at,
                    config,
                }
            })
            .collect())
    }

    async fn create_topic_for_org(&self, org_id: &str, config: TopicConfig) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = Self::now_ms();

        let mut config_map = config.config.clone();
        config_map.insert(
            "cleanup.policy".to_string(),
            config.cleanup_policy.as_str().to_string(),
        );
        let config_json = serde_json::to_string(&config_map)?;

        let result = sqlx::query!(
            r#"
            INSERT INTO topics (organization_id, name, partition_count, retention_ms, created_at, updated_at, config)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            org_id,
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

        for partition_id in 0..config.partition_count {
            sqlx::query!(
                r#"
                INSERT INTO partitions (organization_id, topic, partition_id, high_watermark, created_at, updated_at)
                VALUES (?, ?, ?, 0, ?, ?)
                "#,
                org_id,
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

    async fn delete_topic_for_org(&self, org_id: &str, name: &str) -> Result<()> {
        let rows_affected = sqlx::query!(
            "DELETE FROM topics WHERE organization_id = ? AND name = ?",
            org_id,
            name
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            return Err(MetadataError::TopicNotFound(name.to_string()));
        }
        Ok(())
    }

    async fn get_topic_for_org(&self, org_id: &str, name: &str) -> Result<Option<Topic>> {
        let row = sqlx::query!(
            r#"
            SELECT name as "name!", partition_count as "partition_count!", retention_ms, created_at as "created_at!", config as "config!"
            FROM topics
            WHERE organization_id = ? AND name = ?
            "#,
            org_id,
            name
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let config: HashMap<String, String> =
                serde_json::from_str(&r.config).unwrap_or_default();
            let cleanup_policy = config
                .get("cleanup.policy")
                .map(|s| CleanupPolicy::from_str(s))
                .unwrap_or_default();
            Topic {
                name: r.name,
                partition_count: r.partition_count as u32,
                retention_ms: r.retention_ms,
                cleanup_policy,
                created_at: r.created_at,
                config,
            }
        }))
    }

    async fn ensure_organization(&self, org_id: &str, name: &str) -> Result<()> {
        let now = Self::now_ms();
        let slug = name.to_lowercase().replace(' ', "-");
        sqlx::query!(
            r#"
            INSERT OR IGNORE INTO organizations (id, name, slug, plan, status, created_at, updated_at)
            VALUES (?, ?, ?, 'free', 'active', ?, ?)
            "#,
            org_id,
            name,
            slug,
            now,
            now,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
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

    async fn list_consumer_groups(&self) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT DISTINCT group_id
            FROM consumer_offsets
            ORDER BY group_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
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

        Ok(row.map(
            |(
                agent_id,
                address,
                availability_zone,
                agent_group,
                last_heartbeat,
                started_at,
                metadata_json,
            )| {
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
            },
        ))
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
            .map(
                |(
                    agent_id,
                    address,
                    availability_zone,
                    agent_group,
                    last_heartbeat,
                    started_at,
                    metadata_json,
                )| {
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
                },
            )
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
            .map(
                |(topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch)| {
                    PartitionLease {
                        topic,
                        partition_id: partition_id as u32,
                        leader_agent_id,
                        lease_expires_at,
                        acquired_at,
                        epoch: epoch as u64,
                    }
                },
            )
            .collect())
    }

    // ============================================================
    // ORGANIZATION OPERATIONS (Phase 21.5: Multi-Tenancy)
    // ============================================================

    async fn create_organization(&self, config: CreateOrganization) -> Result<Organization> {
        let now = Self::now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        let plan_str = config.plan.to_string();
        let status_str = OrganizationStatus::Active.to_string();
        let settings_json = serde_json::to_string(&config.settings)?;

        let query = format!(
            "INSERT INTO organizations (id, name, slug, plan, status, created_at, updated_at, settings) \
             VALUES ('{}', '{}', '{}', '{}', '{}', {}, {}, '{}')",
            id,
            config.name.replace('\'', "''"),
            config.slug.replace('\'', "''"),
            plan_str,
            status_str,
            now,
            now,
            settings_json.replace('\'', "''"),
        );
        let result = sqlx::query(&query).execute(&self.pool).await;

        if let Err(e) = result {
            if e.to_string().contains("UNIQUE constraint failed") {
                return Err(MetadataError::ConflictError(format!(
                    "Organization with slug '{}' already exists",
                    config.slug
                )));
            }
            return Err(e.into());
        }

        Ok(Organization {
            id,
            name: config.name,
            slug: config.slug,
            plan: config.plan,
            status: OrganizationStatus::Active,
            created_at: now,
            settings: config.settings,
        })
    }

    async fn get_organization(&self, id: &str) -> Result<Option<Organization>> {
        let query = format!(
            "SELECT id, name, slug, plan, status, created_at, settings \
             FROM organizations WHERE id = '{}'",
            id
        );

        let row: Option<(String, String, String, String, String, i64, String)> =
            sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(id, name, slug, plan, status, created_at, settings_json)| Organization {
                id,
                name,
                slug,
                plan: plan.parse().unwrap_or_default(),
                status: status.parse().unwrap_or_default(),
                created_at,
                settings: serde_json::from_str(&settings_json).unwrap_or_default(),
            },
        ))
    }

    async fn get_organization_by_slug(&self, slug: &str) -> Result<Option<Organization>> {
        let query = format!(
            "SELECT id, name, slug, plan, status, created_at, settings \
             FROM organizations WHERE slug = '{}'",
            slug
        );

        let row: Option<(String, String, String, String, String, i64, String)> =
            sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(id, name, slug, plan, status, created_at, settings_json)| Organization {
                id,
                name,
                slug,
                plan: plan.parse().unwrap_or_default(),
                status: status.parse().unwrap_or_default(),
                created_at,
                settings: serde_json::from_str(&settings_json).unwrap_or_default(),
            },
        ))
    }

    async fn list_organizations(&self) -> Result<Vec<Organization>> {
        let rows: Vec<(String, String, String, String, String, i64, String)> = sqlx::query_as(
            "SELECT id, name, slug, plan, status, created_at, settings \
             FROM organizations ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, name, slug, plan, status, created_at, settings_json)| Organization {
                    id,
                    name,
                    slug,
                    plan: plan.parse().unwrap_or_default(),
                    status: status.parse().unwrap_or_default(),
                    created_at,
                    settings: serde_json::from_str(&settings_json).unwrap_or_default(),
                },
            )
            .collect())
    }

    async fn update_organization_status(&self, id: &str, status: OrganizationStatus) -> Result<()> {
        let status_str = status.to_string();
        let now = Self::now_ms();

        let query = format!(
            "UPDATE organizations SET status = '{}', updated_at = {} WHERE id = '{}'",
            status_str, now, id
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::NotFoundError(format!(
                "Organization {} not found",
                id
            )));
        }

        Ok(())
    }

    async fn update_organization_plan(&self, id: &str, plan: OrganizationPlan) -> Result<()> {
        let plan_str = plan.to_string();
        let now = Self::now_ms();

        let query = format!(
            "UPDATE organizations SET plan = '{}', updated_at = {} WHERE id = '{}'",
            plan_str, now, id
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::NotFoundError(format!(
                "Organization {} not found",
                id
            )));
        }

        Ok(())
    }

    async fn delete_organization(&self, id: &str) -> Result<()> {
        self.update_organization_status(id, OrganizationStatus::Deleted)
            .await
    }

    // ============================================================
    // API KEY OPERATIONS (Phase 21.5: Multi-Tenancy)
    // ============================================================

    async fn create_api_key(
        &self,
        organization_id: &str,
        config: CreateApiKey,
        key_hash: &str,
        key_prefix: &str,
    ) -> Result<ApiKey> {
        let now = Self::now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        let expires_at = config.expires_in_ms.map(|ms| now + ms);
        let permissions_json = serde_json::to_string(&config.permissions)?;
        let scopes_json = serde_json::to_string(&config.scopes)?;

        let expires_at_str = expires_at.map_or("NULL".to_string(), |e| e.to_string());
        let query = format!(
            "INSERT INTO api_keys (id, organization_id, name, key_hash, key_prefix, permissions, scopes, expires_at, created_at) \
             VALUES ('{}', '{}', '{}', '{}', '{}', '{}', '{}', {}, {})",
            id,
            organization_id,
            config.name.replace('\'', "''"),
            key_hash,
            key_prefix,
            permissions_json.replace('\'', "''"),
            scopes_json.replace('\'', "''"),
            expires_at_str,
            now,
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(ApiKey {
            id,
            organization_id: organization_id.to_string(),
            name: config.name,
            key_prefix: key_prefix.to_string(),
            permissions: config.permissions,
            scopes: config.scopes,
            expires_at,
            last_used_at: None,
            created_at: now,
            created_by: None,
        })
    }

    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>> {
        let query = format!(
            "SELECT id, organization_id, name, key_prefix, permissions, scopes, expires_at, last_used_at, created_at, created_by \
             FROM api_keys WHERE id = '{}'",
            id
        );

        let row: Option<(
            String,
            String,
            String,
            String,
            String,
            String,
            Option<i64>,
            Option<i64>,
            i64,
            Option<String>,
        )> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(
                id,
                organization_id,
                name,
                key_prefix,
                permissions_json,
                scopes_json,
                expires_at,
                last_used_at,
                created_at,
                created_by,
            )| {
                ApiKey {
                    id,
                    organization_id,
                    name,
                    key_prefix,
                    permissions: serde_json::from_str(&permissions_json).unwrap_or_default(),
                    scopes: serde_json::from_str(&scopes_json).unwrap_or_default(),
                    expires_at,
                    last_used_at,
                    created_at,
                    created_by,
                }
            },
        ))
    }

    async fn validate_api_key(&self, key_hash: &str) -> Result<Option<ApiKey>> {
        let now = Self::now_ms();

        let query = format!(
            "SELECT id, organization_id, name, key_prefix, permissions, scopes, expires_at, last_used_at, created_at, created_by \
             FROM api_keys \
             WHERE key_hash = '{}' AND (expires_at IS NULL OR expires_at > {})",
            key_hash, now
        );

        let row: Option<(
            String,
            String,
            String,
            String,
            String,
            String,
            Option<i64>,
            Option<i64>,
            i64,
            Option<String>,
        )> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(
                id,
                organization_id,
                name,
                key_prefix,
                permissions_json,
                scopes_json,
                expires_at,
                last_used_at,
                created_at,
                created_by,
            )| {
                ApiKey {
                    id,
                    organization_id,
                    name,
                    key_prefix,
                    permissions: serde_json::from_str(&permissions_json).unwrap_or_default(),
                    scopes: serde_json::from_str(&scopes_json).unwrap_or_default(),
                    expires_at,
                    last_used_at,
                    created_at,
                    created_by,
                }
            },
        ))
    }

    async fn list_api_keys(&self, organization_id: &str) -> Result<Vec<ApiKey>> {
        let query = format!(
            "SELECT id, organization_id, name, key_prefix, permissions, scopes, expires_at, last_used_at, created_at, created_by \
             FROM api_keys WHERE organization_id = '{}' ORDER BY created_at DESC",
            organization_id
        );

        let rows: Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            Option<i64>,
            Option<i64>,
            i64,
            Option<String>,
        )> = sqlx::query_as(&query).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    organization_id,
                    name,
                    key_prefix,
                    permissions_json,
                    scopes_json,
                    expires_at,
                    last_used_at,
                    created_at,
                    created_by,
                )| {
                    ApiKey {
                        id,
                        organization_id,
                        name,
                        key_prefix,
                        permissions: serde_json::from_str(&permissions_json).unwrap_or_default(),
                        scopes: serde_json::from_str(&scopes_json).unwrap_or_default(),
                        expires_at,
                        last_used_at,
                        created_at,
                        created_by,
                    }
                },
            )
            .collect())
    }

    async fn touch_api_key(&self, id: &str) -> Result<()> {
        let now = Self::now_ms();

        let query = format!(
            "UPDATE api_keys SET last_used_at = {} WHERE id = '{}'",
            now, id
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(())
    }

    async fn revoke_api_key(&self, id: &str) -> Result<()> {
        let query = format!("DELETE FROM api_keys WHERE id = '{}'", id);
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(())
    }

    // ============================================================
    // QUOTA OPERATIONS (Phase 21.5: Multi-Tenancy)
    // ============================================================

    async fn get_organization_quota(&self, organization_id: &str) -> Result<OrganizationQuota> {
        let query = format!(
            "SELECT organization_id, max_topics, max_partitions_per_topic, max_total_partitions, \
             max_storage_bytes, max_retention_days, max_produce_bytes_per_sec, max_consume_bytes_per_sec, \
             max_requests_per_sec, max_consumer_groups, max_schemas, max_schema_versions_per_subject, max_connections \
             FROM organization_quotas WHERE organization_id = '{}'",
            organization_id
        );

        let row: Option<(
            String,
            i32,
            i32,
            i32,
            i64,
            i32,
            i64,
            i64,
            i32,
            i32,
            i32,
            i32,
            i32,
        )> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row
            .map(
                |(
                    organization_id,
                    max_topics,
                    max_partitions_per_topic,
                    max_total_partitions,
                    max_storage_bytes,
                    max_retention_days,
                    max_produce_bytes_per_sec,
                    max_consume_bytes_per_sec,
                    max_requests_per_sec,
                    max_consumer_groups,
                    max_schemas,
                    max_schema_versions_per_subject,
                    max_connections,
                )| OrganizationQuota {
                    organization_id,
                    max_topics,
                    max_partitions_per_topic,
                    max_total_partitions,
                    max_storage_bytes,
                    max_retention_days,
                    max_produce_bytes_per_sec,
                    max_consume_bytes_per_sec,
                    max_requests_per_sec,
                    max_consumer_groups,
                    max_schemas,
                    max_schema_versions_per_subject,
                    max_connections,
                },
            )
            .unwrap_or_else(|| OrganizationQuota {
                organization_id: organization_id.to_string(),
                ..Default::default()
            }))
    }

    async fn set_organization_quota(&self, quota: OrganizationQuota) -> Result<()> {
        let query = format!(
            "INSERT INTO organization_quotas (\
                organization_id, max_topics, max_partitions_per_topic, max_total_partitions, \
                max_storage_bytes, max_retention_days, max_produce_bytes_per_sec, max_consume_bytes_per_sec, \
                max_requests_per_sec, max_consumer_groups, max_schemas, max_schema_versions_per_subject, max_connections\
            ) VALUES ('{}', {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) \
            ON CONFLICT(organization_id) DO UPDATE SET \
                max_topics = excluded.max_topics, \
                max_partitions_per_topic = excluded.max_partitions_per_topic, \
                max_total_partitions = excluded.max_total_partitions, \
                max_storage_bytes = excluded.max_storage_bytes, \
                max_retention_days = excluded.max_retention_days, \
                max_produce_bytes_per_sec = excluded.max_produce_bytes_per_sec, \
                max_consume_bytes_per_sec = excluded.max_consume_bytes_per_sec, \
                max_requests_per_sec = excluded.max_requests_per_sec, \
                max_consumer_groups = excluded.max_consumer_groups, \
                max_schemas = excluded.max_schemas, \
                max_schema_versions_per_subject = excluded.max_schema_versions_per_subject, \
                max_connections = excluded.max_connections",
            quota.organization_id,
            quota.max_topics,
            quota.max_partitions_per_topic,
            quota.max_total_partitions,
            quota.max_storage_bytes,
            quota.max_retention_days,
            quota.max_produce_bytes_per_sec,
            quota.max_consume_bytes_per_sec,
            quota.max_requests_per_sec,
            quota.max_consumer_groups,
            quota.max_schemas,
            quota.max_schema_versions_per_subject,
            quota.max_connections,
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(())
    }

    async fn get_organization_usage(
        &self,
        organization_id: &str,
    ) -> Result<Vec<OrganizationUsage>> {
        let query = format!(
            "SELECT organization_id, metric, value, period_start \
             FROM organization_usage WHERE organization_id = '{}' ORDER BY metric",
            organization_id
        );

        let rows: Vec<(String, String, i64, i64)> =
            sqlx::query_as(&query).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(
                |(organization_id, metric, value, period_start)| OrganizationUsage {
                    organization_id,
                    metric,
                    value,
                    period_start,
                },
            )
            .collect())
    }

    async fn update_organization_usage(
        &self,
        organization_id: &str,
        metric: &str,
        value: i64,
    ) -> Result<()> {
        let now = Self::now_ms();

        let query = format!(
            "INSERT INTO organization_usage (organization_id, metric, value, period_start, updated_at) \
             VALUES ('{}', '{}', {}, {}, {}) \
             ON CONFLICT(organization_id, metric) DO UPDATE SET \
                value = excluded.value, \
                updated_at = excluded.updated_at",
            organization_id, metric, value, now, now
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(())
    }

    async fn increment_organization_usage(
        &self,
        organization_id: &str,
        metric: &str,
        delta: i64,
    ) -> Result<()> {
        let now = Self::now_ms();

        let query = format!(
            "INSERT INTO organization_usage (organization_id, metric, value, period_start, updated_at) \
             VALUES ('{}', '{}', {}, {}, {}) \
             ON CONFLICT(organization_id, metric) DO UPDATE SET \
                value = organization_usage.value + excluded.value, \
                updated_at = excluded.updated_at",
            organization_id, metric, delta, now, now
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(())
    }

    // ============================================================
    // EXACTLY-ONCE SEMANTICS (Phase 16)
    // ============================================================

    async fn init_producer(&self, config: InitProducerConfig) -> Result<Producer> {
        let now = Self::now_ms();

        // Check if transactional producer already exists
        if let Some(ref transactional_id) = config.transactional_id {
            let existing = self
                .get_producer_by_transactional_id(
                    transactional_id,
                    config.organization_id.as_deref(),
                )
                .await?;

            if let Some(mut producer) = existing {
                // Bump epoch to fence old producer instances
                let new_epoch = producer.epoch + 1;
                let query = format!(
                    "UPDATE producers SET epoch = {}, last_heartbeat = {}, state = 'active' WHERE id = '{}'",
                    new_epoch, now, producer.id
                );
                sqlx::query(&query).execute(&self.pool).await?;

                producer.epoch = new_epoch;
                producer.last_heartbeat = now;
                producer.state = ProducerState::Active;
                return Ok(producer);
            }
        }

        // Create new producer
        let producer_id = uuid::Uuid::new_v4().to_string();
        let metadata_json = serde_json::to_string(&config.metadata)?;
        let org_id = config.organization_id.as_deref().unwrap_or("");
        let txn_id = config.transactional_id.as_deref().unwrap_or("");

        let query = format!(
            "INSERT INTO producers (id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata) \
             VALUES ('{}', NULLIF('{}', ''), NULLIF('{}', ''), 0, {}, {}, 'active', '{}')",
            producer_id,
            org_id,
            txn_id,
            now,
            now,
            metadata_json.replace('\'', "''")
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(Producer {
            id: producer_id,
            organization_id: config.organization_id,
            transactional_id: config.transactional_id,
            epoch: 0,
            state: ProducerState::Active,
            created_at: now,
            last_heartbeat: now,
            metadata: config.metadata,
            numeric_id: None,
        })
    }

    async fn get_producer(&self, producer_id: &str) -> Result<Option<Producer>> {
        let query = format!(
            "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
             FROM producers WHERE id = '{}'",
            producer_id
        );

        let row: Option<(
            String,
            Option<String>,
            Option<String>,
            i64,
            i64,
            i64,
            String,
            String,
            Option<i64>,
        )> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(
                id,
                organization_id,
                transactional_id,
                epoch,
                created_at,
                last_heartbeat,
                state,
                metadata_json,
                numeric_id,
            )| {
                Producer {
                    id,
                    organization_id,
                    transactional_id,
                    epoch: epoch as u32,
                    state: state.parse().unwrap_or(ProducerState::Active),
                    created_at,
                    last_heartbeat,
                    metadata: serde_json::from_str(&metadata_json).ok(),
                    numeric_id,
                }
            },
        ))
    }

    async fn get_producer_by_transactional_id(
        &self,
        transactional_id: &str,
        organization_id: Option<&str>,
    ) -> Result<Option<Producer>> {
        let query = match organization_id {
            Some(org_id) => format!(
                "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
                 FROM producers WHERE transactional_id = '{}' AND organization_id = '{}'",
                transactional_id, org_id
            ),
            None => format!(
                "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
                 FROM producers WHERE transactional_id = '{}' AND organization_id IS NULL",
                transactional_id
            ),
        };

        let row: Option<(
            String,
            Option<String>,
            Option<String>,
            i64,
            i64,
            i64,
            String,
            String,
            Option<i64>,
        )> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(
                id,
                organization_id,
                transactional_id,
                epoch,
                created_at,
                last_heartbeat,
                state,
                metadata_json,
                numeric_id,
            )| {
                Producer {
                    id,
                    organization_id,
                    transactional_id,
                    epoch: epoch as u32,
                    state: state.parse().unwrap_or(ProducerState::Active),
                    created_at,
                    last_heartbeat,
                    metadata: serde_json::from_str(&metadata_json).ok(),
                    numeric_id,
                }
            },
        ))
    }

    async fn update_producer_heartbeat(&self, producer_id: &str) -> Result<()> {
        let now = Self::now_ms();
        let query = format!(
            "UPDATE producers SET last_heartbeat = {} WHERE id = '{}'",
            now, producer_id
        );
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

    async fn fence_producer(&self, producer_id: &str) -> Result<()> {
        let query = format!(
            "UPDATE producers SET state = 'fenced' WHERE id = '{}'",
            producer_id
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::NotFoundError(format!(
                "Producer {} not found",
                producer_id
            )));
        }

        Ok(())
    }

    async fn cleanup_expired_producers(&self, timeout_ms: i64) -> Result<u64> {
        let cutoff = Self::now_ms() - timeout_ms;
        let query = format!(
            "DELETE FROM producers WHERE last_heartbeat < {} AND state != 'active'",
            cutoff
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn allocate_numeric_producer_id(&self, producer_id: &str) -> Result<i64> {
        // Atomically get and increment the sequence
        sqlx::query("UPDATE producer_id_sequence SET next_id = next_id + 1 WHERE id = 1")
            .execute(&self.pool)
            .await?;

        let row: (i64,) = sqlx::query_as(
            "SELECT next_id - 1 FROM producer_id_sequence WHERE id = 1",
        )
        .fetch_one(&self.pool)
        .await?;
        let numeric_id = row.0;

        // Set the numeric_id on the producer
        let update_query = format!(
            "UPDATE producers SET numeric_id = {} WHERE id = '{}'",
            numeric_id, producer_id
        );
        sqlx::query(&update_query).execute(&self.pool).await?;

        Ok(numeric_id)
    }

    async fn get_producer_by_numeric_id(&self, numeric_id: i64) -> Result<Option<Producer>> {
        let query = format!(
            "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
             FROM producers WHERE numeric_id = {}",
            numeric_id
        );

        let row: Option<(
            String,
            Option<String>,
            Option<String>,
            i64,
            i64,
            i64,
            String,
            String,
            Option<i64>,
        )> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(
                id,
                organization_id,
                transactional_id,
                epoch,
                created_at,
                last_heartbeat,
                state,
                metadata_json,
                numeric_id,
            )| {
                Producer {
                    id,
                    organization_id,
                    transactional_id,
                    epoch: epoch as u32,
                    state: state.parse().unwrap_or(ProducerState::Active),
                    created_at,
                    last_heartbeat,
                    metadata: serde_json::from_str(&metadata_json).ok(),
                    numeric_id,
                }
            },
        ))
    }

    // ---- Sequence Operations ----

    async fn get_producer_sequence(
        &self,
        producer_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<i64>> {
        let query = format!(
            "SELECT last_sequence FROM producer_sequences \
             WHERE producer_id = '{}' AND topic = '{}' AND partition_id = {}",
            producer_id, topic, partition_id
        );

        let row: Option<(i64,)> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;
        Ok(row.map(|(seq,)| seq))
    }

    async fn update_producer_sequence(
        &self,
        producer_id: &str,
        topic: &str,
        partition_id: u32,
        sequence: i64,
    ) -> Result<()> {
        let now = Self::now_ms();
        let query = format!(
            "INSERT INTO producer_sequences (producer_id, topic, partition_id, last_sequence, updated_at) \
             VALUES ('{}', '{}', {}, {}, {}) \
             ON CONFLICT(producer_id, topic, partition_id) DO UPDATE SET \
                last_sequence = excluded.last_sequence, \
                updated_at = excluded.updated_at",
            producer_id, topic, partition_id, sequence, now
        );
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

    async fn check_and_update_sequence(
        &self,
        producer_id: &str,
        topic: &str,
        partition_id: u32,
        base_sequence: i64,
        record_count: u32,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let now = Self::now_ms();

        // Get current sequence
        let query = format!(
            "SELECT last_sequence FROM producer_sequences \
             WHERE producer_id = '{}' AND topic = '{}' AND partition_id = {}",
            producer_id, topic, partition_id
        );
        let row: Option<(i64,)> = sqlx::query_as(&query).fetch_one(&mut *tx).await.ok();
        let last_sequence = row.map(|(seq,)| seq).unwrap_or(-1);

        // Check sequence validity
        if base_sequence <= last_sequence {
            // Duplicate
            return Ok(false);
        }

        if base_sequence > last_sequence + 1 {
            // Gap in sequence - could be due to producer failure
            return Err(MetadataError::SequenceError(format!(
                "Sequence gap: expected {}, got {}",
                last_sequence + 1,
                base_sequence
            )));
        }

        // Update sequence
        let new_sequence = base_sequence + record_count as i64 - 1;
        let query = format!(
            "INSERT INTO producer_sequences (producer_id, topic, partition_id, last_sequence, updated_at) \
             VALUES ('{}', '{}', {}, {}, {}) \
             ON CONFLICT(producer_id, topic, partition_id) DO UPDATE SET \
                last_sequence = excluded.last_sequence, \
                updated_at = excluded.updated_at",
            producer_id, topic, partition_id, new_sequence, now
        );
        sqlx::query(&query).execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(true)
    }

    // ---- Transaction Operations ----

    async fn begin_transaction(&self, producer_id: &str, timeout_ms: u32) -> Result<Transaction> {
        let now = Self::now_ms();
        let transaction_id = uuid::Uuid::new_v4().to_string();

        let query = format!(
            "INSERT INTO transactions (transaction_id, producer_id, state, timeout_ms, started_at, updated_at) \
             VALUES ('{}', '{}', 'ongoing', {}, {}, {})",
            transaction_id, producer_id, timeout_ms, now, now
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(Transaction {
            transaction_id,
            producer_id: producer_id.to_string(),
            state: TransactionState::Ongoing,
            timeout_ms,
            started_at: now,
            updated_at: now,
            completed_at: None,
        })
    }

    async fn get_transaction(&self, transaction_id: &str) -> Result<Option<Transaction>> {
        let query = format!(
            "SELECT transaction_id, producer_id, state, timeout_ms, started_at, updated_at, completed_at \
             FROM transactions WHERE transaction_id = '{}'",
            transaction_id
        );

        let row: Option<(String, String, String, i64, i64, i64, Option<i64>)> =
            sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(
                transaction_id,
                producer_id,
                state,
                timeout_ms,
                started_at,
                updated_at,
                completed_at,
            )| Transaction {
                transaction_id,
                producer_id,
                state: state.parse().unwrap_or(TransactionState::Ongoing),
                timeout_ms: timeout_ms as u32,
                started_at,
                updated_at,
                completed_at,
            },
        ))
    }

    async fn add_transaction_partition(
        &self,
        transaction_id: &str,
        topic: &str,
        partition_id: u32,
        first_offset: u64,
    ) -> Result<()> {
        let query = format!(
            "INSERT INTO transaction_partitions (transaction_id, topic, partition_id, first_offset, last_offset) \
             VALUES ('{}', '{}', {}, {}, {}) \
             ON CONFLICT(transaction_id, topic, partition_id) DO NOTHING",
            transaction_id, topic, partition_id, first_offset, first_offset
        );
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

    async fn update_transaction_partition_offset(
        &self,
        transaction_id: &str,
        topic: &str,
        partition_id: u32,
        last_offset: u64,
    ) -> Result<()> {
        let query = format!(
            "UPDATE transaction_partitions SET last_offset = {} \
             WHERE transaction_id = '{}' AND topic = '{}' AND partition_id = {}",
            last_offset, transaction_id, topic, partition_id
        );
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

    async fn get_transaction_partitions(
        &self,
        transaction_id: &str,
    ) -> Result<Vec<TransactionPartition>> {
        let query = format!(
            "SELECT transaction_id, topic, partition_id, first_offset, last_offset \
             FROM transaction_partitions WHERE transaction_id = '{}' ORDER BY topic, partition_id",
            transaction_id
        );

        let rows: Vec<(String, String, i64, i64, i64)> =
            sqlx::query_as(&query).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(
                |(transaction_id, topic, partition_id, first_offset, last_offset)| {
                    TransactionPartition {
                        transaction_id,
                        topic,
                        partition_id: partition_id as u32,
                        first_offset: first_offset as u64,
                        last_offset: last_offset as u64,
                    }
                },
            )
            .collect())
    }

    async fn prepare_transaction(&self, transaction_id: &str) -> Result<()> {
        let now = Self::now_ms();
        let query = format!(
            "UPDATE transactions SET state = 'preparing', updated_at = {} WHERE transaction_id = '{}' AND state = 'ongoing'",
            now, transaction_id
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} is not in ongoing state",
                transaction_id
            )));
        }

        Ok(())
    }

    async fn commit_transaction(&self, transaction_id: &str) -> Result<i64> {
        let now = Self::now_ms();

        // Get transaction partitions for writing markers
        let partitions = self.get_transaction_partitions(transaction_id).await?;

        let mut tx = self.pool.begin().await?;

        // Update transaction state
        let query = format!(
            "UPDATE transactions SET state = 'committed', updated_at = {}, completed_at = {} \
             WHERE transaction_id = '{}' AND (state = 'ongoing' OR state = 'preparing')",
            now, now, transaction_id
        );
        let result = sqlx::query(&query).execute(&mut *tx).await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} cannot be committed",
                transaction_id
            )));
        }

        // Write commit markers for each partition
        for partition in &partitions {
            let marker_id = uuid::Uuid::new_v4().to_string();
            let marker_query = format!(
                "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, offset, marker_type, created_at) \
                 VALUES ('{}', '{}', '{}', {}, {}, 'commit', {})",
                marker_id, transaction_id, partition.topic, partition.partition_id, partition.last_offset, now
            );
            sqlx::query(&marker_query).execute(&mut *tx).await?;

            // Update LSO for each partition
            let lso_query = format!(
                "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
                 VALUES ('{}', {}, {}, {}) \
                 ON CONFLICT(topic, partition_id) DO UPDATE SET \
                    last_stable_offset = MAX(partition_lso.last_stable_offset, excluded.last_stable_offset), \
                    updated_at = excluded.updated_at",
                partition.topic, partition.partition_id, partition.last_offset + 1, now
            );
            sqlx::query(&lso_query).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(now)
    }

    async fn abort_transaction(&self, transaction_id: &str) -> Result<()> {
        let now = Self::now_ms();

        // Get transaction partitions for writing markers
        let partitions = self.get_transaction_partitions(transaction_id).await?;

        let mut tx = self.pool.begin().await?;

        // Update transaction state
        let query = format!(
            "UPDATE transactions SET state = 'aborted', updated_at = {}, completed_at = {} \
             WHERE transaction_id = '{}' AND (state = 'ongoing' OR state = 'preparing')",
            now, now, transaction_id
        );
        let result = sqlx::query(&query).execute(&mut *tx).await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} cannot be aborted",
                transaction_id
            )));
        }

        // Write abort markers for each partition
        for partition in &partitions {
            let marker_id = uuid::Uuid::new_v4().to_string();
            let marker_query = format!(
                "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, offset, marker_type, created_at) \
                 VALUES ('{}', '{}', '{}', {}, {}, 'abort', {})",
                marker_id, transaction_id, partition.topic, partition.partition_id, partition.last_offset, now
            );
            sqlx::query(&marker_query).execute(&mut *tx).await?;

            // Update LSO for each partition (even aborted transactions advance the LSO)
            let lso_query = format!(
                "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
                 VALUES ('{}', {}, {}, {}) \
                 ON CONFLICT(topic, partition_id) DO UPDATE SET \
                    last_stable_offset = MAX(partition_lso.last_stable_offset, excluded.last_stable_offset), \
                    updated_at = excluded.updated_at",
                partition.topic, partition.partition_id, partition.last_offset + 1, now
            );
            sqlx::query(&lso_query).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn cleanup_completed_transactions(&self, max_age_ms: i64) -> Result<u64> {
        let cutoff = Self::now_ms() - max_age_ms;
        let query = format!(
            "DELETE FROM transactions WHERE completed_at IS NOT NULL AND completed_at < {}",
            cutoff
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    // ---- Transaction Marker Operations ----

    async fn add_transaction_marker(&self, marker: TransactionMarker) -> Result<()> {
        let marker_id = uuid::Uuid::new_v4().to_string();
        let marker_type = match marker.marker_type {
            TransactionMarkerType::Commit => "commit",
            TransactionMarkerType::Abort => "abort",
        };
        let query = format!(
            "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, offset, marker_type, created_at) \
             VALUES ('{}', '{}', '{}', {}, {}, '{}', {})",
            marker_id, marker.transaction_id, marker.topic, marker.partition_id, marker.offset, marker_type, marker.timestamp
        );
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

    async fn get_transaction_markers(
        &self,
        topic: &str,
        partition_id: u32,
        min_offset: u64,
    ) -> Result<Vec<TransactionMarker>> {
        let query = format!(
            "SELECT transaction_id, topic, partition_id, offset, marker_type, created_at \
             FROM transaction_markers \
             WHERE topic = '{}' AND partition_id = {} AND offset >= {} \
             ORDER BY offset",
            topic, partition_id, min_offset
        );

        let rows: Vec<(String, String, i64, i64, String, i64)> =
            sqlx::query_as(&query).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(
                |(transaction_id, topic, partition_id, offset, marker_type, created_at)| {
                    TransactionMarker {
                        transaction_id,
                        topic,
                        partition_id: partition_id as u32,
                        offset: offset as u64,
                        marker_type: match marker_type.as_str() {
                            "commit" => TransactionMarkerType::Commit,
                            _ => TransactionMarkerType::Abort,
                        },
                        timestamp: created_at,
                    }
                },
            )
            .collect())
    }

    // ---- LSO Operations ----

    async fn get_last_stable_offset(&self, topic: &str, partition_id: u32) -> Result<u64> {
        let query = format!(
            "SELECT last_stable_offset FROM partition_lso WHERE topic = '{}' AND partition_id = {}",
            topic, partition_id
        );

        let row: Option<(i64,)> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;
        Ok(row.map(|(lso,)| lso as u64).unwrap_or(0))
    }

    async fn update_last_stable_offset(
        &self,
        topic: &str,
        partition_id: u32,
        lso: u64,
    ) -> Result<()> {
        let now = Self::now_ms();
        let query = format!(
            "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
             VALUES ('{}', {}, {}, {}) \
             ON CONFLICT(topic, partition_id) DO UPDATE SET \
                last_stable_offset = excluded.last_stable_offset, \
                updated_at = excluded.updated_at",
            topic, partition_id, lso, now
        );
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

    // ============================================================
    // FAST LEADER HANDOFF
    // ============================================================

    async fn initiate_lease_transfer(
        &self,
        topic: &str,
        partition_id: u32,
        from_agent_id: &str,
        to_agent_id: &str,
        reason: LeaderChangeReason,
        timeout_ms: u32,
    ) -> Result<LeaseTransfer> {
        let now = Self::now_ms();
        let transfer_id = uuid::Uuid::new_v4().to_string();
        let timeout_at = now + timeout_ms as i64;
        let reason_str = reason.to_string();

        // Verify the lease exists and is held by from_agent_id
        let lease = self.get_partition_lease(topic, partition_id).await?;
        let lease = match lease {
            Some(l) if l.leader_agent_id == from_agent_id => l,
            Some(_) => {
                return Err(MetadataError::ConflictError(format!(
                    "Partition {}/{} is not held by agent {}",
                    topic, partition_id, from_agent_id
                )));
            }
            None => {
                return Err(MetadataError::NotFoundError(format!(
                    "No lease found for partition {}/{}",
                    topic, partition_id
                )));
            }
        };

        // Check for existing pending transfers
        let existing_query = format!(
            "SELECT transfer_id FROM lease_transfers \
             WHERE topic = '{}' AND partition_id = {} AND state IN ('pending', 'accepted', 'completing')",
            topic, partition_id
        );
        let existing: Option<(String,)> = sqlx::query_as(&existing_query)
            .fetch_optional(&self.pool)
            .await?;

        if existing.is_some() {
            return Err(MetadataError::ConflictError(format!(
                "A transfer is already in progress for partition {}/{}",
                topic, partition_id
            )));
        }

        // Create the transfer record
        let query = format!(
            "INSERT INTO lease_transfers (transfer_id, topic, partition_id, from_agent_id, to_agent_id, \
             from_epoch, state, reason, initiated_at, timeout_at) \
             VALUES ('{}', '{}', {}, '{}', '{}', {}, 'pending', '{}', {}, {})",
            transfer_id, topic, partition_id, from_agent_id, to_agent_id,
            lease.epoch, reason_str, now, timeout_at
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(LeaseTransfer {
            transfer_id,
            topic: topic.to_string(),
            partition_id,
            from_agent_id: from_agent_id.to_string(),
            to_agent_id: to_agent_id.to_string(),
            from_epoch: lease.epoch,
            state: LeaseTransferState::Pending,
            reason,
            initiated_at: now,
            completed_at: None,
            timeout_at,
            last_flushed_offset: None,
            high_watermark: None,
            error: None,
        })
    }

    async fn accept_lease_transfer(
        &self,
        transfer_id: &str,
        agent_id: &str,
    ) -> Result<LeaseTransfer> {
        let now = Self::now_ms();

        // Get the transfer
        let transfer = self.get_lease_transfer(transfer_id).await?;
        let transfer = match transfer {
            Some(t) => t,
            None => {
                return Err(MetadataError::NotFoundError(format!(
                    "Transfer {} not found",
                    transfer_id
                )));
            }
        };

        // Verify state and agent
        if transfer.state != LeaseTransferState::Pending {
            return Err(MetadataError::ConflictError(format!(
                "Transfer {} is in state {:?}, not pending",
                transfer_id, transfer.state
            )));
        }

        if transfer.to_agent_id != agent_id {
            return Err(MetadataError::ConflictError(format!(
                "Transfer {} is for agent {}, not {}",
                transfer_id, transfer.to_agent_id, agent_id
            )));
        }

        // Check timeout
        if now > transfer.timeout_at {
            return Err(MetadataError::ConflictError(format!(
                "Transfer {} has timed out",
                transfer_id
            )));
        }

        // Update state to accepted
        let query = format!(
            "UPDATE lease_transfers SET state = 'accepted' WHERE transfer_id = '{}'",
            transfer_id
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(LeaseTransfer {
            state: LeaseTransferState::Accepted,
            ..transfer
        })
    }

    async fn complete_lease_transfer(
        &self,
        transfer_id: &str,
        last_flushed_offset: u64,
        high_watermark: u64,
    ) -> Result<PartitionLease> {
        let now = Self::now_ms();

        // Get the transfer
        let transfer = self.get_lease_transfer(transfer_id).await?;
        let transfer = match transfer {
            Some(t) => t,
            None => {
                return Err(MetadataError::NotFoundError(format!(
                    "Transfer {} not found",
                    transfer_id
                )));
            }
        };

        // Verify state
        if transfer.state != LeaseTransferState::Accepted {
            return Err(MetadataError::ConflictError(format!(
                "Transfer {} is in state {:?}, not accepted",
                transfer_id, transfer.state
            )));
        }

        // Begin transaction for atomic lease transfer
        let mut tx = self.pool.begin().await?;

        // Update transfer state to completing
        let query = format!(
            "UPDATE lease_transfers SET state = 'completing', last_flushed_offset = {}, high_watermark = {} \
             WHERE transfer_id = '{}'",
            last_flushed_offset, high_watermark, transfer_id
        );
        sqlx::query(&query).execute(&mut *tx).await?;

        // Transfer the lease atomically
        let new_epoch = transfer.from_epoch + 1;
        let lease_expires_at = now + 30_000; // 30 second lease

        let lease_query = format!(
            "UPDATE partition_leases SET \
             leader_agent_id = '{}', epoch = {}, lease_expires_at = {}, acquired_at = {} \
             WHERE topic = '{}' AND partition_id = {}",
            transfer.to_agent_id,
            new_epoch,
            lease_expires_at,
            now,
            transfer.topic,
            transfer.partition_id
        );
        sqlx::query(&lease_query).execute(&mut *tx).await?;

        // Mark transfer as completed
        let complete_query = format!(
            "UPDATE lease_transfers SET state = 'completed', completed_at = {} WHERE transfer_id = '{}'",
            now, transfer_id
        );
        sqlx::query(&complete_query).execute(&mut *tx).await?;

        // Record the leader change
        let change_query = format!(
            "INSERT INTO leader_changes (topic, partition_id, from_agent_id, to_agent_id, reason, epoch, gap_ms, changed_at) \
             VALUES ('{}', {}, '{}', '{}', '{}', {}, 0, {})",
            transfer.topic, transfer.partition_id, transfer.from_agent_id,
            transfer.to_agent_id, transfer.reason, new_epoch, now
        );
        sqlx::query(&change_query).execute(&mut *tx).await?;

        tx.commit().await?;

        Ok(PartitionLease {
            topic: transfer.topic,
            partition_id: transfer.partition_id,
            leader_agent_id: transfer.to_agent_id,
            lease_expires_at,
            acquired_at: now,
            epoch: new_epoch,
        })
    }

    async fn reject_lease_transfer(
        &self,
        transfer_id: &str,
        _agent_id: &str,
        reason: &str,
    ) -> Result<()> {
        let now = Self::now_ms();

        let query = format!(
            "UPDATE lease_transfers SET state = 'rejected', completed_at = {}, error = '{}' \
             WHERE transfer_id = '{}' AND state IN ('pending', 'accepted')",
            now,
            reason.replace('\'', "''"),
            transfer_id
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::ConflictError(format!(
                "Transfer {} not found or already completed",
                transfer_id
            )));
        }

        Ok(())
    }

    async fn get_lease_transfer(&self, transfer_id: &str) -> Result<Option<LeaseTransfer>> {
        let query = format!(
            "SELECT transfer_id, topic, partition_id, from_agent_id, to_agent_id, from_epoch, \
             state, reason, initiated_at, completed_at, timeout_at, last_flushed_offset, \
             high_watermark, error \
             FROM lease_transfers WHERE transfer_id = '{}'",
            transfer_id
        );

        let row: Option<(
            String,
            String,
            i64,
            String,
            String,
            i64,
            String,
            String,
            i64,
            Option<i64>,
            i64,
            Option<i64>,
            Option<i64>,
            Option<String>,
        )> = sqlx::query_as(&query).fetch_optional(&self.pool).await?;

        Ok(row.map(
            |(
                transfer_id,
                topic,
                partition_id,
                from_agent_id,
                to_agent_id,
                from_epoch,
                state,
                reason,
                initiated_at,
                completed_at,
                timeout_at,
                last_flushed_offset,
                high_watermark,
                error,
            )| {
                LeaseTransfer {
                    transfer_id,
                    topic,
                    partition_id: partition_id as u32,
                    from_agent_id,
                    to_agent_id,
                    from_epoch: from_epoch as u64,
                    state: state.parse().unwrap_or(LeaseTransferState::Pending),
                    reason: reason
                        .parse()
                        .unwrap_or(LeaderChangeReason::GracefulHandoff),
                    initiated_at,
                    completed_at,
                    timeout_at,
                    last_flushed_offset: last_flushed_offset.map(|o| o as u64),
                    high_watermark: high_watermark.map(|o| o as u64),
                    error,
                }
            },
        ))
    }

    async fn get_pending_transfers_for_agent(&self, agent_id: &str) -> Result<Vec<LeaseTransfer>> {
        let query = format!(
            "SELECT transfer_id, topic, partition_id, from_agent_id, to_agent_id, from_epoch, \
             state, reason, initiated_at, completed_at, timeout_at, last_flushed_offset, \
             high_watermark, error \
             FROM lease_transfers \
             WHERE (from_agent_id = '{}' OR to_agent_id = '{}') \
             AND state IN ('pending', 'accepted', 'completing') \
             ORDER BY initiated_at",
            agent_id, agent_id
        );

        let rows: Vec<(
            String,
            String,
            i64,
            String,
            String,
            i64,
            String,
            String,
            i64,
            Option<i64>,
            i64,
            Option<i64>,
            Option<i64>,
            Option<String>,
        )> = sqlx::query_as(&query).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    transfer_id,
                    topic,
                    partition_id,
                    from_agent_id,
                    to_agent_id,
                    from_epoch,
                    state,
                    reason,
                    initiated_at,
                    completed_at,
                    timeout_at,
                    last_flushed_offset,
                    high_watermark,
                    error,
                )| {
                    LeaseTransfer {
                        transfer_id,
                        topic,
                        partition_id: partition_id as u32,
                        from_agent_id,
                        to_agent_id,
                        from_epoch: from_epoch as u64,
                        state: state.parse().unwrap_or(LeaseTransferState::Pending),
                        reason: reason
                            .parse()
                            .unwrap_or(LeaderChangeReason::GracefulHandoff),
                        initiated_at,
                        completed_at,
                        timeout_at,
                        last_flushed_offset: last_flushed_offset.map(|o| o as u64),
                        high_watermark: high_watermark.map(|o| o as u64),
                        error,
                    }
                },
            )
            .collect())
    }

    async fn cleanup_timed_out_transfers(&self) -> Result<u64> {
        let now = Self::now_ms();

        let query = format!(
            "UPDATE lease_transfers SET state = 'timed_out', completed_at = {}, error = 'Transfer timed out' \
             WHERE state IN ('pending', 'accepted') AND timeout_at < {}",
            now, now
        );
        let result = sqlx::query(&query).execute(&self.pool).await?;

        Ok(result.rows_affected())
    }

    async fn record_leader_change(
        &self,
        topic: &str,
        partition_id: u32,
        from_agent_id: Option<&str>,
        to_agent_id: &str,
        reason: LeaderChangeReason,
        epoch: u64,
        gap_ms: i64,
    ) -> Result<()> {
        let now = Self::now_ms();
        let from_agent = from_agent_id.unwrap_or("");
        let reason_str = reason.to_string();

        let query = format!(
            "INSERT INTO leader_changes (topic, partition_id, from_agent_id, to_agent_id, reason, epoch, gap_ms, changed_at) \
             VALUES ('{}', {}, NULLIF('{}', ''), '{}', '{}', {}, {}, {})",
            topic, partition_id, from_agent, to_agent_id, reason_str, epoch, gap_ms, now
        );
        sqlx::query(&query).execute(&self.pool).await?;

        Ok(())
    }

    // ============================================================
    // MATERIALIZED VIEW OPERATIONS
    // ============================================================

    async fn create_materialized_view(
        &self,
        config: CreateMaterializedView,
    ) -> Result<MaterializedView> {
        let now = Self::now_ms();
        let id = format!("mv-{}-{}", &config.name, uuid::Uuid::new_v4());
        let org_id = config
            .organization_id
            .clone()
            .unwrap_or_else(|| "00000000-0000-0000-0000-000000000000".to_string());

        let (refresh_mode_str, interval_ms): (&str, Option<i64>) = match &config.refresh_mode {
            MaterializedViewRefreshMode::Continuous => ("continuous", None),
            MaterializedViewRefreshMode::Periodic { interval_ms } => {
                ("periodic", Some(*interval_ms))
            }
            MaterializedViewRefreshMode::Manual => ("manual", None),
        };

        sqlx::query(
            "INSERT INTO materialized_views \
             (id, organization_id, name, source_topic, query_sql, refresh_mode, refresh_interval_ms, status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 'initializing', ?, ?)",
        )
        .bind(&id)
        .bind(&org_id)
        .bind(&config.name)
        .bind(&config.source_topic)
        .bind(&config.query_sql)
        .bind(refresh_mode_str)
        .bind(interval_ms)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(MaterializedView {
            id,
            organization_id: org_id,
            name: config.name,
            source_topic: config.source_topic,
            query_sql: config.query_sql,
            refresh_mode: config.refresh_mode,
            status: MaterializedViewStatus::Initializing,
            error_message: None,
            row_count: 0,
            last_refresh_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_materialized_view(&self, name: &str) -> Result<Option<MaterializedView>> {
        let row = sqlx::query(
            "SELECT id, organization_id, name, source_topic, query_sql, \
                    refresh_mode, refresh_interval_ms, status, error_message, \
                    row_count, last_refresh_at, created_at, updated_at \
             FROM materialized_views WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Self::row_to_materialized_view))
    }

    async fn get_materialized_view_by_id(&self, id: &str) -> Result<Option<MaterializedView>> {
        let row = sqlx::query(
            "SELECT id, organization_id, name, source_topic, query_sql, \
                    refresh_mode, refresh_interval_ms, status, error_message, \
                    row_count, last_refresh_at, created_at, updated_at \
             FROM materialized_views WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Self::row_to_materialized_view))
    }

    async fn list_materialized_views(&self) -> Result<Vec<MaterializedView>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, name, source_topic, query_sql, \
                    refresh_mode, refresh_interval_ms, status, error_message, \
                    row_count, last_refresh_at, created_at, updated_at \
             FROM materialized_views ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(Self::row_to_materialized_view)
            .collect())
    }

    async fn update_materialized_view_status(
        &self,
        id: &str,
        status: MaterializedViewStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let status_str = match status {
            MaterializedViewStatus::Initializing => "initializing",
            MaterializedViewStatus::Running => "running",
            MaterializedViewStatus::Paused => "paused",
            MaterializedViewStatus::Error => "error",
        };
        let now = Self::now_ms();

        sqlx::query(
            "UPDATE materialized_views SET status = ?, error_message = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status_str)
        .bind(error_message)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_materialized_view_stats(
        &self,
        id: &str,
        row_count: u64,
        last_refresh_at: i64,
    ) -> Result<()> {
        let now = Self::now_ms();

        sqlx::query(
            "UPDATE materialized_views SET row_count = ?, last_refresh_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(row_count as i64)
        .bind(last_refresh_at)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_materialized_view(&self, name: &str) -> Result<()> {
        // Get the view ID first so we can cascade-delete offsets and data
        let row = sqlx::query("SELECT id FROM materialized_views WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let id: String = row.get("id");
            // Delete data and offsets first (SQLite may not enforce FK cascades)
            sqlx::query("DELETE FROM materialized_view_data WHERE view_id = ?")
                .bind(&id)
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM materialized_view_offsets WHERE view_id = ?")
                .bind(&id)
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM materialized_views WHERE id = ?")
                .bind(&id)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn get_materialized_view_offsets(
        &self,
        view_id: &str,
    ) -> Result<Vec<MaterializedViewOffset>> {
        let rows = sqlx::query(
            "SELECT view_id, partition_id, last_offset, last_processed_at \
             FROM materialized_view_offsets WHERE view_id = ? ORDER BY partition_id",
        )
        .bind(view_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MaterializedViewOffset {
                view_id: r.get("view_id"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                last_offset: r.get::<i64, _>("last_offset") as u64,
                last_processed_at: r.get::<i64, _>("last_processed_at"),
            })
            .collect())
    }

    async fn update_materialized_view_offset(
        &self,
        view_id: &str,
        partition_id: u32,
        last_offset: u64,
    ) -> Result<()> {
        let now = Self::now_ms();

        sqlx::query(
            "INSERT INTO materialized_view_offsets (view_id, partition_id, last_offset, last_processed_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(view_id, partition_id) DO UPDATE SET \
                last_offset = excluded.last_offset, \
                last_processed_at = excluded.last_processed_at",
        )
        .bind(view_id)
        .bind(partition_id as i32)
        .bind(last_offset as i64)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_materialized_view_data(
        &self,
        view_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MaterializedViewData>> {
        let limit_val = limit.unwrap_or(1000) as i64;

        let rows = sqlx::query(
            "SELECT view_id, agg_key, agg_values, window_start, window_end, updated_at \
             FROM materialized_view_data WHERE view_id = ? ORDER BY agg_key LIMIT ?",
        )
        .bind(view_id)
        .bind(limit_val)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let agg_values_str: String = r.get("agg_values");
                let agg_values: serde_json::Value =
                    serde_json::from_str(&agg_values_str).unwrap_or(serde_json::Value::Null);
                MaterializedViewData {
                    view_id: r.get("view_id"),
                    agg_key: r.get("agg_key"),
                    agg_values,
                    window_start: r.get::<Option<i64>, _>("window_start"),
                    window_end: r.get::<Option<i64>, _>("window_end"),
                    updated_at: r.get::<i64, _>("updated_at"),
                }
            })
            .collect())
    }

    async fn upsert_materialized_view_data(&self, data: MaterializedViewData) -> Result<()> {
        let now = Self::now_ms();
        let agg_values_str = serde_json::to_string(&data.agg_values)
            .unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            "INSERT INTO materialized_view_data (view_id, agg_key, agg_values, window_start, window_end, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(view_id, agg_key) DO UPDATE SET \
                agg_values = excluded.agg_values, \
                window_start = excluded.window_start, \
                window_end = excluded.window_end, \
                updated_at = excluded.updated_at",
        )
        .bind(&data.view_id)
        .bind(&data.agg_key)
        .bind(&agg_values_str)
        .bind(data.window_start)
        .bind(data.window_end)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============================================================
    // CONNECTOR OPERATIONS
    // ============================================================

    async fn create_connector(&self, connector: ConnectorInfo) -> Result<()> {
        let topics_json = serde_json::to_string(&connector.topics)?;
        let config_json = serde_json::to_string(&connector.config)?;

        sqlx::query(
            "INSERT INTO connectors (name, connector_type, connector_class, topics, config, state, records_processed, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&connector.name)
        .bind(&connector.connector_type)
        .bind(&connector.connector_class)
        .bind(&topics_json)
        .bind(&config_json)
        .bind(&connector.state)
        .bind(connector.created_at)
        .bind(connector.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_connector(&self, name: &str) -> Result<Option<ConnectorInfo>> {
        let row = sqlx::query(
            "SELECT name, connector_type, connector_class, topics, config, state, \
                    error_message, records_processed, created_at, updated_at \
             FROM connectors WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(Self::row_to_connector_info(r)?)),
            None => Ok(None),
        }
    }

    async fn list_connectors(&self) -> Result<Vec<ConnectorInfo>> {
        let rows = sqlx::query(
            "SELECT name, connector_type, connector_class, topics, config, state, \
                    error_message, records_processed, created_at, updated_at \
             FROM connectors ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(Self::row_to_connector_info)
            .collect()
    }

    async fn delete_connector(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM connectors WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_connector_state(&self, name: &str, state: &str, error_message: Option<&str>) -> Result<()> {
        let now = Self::now_ms();

        sqlx::query(
            "UPDATE connectors SET state = ?, error_message = ?, updated_at = ? WHERE name = ?",
        )
        .bind(state)
        .bind(error_message)
        .bind(now)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_connector_records_processed(&self, name: &str, records_processed: i64) -> Result<()> {
        let now = Self::now_ms();

        sqlx::query(
            "UPDATE connectors SET records_processed = ?, updated_at = ? WHERE name = ?",
        )
        .bind(records_processed)
        .bind(now)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
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
            cleanup_policy: CleanupPolicy::default(),
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
            cleanup_policy: CleanupPolicy::default(),
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
                cleanup_policy: CleanupPolicy::default(),
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
                cleanup_policy: CleanupPolicy::default(),
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

    #[tokio::test]
    async fn test_materialized_view_crud() {
        let store = setup_test_store().await;

        // Create a materialized view
        let config = CreateMaterializedView {
            name: "test_view".to_string(),
            source_topic: "events".to_string(),
            query_sql: "SELECT COUNT(*) FROM events GROUP BY TUMBLE(timestamp, INTERVAL '5 minutes')".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Continuous,
            organization_id: None,
        };

        let view = store.create_materialized_view(config).await.unwrap();
        assert_eq!(view.name, "test_view");
        assert_eq!(view.source_topic, "events");
        assert_eq!(view.row_count, 0);

        // Get by name
        let fetched = store.get_materialized_view("test_view").await.unwrap().unwrap();
        assert_eq!(fetched.id, view.id);
        assert_eq!(fetched.name, "test_view");

        // Get by id
        let fetched_by_id = store.get_materialized_view_by_id(&view.id).await.unwrap().unwrap();
        assert_eq!(fetched_by_id.name, "test_view");

        // List views
        let views = store.list_materialized_views().await.unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].name, "test_view");

        // Update status
        store
            .update_materialized_view_status(&view.id, MaterializedViewStatus::Running, None)
            .await
            .unwrap();
        let updated = store.get_materialized_view_by_id(&view.id).await.unwrap().unwrap();
        assert_eq!(updated.status, MaterializedViewStatus::Running);

        // Update stats
        store
            .update_materialized_view_stats(&view.id, 42, 1000)
            .await
            .unwrap();
        let updated = store.get_materialized_view_by_id(&view.id).await.unwrap().unwrap();
        assert_eq!(updated.row_count, 42);

        // Delete
        store.delete_materialized_view("test_view").await.unwrap();
        let deleted = store.get_materialized_view("test_view").await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_materialized_view_data_upsert_and_query() {
        let store = setup_test_store().await;

        // Create a view first
        let config = CreateMaterializedView {
            name: "data_view".to_string(),
            source_topic: "events".to_string(),
            query_sql: "SELECT COUNT(*) FROM events".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Continuous,
            organization_id: None,
        };
        let view = store.create_materialized_view(config).await.unwrap();

        // Upsert some aggregation data
        let data1 = MaterializedViewData {
            view_id: view.id.clone(),
            agg_key: "user1:1000:2000".to_string(),
            agg_values: serde_json::json!({"count_0": 5, "sum_1": 100}),
            window_start: Some(1000),
            window_end: Some(2000),
            updated_at: 0,
        };
        store.upsert_materialized_view_data(data1).await.unwrap();

        let data2 = MaterializedViewData {
            view_id: view.id.clone(),
            agg_key: "user2:1000:2000".to_string(),
            agg_values: serde_json::json!({"count_0": 3, "sum_1": 50}),
            window_start: Some(1000),
            window_end: Some(2000),
            updated_at: 0,
        };
        store.upsert_materialized_view_data(data2).await.unwrap();

        // Query data
        let results = store.get_materialized_view_data(&view.id, None).await.unwrap();
        assert_eq!(results.len(), 2);

        // Verify values
        let r1 = results.iter().find(|r| r.agg_key == "user1:1000:2000").unwrap();
        assert_eq!(r1.agg_values["count_0"], 5);
        assert_eq!(r1.agg_values["sum_1"], 100);
        assert_eq!(r1.window_start, Some(1000));
        assert_eq!(r1.window_end, Some(2000));

        // Upsert (update existing key)
        let data1_updated = MaterializedViewData {
            view_id: view.id.clone(),
            agg_key: "user1:1000:2000".to_string(),
            agg_values: serde_json::json!({"count_0": 10, "sum_1": 200}),
            window_start: Some(1000),
            window_end: Some(2000),
            updated_at: 0,
        };
        store.upsert_materialized_view_data(data1_updated).await.unwrap();

        let results = store.get_materialized_view_data(&view.id, None).await.unwrap();
        assert_eq!(results.len(), 2); // still 2 rows
        let r1 = results.iter().find(|r| r.agg_key == "user1:1000:2000").unwrap();
        assert_eq!(r1.agg_values["count_0"], 10);

        // Test limit
        let limited = store.get_materialized_view_data(&view.id, Some(1)).await.unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn test_materialized_view_offsets() {
        let store = setup_test_store().await;

        // Create a view
        let config = CreateMaterializedView {
            name: "offset_view".to_string(),
            source_topic: "events".to_string(),
            query_sql: "SELECT COUNT(*) FROM events".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Continuous,
            organization_id: None,
        };
        let view = store.create_materialized_view(config).await.unwrap();

        // Set offsets for partitions
        store.update_materialized_view_offset(&view.id, 0, 100).await.unwrap();
        store.update_materialized_view_offset(&view.id, 1, 200).await.unwrap();

        // Get offsets
        let offsets = store.get_materialized_view_offsets(&view.id).await.unwrap();
        assert_eq!(offsets.len(), 2);
        assert!(offsets.iter().any(|o| o.partition_id == 0 && o.last_offset == 100));
        assert!(offsets.iter().any(|o| o.partition_id == 1 && o.last_offset == 200));

        // Update an existing offset
        store.update_materialized_view_offset(&view.id, 0, 500).await.unwrap();
        let offsets = store.get_materialized_view_offsets(&view.id).await.unwrap();
        let p0 = offsets.iter().find(|o| o.partition_id == 0).unwrap();
        assert_eq!(p0.last_offset, 500);
    }

    #[tokio::test]
    async fn test_materialized_view_cascade_delete() {
        let store = setup_test_store().await;

        // Create view with data and offsets
        let config = CreateMaterializedView {
            name: "cascade_view".to_string(),
            source_topic: "events".to_string(),
            query_sql: "SELECT COUNT(*) FROM events".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Continuous,
            organization_id: None,
        };
        let view = store.create_materialized_view(config).await.unwrap();

        store.update_materialized_view_offset(&view.id, 0, 100).await.unwrap();
        store.upsert_materialized_view_data(MaterializedViewData {
            view_id: view.id.clone(),
            agg_key: "key1:0:1000".to_string(),
            agg_values: serde_json::json!({"count_0": 1}),
            window_start: Some(0),
            window_end: Some(1000),
            updated_at: 0,
        }).await.unwrap();

        // Delete view — should cascade
        store.delete_materialized_view("cascade_view").await.unwrap();

        // Verify data and offsets are gone
        let offsets = store.get_materialized_view_offsets(&view.id).await.unwrap();
        assert!(offsets.is_empty());
        let data = store.get_materialized_view_data(&view.id, None).await.unwrap();
        assert!(data.is_empty());
    }
}
