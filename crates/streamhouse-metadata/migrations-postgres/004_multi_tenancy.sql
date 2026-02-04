-- ============================================================================
-- StreamHouse Metadata Store - Multi-Tenancy Foundation (PostgreSQL)
-- ============================================================================
--
-- Migration: 004_multi_tenancy.sql
-- Backend: PostgreSQL
-- Version: 4
-- Created: 2026-02-03
-- Phase: 21.5 (Multi-Tenancy Foundation)
--
-- ## Purpose
--
-- Adds multi-tenancy support to StreamHouse for SaaS/managed service deployment.
-- Enables isolation between organizations with row-level security.
--
-- ## What This Migration Creates
--
-- 1. **organizations**: Tenant/organization accounts
-- 2. **api_keys**: Authentication credentials per organization
-- 3. **organization_quotas**: Resource limits per organization
-- 4. **organization_usage**: Usage tracking for billing and enforcement
--
-- Plus: Adds organization_id to all existing tables for tenant isolation.
--
-- ## Multi-Tenancy Architecture
--
-- ┌─────────────────────────────────────────────────────────────┐
-- │                    API Layer                                 │
-- │   (extracts organization_id from API key or JWT)            │
-- └──────────────────────────┬──────────────────────────────────┘
--                            │
-- ┌──────────────────────────▼──────────────────────────────────┐
-- │                PostgreSQL + RLS                              │
-- │  - All tables have organization_id column                    │
-- │  - Row-Level Security filters by current org context         │
-- │  - SET app.current_organization_id = 'uuid' per transaction  │
-- └─────────────────────────────────────────────────────────────┘
--
-- ## S3 Path Isolation
--
-- Each organization's data is stored under a unique prefix:
--   s3://bucket/org-{uuid}/data/{topic}/{partition}/segments
--
-- ============================================================================

-- Enable UUID extension if not already enabled
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- ORGANIZATIONS
-- ============================================================================
-- Root table for tenant hierarchy. All other tables reference this.

CREATE TABLE IF NOT EXISTS organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(63) NOT NULL UNIQUE,  -- URL-friendly identifier (e.g., "acme-corp")
    plan VARCHAR(50) NOT NULL DEFAULT 'free',  -- free, pro, enterprise
    status VARCHAR(20) NOT NULL DEFAULT 'active',  -- active, suspended, deleted
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    settings JSONB NOT NULL DEFAULT '{}',

    -- Slug validation: lowercase alphanumeric with hyphens
    CONSTRAINT valid_slug CHECK (slug ~ '^[a-z0-9][a-z0-9-]*[a-z0-9]$' OR slug ~ '^[a-z0-9]$'),
    CONSTRAINT valid_status CHECK (status IN ('active', 'suspended', 'deleted')),
    CONSTRAINT valid_plan CHECK (plan IN ('free', 'pro', 'enterprise'))
);

CREATE INDEX IF NOT EXISTS idx_organizations_slug ON organizations(slug);
CREATE INDEX IF NOT EXISTS idx_organizations_status ON organizations(status);
CREATE INDEX IF NOT EXISTS idx_organizations_plan ON organizations(plan);
CREATE INDEX IF NOT EXISTS idx_organizations_created_at ON organizations(created_at);

-- Insert a default organization for backwards compatibility with existing data
INSERT INTO organizations (id, name, slug, plan, status)
VALUES ('00000000-0000-0000-0000-000000000000', 'Default Organization', 'default', 'enterprise', 'active')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- API KEYS
-- ============================================================================
-- Authentication credentials for programmatic access.

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(64) NOT NULL,  -- SHA-256 hash of the actual key
    key_prefix VARCHAR(12) NOT NULL,  -- First 12 chars for identification (sk_live_xxxx)
    permissions JSONB NOT NULL DEFAULT '["read", "write"]'::jsonb,
    scopes JSONB NOT NULL DEFAULT '[]'::jsonb,  -- Optional: restrict to specific topics
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(255),  -- User ID who created the key

    UNIQUE(key_prefix)
);

CREATE INDEX IF NOT EXISTS idx_api_keys_org ON api_keys(organization_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_api_keys_expires ON api_keys(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================================
-- ORGANIZATION QUOTAS
-- ============================================================================
-- Resource limits per organization for billing tiers and abuse prevention.

CREATE TABLE IF NOT EXISTS organization_quotas (
    organization_id UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,

    -- Topic limits
    max_topics INTEGER NOT NULL DEFAULT 10,
    max_partitions_per_topic INTEGER NOT NULL DEFAULT 12,
    max_total_partitions INTEGER NOT NULL DEFAULT 100,

    -- Storage limits
    max_storage_bytes BIGINT NOT NULL DEFAULT 10737418240,  -- 10 GB
    max_retention_days INTEGER NOT NULL DEFAULT 7,

    -- Throughput limits (per second)
    max_produce_bytes_per_sec BIGINT NOT NULL DEFAULT 10485760,  -- 10 MB/s
    max_consume_bytes_per_sec BIGINT NOT NULL DEFAULT 52428800,  -- 50 MB/s
    max_requests_per_sec INTEGER NOT NULL DEFAULT 1000,

    -- Consumer limits
    max_consumer_groups INTEGER NOT NULL DEFAULT 50,

    -- Schema Registry limits
    max_schemas INTEGER NOT NULL DEFAULT 100,
    max_schema_versions_per_subject INTEGER NOT NULL DEFAULT 100,

    -- Connection limits
    max_connections INTEGER NOT NULL DEFAULT 100,

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert default quotas for the default organization (unlimited)
INSERT INTO organization_quotas (
    organization_id,
    max_topics, max_partitions_per_topic, max_total_partitions,
    max_storage_bytes, max_retention_days,
    max_produce_bytes_per_sec, max_consume_bytes_per_sec, max_requests_per_sec,
    max_consumer_groups, max_schemas, max_schema_versions_per_subject, max_connections
)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    1000000, 1000, 1000000,  -- Unlimited for default org
    9223372036854775807, 36500,  -- ~infinite storage, 100 years retention
    9223372036854775807, 9223372036854775807, 1000000,
    1000000, 1000000, 1000000, 1000000
)
ON CONFLICT (organization_id) DO NOTHING;

-- ============================================================================
-- ORGANIZATION USAGE
-- ============================================================================
-- Real-time usage tracking for quota enforcement and billing.

CREATE TABLE IF NOT EXISTS organization_usage (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    metric VARCHAR(50) NOT NULL,  -- 'storage_bytes', 'topics', 'partitions', etc.
    value BIGINT NOT NULL DEFAULT 0,
    period_start TIMESTAMPTZ NOT NULL,  -- For rate limiting windows (e.g., current second/minute)
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (organization_id, metric, period_start)
);

CREATE INDEX IF NOT EXISTS idx_org_usage_org ON organization_usage(organization_id);
CREATE INDEX IF NOT EXISTS idx_org_usage_metric ON organization_usage(organization_id, metric);
CREATE INDEX IF NOT EXISTS idx_org_usage_period ON organization_usage(period_start);

-- Cleanup old usage records (keep last 7 days)
-- This should be run by a scheduled job:
-- DELETE FROM organization_usage WHERE period_start < NOW() - INTERVAL '7 days';

-- ============================================================================
-- ADD organization_id TO EXISTING TABLES
-- ============================================================================

-- ============================================================================
-- STEP 1: Drop ALL foreign key constraints that reference topics first
-- ============================================================================

-- Drop consumer_offsets -> partitions -> topics chain
ALTER TABLE consumer_offsets DROP CONSTRAINT IF EXISTS consumer_offsets_topic_partition_id_fkey;
ALTER TABLE consumer_offsets DROP CONSTRAINT IF EXISTS consumer_offsets_group_id_fkey;

-- Drop partitions -> topics
ALTER TABLE partitions DROP CONSTRAINT IF EXISTS partitions_topic_fkey;

-- Drop segments -> partitions -> topics
ALTER TABLE segments DROP CONSTRAINT IF EXISTS segments_topic_partition_id_fkey;

-- Drop partition_leases -> topics (if exists)
ALTER TABLE partition_leases DROP CONSTRAINT IF EXISTS partition_leases_topic_fkey;

-- ============================================================================
-- STEP 2: Now we can safely modify topics table
-- ============================================================================

-- Topics: Add organization_id
ALTER TABLE topics ADD COLUMN IF NOT EXISTS organization_id UUID;
UPDATE topics SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
ALTER TABLE topics ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE topics ADD CONSTRAINT fk_topics_org FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_topics_org ON topics(organization_id);

-- Drop old primary key and create new composite primary key
ALTER TABLE topics DROP CONSTRAINT IF EXISTS topics_pkey;
ALTER TABLE topics ADD CONSTRAINT topics_pkey PRIMARY KEY (organization_id, name);

-- ============================================================================
-- STEP 3: Modify partitions and recreate foreign key
-- ============================================================================

-- Partitions: Add organization_id
ALTER TABLE partitions ADD COLUMN IF NOT EXISTS organization_id UUID;
UPDATE partitions SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
ALTER TABLE partitions ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE partitions ADD CONSTRAINT fk_partitions_org FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_partitions_org ON partitions(organization_id);

-- Recreate partitions -> topics foreign key with organization_id
ALTER TABLE partitions ADD CONSTRAINT partitions_topic_fkey
    FOREIGN KEY (organization_id, topic) REFERENCES topics(organization_id, name) ON DELETE CASCADE;

-- Segments: Add organization_id and recreate FK to partitions
ALTER TABLE segments ADD COLUMN IF NOT EXISTS organization_id UUID;
UPDATE segments SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
ALTER TABLE segments ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE segments ADD CONSTRAINT fk_segments_org FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_segments_org ON segments(organization_id);

-- Recreate segments -> partitions foreign key (was dropped in STEP 1)
ALTER TABLE segments ADD CONSTRAINT segments_topic_partition_id_fkey
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE;

-- Consumer Groups
ALTER TABLE consumer_groups ADD COLUMN IF NOT EXISTS organization_id UUID;
UPDATE consumer_groups SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
ALTER TABLE consumer_groups ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE consumer_groups ADD CONSTRAINT fk_consumer_groups_org FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_consumer_groups_org ON consumer_groups(organization_id);

-- Update consumer_groups primary key to include organization_id
ALTER TABLE consumer_offsets DROP CONSTRAINT IF EXISTS consumer_offsets_group_id_fkey;
ALTER TABLE consumer_groups DROP CONSTRAINT IF EXISTS consumer_groups_pkey;
ALTER TABLE consumer_groups ADD CONSTRAINT consumer_groups_pkey PRIMARY KEY (organization_id, group_id);

-- Consumer Offsets
ALTER TABLE consumer_offsets ADD COLUMN IF NOT EXISTS organization_id UUID;
UPDATE consumer_offsets SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
ALTER TABLE consumer_offsets ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE consumer_offsets ADD CONSTRAINT fk_consumer_offsets_org FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_org ON consumer_offsets(organization_id);

-- Update foreign key to include organization_id
ALTER TABLE consumer_offsets ADD CONSTRAINT consumer_offsets_group_fkey
    FOREIGN KEY (organization_id, group_id) REFERENCES consumer_groups(organization_id, group_id) ON DELETE CASCADE;

-- Recreate consumer_offsets -> partitions foreign key (was dropped in STEP 1)
ALTER TABLE consumer_offsets ADD CONSTRAINT consumer_offsets_topic_partition_id_fkey
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE;

-- Agents (can be shared across orgs or per-org)
ALTER TABLE agents ADD COLUMN IF NOT EXISTS organization_id UUID;
-- Agents default to NULL (shared) rather than default org
ALTER TABLE agents ADD CONSTRAINT fk_agents_org FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_agents_org ON agents(organization_id);

-- Partition Leases
ALTER TABLE partition_leases ADD COLUMN IF NOT EXISTS organization_id UUID;
UPDATE partition_leases SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
ALTER TABLE partition_leases ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE partition_leases ADD CONSTRAINT fk_partition_leases_org FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_partition_leases_org ON partition_leases(organization_id);

-- ============================================================================
-- SCHEMA REGISTRY TABLES (if they exist)
-- ============================================================================
-- These may or may not exist depending on whether schema registry migration ran

DO $$
BEGIN
    -- Add organization_id to schema_registry_schemas if table exists
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'schema_registry_schemas') THEN
        ALTER TABLE schema_registry_schemas ADD COLUMN IF NOT EXISTS organization_id UUID;
        UPDATE schema_registry_schemas SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
        ALTER TABLE schema_registry_schemas ALTER COLUMN organization_id SET NOT NULL;

        -- Add foreign key if not exists
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints
            WHERE constraint_name = 'fk_schema_registry_schemas_org'
        ) THEN
            ALTER TABLE schema_registry_schemas ADD CONSTRAINT fk_schema_registry_schemas_org
                FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
        END IF;

        CREATE INDEX IF NOT EXISTS idx_schema_registry_schemas_org ON schema_registry_schemas(organization_id);
    END IF;

    -- Add organization_id to schema_registry_versions if table exists
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'schema_registry_versions') THEN
        ALTER TABLE schema_registry_versions ADD COLUMN IF NOT EXISTS organization_id UUID;
        UPDATE schema_registry_versions SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
        ALTER TABLE schema_registry_versions ALTER COLUMN organization_id SET NOT NULL;

        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints
            WHERE constraint_name = 'fk_schema_registry_versions_org'
        ) THEN
            ALTER TABLE schema_registry_versions ADD CONSTRAINT fk_schema_registry_versions_org
                FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
        END IF;

        CREATE INDEX IF NOT EXISTS idx_schema_registry_versions_org ON schema_registry_versions(organization_id);
    END IF;

    -- Add organization_id to schema_registry_subject_config if table exists
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'schema_registry_subject_config') THEN
        ALTER TABLE schema_registry_subject_config ADD COLUMN IF NOT EXISTS organization_id UUID;
        UPDATE schema_registry_subject_config SET organization_id = '00000000-0000-0000-0000-000000000000' WHERE organization_id IS NULL;
        ALTER TABLE schema_registry_subject_config ALTER COLUMN organization_id SET NOT NULL;

        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints
            WHERE constraint_name = 'fk_schema_registry_subject_config_org'
        ) THEN
            ALTER TABLE schema_registry_subject_config ADD CONSTRAINT fk_schema_registry_subject_config_org
                FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
        END IF;

        CREATE INDEX IF NOT EXISTS idx_schema_registry_subject_config_org ON schema_registry_subject_config(organization_id);
    END IF;
END $$;

-- ============================================================================
-- ROW-LEVEL SECURITY (RLS) POLICIES
-- ============================================================================
-- Automatically filter rows by organization when app.current_organization_id is set.

-- Enable RLS on all tenant tables
ALTER TABLE topics ENABLE ROW LEVEL SECURITY;
ALTER TABLE partitions ENABLE ROW LEVEL SECURITY;
ALTER TABLE segments ENABLE ROW LEVEL SECURITY;
ALTER TABLE consumer_groups ENABLE ROW LEVEL SECURITY;
ALTER TABLE consumer_offsets ENABLE ROW LEVEL SECURITY;
ALTER TABLE partition_leases ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_quotas ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_usage ENABLE ROW LEVEL SECURITY;

-- Create RLS policies for each table
-- These policies check if organization_id matches the session variable app.current_organization_id

CREATE POLICY tenant_isolation_topics ON topics
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_partitions ON partitions
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_segments ON segments
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_consumer_groups ON consumer_groups
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_consumer_offsets ON consumer_offsets
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_partition_leases ON partition_leases
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_api_keys ON api_keys
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_quotas ON organization_quotas
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

CREATE POLICY tenant_isolation_usage ON organization_usage
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

-- Organizations table: users can only see their own organization
CREATE POLICY tenant_isolation_organizations ON organizations
    FOR ALL
    USING (id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

ALTER TABLE organizations ENABLE ROW LEVEL SECURITY;

-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

-- Function to set the current organization context for RLS
CREATE OR REPLACE FUNCTION set_tenant_context(org_id UUID)
RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.current_organization_id', org_id::TEXT, true);
END;
$$ LANGUAGE plpgsql;

-- Function to get the current organization context
CREATE OR REPLACE FUNCTION get_tenant_context()
RETURNS UUID AS $$
BEGIN
    RETURN COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    );
END;
$$ LANGUAGE plpgsql;

-- Function to generate API key (returns the key, caller must hash before storing)
CREATE OR REPLACE FUNCTION generate_api_key_prefix(key_type VARCHAR DEFAULT 'live')
RETURNS VARCHAR AS $$
BEGIN
    RETURN 'sk_' || key_type || '_' || encode(gen_random_bytes(4), 'hex');
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- QUOTA HELPER VIEWS
-- ============================================================================

-- View to get current resource usage vs limits for an organization
CREATE OR REPLACE VIEW organization_resource_status AS
SELECT
    o.id AS organization_id,
    o.name AS organization_name,
    o.plan,
    -- Topic counts
    COALESCE(t.topic_count, 0) AS current_topics,
    q.max_topics,
    ROUND(COALESCE(t.topic_count, 0)::NUMERIC / NULLIF(q.max_topics, 0) * 100, 1) AS topics_usage_pct,
    -- Partition counts
    COALESCE(p.partition_count, 0) AS current_partitions,
    q.max_total_partitions,
    ROUND(COALESCE(p.partition_count, 0)::NUMERIC / NULLIF(q.max_total_partitions, 0) * 100, 1) AS partitions_usage_pct,
    -- Storage
    COALESCE(s.storage_bytes, 0) AS current_storage_bytes,
    q.max_storage_bytes,
    ROUND(COALESCE(s.storage_bytes, 0)::NUMERIC / NULLIF(q.max_storage_bytes, 0) * 100, 1) AS storage_usage_pct,
    -- Consumer groups
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
COMMENT ON TABLE api_keys IS 'API authentication credentials per organization';
COMMENT ON TABLE organization_quotas IS 'Resource limits per organization for billing and abuse prevention';
COMMENT ON TABLE organization_usage IS 'Real-time usage tracking for quota enforcement';
COMMENT ON FUNCTION set_tenant_context IS 'Sets the current organization context for RLS policies';
COMMENT ON FUNCTION get_tenant_context IS 'Gets the current organization context';
COMMENT ON VIEW organization_resource_status IS 'Current resource usage vs quotas per organization';
