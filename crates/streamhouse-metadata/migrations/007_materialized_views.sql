-- Migration 007: Materialized Views
--
-- Stores definitions, offset tracking, and aggregated data for streaming materialized views.

-- Materialized view definitions
CREATE TABLE IF NOT EXISTS materialized_views (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    name TEXT NOT NULL,
    source_topic TEXT NOT NULL,
    query_sql TEXT NOT NULL,
    refresh_mode TEXT NOT NULL DEFAULT 'continuous',
    refresh_interval_ms INTEGER,
    status TEXT NOT NULL DEFAULT 'initializing',
    error_message TEXT,
    row_count INTEGER NOT NULL DEFAULT 0,
    last_refresh_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    UNIQUE(organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_materialized_views_org ON materialized_views(organization_id);
CREATE INDEX IF NOT EXISTS idx_materialized_views_topic ON materialized_views(source_topic);
CREATE INDEX IF NOT EXISTS idx_materialized_views_status ON materialized_views(status);

-- Tracks last processed offset per partition for each view
CREATE TABLE IF NOT EXISTS materialized_view_offsets (
    view_id TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    last_offset INTEGER NOT NULL DEFAULT 0,
    last_processed_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    PRIMARY KEY (view_id, partition_id),
    FOREIGN KEY (view_id) REFERENCES materialized_views(id) ON DELETE CASCADE
);

-- Stores aggregated results for materialized views
CREATE TABLE IF NOT EXISTS materialized_view_data (
    view_id TEXT NOT NULL,
    agg_key TEXT NOT NULL,
    agg_values TEXT NOT NULL,
    window_start INTEGER,
    window_end INTEGER,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    PRIMARY KEY (view_id, agg_key),
    FOREIGN KEY (view_id) REFERENCES materialized_views(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_materialized_view_data_window
    ON materialized_view_data(view_id, window_start, window_end);
