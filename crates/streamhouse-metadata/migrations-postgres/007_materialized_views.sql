-- ============================================================================
-- MATERIALIZED VIEWS (Phase 25)
-- Stores definitions and state for streaming materialized views
-- ============================================================================

-- Materialized view definitions table
CREATE TABLE IF NOT EXISTS materialized_views (
    -- Unique identifier
    id TEXT PRIMARY KEY,

    -- Organization for multi-tenancy
    organization_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',

    -- View name (unique per organization)
    name VARCHAR(255) NOT NULL,

    -- Source topic
    source_topic TEXT NOT NULL,

    -- The SQL query that defines the view
    query_sql TEXT NOT NULL,

    -- Refresh mode: 'continuous', 'periodic', 'manual'
    refresh_mode VARCHAR(50) NOT NULL DEFAULT 'continuous',

    -- For periodic refresh: interval in milliseconds
    refresh_interval_ms BIGINT,

    -- View status: 'initializing', 'running', 'paused', 'error'
    status VARCHAR(50) NOT NULL DEFAULT 'initializing',

    -- Error message if status is 'error'
    error_message TEXT,

    -- Total number of rows in the materialized view
    row_count BIGINT NOT NULL DEFAULT 0,

    -- Last refresh timestamp
    last_refresh_at TIMESTAMPTZ,

    -- Created and updated timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ensure unique view names per organization
    UNIQUE(organization_id, name)
);

-- Index for fast lookup by organization
CREATE INDEX IF NOT EXISTS idx_materialized_views_org
    ON materialized_views(organization_id);

-- Index for fast lookup by source topic
CREATE INDEX IF NOT EXISTS idx_materialized_views_topic
    ON materialized_views(source_topic);

-- Index for status queries (find views that need processing)
CREATE INDEX IF NOT EXISTS idx_materialized_views_status
    ON materialized_views(status) WHERE status IN ('running', 'initializing');

-- ============================================================================
-- MATERIALIZED VIEW OFFSETS
-- Tracks the last processed offset for each partition
-- ============================================================================

CREATE TABLE IF NOT EXISTS materialized_view_offsets (
    -- View ID (foreign key)
    view_id TEXT NOT NULL,

    -- Source partition
    partition_id INTEGER NOT NULL,

    -- Last processed offset (exclusive - next offset to process)
    last_offset BIGINT NOT NULL DEFAULT 0,

    -- Last processed timestamp
    last_processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (view_id, partition_id),

    FOREIGN KEY (view_id) REFERENCES materialized_views(id) ON DELETE CASCADE
);

-- ============================================================================
-- MATERIALIZED VIEW DATA
-- Stores the actual aggregated data for the view
-- For simple key-value storage; complex views may use dedicated topics
-- ============================================================================

CREATE TABLE IF NOT EXISTS materialized_view_data (
    -- View ID
    view_id TEXT NOT NULL,

    -- Aggregation key (window_start|group_key or just group_key)
    agg_key TEXT NOT NULL,

    -- Aggregated values as JSONB
    agg_values JSONB NOT NULL,

    -- Window boundaries (for windowed aggregations)
    window_start BIGINT,
    window_end BIGINT,

    -- Last updated timestamp
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (view_id, agg_key),

    FOREIGN KEY (view_id) REFERENCES materialized_views(id) ON DELETE CASCADE
);

-- Index for time-based queries on view data
CREATE INDEX IF NOT EXISTS idx_materialized_view_data_window
    ON materialized_view_data(view_id, window_start, window_end)
    WHERE window_start IS NOT NULL;

-- ============================================================================
-- ROW LEVEL SECURITY
-- Enable RLS for multi-tenant isolation
-- ============================================================================

ALTER TABLE materialized_views ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see views in their organization
CREATE POLICY tenant_isolation_materialized_views ON materialized_views
    FOR ALL
    USING (organization_id = COALESCE(
        NULLIF(current_setting('app.current_organization_id', true), '')::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    ));

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE materialized_views IS 'Materialized view definitions for streaming aggregations';
COMMENT ON COLUMN materialized_views.refresh_mode IS 'continuous: update on each message, periodic: refresh on schedule, manual: only on REFRESH command';
COMMENT ON COLUMN materialized_views.status IS 'Current state: initializing (bootstrapping), running (active), paused (stopped), error (failed)';

COMMENT ON TABLE materialized_view_offsets IS 'Tracks processing progress for each view partition';
COMMENT ON COLUMN materialized_view_offsets.last_offset IS 'Next offset to process (exclusive upper bound)';

COMMENT ON TABLE materialized_view_data IS 'Stores aggregated results for materialized views';
COMMENT ON COLUMN materialized_view_data.agg_key IS 'Composite key: window_start|group_key for windowed views, group_key for non-windowed';
