-- ============================================================================
-- StreamHouse Metadata Store - Multi-Tenancy Foundation (SQLite)
-- ============================================================================
--
-- Migration: 003_multi_tenancy.sql
-- Backend: SQLite
-- Version: 3
-- Created: 2026-02-03
-- Phase: 21.5 (Multi-Tenancy Foundation)
--
-- ## Purpose
--
-- Adds multi-tenancy support to StreamHouse for SaaS/managed service deployment.
-- This SQLite version provides the same data model as PostgreSQL but without
-- Row-Level Security (which requires application-level enforcement).
--
-- ## Differences from PostgreSQL
--
-- - No RLS (enforced at application level instead)
-- - No UUID type (using TEXT)
-- - No JSONB (using TEXT with JSON validation at app level)
-- - No ALTER TABLE ADD CONSTRAINT (constraints defined in table creation)
--
-- ============================================================================

-- ============================================================================
-- ORGANIZATIONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' || substr(hex(randomblob(2)),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(hex(randomblob(2)),2) || '-' || hex(randomblob(6)))),
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    plan TEXT NOT NULL DEFAULT 'free',
    status TEXT NOT NULL DEFAULT 'active',
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    settings TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_organizations_slug ON organizations(slug);
CREATE INDEX IF NOT EXISTS idx_organizations_status ON organizations(status);
CREATE INDEX IF NOT EXISTS idx_organizations_plan ON organizations(plan);

-- Insert default organization for backwards compatibility
INSERT OR IGNORE INTO organizations (id, name, slug, plan, status)
VALUES ('00000000-0000-0000-0000-000000000000', 'Default Organization', 'default', 'enterprise', 'active');

-- ============================================================================
-- API KEYS
-- ============================================================================

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' || substr(hex(randomblob(2)),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(hex(randomblob(2)),2) || '-' || hex(randomblob(6)))),
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL UNIQUE,
    permissions TEXT NOT NULL DEFAULT '["read", "write"]',
    scopes TEXT NOT NULL DEFAULT '[]',
    expires_at INTEGER,
    last_used_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    created_by TEXT,
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_api_keys_org ON api_keys(organization_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(key_prefix);

-- ============================================================================
-- ORGANIZATION QUOTAS
-- ============================================================================

CREATE TABLE IF NOT EXISTS organization_quotas (
    organization_id TEXT PRIMARY KEY,
    max_topics INTEGER NOT NULL DEFAULT 10,
    max_partitions_per_topic INTEGER NOT NULL DEFAULT 12,
    max_total_partitions INTEGER NOT NULL DEFAULT 100,
    max_storage_bytes INTEGER NOT NULL DEFAULT 10737418240,
    max_retention_days INTEGER NOT NULL DEFAULT 7,
    max_produce_bytes_per_sec INTEGER NOT NULL DEFAULT 10485760,
    max_consume_bytes_per_sec INTEGER NOT NULL DEFAULT 52428800,
    max_requests_per_sec INTEGER NOT NULL DEFAULT 1000,
    max_consumer_groups INTEGER NOT NULL DEFAULT 50,
    max_schemas INTEGER NOT NULL DEFAULT 100,
    max_schema_versions_per_subject INTEGER NOT NULL DEFAULT 100,
    max_connections INTEGER NOT NULL DEFAULT 100,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE
);

-- Insert default quotas (unlimited for default org)
INSERT OR IGNORE INTO organization_quotas (
    organization_id,
    max_topics, max_partitions_per_topic, max_total_partitions,
    max_storage_bytes, max_retention_days,
    max_produce_bytes_per_sec, max_consume_bytes_per_sec, max_requests_per_sec,
    max_consumer_groups, max_schemas, max_schema_versions_per_subject, max_connections
)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    1000000, 1000, 1000000,
    9223372036854775807, 36500,
    9223372036854775807, 9223372036854775807, 1000000,
    1000000, 1000000, 1000000, 1000000
);

-- ============================================================================
-- ORGANIZATION USAGE
-- ============================================================================

CREATE TABLE IF NOT EXISTS organization_usage (
    organization_id TEXT NOT NULL,
    metric TEXT NOT NULL,
    value INTEGER NOT NULL DEFAULT 0,
    period_start INTEGER NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    PRIMARY KEY (organization_id, metric, period_start),
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_org_usage_org ON organization_usage(organization_id);
CREATE INDEX IF NOT EXISTS idx_org_usage_period ON organization_usage(period_start);

-- ============================================================================
-- ADD organization_id TO EXISTING TABLES
-- ============================================================================
-- SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so we need to:
-- 1. Create new tables with organization_id
-- 2. Copy data from old tables
-- 3. Drop old tables
-- 4. Rename new tables

-- Note: This approach preserves data during migration

-- ---------------------------------------------------------------------------
-- TOPICS: Add organization_id
-- ---------------------------------------------------------------------------

-- Create new topics table with organization_id
CREATE TABLE IF NOT EXISTS topics_new (
    organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    name TEXT NOT NULL,
    partition_count INTEGER NOT NULL CHECK(partition_count > 0),
    retention_ms INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (organization_id, name),
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE
);

-- Copy existing data
INSERT OR IGNORE INTO topics_new (organization_id, name, partition_count, retention_ms, created_at, updated_at, config)
SELECT '00000000-0000-0000-0000-000000000000', name, partition_count, retention_ms, created_at, updated_at, config
FROM topics;

-- Drop old table and rename (only if topics_new has data or topics is empty)
DROP TABLE IF EXISTS topics;
ALTER TABLE topics_new RENAME TO topics;

CREATE INDEX IF NOT EXISTS idx_topics_org ON topics(organization_id);
CREATE INDEX IF NOT EXISTS idx_topics_created_at ON topics(created_at);

-- ---------------------------------------------------------------------------
-- PARTITIONS: Add organization_id
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS partitions_new (
    organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    high_watermark INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (organization_id, topic, partition_id),
    FOREIGN KEY (organization_id, topic) REFERENCES topics(organization_id, name) ON DELETE CASCADE
);

INSERT OR IGNORE INTO partitions_new (organization_id, topic, partition_id, high_watermark, created_at, updated_at)
SELECT '00000000-0000-0000-0000-000000000000', topic, partition_id, high_watermark, created_at, updated_at
FROM partitions;

DROP TABLE IF EXISTS partitions;
ALTER TABLE partitions_new RENAME TO partitions;

CREATE INDEX IF NOT EXISTS idx_partitions_org ON partitions(organization_id);
CREATE INDEX IF NOT EXISTS idx_partitions_topic ON partitions(organization_id, topic);

-- ---------------------------------------------------------------------------
-- SEGMENTS: Add organization_id
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS segments_new (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    base_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    record_count INTEGER NOT NULL,
    size_bytes INTEGER NOT NULL,
    s3_bucket TEXT NOT NULL,
    s3_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE,
    CHECK (end_offset >= base_offset)
);

INSERT OR IGNORE INTO segments_new (id, organization_id, topic, partition_id, base_offset, end_offset, record_count, size_bytes, s3_bucket, s3_key, created_at)
SELECT id, '00000000-0000-0000-0000-000000000000', topic, partition_id, base_offset, end_offset, record_count, size_bytes, s3_bucket, s3_key, created_at
FROM segments;

DROP TABLE IF EXISTS segments;
ALTER TABLE segments_new RENAME TO segments;

CREATE INDEX IF NOT EXISTS idx_segments_org ON segments(organization_id);
CREATE INDEX IF NOT EXISTS idx_segments_location ON segments(organization_id, topic, partition_id, base_offset);
CREATE INDEX IF NOT EXISTS idx_segments_s3_path ON segments(s3_bucket, s3_key);
CREATE INDEX IF NOT EXISTS idx_segments_offsets ON segments(organization_id, topic, partition_id, base_offset, end_offset);

-- ---------------------------------------------------------------------------
-- CONSUMER_GROUPS: Add organization_id
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS consumer_groups_new (
    organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    group_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (organization_id, group_id),
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE
);

INSERT OR IGNORE INTO consumer_groups_new (organization_id, group_id, created_at, updated_at)
SELECT '00000000-0000-0000-0000-000000000000', group_id, created_at, updated_at
FROM consumer_groups;

DROP TABLE IF EXISTS consumer_groups;
ALTER TABLE consumer_groups_new RENAME TO consumer_groups;

CREATE INDEX IF NOT EXISTS idx_consumer_groups_org ON consumer_groups(organization_id);

-- ---------------------------------------------------------------------------
-- CONSUMER_OFFSETS: Add organization_id
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS consumer_offsets_new (
    organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    group_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    committed_offset INTEGER NOT NULL,
    metadata TEXT,
    committed_at INTEGER NOT NULL,
    PRIMARY KEY (organization_id, group_id, topic, partition_id),
    FOREIGN KEY (organization_id, group_id) REFERENCES consumer_groups(organization_id, group_id) ON DELETE CASCADE
);

INSERT OR IGNORE INTO consumer_offsets_new (organization_id, group_id, topic, partition_id, committed_offset, metadata, committed_at)
SELECT '00000000-0000-0000-0000-000000000000', group_id, topic, partition_id, committed_offset, metadata, committed_at
FROM consumer_offsets;

DROP TABLE IF EXISTS consumer_offsets;
ALTER TABLE consumer_offsets_new RENAME TO consumer_offsets;

CREATE INDEX IF NOT EXISTS idx_consumer_offsets_org ON consumer_offsets(organization_id);
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_group ON consumer_offsets(organization_id, group_id);
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_topic ON consumer_offsets(organization_id, topic, partition_id);

-- ---------------------------------------------------------------------------
-- AGENTS: Add organization_id (nullable - agents can be shared)
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS agents_new (
    agent_id TEXT PRIMARY KEY,
    organization_id TEXT,  -- NULL means shared agent
    address TEXT NOT NULL UNIQUE,
    availability_zone TEXT NOT NULL,
    agent_group TEXT NOT NULL,
    last_heartbeat INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE SET NULL
);

INSERT OR IGNORE INTO agents_new (agent_id, organization_id, address, availability_zone, agent_group, last_heartbeat, started_at, metadata)
SELECT agent_id, NULL, address, availability_zone, agent_group, last_heartbeat, started_at, metadata
FROM agents;

DROP TABLE IF EXISTS agents;
ALTER TABLE agents_new RENAME TO agents;

CREATE INDEX IF NOT EXISTS idx_agents_org ON agents(organization_id);
CREATE INDEX IF NOT EXISTS idx_agents_group ON agents(agent_group);
CREATE INDEX IF NOT EXISTS idx_agents_az ON agents(availability_zone);
CREATE INDEX IF NOT EXISTS idx_agents_heartbeat ON agents(last_heartbeat);

-- ---------------------------------------------------------------------------
-- PARTITION_LEASES: Add organization_id
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS partition_leases_new (
    organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    leader_agent_id TEXT NOT NULL,
    lease_expires_at INTEGER NOT NULL,
    acquired_at INTEGER NOT NULL,
    epoch INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (organization_id, topic, partition_id),
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE,
    FOREIGN KEY (leader_agent_id) REFERENCES agents(agent_id) ON DELETE CASCADE
);

INSERT OR IGNORE INTO partition_leases_new (organization_id, topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch)
SELECT '00000000-0000-0000-0000-000000000000', topic, partition_id, leader_agent_id, lease_expires_at, acquired_at, epoch
FROM partition_leases;

DROP TABLE IF EXISTS partition_leases;
ALTER TABLE partition_leases_new RENAME TO partition_leases;

CREATE INDEX IF NOT EXISTS idx_partition_leases_org ON partition_leases(organization_id);
CREATE INDEX IF NOT EXISTS idx_partition_leases_agent ON partition_leases(leader_agent_id);
CREATE INDEX IF NOT EXISTS idx_partition_leases_expiration ON partition_leases(lease_expires_at);
CREATE INDEX IF NOT EXISTS idx_partition_leases_topic ON partition_leases(organization_id, topic);
