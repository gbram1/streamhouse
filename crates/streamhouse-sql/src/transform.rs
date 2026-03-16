//! SQL transform engine for applying SQL transformations to SinkRecords.
//!
//! Converts `SinkRecord` batches into Arrow `RecordBatch`es, executes arbitrary
//! SQL SELECT queries via DataFusion, and converts the results back into
//! `SinkRecord`s. This enables pipeline-level SQL transforms on streaming data.

use std::sync::Arc;

use arrow::array::{Array, Int64Array, StringArray, UInt32Array, UInt64Array};
use datafusion::datasource::MemTable;
use datafusion::execution::context::SessionContext;

use bytes::Bytes;
use streamhouse_connectors::SinkRecord;

use crate::arrow_executor::ArrowExecutor;
use crate::error::SqlError;
use crate::types::MessageRow;
use crate::Result;

/// SQL transform engine that applies SQL queries to batches of records.
pub struct TransformEngine;

impl TransformEngine {
    /// Create a new `TransformEngine`.
    pub fn new() -> Self {
        Self
    }

    /// Validate that `sql` is a syntactically valid SELECT statement.
    ///
    /// Returns `Ok(())` when the SQL parses successfully as a SELECT.
    /// Returns an error for non-SELECT statements or syntax errors.
    pub fn validate_sql(sql: &str) -> Result<()> {
        use sqlparser::dialect::GenericDialect;
        use sqlparser::parser::Parser;

        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, sql)
            .map_err(|e| SqlError::ParseError(format!("Invalid SQL: {e}")))?;

        if statements.is_empty() {
            return Err(SqlError::ParseError("Empty SQL statement".to_string()));
        }

        if statements.len() > 1 {
            return Err(SqlError::ParseError(
                "Only a single SELECT statement is allowed".to_string(),
            ));
        }

        match &statements[0] {
            sqlparser::ast::Statement::Query(_) => Ok(()),
            other => Err(SqlError::InvalidQuery(format!(
                "Expected a SELECT statement, got: {other}"
            ))),
        }
    }

    /// Apply a SQL transform to a batch of `SinkRecord`s.
    ///
    /// The records are registered as a table called `input` with columns:
    /// `topic`, `partition`, `offset`, `key`, `value`, `timestamp`.
    ///
    /// The SQL query should reference the `input` table, e.g.:
    /// ```sql
    /// SELECT * FROM input WHERE value LIKE '%error%'
    /// ```
    ///
    /// Result rows are converted back to `SinkRecord`s. If the result contains
    /// a `value` column it is used as the record value; otherwise the entire
    /// row is serialized as JSON.
    pub async fn apply_transform(records: &[SinkRecord], sql: &str) -> Result<Vec<SinkRecord>> {
        if records.is_empty() {
            return Ok(Vec::new());
        }

        // Validate the SQL first.
        Self::validate_sql(sql)?;

        // Convert SinkRecord -> MessageRow
        let message_rows: Vec<MessageRow> = records
            .iter()
            .map(|r| MessageRow {
                topic: r.topic.clone(),
                partition: r.partition,
                offset: r.offset,
                key: r
                    .key
                    .as_ref()
                    .map(|k| String::from_utf8_lossy(k).to_string()),
                value: String::from_utf8_lossy(&r.value).to_string(),
                timestamp: r.timestamp as i64,
            })
            .collect();

        // Convert to Arrow RecordBatch
        let batch = ArrowExecutor::messages_to_record_batch(&message_rows)?;
        let schema = batch.schema();

        // Register the batch as a table named "input"
        let ctx = SessionContext::new();
        let provider = MemTable::try_new(schema, vec![vec![batch]])
            .map_err(|e| SqlError::DataFusionError(e.to_string()))?;
        ctx.register_table("input", Arc::new(provider))
            .map_err(|e| SqlError::DataFusionError(e.to_string()))?;

        // Execute the SQL query
        let df = ctx
            .sql(sql)
            .await
            .map_err(|e| SqlError::DataFusionError(e.to_string()))?;

        let result_batches = df
            .collect()
            .await
            .map_err(|e| SqlError::DataFusionError(e.to_string()))?;

        // Convert result batches back to SinkRecords
        let mut output = Vec::new();

        for result_batch in &result_batches {
            let result_schema = result_batch.schema();
            let num_rows = result_batch.num_rows();

            // Detect column indices by name for known fields
            let topic_idx = result_schema.index_of("topic").ok();
            let partition_idx = result_schema.index_of("partition").ok();
            let offset_idx = result_schema.index_of("offset").ok();
            let key_idx = result_schema.index_of("key").ok();
            let value_idx = result_schema.index_of("value").ok();
            let timestamp_idx = result_schema.index_of("timestamp").ok();

            for row in 0..num_rows {
                // Extract topic
                let topic = topic_idx
                    .and_then(|i| {
                        result_batch
                            .column(i)
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .map(|a| a.value(row).to_string())
                    })
                    .unwrap_or_default();

                // Extract partition
                let partition = partition_idx
                    .and_then(|i| {
                        result_batch
                            .column(i)
                            .as_any()
                            .downcast_ref::<UInt32Array>()
                            .map(|a| a.value(row))
                    })
                    .unwrap_or(0);

                // Extract offset
                let offset = offset_idx
                    .and_then(|i| {
                        result_batch
                            .column(i)
                            .as_any()
                            .downcast_ref::<UInt64Array>()
                            .map(|a| a.value(row))
                    })
                    .unwrap_or(0);

                // Extract key
                let key = key_idx.and_then(|i| {
                    let col = result_batch.column(i);
                    if col.is_null(row) {
                        None
                    } else {
                        col.as_any()
                            .downcast_ref::<StringArray>()
                            .map(|a| Bytes::from(a.value(row).to_string()))
                    }
                });

                // Extract value: use the `value` column if present, otherwise
                // serialize the entire row as a JSON object.
                let value = if let Some(vi) = value_idx {
                    let col = result_batch.column(vi);
                    col.as_any()
                        .downcast_ref::<StringArray>()
                        .map(|a| a.value(row).to_string())
                        .unwrap_or_default()
                } else {
                    // Serialize all columns as a JSON object
                    let mut map = serde_json::Map::new();
                    for (col_idx, field) in result_schema.fields().iter().enumerate() {
                        let col = result_batch.column(col_idx);
                        let val = column_value_to_json(col, row);
                        map.insert(field.name().clone(), val);
                    }
                    serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default()
                };

                // Extract timestamp
                let timestamp = timestamp_idx
                    .and_then(|i| {
                        result_batch
                            .column(i)
                            .as_any()
                            .downcast_ref::<Int64Array>()
                            .map(|a| a.value(row) as u64)
                    })
                    .unwrap_or(0);

                output.push(SinkRecord {
                    topic,
                    partition,
                    offset,
                    timestamp,
                    key,
                    value: Bytes::from(value),
                });
            }
        }

        Ok(output)
    }
}

impl Default for TransformEngine {
    fn default() -> Self {
        Self
    }
}

/// Convert an Arrow column value at a given row index to a `serde_json::Value`.
fn column_value_to_json(array: &Arc<dyn Array>, index: usize) -> serde_json::Value {
    if array.is_null(index) {
        return serde_json::Value::Null;
    }

    if let Some(a) = array.as_any().downcast_ref::<StringArray>() {
        return serde_json::Value::String(a.value(index).to_string());
    }
    if let Some(a) = array.as_any().downcast_ref::<UInt64Array>() {
        return serde_json::Value::Number(a.value(index).into());
    }
    if let Some(a) = array.as_any().downcast_ref::<UInt32Array>() {
        return serde_json::Value::Number(a.value(index).into());
    }
    if let Some(a) = array.as_any().downcast_ref::<Int64Array>() {
        return serde_json::Value::Number(a.value(index).into());
    }
    if let Some(a) = array.as_any().downcast_ref::<arrow::array::Float64Array>() {
        let val = a.value(index);
        return serde_json::Number::from_f64(val)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null);
    }
    if let Some(a) = array.as_any().downcast_ref::<arrow::array::BooleanArray>() {
        return serde_json::Value::Bool(a.value(index));
    }

    serde_json::Value::String(format!("{array:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_sql_valid_select() {
        assert!(TransformEngine::validate_sql("SELECT * FROM input").is_ok());
    }

    #[test]
    fn test_validate_sql_with_where() {
        assert!(
            TransformEngine::validate_sql("SELECT value FROM input WHERE partition = 0").is_ok()
        );
    }

    #[test]
    fn test_validate_sql_rejects_insert() {
        assert!(TransformEngine::validate_sql("INSERT INTO foo VALUES (1)").is_err());
    }

    #[test]
    fn test_validate_sql_rejects_empty() {
        assert!(TransformEngine::validate_sql("").is_err());
    }

    #[test]
    fn test_validate_sql_rejects_syntax_error() {
        assert!(TransformEngine::validate_sql("SELECTT * FORM input").is_err());
    }

    #[test]
    fn test_validate_sql_rejects_multiple_statements() {
        assert!(TransformEngine::validate_sql("SELECT 1; SELECT 2").is_err());
    }

    #[tokio::test]
    async fn test_apply_transform_empty_records() {
        let result = TransformEngine::apply_transform(&[], "SELECT * FROM input")
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_apply_transform_passthrough() {
        let records = vec![SinkRecord {
            topic: "events".to_string(),
            partition: 0,
            offset: 1,
            timestamp: 1000,
            key: Some(Bytes::from("k1")),
            value: Bytes::from(r#"{"action":"click"}"#),
        }];

        let result = TransformEngine::apply_transform(&records, "SELECT * FROM input")
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].topic, "events");
        assert_eq!(result[0].partition, 0);
        assert_eq!(result[0].offset, 1);
        assert_eq!(result[0].value, Bytes::from(r#"{"action":"click"}"#));
    }

    #[tokio::test]
    async fn test_apply_transform_filter() {
        let records = vec![
            SinkRecord {
                topic: "events".to_string(),
                partition: 0,
                offset: 1,
                timestamp: 1000,
                key: None,
                value: Bytes::from("keep"),
            },
            SinkRecord {
                topic: "events".to_string(),
                partition: 0,
                offset: 2,
                timestamp: 2000,
                key: None,
                value: Bytes::from("drop"),
            },
        ];

        let result =
            TransformEngine::apply_transform(&records, "SELECT * FROM input WHERE value = 'keep'")
                .await
                .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, Bytes::from("keep"));
    }

    #[tokio::test]
    async fn test_apply_transform_rejects_invalid_sql() {
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("v"),
        }];

        let result = TransformEngine::apply_transform(&records, "DROP TABLE input").await;
        assert!(result.is_err());
    }
}
