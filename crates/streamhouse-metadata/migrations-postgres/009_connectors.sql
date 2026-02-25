-- Connector definitions and state tracking
CREATE TABLE IF NOT EXISTS connectors (
    name TEXT PRIMARY KEY,
    connector_type TEXT NOT NULL CHECK (connector_type IN ('source', 'sink')),
    connector_class TEXT NOT NULL,
    topics TEXT NOT NULL,
    config TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'stopped' CHECK (state IN ('running', 'paused', 'stopped', 'failed')),
    error_message TEXT,
    records_processed BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
