-- Add organization_id to connectors for multi-tenant isolation
CREATE TABLE IF NOT EXISTS connectors_new (
    organization_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
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
    PRIMARY KEY (organization_id, name),
    FOREIGN KEY (organization_id) REFERENCES organizations(id)
);

INSERT INTO connectors_new (organization_id, name, connector_type, connector_class, topics, config, state, error_message, records_processed, created_at, updated_at)
SELECT '00000000-0000-0000-0000-000000000000', name, connector_type, connector_class, topics, config, state, error_message, records_processed, created_at, updated_at
FROM connectors
ON CONFLICT DO NOTHING;

DROP TABLE IF EXISTS connectors;
ALTER TABLE connectors_new RENAME TO connectors;
