-- Schema Registry PostgreSQL Migration
-- Creates tables for persistent schema storage

-- Core schemas table (stores schema content)
CREATE TABLE schema_registry_schemas (
    id SERIAL PRIMARY KEY,
    schema_format VARCHAR(20) NOT NULL,  -- 'avro', 'protobuf', 'json'
    schema_definition TEXT NOT NULL,
    schema_hash VARCHAR(64) NOT NULL,    -- SHA-256 for deduplication
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(schema_hash)
);

-- Subject-version mapping (e.g. "orders-value" v1, v2, v3)
CREATE TABLE schema_registry_versions (
    id SERIAL PRIMARY KEY,
    subject VARCHAR(255) NOT NULL,
    version INTEGER NOT NULL,
    schema_id INTEGER NOT NULL REFERENCES schema_registry_schemas(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(subject, version)
);

-- Indexes for performance
CREATE INDEX idx_versions_subject ON schema_registry_versions(subject);
CREATE INDEX idx_versions_schema_id ON schema_registry_versions(schema_id);

-- Subject-specific compatibility config
CREATE TABLE schema_registry_subject_config (
    subject VARCHAR(255) PRIMARY KEY,
    compatibility VARCHAR(20) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Global compatibility config (single row)
CREATE TABLE schema_registry_global_config (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE,
    compatibility VARCHAR(20) NOT NULL DEFAULT 'backward',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT single_row CHECK (id = TRUE)
);

-- Initialize global config with default backward compatibility
INSERT INTO schema_registry_global_config (id, compatibility) VALUES (TRUE, 'backward');
