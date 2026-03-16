//! PostgreSQL Metadata Store Implementation
//!
//! Production-ready metadata backend for StreamHouse using PostgreSQL 14+.
//!
//! ## What Is This?
//!
//! This module implements the [`MetadataStore`] trait using PostgreSQL as the
//! persistence layer. It stores **metadata only** - actual event data lives in S3.
//!
//! ## What Gets Stored in PostgreSQL?
//!
//! | Data | Example | Size per Entry |
//! |------|---------|----------------|
//! | Topic metadata | Name, partition count, retention | ~100 bytes |
//! | Partition state | High watermark offsets | ~50 bytes |
//! | Segment locations | S3 paths (s3://bucket/key) | ~200 bytes |
//! | Consumer offsets | Group progress tracking | ~100 bytes |
//! | Agent info (Phase 4) | Agent registration, leases | ~200 bytes |
//!
//! **Event data is NOT stored here** - it lives in S3 as binary segment files.
//!
//! ## When to Use This Backend
//!
//! Use PostgreSQL instead of SQLite when you need:
//!
//! - **Multi-node deployments**: Multiple StreamHouse agents sharing metadata
//! - **High availability**: PostgreSQL replication for metadata durability
//! - **10K+ partitions**: Scales better than SQLite for large partition counts
//! - **Distributed coordination**: Agent leases for multi-writer scenarios (Phase 4)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Agent 1    │     │  Agent 2    │     │  Agent 3    │
//! │ (us-east-1) │     │ (us-west-2) │     │ (eu-west-1) │
//! └──────┬──────┘     └──────┬──────┘     └──────┬──────┘
//!        │                   │                   │
//!        └───────────────────┼───────────────────┘
//!                            │
//!                   ┌────────▼─────────┐
//!                   │   PostgreSQL     │  ← Shared metadata
//!                   │  (RDS/Aurora)    │
//!                   └──────────────────┘
//!
//! Each agent:
//! - Writes events to S3 (independent, parallel)
//! - Registers segments in PostgreSQL (coordinated via transactions)
//! - Acquires partition leases for leadership (Phase 4)
//! ```
//!
//! ## Performance Characteristics
//!
//! Based on PostgreSQL 16 on RDS (db.t3.medium):
//!
//! | Operation | Latency | QPS |
//! |-----------|---------|-----|
//! | Get topic | < 5ms | 10,000+ |
//! | Add segment | < 10ms | 5,000+ |
//! | Update high watermark | < 10ms | 5,000+ |
//! | Find segment for offset | < 10ms | 10,000+ |
//! | Acquire partition lease | < 50ms | 1,000+ |
//!
//! **Note**: Latencies include network RTT. Use connection pooling for best performance.
//!
//! ## Runtime Queries vs Compile-Time Macros
//!
//! This implementation uses **runtime queries** (`sqlx::query`) instead of
//! compile-time macros (`sqlx::query!`) to avoid DATABASE_URL dependency during
//! compilation. This allows building both SQLite and PostgreSQL backends together.
//!
//! ### Trade-offs
//!
//! | Approach | Pros | Cons |
//! |----------|------|------|
//! | `sqlx::query!` (compile-time) | Type-safe at compile time | Requires DATABASE_URL, can't build multi-backend |
//! | `sqlx::query` (runtime) | No DB dependency, flexible | Manual type casting with `.get()` |
//!
//! We chose runtime queries for **deployment flexibility** - compile once, run with
//! either SQLite (dev) or PostgreSQL (prod) by changing DATABASE_URL.
//!
//! ## Connection Pooling
//!
//! Uses `sqlx::PgPool` with:
//! - Default: 20 connections
//! - Configurable via [`PostgresMetadataStore::with_pool_options`]
//! - Thread-safe, shareable via `Arc<PostgresMetadataStore>`
//!
//! ## Migrations
//!
//! Runs automatically on startup via `sqlx::migrate!("./migrations-postgres")`:
//!
//! 1. `001_initial_schema.sql`: Topics, partitions, segments, consumers
//! 2. `002_agent_coordination.sql`: Agent registration and partition leases
//!
//! ## JSONB for Configuration
//!
//! Topic configs and agent metadata use PostgreSQL's JSONB type:
//!
//! ```sql
//! -- Store topic config as JSONB
//! INSERT INTO topics (name, config)
//! VALUES ('orders', '{"compression": "lz4", "retention.ms": "86400000"}');
//!
//! -- Query nested fields (future use)
//! SELECT * FROM topics WHERE config->>'compression' = 'lz4';
//! ```
//!
//! **Why JSONB?**
//! - Binary format (faster than TEXT)
//! - Indexable with GIN indexes
//! - No schema changes for new config keys
//! - Minimal serialization overhead (< 1µs for small HashMaps)
//!
//! ## Example Usage
//!
//! ```no_run
//! use streamhouse_metadata::{PostgresMetadataStore, MetadataStore, TopicConfig};
//! use std::collections::HashMap;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Connect to PostgreSQL
//! let store = PostgresMetadataStore::new(
//!     "postgres://user:pass@localhost/streamhouse"
//! ).await?;
//!
//! // Create a topic
//! let config = TopicConfig {
//!     name: "events".to_string(),
//!     partition_count: 16,
//!     retention_ms: Some(86400000), // 1 day
//!     config: HashMap::new(),
//! };
//! store.create_topic(config).await?;
//!
//! // Query metadata
//! let topic = store.get_topic("events").await?;
//! println!("Topic has {} partitions", topic.unwrap().partition_count);
//! # Ok(())
//! # }
//! ```
//!
//! ## Production Deployment
//!
//! ### Recommended Setup
//!
//! - **PostgreSQL 14+** (16 recommended)
//! - **Replication**: Streaming replication or Aurora multi-AZ
//! - **Connection pooling**: PgBouncer or built-in pool (20+ connections)
//! - **Monitoring**: Track query latency, connection pool saturation
//! - **Backups**: Point-in-time recovery (PITR) enabled
//!
//! ### Environment Configuration
//!
//! ```bash
//! # Production
//! DATABASE_URL=postgres://streamhouse:secret@prod-db.internal:5432/metadata
//!
//! # With SSL
//! DATABASE_URL=postgres://user:pass@db.example.com/streamhouse?sslmode=require
//! ```
//!
//! ### High Availability
//!
//! For multi-region deployments:
//!
//! 1. Use PostgreSQL replication (streaming or logical)
//! 2. Agents write to closest PostgreSQL replica
//! 3. Read replicas for metadata queries (if needed)
//! 4. Monitor lease expiration times for failover detection
//!
//! ## Phase 4: Agent Coordination
//!
//! The agent coordination tables (from migration 002) enable distributed writes:
//!
//! ```no_run
//! # use streamhouse_metadata::{PostgresMetadataStore, MetadataStore, AgentInfo};
//! # use std::collections::HashMap;
//! # async fn example(store: PostgresMetadataStore) -> Result<(), Box<dyn std::error::Error>> {
//! // Register agent
//! let agent = AgentInfo {
//!     agent_id: "agent-1".to_string(),
//!     address: "10.0.1.5:9090".to_string(),
//!     availability_zone: "us-east-1a".to_string(),
//!     agent_group: "writers".to_string(),
//!     last_heartbeat: chrono::Utc::now().timestamp_millis(),
//!     started_at: chrono::Utc::now().timestamp_millis(),
//!     metadata: HashMap::new(),
//! };
//! store.register_agent(agent).await?;
//!
//! // Acquire partition lease (leadership)
//! let lease = store.acquire_partition_lease(
//!     DEFAULT_ORGANIZATION_ID, // organization_id
//!     "events",      // topic
//!     0,             // partition_id
//!     "agent-1",     // agent_id
//!     30000          // lease_duration_ms
//! ).await?;
//!
//! println!("Acquired lease with epoch {}", lease.epoch);
//! # Ok(())
//! # }
//! ```
//!
//! ## Implementation Notes
//!
//! - All timestamps are **milliseconds since Unix epoch** (i64)
//! - Offsets are **64-bit unsigned** (u64 in Rust, BIGINT in PostgreSQL)
//! - Transactions ensure **atomicity** (e.g., create topic + partitions)
//! - **ON CONFLICT** handles upserts for idempotent operations
//! - **Foreign keys** with CASCADE ensure cleanup when topics deleted

use crate::{
    error::{MetadataError, Result},
    types::*,
    MetadataStore,
};
use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::Row;
use std::str::FromStr;

pub struct PostgresMetadataStore {
    pool: PgPool,
}

impl PostgresMetadataStore {
    pub async fn new(url: &str) -> Result<Self> {
        let options = PgConnectOptions::from_str(url)?;
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations-postgres").run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn with_pool_options(url: &str, pool_options: PgPoolOptions) -> Result<Self> {
        let options = PgConnectOptions::from_str(url)?;
        let pool = pool_options.connect_with(options).await?;
        sqlx::migrate!("./migrations-postgres").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}

#[async_trait]
impl MetadataStore for PostgresMetadataStore {
    async fn create_topic(&self, config: TopicConfig) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Include cleanup_policy in the config map for storage
        let mut config_map = config.config.clone();
        config_map.insert(
            "cleanup.policy".to_string(),
            config.cleanup_policy.as_str().to_string(),
        );
        let config_json = serde_json::to_value(&config_map)?;
        let mut tx = self.pool.begin().await?;

        // Use default organization for backwards compatibility
        let default_org_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap();

        sqlx::query(
            "INSERT INTO topics (organization_id, name, partition_count, retention_ms, created_at, updated_at, config)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(default_org_id)
        .bind(&config.name)
        .bind(config.partition_count as i32)
        .bind(config.retention_ms)
        .bind(now_ms)
        .bind(now_ms)
        .bind(config_json)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") {
                MetadataError::TopicAlreadyExists(config.name.clone())
            } else {
                MetadataError::from(e)
            }
        })?;

        for partition_id in 0..config.partition_count {
            sqlx::query(
                "INSERT INTO partitions (organization_id, topic, partition_id, high_watermark, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(default_org_id)
            .bind(&config.name)
            .bind(partition_id as i32)
            .bind(0i64)
            .bind(now_ms)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn delete_topic(&self, name: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM topics WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TopicNotFound(name.to_string()));
        }
        Ok(())
    }

    async fn get_topic(&self, name: &str) -> Result<Option<Topic>> {
        let row = sqlx::query(
            "SELECT name, partition_count, retention_ms, created_at, config
             FROM topics WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let config: std::collections::HashMap<String, String> =
                serde_json::from_value(r.get("config")).unwrap_or_default();
            let cleanup_policy = config
                .get("cleanup.policy")
                .map(|s| CleanupPolicy::from_str(s))
                .unwrap_or_default();

            Topic {
                name: r.get("name"),
                partition_count: r.get::<i32, _>("partition_count") as u32,
                retention_ms: r.get("retention_ms"),
                cleanup_policy,
                created_at: r.get("created_at"),
                config,
            }
        }))
    }

    async fn get_topic_organization_id(&self, topic: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT organization_id::text FROM topics WHERE name = $1")
            .bind(topic)
            .fetch_optional(&self.pool)
            .await?;

        use sqlx::Row;
        Ok(row.map(|r| r.get::<String, _>("organization_id")))
    }

    async fn list_topics(&self) -> Result<Vec<Topic>> {
        let rows = sqlx::query(
            "SELECT name, partition_count, retention_ms, created_at, config
             FROM topics ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let config: std::collections::HashMap<String, String> =
                    serde_json::from_value(r.get("config")).unwrap_or_default();
                let cleanup_policy = config
                    .get("cleanup.policy")
                    .map(|s| CleanupPolicy::from_str(s))
                    .unwrap_or_default();

                Topic {
                    name: r.get("name"),
                    partition_count: r.get::<i32, _>("partition_count") as u32,
                    retention_ms: r.get("retention_ms"),
                    cleanup_policy,
                    created_at: r.get("created_at"),
                    config,
                }
            })
            .collect())
    }

    async fn list_topics_for_org(&self, org_id: &str) -> Result<Vec<Topic>> {
        let rows = sqlx::query(
            "SELECT name, partition_count, retention_ms, created_at, config
             FROM topics WHERE organization_id = $1::UUID ORDER BY created_at DESC",
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let config: std::collections::HashMap<String, String> =
                    serde_json::from_value(r.get("config")).unwrap_or_default();
                let cleanup_policy = config
                    .get("cleanup.policy")
                    .map(|s| CleanupPolicy::from_str(s))
                    .unwrap_or_default();
                Topic {
                    name: r.get("name"),
                    partition_count: r.get::<i32, _>("partition_count") as u32,
                    retention_ms: r.get("retention_ms"),
                    cleanup_policy,
                    created_at: r.get("created_at"),
                    config,
                }
            })
            .collect())
    }

    async fn create_topic_for_org(&self, org_id: &str, config: TopicConfig) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut config_map = config.config.clone();
        config_map.insert(
            "cleanup.policy".to_string(),
            config.cleanup_policy.as_str().to_string(),
        );
        let config_json = serde_json::to_value(&config_map)?;

        sqlx::query(
            "INSERT INTO topics (organization_id, name, partition_count, retention_ms, created_at, updated_at, config)
             VALUES ($1::uuid, $2, $3, $4, $5, $6, $7)",
        )
        .bind(org_id)
        .bind(&config.name)
        .bind(config.partition_count as i32)
        .bind(config.retention_ms)
        .bind(now_ms)
        .bind(now_ms)
        .bind(&config_json)
        .execute(&mut *tx)
        .await?;

        for partition_id in 0..config.partition_count {
            sqlx::query(
                "INSERT INTO partitions (organization_id, topic, partition_id, high_watermark, created_at, updated_at)
                 VALUES ($1::uuid, $2, $3, 0, $4, $5)",
            )
            .bind(org_id)
            .bind(&config.name)
            .bind(partition_id as i32)
            .bind(now_ms)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn delete_topic_for_org(&self, org_id: &str, name: &str) -> Result<()> {
        let result =
            sqlx::query("DELETE FROM topics WHERE organization_id = $1::UUID AND name = $2")
                .bind(org_id)
                .bind(name)
                .execute(&self.pool)
                .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TopicNotFound(name.to_string()));
        }
        Ok(())
    }

    async fn get_topic_for_org(&self, org_id: &str, name: &str) -> Result<Option<Topic>> {
        let row = sqlx::query(
            "SELECT name, partition_count, retention_ms, created_at, config
             FROM topics WHERE organization_id = $1::UUID AND name = $2",
        )
        .bind(org_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let config: std::collections::HashMap<String, String> =
                serde_json::from_value(r.get("config")).unwrap_or_default();
            let cleanup_policy = config
                .get("cleanup.policy")
                .map(|s| CleanupPolicy::from_str(s))
                .unwrap_or_default();
            Topic {
                name: r.get("name"),
                partition_count: r.get::<i32, _>("partition_count") as u32,
                retention_ms: r.get("retention_ms"),
                cleanup_policy,
                created_at: r.get("created_at"),
                config,
            }
        }))
    }

    async fn ensure_organization(&self, org_id: &str, name: &str) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let slug = name.to_lowercase().replace(' ', "-");
        sqlx::query(
            "INSERT INTO organizations (id, name, slug, plan, status, created_at, updated_at)
             VALUES ($1::uuid, $2, $3, 'free', 'active', $4, $5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(org_id)
        .bind(name)
        .bind(&slug)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_partition(
        &self,
        org_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<Partition>> {
        let org_uuid = uuid::Uuid::parse_str(org_id).unwrap_or_else(|_| {
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        });
        let row = sqlx::query(
            "SELECT topic, partition_id, high_watermark
             FROM partitions WHERE organization_id = $1 AND topic = $2 AND partition_id = $3",
        )
        .bind(org_uuid)
        .bind(topic)
        .bind(partition_id as i32)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Partition {
            topic: r.get("topic"),
            partition_id: r.get::<i32, _>("partition_id") as u32,
            high_watermark: r.get::<i64, _>("high_watermark") as u64,
        }))
    }

    async fn update_high_watermark(
        &self,
        org_id: &str,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<()> {
        let org_uuid = uuid::Uuid::parse_str(org_id).unwrap_or_else(|_| {
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        });
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        sqlx::query(
            "UPDATE partitions SET high_watermark = $4, updated_at = $5
             WHERE organization_id = $1 AND topic = $2 AND partition_id = $3",
        )
        .bind(org_uuid)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(offset as i64)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_partitions(&self, org_id: &str, topic: &str) -> Result<Vec<Partition>> {
        let org_uuid = uuid::Uuid::parse_str(org_id).unwrap_or_else(|_| {
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        });
        let rows = sqlx::query(
            "SELECT topic, partition_id, high_watermark
             FROM partitions WHERE organization_id = $1 AND topic = $2 ORDER BY partition_id",
        )
        .bind(org_uuid)
        .bind(topic)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Partition {
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                high_watermark: r.get::<i64, _>("high_watermark") as u64,
            })
            .collect())
    }

    async fn add_segment(&self, org_id: &str, segment: SegmentInfo) -> Result<()> {
        let org_uuid = uuid::Uuid::parse_str(org_id).unwrap_or_else(|_| {
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        });

        sqlx::query(
            "INSERT INTO segments (id, organization_id, topic, partition_id, base_offset, end_offset,
                                   record_count, size_bytes, s3_bucket, s3_key, created_at, min_timestamp, max_timestamp)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(&segment.id)
        .bind(org_uuid)
        .bind(&segment.topic)
        .bind(segment.partition_id as i32)
        .bind(segment.base_offset as i64)
        .bind(segment.end_offset as i64)
        .bind(segment.record_count as i32)
        .bind(segment.size_bytes as i64)
        .bind(&segment.s3_bucket)
        .bind(&segment.s3_key)
        .bind(segment.created_at)
        .bind(segment.min_timestamp)
        .bind(segment.max_timestamp)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn add_segment_and_update_watermark(
        &self,
        org_id: &str,
        segment: SegmentInfo,
        high_watermark: u64,
    ) -> Result<()> {
        let org_uuid = uuid::Uuid::parse_str(org_id).unwrap_or_else(|_| {
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        });
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO segments (id, organization_id, topic, partition_id, base_offset, end_offset,
                                   record_count, size_bytes, s3_bucket, s3_key, created_at, min_timestamp, max_timestamp)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(&segment.id)
        .bind(org_uuid)
        .bind(&segment.topic)
        .bind(segment.partition_id as i32)
        .bind(segment.base_offset as i64)
        .bind(segment.end_offset as i64)
        .bind(segment.record_count as i32)
        .bind(segment.size_bytes as i64)
        .bind(&segment.s3_bucket)
        .bind(&segment.s3_key)
        .bind(segment.created_at)
        .bind(segment.min_timestamp)
        .bind(segment.max_timestamp)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE partitions SET high_watermark = $4, updated_at = $5
             WHERE organization_id = $1 AND topic = $2 AND partition_id = $3",
        )
        .bind(org_uuid)
        .bind(&segment.topic)
        .bind(segment.partition_id as i32)
        .bind(high_watermark as i64)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_segments(
        &self,
        org_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Vec<SegmentInfo>> {
        let org_uuid = uuid::Uuid::parse_str(org_id).unwrap_or_else(|_| {
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        });
        let rows = sqlx::query(
            "SELECT id, topic, partition_id, base_offset, end_offset, record_count,
                    size_bytes, s3_bucket, s3_key, created_at, min_timestamp, max_timestamp
             FROM segments WHERE organization_id = $1 AND topic = $2 AND partition_id = $3 ORDER BY base_offset",
        )
        .bind(org_uuid)
        .bind(topic)
        .bind(partition_id as i32)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SegmentInfo {
                id: r.get("id"),
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                base_offset: r.get::<i64, _>("base_offset") as u64,
                end_offset: r.get::<i64, _>("end_offset") as u64,
                record_count: r.get::<i32, _>("record_count") as u32,
                size_bytes: r.get::<i64, _>("size_bytes") as u64,
                s3_bucket: r.get("s3_bucket"),
                s3_key: r.get("s3_key"),
                created_at: r.get("created_at"),
                min_timestamp: r.get("min_timestamp"),
                max_timestamp: r.get("max_timestamp"),
            })
            .collect())
    }

    async fn find_segment_for_offset(
        &self,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<Option<SegmentInfo>> {
        let row = sqlx::query(
            "SELECT id, topic, partition_id, base_offset, end_offset, record_count,
                    size_bytes, s3_bucket, s3_key, created_at, min_timestamp, max_timestamp
             FROM segments
             WHERE topic = $1 AND partition_id = $2
               AND base_offset <= $3 AND end_offset >= $3
             ORDER BY base_offset LIMIT 1",
        )
        .bind(topic)
        .bind(partition_id as i32)
        .bind(offset as i64)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| SegmentInfo {
            id: r.get("id"),
            topic: r.get("topic"),
            partition_id: r.get::<i32, _>("partition_id") as u32,
            base_offset: r.get::<i64, _>("base_offset") as u64,
            end_offset: r.get::<i64, _>("end_offset") as u64,
            record_count: r.get::<i32, _>("record_count") as u32,
            size_bytes: r.get::<i64, _>("size_bytes") as u64,
            s3_bucket: r.get("s3_bucket"),
            s3_key: r.get("s3_key"),
            created_at: r.get("created_at"),
            min_timestamp: r.get("min_timestamp"),
            max_timestamp: r.get("max_timestamp"),
        }))
    }

    async fn delete_segments_before(
        &self,
        org_id: &str,
        topic: &str,
        partition_id: u32,
        before_offset: u64,
    ) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM segments
             WHERE organization_id = $1 AND topic = $2 AND partition_id = $3 AND end_offset < $4",
        )
        .bind(org_id)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(before_offset as i64)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn get_segment_storage_stats(&self) -> Result<Vec<TopicStorageStats>> {
        let rows = sqlx::query(
            "SELECT topic, COUNT(*)::BIGINT as segment_count, COALESCE(SUM(size_bytes), 0)::BIGINT as total_size_bytes
             FROM segments GROUP BY topic",
        )
        .fetch_all(&self.pool)
        .await?;

        use sqlx::Row;
        Ok(rows
            .iter()
            .map(|r| TopicStorageStats {
                topic: r.get::<String, _>("topic"),
                segment_count: r.get::<i64, _>("segment_count") as u64,
                total_size_bytes: r.get::<i64, _>("total_size_bytes") as u64,
            })
            .collect())
    }

    async fn get_segment_storage_stats_for_org(
        &self,
        org_id: &str,
    ) -> Result<Vec<TopicStorageStats>> {
        let rows = sqlx::query(
            "SELECT topic, COUNT(*)::BIGINT as segment_count, COALESCE(SUM(size_bytes), 0)::BIGINT as total_size_bytes
             FROM segments WHERE organization_id = $1::UUID GROUP BY topic",
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;

        use sqlx::Row;
        Ok(rows
            .iter()
            .map(|r| TopicStorageStats {
                topic: r.get::<String, _>("topic"),
                segment_count: r.get::<i64, _>("segment_count") as u64,
                total_size_bytes: r.get::<i64, _>("total_size_bytes") as u64,
            })
            .collect())
    }

    async fn ensure_consumer_group(&self, group_id: &str) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Use default organization for backwards compatibility
        let default_org_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap();

        sqlx::query(
            "INSERT INTO consumer_groups (organization_id, group_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4) ON CONFLICT (organization_id, group_id) DO NOTHING",
        )
        .bind(default_org_id)
        .bind(group_id)
        .bind(now_ms)
        .bind(now_ms)
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
        self.ensure_consumer_group(group_id).await?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Use default organization for backwards compatibility
        let default_org_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap();

        sqlx::query(
            "INSERT INTO consumer_offsets (organization_id, group_id, topic, partition_id, committed_offset, metadata, committed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (organization_id, group_id, topic, partition_id)
             DO UPDATE SET committed_offset = $5, metadata = $6, committed_at = $7"
        )
        .bind(default_org_id)
        .bind(group_id)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(offset as i64)
        .bind(metadata)
        .bind(now_ms)
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
        let row = sqlx::query(
            "SELECT committed_offset FROM consumer_offsets
             WHERE group_id = $1 AND topic = $2 AND partition_id = $3",
        )
        .bind(group_id)
        .bind(topic)
        .bind(partition_id as i32)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.get::<i64, _>("committed_offset") as u64))
    }

    async fn get_consumer_offsets(&self, group_id: &str) -> Result<Vec<ConsumerOffset>> {
        let rows = sqlx::query(
            "SELECT group_id, topic, partition_id, committed_offset, metadata
             FROM consumer_offsets WHERE group_id = $1 ORDER BY topic, partition_id",
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ConsumerOffset {
                group_id: r.get("group_id"),
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                committed_offset: r.get::<i64, _>("committed_offset") as u64,
                metadata: r.get("metadata"),
            })
            .collect())
    }

    async fn list_consumer_groups(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT DISTINCT group_id FROM consumer_offsets ORDER BY group_id")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.get("group_id")).collect())
    }

    async fn delete_consumer_group(&self, group_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM consumer_groups WHERE group_id = $1")
            .bind(group_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Org-scoped consumer group operations ──────────────────────

    async fn list_consumer_groups_for_org(&self, org_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT DISTINCT group_id FROM consumer_offsets WHERE organization_id = $1::UUID ORDER BY group_id",
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.get("group_id")).collect())
    }

    async fn ensure_consumer_group_for_org(&self, org_id: &str, group_id: &str) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        sqlx::query(
            "INSERT INTO consumer_groups (organization_id, group_id, created_at, updated_at)
             VALUES ($1::UUID, $2, $3, $4) ON CONFLICT (organization_id, group_id) DO NOTHING",
        )
        .bind(org_id)
        .bind(group_id)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn commit_offset_for_org(
        &self,
        org_id: &str,
        group_id: &str,
        topic: &str,
        partition_id: u32,
        offset: u64,
        metadata: Option<String>,
    ) -> Result<()> {
        self.ensure_consumer_group_for_org(org_id, group_id).await?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        sqlx::query(
            "INSERT INTO consumer_offsets (organization_id, group_id, topic, partition_id, committed_offset, metadata, committed_at)
             VALUES ($1::UUID, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (organization_id, group_id, topic, partition_id)
             DO UPDATE SET committed_offset = $5, metadata = $6, committed_at = $7",
        )
        .bind(org_id)
        .bind(group_id)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(offset as i64)
        .bind(metadata)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_committed_offset_for_org(
        &self,
        org_id: &str,
        group_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<u64>> {
        let row = sqlx::query(
            "SELECT committed_offset FROM consumer_offsets
             WHERE organization_id = $1::UUID AND group_id = $2 AND topic = $3 AND partition_id = $4",
        )
        .bind(org_id)
        .bind(group_id)
        .bind(topic)
        .bind(partition_id as i32)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.get::<i64, _>("committed_offset") as u64))
    }

    async fn get_consumer_offsets_for_org(
        &self,
        org_id: &str,
        group_id: &str,
    ) -> Result<Vec<ConsumerOffset>> {
        let rows = sqlx::query(
            "SELECT group_id, topic, partition_id, committed_offset, metadata
             FROM consumer_offsets WHERE organization_id = $1::UUID AND group_id = $2
             ORDER BY topic, partition_id",
        )
        .bind(org_id)
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ConsumerOffset {
                group_id: r.get("group_id"),
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                committed_offset: r.get::<i64, _>("committed_offset") as u64,
                metadata: r.get("metadata"),
            })
            .collect())
    }

    async fn delete_consumer_group_for_org(&self, org_id: &str, group_id: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM consumer_groups WHERE organization_id = $1::UUID AND group_id = $2",
        )
        .bind(org_id)
        .bind(group_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn register_agent(&self, agent: AgentInfo) -> Result<()> {
        let metadata_json = serde_json::to_value(&agent.metadata)?;

        // Remove any stale agent with the same address (e.g. previous instance with different PID)
        sqlx::query("DELETE FROM agents WHERE address = $1 AND agent_id != $2")
            .bind(&agent.address)
            .bind(&agent.agent_id)
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "INSERT INTO agents (agent_id, address, availability_zone, agent_group,
                                 last_heartbeat, started_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT(agent_id) DO UPDATE SET
                 address = $2, availability_zone = $3, agent_group = $4,
                 last_heartbeat = $5, metadata = $7",
        )
        .bind(&agent.agent_id)
        .bind(&agent.address)
        .bind(&agent.availability_zone)
        .bind(&agent.agent_group)
        .bind(agent.last_heartbeat)
        .bind(agent.started_at)
        .bind(metadata_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_agent(&self, agent_id: &str) -> Result<Option<AgentInfo>> {
        let row = sqlx::query(
            "SELECT agent_id, address, availability_zone, agent_group,
                    last_heartbeat, started_at, metadata
             FROM agents WHERE agent_id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_value(r.get("metadata")).unwrap_or_default();

            AgentInfo {
                agent_id: r.get("agent_id"),
                address: r.get("address"),
                availability_zone: r.get("availability_zone"),
                agent_group: r.get("agent_group"),
                last_heartbeat: r.get("last_heartbeat"),
                started_at: r.get("started_at"),
                metadata,
            }
        }))
    }

    async fn list_agents(
        &self,
        agent_group: Option<&str>,
        availability_zone: Option<&str>,
    ) -> Result<Vec<AgentInfo>> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let stale_threshold = now_ms - 60_000;

        let rows = match (agent_group, availability_zone) {
            (Some(group), Some(az)) => {
                sqlx::query(
                    "SELECT agent_id, address, availability_zone, agent_group,
                            last_heartbeat, started_at, metadata
                     FROM agents
                     WHERE agent_group = $1 AND availability_zone = $2 AND last_heartbeat > $3
                     ORDER BY agent_id",
                )
                .bind(group)
                .bind(az)
                .bind(stale_threshold)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(group), None) => {
                sqlx::query(
                    "SELECT agent_id, address, availability_zone, agent_group,
                            last_heartbeat, started_at, metadata
                     FROM agents
                     WHERE agent_group = $1 AND last_heartbeat > $2
                     ORDER BY agent_id",
                )
                .bind(group)
                .bind(stale_threshold)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(az)) => {
                sqlx::query(
                    "SELECT agent_id, address, availability_zone, agent_group,
                            last_heartbeat, started_at, metadata
                     FROM agents
                     WHERE availability_zone = $1 AND last_heartbeat > $2
                     ORDER BY agent_id",
                )
                .bind(az)
                .bind(stale_threshold)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None) => {
                sqlx::query(
                    "SELECT agent_id, address, availability_zone, agent_group,
                            last_heartbeat, started_at, metadata
                     FROM agents
                     WHERE last_heartbeat > $1
                     ORDER BY agent_id",
                )
                .bind(stale_threshold)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows
            .into_iter()
            .map(|r| {
                let metadata: std::collections::HashMap<String, String> =
                    serde_json::from_value(r.get("metadata")).unwrap_or_default();

                AgentInfo {
                    agent_id: r.get("agent_id"),
                    address: r.get("address"),
                    availability_zone: r.get("availability_zone"),
                    agent_group: r.get("agent_group"),
                    last_heartbeat: r.get("last_heartbeat"),
                    started_at: r.get("started_at"),
                    metadata,
                }
            })
            .collect())
    }

    async fn deregister_agent(&self, agent_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM agents WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn acquire_partition_lease(
        &self,
        organization_id: &str,
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

        let org_uuid = uuid::Uuid::parse_str(organization_id)
            .unwrap_or_else(|_| uuid::Uuid::parse_str(crate::DEFAULT_ORGANIZATION_ID).unwrap());

        sqlx::query(
            "INSERT INTO partition_leases (organization_id, topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch)
             VALUES ($1, $2, $3, $4, $5, $6, 1)
             ON CONFLICT (topic, partition_id) DO UPDATE SET
                 leader_agent_id = CASE
                     WHEN partition_leases.leader_agent_id = $4 OR partition_leases.lease_expires_at < $7
                     THEN $4 ELSE partition_leases.leader_agent_id
                 END,
                 lease_expires_at = CASE
                     WHEN partition_leases.leader_agent_id = $4 OR partition_leases.lease_expires_at < $7
                     THEN $5 ELSE partition_leases.lease_expires_at
                 END,
                 acquired_at = CASE
                     WHEN partition_leases.leader_agent_id = $4 OR partition_leases.lease_expires_at < $7
                     THEN $6 ELSE partition_leases.acquired_at
                 END,
                 epoch = CASE
                     WHEN partition_leases.leader_agent_id = $4 OR partition_leases.lease_expires_at < $7
                     THEN partition_leases.epoch + 1 ELSE partition_leases.epoch
                 END"
        )
        .bind(org_uuid)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(agent_id)
        .bind(expires_at)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;

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

        let row = sqlx::query(
            "SELECT organization_id, topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
             FROM partition_leases
             WHERE topic = $1 AND partition_id = $2 AND lease_expires_at > $3",
        )
        .bind(topic)
        .bind(partition_id as i32)
        .bind(now_ms)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| PartitionLease {
            organization_id: r
                .try_get::<uuid::Uuid, _>("organization_id")
                .map(|u| u.to_string())
                .unwrap_or_else(|_| crate::DEFAULT_ORGANIZATION_ID.to_string()),
            topic: r.get("topic"),
            partition_id: r.get::<i32, _>("partition_id") as u32,
            leader_agent_id: r.get("leader_agent_id"),
            lease_expires_at: r.get("lease_expires_at"),
            acquired_at: r.get("acquired_at"),
            epoch: r.get::<i64, _>("epoch") as u64,
        }))
    }

    async fn release_partition_lease(
        &self,
        topic: &str,
        partition_id: u32,
        agent_id: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "DELETE FROM partition_leases
             WHERE topic = $1 AND partition_id = $2 AND leader_agent_id = $3",
        )
        .bind(topic)
        .bind(partition_id as i32)
        .bind(agent_id)
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

        let rows = match (topic, agent_id) {
            (Some(t), Some(a)) => sqlx::query(
                "SELECT organization_id, topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
                     FROM partition_leases
                     WHERE topic = $1 AND leader_agent_id = $2 AND lease_expires_at > $3
                     ORDER BY topic, partition_id",
            )
            .bind(t)
            .bind(a)
            .bind(now_ms)
            .fetch_all(&self.pool)
            .await?,
            (Some(t), None) => sqlx::query(
                "SELECT organization_id, topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
                     FROM partition_leases
                     WHERE topic = $1 AND lease_expires_at > $2
                     ORDER BY topic, partition_id",
            )
            .bind(t)
            .bind(now_ms)
            .fetch_all(&self.pool)
            .await?,
            (None, Some(a)) => sqlx::query(
                "SELECT organization_id, topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
                     FROM partition_leases
                     WHERE leader_agent_id = $1 AND lease_expires_at > $2
                     ORDER BY topic, partition_id",
            )
            .bind(a)
            .bind(now_ms)
            .fetch_all(&self.pool)
            .await?,
            (None, None) => sqlx::query(
                "SELECT organization_id, topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
                     FROM partition_leases
                     WHERE lease_expires_at > $1
                     ORDER BY topic, partition_id",
            )
            .bind(now_ms)
            .fetch_all(&self.pool)
            .await?,
        };

        Ok(rows
            .into_iter()
            .map(|r| PartitionLease {
                organization_id: r
                    .try_get::<uuid::Uuid, _>("organization_id")
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| crate::DEFAULT_ORGANIZATION_ID.to_string()),
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                leader_agent_id: r.get("leader_agent_id"),
                lease_expires_at: r.get("lease_expires_at"),
                acquired_at: r.get("acquired_at"),
                epoch: r.get::<i64, _>("epoch") as u64,
            })
            .collect())
    }

    // ============================================================
    // ORGANIZATION OPERATIONS (Phase 21.5: Multi-Tenancy)
    // ============================================================

    async fn create_organization(&self, config: CreateOrganization) -> Result<Organization> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let id = uuid::Uuid::new_v4().to_string();
        let plan_str = config.plan.to_string();
        let status_str = OrganizationStatus::Active.to_string();
        let settings_json = serde_json::to_value(&config.settings)?;

        sqlx::query(
            "INSERT INTO organizations (id, name, slug, plan, status, created_at, updated_at, settings, clerk_id)
             VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(&id)
        .bind(&config.name)
        .bind(&config.slug)
        .bind(&plan_str)
        .bind(&status_str)
        .bind(now_ms)
        .bind(now_ms)
        .bind(&settings_json)
        .bind(&config.clerk_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
                MetadataError::ConflictError(format!("Organization with slug '{}' already exists", config.slug))
            } else {
                MetadataError::from(e)
            }
        })?;

        Ok(Organization {
            id,
            name: config.name,
            slug: config.slug,
            plan: config.plan,
            status: OrganizationStatus::Active,
            created_at: now_ms,
            settings: config.settings,
            clerk_id: config.clerk_id,
        })
    }

    async fn get_organization(&self, id: &str) -> Result<Option<Organization>> {
        let row = sqlx::query(
            "SELECT id::text, name, slug, plan, status, created_at, settings, clerk_id
             FROM organizations WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let settings: std::collections::HashMap<String, String> =
                serde_json::from_value(r.get("settings")).unwrap_or_default();
            let plan: String = r.get("plan");
            let status: String = r.get("status");
            Organization {
                id: r.get("id"),
                name: r.get("name"),
                slug: r.get("slug"),
                plan: plan.parse().unwrap_or_default(),
                status: status.parse().unwrap_or_default(),
                created_at: r.get("created_at"),
                settings,
                clerk_id: r.get("clerk_id"),
            }
        }))
    }

    async fn get_organization_by_slug(&self, slug: &str) -> Result<Option<Organization>> {
        let row = sqlx::query(
            "SELECT id::text, name, slug, plan, status, created_at, settings, clerk_id
             FROM organizations WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let settings: std::collections::HashMap<String, String> =
                serde_json::from_value(r.get("settings")).unwrap_or_default();
            let plan: String = r.get("plan");
            let status: String = r.get("status");
            Organization {
                id: r.get("id"),
                name: r.get("name"),
                slug: r.get("slug"),
                plan: plan.parse().unwrap_or_default(),
                status: status.parse().unwrap_or_default(),
                created_at: r.get("created_at"),
                settings,
                clerk_id: r.get("clerk_id"),
            }
        }))
    }

    async fn get_organization_by_clerk_id(&self, clerk_id: &str) -> Result<Option<Organization>> {
        let row = sqlx::query(
            "SELECT id::text, name, slug, plan, status, created_at, settings, clerk_id
             FROM organizations WHERE clerk_id = $1",
        )
        .bind(clerk_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let settings: std::collections::HashMap<String, String> =
                serde_json::from_value(r.get("settings")).unwrap_or_default();
            let plan: String = r.get("plan");
            let status: String = r.get("status");
            Organization {
                id: r.get("id"),
                name: r.get("name"),
                slug: r.get("slug"),
                plan: plan.parse().unwrap_or_default(),
                status: status.parse().unwrap_or_default(),
                created_at: r.get("created_at"),
                settings,
                clerk_id: r.get("clerk_id"),
            }
        }))
    }

    async fn list_organizations(&self) -> Result<Vec<Organization>> {
        let rows = sqlx::query(
            "SELECT id::text, name, slug, plan, status, created_at, settings, clerk_id
             FROM organizations ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let settings: std::collections::HashMap<String, String> =
                    serde_json::from_value(r.get("settings")).unwrap_or_default();
                let plan: String = r.get("plan");
                let status: String = r.get("status");
                Organization {
                    id: r.get("id"),
                    name: r.get("name"),
                    slug: r.get("slug"),
                    plan: plan.parse().unwrap_or_default(),
                    status: status.parse().unwrap_or_default(),
                    created_at: r.get("created_at"),
                    settings,
                    clerk_id: r.get("clerk_id"),
                }
            })
            .collect())
    }

    async fn update_organization_status(&self, id: &str, status: OrganizationStatus) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let status_str = status.to_string();

        let result = sqlx::query(
            "UPDATE organizations SET status = $1, updated_at = $2 WHERE id = $3::uuid",
        )
        .bind(&status_str)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::NotFoundError(format!(
                "Organization {} not found",
                id
            )));
        }
        Ok(())
    }

    async fn update_organization_plan(&self, id: &str, plan: OrganizationPlan) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let plan_str = plan.to_string();

        let result =
            sqlx::query("UPDATE organizations SET plan = $1, updated_at = $2 WHERE id = $3::uuid")
                .bind(&plan_str)
                .bind(now_ms)
                .bind(id)
                .execute(&self.pool)
                .await?;

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
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let id_uuid = uuid::Uuid::new_v4();
        let id = id_uuid.to_string();
        let org_uuid = uuid::Uuid::parse_str(organization_id).unwrap_or_else(|_| {
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        });
        let expires_at = config.expires_in_ms.map(|ms| now_ms + ms);
        let now_dt = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_ms)
            .unwrap_or_else(chrono::Utc::now);
        let expires_at_dt =
            expires_at.and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis);
        let permissions_json = serde_json::to_value(&config.permissions)?;
        let scopes_json = serde_json::to_value(&config.scopes)?;

        sqlx::query(
            "INSERT INTO api_keys (id, organization_id, name, key_hash, key_prefix, permissions, scopes, expires_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(id_uuid)
        .bind(org_uuid)
        .bind(&config.name)
        .bind(key_hash)
        .bind(key_prefix)
        .bind(&permissions_json)
        .bind(&scopes_json)
        .bind(expires_at_dt)
        .bind(now_dt)
        .execute(&self.pool)
        .await?;

        Ok(ApiKey {
            id,
            organization_id: organization_id.to_string(),
            name: config.name,
            key_prefix: key_prefix.to_string(),
            permissions: config.permissions,
            scopes: config.scopes,
            expires_at,
            last_used_at: None,
            created_at: now_ms,
            created_by: None,
            max_requests_per_sec: None,
            max_produce_bytes_per_sec: None,
            max_consume_bytes_per_sec: None,
        })
    }

    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>> {
        let row = sqlx::query(
            "SELECT id::TEXT, organization_id::TEXT, name, key_prefix, permissions, scopes,
                    EXTRACT(EPOCH FROM expires_at)::BIGINT * 1000 as expires_at_ms,
                    EXTRACT(EPOCH FROM last_used_at)::BIGINT * 1000 as last_used_at_ms,
                    EXTRACT(EPOCH FROM created_at)::BIGINT * 1000 as created_at_ms,
                    created_by,
                    max_requests_per_sec, max_produce_bytes_per_sec, max_consume_bytes_per_sec
             FROM api_keys WHERE id = $1::UUID",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let permissions: Vec<String> =
                serde_json::from_value(r.get("permissions")).unwrap_or_default();
            let scopes: Vec<String> = serde_json::from_value(r.get("scopes")).unwrap_or_default();
            ApiKey {
                id: r.get("id"),
                organization_id: r.get("organization_id"),
                name: r.get("name"),
                key_prefix: r.get("key_prefix"),
                permissions,
                scopes,
                expires_at: r.get("expires_at_ms"),
                last_used_at: r.get("last_used_at_ms"),
                created_at: r.get::<i64, _>("created_at_ms"),
                created_by: r.get("created_by"),
                max_requests_per_sec: r.get("max_requests_per_sec"),
                max_produce_bytes_per_sec: r.get("max_produce_bytes_per_sec"),
                max_consume_bytes_per_sec: r.get("max_consume_bytes_per_sec"),
            }
        }))
    }

    async fn validate_api_key(&self, key_hash: &str) -> Result<Option<ApiKey>> {
        let row = sqlx::query(
            "SELECT id::TEXT, organization_id::TEXT, name, key_prefix, permissions, scopes,
                    EXTRACT(EPOCH FROM expires_at)::BIGINT * 1000 as expires_at_ms,
                    EXTRACT(EPOCH FROM last_used_at)::BIGINT * 1000 as last_used_at_ms,
                    EXTRACT(EPOCH FROM created_at)::BIGINT * 1000 as created_at_ms,
                    created_by,
                    max_requests_per_sec, max_produce_bytes_per_sec, max_consume_bytes_per_sec
             FROM api_keys WHERE key_hash = $1 AND (expires_at IS NULL OR expires_at > NOW())",
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let permissions: Vec<String> =
                serde_json::from_value(r.get("permissions")).unwrap_or_default();
            let scopes: Vec<String> = serde_json::from_value(r.get("scopes")).unwrap_or_default();
            ApiKey {
                id: r.get("id"),
                organization_id: r.get("organization_id"),
                name: r.get("name"),
                key_prefix: r.get("key_prefix"),
                permissions,
                scopes,
                expires_at: r.get("expires_at_ms"),
                last_used_at: r.get("last_used_at_ms"),
                created_at: r.get::<i64, _>("created_at_ms"),
                created_by: r.get("created_by"),
                max_requests_per_sec: r.get("max_requests_per_sec"),
                max_produce_bytes_per_sec: r.get("max_produce_bytes_per_sec"),
                max_consume_bytes_per_sec: r.get("max_consume_bytes_per_sec"),
            }
        }))
    }

    async fn list_api_keys(&self, organization_id: &str) -> Result<Vec<ApiKey>> {
        let rows = sqlx::query(
            "SELECT id::TEXT, organization_id::TEXT, name, key_prefix, permissions, scopes,
                    EXTRACT(EPOCH FROM expires_at)::BIGINT * 1000 as expires_at_ms,
                    EXTRACT(EPOCH FROM last_used_at)::BIGINT * 1000 as last_used_at_ms,
                    EXTRACT(EPOCH FROM created_at)::BIGINT * 1000 as created_at_ms,
                    created_by,
                    max_requests_per_sec, max_produce_bytes_per_sec, max_consume_bytes_per_sec
             FROM api_keys WHERE organization_id = $1::UUID ORDER BY created_at DESC",
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let permissions: Vec<String> =
                    serde_json::from_value(r.get("permissions")).unwrap_or_default();
                let scopes: Vec<String> =
                    serde_json::from_value(r.get("scopes")).unwrap_or_default();
                ApiKey {
                    id: r.get("id"),
                    organization_id: r.get("organization_id"),
                    name: r.get("name"),
                    key_prefix: r.get("key_prefix"),
                    permissions,
                    scopes,
                    expires_at: r.get("expires_at_ms"),
                    last_used_at: r.get("last_used_at_ms"),
                    created_at: r.get::<i64, _>("created_at_ms"),
                    created_by: r.get("created_by"),
                    max_requests_per_sec: r.get("max_requests_per_sec"),
                    max_produce_bytes_per_sec: r.get("max_produce_bytes_per_sec"),
                    max_consume_bytes_per_sec: r.get("max_consume_bytes_per_sec"),
                }
            })
            .collect())
    }

    async fn touch_api_key(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1::UUID")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn revoke_api_key(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM api_keys WHERE id = $1::UUID")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ============================================================
    // QUOTA OPERATIONS (Phase 21.5: Multi-Tenancy)
    // ============================================================

    async fn get_organization_quota(&self, organization_id: &str) -> Result<OrganizationQuota> {
        let row = sqlx::query(
            "SELECT organization_id::TEXT, max_topics, max_partitions_per_topic, max_total_partitions,
                    max_storage_bytes, max_retention_days, max_produce_bytes_per_sec, max_consume_bytes_per_sec,
                    max_requests_per_sec, max_consumer_groups, max_schemas, max_schema_versions_per_subject, max_connections
             FROM organization_quotas WHERE organization_id = $1::UUID"
        )
        .bind(organization_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row
            .map(|r| OrganizationQuota {
                organization_id: r.get("organization_id"),
                max_topics: r.get("max_topics"),
                max_partitions_per_topic: r.get("max_partitions_per_topic"),
                max_total_partitions: r.get("max_total_partitions"),
                max_storage_bytes: r.get("max_storage_bytes"),
                max_retention_days: r.get("max_retention_days"),
                max_produce_bytes_per_sec: r.get("max_produce_bytes_per_sec"),
                max_consume_bytes_per_sec: r.get("max_consume_bytes_per_sec"),
                max_requests_per_sec: r.get("max_requests_per_sec"),
                max_consumer_groups: r.get("max_consumer_groups"),
                max_schemas: r.get("max_schemas"),
                max_schema_versions_per_subject: r.get("max_schema_versions_per_subject"),
                max_connections: r.get("max_connections"),
            })
            .unwrap_or_else(|| OrganizationQuota {
                organization_id: organization_id.to_string(),
                ..Default::default()
            }))
    }

    async fn set_organization_quota(&self, quota: OrganizationQuota) -> Result<()> {
        sqlx::query(
            "INSERT INTO organization_quotas (
                organization_id, max_topics, max_partitions_per_topic, max_total_partitions,
                max_storage_bytes, max_retention_days, max_produce_bytes_per_sec, max_consume_bytes_per_sec,
                max_requests_per_sec, max_consumer_groups, max_schemas, max_schema_versions_per_subject, max_connections
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (organization_id) DO UPDATE SET
                max_topics = EXCLUDED.max_topics,
                max_partitions_per_topic = EXCLUDED.max_partitions_per_topic,
                max_total_partitions = EXCLUDED.max_total_partitions,
                max_storage_bytes = EXCLUDED.max_storage_bytes,
                max_retention_days = EXCLUDED.max_retention_days,
                max_produce_bytes_per_sec = EXCLUDED.max_produce_bytes_per_sec,
                max_consume_bytes_per_sec = EXCLUDED.max_consume_bytes_per_sec,
                max_requests_per_sec = EXCLUDED.max_requests_per_sec,
                max_consumer_groups = EXCLUDED.max_consumer_groups,
                max_schemas = EXCLUDED.max_schemas,
                max_schema_versions_per_subject = EXCLUDED.max_schema_versions_per_subject,
                max_connections = EXCLUDED.max_connections"
        )
        .bind(&quota.organization_id)
        .bind(quota.max_topics)
        .bind(quota.max_partitions_per_topic)
        .bind(quota.max_total_partitions)
        .bind(quota.max_storage_bytes)
        .bind(quota.max_retention_days)
        .bind(quota.max_produce_bytes_per_sec)
        .bind(quota.max_consume_bytes_per_sec)
        .bind(quota.max_requests_per_sec)
        .bind(quota.max_consumer_groups)
        .bind(quota.max_schemas)
        .bind(quota.max_schema_versions_per_subject)
        .bind(quota.max_connections)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_organization_usage(
        &self,
        organization_id: &str,
    ) -> Result<Vec<OrganizationUsage>> {
        let rows = sqlx::query(
            "SELECT organization_id::TEXT, metric, value, period_start
             FROM organization_usage WHERE organization_id = $1::UUID ORDER BY metric",
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| OrganizationUsage {
                organization_id: r.get("organization_id"),
                metric: r.get("metric"),
                value: r.get("value"),
                period_start: r.get("period_start"),
            })
            .collect())
    }

    async fn update_organization_usage(
        &self,
        organization_id: &str,
        metric: &str,
        value: i64,
    ) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        sqlx::query(
            "INSERT INTO organization_usage (organization_id, metric, value, period_start, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (organization_id, metric) DO UPDATE SET
                value = EXCLUDED.value,
                updated_at = EXCLUDED.updated_at"
        )
        .bind(organization_id)
        .bind(metric)
        .bind(value)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn increment_organization_usage(
        &self,
        organization_id: &str,
        metric: &str,
        delta: i64,
    ) -> Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        sqlx::query(
            "INSERT INTO organization_usage (organization_id, metric, value, period_start, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (organization_id, metric) DO UPDATE SET
                value = organization_usage.value + EXCLUDED.value,
                updated_at = EXCLUDED.updated_at"
        )
        .bind(organization_id)
        .bind(metric)
        .bind(delta)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ============================================================
    // FAST LEADER HANDOFF (Phase 17)
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
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let transfer_id = uuid::Uuid::new_v4().to_string();
        let timeout_at = now_ms + timeout_ms as i64;

        // Get current lease epoch
        let lease = self
            .get_partition_lease(topic, partition_id)
            .await?
            .ok_or_else(|| {
                MetadataError::NotFoundError(format!("No lease for {}/{}", topic, partition_id))
            })?;

        if lease.leader_agent_id != from_agent_id {
            return Err(MetadataError::ConflictError(format!(
                "Agent {} is not the current leader",
                from_agent_id
            )));
        }

        let reason_str = reason.to_string();
        sqlx::query(
            "INSERT INTO lease_transfers (transfer_id, topic, partition_id, from_agent_id, to_agent_id, from_epoch, state, reason, initiated_at, timeout_at)
             VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, $9)"
        )
        .bind(&transfer_id)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(from_agent_id)
        .bind(to_agent_id)
        .bind(lease.epoch as i64)
        .bind(&reason_str)
        .bind(now_ms)
        .bind(timeout_at)
        .execute(&self.pool)
        .await?;

        Ok(LeaseTransfer {
            transfer_id,
            topic: topic.to_string(),
            partition_id,
            from_agent_id: from_agent_id.to_string(),
            to_agent_id: to_agent_id.to_string(),
            from_epoch: lease.epoch,
            state: LeaseTransferState::Pending,
            reason,
            initiated_at: now_ms,
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
        let result = sqlx::query(
            "UPDATE lease_transfers SET state = 'accepted'
             WHERE transfer_id = $1 AND to_agent_id = $2 AND state = 'pending'",
        )
        .bind(transfer_id)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::NotFoundError(format!(
                "Transfer {} not found or not pending for agent {}",
                transfer_id, agent_id
            )));
        }

        self.get_lease_transfer(transfer_id).await?.ok_or_else(|| {
            MetadataError::NotFoundError(format!("Transfer {} not found", transfer_id))
        })
    }

    async fn complete_lease_transfer(
        &self,
        transfer_id: &str,
        last_flushed_offset: u64,
        high_watermark: u64,
    ) -> Result<PartitionLease> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let transfer = self.get_lease_transfer(transfer_id).await?.ok_or_else(|| {
            MetadataError::NotFoundError(format!("Transfer {} not found", transfer_id))
        })?;

        if transfer.state != LeaseTransferState::Accepted {
            return Err(MetadataError::ConflictError(format!(
                "Transfer {} is not in accepted state",
                transfer_id
            )));
        }

        // Update transfer state
        sqlx::query(
            "UPDATE lease_transfers SET state = 'completed', completed_at = $1, last_flushed_offset = $2, high_watermark = $3
             WHERE transfer_id = $4"
        )
        .bind(now_ms)
        .bind(last_flushed_offset as i64)
        .bind(high_watermark as i64)
        .bind(transfer_id)
        .execute(&self.pool)
        .await?;

        // Transfer the lease to the new agent
        let new_lease = self
            .acquire_partition_lease(
                crate::DEFAULT_ORGANIZATION_ID,
                &transfer.topic,
                transfer.partition_id,
                &transfer.to_agent_id,
                30000, // Default lease duration
            )
            .await?;

        Ok(new_lease)
    }

    async fn reject_lease_transfer(
        &self,
        transfer_id: &str,
        agent_id: &str,
        reason: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE lease_transfers SET state = 'rejected', error = $1
             WHERE transfer_id = $2 AND (from_agent_id = $3 OR to_agent_id = $3) AND state IN ('pending', 'accepted')"
        )
        .bind(reason)
        .bind(transfer_id)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::NotFoundError(format!(
                "Transfer {} not found or not in rejectable state",
                transfer_id
            )));
        }
        Ok(())
    }

    async fn get_lease_transfer(&self, transfer_id: &str) -> Result<Option<LeaseTransfer>> {
        let row = sqlx::query(
            "SELECT transfer_id, topic, partition_id, from_agent_id, to_agent_id, from_epoch, state, reason, initiated_at, completed_at, timeout_at, last_flushed_offset, high_watermark, error
             FROM lease_transfers WHERE transfer_id = $1"
        )
        .bind(transfer_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let state_str: String = r.get("state");
            let reason_str: String = r.get("reason");
            LeaseTransfer {
                transfer_id: r.get("transfer_id"),
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                from_agent_id: r.get("from_agent_id"),
                to_agent_id: r.get("to_agent_id"),
                from_epoch: r.get::<i64, _>("from_epoch") as u64,
                state: state_str.parse().unwrap_or(LeaseTransferState::Pending),
                reason: reason_str
                    .parse()
                    .unwrap_or(LeaderChangeReason::GracefulHandoff),
                initiated_at: r.get("initiated_at"),
                completed_at: r.get("completed_at"),
                timeout_at: r.get("timeout_at"),
                last_flushed_offset: r
                    .get::<Option<i64>, _>("last_flushed_offset")
                    .map(|v| v as u64),
                high_watermark: r.get::<Option<i64>, _>("high_watermark").map(|v| v as u64),
                error: r.get("error"),
            }
        }))
    }

    async fn get_pending_transfers_for_agent(&self, agent_id: &str) -> Result<Vec<LeaseTransfer>> {
        let rows = sqlx::query(
            "SELECT transfer_id, topic, partition_id, from_agent_id, to_agent_id, from_epoch, state, reason, initiated_at, completed_at, timeout_at, last_flushed_offset, high_watermark, error
             FROM lease_transfers
             WHERE (from_agent_id = $1 OR to_agent_id = $1) AND state IN ('pending', 'accepted')
             ORDER BY initiated_at"
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let state_str: String = r.get("state");
                let reason_str: String = r.get("reason");
                LeaseTransfer {
                    transfer_id: r.get("transfer_id"),
                    topic: r.get("topic"),
                    partition_id: r.get::<i32, _>("partition_id") as u32,
                    from_agent_id: r.get("from_agent_id"),
                    to_agent_id: r.get("to_agent_id"),
                    from_epoch: r.get::<i64, _>("from_epoch") as u64,
                    state: state_str.parse().unwrap_or(LeaseTransferState::Pending),
                    reason: reason_str
                        .parse()
                        .unwrap_or(LeaderChangeReason::GracefulHandoff),
                    initiated_at: r.get("initiated_at"),
                    completed_at: r.get("completed_at"),
                    timeout_at: r.get("timeout_at"),
                    last_flushed_offset: r
                        .get::<Option<i64>, _>("last_flushed_offset")
                        .map(|v| v as u64),
                    high_watermark: r.get::<Option<i64>, _>("high_watermark").map(|v| v as u64),
                    error: r.get("error"),
                }
            })
            .collect())
    }

    async fn cleanup_timed_out_transfers(&self) -> Result<u64> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let result = sqlx::query(
            "UPDATE lease_transfers SET state = 'timed_out', error = 'Transfer timed out'
             WHERE state IN ('pending', 'accepted') AND timeout_at < $1",
        )
        .bind(now_ms)
        .execute(&self.pool)
        .await?;

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
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let reason_str = reason.to_string();

        sqlx::query(
            "INSERT INTO leader_changes (topic, partition_id, from_agent_id, to_agent_id, reason, epoch, gap_ms, changed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(topic)
        .bind(partition_id as i32)
        .bind(from_agent_id)
        .bind(to_agent_id)
        .bind(&reason_str)
        .bind(epoch as i64)
        .bind(gap_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============================================================
    // EXACTLY-ONCE SEMANTICS (Phase 16)
    // ============================================================

    // ---- Producer Operations ----

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
                sqlx::query(
                    "UPDATE producers SET epoch = $1, last_heartbeat = $2, state = 'active' WHERE id = $3"
                )
                .bind(new_epoch as i32)
                .bind(now)
                .bind(&producer.id)
                .execute(&self.pool)
                .await?;

                producer.epoch = new_epoch;
                producer.last_heartbeat = now;
                producer.state = ProducerState::Active;
                return Ok(producer);
            }
        }

        // Create new producer
        let producer_id = uuid::Uuid::new_v4().to_string();
        let metadata_json = serde_json::to_string(&config.metadata)?;

        sqlx::query(
            "INSERT INTO producers (id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata) \
             VALUES ($1, $2, $3, 0, $4, $5, 'active', $6)"
        )
        .bind(&producer_id)
        .bind(&config.organization_id)
        .bind(&config.transactional_id)
        .bind(now)
        .bind(now)
        .bind(&metadata_json)
        .execute(&self.pool)
        .await?;

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
        let row = sqlx::query(
            "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
             FROM producers WHERE id = $1"
        )
        .bind(producer_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let state_str: String = r.get("state");
            let metadata_json: Option<String> = r.get("metadata");
            Producer {
                id: r.get("id"),
                organization_id: r.get("organization_id"),
                transactional_id: r.get("transactional_id"),
                epoch: r.get::<i32, _>("epoch") as u32,
                state: state_str.parse().unwrap_or(ProducerState::Active),
                created_at: r.get("created_at"),
                last_heartbeat: r.get("last_heartbeat"),
                metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
                numeric_id: r.get("numeric_id"),
            }
        }))
    }

    async fn get_producer_by_transactional_id(
        &self,
        transactional_id: &str,
        organization_id: Option<&str>,
    ) -> Result<Option<Producer>> {
        let row = match organization_id {
            Some(org_id) => sqlx::query(
                "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
                 FROM producers WHERE transactional_id = $1 AND organization_id = $2::UUID"
            )
            .bind(transactional_id)
            .bind(org_id)
            .fetch_optional(&self.pool)
            .await?,
            None => sqlx::query(
                "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
                 FROM producers WHERE transactional_id = $1 AND organization_id IS NULL"
            )
            .bind(transactional_id)
            .fetch_optional(&self.pool)
            .await?,
        };

        Ok(row.map(|r| {
            let state_str: String = r.get("state");
            let metadata_json: Option<String> = r.get("metadata");
            Producer {
                id: r.get("id"),
                organization_id: r.get("organization_id"),
                transactional_id: r.get("transactional_id"),
                epoch: r.get::<i32, _>("epoch") as u32,
                state: state_str.parse().unwrap_or(ProducerState::Active),
                created_at: r.get("created_at"),
                last_heartbeat: r.get("last_heartbeat"),
                metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
                numeric_id: r.get("numeric_id"),
            }
        }))
    }

    async fn update_producer_heartbeat(&self, producer_id: &str) -> Result<()> {
        let now = Self::now_ms();
        sqlx::query("UPDATE producers SET last_heartbeat = $1 WHERE id = $2")
            .bind(now)
            .bind(producer_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn fence_producer(&self, producer_id: &str) -> Result<()> {
        let result = sqlx::query("UPDATE producers SET state = 'fenced' WHERE id = $1")
            .bind(producer_id)
            .execute(&self.pool)
            .await?;

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
        let result =
            sqlx::query("DELETE FROM producers WHERE last_heartbeat < $1 AND state != 'active'")
                .bind(cutoff)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    async fn allocate_numeric_producer_id(&self, producer_id: &str) -> Result<i64> {
        // Atomically get and increment the sequence, returning the allocated ID
        let row = sqlx::query(
            "UPDATE producer_id_sequence SET next_id = next_id + 1 WHERE id = 1 RETURNING next_id - 1 AS allocated_id"
        )
        .fetch_one(&self.pool)
        .await?;
        let numeric_id: i64 = row.get("allocated_id");

        // Set the numeric_id on the producer
        sqlx::query("UPDATE producers SET numeric_id = $1 WHERE id = $2")
            .bind(numeric_id)
            .bind(producer_id)
            .execute(&self.pool)
            .await?;

        Ok(numeric_id)
    }

    async fn get_producer_by_numeric_id(&self, numeric_id: i64) -> Result<Option<Producer>> {
        let row = sqlx::query(
            "SELECT id, organization_id, transactional_id, epoch, created_at, last_heartbeat, state, metadata, numeric_id \
             FROM producers WHERE numeric_id = $1"
        )
        .bind(numeric_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let state_str: String = r.get("state");
            let metadata_json: Option<String> = r.get("metadata");
            Producer {
                id: r.get("id"),
                organization_id: r.get("organization_id"),
                transactional_id: r.get("transactional_id"),
                epoch: r.get::<i32, _>("epoch") as u32,
                state: state_str.parse().unwrap_or(ProducerState::Active),
                created_at: r.get("created_at"),
                last_heartbeat: r.get("last_heartbeat"),
                metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
                numeric_id: r.get("numeric_id"),
            }
        }))
    }

    // ---- Sequence Operations ----

    async fn get_producer_sequence(
        &self,
        producer_id: &str,
        topic: &str,
        partition_id: u32,
    ) -> Result<Option<i64>> {
        let row = sqlx::query(
            "SELECT last_sequence FROM producer_sequences \
             WHERE producer_id = $1 AND topic = $2 AND partition_id = $3",
        )
        .bind(producer_id)
        .bind(topic)
        .bind(partition_id as i32)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.get::<i64, _>("last_sequence")))
    }

    async fn update_producer_sequence(
        &self,
        producer_id: &str,
        topic: &str,
        partition_id: u32,
        sequence: i64,
    ) -> Result<()> {
        let now = Self::now_ms();
        sqlx::query(
            "INSERT INTO producer_sequences (producer_id, topic, partition_id, last_sequence, updated_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT(producer_id, topic, partition_id) DO UPDATE SET \
                last_sequence = EXCLUDED.last_sequence, \
                updated_at = EXCLUDED.updated_at"
        )
        .bind(producer_id)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(sequence)
        .bind(now)
        .execute(&self.pool)
        .await?;
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
        let row = sqlx::query(
            "SELECT last_sequence FROM producer_sequences \
             WHERE producer_id = $1 AND topic = $2 AND partition_id = $3",
        )
        .bind(producer_id)
        .bind(topic)
        .bind(partition_id as i32)
        .fetch_optional(&mut *tx)
        .await?;

        let last_sequence = row.map(|r| r.get::<i64, _>("last_sequence")).unwrap_or(-1);

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
        sqlx::query(
            "INSERT INTO producer_sequences (producer_id, topic, partition_id, last_sequence, updated_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT(producer_id, topic, partition_id) DO UPDATE SET \
                last_sequence = EXCLUDED.last_sequence, \
                updated_at = EXCLUDED.updated_at"
        )
        .bind(producer_id)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(new_sequence)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(true)
    }

    // ---- Transaction Operations ----

    async fn begin_transaction(&self, producer_id: &str, timeout_ms: u32) -> Result<Transaction> {
        let now = Self::now_ms();
        let transaction_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO transactions (transaction_id, producer_id, state, timeout_ms, started_at, updated_at) \
             VALUES ($1, $2, 'ongoing', $3, $4, $5)"
        )
        .bind(&transaction_id)
        .bind(producer_id)
        .bind(timeout_ms as i32)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

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
        let row = sqlx::query(
            "SELECT transaction_id, producer_id, state, timeout_ms, started_at, updated_at, completed_at \
             FROM transactions WHERE transaction_id = $1"
        )
        .bind(transaction_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let state_str: String = r.get("state");
            Transaction {
                transaction_id: r.get("transaction_id"),
                producer_id: r.get("producer_id"),
                state: state_str.parse().unwrap_or(TransactionState::Ongoing),
                timeout_ms: r.get::<i32, _>("timeout_ms") as u32,
                started_at: r.get("started_at"),
                updated_at: r.get("updated_at"),
                completed_at: r.get("completed_at"),
            }
        }))
    }

    async fn add_transaction_partition(
        &self,
        transaction_id: &str,
        topic: &str,
        partition_id: u32,
        first_offset: u64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO transaction_partitions (transaction_id, topic, partition_id, first_offset, last_offset) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT(transaction_id, topic, partition_id) DO NOTHING"
        )
        .bind(transaction_id)
        .bind(topic)
        .bind(partition_id as i32)
        .bind(first_offset as i64)
        .bind(first_offset as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_transaction_partition_offset(
        &self,
        transaction_id: &str,
        topic: &str,
        partition_id: u32,
        last_offset: u64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE transaction_partitions SET last_offset = $1 \
             WHERE transaction_id = $2 AND topic = $3 AND partition_id = $4",
        )
        .bind(last_offset as i64)
        .bind(transaction_id)
        .bind(topic)
        .bind(partition_id as i32)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_transaction_partitions(
        &self,
        transaction_id: &str,
    ) -> Result<Vec<TransactionPartition>> {
        let rows = sqlx::query(
            "SELECT transaction_id, topic, partition_id, first_offset, last_offset \
             FROM transaction_partitions WHERE transaction_id = $1 ORDER BY topic, partition_id",
        )
        .bind(transaction_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TransactionPartition {
                transaction_id: r.get("transaction_id"),
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                first_offset: r.get::<i64, _>("first_offset") as u64,
                last_offset: r.get::<i64, _>("last_offset") as u64,
            })
            .collect())
    }

    async fn prepare_transaction(&self, transaction_id: &str) -> Result<()> {
        let now = Self::now_ms();
        let result = sqlx::query(
            "UPDATE transactions SET state = 'preparing', updated_at = $1 WHERE transaction_id = $2 AND state = 'ongoing'"
        )
        .bind(now)
        .bind(transaction_id)
        .execute(&self.pool)
        .await?;

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
        let result = sqlx::query(
            "UPDATE transactions SET state = 'committed', updated_at = $1, completed_at = $2 \
             WHERE transaction_id = $3 AND (state = 'ongoing' OR state = 'preparing')",
        )
        .bind(now)
        .bind(now)
        .bind(transaction_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} cannot be committed",
                transaction_id
            )));
        }

        // Write commit markers for each partition
        for partition in &partitions {
            let marker_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, \"offset\", marker_type, created_at) \
                 VALUES ($1, $2, $3, $4, $5, 'commit', $6)"
            )
            .bind(&marker_id)
            .bind(transaction_id)
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind(partition.last_offset as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            // Update LSO for each partition
            sqlx::query(
                "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT(topic, partition_id) DO UPDATE SET \
                    last_stable_offset = GREATEST(partition_lso.last_stable_offset, EXCLUDED.last_stable_offset), \
                    updated_at = EXCLUDED.updated_at"
            )
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind((partition.last_offset + 1) as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
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
        let result = sqlx::query(
            "UPDATE transactions SET state = 'aborted', updated_at = $1, completed_at = $2 \
             WHERE transaction_id = $3 AND (state = 'ongoing' OR state = 'preparing')",
        )
        .bind(now)
        .bind(now)
        .bind(transaction_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} cannot be aborted",
                transaction_id
            )));
        }

        // Write abort markers for each partition
        for partition in &partitions {
            let marker_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, \"offset\", marker_type, created_at) \
                 VALUES ($1, $2, $3, $4, $5, 'abort', $6)"
            )
            .bind(&marker_id)
            .bind(transaction_id)
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind(partition.last_offset as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            // Update LSO for each partition (even aborted transactions advance the LSO)
            sqlx::query(
                "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT(topic, partition_id) DO UPDATE SET \
                    last_stable_offset = GREATEST(partition_lso.last_stable_offset, EXCLUDED.last_stable_offset), \
                    updated_at = EXCLUDED.updated_at"
            )
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind((partition.last_offset + 1) as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn cleanup_completed_transactions(&self, max_age_ms: i64) -> Result<u64> {
        let cutoff = Self::now_ms() - max_age_ms;
        let result = sqlx::query(
            "DELETE FROM transactions WHERE completed_at IS NOT NULL AND completed_at < $1",
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // ---- Transaction Marker Operations ----

    async fn add_transaction_marker(&self, marker: TransactionMarker) -> Result<()> {
        let marker_id = uuid::Uuid::new_v4().to_string();
        let marker_type = match marker.marker_type {
            TransactionMarkerType::Commit => "commit",
            TransactionMarkerType::Abort => "abort",
        };
        sqlx::query(
            "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, \"offset\", marker_type, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(&marker_id)
        .bind(&marker.transaction_id)
        .bind(&marker.topic)
        .bind(marker.partition_id as i32)
        .bind(marker.offset as i64)
        .bind(marker_type)
        .bind(marker.timestamp)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_transaction_markers(
        &self,
        topic: &str,
        partition_id: u32,
        min_offset: u64,
    ) -> Result<Vec<TransactionMarker>> {
        let rows = sqlx::query(
            "SELECT transaction_id, topic, partition_id, \"offset\", marker_type, created_at \
             FROM transaction_markers \
             WHERE topic = $1 AND partition_id = $2 AND \"offset\" >= $3 \
             ORDER BY \"offset\"",
        )
        .bind(topic)
        .bind(partition_id as i32)
        .bind(min_offset as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let marker_type_str: String = r.get("marker_type");
                TransactionMarker {
                    transaction_id: r.get("transaction_id"),
                    topic: r.get("topic"),
                    partition_id: r.get::<i32, _>("partition_id") as u32,
                    offset: r.get::<i64, _>("offset") as u64,
                    marker_type: match marker_type_str.as_str() {
                        "commit" => TransactionMarkerType::Commit,
                        _ => TransactionMarkerType::Abort,
                    },
                    timestamp: r.get("created_at"),
                }
            })
            .collect())
    }

    // ---- LSO Operations ----

    async fn get_last_stable_offset(&self, topic: &str, partition_id: u32) -> Result<u64> {
        let row = sqlx::query(
            "SELECT last_stable_offset FROM partition_lso WHERE topic = $1 AND partition_id = $2",
        )
        .bind(topic)
        .bind(partition_id as i32)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row
            .map(|r| r.get::<i64, _>("last_stable_offset") as u64)
            .unwrap_or(0))
    }

    async fn update_last_stable_offset(
        &self,
        topic: &str,
        partition_id: u32,
        lso: u64,
    ) -> Result<()> {
        let now = Self::now_ms();
        sqlx::query(
            "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT(topic, partition_id) DO UPDATE SET \
                last_stable_offset = EXCLUDED.last_stable_offset, \
                updated_at = EXCLUDED.updated_at",
        )
        .bind(topic)
        .bind(partition_id as i32)
        .bind(lso as i64)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ============================================================
    // MATERIALIZED VIEW OPERATIONS
    // ============================================================

    async fn create_materialized_view(
        &self,
        config: CreateMaterializedView,
    ) -> Result<MaterializedView> {
        let now = chrono::Utc::now();
        let id = format!("mv-{}-{}", &config.name, uuid::Uuid::new_v4());
        let org_id = config
            .organization_id
            .clone()
            .unwrap_or_else(|| "00000000-0000-0000-0000-000000000000".to_string());

        // Convert refresh mode to storage format
        let (refresh_mode_str, interval_ms) = match &config.refresh_mode {
            MaterializedViewRefreshMode::Continuous => ("continuous", None),
            MaterializedViewRefreshMode::Periodic { interval_ms } => {
                ("periodic", Some(*interval_ms))
            }
            MaterializedViewRefreshMode::Manual => ("manual", None),
        };

        sqlx::query(
            "INSERT INTO materialized_views \
             (id, organization_id, name, source_topic, query_sql, refresh_mode, refresh_interval_ms, status, created_at, updated_at) \
             VALUES ($1, $2::uuid, $3, $4, $5, $6, $7, 'initializing', $8, $9)"
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
            created_at: now.timestamp_millis(),
            updated_at: now.timestamp_millis(),
        })
    }

    async fn get_materialized_view(&self, name: &str) -> Result<Option<MaterializedView>> {
        let row = sqlx::query(
            "SELECT id, organization_id, name, source_topic, query_sql, \
                    refresh_mode, refresh_interval_ms, status, error_message, \
                    row_count, last_refresh_at, created_at, updated_at \
             FROM materialized_views WHERE name = $1",
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
             FROM materialized_views WHERE id = $1",
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
        let now = chrono::Utc::now();

        sqlx::query(
            "UPDATE materialized_views SET status = $1, error_message = $2, updated_at = $3 WHERE id = $4"
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
        let now = chrono::Utc::now();
        let refresh_ts = chrono::DateTime::from_timestamp_millis(last_refresh_at).unwrap_or(now);

        sqlx::query(
            "UPDATE materialized_views SET row_count = $1, last_refresh_at = $2, updated_at = $3 WHERE id = $4"
        )
        .bind(row_count as i64)
        .bind(refresh_ts)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_materialized_view(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM materialized_views WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_materialized_view_offsets(
        &self,
        view_id: &str,
    ) -> Result<Vec<MaterializedViewOffset>> {
        let rows = sqlx::query(
            "SELECT view_id, partition_id, last_offset, last_processed_at \
             FROM materialized_view_offsets WHERE view_id = $1 ORDER BY partition_id",
        )
        .bind(view_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let last_processed: chrono::DateTime<chrono::Utc> = r.get("last_processed_at");
                MaterializedViewOffset {
                    view_id: r.get("view_id"),
                    partition_id: r.get::<i32, _>("partition_id") as u32,
                    last_offset: r.get::<i64, _>("last_offset") as u64,
                    last_processed_at: last_processed.timestamp_millis(),
                }
            })
            .collect())
    }

    async fn update_materialized_view_offset(
        &self,
        view_id: &str,
        partition_id: u32,
        last_offset: u64,
    ) -> Result<()> {
        let now = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO materialized_view_offsets (view_id, partition_id, last_offset, last_processed_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT(view_id, partition_id) DO UPDATE SET \
                last_offset = EXCLUDED.last_offset, \
                last_processed_at = EXCLUDED.last_processed_at"
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
             FROM materialized_view_data WHERE view_id = $1 ORDER BY agg_key LIMIT $2",
        )
        .bind(view_id)
        .bind(limit_val)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let updated_at: chrono::DateTime<chrono::Utc> = r.get("updated_at");
                MaterializedViewData {
                    view_id: r.get("view_id"),
                    agg_key: r.get("agg_key"),
                    agg_values: r.get("agg_values"),
                    window_start: r.get::<Option<i64>, _>("window_start"),
                    window_end: r.get::<Option<i64>, _>("window_end"),
                    updated_at: updated_at.timestamp_millis(),
                }
            })
            .collect())
    }

    async fn upsert_materialized_view_data(&self, data: MaterializedViewData) -> Result<()> {
        let now = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO materialized_view_data (view_id, agg_key, agg_values, window_start, window_end, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT(view_id, agg_key) DO UPDATE SET \
                agg_values = EXCLUDED.agg_values, \
                window_start = EXCLUDED.window_start, \
                window_end = EXCLUDED.window_end, \
                updated_at = EXCLUDED.updated_at"
        )
        .bind(&data.view_id)
        .bind(&data.agg_key)
        .bind(&data.agg_values)
        .bind(data.window_start)
        .bind(data.window_end)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn create_connector(&self, connector: ConnectorInfo) -> Result<()> {
        let topics_json = serde_json::to_string(&connector.topics)?;
        let config_json = serde_json::to_string(&connector.config)?;
        let org_uuid = uuid::Uuid::parse_str(&connector.organization_id)
            .unwrap_or_else(|_| uuid::Uuid::parse_str(crate::DEFAULT_ORGANIZATION_ID).unwrap());

        sqlx::query(
            "INSERT INTO connectors (organization_id, name, connector_type, connector_class, topics, config, state, records_processed, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8, $9)",
        )
        .bind(org_uuid)
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
            "SELECT organization_id, name, connector_type, connector_class, topics, config, state, \
                    error_message, records_processed, created_at, updated_at \
             FROM connectors WHERE name = $1",
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
            "SELECT organization_id, name, connector_type, connector_class, topics, config, state, \
                    error_message, records_processed, created_at, updated_at \
             FROM connectors ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_connector_info).collect()
    }

    async fn delete_connector(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM connectors WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_connector_state(
        &self,
        name: &str,
        state: &str,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Self::now_ms();

        sqlx::query(
            "UPDATE connectors SET state = $1, error_message = $2, updated_at = $3 WHERE name = $4",
        )
        .bind(state)
        .bind(error_message)
        .bind(now)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_connector_records_processed(
        &self,
        name: &str,
        records_processed: i64,
    ) -> Result<()> {
        let now = Self::now_ms();

        sqlx::query(
            "UPDATE connectors SET records_processed = $1, updated_at = $2 WHERE name = $3",
        )
        .bind(records_processed)
        .bind(now)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ── Org-scoped connector operations ──────────────────────

    async fn create_connector_for_org(
        &self,
        _org_id: &str,
        connector: ConnectorInfo,
    ) -> Result<()> {
        self.create_connector(connector).await
    }

    async fn get_connector_for_org(
        &self,
        org_id: &str,
        name: &str,
    ) -> Result<Option<ConnectorInfo>> {
        let org_uuid = uuid::Uuid::parse_str(org_id)
            .unwrap_or_else(|_| uuid::Uuid::parse_str(crate::DEFAULT_ORGANIZATION_ID).unwrap());

        let row = sqlx::query(
            "SELECT organization_id, name, connector_type, connector_class, topics, config, state, \
                    error_message, records_processed, created_at, updated_at \
             FROM connectors WHERE organization_id = $1 AND name = $2",
        )
        .bind(org_uuid)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(Self::row_to_connector_info(r)?)),
            None => Ok(None),
        }
    }

    async fn list_connectors_for_org(&self, org_id: &str) -> Result<Vec<ConnectorInfo>> {
        let org_uuid = uuid::Uuid::parse_str(org_id)
            .unwrap_or_else(|_| uuid::Uuid::parse_str(crate::DEFAULT_ORGANIZATION_ID).unwrap());

        let rows = sqlx::query(
            "SELECT organization_id, name, connector_type, connector_class, topics, config, state, \
                    error_message, records_processed, created_at, updated_at \
             FROM connectors WHERE organization_id = $1 ORDER BY name",
        )
        .bind(org_uuid)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_connector_info).collect()
    }

    async fn delete_connector_for_org(&self, org_id: &str, name: &str) -> Result<()> {
        let org_uuid = uuid::Uuid::parse_str(org_id)
            .unwrap_or_else(|_| uuid::Uuid::parse_str(crate::DEFAULT_ORGANIZATION_ID).unwrap());

        sqlx::query("DELETE FROM connectors WHERE organization_id = $1 AND name = $2")
            .bind(org_uuid)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_connector_state_for_org(
        &self,
        org_id: &str,
        name: &str,
        state: &str,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Self::now_ms();
        let org_uuid = uuid::Uuid::parse_str(org_id)
            .unwrap_or_else(|_| uuid::Uuid::parse_str(crate::DEFAULT_ORGANIZATION_ID).unwrap());

        sqlx::query(
            "UPDATE connectors SET state = $1, error_message = $2, updated_at = $3 \
             WHERE organization_id = $4 AND name = $5",
        )
        .bind(state)
        .bind(error_message)
        .bind(now)
        .bind(org_uuid)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_connector_records_processed_for_org(
        &self,
        org_id: &str,
        name: &str,
        records_processed: i64,
    ) -> Result<()> {
        let now = Self::now_ms();
        let org_uuid = uuid::Uuid::parse_str(org_id)
            .unwrap_or_else(|_| uuid::Uuid::parse_str(crate::DEFAULT_ORGANIZATION_ID).unwrap());

        sqlx::query(
            "UPDATE connectors SET records_processed = $1, updated_at = $2 \
             WHERE organization_id = $3 AND name = $4",
        )
        .bind(records_processed)
        .bind(now)
        .bind(org_uuid)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ── Org-scoped transaction operations ──────────────────────

    async fn begin_transaction_for_org(
        &self,
        org_id: &str,
        producer_id: &str,
        timeout_ms: u32,
    ) -> Result<Transaction> {
        let now = Self::now_ms();
        let transaction_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO transactions (transaction_id, producer_id, state, timeout_ms, started_at, updated_at, organization_id) \
             VALUES ($1, $2, 'ongoing', $3, $4, $5, $6)",
        )
        .bind(&transaction_id)
        .bind(producer_id)
        .bind(timeout_ms as i32)
        .bind(now)
        .bind(now)
        .bind(org_id)
        .execute(&self.pool)
        .await?;

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

    async fn get_transaction_for_org(
        &self,
        org_id: &str,
        transaction_id: &str,
    ) -> Result<Option<Transaction>> {
        let row = sqlx::query(
            "SELECT transaction_id, producer_id, state, timeout_ms, started_at, updated_at, completed_at \
             FROM transactions WHERE transaction_id = $1 AND organization_id = $2",
        )
        .bind(transaction_id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let state_str: String = r.get("state");
            Transaction {
                transaction_id: r.get("transaction_id"),
                producer_id: r.get("producer_id"),
                state: state_str.parse().unwrap_or(TransactionState::Ongoing),
                timeout_ms: r.get::<i32, _>("timeout_ms") as u32,
                started_at: r.get("started_at"),
                updated_at: r.get("updated_at"),
                completed_at: r.get("completed_at"),
            }
        }))
    }

    async fn prepare_transaction_for_org(&self, org_id: &str, transaction_id: &str) -> Result<()> {
        let now = Self::now_ms();
        let result = sqlx::query(
            "UPDATE transactions SET state = 'preparing', updated_at = $1 \
             WHERE transaction_id = $2 AND organization_id = $3 AND state = 'ongoing'",
        )
        .bind(now)
        .bind(transaction_id)
        .bind(org_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} is not in ongoing state",
                transaction_id
            )));
        }

        Ok(())
    }

    async fn commit_transaction_for_org(&self, org_id: &str, transaction_id: &str) -> Result<i64> {
        let now = Self::now_ms();

        // Get transaction partitions for writing markers
        let partitions = self.get_transaction_partitions(transaction_id).await?;

        let mut tx = self.pool.begin().await?;

        // Update transaction state with org check
        let result = sqlx::query(
            "UPDATE transactions SET state = 'committed', updated_at = $1, completed_at = $2 \
             WHERE transaction_id = $3 AND organization_id = $4 AND (state = 'ongoing' OR state = 'preparing')",
        )
        .bind(now)
        .bind(now)
        .bind(transaction_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} cannot be committed",
                transaction_id
            )));
        }

        // Write commit markers for each partition
        for partition in &partitions {
            let marker_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, \"offset\", marker_type, created_at) \
                 VALUES ($1, $2, $3, $4, $5, 'commit', $6)",
            )
            .bind(&marker_id)
            .bind(transaction_id)
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind(partition.last_offset as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            // Update LSO for each partition
            sqlx::query(
                "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT(topic, partition_id) DO UPDATE SET \
                    last_stable_offset = GREATEST(partition_lso.last_stable_offset, EXCLUDED.last_stable_offset), \
                    updated_at = EXCLUDED.updated_at",
            )
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind((partition.last_offset + 1) as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(now)
    }

    async fn abort_transaction_for_org(&self, org_id: &str, transaction_id: &str) -> Result<()> {
        let now = Self::now_ms();

        // Get transaction partitions for writing markers
        let partitions = self.get_transaction_partitions(transaction_id).await?;

        let mut tx = self.pool.begin().await?;

        // Update transaction state with org check
        let result = sqlx::query(
            "UPDATE transactions SET state = 'aborted', updated_at = $1, completed_at = $2 \
             WHERE transaction_id = $3 AND organization_id = $4 AND (state = 'ongoing' OR state = 'preparing')",
        )
        .bind(now)
        .bind(now)
        .bind(transaction_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} cannot be aborted",
                transaction_id
            )));
        }

        // Write abort markers for each partition
        for partition in &partitions {
            let marker_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO transaction_markers (id, transaction_id, topic, partition_id, \"offset\", marker_type, created_at) \
                 VALUES ($1, $2, $3, $4, $5, 'abort', $6)",
            )
            .bind(&marker_id)
            .bind(transaction_id)
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind(partition.last_offset as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            // Update LSO for each partition (even aborted transactions advance the LSO)
            sqlx::query(
                "INSERT INTO partition_lso (topic, partition_id, last_stable_offset, updated_at) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT(topic, partition_id) DO UPDATE SET \
                    last_stable_offset = GREATEST(partition_lso.last_stable_offset, EXCLUDED.last_stable_offset), \
                    updated_at = EXCLUDED.updated_at",
            )
            .bind(&partition.topic)
            .bind(partition.partition_id as i32)
            .bind((partition.last_offset + 1) as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn add_transaction_partition_for_org(
        &self,
        org_id: &str,
        transaction_id: &str,
        topic: &str,
        partition_id: u32,
        first_offset: u64,
    ) -> Result<()> {
        // Verify transaction belongs to org before adding partition
        let row = sqlx::query(
            "SELECT COUNT(*) as cnt FROM transactions WHERE transaction_id = $1 AND organization_id = $2",
        )
        .bind(transaction_id)
        .bind(org_id)
        .fetch_one(&self.pool)
        .await?;

        let count: i64 = row.get("cnt");
        if count == 0 {
            return Err(MetadataError::TransactionError(format!(
                "Transaction {} not found for organization {}",
                transaction_id, org_id
            )));
        }

        self.add_transaction_partition(transaction_id, topic, partition_id, first_offset)
            .await
    }

    async fn get_transaction_partitions_for_org(
        &self,
        org_id: &str,
        transaction_id: &str,
    ) -> Result<Vec<TransactionPartition>> {
        // JOIN with transactions to verify org ownership
        let rows = sqlx::query(
            "SELECT tp.transaction_id, tp.topic, tp.partition_id, tp.first_offset, tp.last_offset \
             FROM transaction_partitions tp \
             JOIN transactions t ON tp.transaction_id = t.transaction_id \
             WHERE tp.transaction_id = $1 AND t.organization_id = $2 \
             ORDER BY tp.topic, tp.partition_id",
        )
        .bind(transaction_id)
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TransactionPartition {
                transaction_id: r.get("transaction_id"),
                topic: r.get("topic"),
                partition_id: r.get::<i32, _>("partition_id") as u32,
                first_offset: r.get::<i64, _>("first_offset") as u64,
                last_offset: r.get::<i64, _>("last_offset") as u64,
            })
            .collect())
    }

    // ── Pipeline Target operations ──────────────────────

    async fn create_pipeline_target(&self, target: PipelineTarget) -> Result<()> {
        let config_json = serde_json::to_string(&target.connection_config)
            .map_err(|e| MetadataError::InternalError(e.to_string()))?;
        let org_uuid = uuid::Uuid::parse_str(&target.organization_id)
            .map_err(|e| MetadataError::InternalError(format!("Invalid org UUID: {}", e)))?;
        sqlx::query(
            "INSERT INTO pipeline_targets (id, organization_id, name, target_type, connection_config, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(uuid::Uuid::parse_str(&target.id).map_err(|e| MetadataError::InternalError(e.to_string()))?)
        .bind(org_uuid)
        .bind(&target.name)
        .bind(&target.target_type)
        .bind(&config_json)
        .bind(target.created_at)
        .bind(target.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_pipeline_target(&self, name: &str) -> Result<Option<PipelineTarget>> {
        let row = sqlx::query(
            "SELECT id, organization_id, name, target_type, connection_config, created_at, updated_at \
             FROM pipeline_targets WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| Self::row_to_pipeline_target(r)))
    }

    async fn get_pipeline_target_by_id(&self, id: &str) -> Result<Option<PipelineTarget>> {
        let id_uuid = uuid::Uuid::parse_str(id)
            .map_err(|e| MetadataError::InternalError(format!("Invalid UUID: {}", e)))?;
        let row = sqlx::query(
            "SELECT id, organization_id, name, target_type, connection_config, created_at, updated_at \
             FROM pipeline_targets WHERE id = $1",
        )
        .bind(id_uuid)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| Self::row_to_pipeline_target(r)))
    }

    async fn list_pipeline_targets(&self) -> Result<Vec<PipelineTarget>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, name, target_type, connection_config, created_at, updated_at \
             FROM pipeline_targets ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Self::row_to_pipeline_target(r))
            .collect())
    }

    async fn delete_pipeline_target(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM pipeline_targets WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Pipeline operations ──────────────────────

    async fn create_pipeline(&self, pipeline: PipelineInfo) -> Result<()> {
        let org_uuid = uuid::Uuid::parse_str(&pipeline.organization_id)
            .map_err(|e| MetadataError::InternalError(format!("Invalid org UUID: {}", e)))?;
        sqlx::query(
            "INSERT INTO pipelines (id, organization_id, name, source_topic, consumer_group, target_id, transform_sql, state, error_message, records_processed, last_offset, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(uuid::Uuid::parse_str(&pipeline.id).map_err(|e| MetadataError::InternalError(e.to_string()))?)
        .bind(org_uuid)
        .bind(&pipeline.name)
        .bind(&pipeline.source_topic)
        .bind(&pipeline.consumer_group)
        .bind(uuid::Uuid::parse_str(&pipeline.target_id).map_err(|e| MetadataError::InternalError(e.to_string()))?)
        .bind(&pipeline.transform_sql)
        .bind(&pipeline.state)
        .bind(&pipeline.error_message)
        .bind(pipeline.records_processed)
        .bind(pipeline.last_offset)
        .bind(pipeline.created_at)
        .bind(pipeline.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_pipeline(&self, name: &str) -> Result<Option<PipelineInfo>> {
        let row = sqlx::query(
            "SELECT id, organization_id, name, source_topic, consumer_group, target_id, transform_sql, state, error_message, records_processed, last_offset, created_at, updated_at \
             FROM pipelines WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| Self::row_to_pipeline_info(r)))
    }

    async fn get_pipeline_by_id(&self, id: &str) -> Result<Option<PipelineInfo>> {
        let id_uuid = uuid::Uuid::parse_str(id)
            .map_err(|e| MetadataError::InternalError(format!("Invalid UUID: {}", e)))?;
        let row = sqlx::query(
            "SELECT id, organization_id, name, source_topic, consumer_group, target_id, transform_sql, state, error_message, records_processed, last_offset, created_at, updated_at \
             FROM pipelines WHERE id = $1",
        )
        .bind(id_uuid)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| Self::row_to_pipeline_info(r)))
    }

    async fn list_pipelines(&self) -> Result<Vec<PipelineInfo>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, name, source_topic, consumer_group, target_id, transform_sql, state, error_message, records_processed, last_offset, created_at, updated_at \
             FROM pipelines ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Self::row_to_pipeline_info(r))
            .collect())
    }

    async fn delete_pipeline(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM pipelines WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_pipeline_state(
        &self,
        name: &str,
        state: &str,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE pipelines SET state = $1, error_message = $2, updated_at = $3 WHERE name = $4",
        )
        .bind(state)
        .bind(error_message)
        .bind(now)
        .bind(name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_pipeline_progress(
        &self,
        name: &str,
        records_processed: i64,
        last_offset: i64,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE pipelines SET records_processed = $1, last_offset = $2, updated_at = $3 WHERE name = $4",
        )
        .bind(records_processed)
        .bind(last_offset)
        .bind(now)
        .bind(name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

impl PostgresMetadataStore {
    /// Helper to convert a database row to PipelineTarget
    fn row_to_pipeline_target(r: sqlx::postgres::PgRow) -> PipelineTarget {
        use sqlx::Row;
        let config_str: String = r.get("connection_config");
        let connection_config: std::collections::HashMap<String, String> =
            serde_json::from_str(&config_str).unwrap_or_default();
        let org_id: uuid::Uuid = r.get("organization_id");
        let id: uuid::Uuid = r.get("id");
        PipelineTarget {
            id: id.to_string(),
            organization_id: org_id.to_string(),
            name: r.get("name"),
            target_type: r.get("target_type"),
            connection_config,
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }

    /// Helper to convert a database row to PipelineInfo
    fn row_to_pipeline_info(r: sqlx::postgres::PgRow) -> PipelineInfo {
        use sqlx::Row;
        let org_id: uuid::Uuid = r.get("organization_id");
        let id: uuid::Uuid = r.get("id");
        let target_id: uuid::Uuid = r.get("target_id");
        PipelineInfo {
            id: id.to_string(),
            organization_id: org_id.to_string(),
            name: r.get("name"),
            source_topic: r.get("source_topic"),
            consumer_group: r.get("consumer_group"),
            target_id: target_id.to_string(),
            transform_sql: r.get("transform_sql"),
            state: r.get("state"),
            error_message: r.get("error_message"),
            records_processed: r.get("records_processed"),
            last_offset: r.get("last_offset"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }

    /// Helper to convert a database row to ConnectorInfo
    fn row_to_connector_info(r: sqlx::postgres::PgRow) -> Result<ConnectorInfo> {
        use sqlx::Row;

        let topics_str: String = r.get("topics");
        let config_str: String = r.get("config");

        let topics: Vec<String> = serde_json::from_str(&topics_str)?;
        let config: std::collections::HashMap<String, String> = serde_json::from_str(&config_str)?;

        let org_id: uuid::Uuid = r.get("organization_id");
        Ok(ConnectorInfo {
            organization_id: org_id.to_string(),
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

    /// Helper to convert a database row to MaterializedView
    fn row_to_materialized_view(r: sqlx::postgres::PgRow) -> MaterializedView {
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

        let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = r.get("updated_at");
        let last_refresh_at: Option<chrono::DateTime<chrono::Utc>> = r.get("last_refresh_at");
        let org_id: uuid::Uuid = r.get("organization_id");

        MaterializedView {
            id: r.get("id"),
            organization_id: org_id.to_string(),
            name: r.get("name"),
            source_topic: r.get("source_topic"),
            query_sql: r.get("query_sql"),
            refresh_mode,
            status,
            error_message: r.get("error_message"),
            row_count: r.get::<i64, _>("row_count") as u64,
            last_refresh_at: last_refresh_at.map(|t| t.timestamp_millis()),
            created_at: created_at.timestamp_millis(),
            updated_at: updated_at.timestamp_millis(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Helper to get test database URL from environment
    fn get_test_db_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    #[tokio::test]
    #[ignore] // Run only when PostgreSQL is available
    async fn test_postgres_create_and_get_topic() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => {
                eprintln!("Skipping test: DATABASE_URL not set or not PostgreSQL");
                return;
            }
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        let config = TopicConfig {
            name: "pg_test_topic".to_string(),
            partition_count: 3,
            retention_ms: None,
            config: HashMap::new(),
        };

        // Clean up any existing test data
        let _ = store.delete_topic("pg_test_topic").await;

        // Create topic
        store.create_topic(config.clone()).await.unwrap();

        // Get topic
        let topic = store.get_topic("pg_test_topic").await.unwrap().unwrap();
        assert_eq!(topic.name, "pg_test_topic");
        assert_eq!(topic.partition_count, 3);

        // Should have created 3 partitions
        let partitions = store
            .list_partitions(crate::DEFAULT_ORGANIZATION_ID, "pg_test_topic")
            .await
            .unwrap();
        assert_eq!(partitions.len(), 3);

        // Clean up
        store.delete_topic("pg_test_topic").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_agent_registration() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        let agent = AgentInfo {
            agent_id: "test-agent-1".to_string(),
            address: "127.0.0.1:8080".to_string(),
            availability_zone: "us-east-1a".to_string(),
            agent_group: "writers".to_string(),
            last_heartbeat: chrono::Utc::now().timestamp_millis(),
            started_at: chrono::Utc::now().timestamp_millis(),
            metadata: HashMap::new(),
        };

        // Clean up
        let _ = store.deregister_agent("test-agent-1").await;

        // Register agent
        store.register_agent(agent.clone()).await.unwrap();

        // Get agent
        let retrieved = store.get_agent("test-agent-1").await.unwrap().unwrap();
        assert_eq!(retrieved.agent_id, "test-agent-1");
        assert_eq!(retrieved.address, "127.0.0.1:8080");
        assert_eq!(retrieved.availability_zone, "us-east-1a");

        // List agents
        let agents = store.list_agents(Some("writers"), None).await.unwrap();
        assert!(agents.iter().any(|a| a.agent_id == "test-agent-1"));

        // Clean up
        store.deregister_agent("test-agent-1").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_partition_lease() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        // Create test topic and agent
        let config = TopicConfig {
            name: "lease_test".to_string(),
            partition_count: 1,
            retention_ms: None,
            config: HashMap::new(),
        };

        let _ = store.delete_topic("lease_test").await;
        store.create_topic(config).await.unwrap();

        let agent = AgentInfo {
            agent_id: "lease-agent-1".to_string(),
            address: "127.0.0.1:9090".to_string(),
            availability_zone: "us-east-1a".to_string(),
            agent_group: "writers".to_string(),
            last_heartbeat: chrono::Utc::now().timestamp_millis(),
            started_at: chrono::Utc::now().timestamp_millis(),
            metadata: HashMap::new(),
        };

        let _ = store.deregister_agent("lease-agent-1").await;
        store.register_agent(agent).await.unwrap();

        // Acquire lease
        let lease = store
            .acquire_partition_lease(
                crate::DEFAULT_ORGANIZATION_ID,
                "lease_test",
                0,
                "lease-agent-1",
                30000,
            )
            .await
            .unwrap();

        assert_eq!(lease.topic, "lease_test");
        assert_eq!(lease.partition_id, 0);
        assert_eq!(lease.leader_agent_id, "lease-agent-1");

        // Get lease
        let retrieved = store
            .get_partition_lease("lease_test", 0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.leader_agent_id, "lease-agent-1");

        // Release lease
        store
            .release_partition_lease("lease_test", 0, "lease-agent-1")
            .await
            .unwrap();

        // Lease should be gone
        let no_lease = store.get_partition_lease("lease_test", 0).await.unwrap();
        assert!(no_lease.is_none());

        // Clean up
        store.delete_topic("lease_test").await.unwrap();
        store.deregister_agent("lease-agent-1").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_10k_partitions() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        // Clean up any existing test data
        let _ = store.delete_topic("perf_test_10k").await;

        println!("\nCreating topic with 10,000 partitions...");
        let start = std::time::Instant::now();

        let config = TopicConfig {
            name: "perf_test_10k".to_string(),
            partition_count: 10_000,
            retention_ms: None,
            config: HashMap::new(),
        };

        store.create_topic(config).await.unwrap();
        let create_duration = start.elapsed();

        println!(
            "✓ Created topic with 10K partitions in {:?}",
            create_duration
        );

        // Test partition lookups
        println!("Testing partition lookups...");
        let lookup_start = std::time::Instant::now();

        for partition_id in [0, 1000, 5000, 9999] {
            let partition = store
                .get_partition(
                    crate::DEFAULT_ORGANIZATION_ID,
                    "perf_test_10k",
                    partition_id,
                )
                .await
                .unwrap()
                .unwrap();
            assert_eq!(partition.partition_id, partition_id);
        }

        let lookup_duration = lookup_start.elapsed();
        println!("✓ 4 partition lookups completed in {:?}", lookup_duration);

        // List all partitions
        println!("Listing all 10,000 partitions...");
        let list_start = std::time::Instant::now();
        let partitions = store
            .list_partitions(crate::DEFAULT_ORGANIZATION_ID, "perf_test_10k")
            .await
            .unwrap();
        let list_duration = list_start.elapsed();

        assert_eq!(partitions.len(), 10_000);
        println!("✓ Listed 10K partitions in {:?}", list_duration);

        // Clean up
        println!("Cleaning up...");
        store.delete_topic("perf_test_10k").await.unwrap();

        println!("\n=== Performance Summary ===");
        println!("Topic creation (10K partitions): {:?}", create_duration);
        println!("Partition lookups (4 queries):   {:?}", lookup_duration);
        println!("List all partitions (10K):       {:?}", list_duration);

        // Assertions for acceptable performance
        assert!(
            create_duration.as_secs() < 60,
            "Topic creation should complete within 60 seconds"
        );
        assert!(
            lookup_duration.as_millis() < 500,
            "Partition lookups should be < 500ms"
        );
        assert!(
            list_duration.as_secs() < 10,
            "Listing 10K partitions should complete within 10 seconds"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_segment_operations() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        // Create test topic
        let _ = store.delete_topic("segment_test").await;
        store
            .create_topic(TopicConfig {
                name: "segment_test".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .unwrap();

        // Add segments
        let segment1 = SegmentInfo {
            id: "seg-1".to_string(),
            topic: "segment_test".to_string(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 999,
            record_count: 1000,
            size_bytes: 1024000,
            s3_bucket: "test-bucket".to_string(),
            s3_key: "segment_test/0/00000000.seg".to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            min_timestamp: 0,
            max_timestamp: 0,
        };

        let segment2 = SegmentInfo {
            id: "seg-2".to_string(),
            topic: "segment_test".to_string(),
            partition_id: 0,
            base_offset: 1000,
            end_offset: 1999,
            record_count: 1000,
            size_bytes: 1024000,
            s3_bucket: "test-bucket".to_string(),
            s3_key: "segment_test/0/00001000.seg".to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            min_timestamp: 0,
            max_timestamp: 0,
        };

        store
            .add_segment(crate::DEFAULT_ORGANIZATION_ID, segment1)
            .await
            .unwrap();
        store
            .add_segment(crate::DEFAULT_ORGANIZATION_ID, segment2)
            .await
            .unwrap();

        // Get all segments
        let segments = store
            .get_segments(crate::DEFAULT_ORGANIZATION_ID, "segment_test", 0)
            .await
            .unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].base_offset, 0);
        assert_eq!(segments[1].base_offset, 1000);

        // Find segment by offset
        let seg = store
            .find_segment_for_offset("segment_test", 0, 500)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(seg.id, "seg-1");
        assert_eq!(seg.base_offset, 0);
        assert_eq!(seg.end_offset, 999);

        let seg = store
            .find_segment_for_offset("segment_test", 0, 1500)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(seg.id, "seg-2");

        // Offset not found
        let seg = store
            .find_segment_for_offset("segment_test", 0, 5000)
            .await
            .unwrap();
        assert!(seg.is_none());

        // Delete old segments
        let deleted = store
            .delete_segments_before(DEFAULT_ORGANIZATION_ID, "segment_test", 0, 1000)
            .await
            .unwrap();
        assert_eq!(deleted, 1); // Only seg-1 should be deleted

        let segments = store
            .get_segments(crate::DEFAULT_ORGANIZATION_ID, "segment_test", 0)
            .await
            .unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].id, "seg-2");

        // Clean up
        store.delete_topic("segment_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_high_watermark() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        // Create test topic
        let _ = store.delete_topic("watermark_test").await;
        store
            .create_topic(TopicConfig {
                name: "watermark_test".to_string(),
                partition_count: 2,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .unwrap();

        // Initial watermark should be 0
        let partition = store
            .get_partition(crate::DEFAULT_ORGANIZATION_ID, "watermark_test", 0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(partition.high_watermark, 0);

        // Update watermark
        store
            .update_high_watermark(crate::DEFAULT_ORGANIZATION_ID, "watermark_test", 0, 1000)
            .await
            .unwrap();

        let partition = store
            .get_partition(crate::DEFAULT_ORGANIZATION_ID, "watermark_test", 0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(partition.high_watermark, 1000);

        // Update to higher value
        store
            .update_high_watermark(crate::DEFAULT_ORGANIZATION_ID, "watermark_test", 0, 5000)
            .await
            .unwrap();

        let partition = store
            .get_partition(crate::DEFAULT_ORGANIZATION_ID, "watermark_test", 0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(partition.high_watermark, 5000);

        // Other partition should still be 0
        let partition = store
            .get_partition(crate::DEFAULT_ORGANIZATION_ID, "watermark_test", 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(partition.high_watermark, 0);

        // Clean up
        store.delete_topic("watermark_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_consumer_groups() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        // Create test topic
        let _ = store.delete_topic("consumer_test").await;
        store
            .create_topic(TopicConfig {
                name: "consumer_test".to_string(),
                partition_count: 3,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .unwrap();

        // Clean up any existing consumer group
        let _ = store.delete_consumer_group("test-group").await;

        // Initially no offset
        let offset = store
            .get_committed_offset("test-group", "consumer_test", 0)
            .await
            .unwrap();
        assert!(offset.is_none());

        // Commit offset
        store
            .commit_offset(
                "test-group",
                "consumer_test",
                0,
                100,
                Some("metadata1".to_string()),
            )
            .await
            .unwrap();

        // Retrieve offset
        let offset = store
            .get_committed_offset("test-group", "consumer_test", 0)
            .await
            .unwrap();
        assert_eq!(offset, Some(100));

        // Commit offsets for multiple partitions
        store
            .commit_offset("test-group", "consumer_test", 1, 200, None)
            .await
            .unwrap();
        store
            .commit_offset("test-group", "consumer_test", 2, 300, None)
            .await
            .unwrap();

        // Get all offsets for group
        let offsets = store.get_consumer_offsets("test-group").await.unwrap();
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0].committed_offset, 100);
        assert_eq!(offsets[0].metadata, Some("metadata1".to_string()));
        assert_eq!(offsets[1].committed_offset, 200);
        assert_eq!(offsets[2].committed_offset, 300);

        // Update existing offset (upsert)
        store
            .commit_offset(
                "test-group",
                "consumer_test",
                0,
                150,
                Some("metadata2".to_string()),
            )
            .await
            .unwrap();

        let offset = store
            .get_committed_offset("test-group", "consumer_test", 0)
            .await
            .unwrap();
        assert_eq!(offset, Some(150));

        // Delete consumer group
        store.delete_consumer_group("test-group").await.unwrap();

        // Offsets should be gone
        let offset = store
            .get_committed_offset("test-group", "consumer_test", 0)
            .await
            .unwrap();
        assert!(offset.is_none());

        // Clean up
        store.delete_topic("consumer_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_list_topics() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        // Clean up
        let _ = store.delete_topic("topic1").await;
        let _ = store.delete_topic("topic2").await;
        let _ = store.delete_topic("topic3").await;

        // Create multiple topics
        for (name, partitions) in [("topic1", 4), ("topic2", 8), ("topic3", 16)] {
            store
                .create_topic(TopicConfig {
                    name: name.to_string(),
                    partition_count: partitions,
                    retention_ms: Some(86400000),
                    config: HashMap::new(),
                })
                .await
                .unwrap();
        }

        // List all topics
        let topics = store.list_topics().await.unwrap();
        assert!(topics.len() >= 3); // May have other topics from other tests

        // Find our topics
        let topic1 = topics.iter().find(|t| t.name == "topic1").unwrap();
        let topic2 = topics.iter().find(|t| t.name == "topic2").unwrap();
        let topic3 = topics.iter().find(|t| t.name == "topic3").unwrap();

        assert_eq!(topic1.partition_count, 4);
        assert_eq!(topic2.partition_count, 8);
        assert_eq!(topic3.partition_count, 16);
        assert_eq!(topic1.retention_ms, Some(86400000));

        // Clean up
        store.delete_topic("topic1").await.unwrap();
        store.delete_topic("topic2").await.unwrap();
        store.delete_topic("topic3").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_lease_conflict() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        // Create test topic
        let _ = store.delete_topic("lease_conflict_test").await;
        store
            .create_topic(TopicConfig {
                name: "lease_conflict_test".to_string(),
                partition_count: 1,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .unwrap();

        // Register two agents
        let _ = store.deregister_agent("agent-1").await;
        let _ = store.deregister_agent("agent-2").await;

        for agent_id in ["agent-1", "agent-2"] {
            store
                .register_agent(AgentInfo {
                    agent_id: agent_id.to_string(),
                    address: format!(
                        "127.0.0.1:{}",
                        if agent_id == "agent-1" { 9091 } else { 9092 }
                    ),
                    availability_zone: "us-east-1a".to_string(),
                    agent_group: "writers".to_string(),
                    last_heartbeat: chrono::Utc::now().timestamp_millis(),
                    started_at: chrono::Utc::now().timestamp_millis(),
                    metadata: HashMap::new(),
                })
                .await
                .unwrap();
        }

        // Agent-1 acquires lease
        let lease = store
            .acquire_partition_lease(
                crate::DEFAULT_ORGANIZATION_ID,
                "lease_conflict_test",
                0,
                "agent-1",
                30000,
            )
            .await
            .unwrap();
        assert_eq!(lease.leader_agent_id, "agent-1");
        assert_eq!(lease.epoch, 1);

        // Agent-2 tries to acquire the same lease (should fail)
        let result = store
            .acquire_partition_lease(
                crate::DEFAULT_ORGANIZATION_ID,
                "lease_conflict_test",
                0,
                "agent-2",
                30000,
            )
            .await;
        assert!(result.is_err());
        assert!(matches!(result, Err(MetadataError::ConflictError(_))));

        // Agent-1 can renew its own lease
        let lease = store
            .acquire_partition_lease(
                crate::DEFAULT_ORGANIZATION_ID,
                "lease_conflict_test",
                0,
                "agent-1",
                30000,
            )
            .await
            .unwrap();
        assert_eq!(lease.leader_agent_id, "agent-1");
        assert_eq!(lease.epoch, 2); // Epoch incremented

        // List leases
        let leases = store
            .list_partition_leases(Some("lease_conflict_test"), None)
            .await
            .unwrap();
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].leader_agent_id, "agent-1");

        // Clean up
        store
            .release_partition_lease("lease_conflict_test", 0, "agent-1")
            .await
            .unwrap();
        store.delete_topic("lease_conflict_test").await.unwrap();
        store.deregister_agent("agent-1").await.unwrap();
        store.deregister_agent("agent-2").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_stale_agent_detection() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        let _ = store.deregister_agent("fresh-agent").await;
        let _ = store.deregister_agent("stale-agent").await;

        // Register fresh agent (recent heartbeat)
        store
            .register_agent(AgentInfo {
                agent_id: "fresh-agent".to_string(),
                address: "127.0.0.1:9091".to_string(),
                availability_zone: "us-east-1a".to_string(),
                agent_group: "writers".to_string(),
                last_heartbeat: chrono::Utc::now().timestamp_millis(),
                started_at: chrono::Utc::now().timestamp_millis(),
                metadata: HashMap::new(),
            })
            .await
            .unwrap();

        // Register stale agent (old heartbeat - 2 minutes ago)
        let two_minutes_ago = chrono::Utc::now().timestamp_millis() - 120_000;
        store
            .register_agent(AgentInfo {
                agent_id: "stale-agent".to_string(),
                address: "127.0.0.1:9092".to_string(),
                availability_zone: "us-east-1a".to_string(),
                agent_group: "writers".to_string(),
                last_heartbeat: two_minutes_ago,
                started_at: two_minutes_ago,
                metadata: HashMap::new(),
            })
            .await
            .unwrap();

        // list_agents filters out agents with heartbeat > 60s old
        let agents = store.list_agents(Some("writers"), None).await.unwrap();

        // Should only get fresh agent
        assert!(agents.iter().any(|a| a.agent_id == "fresh-agent"));
        assert!(!agents.iter().any(|a| a.agent_id == "stale-agent"));

        // Clean up
        store.deregister_agent("fresh-agent").await.unwrap();
        store.deregister_agent("stale-agent").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_topic_with_config() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        let _ = store.delete_topic("config_test").await;

        // Create topic with custom config
        let mut config_map = HashMap::new();
        config_map.insert("compression".to_string(), "lz4".to_string());
        config_map.insert("retention.policy".to_string(), "delete".to_string());
        config_map.insert("max.message.bytes".to_string(), "1048576".to_string());

        store
            .create_topic(TopicConfig {
                name: "config_test".to_string(),
                partition_count: 1,
                retention_ms: Some(86400000),
                config: config_map.clone(),
            })
            .await
            .unwrap();

        // Retrieve and verify config
        let topic = store.get_topic("config_test").await.unwrap().unwrap();
        assert_eq!(topic.config.len(), 3);
        assert_eq!(topic.config.get("compression").unwrap(), "lz4");
        assert_eq!(topic.config.get("retention.policy").unwrap(), "delete");
        assert_eq!(topic.config.get("max.message.bytes").unwrap(), "1048576");

        // Clean up
        store.delete_topic("config_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_cascade_delete() {
        let db_url = match get_test_db_url() {
            Some(url) if url.starts_with("postgres://") => url,
            _ => return,
        };

        let store = PostgresMetadataStore::new(&db_url).await.unwrap();

        let _ = store.delete_topic("cascade_test").await;

        // Create topic with partitions and segments
        store
            .create_topic(TopicConfig {
                name: "cascade_test".to_string(),
                partition_count: 2,
                retention_ms: None,
                config: HashMap::new(),
            })
            .await
            .unwrap();

        // Add segment
        store
            .add_segment(
                crate::DEFAULT_ORGANIZATION_ID,
                SegmentInfo {
                    id: "cascade-seg".to_string(),
                    topic: "cascade_test".to_string(),
                    partition_id: 0,
                    base_offset: 0,
                    end_offset: 999,
                    record_count: 1000,
                    size_bytes: 1024,
                    s3_bucket: "test".to_string(),
                    s3_key: "test/key".to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    min_timestamp: 0,
                    max_timestamp: 0,
                },
            )
            .await
            .unwrap();

        // Commit consumer offset
        let _ = store.delete_consumer_group("cascade-group").await;
        store
            .commit_offset("cascade-group", "cascade_test", 0, 500, None)
            .await
            .unwrap();

        // Verify everything exists
        let partitions = store
            .list_partitions(crate::DEFAULT_ORGANIZATION_ID, "cascade_test")
            .await
            .unwrap();
        assert_eq!(partitions.len(), 2);

        let segments = store
            .get_segments(crate::DEFAULT_ORGANIZATION_ID, "cascade_test", 0)
            .await
            .unwrap();
        assert_eq!(segments.len(), 1);

        let offset = store
            .get_committed_offset("cascade-group", "cascade_test", 0)
            .await
            .unwrap();
        assert_eq!(offset, Some(500));

        // Delete topic (should cascade delete partitions and segments)
        store.delete_topic("cascade_test").await.unwrap();

        // Verify cascaded deletes
        let partitions = store
            .list_partitions(crate::DEFAULT_ORGANIZATION_ID, "cascade_test")
            .await
            .unwrap();
        assert_eq!(partitions.len(), 0);

        let segments = store
            .get_segments(crate::DEFAULT_ORGANIZATION_ID, "cascade_test", 0)
            .await
            .unwrap();
        assert_eq!(segments.len(), 0);

        // Consumer offsets should also be cleaned up
        let offset = store
            .get_committed_offset("cascade-group", "cascade_test", 0)
            .await
            .unwrap();
        assert!(offset.is_none());

        // Clean up
        let _ = store.delete_consumer_group("cascade-group").await;
    }
}
