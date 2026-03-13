-- Pipeline targets: sink destinations for pipelines
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

-- Pipelines: streaming jobs that read from topics and write to targets
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
