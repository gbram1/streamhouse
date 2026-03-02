-- Migration 011: Fix partitions primary key for multi-tenancy
--
-- Migration 004 changed topics PK to (organization_id, name) but forgot to
-- update partitions PK from (topic, partition_id) to (organization_id, topic, partition_id).
-- This means two orgs cannot have the same topic name — the second org's partition
-- inserts fail with a PK violation.
--
-- Also adds organization_id to compaction_state (missed in migration 008).

-- ============================================================================
-- STEP 1: Add organization_id to compaction_state
-- ============================================================================

ALTER TABLE compaction_state ADD COLUMN IF NOT EXISTS organization_id UUID;
UPDATE compaction_state SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
ALTER TABLE compaction_state ALTER COLUMN organization_id SET NOT NULL;

-- ============================================================================
-- STEP 2: Drop all foreign keys referencing partitions(topic, partition_id)
-- ============================================================================

-- partition_leases (from 002, auto-named)
ALTER TABLE partition_leases DROP CONSTRAINT IF EXISTS partition_leases_topic_partition_id_fkey;
ALTER TABLE partition_leases DROP CONSTRAINT IF EXISTS partition_leases_topic_fkey;

-- segments (recreated in 004)
ALTER TABLE segments DROP CONSTRAINT IF EXISTS segments_topic_partition_id_fkey;

-- consumer_offsets (recreated in 004)
ALTER TABLE consumer_offsets DROP CONSTRAINT IF EXISTS consumer_offsets_topic_partition_id_fkey;

-- compaction_state (from 008, auto-named)
ALTER TABLE compaction_state DROP CONSTRAINT IF EXISTS compaction_state_topic_partition_id_fkey;
ALTER TABLE compaction_state DROP CONSTRAINT IF EXISTS compaction_state_pkey;

-- ============================================================================
-- STEP 3: Change partitions PK to include organization_id
-- ============================================================================

ALTER TABLE partitions DROP CONSTRAINT IF EXISTS partitions_pkey;
ALTER TABLE partitions ADD CONSTRAINT partitions_pkey
    PRIMARY KEY (organization_id, topic, partition_id);

-- ============================================================================
-- STEP 4: Recreate foreign keys with organization_id
-- ============================================================================

ALTER TABLE partition_leases ADD CONSTRAINT partition_leases_partition_fkey
    FOREIGN KEY (organization_id, topic, partition_id)
    REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE;

ALTER TABLE segments ADD CONSTRAINT segments_partition_fkey
    FOREIGN KEY (organization_id, topic, partition_id)
    REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE;

ALTER TABLE consumer_offsets ADD CONSTRAINT consumer_offsets_partition_fkey
    FOREIGN KEY (organization_id, topic, partition_id)
    REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE;

ALTER TABLE compaction_state ADD CONSTRAINT compaction_state_pkey
    PRIMARY KEY (organization_id, topic, partition_id);

ALTER TABLE compaction_state ADD CONSTRAINT compaction_state_partition_fkey
    FOREIGN KEY (organization_id, topic, partition_id)
    REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE;
