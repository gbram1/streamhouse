-- Migration 010: Fix organizations timestamps and consumer_offsets primary key
--
-- Two bugs introduced in migration 004:
--
-- 1. organizations.created_at/updated_at use TIMESTAMPTZ but the Rust code
--    binds i64 milliseconds (consistent with every other table). Convert to BIGINT.
--
-- 2. consumer_offsets PK was never updated to include organization_id,
--    but commit_offset() uses ON CONFLICT (organization_id, group_id, topic, partition_id).
--    Add the missing unique constraint.

-- ============================================================================
-- FIX 1: organizations timestamps — TIMESTAMPTZ → BIGINT
-- ============================================================================

-- Drop the TIMESTAMPTZ defaults first (they can't be auto-cast to BIGINT)
ALTER TABLE organizations ALTER COLUMN created_at DROP DEFAULT;
ALTER TABLE organizations ALTER COLUMN updated_at DROP DEFAULT;

-- Convert existing TIMESTAMPTZ values to epoch milliseconds and change column type
ALTER TABLE organizations
    ALTER COLUMN created_at TYPE BIGINT
    USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

ALTER TABLE organizations
    ALTER COLUMN updated_at TYPE BIGINT
    USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- Set new defaults using the i64 millis pattern (consistent with every other table)
ALTER TABLE organizations ALTER COLUMN created_at SET DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
ALTER TABLE organizations ALTER COLUMN updated_at SET DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;

-- ============================================================================
-- FIX 2: consumer_offsets — add unique constraint for ON CONFLICT
-- ============================================================================

-- Drop the old PK that doesn't include organization_id
ALTER TABLE consumer_offsets DROP CONSTRAINT IF EXISTS consumer_offsets_pkey;

-- Create new PK that matches the ON CONFLICT clause in commit_offset()
ALTER TABLE consumer_offsets
    ADD CONSTRAINT consumer_offsets_pkey
    PRIMARY KEY (organization_id, group_id, topic, partition_id);
