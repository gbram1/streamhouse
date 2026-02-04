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
    CountDistinct { column: String, alias: Option<String> },
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
    ZScore {
        path: String,
        alias: Option<String>,
    },
    /// Moving average: moving_avg(json_extract(value, '$.path'), window_size)
    MovingAvg {
        path: String,
        window_size: usize,
        alias: Option<String>,
    },
    /// Standard deviation: stddev(json_extract(value, '$.path'))
    Stddev {
        path: String,
        alias: Option<String>,
    },
    /// Mean/average: avg(json_extract(value, '$.path'))
    Avg {
        path: String,
        alias: Option<String>,
    },
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
    VectorNorm {
        path: String,
        alias: Option<String>,
    },
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
    ZScoreGt {
        path: String,
        threshold: f64,
    },
    /// zscore(json_extract(value, '$.path')) < threshold
    ZScoreLt {
        path: String,
        threshold: f64,
    },
    /// |zscore(json_extract(value, '$.path'))| > threshold
    /// Filters for statistical outliers (anomalies)
    AnomalyThreshold {
        path: String,
        threshold: f64,
    },
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
                    let extracted = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.value) {
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
                SelectColumn::MovingAvg { path, window_size, .. } => {
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
                SelectColumn::Anomaly { path, threshold, .. } => {
                    let is_anomaly = self.is_anomaly(path, *threshold, ctx);
                    vec![serde_json::Value::Bool(is_anomaly)]
                }
                SelectColumn::CosineSimilarity { path, query_vector, .. } => {
                    let similarity = self.compute_cosine_similarity(path, query_vector);
                    vec![serde_json::json!(similarity)]
                }
                SelectColumn::EuclideanDistance { path, query_vector, .. } => {
                    let distance = self.compute_euclidean_distance(path, query_vector);
                    vec![serde_json::json!(distance)]
                }
                SelectColumn::DotProduct { path, query_vector, .. } => {
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
    fn calculate_moving_avg(&self, path: &str, window_size: usize, ctx: Option<&RowContext>) -> f64 {
        ctx.and_then(|c| {
            c.stats.get(path).map(|s| s.moving_avg(window_size, c.row_index))
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
    pub fn matches_filters_with_context(&self, filters: &[Filter], ctx: Option<&RowContext>) -> bool {
        filters.iter().all(|filter| self.matches_filter_with_context(filter, ctx))
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
            Filter::AnomalyThreshold { path, threshold } => {
                self.is_anomaly(path, *threshold, ctx)
            }
            Filter::CosineSimilarityGt { path, query_vector, threshold } => {
                let similarity = self.compute_cosine_similarity(path, query_vector);
                similarity > *threshold
            }
            Filter::EuclideanDistanceLt { path, query_vector, threshold } => {
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
            serde_json::Value::Object(map) => map.get(part).cloned().unwrap_or(serde_json::Value::Null),
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
    pub values: Vec<f64>,  // For moving average calculations
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
        let start = if index >= window { index - window + 1 } else { 0 };
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
}
