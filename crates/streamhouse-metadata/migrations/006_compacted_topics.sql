-- Migration 006: Add cleanup_policy for compacted topics and wildcard subscriptions
--
-- This migration adds support for:
-- 1. Compacted topics (cleanup_policy: delete, compact, compact,delete)
-- 2. Index for wildcard pattern matching on topic names

-- Add cleanup_policy column to topics table
ALTER TABLE topics ADD COLUMN cleanup_policy TEXT NOT NULL DEFAULT 'delete';

-- Create index for wildcard subscription pattern matching
CREATE INDEX IF NOT EXISTS idx_topics_name_pattern ON topics(name);

-- Create compaction state table to track compaction progress per partition
CREATE TABLE IF NOT EXISTS compaction_state (
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    last_compacted_offset BIGINT NOT NULL DEFAULT 0,
    last_compaction_at BIGINT,
    compacted_segments INTEGER NOT NULL DEFAULT 0,
    keys_removed BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE
);

-- Index for finding partitions needing compaction
CREATE INDEX IF NOT EXISTS idx_compaction_state_last_at ON compaction_state(last_compaction_at);
