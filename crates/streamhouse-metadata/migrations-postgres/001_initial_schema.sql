-- ============================================================================
-- StreamHouse Metadata Store - Initial Schema (PostgreSQL)
-- ============================================================================
--
-- Migration: 001_initial_schema.sql
-- Backend: PostgreSQL
-- Version: 1
-- Created: 2026-01-22
-- Phase: 3.2 (PostgreSQL Backend)
--
-- ## Purpose
--
-- Creates the core metadata tables for StreamHouse's PostgreSQL backend.
-- This schema stores METADATA ONLY - actual event data lives in S3.
--
-- ## What This Schema Does NOT Store
--
-- ❌ Actual streaming events/records (those are in S3 segments)
-- ❌ Event payloads or message bodies
-- ❌ Large binary data
--
-- ## What This Schema DOES Store
--
-- ✅ Topic definitions and configurations (which topics exist)
-- ✅ Partition metadata (how many partitions, high watermark offsets)
-- ✅ Segment locations (where S3 files are located)
-- ✅ Consumer group progress (which offset each group has consumed)
--
-- ## Tables Created
--
-- 1. **topics**: Stream definitions with partition count and retention policy
-- 2. **partitions**: Per-partition state tracking with high watermark offsets
-- 3. **segments**: Immutable S3 segment file locations and offset ranges
-- 4. **consumer_groups**: Consumer group registration
-- 5. **consumer_offsets**: Consumer progress tracking (group/topic/partition)
--
-- ## Design Principles
--
-- - **Immutable Segments**: Once written to S3, segment metadata never changes
-- - **High Watermark**: Tracks latest offset written to each partition
-- - **JSONB for Configs**: Flexible topic/group configs without schema changes
-- - **Cascading Deletes**: Foreign keys ensure cleanup when topics are deleted
-- - **Indexed Queries**: Fast lookups for offset ranges and consumer positions
--
-- ## Data Flow Example
--
-- Write Path:
--   Producer → StreamHouse Agent → Segment Writer → S3
--                                         ↓
--                                   PostgreSQL (this schema):
--                                   - add_segment("s3://bucket/seg.bin", offsets 0-999)
--                                   - update_high_watermark(partition=0, offset=1000)
--
-- Read Path:
--   Consumer → PostgreSQL: "Which segment contains offset 500?"
--           ↓
--           Returns: SegmentInfo { s3_key: "topic/0/00000000.seg", base_offset: 0 }
--           ↓
--           Fetch from S3 → Decompress → Return records
--
-- ## PostgreSQL-Specific Features
--
-- - JSONB for efficient config storage (binary format, indexable)
-- - ON DELETE CASCADE for referential integrity
-- - Composite indexes for multi-column queries (topic + partition + offset)
-- - CHECK constraints for data validation (partition_count > 0)
--
-- ## Performance Targets
--
-- - Get topic by name: < 1ms (primary key lookup)
-- - Find segment for offset: < 10ms (indexed range query)
-- - Get consumer offset: < 1ms (composite primary key)
-- - List 10K partitions: < 5s (sequential scan)

-- ============================================================
-- TOPICS
-- ============================================================
CREATE TABLE IF NOT EXISTS topics (
    name TEXT PRIMARY KEY,
    partition_count INTEGER NOT NULL CHECK(partition_count > 0),
    retention_ms BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb
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
