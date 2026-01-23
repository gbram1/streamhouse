-- Agent Coordination Schema (Phase 4 - Multi-Agent Architecture)
--
-- This migration adds tables for stateless agent coordination:
-- - Agents: Registration and heartbeat tracking
-- - Partition Leases: Leadership leases for partition ownership
--
-- Design Principles:
-- - Agents are stateless and ephemeral
-- - Leadership uses lease-based consensus (no Raft/Paxos complexity)
-- - Stale agents are identified by heartbeat timeout
-- - Lease expiration enables automatic failover
--
-- Migration: 002_agent_coordination.sql
-- Version: 2
-- Created: 2026-01-22

-- ============================================================
-- AGENTS
-- ============================================================
CREATE TABLE IF NOT EXISTS agents (
    agent_id TEXT PRIMARY KEY,
    address TEXT NOT NULL,
    availability_zone TEXT NOT NULL,
    agent_group TEXT NOT NULL,
    last_heartbeat BIGINT NOT NULL,
    started_at BIGINT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    UNIQUE(address)
);

-- Index for listing agents by group/AZ
CREATE INDEX IF NOT EXISTS idx_agents_group ON agents(agent_group);
CREATE INDEX IF NOT EXISTS idx_agents_az ON agents(availability_zone);
CREATE INDEX IF NOT EXISTS idx_agents_heartbeat ON agents(last_heartbeat);

-- ============================================================
-- PARTITION LEASES
-- ============================================================
CREATE TABLE IF NOT EXISTS partition_leases (
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    leader_agent_id TEXT NOT NULL,
    lease_expires_at BIGINT NOT NULL,
    acquired_at BIGINT NOT NULL,
    epoch BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE,
    FOREIGN KEY (leader_agent_id) REFERENCES agents(agent_id) ON DELETE CASCADE
);

-- Index for finding leases by agent
CREATE INDEX IF NOT EXISTS idx_partition_leases_agent ON partition_leases(leader_agent_id);

-- Index for finding expired leases (cleanup job)
CREATE INDEX IF NOT EXISTS idx_partition_leases_expiration ON partition_leases(lease_expires_at);

-- Index for querying leases by topic
CREATE INDEX IF NOT EXISTS idx_partition_leases_topic ON partition_leases(topic);
