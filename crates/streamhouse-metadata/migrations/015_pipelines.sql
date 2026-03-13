-- Pipeline targets: sink destinations for pipelines
CREATE TABLE IF NOT EXISTS pipeline_targets (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    target_type TEXT NOT NULL,
    connection_config TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(organization_id, name)
);

-- Pipelines: streaming jobs that read from topics and write to targets
CREATE TABLE IF NOT EXISTS pipelines (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    source_topic TEXT NOT NULL,
    consumer_group TEXT NOT NULL,
    target_id TEXT NOT NULL,
    transform_sql TEXT,
    state TEXT NOT NULL DEFAULT 'stopped',
    error_message TEXT,
    records_processed INTEGER NOT NULL DEFAULT 0,
    last_offset INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(organization_id, name),
    FOREIGN KEY (target_id) REFERENCES pipeline_targets(id)
);
