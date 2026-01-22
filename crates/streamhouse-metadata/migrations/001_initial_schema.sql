-- Initial Database Schema for StreamHouse Metadata Store
--
-- This migration creates the core tables for tracking:
-- - Topics: Stream definitions with configuration
-- - Partitions: Divisions of topics with high watermark tracking
-- - Segments: S3 file locations and offset ranges
-- - Consumer Groups: Consumer offset tracking
--
-- Schema Design Principles:
-- - Foreign keys for referential integrity (CASCADE deletes)
-- - Indexes on all query patterns for fast lookups
-- - CHECK constraints for data validation
-- - Timestamps as BIGINT (milliseconds since epoch)
-- - JSON/TEXT for flexible configuration
--
-- Performance Targets:
-- - Get topic by name: < 100µs (primary key)
-- - Find segment for offset: < 1ms (indexed)
-- - Get consumer offset: < 100µs (composite primary key)
--
-- Migration: 001_initial_schema.sql
-- Version: 1
-- Created: 2026-01-21

-- ============================================================
-- TOPICS
-- ============================================================
CREATE TABLE IF NOT EXISTS topics (
    name TEXT PRIMARY KEY,
    partition_count INTEGER NOT NULL CHECK(partition_count > 0),
    retention_ms BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_topics_created_at ON topics(created_at);

-- ============================================================
-- PARTITIONS
-- ============================================================
CREATE TABLE IF NOT EXISTS partitions (
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    high_watermark BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (topic) REFERENCES topics(name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_partitions_topic ON partitions(topic);

-- ============================================================
-- SEGMENTS
-- ============================================================
CREATE TABLE IF NOT EXISTS segments (
    id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    base_offset BIGINT NOT NULL,
    end_offset BIGINT NOT NULL,
    record_count INTEGER NOT NULL,
    size_bytes BIGINT NOT NULL,
    s3_bucket TEXT NOT NULL,
    s3_key TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE,
    CHECK (end_offset >= base_offset)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_segments_location ON segments(topic, partition_id, base_offset);
CREATE INDEX IF NOT EXISTS idx_segments_s3_path ON segments(s3_bucket, s3_key);
CREATE INDEX IF NOT EXISTS idx_segments_offsets ON segments(topic, partition_id, base_offset, end_offset);
CREATE INDEX IF NOT EXISTS idx_segments_created_at ON segments(created_at);

-- ============================================================
-- CONSUMER GROUPS & OFFSETS
-- ============================================================
CREATE TABLE IF NOT EXISTS consumer_groups (
    group_id TEXT PRIMARY KEY,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS consumer_offsets (
    group_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    committed_offset BIGINT NOT NULL,
    metadata TEXT,
    committed_at BIGINT NOT NULL,
    PRIMARY KEY (group_id, topic, partition_id),
    FOREIGN KEY (group_id) REFERENCES consumer_groups(group_id) ON DELETE CASCADE,
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_consumer_offsets_group ON consumer_offsets(group_id);
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_topic ON consumer_offsets(topic, partition_id);
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_committed_at ON consumer_offsets(committed_at);
