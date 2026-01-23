-- ============================================================================
-- StreamHouse Agent Coordination Schema (PostgreSQL)
-- ============================================================================
--
-- Migration: 002_agent_coordination.sql
-- Backend: PostgreSQL
-- Version: 2
-- Created: 2026-01-22
-- Phase: 4 Preparation (Multi-Agent Architecture)
--
-- ## Purpose
--
-- Adds tables for distributed agent coordination in Phase 4. These tables
-- enable multiple StreamHouse agents to coordinate writes to the same topic
-- without requiring complex consensus protocols like Raft or Paxos.
--
-- ## Why Added in Phase 3.2?
--
-- Including these tables now avoids a future schema migration when Phase 4
-- is implemented. They have zero runtime overhead if unused.
--
-- ## Tables Created
--
-- 1. **agents**: Agent registration and heartbeat tracking
-- 2. **partition_leases**: Partition leadership leases with epoch fencing
--
-- ## Agent Coordination Model
--
-- StreamHouse uses a **lease-based coordination** model:
--
-- 1. Agents are **stateless** - can be killed and restarted anytime
-- 2. Each partition has at most ONE leader at a time
-- 3. Leadership is acquired via **time-based leases** (e.g., 30 seconds)
-- 4. Agents must renew leases before expiration or lose leadership
-- 5. **Epoch fencing** prevents split-brain scenarios
--
-- ## Coordination Flow Example
--
-- Agent Startup:
--   Agent-1 starts → register_agent("agent-1", address="10.0.1.5:9090")
--   Agent-1 → acquire_partition_lease("orders", partition=0, duration=30s)
--   → Lease granted with epoch=1
--
-- Agent Heartbeat:
--   Every 10s → register_agent(..., last_heartbeat=now())
--   Every 20s → renew lease (extends expiration, increments epoch)
--
-- Agent Failure:
--   Agent-1 crashes → heartbeat stops updating
--   After 60s → list_agents() excludes agent-1 (stale)
--   After 30s → lease expires automatically
--   Agent-2 → acquire_partition_lease("orders", partition=0)
--   → Lease granted with epoch=2 (fencing token)
--
-- ## Epoch Fencing
--
-- Prevents "zombie" agents from corrupting data after losing leadership:
--
-- 1. Agent-1 acquires lease with epoch=1
-- 2. Agent-1 experiences network partition
-- 3. Lease expires, Agent-2 acquires with epoch=2
-- 4. Agent-1 recovers, tries to write with epoch=1
-- 5. Write REJECTED (epoch too old)
--
-- ## Comparison to Other Systems
--
-- | System | Coordination Mechanism | Complexity |
-- |--------|------------------------|------------|
-- | Kafka | ZooKeeper (Raft-like) | High |
-- | Pulsar | BookKeeper + metadata store | High |
-- | **StreamHouse** | PostgreSQL leases + epochs | Low |
--
-- ## Why Not Raft/Paxos?
--
-- StreamHouse chooses lease-based coordination because:
-- - Simpler to implement and understand
-- - PostgreSQL provides ACID transactions for CAS operations
-- - Failover is fast (lease timeout, typically 10-30s)
-- - No need for quorum (N/2+1) - any agent can acquire expired lease
-- - Sufficient for S3-native architecture (data already durable)
--
-- ## Performance Characteristics
--
-- - Lease acquisition: < 50ms (single INSERT with ON CONFLICT)
-- - Lease renewal: < 50ms (UPDATE with CAS check)
-- - Stale agent detection: O(N) scan with heartbeat filter
-- - Failover latency: lease_duration (configurable, default 30s)

-- ============================================================
-- AGENTS
-- ============================================================
CREATE TABLE IF NOT EXISTS agents (
    agent_id TEXT PRIMARY KEY,
    address TEXT NOT NULL UNIQUE,
    availability_zone TEXT NOT NULL,
    agent_group TEXT NOT NULL,
    last_heartbeat BIGINT NOT NULL,
    started_at BIGINT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
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
