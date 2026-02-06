//! SQL query types and result structures

use serde::{Deserialize, Serialize};

/// Parsed SQL query representation
#[derive(Debug, Clone)]
pub enum SqlQuery {
    /// SELECT query on a topic
    Select(SelectQuery),
    /// SHOW TOPICS command
    ShowTopics,
    /// DESCRIBE topic command
    DescribeTopic(String),
    /// COUNT(*) query
    Count(CountQuery),
    /// Window aggregation query
    WindowAggregate(WindowAggregateQuery),
    /// JOIN query across multiple topics
    Join(JoinQuery),
    /// CREATE MATERIALIZED VIEW command
    CreateMaterializedView(CreateMaterializedViewQuery),
    /// DROP MATERIALIZED VIEW command
    DropMaterializedView(String),
    /// REFRESH MATERIALIZED VIEW command
    RefreshMaterializedView(String),
    /// SHOW MATERIALIZED VIEWS command
    ShowMaterializedViews,
    /// DESCRIBE MATERIALIZED VIEW command
    DescribeMaterializedView(String),
}

/// JOIN type
#[derive(Debug, Clone, PartialEq)]
pub enum JoinType {
    /// INNER JOIN - only matching rows
    Inner,
    /// LEFT [OUTER] JOIN - all left rows, matching right rows
    Left,
    /// RIGHT [OUTER] JOIN - matching left rows, all right rows
    Right,
    /// FULL [OUTER] JOIN - all rows from both sides
    Full,
}

/// Table reference in FROM clause (topic with optional alias)
#[derive(Debug, Clone)]
pub struct TableRef {
    /// Topic name
    pub topic: String,
    /// Optional alias (e.g., "orders o" -> alias = "o")
    pub alias: Option<String>,
    /// Whether this is a TABLE() reference (for stream-table joins)
    pub is_table: bool,
}

impl TableRef {
    /// Get the name to use for column qualification (alias if present, else topic)
    pub fn qualifier(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.topic)
    }
}

/// JOIN condition from ON clause
#[derive(Debug, Clone)]
pub struct JoinCondition {
    /// Left side: (qualifier, field_path) e.g., ("o", "$.user_id") or ("o", "key")
    pub left: (String, String),
    /// Right side: (qualifier, field_path) e.g., ("u", "$.id") or ("u", "key")
    pub right: (String, String),
}

/// JOIN query structure
#[derive(Debug, Clone)]
pub struct JoinQuery {
    /// Left table reference
    pub left: TableRef,
    /// Right table reference
    pub right: TableRef,
    /// Type of join
    pub join_type: JoinType,
    /// Join condition (ON clause)
    pub condition: JoinCondition,
    /// Columns to select
    pub columns: Vec<JoinSelectColumn>,
    /// WHERE clause filters
    pub filters: Vec<Filter>,
    /// ORDER BY clause
    pub order_by: Option<OrderBy>,
    /// LIMIT clause
    pub limit: Option<usize>,
    /// Time window for join buffer (in milliseconds, default 1 hour)
    pub window_ms: Option<i64>,
}

/// Column selection in JOIN query (supports qualified names)
#[derive(Debug, Clone)]
pub enum JoinSelectColumn {
    /// All columns from all tables (*)
    AllFrom(Option<String>), // None = *, Some("o") = o.*
    /// Qualified column: o.key, u.value, etc.
    QualifiedColumn {
        qualifier: String,
        column: String,
        alias: Option<String>,
    },
    /// JSON extraction with qualifier: json_extract(o.value, '$.field')
    QualifiedJsonExtract {
        qualifier: String,
        path: String,
        alias: Option<String>,
    },
}

// ============================================================================
// Materialized View Types
// ============================================================================

/// Refresh mode for materialized views
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefreshMode {
    /// Continuously update as new messages arrive (streaming)
    Continuous,
    /// Refresh periodically on a schedule
    Periodic {
        /// Refresh interval in milliseconds
        interval_ms: i64,
    },
    /// Only refresh when explicitly triggered via REFRESH command
    Manual,
}

impl Default for RefreshMode {
    fn default() -> Self {
        RefreshMode::Continuous
    }
}

/// CREATE MATERIALIZED VIEW query structure
#[derive(Debug, Clone)]
pub struct CreateMaterializedViewQuery {
    /// View name
    pub name: String,
    /// Source topic
    pub source_topic: String,
    /// The underlying query definition (as SQL string for storage)
    pub query_sql: String,
    /// Window specification (for window aggregations)
    pub window: Option<WindowType>,
    /// Aggregations to maintain
    pub aggregations: Vec<WindowAggregation>,
    /// Group by columns (besides window)
    pub group_by: Vec<String>,
    /// WHERE clause filters
    pub filters: Vec<Filter>,
    /// Refresh mode
    pub refresh_mode: RefreshMode,
    /// Whether to replace existing view if it exists (CREATE OR REPLACE)
    pub or_replace: bool,
}

/// Materialized view metadata (stored and returned by DESCRIBE/SHOW)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterializedViewInfo {
    /// View name
    pub name: String,
    /// Source topic
    pub source_topic: String,
    /// The query definition (as SQL)
    pub query_sql: String,
    /// Refresh mode
    pub refresh_mode: RefreshMode,
    /// View state (running, paused, error)
    pub status: MaterializedViewStatus,
    /// Last refresh timestamp
    pub last_refresh_at: Option<i64>,
    /// Last processed offset per partition
    pub last_offsets: std::collections::HashMap<u32, u64>,
    /// Number of rows in the view
    pub row_count: u64,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}

/// Status of a materialized view
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaterializedViewStatus {
    /// View is actively being maintained
    Running,
    /// View maintenance is paused
    Paused,
    /// View encountered an error
    Error(String),
    /// View is being initialized/bootstrapped
    Initializing,
}

impl Default for MaterializedViewStatus {
    fn default() -> Self {
        MaterializedViewStatus::Initializing
    }
}

/// Window type for streaming aggregations
#[derive(Debug, Clone)]
pub enum WindowType {
    /// Fixed-size, non-overlapping windows
    /// TUMBLE(timestamp, INTERVAL '5 minutes')
    Tumble {
        /// Window size in milliseconds
        size_ms: i64,
    },
    /// Sliding/hopping windows that may overlap
    /// HOP(timestamp, INTERVAL '10 minutes', INTERVAL '5 minutes')
    Hop {
        /// Window size in milliseconds
        size_ms: i64,
        /// Slide interval in milliseconds
        slide_ms: i64,
    },
    /// Session windows based on activity gaps
    /// SESSION(timestamp, INTERVAL '30 minutes')
    Session {
        /// Gap timeout in milliseconds
        gap_ms: i64,
    },
}

/// Window aggregation query structure
#[derive(Debug, Clone)]
pub struct WindowAggregateQuery {
    /// Topic name (FROM clause)
    pub topic: String,
    /// Window specification
    pub window: WindowType,
    /// Aggregations to compute (e.g., SUM, COUNT, AVG)
    pub aggregations: Vec<WindowAggregation>,
    /// Group by columns (besides window)
    pub group_by: Vec<String>,
    /// WHERE clause filters
    pub filters: Vec<Filter>,
    /// LIMIT clause
    pub limit: Option<usize>,
}

/// Aggregation function for window queries
#[derive(Debug, Clone)]
pub enum WindowAggregation {
    /// COUNT(*)
    Count { alias: Option<String> },
    /// COUNT(DISTINCT column)
    CountDistinct {
        column: String,
        alias: Option<String>,
    },
    /// SUM(column)
    Sum { path: String, alias: Option<String> },
    /// AVG(column)
    Avg { path: String, alias: Option<String> },
    /// MIN(column)
    Min { path: String, alias: Option<String> },
    /// MAX(column)
    Max { path: String, alias: Option<String> },
    /// FIRST(column) - first value in window
    First { path: String, alias: Option<String> },
    /// LAST(column) - last value in window
    Last { path: String, alias: Option<String> },
}

/// Result of a window aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowResult {
    /// Window start timestamp
    pub window_start: i64,
    /// Window end timestamp
    pub window_end: i64,
    /// Group key (if any)
    pub group_key: Option<String>,
    /// Aggregation values
    pub values: Vec<serde_json::Value>,
}

/// SELECT query structure
#[derive(Debug, Clone)]
pub struct SelectQuery {
    /// Topic name (FROM clause)
    pub topic: String,
    /// Columns to select (* for all)
    pub columns: Vec<SelectColumn>,
    /// WHERE clause filters
    pub filters: Vec<Filter>,
    /// ORDER BY clause
    pub order_by: Option<OrderBy>,
    /// LIMIT clause
    pub limit: Option<usize>,
    /// OFFSET clause
    pub offset: Option<usize>,
}

/// Column selection
#[derive(Debug, Clone)]
pub enum SelectColumn {
    /// All columns (*)
    All,
    /// Specific column
    Column(String),
    /// JSON extraction: json_extract(value, '$.path')
    JsonExtract {
        column: String,
        path: String,
        alias: Option<String>,
    },
    /// Z-score calculation: zscore(json_extract(value, '$.path'))
    /// Calculates (value - mean) / stddev for anomaly detection
    ZScore { path: String, alias: Option<String> },
    /// Moving average: moving_avg(json_extract(value, '$.path'), window_size)
    MovingAvg {
        path: String,
        window_size: usize,
        alias: Option<String>,
    },
    /// Standard deviation: stddev(json_extract(value, '$.path'))
    Stddev { path: String, alias: Option<String> },
    /// Mean/average: avg(json_extract(value, '$.path'))
    Avg { path: String, alias: Option<String> },
    /// Anomaly indicator: anomaly(json_extract(value, '$.path'), threshold)
    /// Returns true if |zscore| > threshold
    Anomaly {
        path: String,
        threshold: f64,
        alias: Option<String>,
    },
    /// Cosine similarity: cosine_similarity(vector_path, query_vector)
    CosineSimilarity {
        path: String,
        query_vector: Vec<f64>,
        alias: Option<String>,
    },
    /// Euclidean distance: euclidean_distance(vector_path, query_vector)
    EuclideanDistance {
        path: String,
        query_vector: Vec<f64>,
        alias: Option<String>,
    },
    /// Dot product: dot_product(vector_path, query_vector)
    DotProduct {
        path: String,
        query_vector: Vec<f64>,
        alias: Option<String>,
    },
    /// Vector magnitude/norm: vector_norm(vector_path)
    VectorNorm { path: String, alias: Option<String> },
}

/// COUNT(*) query structure
#[derive(Debug, Clone)]
pub struct CountQuery {
    /// Topic name
    pub topic: String,
    /// WHERE clause filters
    pub filters: Vec<Filter>,
}

/// Filter condition
#[derive(Debug, Clone)]
pub enum Filter {
    /// key = 'value'
    KeyEquals(String),
    /// partition = N
    PartitionEquals(u32),
    /// offset >= N
    OffsetGte(u64),
    /// offset < N
    OffsetLt(u64),
    /// offset = N
    OffsetEquals(u64),
    /// timestamp >= 'ISO8601'
    TimestampGte(i64),
    /// timestamp < 'ISO8601'
    TimestampLt(i64),
    /// json_extract(value, '$.path') = 'value'
    JsonEquals {
        path: String,
        value: serde_json::Value,
    },
    /// json_extract(value, '$.path') > N
    JsonGt {
        path: String,
        value: serde_json::Value,
    },
    /// json_extract(value, '$.path') < N
    JsonLt {
        path: String,
        value: serde_json::Value,
    },
    /// zscore(json_extract(value, '$.path')) > threshold
    /// Filters rows where z-score exceeds threshold (anomalies)
    ZScoreGt { path: String, threshold: f64 },
    /// zscore(json_extract(value, '$.path')) < threshold
    ZScoreLt { path: String, threshold: f64 },
    /// |zscore(json_extract(value, '$.path'))| > threshold
    /// Filters for statistical outliers (anomalies)
    AnomalyThreshold { path: String, threshold: f64 },
    /// cosine_similarity(vector_path, query_vector) > threshold
    /// Filter for vector similarity search
    CosineSimilarityGt {
        path: String,
        query_vector: Vec<f64>,
        threshold: f64,
    },
    /// euclidean_distance(vector_path, query_vector) < threshold
    /// Filter for nearest neighbor search
    EuclideanDistanceLt {
        path: String,
        query_vector: Vec<f64>,
        threshold: f64,
    },
}

/// ORDER BY clause
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub column: String,
    pub descending: bool,
}

/// Column metadata for query results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

/// Query result row
pub type Row = Vec<serde_json::Value>;

/// Complete query response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    /// Column metadata
    pub columns: Vec<ColumnInfo>,
    /// Result rows
    pub rows: Vec<Row>,
    /// Total row count returned
    pub row_count: usize,
    /// Query execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether results were truncated due to limit
    pub truncated: bool,
}

/// Topic description result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicDescription {
    pub name: String,
    pub partition_count: u32,
    pub total_messages: u64,
    pub schema_subject: Option<String>,
    pub partitions: Vec<PartitionInfo>,
}

/// Partition info for DESCRIBE
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInfo {
    pub partition_id: u32,
    pub high_watermark: u64,
    pub segment_count: usize,
}

/// Message row (internal representation before projection)
#[derive(Debug, Clone)]
pub struct MessageRow {
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub key: Option<String>,
    pub value: String,
    pub timestamp: i64,
}

impl MessageRow {
    /// Convert to a result row based on selected columns
    pub fn to_row(&self, columns: &[SelectColumn]) -> Row {
        self.to_row_with_context(columns, None)
    }

    /// Convert to a result row with optional statistics context for anomaly detection
    pub fn to_row_with_context(&self, columns: &[SelectColumn], ctx: Option<&RowContext>) -> Row {
        columns
            .iter()
            .flat_map(|col| match col {
                SelectColumn::All => vec![
                    serde_json::Value::String(self.topic.clone()),
                    serde_json::Value::Number(self.partition.into()),
                    serde_json::Value::Number(self.offset.into()),
                    self.key
                        .as_ref()
                        .map(|k| serde_json::Value::String(k.clone()))
                        .unwrap_or(serde_json::Value::Null),
                    serde_json::Value::String(self.value.clone()),
                    serde_json::Value::Number(self.timestamp.into()),
                ],
                SelectColumn::Column(name) => {
                    let value = match name.as_str() {
                        "topic" => serde_json::Value::String(self.topic.clone()),
                        "partition" => serde_json::Value::Number(self.partition.into()),
                        "offset" => serde_json::Value::Number(self.offset.into()),
                        "key" => self
                            .key
                            .as_ref()
                            .map(|k| serde_json::Value::String(k.clone()))
                            .unwrap_or(serde_json::Value::Null),
                        "value" => serde_json::Value::String(self.value.clone()),
                        "timestamp" => serde_json::Value::Number(self.timestamp.into()),
                        _ => serde_json::Value::Null,
                    };
                    vec![value]
                }
                SelectColumn::JsonExtract { path, .. } => {
                    // Try to parse value as JSON and extract path
                    let extracted =
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.value) {
                            extract_json_path(&json, path)
                        } else {
                            serde_json::Value::Null
                        };
                    vec![extracted]
                }
                SelectColumn::ZScore { path, .. } => {
                    let zscore = self.calculate_zscore(path, ctx);
                    vec![serde_json::json!(zscore)]
                }
                SelectColumn::MovingAvg {
                    path, window_size, ..
                } => {
                    let ma = self.calculate_moving_avg(path, *window_size, ctx);
                    vec![serde_json::json!(ma)]
                }
                SelectColumn::Stddev { path, .. } => {
                    let stddev = ctx
                        .and_then(|c| c.stats.get(path))
                        .map(|s| s.stddev())
                        .unwrap_or(0.0);
                    vec![serde_json::json!(stddev)]
                }
                SelectColumn::Avg { path, .. } => {
                    let avg = ctx
                        .and_then(|c| c.stats.get(path))
                        .map(|s| s.mean())
                        .unwrap_or(0.0);
                    vec![serde_json::json!(avg)]
                }
                SelectColumn::Anomaly {
                    path, threshold, ..
                } => {
                    let is_anomaly = self.is_anomaly(path, *threshold, ctx);
                    vec![serde_json::Value::Bool(is_anomaly)]
                }
                SelectColumn::CosineSimilarity {
                    path, query_vector, ..
                } => {
                    let similarity = self.compute_cosine_similarity(path, query_vector);
                    vec![serde_json::json!(similarity)]
                }
                SelectColumn::EuclideanDistance {
                    path, query_vector, ..
                } => {
                    let distance = self.compute_euclidean_distance(path, query_vector);
                    vec![serde_json::json!(distance)]
                }
                SelectColumn::DotProduct {
                    path, query_vector, ..
                } => {
                    let dot = self.compute_dot_product(path, query_vector);
                    vec![serde_json::json!(dot)]
                }
                SelectColumn::VectorNorm { path, .. } => {
                    let norm = self.compute_vector_norm(path);
                    vec![serde_json::json!(norm)]
                }
            })
            .collect()
    }

    /// Extract a numeric value from JSON path
    fn extract_numeric(&self, path: &str) -> Option<f64> {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.value) {
            let extracted = extract_json_path(&json, path);
            extracted.as_f64()
        } else {
            None
        }
    }

    /// Calculate z-score for a field
    fn calculate_zscore(&self, path: &str, ctx: Option<&RowContext>) -> f64 {
        let value = self.extract_numeric(path).unwrap_or(0.0);
        ctx.and_then(|c| c.stats.get(path))
            .map(|s| s.zscore(value))
            .unwrap_or(0.0)
    }

    /// Calculate moving average for a field
    fn calculate_moving_avg(
        &self,
        path: &str,
        window_size: usize,
        ctx: Option<&RowContext>,
    ) -> f64 {
        ctx.and_then(|c| {
            c.stats
                .get(path)
                .map(|s| s.moving_avg(window_size, c.row_index))
        })
        .unwrap_or(0.0)
    }

    /// Check if this row is an anomaly
    fn is_anomaly(&self, path: &str, threshold: f64, ctx: Option<&RowContext>) -> bool {
        let value = self.extract_numeric(path).unwrap_or(0.0);
        ctx.and_then(|c| c.stats.get(path))
            .map(|s| s.is_anomaly(value, threshold))
            .unwrap_or(false)
    }

    /// Extract a vector from JSON path
    fn extract_vector(&self, path: &str) -> Option<Vec<f64>> {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.value) {
            let extracted = extract_json_path(&json, path);
            vector_math::parse_vector(&extracted)
        } else {
            None
        }
    }

    /// Compute cosine similarity
    fn compute_cosine_similarity(&self, path: &str, query_vector: &[f64]) -> f64 {
        self.extract_vector(path)
            .map(|v| vector_math::cosine_similarity(&v, query_vector))
            .unwrap_or(0.0)
    }

    /// Compute Euclidean distance
    fn compute_euclidean_distance(&self, path: &str, query_vector: &[f64]) -> f64 {
        self.extract_vector(path)
            .map(|v| vector_math::euclidean_distance(&v, query_vector))
            .unwrap_or(f64::MAX)
    }

    /// Compute dot product
    fn compute_dot_product(&self, path: &str, query_vector: &[f64]) -> f64 {
        self.extract_vector(path)
            .map(|v| vector_math::dot_product(&v, query_vector))
            .unwrap_or(0.0)
    }

    /// Compute vector norm
    fn compute_vector_norm(&self, path: &str) -> f64 {
        self.extract_vector(path)
            .map(|v| vector_math::vector_norm(&v))
            .unwrap_or(0.0)
    }

    /// Check if row matches all filters
    pub fn matches_filters(&self, filters: &[Filter]) -> bool {
        self.matches_filters_with_context(filters, None)
    }

    /// Check if row matches all filters with optional statistics context
    pub fn matches_filters_with_context(
        &self,
        filters: &[Filter],
        ctx: Option<&RowContext>,
    ) -> bool {
        filters
            .iter()
            .all(|filter| self.matches_filter_with_context(filter, ctx))
    }

    fn matches_filter(&self, filter: &Filter) -> bool {
        self.matches_filter_with_context(filter, None)
    }

    fn matches_filter_with_context(&self, filter: &Filter, ctx: Option<&RowContext>) -> bool {
        match filter {
            Filter::KeyEquals(key) => self.key.as_ref().map(|k| k == key).unwrap_or(false),
            Filter::PartitionEquals(p) => self.partition == *p,
            Filter::OffsetGte(o) => self.offset >= *o,
            Filter::OffsetLt(o) => self.offset < *o,
            Filter::OffsetEquals(o) => self.offset == *o,
            Filter::TimestampGte(ts) => self.timestamp >= *ts,
            Filter::TimestampLt(ts) => self.timestamp < *ts,
            Filter::JsonEquals { path, value } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.value) {
                    let extracted = extract_json_path(&json, path);
                    &extracted == value
                } else {
                    false
                }
            }
            Filter::JsonGt { path, value } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.value) {
                    let extracted = extract_json_path(&json, path);
                    compare_json_values(&extracted, value) == std::cmp::Ordering::Greater
                } else {
                    false
                }
            }
            Filter::JsonLt { path, value } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.value) {
                    let extracted = extract_json_path(&json, path);
                    compare_json_values(&extracted, value) == std::cmp::Ordering::Less
                } else {
                    false
                }
            }
            Filter::ZScoreGt { path, threshold } => {
                let zscore = self.calculate_zscore(path, ctx);
                zscore > *threshold
            }
            Filter::ZScoreLt { path, threshold } => {
                let zscore = self.calculate_zscore(path, ctx);
                zscore < *threshold
            }
            Filter::AnomalyThreshold { path, threshold } => self.is_anomaly(path, *threshold, ctx),
            Filter::CosineSimilarityGt {
                path,
                query_vector,
                threshold,
            } => {
                let similarity = self.compute_cosine_similarity(path, query_vector);
                similarity > *threshold
            }
            Filter::EuclideanDistanceLt {
                path,
                query_vector,
                threshold,
            } => {
                let distance = self.compute_euclidean_distance(path, query_vector);
                distance < *threshold
            }
        }
    }
}

/// Extract a value from JSON using a simple path ($.field.subfield)
fn extract_json_path(json: &serde_json::Value, path: &str) -> serde_json::Value {
    let path = path.trim_start_matches('$').trim_start_matches('.');
    let parts: Vec<&str> = path.split('.').collect();

    let mut current = json.clone();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        current = match current {
            serde_json::Value::Object(map) => {
                map.get(part).cloned().unwrap_or(serde_json::Value::Null)
            }
            serde_json::Value::Array(arr) => {
                if let Ok(idx) = part.parse::<usize>() {
                    arr.get(idx).cloned().unwrap_or(serde_json::Value::Null)
                } else {
                    serde_json::Value::Null
                }
            }
            _ => serde_json::Value::Null,
        };
    }
    current
}

/// Compare two JSON values for ordering
fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    use serde_json::Value;
    use std::cmp::Ordering;

    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let a_f = a.as_f64().unwrap_or(0.0);
            let b_f = b.as_f64().unwrap_or(0.0);
            a_f.partial_cmp(&b_f).unwrap_or(Ordering::Equal)
        }
        (Value::String(a), Value::String(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}

/// Statistics for a numeric field (used for anomaly detection)
#[derive(Debug, Clone, Default)]
pub struct FieldStatistics {
    pub count: usize,
    pub sum: f64,
    pub sum_squared: f64,
    pub values: Vec<f64>, // For moving average calculations
}

impl FieldStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.sum_squared += value * value;
        self.values.push(value);
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }

    pub fn variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            let mean = self.mean();
            (self.sum_squared / self.count as f64) - (mean * mean)
        }
    }

    pub fn stddev(&self) -> f64 {
        self.variance().sqrt()
    }

    pub fn zscore(&self, value: f64) -> f64 {
        let stddev = self.stddev();
        if stddev == 0.0 {
            0.0
        } else {
            (value - self.mean()) / stddev
        }
    }

    pub fn is_anomaly(&self, value: f64, threshold: f64) -> bool {
        self.zscore(value).abs() > threshold
    }

    pub fn moving_avg(&self, window: usize, index: usize) -> f64 {
        let start = if index >= window {
            index - window + 1
        } else {
            0
        };
        let end = (index + 1).min(self.values.len());
        let slice = &self.values[start..end];
        if slice.is_empty() {
            0.0
        } else {
            slice.iter().sum::<f64>() / slice.len() as f64
        }
    }
}

/// Context for row projection with pre-computed statistics
#[derive(Debug, Clone)]
pub struct RowContext {
    /// Pre-computed statistics for each JSON path
    pub stats: std::collections::HashMap<String, FieldStatistics>,
    /// Current row index (for moving average)
    pub row_index: usize,
}

impl RowContext {
    pub fn new() -> Self {
        Self {
            stats: std::collections::HashMap::new(),
            row_index: 0,
        }
    }
}

impl Default for RowContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Vector math utilities for similarity search
pub mod vector_math {
    /// Compute cosine similarity between two vectors
    pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
        let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }

    /// Compute Euclidean distance between two vectors
    pub fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
        if a.len() != b.len() {
            return f64::MAX;
        }

        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    /// Compute dot product between two vectors
    pub fn dot_product(a: &[f64], b: &[f64]) -> f64 {
        if a.len() != b.len() {
            return 0.0;
        }

        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// Compute L2 norm (magnitude) of a vector
    pub fn vector_norm(v: &[f64]) -> f64 {
        v.iter().map(|x| x * x).sum::<f64>().sqrt()
    }

    /// Parse a vector from JSON value (array of numbers)
    pub fn parse_vector(value: &serde_json::Value) -> Option<Vec<f64>> {
        match value {
            serde_json::Value::Array(arr) => {
                let mut vec = Vec::with_capacity(arr.len());
                for item in arr {
                    match item.as_f64() {
                        Some(f) => vec.push(f),
                        None => return None,
                    }
                }
                Some(vec)
            }
            _ => None,
        }
    }
}

/// Get column info for SELECT * queries
pub fn default_columns() -> Vec<ColumnInfo> {
    vec![
        ColumnInfo {
            name: "topic".to_string(),
            data_type: "string".to_string(),
        },
        ColumnInfo {
            name: "partition".to_string(),
            data_type: "integer".to_string(),
        },
        ColumnInfo {
            name: "offset".to_string(),
            data_type: "bigint".to_string(),
        },
        ColumnInfo {
            name: "key".to_string(),
            data_type: "string".to_string(),
        },
        ColumnInfo {
            name: "value".to_string(),
            data_type: "string".to_string(),
        },
        ColumnInfo {
            name: "timestamp".to_string(),
            data_type: "bigint".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_statistics_basic() {
        let mut stats = FieldStatistics::new();
        stats.add(10.0);
        stats.add(20.0);
        stats.add(30.0);

        assert_eq!(stats.count, 3);
        assert!((stats.mean() - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_field_statistics_stddev() {
        let mut stats = FieldStatistics::new();
        // Standard test case: values 2, 4, 4, 4, 5, 5, 7, 9
        // Mean = 5, Variance = 4, StdDev = 2
        for v in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
            stats.add(v);
        }

        assert!((stats.mean() - 5.0).abs() < 0.001);
        assert!((stats.stddev() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_field_statistics_zscore() {
        let mut stats = FieldStatistics::new();
        // Mean = 100, StdDev = 15 (like IQ scores)
        let values = vec![85.0, 100.0, 115.0, 70.0, 130.0, 100.0, 100.0, 100.0];
        for v in values {
            stats.add(v);
        }

        let mean = stats.mean();
        let stddev = stats.stddev();

        // Z-score of mean should be 0
        assert!((stats.zscore(mean)).abs() < 0.001);

        // Z-score of value one stddev above mean should be ~1
        assert!((stats.zscore(mean + stddev) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_field_statistics_anomaly() {
        let mut stats = FieldStatistics::new();
        // Normal values around 100
        for v in [98.0, 99.0, 100.0, 101.0, 102.0, 99.5, 100.5, 100.0] {
            stats.add(v);
        }

        // Value close to mean is not anomaly
        assert!(!stats.is_anomaly(100.0, 2.0));

        // Value far from mean should be anomaly
        assert!(stats.is_anomaly(150.0, 2.0));
        assert!(stats.is_anomaly(50.0, 2.0));
    }

    #[test]
    fn test_field_statistics_moving_avg() {
        let mut stats = FieldStatistics::new();
        for v in [10.0, 20.0, 30.0, 40.0, 50.0] {
            stats.add(v);
        }

        // Moving average with window 3 at index 4 (last element)
        // Should average values at indices 2, 3, 4 = (30 + 40 + 50) / 3 = 40
        assert!((stats.moving_avg(3, 4) - 40.0).abs() < 0.001);

        // Moving average with window 2 at index 2
        // Should average values at indices 1, 2 = (20 + 30) / 2 = 25
        assert!((stats.moving_avg(2, 2) - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_message_row_zscore_with_context() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"price": 150.0}"#.to_string(),
            timestamp: 0,
        };

        // Create statistics context
        let mut ctx = RowContext::new();
        let mut stats = FieldStatistics::new();
        for v in [100.0, 100.0, 100.0, 100.0, 150.0] {
            stats.add(v);
        }
        ctx.stats.insert("$.price".to_string(), stats);

        // Test zscore column
        let columns = vec![SelectColumn::ZScore {
            path: "$.price".to_string(),
            alias: Some("z".to_string()),
        }];

        let row = msg.to_row_with_context(&columns, Some(&ctx));
        assert_eq!(row.len(), 1);
        // The z-score should be positive (150 is above mean of 110)
        let zscore = row[0].as_f64().unwrap();
        assert!(zscore > 0.0);
    }

    #[test]
    fn test_message_row_anomaly_detection() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"amount": 1000.0}"#.to_string(),
            timestamp: 0,
        };

        // Create statistics context with tight distribution
        let mut ctx = RowContext::new();
        let mut stats = FieldStatistics::new();
        for v in [100.0, 101.0, 99.0, 100.5, 99.5, 100.0, 100.0, 100.0] {
            stats.add(v);
        }
        ctx.stats.insert("$.amount".to_string(), stats);

        // Test anomaly detection (1000 should be far from mean of ~100)
        let columns = vec![SelectColumn::Anomaly {
            path: "$.amount".to_string(),
            threshold: 2.0,
            alias: None,
        }];

        let row = msg.to_row_with_context(&columns, Some(&ctx));
        assert_eq!(row.len(), 1);
        assert_eq!(row[0], serde_json::Value::Bool(true)); // Should be anomaly
    }

    #[test]
    fn test_extract_json_path() {
        let json: serde_json::Value = serde_json::json!({
            "order": {
                "amount": 123.45,
                "items": [{"name": "item1"}, {"name": "item2"}]
            }
        });

        // Test nested path
        let extracted = extract_json_path(&json, "$.order.amount");
        assert_eq!(extracted.as_f64().unwrap(), 123.45);

        // Test array indexing
        let extracted = extract_json_path(&json, "$.order.items.0.name");
        assert_eq!(extracted.as_str().unwrap(), "item1");
    }

    // Vector Math Tests

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors should have similarity 1.0
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((vector_math::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        // Orthogonal vectors should have similarity 0.0
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(vector_math::cosine_similarity(&a, &b).abs() < 0.001);

        // Opposite vectors should have similarity -1.0
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((vector_math::cosine_similarity(&a, &b) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert!((vector_math::euclidean_distance(&a, &b) - 5.0).abs() < 0.001);

        let a = vec![1.0, 1.0, 1.0];
        let b = vec![1.0, 1.0, 1.0];
        assert!(vector_math::euclidean_distance(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
        assert!((vector_math::dot_product(&a, &b) - 32.0).abs() < 0.001);
    }

    #[test]
    fn test_vector_norm() {
        let v = vec![3.0, 4.0];
        assert!((vector_math::vector_norm(&v) - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_vector() {
        let json = serde_json::json!([0.1, 0.2, 0.3]);
        let vec = vector_math::parse_vector(&json).unwrap();
        assert_eq!(vec, vec![0.1, 0.2, 0.3]);

        let json = serde_json::json!("not a vector");
        assert!(vector_math::parse_vector(&json).is_none());
    }

    #[test]
    fn test_message_row_vector_operations() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"embedding": [0.5, 0.5, 0.0]}"#.to_string(),
            timestamp: 0,
        };

        // Test cosine similarity
        let query_vec = vec![1.0, 0.0, 0.0];
        let sim = msg.compute_cosine_similarity("$.embedding", &query_vec);
        // [0.5, 0.5, 0] dot [1, 0, 0] = 0.5
        // |[0.5, 0.5, 0]| = sqrt(0.5) ≈ 0.707
        // similarity = 0.5 / 0.707 ≈ 0.707
        assert!(sim > 0.7 && sim < 0.72);

        // Test euclidean distance
        let query_vec = vec![0.5, 0.5, 0.0];
        let dist = msg.compute_euclidean_distance("$.embedding", &query_vec);
        assert!(dist < 0.001); // Same vector

        // Test vector norm
        let norm = msg.compute_vector_norm("$.embedding");
        assert!((norm - 0.7071).abs() < 0.01); // sqrt(0.5)
    }

    // ========================================================================
    // FieldStatistics - comprehensive tests
    // ========================================================================

    #[test]
    fn test_field_statistics_empty() {
        let stats = FieldStatistics::new();
        assert_eq!(stats.count, 0);
        assert_eq!(stats.mean(), 0.0);
        assert_eq!(stats.variance(), 0.0);
        assert_eq!(stats.stddev(), 0.0);
        assert_eq!(stats.zscore(5.0), 0.0); // stddev is 0 => zscore is 0
    }

    #[test]
    fn test_field_statistics_single_value() {
        let mut stats = FieldStatistics::new();
        stats.add(42.0);
        assert_eq!(stats.count, 1);
        assert_eq!(stats.mean(), 42.0);
        assert_eq!(stats.variance(), 0.0); // Needs at least 2 values
        assert_eq!(stats.stddev(), 0.0);
    }

    #[test]
    fn test_field_statistics_two_values() {
        let mut stats = FieldStatistics::new();
        stats.add(0.0);
        stats.add(10.0);
        assert_eq!(stats.count, 2);
        assert!((stats.mean() - 5.0).abs() < 0.001);
        // variance = E[X^2] - (E[X])^2 = (0+100)/2 - 25 = 25
        assert!((stats.variance() - 25.0).abs() < 0.001);
        assert!((stats.stddev() - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_field_statistics_all_same_values() {
        let mut stats = FieldStatistics::new();
        for _ in 0..100 {
            stats.add(7.0);
        }
        assert_eq!(stats.count, 100);
        assert!((stats.mean() - 7.0).abs() < 0.001);
        assert!(stats.variance().abs() < 0.001);
        assert!(stats.stddev().abs() < 0.001);
        // z-score of any value should be 0 since stddev is 0
        assert_eq!(stats.zscore(7.0), 0.0);
        assert_eq!(stats.zscore(100.0), 0.0);
    }

    #[test]
    fn test_field_statistics_negative_values() {
        let mut stats = FieldStatistics::new();
        stats.add(-10.0);
        stats.add(-20.0);
        stats.add(-30.0);
        assert!((stats.mean() - (-20.0)).abs() < 0.001);
        assert_eq!(stats.count, 3);
    }

    #[test]
    fn test_field_statistics_mixed_pos_neg() {
        let mut stats = FieldStatistics::new();
        stats.add(-5.0);
        stats.add(5.0);
        assert!((stats.mean() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_field_statistics_zscore_symmetry() {
        let mut stats = FieldStatistics::new();
        for v in [10.0, 20.0, 30.0, 40.0, 50.0] {
            stats.add(v);
        }
        let mean = stats.mean(); // 30
        let stddev = stats.stddev();

        let z_above = stats.zscore(mean + stddev);
        let z_below = stats.zscore(mean - stddev);
        // z-scores should be symmetric around 0
        assert!((z_above + z_below).abs() < 0.001);
        assert!((z_above - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_field_statistics_anomaly_not_anomaly() {
        let mut stats = FieldStatistics::new();
        for v in [100.0, 101.0, 99.0, 100.5, 99.5] {
            stats.add(v);
        }
        // Mean value should not be an anomaly at any reasonable threshold
        assert!(!stats.is_anomaly(100.0, 1.0));
        assert!(!stats.is_anomaly(100.0, 2.0));
        assert!(!stats.is_anomaly(100.0, 3.0));
    }

    #[test]
    fn test_field_statistics_anomaly_extreme_value() {
        let mut stats = FieldStatistics::new();
        for v in [100.0, 100.0, 100.0, 100.0, 100.0] {
            stats.add(v);
        }
        // All values identical: stddev is 0, so z-score for any value is 0
        // This means is_anomaly should return false even for extreme values
        assert!(!stats.is_anomaly(1000000.0, 2.0));
    }

    #[test]
    fn test_field_statistics_moving_avg_at_start() {
        let mut stats = FieldStatistics::new();
        for v in [10.0, 20.0, 30.0, 40.0, 50.0] {
            stats.add(v);
        }
        // At index 0 with window 3, only value at index 0 is available
        assert!((stats.moving_avg(3, 0) - 10.0).abs() < 0.001);

        // At index 1 with window 3, values at 0 and 1 are available
        assert!((stats.moving_avg(3, 1) - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_field_statistics_moving_avg_full_window() {
        let mut stats = FieldStatistics::new();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            stats.add(v);
        }
        // Window of 5 at last index: average of all = 3.0
        assert!((stats.moving_avg(5, 4) - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_field_statistics_moving_avg_window_larger_than_data() {
        let mut stats = FieldStatistics::new();
        stats.add(10.0);
        stats.add(20.0);
        // Window size 10 but only 2 values
        assert!((stats.moving_avg(10, 1) - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_field_statistics_moving_avg_empty() {
        let stats = FieldStatistics::new();
        assert_eq!(stats.moving_avg(5, 0), 0.0);
    }

    #[test]
    fn test_field_statistics_default() {
        let stats = FieldStatistics::default();
        assert_eq!(stats.count, 0);
        assert_eq!(stats.sum, 0.0);
        assert_eq!(stats.sum_squared, 0.0);
        assert!(stats.values.is_empty());
    }

    #[test]
    fn test_field_statistics_large_dataset() {
        let mut stats = FieldStatistics::new();
        // Add 1000 values: 0, 1, 2, ..., 999
        for i in 0..1000 {
            stats.add(i as f64);
        }
        assert_eq!(stats.count, 1000);
        // Mean of 0..999 = 499.5
        assert!((stats.mean() - 499.5).abs() < 0.001);
    }

    // ========================================================================
    // RowContext tests
    // ========================================================================

    #[test]
    fn test_row_context_new() {
        let ctx = RowContext::new();
        assert!(ctx.stats.is_empty());
        assert_eq!(ctx.row_index, 0);
    }

    #[test]
    fn test_row_context_default() {
        let ctx = RowContext::default();
        assert!(ctx.stats.is_empty());
        assert_eq!(ctx.row_index, 0);
    }

    #[test]
    fn test_row_context_with_stats() {
        let mut ctx = RowContext::new();
        let mut stats = FieldStatistics::new();
        stats.add(1.0);
        stats.add(2.0);
        ctx.stats.insert("$.price".to_string(), stats);
        ctx.row_index = 5;

        assert_eq!(ctx.stats.len(), 1);
        assert!(ctx.stats.contains_key("$.price"));
        assert_eq!(ctx.row_index, 5);
    }

    // ========================================================================
    // RefreshMode tests
    // ========================================================================

    #[test]
    fn test_refresh_mode_default() {
        let mode = RefreshMode::default();
        assert!(matches!(mode, RefreshMode::Continuous));
    }

    #[test]
    fn test_refresh_mode_continuous_eq() {
        assert_eq!(RefreshMode::Continuous, RefreshMode::Continuous);
    }

    #[test]
    fn test_refresh_mode_manual_eq() {
        assert_eq!(RefreshMode::Manual, RefreshMode::Manual);
    }

    #[test]
    fn test_refresh_mode_periodic_eq() {
        assert_eq!(
            RefreshMode::Periodic { interval_ms: 5000 },
            RefreshMode::Periodic { interval_ms: 5000 }
        );
    }

    #[test]
    fn test_refresh_mode_periodic_neq() {
        assert_ne!(
            RefreshMode::Periodic { interval_ms: 5000 },
            RefreshMode::Periodic { interval_ms: 10000 }
        );
    }

    #[test]
    fn test_refresh_mode_different_variants_neq() {
        assert_ne!(RefreshMode::Continuous, RefreshMode::Manual);
    }

    #[test]
    fn test_refresh_mode_serialize_continuous() {
        let mode = RefreshMode::Continuous;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""continuous""#);
    }

    #[test]
    fn test_refresh_mode_serialize_manual() {
        let mode = RefreshMode::Manual;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""manual""#);
    }

    #[test]
    fn test_refresh_mode_serialize_periodic() {
        let mode = RefreshMode::Periodic { interval_ms: 60000 };
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["periodic"]["interval_ms"], 60000);
    }

    #[test]
    fn test_refresh_mode_deserialize_continuous() {
        let mode: RefreshMode = serde_json::from_str(r#""continuous""#).unwrap();
        assert_eq!(mode, RefreshMode::Continuous);
    }

    #[test]
    fn test_refresh_mode_deserialize_manual() {
        let mode: RefreshMode = serde_json::from_str(r#""manual""#).unwrap();
        assert_eq!(mode, RefreshMode::Manual);
    }

    #[test]
    fn test_refresh_mode_roundtrip() {
        let modes = vec![
            RefreshMode::Continuous,
            RefreshMode::Manual,
            RefreshMode::Periodic {
                interval_ms: 300000,
            },
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: RefreshMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }

    // ========================================================================
    // MaterializedViewStatus tests
    // ========================================================================

    #[test]
    fn test_materialized_view_status_default() {
        let status = MaterializedViewStatus::default();
        assert!(matches!(status, MaterializedViewStatus::Initializing));
    }

    #[test]
    fn test_materialized_view_status_eq() {
        assert_eq!(
            MaterializedViewStatus::Running,
            MaterializedViewStatus::Running
        );
        assert_eq!(
            MaterializedViewStatus::Paused,
            MaterializedViewStatus::Paused
        );
        assert_eq!(
            MaterializedViewStatus::Initializing,
            MaterializedViewStatus::Initializing
        );
        assert_eq!(
            MaterializedViewStatus::Error("test".to_string()),
            MaterializedViewStatus::Error("test".to_string())
        );
    }

    #[test]
    fn test_materialized_view_status_neq() {
        assert_ne!(
            MaterializedViewStatus::Running,
            MaterializedViewStatus::Paused
        );
        assert_ne!(
            MaterializedViewStatus::Error("a".to_string()),
            MaterializedViewStatus::Error("b".to_string())
        );
    }

    #[test]
    fn test_materialized_view_status_serialize() {
        assert_eq!(
            serde_json::to_string(&MaterializedViewStatus::Running).unwrap(),
            r#""running""#
        );
        assert_eq!(
            serde_json::to_string(&MaterializedViewStatus::Paused).unwrap(),
            r#""paused""#
        );
        assert_eq!(
            serde_json::to_string(&MaterializedViewStatus::Initializing).unwrap(),
            r#""initializing""#
        );
    }

    #[test]
    fn test_materialized_view_status_serialize_error() {
        let status = MaterializedViewStatus::Error("connection lost".to_string());
        let json = serde_json::to_string(&status).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error"], "connection lost");
    }

    #[test]
    fn test_materialized_view_status_roundtrip() {
        let statuses = vec![
            MaterializedViewStatus::Running,
            MaterializedViewStatus::Paused,
            MaterializedViewStatus::Initializing,
            MaterializedViewStatus::Error("fail".to_string()),
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: MaterializedViewStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    // ========================================================================
    // JoinType tests
    // ========================================================================

    #[test]
    fn test_join_type_eq() {
        assert_eq!(JoinType::Inner, JoinType::Inner);
        assert_eq!(JoinType::Left, JoinType::Left);
        assert_eq!(JoinType::Right, JoinType::Right);
        assert_eq!(JoinType::Full, JoinType::Full);
    }

    #[test]
    fn test_join_type_neq() {
        assert_ne!(JoinType::Inner, JoinType::Left);
        assert_ne!(JoinType::Left, JoinType::Right);
        assert_ne!(JoinType::Right, JoinType::Full);
    }

    // ========================================================================
    // TableRef tests
    // ========================================================================

    #[test]
    fn test_table_ref_qualifier_with_alias() {
        let tr = TableRef {
            topic: "orders".to_string(),
            alias: Some("o".to_string()),
            is_table: false,
        };
        assert_eq!(tr.qualifier(), "o");
    }

    #[test]
    fn test_table_ref_qualifier_without_alias() {
        let tr = TableRef {
            topic: "orders".to_string(),
            alias: None,
            is_table: false,
        };
        assert_eq!(tr.qualifier(), "orders");
    }

    #[test]
    fn test_table_ref_is_table_flag() {
        let stream_ref = TableRef {
            topic: "events".to_string(),
            alias: None,
            is_table: false,
        };
        assert!(!stream_ref.is_table);

        let table_ref = TableRef {
            topic: "users".to_string(),
            alias: Some("u".to_string()),
            is_table: true,
        };
        assert!(table_ref.is_table);
        assert_eq!(table_ref.qualifier(), "u");
    }

    // ========================================================================
    // extract_json_path - comprehensive tests
    // ========================================================================

    #[test]
    fn test_extract_json_path_simple_field() {
        let json: serde_json::Value = serde_json::json!({"name": "Alice"});
        let result = extract_json_path(&json, "$.name");
        assert_eq!(result, serde_json::json!("Alice"));
    }

    #[test]
    fn test_extract_json_path_nested_field() {
        let json: serde_json::Value = serde_json::json!({
            "user": {"address": {"city": "Seattle"}}
        });
        let result = extract_json_path(&json, "$.user.address.city");
        assert_eq!(result, serde_json::json!("Seattle"));
    }

    #[test]
    fn test_extract_json_path_array_index() {
        let json: serde_json::Value = serde_json::json!({
            "items": ["apple", "banana", "cherry"]
        });
        let result = extract_json_path(&json, "$.items.1");
        assert_eq!(result, serde_json::json!("banana"));
    }

    #[test]
    fn test_extract_json_path_missing_field() {
        let json: serde_json::Value = serde_json::json!({"name": "Alice"});
        let result = extract_json_path(&json, "$.nonexistent");
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn test_extract_json_path_deep_missing() {
        let json: serde_json::Value = serde_json::json!({"a": {"b": 1}});
        let result = extract_json_path(&json, "$.a.b.c.d");
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn test_extract_json_path_root_only() {
        let json: serde_json::Value = serde_json::json!({"x": 42});
        let result = extract_json_path(&json, "$");
        // With empty path, should return the whole json
        assert_eq!(result, json);
    }

    #[test]
    fn test_extract_json_path_number_value() {
        let json: serde_json::Value = serde_json::json!({"price": 99.99});
        let result = extract_json_path(&json, "$.price");
        assert_eq!(result.as_f64().unwrap(), 99.99);
    }

    #[test]
    fn test_extract_json_path_boolean_value() {
        let json: serde_json::Value = serde_json::json!({"active": true});
        let result = extract_json_path(&json, "$.active");
        assert_eq!(result, serde_json::json!(true));
    }

    #[test]
    fn test_extract_json_path_null_value() {
        let json: serde_json::Value = serde_json::json!({"nothing": null});
        let result = extract_json_path(&json, "$.nothing");
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn test_extract_json_path_array_out_of_bounds() {
        let json: serde_json::Value = serde_json::json!({"items": [1, 2, 3]});
        let result = extract_json_path(&json, "$.items.10");
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn test_extract_json_path_non_numeric_index_on_array() {
        let json: serde_json::Value = serde_json::json!({"items": [1, 2, 3]});
        let result = extract_json_path(&json, "$.items.name");
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn test_extract_json_path_without_dollar() {
        let json: serde_json::Value = serde_json::json!({"name": "Bob"});
        // The function strips $ and . prefix
        let result = extract_json_path(&json, "name");
        assert_eq!(result, serde_json::json!("Bob"));
    }

    // ========================================================================
    // compare_json_values tests
    // ========================================================================

    #[test]
    fn test_compare_json_numbers() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_json_values(&serde_json::json!(1), &serde_json::json!(2)),
            Ordering::Less
        );
        assert_eq!(
            compare_json_values(&serde_json::json!(2), &serde_json::json!(1)),
            Ordering::Greater
        );
        assert_eq!(
            compare_json_values(&serde_json::json!(5), &serde_json::json!(5)),
            Ordering::Equal
        );
    }

    #[test]
    fn test_compare_json_floats() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_json_values(&serde_json::json!(1.5), &serde_json::json!(2.5)),
            Ordering::Less
        );
        assert_eq!(
            compare_json_values(&serde_json::json!(3.14), &serde_json::json!(2.72)),
            Ordering::Greater
        );
    }

    #[test]
    fn test_compare_json_strings() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_json_values(&serde_json::json!("apple"), &serde_json::json!("banana")),
            Ordering::Less
        );
        assert_eq!(
            compare_json_values(&serde_json::json!("zebra"), &serde_json::json!("apple")),
            Ordering::Greater
        );
        assert_eq!(
            compare_json_values(&serde_json::json!("same"), &serde_json::json!("same")),
            Ordering::Equal
        );
    }

    #[test]
    fn test_compare_json_mixed_types() {
        use std::cmp::Ordering;
        // Comparing different types returns Equal (neither can be ordered)
        assert_eq!(
            compare_json_values(&serde_json::json!(42), &serde_json::json!("hello")),
            Ordering::Equal
        );
        assert_eq!(
            compare_json_values(&serde_json::json!(true), &serde_json::json!(42)),
            Ordering::Equal
        );
    }

    // ========================================================================
    // MessageRow tests
    // ========================================================================

    fn make_test_row() -> MessageRow {
        MessageRow {
            topic: "test-topic".to_string(),
            partition: 2,
            offset: 100,
            key: Some("my-key".to_string()),
            value: r#"{"amount": 42.5, "status": "shipped", "items": [1,2,3]}"#.to_string(),
            timestamp: 1700000000000,
        }
    }

    fn make_test_row_no_key() -> MessageRow {
        MessageRow {
            topic: "events".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"count": 10}"#.to_string(),
            timestamp: 1700000000000,
        }
    }

    #[test]
    fn test_message_row_to_row_all() {
        let msg = make_test_row();
        let row = msg.to_row(&[SelectColumn::All]);
        // All columns returns: topic, partition, offset, key, value, timestamp
        assert_eq!(row.len(), 6);
        assert_eq!(row[0], serde_json::json!("test-topic"));
        assert_eq!(row[1], serde_json::json!(2));
        assert_eq!(row[2], serde_json::json!(100));
        assert_eq!(row[3], serde_json::json!("my-key"));
        assert_eq!(row[4], serde_json::json!(msg.value));
        assert_eq!(row[5], serde_json::json!(1700000000000_i64));
    }

    #[test]
    fn test_message_row_to_row_specific_columns() {
        let msg = make_test_row();
        let columns = vec![
            SelectColumn::Column("key".to_string()),
            SelectColumn::Column("offset".to_string()),
        ];
        let row = msg.to_row(&columns);
        assert_eq!(row.len(), 2);
        assert_eq!(row[0], serde_json::json!("my-key"));
        assert_eq!(row[1], serde_json::json!(100));
    }

    #[test]
    fn test_message_row_to_row_null_key() {
        let msg = make_test_row_no_key();
        let row = msg.to_row(&[SelectColumn::Column("key".to_string())]);
        assert_eq!(row.len(), 1);
        assert_eq!(row[0], serde_json::Value::Null);
    }

    #[test]
    fn test_message_row_to_row_null_key_in_all() {
        let msg = make_test_row_no_key();
        let row = msg.to_row(&[SelectColumn::All]);
        // key field (index 3) should be Null
        assert_eq!(row[3], serde_json::Value::Null);
    }

    #[test]
    fn test_message_row_to_row_unknown_column() {
        let msg = make_test_row();
        let row = msg.to_row(&[SelectColumn::Column("nonexistent".to_string())]);
        assert_eq!(row.len(), 1);
        assert_eq!(row[0], serde_json::Value::Null);
    }

    #[test]
    fn test_message_row_to_row_each_column() {
        let msg = make_test_row();
        let cols = ["topic", "partition", "offset", "key", "value", "timestamp"];
        for col_name in &cols {
            let row = msg.to_row(&[SelectColumn::Column(col_name.to_string())]);
            assert_eq!(row.len(), 1, "Column {} should produce 1 value", col_name);
            assert_ne!(
                row[0],
                serde_json::Value::Null,
                "Column {} should not be null",
                col_name
            );
        }
    }

    #[test]
    fn test_message_row_to_row_json_extract() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::JsonExtract {
            column: "value".to_string(),
            path: "$.amount".to_string(),
            alias: None,
        }];
        let row = msg.to_row(&columns);
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].as_f64().unwrap(), 42.5);
    }

    #[test]
    fn test_message_row_to_row_json_extract_string() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::JsonExtract {
            column: "value".to_string(),
            path: "$.status".to_string(),
            alias: Some("s".to_string()),
        }];
        let row = msg.to_row(&columns);
        assert_eq!(row[0], serde_json::json!("shipped"));
    }

    #[test]
    fn test_message_row_to_row_json_extract_missing() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::JsonExtract {
            column: "value".to_string(),
            path: "$.nonexistent".to_string(),
            alias: None,
        }];
        let row = msg.to_row(&columns);
        assert_eq!(row[0], serde_json::Value::Null);
    }

    #[test]
    fn test_message_row_to_row_json_extract_invalid_json() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: "not json at all".to_string(),
            timestamp: 0,
        };
        let columns = vec![SelectColumn::JsonExtract {
            column: "value".to_string(),
            path: "$.anything".to_string(),
            alias: None,
        }];
        let row = msg.to_row(&columns);
        assert_eq!(row[0], serde_json::Value::Null);
    }

    // ========================================================================
    // MessageRow filter tests
    // ========================================================================

    #[test]
    fn test_matches_filter_key_equals_match() {
        let msg = make_test_row();
        assert!(msg.matches_filters(&[Filter::KeyEquals("my-key".to_string())]));
    }

    #[test]
    fn test_matches_filter_key_equals_no_match() {
        let msg = make_test_row();
        assert!(!msg.matches_filters(&[Filter::KeyEquals("other-key".to_string())]));
    }

    #[test]
    fn test_matches_filter_key_equals_null_key() {
        let msg = make_test_row_no_key();
        assert!(!msg.matches_filters(&[Filter::KeyEquals("any".to_string())]));
    }

    #[test]
    fn test_matches_filter_partition_equals() {
        let msg = make_test_row(); // partition = 2
        assert!(msg.matches_filters(&[Filter::PartitionEquals(2)]));
        assert!(!msg.matches_filters(&[Filter::PartitionEquals(0)]));
    }

    #[test]
    fn test_matches_filter_offset_gte() {
        let msg = make_test_row(); // offset = 100
        assert!(msg.matches_filters(&[Filter::OffsetGte(100)]));
        assert!(msg.matches_filters(&[Filter::OffsetGte(50)]));
        assert!(!msg.matches_filters(&[Filter::OffsetGte(101)]));
    }

    #[test]
    fn test_matches_filter_offset_lt() {
        let msg = make_test_row(); // offset = 100
        assert!(msg.matches_filters(&[Filter::OffsetLt(200)]));
        assert!(msg.matches_filters(&[Filter::OffsetLt(101)]));
        assert!(!msg.matches_filters(&[Filter::OffsetLt(100)]));
        assert!(!msg.matches_filters(&[Filter::OffsetLt(50)]));
    }

    #[test]
    fn test_matches_filter_offset_equals() {
        let msg = make_test_row(); // offset = 100
        assert!(msg.matches_filters(&[Filter::OffsetEquals(100)]));
        assert!(!msg.matches_filters(&[Filter::OffsetEquals(99)]));
    }

    #[test]
    fn test_matches_filter_timestamp_gte() {
        let msg = make_test_row(); // timestamp = 1700000000000
        assert!(msg.matches_filters(&[Filter::TimestampGte(1700000000000)]));
        assert!(msg.matches_filters(&[Filter::TimestampGte(1600000000000)]));
        assert!(!msg.matches_filters(&[Filter::TimestampGte(1800000000000)]));
    }

    #[test]
    fn test_matches_filter_timestamp_lt() {
        let msg = make_test_row(); // timestamp = 1700000000000
        assert!(msg.matches_filters(&[Filter::TimestampLt(1800000000000)]));
        assert!(!msg.matches_filters(&[Filter::TimestampLt(1700000000000)]));
    }

    #[test]
    fn test_matches_filter_json_equals_number() {
        let msg = make_test_row();
        assert!(msg.matches_filters(&[Filter::JsonEquals {
            path: "$.amount".to_string(),
            value: serde_json::json!(42.5),
        }]));
        assert!(!msg.matches_filters(&[Filter::JsonEquals {
            path: "$.amount".to_string(),
            value: serde_json::json!(99),
        }]));
    }

    #[test]
    fn test_matches_filter_json_equals_string() {
        let msg = make_test_row();
        assert!(msg.matches_filters(&[Filter::JsonEquals {
            path: "$.status".to_string(),
            value: serde_json::json!("shipped"),
        }]));
        assert!(!msg.matches_filters(&[Filter::JsonEquals {
            path: "$.status".to_string(),
            value: serde_json::json!("pending"),
        }]));
    }

    #[test]
    fn test_matches_filter_json_gt() {
        let msg = make_test_row(); // amount = 42.5
        assert!(msg.matches_filters(&[Filter::JsonGt {
            path: "$.amount".to_string(),
            value: serde_json::json!(40),
        }]));
        assert!(!msg.matches_filters(&[Filter::JsonGt {
            path: "$.amount".to_string(),
            value: serde_json::json!(50),
        }]));
    }

    #[test]
    fn test_matches_filter_json_lt() {
        let msg = make_test_row(); // amount = 42.5
        assert!(msg.matches_filters(&[Filter::JsonLt {
            path: "$.amount".to_string(),
            value: serde_json::json!(50),
        }]));
        assert!(!msg.matches_filters(&[Filter::JsonLt {
            path: "$.amount".to_string(),
            value: serde_json::json!(40),
        }]));
    }

    #[test]
    fn test_matches_filter_json_invalid_value() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: "not json".to_string(),
            timestamp: 0,
        };
        // Should return false, not panic
        assert!(!msg.matches_filters(&[Filter::JsonEquals {
            path: "$.anything".to_string(),
            value: serde_json::json!("x"),
        }]));
        assert!(!msg.matches_filters(&[Filter::JsonGt {
            path: "$.anything".to_string(),
            value: serde_json::json!(0),
        }]));
        assert!(!msg.matches_filters(&[Filter::JsonLt {
            path: "$.anything".to_string(),
            value: serde_json::json!(0),
        }]));
    }

    #[test]
    fn test_matches_filter_multiple_all_match() {
        let msg = make_test_row();
        let filters = vec![
            Filter::PartitionEquals(2),
            Filter::OffsetGte(50),
            Filter::OffsetLt(200),
            Filter::KeyEquals("my-key".to_string()),
        ];
        assert!(msg.matches_filters(&filters));
    }

    #[test]
    fn test_matches_filter_multiple_one_fails() {
        let msg = make_test_row();
        let filters = vec![
            Filter::PartitionEquals(2),             // matches
            Filter::OffsetGte(50),                  // matches
            Filter::KeyEquals("wrong".to_string()), // does NOT match
        ];
        assert!(!msg.matches_filters(&filters));
    }

    #[test]
    fn test_matches_filter_empty_filters() {
        let msg = make_test_row();
        assert!(msg.matches_filters(&[])); // No filters means match everything
    }

    // ========================================================================
    // MessageRow vector operations - edge cases
    // ========================================================================

    #[test]
    fn test_message_row_vector_missing_field() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"name": "test"}"#.to_string(),
            timestamp: 0,
        };
        // When vector field is missing, cosine_similarity returns 0.0
        assert_eq!(msg.compute_cosine_similarity("$.missing", &[1.0, 0.0]), 0.0);
        // euclidean_distance returns f64::MAX
        assert_eq!(
            msg.compute_euclidean_distance("$.missing", &[1.0, 0.0]),
            f64::MAX
        );
        // dot_product returns 0.0
        assert_eq!(msg.compute_dot_product("$.missing", &[1.0, 0.0]), 0.0);
        // vector_norm returns 0.0
        assert_eq!(msg.compute_vector_norm("$.missing"), 0.0);
    }

    #[test]
    fn test_message_row_vector_non_array_field() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"embedding": "not_a_vector"}"#.to_string(),
            timestamp: 0,
        };
        assert_eq!(msg.compute_cosine_similarity("$.embedding", &[1.0]), 0.0);
        assert_eq!(msg.compute_vector_norm("$.embedding"), 0.0);
    }

    // ========================================================================
    // Vector math utilities - additional edge cases
    // ========================================================================

    #[test]
    fn test_cosine_similarity_empty_vectors() {
        assert_eq!(vector_math::cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(vector_math::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(vector_math::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_parallel_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![2.0, 4.0, 6.0]; // Scalar multiple of a
        assert!((vector_math::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_distance_same_point() {
        let a = vec![1.0, 2.0, 3.0];
        assert!(vector_math::euclidean_distance(&a, &a) < 0.001);
    }

    #[test]
    fn test_euclidean_distance_different_lengths() {
        let a = vec![1.0];
        let b = vec![1.0, 2.0];
        assert_eq!(vector_math::euclidean_distance(&a, &b), f64::MAX);
    }

    #[test]
    fn test_euclidean_distance_unit_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        // sqrt(1 + 1) = sqrt(2)
        assert!((vector_math::euclidean_distance(&a, &b) - std::f64::consts::SQRT_2).abs() < 0.001);
    }

    #[test]
    fn test_dot_product_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(vector_math::dot_product(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_dot_product_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0];
        assert_eq!(vector_math::dot_product(&a, &b), 0.0);
    }

    #[test]
    fn test_dot_product_with_negatives() {
        let a = vec![1.0, -1.0];
        let b = vec![-1.0, 1.0];
        // 1*(-1) + (-1)*1 = -2
        assert!((vector_math::dot_product(&a, &b) - (-2.0)).abs() < 0.001);
    }

    #[test]
    fn test_vector_norm_zero_vector() {
        assert_eq!(vector_math::vector_norm(&[0.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn test_vector_norm_unit_vector() {
        assert!((vector_math::vector_norm(&[1.0, 0.0, 0.0]) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_vector_norm_empty() {
        assert_eq!(vector_math::vector_norm(&[]), 0.0);
    }

    #[test]
    fn test_parse_vector_array_of_numbers() {
        let json = serde_json::json!([1.0, 2.0, 3.0]);
        let v = vector_math::parse_vector(&json).unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_parse_vector_array_of_integers() {
        let json = serde_json::json!([1, 2, 3]);
        let v = vector_math::parse_vector(&json).unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_parse_vector_array_with_non_numbers() {
        let json = serde_json::json!([1, "two", 3]);
        assert!(vector_math::parse_vector(&json).is_none());
    }

    #[test]
    fn test_parse_vector_not_array() {
        assert!(vector_math::parse_vector(&serde_json::json!(42)).is_none());
        assert!(vector_math::parse_vector(&serde_json::json!("hello")).is_none());
        assert!(vector_math::parse_vector(&serde_json::json!(null)).is_none());
        assert!(vector_math::parse_vector(&serde_json::json!({})).is_none());
    }

    #[test]
    fn test_parse_vector_empty_array() {
        let json = serde_json::json!([]);
        let v = vector_math::parse_vector(&json).unwrap();
        assert!(v.is_empty());
    }

    // ========================================================================
    // default_columns tests
    // ========================================================================

    #[test]
    fn test_default_columns_count() {
        let cols = default_columns();
        assert_eq!(cols.len(), 6);
    }

    #[test]
    fn test_default_columns_names() {
        let cols = default_columns();
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["topic", "partition", "offset", "key", "value", "timestamp"]
        );
    }

    #[test]
    fn test_default_columns_types() {
        let cols = default_columns();
        assert_eq!(cols[0].data_type, "string");
        assert_eq!(cols[1].data_type, "integer");
        assert_eq!(cols[2].data_type, "bigint");
        assert_eq!(cols[3].data_type, "string");
        assert_eq!(cols[4].data_type, "string");
        assert_eq!(cols[5].data_type, "bigint");
    }

    // ========================================================================
    // ColumnInfo tests
    // ========================================================================

    #[test]
    fn test_column_info_serialize() {
        let col = ColumnInfo {
            name: "offset".to_string(),
            data_type: "bigint".to_string(),
        };
        let json = serde_json::to_value(&col).unwrap();
        assert_eq!(json["name"], "offset");
        assert_eq!(json["data_type"], "bigint");
    }

    #[test]
    fn test_column_info_deserialize() {
        let json = r#"{"name": "key", "data_type": "string"}"#;
        let col: ColumnInfo = serde_json::from_str(json).unwrap();
        assert_eq!(col.name, "key");
        assert_eq!(col.data_type, "string");
    }

    // ========================================================================
    // QueryResult tests
    // ========================================================================

    #[test]
    fn test_query_result_serialize() {
        let result = QueryResult {
            columns: vec![ColumnInfo {
                name: "key".to_string(),
                data_type: "string".to_string(),
            }],
            rows: vec![vec![serde_json::json!("test-key")]],
            row_count: 1,
            execution_time_ms: 42,
            truncated: false,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["rowCount"], 1);
        assert_eq!(json["executionTimeMs"], 42);
        assert_eq!(json["truncated"], false);
        assert_eq!(json["columns"][0]["name"], "key");
        assert_eq!(json["rows"][0][0], "test-key");
    }

    #[test]
    fn test_query_result_empty() {
        let result = QueryResult {
            columns: default_columns(),
            rows: vec![],
            row_count: 0,
            execution_time_ms: 1,
            truncated: false,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["rowCount"], 0);
        assert!(json["rows"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_query_result_truncated() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
            row_count: 10000,
            execution_time_ms: 5000,
            truncated: true,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["truncated"], true);
    }

    #[test]
    fn test_query_result_roundtrip() {
        let result = QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "key".to_string(),
                    data_type: "string".to_string(),
                },
                ColumnInfo {
                    name: "offset".to_string(),
                    data_type: "bigint".to_string(),
                },
            ],
            rows: vec![
                vec![serde_json::json!("k1"), serde_json::json!(100)],
                vec![serde_json::json!("k2"), serde_json::json!(200)],
            ],
            row_count: 2,
            execution_time_ms: 10,
            truncated: false,
        };
        let json_str = serde_json::to_string(&result).unwrap();
        let deserialized: QueryResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.row_count, 2);
        assert_eq!(deserialized.columns.len(), 2);
        assert_eq!(deserialized.rows.len(), 2);
        assert_eq!(deserialized.execution_time_ms, 10);
        assert!(!deserialized.truncated);
    }

    // ========================================================================
    // TopicDescription and PartitionInfo tests
    // ========================================================================

    #[test]
    fn test_topic_description_serialize() {
        let desc = TopicDescription {
            name: "orders".to_string(),
            partition_count: 3,
            total_messages: 50000,
            schema_subject: Some("orders-value".to_string()),
            partitions: vec![
                PartitionInfo {
                    partition_id: 0,
                    high_watermark: 20000,
                    segment_count: 5,
                },
                PartitionInfo {
                    partition_id: 1,
                    high_watermark: 15000,
                    segment_count: 4,
                },
                PartitionInfo {
                    partition_id: 2,
                    high_watermark: 15000,
                    segment_count: 4,
                },
            ],
        };
        let json = serde_json::to_value(&desc).unwrap();
        assert_eq!(json["name"], "orders");
        assert_eq!(json["partitionCount"], 3);
        assert_eq!(json["totalMessages"], 50000);
        assert_eq!(json["schemaSubject"], "orders-value");
        assert_eq!(json["partitions"].as_array().unwrap().len(), 3);
        assert_eq!(json["partitions"][0]["partitionId"], 0);
        assert_eq!(json["partitions"][0]["highWatermark"], 20000);
        assert_eq!(json["partitions"][0]["segmentCount"], 5);
    }

    #[test]
    fn test_topic_description_no_schema() {
        let desc = TopicDescription {
            name: "events".to_string(),
            partition_count: 1,
            total_messages: 0,
            schema_subject: None,
            partitions: vec![],
        };
        let json = serde_json::to_value(&desc).unwrap();
        assert_eq!(json["schemaSubject"], serde_json::Value::Null);
    }

    #[test]
    fn test_topic_description_roundtrip() {
        let desc = TopicDescription {
            name: "test".to_string(),
            partition_count: 2,
            total_messages: 100,
            schema_subject: None,
            partitions: vec![
                PartitionInfo {
                    partition_id: 0,
                    high_watermark: 50,
                    segment_count: 1,
                },
                PartitionInfo {
                    partition_id: 1,
                    high_watermark: 50,
                    segment_count: 1,
                },
            ],
        };
        let json_str = serde_json::to_string(&desc).unwrap();
        let deserialized: TopicDescription = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.partition_count, 2);
        assert_eq!(deserialized.partitions.len(), 2);
    }

    // ========================================================================
    // WindowResult tests
    // ========================================================================

    #[test]
    fn test_window_result_serialize() {
        let wr = WindowResult {
            window_start: 1700000000000,
            window_end: 1700003600000,
            group_key: Some("user-1".to_string()),
            values: vec![serde_json::json!(42), serde_json::json!(99.5)],
        };
        let json = serde_json::to_value(&wr).unwrap();
        assert_eq!(json["windowStart"], 1700000000000_i64);
        assert_eq!(json["windowEnd"], 1700003600000_i64);
        assert_eq!(json["groupKey"], "user-1");
        assert_eq!(json["values"][0], 42);
    }

    #[test]
    fn test_window_result_no_group_key() {
        let wr = WindowResult {
            window_start: 0,
            window_end: 3600000,
            group_key: None,
            values: vec![serde_json::json!(10)],
        };
        let json = serde_json::to_value(&wr).unwrap();
        assert_eq!(json["groupKey"], serde_json::Value::Null);
    }

    #[test]
    fn test_window_result_roundtrip() {
        let wr = WindowResult {
            window_start: 1000,
            window_end: 2000,
            group_key: Some("g1".to_string()),
            values: vec![serde_json::json!(5), serde_json::json!("hello")],
        };
        let json_str = serde_json::to_string(&wr).unwrap();
        let deserialized: WindowResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.window_start, 1000);
        assert_eq!(deserialized.window_end, 2000);
        assert_eq!(deserialized.group_key, Some("g1".to_string()));
        assert_eq!(deserialized.values.len(), 2);
    }

    // ========================================================================
    // MaterializedViewInfo tests
    // ========================================================================

    #[test]
    fn test_materialized_view_info_serialize() {
        let info = MaterializedViewInfo {
            name: "hourly_sales".to_string(),
            source_topic: "orders".to_string(),
            query_sql: "SELECT COUNT(*) FROM orders GROUP BY TUMBLE(timestamp, '1 hour')"
                .to_string(),
            refresh_mode: RefreshMode::Continuous,
            status: MaterializedViewStatus::Running,
            last_refresh_at: Some(1700000000000),
            last_offsets: {
                let mut m = std::collections::HashMap::new();
                m.insert(0, 1000);
                m.insert(1, 2000);
                m
            },
            row_count: 500,
            created_at: 1699000000000,
            updated_at: 1700000000000,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "hourly_sales");
        assert_eq!(json["sourceTopic"], "orders");
        assert_eq!(json["refreshMode"], "continuous");
        assert_eq!(json["status"], "running");
        assert_eq!(json["rowCount"], 500);
    }

    #[test]
    fn test_materialized_view_info_roundtrip() {
        let info = MaterializedViewInfo {
            name: "test_view".to_string(),
            source_topic: "events".to_string(),
            query_sql: "SELECT COUNT(*) FROM events".to_string(),
            refresh_mode: RefreshMode::Manual,
            status: MaterializedViewStatus::Initializing,
            last_refresh_at: None,
            last_offsets: std::collections::HashMap::new(),
            row_count: 0,
            created_at: 1000,
            updated_at: 1000,
        };
        let json_str = serde_json::to_string(&info).unwrap();
        let deserialized: MaterializedViewInfo = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.name, "test_view");
        assert_eq!(deserialized.source_topic, "events");
        assert_eq!(deserialized.refresh_mode, RefreshMode::Manual);
        assert_eq!(deserialized.status, MaterializedViewStatus::Initializing);
        assert!(deserialized.last_refresh_at.is_none());
        assert_eq!(deserialized.row_count, 0);
    }

    // ========================================================================
    // MessageRow with context - zscore and anomaly via filters
    // ========================================================================

    #[test]
    fn test_matches_filter_zscore_gt_with_context() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"price": 200.0}"#.to_string(),
            timestamp: 0,
        };

        let mut ctx = RowContext::new();
        let mut stats = FieldStatistics::new();
        // Tight distribution around 100
        for v in [99.0, 100.0, 101.0, 100.0, 100.0, 99.5, 100.5, 100.0] {
            stats.add(v);
        }
        ctx.stats.insert("$.price".to_string(), stats);

        let filter = Filter::ZScoreGt {
            path: "$.price".to_string(),
            threshold: 2.0,
        };
        // 200 is way above mean of ~100, so z-score should be > 2
        assert!(msg.matches_filters_with_context(&[filter], Some(&ctx)));
    }

    #[test]
    fn test_matches_filter_zscore_gt_not_exceeded() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"price": 100.5}"#.to_string(),
            timestamp: 0,
        };

        let mut ctx = RowContext::new();
        let mut stats = FieldStatistics::new();
        for v in [99.0, 100.0, 101.0, 100.0, 100.0, 99.5, 100.5, 100.0] {
            stats.add(v);
        }
        ctx.stats.insert("$.price".to_string(), stats);

        let filter = Filter::ZScoreGt {
            path: "$.price".to_string(),
            threshold: 2.0,
        };
        // 100.5 is very close to mean, z-score should not exceed 2
        assert!(!msg.matches_filters_with_context(&[filter], Some(&ctx)));
    }

    #[test]
    fn test_matches_filter_anomaly_threshold_with_context() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"temp": 500.0}"#.to_string(),
            timestamp: 0,
        };

        let mut ctx = RowContext::new();
        let mut stats = FieldStatistics::new();
        for v in [20.0, 21.0, 19.0, 20.5, 20.0, 19.5, 20.5, 20.0] {
            stats.add(v);
        }
        ctx.stats.insert("$.temp".to_string(), stats);

        let filter = Filter::AnomalyThreshold {
            path: "$.temp".to_string(),
            threshold: 3.0,
        };
        // 500 is extremely far from mean of ~20
        assert!(msg.matches_filters_with_context(&[filter], Some(&ctx)));
    }

    #[test]
    fn test_matches_filter_cosine_similarity_gt() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"vec": [1.0, 0.0, 0.0]}"#.to_string(),
            timestamp: 0,
        };

        // Query vector same direction => similarity ~= 1.0
        let filter = Filter::CosineSimilarityGt {
            path: "$.vec".to_string(),
            query_vector: vec![1.0, 0.0, 0.0],
            threshold: 0.9,
        };
        assert!(msg.matches_filters(&[filter]));

        // Query vector orthogonal => similarity ~= 0.0
        let filter2 = Filter::CosineSimilarityGt {
            path: "$.vec".to_string(),
            query_vector: vec![0.0, 1.0, 0.0],
            threshold: 0.5,
        };
        assert!(!msg.matches_filters(&[filter2]));
    }

    #[test]
    fn test_matches_filter_euclidean_distance_lt() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"point": [1.0, 1.0]}"#.to_string(),
            timestamp: 0,
        };

        // Close point
        let filter = Filter::EuclideanDistanceLt {
            path: "$.point".to_string(),
            query_vector: vec![1.0, 1.1],
            threshold: 0.5,
        };
        assert!(msg.matches_filters(&[filter]));

        // Far point
        let filter2 = Filter::EuclideanDistanceLt {
            path: "$.point".to_string(),
            query_vector: vec![100.0, 100.0],
            threshold: 0.5,
        };
        assert!(!msg.matches_filters(&[filter2]));
    }

    // ========================================================================
    // MessageRow to_row with multiple SelectColumn::All
    // ========================================================================

    #[test]
    fn test_message_row_to_row_multiple_all() {
        let msg = make_test_row();
        // Two All columns should produce 12 values (6 * 2)
        let row = msg.to_row(&[SelectColumn::All, SelectColumn::All]);
        assert_eq!(row.len(), 12);
    }

    #[test]
    fn test_message_row_to_row_mixed_all_and_specific() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::All, SelectColumn::Column("key".to_string())];
        let row = msg.to_row(&columns);
        assert_eq!(row.len(), 7); // 6 from All + 1 from Column
    }

    // ========================================================================
    // MessageRow extract_numeric (tested indirectly via zscore)
    // ========================================================================

    #[test]
    fn test_message_row_zscore_missing_field() {
        let msg = MessageRow {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            value: r#"{"other": 42}"#.to_string(),
            timestamp: 0,
        };

        let mut ctx = RowContext::new();
        let mut stats = FieldStatistics::new();
        stats.add(10.0);
        stats.add(20.0);
        ctx.stats.insert("$.missing".to_string(), stats);

        let columns = vec![SelectColumn::ZScore {
            path: "$.missing".to_string(),
            alias: None,
        }];
        let row = msg.to_row_with_context(&columns, Some(&ctx));
        // Value extraction will fail, use 0.0 as default
        assert_eq!(row.len(), 1);
    }

    #[test]
    fn test_message_row_zscore_no_context() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::ZScore {
            path: "$.amount".to_string(),
            alias: None,
        }];
        // Without context, zscore should return 0.0
        let row = msg.to_row(&columns);
        assert_eq!(row[0].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn test_message_row_anomaly_no_context() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::Anomaly {
            path: "$.amount".to_string(),
            threshold: 2.0,
            alias: None,
        }];
        // Without context, anomaly should return false
        let row = msg.to_row(&columns);
        assert_eq!(row[0], serde_json::Value::Bool(false));
    }

    #[test]
    fn test_message_row_stddev_no_context() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::Stddev {
            path: "$.amount".to_string(),
            alias: None,
        }];
        let row = msg.to_row(&columns);
        assert_eq!(row[0].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn test_message_row_avg_no_context() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::Avg {
            path: "$.amount".to_string(),
            alias: None,
        }];
        let row = msg.to_row(&columns);
        assert_eq!(row[0].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn test_message_row_moving_avg_no_context() {
        let msg = make_test_row();
        let columns = vec![SelectColumn::MovingAvg {
            path: "$.amount".to_string(),
            window_size: 5,
            alias: None,
        }];
        let row = msg.to_row(&columns);
        assert_eq!(row[0].as_f64().unwrap(), 0.0);
    }
}
