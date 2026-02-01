-- Phase 9.1: Schema Registry PostgreSQL Storage
-- Creates tables for storing schemas, versions, and compatibility settings

-- Core schemas table (stores schema content with deduplication)
CREATE TABLE IF NOT EXISTS schema_registry_schemas (
    id SERIAL PRIMARY KEY,
    schema_format VARCHAR(20) NOT NULL,  -- 'avro', 'protobuf', 'json'
    schema_definition TEXT NOT NULL,      -- Actual schema content (JSON)
    schema_hash VARCHAR(64) NOT NULL,     -- SHA-256 hash for deduplication
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(schema_hash)
);

CREATE INDEX idx_schemas_format ON schema_registry_schemas(schema_format);

-- Subject-version mapping (e.g. "orders-value" v1, v2, v3)
CREATE TABLE IF NOT EXISTS schema_registry_versions (
    id SERIAL PRIMARY KEY,
    subject VARCHAR(255) NOT NULL,
    version INTEGER NOT NULL,
    schema_id INTEGER NOT NULL REFERENCES schema_registry_schemas(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(subject, version)
);

CREATE INDEX idx_versions_subject ON schema_registry_versions(subject);
CREATE INDEX idx_versions_schema_id ON schema_registry_versions(schema_id);
CREATE INDEX idx_versions_subject_version ON schema_registry_versions(subject, version DESC);

-- Subject-specific compatibility configuration
CREATE TABLE IF NOT EXISTS schema_registry_subject_config (
    subject VARCHAR(255) PRIMARY KEY,
    compatibility VARCHAR(20) NOT NULL,  -- 'backward', 'forward', 'full', 'transitive', 'none'
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Global compatibility configuration (single row)
CREATE TABLE IF NOT EXISTS schema_registry_global_config (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE,  -- Ensures single row
    compatibility VARCHAR(20) NOT NULL DEFAULT 'backward',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT single_row CHECK (id = TRUE)
);

-- Insert default global config (backward compatibility by default)
INSERT INTO schema_registry_global_config (id, compatibility)
VALUES (TRUE, 'backward')
ON CONFLICT (id) DO NOTHING;

-- Comments for documentation
COMMENT ON TABLE schema_registry_schemas IS 'Stores schema definitions with deduplication via SHA-256 hash';
COMMENT ON TABLE schema_registry_versions IS 'Maps subjects to schema versions (subject + version -> schema_id)';
COMMENT ON TABLE schema_registry_subject_config IS 'Per-subject compatibility mode overrides';
COMMENT ON TABLE schema_registry_global_config IS 'Global default compatibility mode (single row)';

COMMENT ON COLUMN schema_registry_schemas.schema_hash IS 'SHA-256 hash of schema_definition for deduplication';
COMMENT ON COLUMN schema_registry_versions.subject IS 'Subject name (e.g. "orders-value", "users-key")';
COMMENT ON COLUMN schema_registry_versions.version IS 'Version number (1, 2, 3, ...) auto-incremented per subject';
