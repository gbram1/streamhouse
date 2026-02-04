-- Fast Leader Handoff Migration (Phase 17 - PostgreSQL)
-- Adds tables for tracking lease transfers and leadership changes.

-- Lease transfers for graceful handoff
CREATE TABLE IF NOT EXISTS lease_transfers (
    transfer_id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    from_agent_id TEXT NOT NULL,
    to_agent_id TEXT NOT NULL,
    from_epoch BIGINT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    reason TEXT NOT NULL,
    initiated_at BIGINT NOT NULL,
    completed_at BIGINT,
    timeout_at BIGINT NOT NULL,
    last_flushed_offset BIGINT,
    high_watermark BIGINT,
    error TEXT
);

-- Index for finding transfers by topic/partition
CREATE INDEX IF NOT EXISTS idx_lease_transfers_partition
ON lease_transfers(topic, partition_id, state);

-- Index for finding transfers by agent
CREATE INDEX IF NOT EXISTS idx_lease_transfers_from_agent
ON lease_transfers(from_agent_id, state);

CREATE INDEX IF NOT EXISTS idx_lease_transfers_to_agent
ON lease_transfers(to_agent_id, state);

-- Index for cleanup of timed out transfers
CREATE INDEX IF NOT EXISTS idx_lease_transfers_timeout
ON lease_transfers(timeout_at, state);

-- Leader change history for metrics and debugging
CREATE TABLE IF NOT EXISTS leader_changes (
    id BIGSERIAL PRIMARY KEY,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    from_agent_id TEXT,
    to_agent_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    epoch BIGINT NOT NULL,
    gap_ms BIGINT NOT NULL DEFAULT 0,
    changed_at BIGINT NOT NULL
);

-- Index for querying leader changes by partition
CREATE INDEX IF NOT EXISTS idx_leader_changes_partition
ON leader_changes(topic, partition_id, changed_at);

-- Index for querying leader changes by reason (for metrics)
CREATE INDEX IF NOT EXISTS idx_leader_changes_reason
ON leader_changes(reason, changed_at);
