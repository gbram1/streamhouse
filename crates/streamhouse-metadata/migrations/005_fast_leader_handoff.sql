-- Fast Leader Handoff Migration (Phase 17)
-- Adds tables for tracking lease transfers and leadership changes.

-- Lease transfers for graceful handoff
CREATE TABLE IF NOT EXISTS lease_transfers (
    transfer_id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    from_agent_id TEXT NOT NULL,
    to_agent_id TEXT NOT NULL,
    from_epoch INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    reason TEXT NOT NULL,
    initiated_at INTEGER NOT NULL,
    completed_at INTEGER,
    timeout_at INTEGER NOT NULL,
    last_flushed_offset INTEGER,
    high_watermark INTEGER,
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
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    from_agent_id TEXT,
    to_agent_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    epoch INTEGER NOT NULL,
    gap_ms INTEGER NOT NULL DEFAULT 0,
    changed_at INTEGER NOT NULL
);

-- Index for querying leader changes by partition
CREATE INDEX IF NOT EXISTS idx_leader_changes_partition
ON leader_changes(topic, partition_id, changed_at);

-- Index for querying leader changes by reason (for metrics)
CREATE INDEX IF NOT EXISTS idx_leader_changes_reason
ON leader_changes(reason, changed_at);
