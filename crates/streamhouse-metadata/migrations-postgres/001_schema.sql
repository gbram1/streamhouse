-- ============================================================================
-- StreamHouse Metadata Store - Consolidated Schema (PostgreSQL)
-- ============================================================================
--
-- This is the complete schema for the StreamHouse metadata store, consolidated
-- from migrations 001-019. It represents the final state and can be applied to
-- a fresh database.
--
-- Tables:
--   Core:        organizations, topics, partitions, segments
--   Consumer:    consumer_groups, consumer_offsets, consumer_group_config
--   Agents:      agents, partition_leases, lease_transfers, leader_changes
--   EOS:         producers, producer_sequences, transactions,
--                transaction_partitions, transaction_markers, partition_lso
--   Compaction:  compaction_state
--   Connectors:  connectors
--   Pipelines:   pipeline_targets, pipelines
--   Schema Reg:  schema_registry_schemas, schema_registry_versions,
--                schema_registry_subject_config, schema_registry_global_config
--   Mat. Views:  materialized_views, materialized_view_offsets,
--                materialized_view_data
--   Auth/Quota:  api_keys, organization_quotas, organization_usage
--
-- ============================================================================

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- ORGANIZATIONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(63) NOT NULL UNIQUE,
    external_id VARCHAR(255) UNIQUE,
    plan VARCHAR(50) NOT NULL DEFAULT 'free',
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    deployment_mode VARCHAR(20) NOT NULL DEFAULT 'self_hosted',
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    settings JSONB NOT NULL DEFAULT '{}',

    CONSTRAINT valid_slug CHECK (slug ~ '^[a-z0-9][a-z0-9-]*[a-z0-9]$' OR slug ~ '^[a-z0-9]$'),
    CONSTRAINT valid_status CHECK (status IN ('active', 'suspended', 'deleted')),
    CONSTRAINT valid_plan CHECK (plan IN ('free', 'pro', 'enterprise')),
    CONSTRAINT valid_deployment_mode CHECK (deployment_mode IN ('self_hosted', 'byoc', 'managed'))
);

CREATE INDEX IF NOT EXISTS idx_organizations_slug ON organizations(slug);
CREATE INDEX IF NOT EXISTS idx_organizations_status ON organizations(status);
CREATE INDEX IF NOT EXISTS idx_organizations_plan ON organizations(plan);
CREATE INDEX IF NOT EXISTS idx_organizations_created_at ON organizations(created_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_organizations_external_id ON organizations(external_id) WHERE external_id IS NOT NULL;

-- Default organization for backwards compatibility
INSERT INTO organizations (id, name, slug, plan, status)
VALUES ('00000000-0000-0000-0000-000000000000', 'Default Organization', 'default', 'enterprise', 'active')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- TOPICS
-- ============================================================================

CREATE TABLE IF NOT EXISTS topics (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    partition_count INTEGER NOT NULL CHECK(partition_count > 0),
    retention_ms BIGINT,
    cleanup_policy TEXT NOT NULL DEFAULT 'delete',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_topics_created_at ON topics(created_at);
CREATE INDEX IF NOT EXISTS idx_topics_org ON topics(organization_id);
CREATE INDEX IF NOT EXISTS idx_topics_name_pattern ON topics(name text_pattern_ops);

-- ============================================================================
-- PARTITIONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS partitions (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    high_watermark BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (organization_id, topic, partition_id),
    FOREIGN KEY (organization_id, topic) REFERENCES topics(organization_id, name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_partitions_topic ON partitions(topic);
CREATE INDEX IF NOT EXISTS idx_partitions_org ON partitions(organization_id);

-- ============================================================================
-- SEGMENTS
-- ============================================================================

CREATE TABLE IF NOT EXISTS segments (
    id TEXT PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    base_offset BIGINT NOT NULL,
    end_offset BIGINT NOT NULL,
    record_count INTEGER NOT NULL,
    size_bytes BIGINT NOT NULL,
    s3_bucket TEXT NOT NULL,
    s3_key TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    min_timestamp BIGINT NOT NULL,
    max_timestamp BIGINT NOT NULL,
    FOREIGN KEY (organization_id, topic, partition_id)
        REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE,
    CHECK (end_offset >= base_offset)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_segments_location ON segments(topic, partition_id, base_offset);
CREATE INDEX IF NOT EXISTS idx_segments_s3_path ON segments(s3_bucket, s3_key);
CREATE INDEX IF NOT EXISTS idx_segments_offsets ON segments(topic, partition_id, base_offset, end_offset);
CREATE INDEX IF NOT EXISTS idx_segments_created_at ON segments(created_at);
CREATE INDEX IF NOT EXISTS idx_segments_org ON segments(organization_id);

-- ============================================================================
-- CONSUMER GROUPS
-- ============================================================================

CREATE TABLE IF NOT EXISTS consumer_groups (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (organization_id, group_id)
);

CREATE INDEX IF NOT EXISTS idx_consumer_groups_org ON consumer_groups(organization_id);

-- ============================================================================
-- CONSUMER OFFSETS
-- ============================================================================

CREATE TABLE IF NOT EXISTS consumer_offsets (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    committed_offset BIGINT NOT NULL,
    metadata TEXT,
    committed_at BIGINT NOT NULL,
    PRIMARY KEY (organization_id, group_id, topic, partition_id),
    FOREIGN KEY (organization_id, group_id)
        REFERENCES consumer_groups(organization_id, group_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, topic, partition_id)
        REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_consumer_offsets_group ON consumer_offsets(group_id);
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_topic ON consumer_offsets(topic, partition_id);
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_committed_at ON consumer_offsets(committed_at);
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_org ON consumer_offsets(organization_id);

-- ============================================================================
-- CONSUMER GROUP CONFIG (Isolation Level)
-- ============================================================================

CREATE TABLE IF NOT EXISTS consumer_group_config (
    group_id TEXT PRIMARY KEY,
    isolation_level TEXT NOT NULL DEFAULT 'read_uncommitted',
    enable_auto_commit BOOLEAN NOT NULL DEFAULT true,
    auto_commit_interval_ms INTEGER NOT NULL DEFAULT 5000,
    session_timeout_ms INTEGER NOT NULL DEFAULT 30000,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

-- ============================================================================
-- AGENTS
-- ============================================================================

CREATE TABLE IF NOT EXISTS agents (
    agent_id TEXT PRIMARY KEY,
    organization_id UUID REFERENCES organizations(id) ON DELETE SET NULL,
    address TEXT NOT NULL UNIQUE,
    availability_zone TEXT NOT NULL,
    agent_group TEXT NOT NULL,
    last_heartbeat BIGINT NOT NULL,
    started_at BIGINT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_agents_group ON agents(agent_group);
CREATE INDEX IF NOT EXISTS idx_agents_az ON agents(availability_zone);
CREATE INDEX IF NOT EXISTS idx_agents_heartbeat ON agents(last_heartbeat);
CREATE INDEX IF NOT EXISTS idx_agents_org ON agents(organization_id);

-- ============================================================================
-- PARTITION LEASES
-- ============================================================================

CREATE TABLE IF NOT EXISTS partition_leases (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    leader_agent_id TEXT NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    lease_expires_at BIGINT NOT NULL,
    acquired_at BIGINT NOT NULL,
    epoch BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (organization_id, topic, partition_id)
        REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_partition_leases_agent ON partition_leases(leader_agent_id);
CREATE INDEX IF NOT EXISTS idx_partition_leases_expiration ON partition_leases(lease_expires_at);
CREATE INDEX IF NOT EXISTS idx_partition_leases_topic ON partition_leases(topic);
CREATE INDEX IF NOT EXISTS idx_partition_leases_org ON partition_leases(organization_id);

-- ============================================================================
-- LEASE TRANSFERS
-- ============================================================================

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

CREATE INDEX IF NOT EXISTS idx_lease_transfers_partition ON lease_transfers(topic, partition_id, state);
CREATE INDEX IF NOT EXISTS idx_lease_transfers_from_agent ON lease_transfers(from_agent_id, state);
CREATE INDEX IF NOT EXISTS idx_lease_transfers_to_agent ON lease_transfers(to_agent_id, state);
CREATE INDEX IF NOT EXISTS idx_lease_transfers_timeout ON lease_transfers(timeout_at, state);

-- ============================================================================
-- LEADER CHANGES
-- ============================================================================

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

CREATE INDEX IF NOT EXISTS idx_leader_changes_partition ON leader_changes(topic, partition_id, changed_at);
CREATE INDEX IF NOT EXISTS idx_leader_changes_reason ON leader_changes(reason, changed_at);

-- ============================================================================
-- COMPACTION STATE
-- ============================================================================

CREATE TABLE IF NOT EXISTS compaction_state (
    organization_id UUID NOT NULL,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    last_compacted_offset BIGINT NOT NULL DEFAULT 0,
    last_compaction_at BIGINT,
    compacted_segments INTEGER NOT NULL DEFAULT 0,
    keys_removed BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (organization_id, topic, partition_id),
    FOREIGN KEY (organization_id, topic, partition_id)
        REFERENCES partitions(organization_id, topic, partition_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_compaction_state_last_at ON compaction_state(last_compaction_at);

-- ============================================================================
-- PRODUCERS (Idempotent / Exactly-Once)
-- ============================================================================

CREATE TABLE IF NOT EXISTS producers (
    id TEXT PRIMARY KEY,
    organization_id TEXT,
    transactional_id TEXT,
    epoch INTEGER NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    last_heartbeat BIGINT NOT NULL,
    state TEXT NOT NULL DEFAULT 'active',
    metadata TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_producers_org_txn
    ON producers(organization_id, transactional_id)
    WHERE transactional_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_producers_org ON producers(organization_id);
CREATE INDEX IF NOT EXISTS idx_producers_transactional ON producers(transactional_id);
CREATE INDEX IF NOT EXISTS idx_producers_state ON producers(state);

-- ============================================================================
-- PRODUCER SEQUENCES
-- ============================================================================

CREATE TABLE IF NOT EXISTS producer_sequences (
    producer_id TEXT NOT NULL REFERENCES producers(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    last_sequence BIGINT NOT NULL DEFAULT -1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (producer_id, topic, partition_id)
);

CREATE INDEX IF NOT EXISTS idx_producer_seq_topic ON producer_sequences(topic, partition_id);

-- ============================================================================
-- TRANSACTIONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS transactions (
    transaction_id TEXT PRIMARY KEY,
    producer_id TEXT NOT NULL REFERENCES producers(id) ON DELETE CASCADE,
    organization_id TEXT,
    state TEXT NOT NULL DEFAULT 'ongoing',
    timeout_ms INTEGER NOT NULL DEFAULT 60000,
    started_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    completed_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_transactions_producer ON transactions(producer_id);
CREATE INDEX IF NOT EXISTS idx_transactions_state ON transactions(state);
CREATE INDEX IF NOT EXISTS idx_transactions_completed ON transactions(completed_at) WHERE completed_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_transactions_org_state ON transactions(organization_id, state);

-- ============================================================================
-- TRANSACTION PARTITIONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS transaction_partitions (
    transaction_id TEXT NOT NULL REFERENCES transactions(transaction_id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    first_offset BIGINT NOT NULL,
    last_offset BIGINT NOT NULL,
    PRIMARY KEY (transaction_id, topic, partition_id)
);

CREATE INDEX IF NOT EXISTS idx_txn_partitions_topic ON transaction_partitions(topic, partition_id);

-- ============================================================================
-- TRANSACTION MARKERS
-- ============================================================================

CREATE TABLE IF NOT EXISTS transaction_markers (
    id TEXT PRIMARY KEY,
    transaction_id TEXT NOT NULL REFERENCES transactions(transaction_id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    "offset" BIGINT NOT NULL,
    marker_type TEXT NOT NULL,
    created_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_txn_markers_partition ON transaction_markers(topic, partition_id, "offset");

-- ============================================================================
-- PARTITION LSO (Last Stable Offset)
-- ============================================================================

CREATE TABLE IF NOT EXISTS partition_lso (
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    last_stable_offset BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (topic, partition_id)
);

CREATE INDEX IF NOT EXISTS idx_partition_lso_topic ON partition_lso(topic);

-- ============================================================================
-- CONNECTORS (org-scoped)
-- ============================================================================

CREATE TABLE IF NOT EXISTS connectors (
    organization_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'
        REFERENCES organizations(id),
    name TEXT NOT NULL,
    connector_type TEXT NOT NULL CHECK (connector_type IN ('source', 'sink')),
    connector_class TEXT NOT NULL,
    topics TEXT NOT NULL,
    config TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'stopped' CHECK (state IN ('running', 'paused', 'stopped', 'failed')),
    error_message TEXT,
    records_processed BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (organization_id, name)
);

-- ============================================================================
-- PIPELINE TARGETS
-- ============================================================================

CREATE TABLE IF NOT EXISTS pipeline_targets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    target_type TEXT NOT NULL,
    connection_config TEXT NOT NULL DEFAULT '{}',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE(organization_id, name)
);

-- ============================================================================
-- PIPELINES
-- ============================================================================

CREATE TABLE IF NOT EXISTS pipelines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    source_topic TEXT NOT NULL,
    consumer_group TEXT NOT NULL,
    target_id UUID NOT NULL REFERENCES pipeline_targets(id),
    transform_sql TEXT,
    state TEXT NOT NULL DEFAULT 'stopped',
    error_message TEXT,
    records_processed BIGINT NOT NULL DEFAULT 0,
    last_offset BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE(organization_id, name)
);

-- ============================================================================
-- SCHEMA REGISTRY
-- ============================================================================

CREATE TABLE IF NOT EXISTS schema_registry_schemas (
    id SERIAL PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    schema_format VARCHAR(20) NOT NULL,
    schema_definition TEXT NOT NULL,
    schema_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(schema_hash)
);

CREATE INDEX IF NOT EXISTS idx_schemas_format ON schema_registry_schemas(schema_format);
CREATE INDEX IF NOT EXISTS idx_schema_registry_schemas_org ON schema_registry_schemas(organization_id);

CREATE TABLE IF NOT EXISTS schema_registry_versions (
    id SERIAL PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    subject VARCHAR(255) NOT NULL,
    version INTEGER NOT NULL,
    schema_id INTEGER NOT NULL REFERENCES schema_registry_schemas(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(subject, version)
);

CREATE INDEX IF NOT EXISTS idx_versions_subject ON schema_registry_versions(subject);
CREATE INDEX IF NOT EXISTS idx_versions_schema_id ON schema_registry_versions(schema_id);
CREATE INDEX IF NOT EXISTS idx_versions_subject_version ON schema_registry_versions(subject, version DESC);
CREATE INDEX IF NOT EXISTS idx_schema_registry_versions_org ON schema_registry_versions(organization_id);

CREATE TABLE IF NOT EXISTS schema_registry_subject_config (
    subject VARCHAR(255) PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    compatibility VARCHAR(20) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_schema_registry_subject_config_org ON schema_registry_subject_config(organization_id);

CREATE TABLE IF NOT EXISTS schema_registry_global_config (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE,
    compatibility VARCHAR(20) NOT NULL DEFAULT 'backward',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT single_row CHECK (id = TRUE)
);

INSERT INTO schema_registry_global_config (id, compatibility)
VALUES (TRUE, 'backward')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- MATERIALIZED VIEWS
-- ============================================================================

CREATE TABLE IF NOT EXISTS materialized_views (
    id TEXT PRIMARY KEY,
    organization_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    name VARCHAR(255) NOT NULL,
    source_topic TEXT NOT NULL,
    query_sql TEXT NOT NULL,
    refresh_mode VARCHAR(50) NOT NULL DEFAULT 'continuous',
    refresh_interval_ms BIGINT,
    status VARCHAR(50) NOT NULL DEFAULT 'initializing',
    error_message TEXT,
    row_count BIGINT NOT NULL DEFAULT 0,
    last_refresh_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_materialized_views_org ON materialized_views(organization_id);
CREATE INDEX IF NOT EXISTS idx_materialized_views_topic ON materialized_views(source_topic);
CREATE INDEX IF NOT EXISTS idx_materialized_views_status
    ON materialized_views(status) WHERE status IN ('running', 'initializing');

CREATE TABLE IF NOT EXISTS materialized_view_offsets (
    view_id TEXT NOT NULL REFERENCES materialized_views(id) ON DELETE CASCADE,
    partition_id INTEGER NOT NULL,
    last_offset BIGINT NOT NULL DEFAULT 0,
    last_processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (view_id, partition_id)
);

CREATE TABLE IF NOT EXISTS materialized_view_data (
    view_id TEXT NOT NULL REFERENCES materialized_views(id) ON DELETE CASCADE,
    agg_key TEXT NOT NULL,
    agg_values JSONB NOT NULL,
    window_start BIGINT,
    window_end BIGINT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (view_id, agg_key)
);

CREATE INDEX IF NOT EXISTS idx_materialized_view_data_window
    ON materialized_view_data(view_id, window_start, window_end)
    WHERE window_start IS NOT NULL;

-- ============================================================================
-- API KEYS
-- ============================================================================

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(64) NOT NULL,
    key_prefix VARCHAR(12) NOT NULL,
    permissions JSONB NOT NULL DEFAULT '["read", "write"]'::jsonb,
    scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(255),
    max_requests_per_sec INTEGER,
    max_produce_bytes_per_sec INTEGER,
    max_consume_bytes_per_sec INTEGER,
    UNIQUE(key_prefix)
);

CREATE INDEX IF NOT EXISTS idx_api_keys_org ON api_keys(organization_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_api_keys_expires ON api_keys(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================================
-- ORGANIZATION QUOTAS
-- ============================================================================

CREATE TABLE IF NOT EXISTS organization_quotas (
    organization_id UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
    max_topics INTEGER NOT NULL DEFAULT 10,
    max_partitions_per_topic INTEGER NOT NULL DEFAULT 12,
    max_total_partitions INTEGER NOT NULL DEFAULT 100,
    max_storage_bytes BIGINT NOT NULL DEFAULT 10737418240,
    max_retention_days INTEGER NOT NULL DEFAULT 7,
    max_produce_bytes_per_sec BIGINT NOT NULL DEFAULT 10485760,
    max_consume_bytes_per_sec BIGINT NOT NULL DEFAULT 52428800,
    max_requests_per_sec INTEGER NOT NULL DEFAULT 1000,
    max_consumer_groups INTEGER NOT NULL DEFAULT 50,
    max_schemas INTEGER NOT NULL DEFAULT 100,
    max_schema_versions_per_subject INTEGER NOT NULL DEFAULT 100,
    max_connections INTEGER NOT NULL DEFAULT 100,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Default quotas (unlimited for default org)
INSERT INTO organization_quotas (
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
)
ON CONFLICT (organization_id) DO NOTHING;

-- ============================================================================
-- ORGANIZATION USAGE
-- ============================================================================

CREATE TABLE IF NOT EXISTS organization_usage (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    metric VARCHAR(50) NOT NULL,
    value BIGINT NOT NULL DEFAULT 0,
    period_start TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, metric, period_start)
);

CREATE INDEX IF NOT EXISTS idx_org_usage_org ON organization_usage(organization_id);
CREATE INDEX IF NOT EXISTS idx_org_usage_metric ON organization_usage(organization_id, metric);
CREATE INDEX IF NOT EXISTS idx_org_usage_period ON organization_usage(period_start);

-- ============================================================================
-- ROW-LEVEL SECURITY POLICIES
-- ============================================================================

ALTER TABLE organizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE topics ENABLE ROW LEVEL SECURITY;
ALTER TABLE partitions ENABLE ROW LEVEL SECURITY;
ALTER TABLE segments ENABLE ROW LEVEL SECURITY;
ALTER TABLE consumer_groups ENABLE ROW LEVEL SECURITY;
ALTER TABLE consumer_offsets ENABLE ROW LEVEL SECURITY;
ALTER TABLE partition_leases ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_quotas ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_usage ENABLE ROW LEVEL SECURITY;
ALTER TABLE materialized_views ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_organizations ON organizations
    FOR ALL USING (id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_topics ON topics
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_partitions ON partitions
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_segments ON segments
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_consumer_groups ON consumer_groups
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_consumer_offsets ON consumer_offsets
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_partition_leases ON partition_leases
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_api_keys ON api_keys
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_quotas ON organization_quotas
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_usage ON organization_usage
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_materialized_views ON materialized_views
    FOR ALL USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION set_tenant_context(org_id UUID)
RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.current_organization_id', org_id::TEXT, true);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION get_tenant_context()
RETURNS UUID AS $$
BEGIN
    RETURN COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION generate_api_key_prefix(key_type VARCHAR DEFAULT 'live')
RETURNS VARCHAR AS $$
BEGIN
    RETURN 'sk_' || key_type || '_' || encode(gen_random_bytes(4), 'hex');
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- HELPER VIEWS
-- ============================================================================

CREATE OR REPLACE VIEW organization_resource_status AS
SELECT
    o.id AS organization_id,
    o.name AS organization_name,
    o.plan,
    COALESCE(t.topic_count, 0) AS current_topics,
    q.max_topics,
    ROUND(COALESCE(t.topic_count, 0)::NUMERIC / NULLIF(q.max_topics, 0) * 100, 1) AS topics_usage_pct,
    COALESCE(p.partition_count, 0) AS current_partitions,
    q.max_total_partitions,
    ROUND(COALESCE(p.partition_count, 0)::NUMERIC / NULLIF(q.max_total_partitions, 0) * 100, 1) AS partitions_usage_pct,
    COALESCE(s.storage_bytes, 0) AS current_storage_bytes,
    q.max_storage_bytes,
    ROUND(COALESCE(s.storage_bytes, 0)::NUMERIC / NULLIF(q.max_storage_bytes, 0) * 100, 1) AS storage_usage_pct,
    COALESCE(cg.group_count, 0) AS current_consumer_groups,
    q.max_consumer_groups,
    ROUND(COALESCE(cg.group_count, 0)::NUMERIC / NULLIF(q.max_consumer_groups, 0) * 100, 1) AS consumer_groups_usage_pct
FROM organizations o
LEFT JOIN organization_quotas q ON q.organization_id = o.id
LEFT JOIN (
    SELECT organization_id, COUNT(*) AS topic_count
    FROM topics GROUP BY organization_id
) t ON t.organization_id = o.id
LEFT JOIN (
    SELECT organization_id, COUNT(*) AS partition_count
    FROM partitions GROUP BY organization_id
) p ON p.organization_id = o.id
LEFT JOIN (
    SELECT organization_id, SUM(size_bytes) AS storage_bytes
    FROM segments GROUP BY organization_id
) s ON s.organization_id = o.id
LEFT JOIN (
    SELECT organization_id, COUNT(*) AS group_count
    FROM consumer_groups GROUP BY organization_id
) cg ON cg.organization_id = o.id;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE organizations IS 'Tenant/organization accounts for multi-tenancy';
COMMENT ON TABLE topics IS 'Stream definitions with partition count and retention policy';
COMMENT ON COLUMN topics.cleanup_policy IS 'Topic cleanup policy: delete, compact, or compact,delete';
COMMENT ON TABLE partitions IS 'Per-partition state tracking with high watermark offsets';
COMMENT ON TABLE segments IS 'Immutable S3 segment file locations and offset ranges';
COMMENT ON TABLE consumer_groups IS 'Consumer group registration';
COMMENT ON TABLE consumer_offsets IS 'Consumer progress tracking (group/topic/partition)';
COMMENT ON TABLE agents IS 'Agent registration and heartbeat tracking';
COMMENT ON TABLE partition_leases IS 'Partition leadership leases with epoch fencing';
COMMENT ON TABLE compaction_state IS 'Tracks log compaction progress per partition';
COMMENT ON TABLE api_keys IS 'API authentication credentials per organization';
COMMENT ON TABLE organization_quotas IS 'Resource limits per organization for billing and abuse prevention';
COMMENT ON TABLE organization_usage IS 'Real-time usage tracking for quota enforcement';
COMMENT ON TABLE connectors IS 'Connector definitions and state tracking';
COMMENT ON TABLE pipeline_targets IS 'Sink destinations for pipelines';
COMMENT ON TABLE pipelines IS 'Streaming jobs that read from topics and write to targets';

COMMENT ON TABLE schema_registry_schemas IS 'Stores schema definitions with deduplication via SHA-256 hash';
COMMENT ON TABLE schema_registry_versions IS 'Maps subjects to schema versions (subject + version -> schema_id)';
COMMENT ON TABLE schema_registry_subject_config IS 'Per-subject compatibility mode overrides';
COMMENT ON TABLE schema_registry_global_config IS 'Global default compatibility mode (single row)';
COMMENT ON COLUMN schema_registry_schemas.schema_hash IS 'SHA-256 hash of schema_definition for deduplication';
COMMENT ON COLUMN schema_registry_versions.subject IS 'Subject name (e.g. "orders-value", "users-key")';
COMMENT ON COLUMN schema_registry_versions.version IS 'Version number (1, 2, 3, ...) auto-incremented per subject';

COMMENT ON TABLE materialized_views IS 'Materialized view definitions for streaming aggregations';
COMMENT ON COLUMN materialized_views.refresh_mode IS 'continuous: update on each message, periodic: refresh on schedule, manual: only on REFRESH command';
COMMENT ON COLUMN materialized_views.status IS 'Current state: initializing (bootstrapping), running (active), paused (stopped), error (failed)';
COMMENT ON TABLE materialized_view_offsets IS 'Tracks processing progress for each view partition';
COMMENT ON COLUMN materialized_view_offsets.last_offset IS 'Next offset to process (exclusive upper bound)';
COMMENT ON TABLE materialized_view_data IS 'Stores aggregated results for materialized views';
COMMENT ON COLUMN materialized_view_data.agg_key IS 'Composite key: window_start|group_key for windowed views, group_key for non-windowed';

COMMENT ON FUNCTION set_tenant_context IS 'Sets the current organization context for RLS policies';
COMMENT ON FUNCTION get_tenant_context IS 'Gets the current organization context';
COMMENT ON VIEW organization_resource_status IS 'Current resource usage vs quotas per organization';
