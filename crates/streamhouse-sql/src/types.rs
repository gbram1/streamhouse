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
            })
            .collect()
    }

    /// Check if row matches all filters
    pub fn matches_filters(&self, filters: &[Filter]) -> bool {
        filters.iter().all(|filter| self.matches_filter(filter))
    }

    fn matches_filter(&self, filter: &Filter) -> bool {
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
