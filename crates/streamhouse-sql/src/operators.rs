//! Streaming operators for continuous stream processing
//!
//! Provides composable operators that transform, filter, window-aggregate,
//! and join streaming records. Operators are chained together via
//! [`OperatorChain`] to build processing pipelines.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::SqlError;
use crate::streaming::StateStore;
use crate::types::{JoinType, WindowType};
use crate::watermark::WatermarkTracker;
use crate::Result;

// ---------------------------------------------------------------------------
// StreamRecord
// ---------------------------------------------------------------------------

/// A single record flowing through the operator pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRecord {
    /// Optional key (e.g. for partitioning or joining).
    pub key: Option<Vec<u8>>,
    /// The payload bytes.
    pub value: Vec<u8>,
    /// Event timestamp in milliseconds since epoch.
    pub timestamp: i64,
    /// Partition the record originated from.
    pub partition: u32,
    /// Offset within the partition.
    pub offset: u64,
    /// Arbitrary string headers / metadata.
    pub headers: HashMap<String, String>,
}

impl StreamRecord {
    /// Convenience constructor.
    pub fn new(value: Vec<u8>, timestamp: i64) -> Self {
        Self {
            key: None,
            value,
            timestamp,
            partition: 0,
            offset: 0,
            headers: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Operator trait
// ---------------------------------------------------------------------------

/// An asynchronous streaming operator that transforms records.
#[async_trait]
pub trait Operator: Send + Sync {
    /// Process a single input record and return zero or more output records.
    async fn process(&mut self, record: StreamRecord) -> Result<Vec<StreamRecord>>;

    /// Flush any buffered state (e.g. when a window closes).
    async fn flush(&mut self) -> Result<Vec<StreamRecord>>;
}

// ---------------------------------------------------------------------------
// FilterOperator
// ---------------------------------------------------------------------------

/// Predicate tree for filtering JSON-encoded record values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterPredicate {
    /// Field equals a JSON value.
    Equals { field: String, value: serde_json::Value },
    /// Field is greater than a numeric JSON value.
    GreaterThan { field: String, value: serde_json::Value },
    /// Field is less than a numeric JSON value.
    LessThan { field: String, value: serde_json::Value },
    /// Field (string) contains a substring.
    Contains { field: String, substring: String },
    /// Logical AND of two predicates.
    And(Box<FilterPredicate>, Box<FilterPredicate>),
    /// Logical OR of two predicates.
    Or(Box<FilterPredicate>, Box<FilterPredicate>),
    /// Logical NOT of a predicate.
    Not(Box<FilterPredicate>),
}

impl FilterPredicate {
    /// Evaluate the predicate against a JSON value.
    pub fn evaluate(&self, json: &serde_json::Value) -> bool {
        match self {
            FilterPredicate::Equals { field, value } => {
                json.get(field).map_or(false, |v| v == value)
            }
            FilterPredicate::GreaterThan { field, value } => {
                match (json.get(field).and_then(|v| v.as_f64()), value.as_f64()) {
                    (Some(a), Some(b)) => a > b,
                    _ => false,
                }
            }
            FilterPredicate::LessThan { field, value } => {
                match (json.get(field).and_then(|v| v.as_f64()), value.as_f64()) {
                    (Some(a), Some(b)) => a < b,
                    _ => false,
                }
            }
            FilterPredicate::Contains { field, substring } => json
                .get(field)
                .and_then(|v| v.as_str())
                .map_or(false, |s| s.contains(substring.as_str())),
            FilterPredicate::And(left, right) => left.evaluate(json) && right.evaluate(json),
            FilterPredicate::Or(left, right) => left.evaluate(json) || right.evaluate(json),
            FilterPredicate::Not(inner) => !inner.evaluate(json),
        }
    }
}

/// An operator that filters records based on a [`FilterPredicate`] tree
/// evaluated against JSON-encoded values.
pub struct FilterOperator {
    predicate: FilterPredicate,
}

impl FilterOperator {
    pub fn new(predicate: FilterPredicate) -> Self {
        Self { predicate }
    }
}

#[async_trait]
impl Operator for FilterOperator {
    async fn process(&mut self, record: StreamRecord) -> Result<Vec<StreamRecord>> {
        let json: serde_json::Value =
            serde_json::from_slice(&record.value).map_err(|e| {
                SqlError::ExecutionError(format!("failed to parse record as JSON: {e}"))
            })?;
        if self.predicate.evaluate(&json) {
            Ok(vec![record])
        } else {
            Ok(vec![])
        }
    }

    async fn flush(&mut self) -> Result<Vec<StreamRecord>> {
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// ProjectOperator
// ---------------------------------------------------------------------------

/// Field projection specification: select a field and optionally rename it.
#[derive(Debug, Clone)]
pub struct FieldProjection {
    /// Source field name in the JSON value.
    pub source: String,
    /// Target field name (if `None`, keeps the source name).
    pub target: Option<String>,
}

/// An operator that projects (selects and optionally renames) fields from
/// JSON-encoded record values.
pub struct ProjectOperator {
    projections: Vec<FieldProjection>,
}

impl ProjectOperator {
    pub fn new(projections: Vec<FieldProjection>) -> Self {
        Self { projections }
    }
}

#[async_trait]
impl Operator for ProjectOperator {
    async fn process(&mut self, record: StreamRecord) -> Result<Vec<StreamRecord>> {
        let json: serde_json::Value =
            serde_json::from_slice(&record.value).map_err(|e| {
                SqlError::ExecutionError(format!("failed to parse record as JSON: {e}"))
            })?;

        let mut projected = serde_json::Map::new();
        for proj in &self.projections {
            if let Some(val) = json.get(&proj.source) {
                let target_name = proj.target.as_deref().unwrap_or(&proj.source);
                projected.insert(target_name.to_string(), val.clone());
            }
        }

        let new_value = serde_json::to_vec(&serde_json::Value::Object(projected))
            .map_err(|e| SqlError::ExecutionError(format!("failed to serialize projection: {e}")))?;

        Ok(vec![StreamRecord {
            value: new_value,
            ..record
        }])
    }

    async fn flush(&mut self) -> Result<Vec<StreamRecord>> {
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// WindowAggregateOperator
// ---------------------------------------------------------------------------

/// Aggregate functions supported by the window operator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregateFunction {
    Count,
    Sum { field: String },
    Avg { field: String },
    Min { field: String },
    Max { field: String },
}

/// Internal per-window accumulator.
#[derive(Debug, Clone, Default)]
struct WindowAccumulator {
    count: u64,
    sum: f64,
    min: Option<f64>,
    max: Option<f64>,
}

/// A time-windowed aggregation operator.
///
/// Supports tumbling, hopping, and session windows. Integrates with
/// [`StateStore`] for durable state and [`WatermarkTracker`] for event-time
/// progress tracking.
pub struct WindowAggregateOperator {
    window_type: WindowType,
    aggregate_fn: AggregateFunction,
    _state_store: Arc<Box<dyn StateStore>>,
    watermark_tracker: Arc<WatermarkTracker>,
    /// window_start_ms -> accumulator
    windows: HashMap<i64, WindowAccumulator>,
}

impl WindowAggregateOperator {
    pub fn new(
        window_type: WindowType,
        aggregate_fn: AggregateFunction,
        state_store: Arc<Box<dyn StateStore>>,
        watermark_tracker: Arc<WatermarkTracker>,
    ) -> Self {
        Self {
            window_type,
            aggregate_fn,
            _state_store: state_store,
            watermark_tracker,
            windows: HashMap::new(),
        }
    }

    /// Determine which window(s) a timestamp belongs to.
    fn assign_windows(&self, timestamp_ms: i64) -> Vec<i64> {
        match &self.window_type {
            WindowType::Tumble { size_ms } => {
                let start = (timestamp_ms / size_ms) * size_ms;
                vec![start]
            }
            WindowType::Hop { size_ms, slide_ms } => {
                let mut starts = Vec::new();
                // The record falls in every window where
                // window_start <= timestamp < window_start + size_ms
                let earliest = ((timestamp_ms - size_ms + slide_ms) / slide_ms) * slide_ms;
                let mut w = earliest;
                while w <= timestamp_ms {
                    if timestamp_ms < w + size_ms {
                        starts.push(w);
                    }
                    w += slide_ms;
                }
                starts
            }
            WindowType::Session { gap_ms } => {
                // Session windows: find an existing window within gap or start new
                let mut found = None;
                for &start in self.windows.keys() {
                    // Simple heuristic: if the timestamp falls within gap of this
                    // window start we merge into it.
                    if (timestamp_ms - start).abs() <= *gap_ms {
                        found = Some(start);
                        break;
                    }
                }
                vec![found.unwrap_or(timestamp_ms)]
            }
        }
    }

    /// Extract the numeric value for an aggregate field from a JSON record.
    fn extract_value(&self, json: &serde_json::Value) -> Option<f64> {
        match &self.aggregate_fn {
            AggregateFunction::Count => None,
            AggregateFunction::Sum { field }
            | AggregateFunction::Avg { field }
            | AggregateFunction::Min { field }
            | AggregateFunction::Max { field } => json.get(field).and_then(|v| v.as_f64()),
        }
    }

    /// Emit completed windows (those behind the watermark).
    async fn emit_completed_windows(&mut self) -> Result<Vec<StreamRecord>> {
        let global_wm = self
            .watermark_tracker
            .get_global_watermark()
            .await
            .unwrap_or(i64::MIN);

        let window_size = match &self.window_type {
            WindowType::Tumble { size_ms } => *size_ms,
            WindowType::Hop { size_ms, .. } => *size_ms,
            WindowType::Session { gap_ms } => *gap_ms,
        };

        let completed_starts: Vec<i64> = self
            .windows
            .keys()
            .filter(|&&start| start + window_size <= global_wm)
            .cloned()
            .collect();

        let mut output = Vec::new();
        for start in completed_starts {
            if let Some(acc) = self.windows.remove(&start) {
                let result_value = self.compute_result(&acc);
                let result_json = serde_json::json!({
                    "window_start": start,
                    "window_end": start + window_size,
                    "result": result_value,
                    "count": acc.count,
                });
                let value = serde_json::to_vec(&result_json).map_err(|e| {
                    SqlError::ExecutionError(format!("failed to serialize window result: {e}"))
                })?;
                output.push(StreamRecord::new(value, start));
            }
        }

        Ok(output)
    }

    fn compute_result(&self, acc: &WindowAccumulator) -> f64 {
        match &self.aggregate_fn {
            AggregateFunction::Count => acc.count as f64,
            AggregateFunction::Sum { .. } => acc.sum,
            AggregateFunction::Avg { .. } => {
                if acc.count > 0 {
                    acc.sum / acc.count as f64
                } else {
                    0.0
                }
            }
            AggregateFunction::Min { .. } => acc.min.unwrap_or(0.0),
            AggregateFunction::Max { .. } => acc.max.unwrap_or(0.0),
        }
    }
}

#[async_trait]
impl Operator for WindowAggregateOperator {
    async fn process(&mut self, record: StreamRecord) -> Result<Vec<StreamRecord>> {
        let json: serde_json::Value = serde_json::from_slice(&record.value).unwrap_or_default();
        let numeric_val = self.extract_value(&json);

        // Update watermark
        let _ = self
            .watermark_tracker
            .update_watermark("stream", record.partition, record.timestamp)
            .await;

        // Assign to windows
        let window_starts = self.assign_windows(record.timestamp);
        for start in window_starts {
            let acc = self.windows.entry(start).or_default();
            acc.count += 1;
            if let Some(v) = numeric_val {
                acc.sum += v;
                acc.min = Some(acc.min.map_or(v, |m: f64| m.min(v)));
                acc.max = Some(acc.max.map_or(v, |m: f64| m.max(v)));
            }
        }

        // Try to emit completed windows
        self.emit_completed_windows().await
    }

    async fn flush(&mut self) -> Result<Vec<StreamRecord>> {
        // Force-emit all remaining windows
        let mut output = Vec::new();
        let window_size = match &self.window_type {
            WindowType::Tumble { size_ms } => *size_ms,
            WindowType::Hop { size_ms, .. } => *size_ms,
            WindowType::Session { gap_ms } => *gap_ms,
        };

        let starts: Vec<i64> = self.windows.keys().cloned().collect();
        for start in starts {
            if let Some(acc) = self.windows.remove(&start) {
                let result_value = self.compute_result(&acc);
                let result_json = serde_json::json!({
                    "window_start": start,
                    "window_end": start + window_size,
                    "result": result_value,
                    "count": acc.count,
                });
                let value = serde_json::to_vec(&result_json).map_err(|e| {
                    SqlError::ExecutionError(format!("failed to serialize window result: {e}"))
                })?;
                output.push(StreamRecord::new(value, start));
            }
        }
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// JoinOperator
// ---------------------------------------------------------------------------

/// Configuration for a stream-stream join.
#[derive(Debug, Clone)]
pub struct JoinConfig {
    /// JSON field path to extract the join key from the left stream.
    pub left_key: String,
    /// JSON field path to extract the join key from the right stream.
    pub right_key: String,
    /// Time window in milliseconds for join buffer expiry.
    pub window_ms: i64,
    /// Type of join to perform.
    pub join_type: JoinType,
}

/// Indicates which side of the join a record comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinSide {
    Left,
    Right,
}

/// Buffered record in the join operator.
#[derive(Debug, Clone)]
struct JoinBufferEntry {
    record: StreamRecord,
    join_key: serde_json::Value,
    json: serde_json::Value,
}

/// Stream-stream join operator with time-bounded buffers.
pub struct JoinOperator {
    config: JoinConfig,
    left_buffer: Vec<JoinBufferEntry>,
    right_buffer: Vec<JoinBufferEntry>,
    _state_store: Arc<Box<dyn StateStore>>,
    _namespace: String,
}

impl JoinOperator {
    pub fn new(
        config: JoinConfig,
        state_store: Arc<Box<dyn StateStore>>,
        namespace: String,
    ) -> Self {
        Self {
            config,
            left_buffer: Vec::new(),
            right_buffer: Vec::new(),
            _state_store: state_store,
            _namespace: namespace,
        }
    }

    /// Process a record from one side of the join.
    pub async fn process_sided(
        &mut self,
        record: StreamRecord,
        side: JoinSide,
    ) -> Result<Vec<StreamRecord>> {
        let json: serde_json::Value =
            serde_json::from_slice(&record.value).map_err(|e| {
                SqlError::ExecutionError(format!("failed to parse join record as JSON: {e}"))
            })?;

        let key_field = match side {
            JoinSide::Left => &self.config.left_key,
            JoinSide::Right => &self.config.right_key,
        };

        let join_key = json
            .get(key_field)
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let entry = JoinBufferEntry {
            record: record.clone(),
            join_key: join_key.clone(),
            json: json.clone(),
        };

        let mut results = Vec::new();

        match side {
            JoinSide::Left => {
                // Probe right buffer
                for right_entry in &self.right_buffer {
                    if right_entry.join_key == join_key
                        && (record.timestamp - right_entry.record.timestamp).abs()
                            <= self.config.window_ms
                    {
                        if let Some(output) =
                            Self::build_output(&json, &right_entry.json, &self.config.join_type)
                        {
                            results.push(output);
                        }
                    }
                }
                self.left_buffer.push(entry);
            }
            JoinSide::Right => {
                // Probe left buffer
                for left_entry in &self.left_buffer {
                    if left_entry.join_key == join_key
                        && (record.timestamp - left_entry.record.timestamp).abs()
                            <= self.config.window_ms
                    {
                        if let Some(output) =
                            Self::build_output(&left_entry.json, &json, &self.config.join_type)
                        {
                            results.push(output);
                        }
                    }
                }
                self.right_buffer.push(entry);
            }
        }

        self.evict_expired(record.timestamp);

        Ok(results)
    }

    /// Build a joined output record from left and right JSON values.
    fn build_output(
        left: &serde_json::Value,
        right: &serde_json::Value,
        join_type: &JoinType,
    ) -> Option<StreamRecord> {
        match join_type {
            JoinType::Inner => {
                // Both sides must have non-null join keys (they matched to get here)
                let merged = serde_json::json!({
                    "left": left,
                    "right": right,
                });
                let value = serde_json::to_vec(&merged).ok()?;
                Some(StreamRecord::new(value, chrono::Utc::now().timestamp_millis()))
            }
            JoinType::Left | JoinType::Right | JoinType::Full => {
                let merged = serde_json::json!({
                    "left": left,
                    "right": right,
                });
                let value = serde_json::to_vec(&merged).ok()?;
                Some(StreamRecord::new(value, chrono::Utc::now().timestamp_millis()))
            }
        }
    }

    /// Remove entries from both buffers that have fallen outside the time window.
    #[allow(dead_code)]
    fn evict_expired(&mut self, current_timestamp: i64) {
        let cutoff = current_timestamp - self.config.window_ms;
        self.left_buffer.retain(|e| e.record.timestamp >= cutoff);
        self.right_buffer.retain(|e| e.record.timestamp >= cutoff);
    }
}

#[async_trait]
impl Operator for JoinOperator {
    async fn process(&mut self, record: StreamRecord) -> Result<Vec<StreamRecord>> {
        // Default: treat incoming records as left-side.
        // Use `process_sided` for explicit side control.
        self.process_sided(record, JoinSide::Left).await
    }

    async fn flush(&mut self) -> Result<Vec<StreamRecord>> {
        // For outer joins, emit unmatched records
        let mut output = Vec::new();
        match self.config.join_type {
            JoinType::Left | JoinType::Full => {
                // Emit left records that had no match
                for entry in &self.left_buffer {
                    let merged = serde_json::json!({
                        "left": entry.json,
                        "right": serde_json::Value::Null,
                    });
                    if let Ok(value) = serde_json::to_vec(&merged) {
                        output.push(StreamRecord::new(value, entry.record.timestamp));
                    }
                }
            }
            _ => {}
        }
        match self.config.join_type {
            JoinType::Right | JoinType::Full => {
                for entry in &self.right_buffer {
                    let merged = serde_json::json!({
                        "left": serde_json::Value::Null,
                        "right": entry.json,
                    });
                    if let Ok(value) = serde_json::to_vec(&merged) {
                        output.push(StreamRecord::new(value, entry.record.timestamp));
                    }
                }
            }
            _ => {}
        }
        self.left_buffer.clear();
        self.right_buffer.clear();
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// OperatorChain
// ---------------------------------------------------------------------------

/// Chains multiple operators together into a processing pipeline.
pub struct OperatorChain {
    operators: Vec<Box<dyn Operator>>,
}

impl OperatorChain {
    pub fn new() -> Self {
        Self {
            operators: Vec::new(),
        }
    }

    /// Add an operator to the end of the chain.
    pub fn add(mut self, operator: Box<dyn Operator>) -> Self {
        self.operators.push(operator);
        self
    }

    /// Process a record through all operators in sequence.
    pub async fn process(&mut self, record: StreamRecord) -> Result<Vec<StreamRecord>> {
        let mut current = vec![record];
        for op in &mut self.operators {
            let mut next = Vec::new();
            for rec in current {
                let results = op.process(rec).await?;
                next.extend(results);
            }
            current = next;
        }
        Ok(current)
    }

    /// Flush all operators in reverse order.
    pub async fn flush(&mut self) -> Result<Vec<StreamRecord>> {
        let mut output = Vec::new();
        for op in self.operators.iter_mut().rev() {
            let flushed = op.flush().await?;
            output.extend(flushed);
        }
        Ok(output)
    }
}

impl Default for OperatorChain {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::InMemoryStateStore;
    use crate::watermark::WatermarkConfig;
    use std::time::Duration;

    fn make_json_record(json: serde_json::Value, timestamp: i64) -> StreamRecord {
        StreamRecord {
            key: None,
            value: serde_json::to_vec(&json).unwrap(),
            timestamp,
            partition: 0,
            offset: 0,
            headers: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn make_record_with_offset(
        json: serde_json::Value,
        timestamp: i64,
        offset: u64,
    ) -> StreamRecord {
        StreamRecord {
            key: None,
            value: serde_json::to_vec(&json).unwrap(),
            timestamp,
            partition: 0,
            offset,
            headers: HashMap::new(),
        }
    }

    // -- FilterOperator --

    #[tokio::test]
    async fn test_filter_equals_match() {
        let pred = FilterPredicate::Equals {
            field: "status".to_string(),
            value: serde_json::json!("active"),
        };
        let mut op = FilterOperator::new(pred);
        let record = make_json_record(serde_json::json!({"status": "active", "id": 1}), 1000);
        let results = op.process(record).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_filter_equals_no_match() {
        let pred = FilterPredicate::Equals {
            field: "status".to_string(),
            value: serde_json::json!("active"),
        };
        let mut op = FilterOperator::new(pred);
        let record = make_json_record(serde_json::json!({"status": "inactive"}), 1000);
        let results = op.process(record).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_filter_greater_than() {
        let pred = FilterPredicate::GreaterThan {
            field: "amount".to_string(),
            value: serde_json::json!(100),
        };
        let mut op = FilterOperator::new(pred);

        let record = make_json_record(serde_json::json!({"amount": 150}), 1000);
        let results = op.process(record).await.unwrap();
        assert_eq!(results.len(), 1);

        let record = make_json_record(serde_json::json!({"amount": 50}), 1000);
        let results = op.process(record).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_filter_less_than() {
        let pred = FilterPredicate::LessThan {
            field: "price".to_string(),
            value: serde_json::json!(50.0),
        };
        let mut op = FilterOperator::new(pred);

        let record = make_json_record(serde_json::json!({"price": 30.0}), 1000);
        let results = op.process(record).await.unwrap();
        assert_eq!(results.len(), 1);

        let record = make_json_record(serde_json::json!({"price": 70.0}), 1000);
        let results = op.process(record).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_filter_contains() {
        let pred = FilterPredicate::Contains {
            field: "name".to_string(),
            substring: "stream".to_string(),
        };
        let mut op = FilterOperator::new(pred);

        let record = make_json_record(serde_json::json!({"name": "streamhouse"}), 1000);
        let results = op.process(record).await.unwrap();
        assert_eq!(results.len(), 1);

        let record = make_json_record(serde_json::json!({"name": "kafka"}), 1000);
        let results = op.process(record).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_filter_and() {
        let pred = FilterPredicate::And(
            Box::new(FilterPredicate::Equals {
                field: "type".to_string(),
                value: serde_json::json!("order"),
            }),
            Box::new(FilterPredicate::GreaterThan {
                field: "amount".to_string(),
                value: serde_json::json!(100),
            }),
        );
        let mut op = FilterOperator::new(pred);

        let record =
            make_json_record(serde_json::json!({"type": "order", "amount": 150}), 1000);
        let results = op.process(record).await.unwrap();
        assert_eq!(results.len(), 1);

        let record = make_json_record(serde_json::json!({"type": "order", "amount": 50}), 1000);
        let results = op.process(record).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_filter_or() {
        let pred = FilterPredicate::Or(
            Box::new(FilterPredicate::Equals {
                field: "status".to_string(),
                value: serde_json::json!("active"),
            }),
            Box::new(FilterPredicate::Equals {
                field: "status".to_string(),
                value: serde_json::json!("pending"),
            }),
        );
        let mut op = FilterOperator::new(pred);

        let record = make_json_record(serde_json::json!({"status": "active"}), 1000);
        assert_eq!(op.process(record).await.unwrap().len(), 1);

        let record = make_json_record(serde_json::json!({"status": "pending"}), 1000);
        assert_eq!(op.process(record).await.unwrap().len(), 1);

        let record = make_json_record(serde_json::json!({"status": "closed"}), 1000);
        assert!(op.process(record).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_filter_not() {
        let pred = FilterPredicate::Not(Box::new(FilterPredicate::Equals {
            field: "deleted".to_string(),
            value: serde_json::json!(true),
        }));
        let mut op = FilterOperator::new(pred);

        let record = make_json_record(serde_json::json!({"deleted": false, "id": 1}), 1000);
        assert_eq!(op.process(record).await.unwrap().len(), 1);

        let record = make_json_record(serde_json::json!({"deleted": true, "id": 2}), 1000);
        assert!(op.process(record).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_filter_flush_is_empty() {
        let pred = FilterPredicate::Equals {
            field: "x".to_string(),
            value: serde_json::json!(1),
        };
        let mut op = FilterOperator::new(pred);
        let flushed = op.flush().await.unwrap();
        assert!(flushed.is_empty());
    }

    // -- ProjectOperator --

    #[tokio::test]
    async fn test_project_select_fields() {
        let projections = vec![
            FieldProjection {
                source: "name".to_string(),
                target: None,
            },
            FieldProjection {
                source: "age".to_string(),
                target: None,
            },
        ];
        let mut op = ProjectOperator::new(projections);
        let record = make_json_record(
            serde_json::json!({"name": "Alice", "age": 30, "email": "alice@example.com"}),
            1000,
        );
        let results = op.process(record).await.unwrap();
        assert_eq!(results.len(), 1);
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert_eq!(json.get("name").unwrap(), "Alice");
        assert_eq!(json.get("age").unwrap(), 30);
        assert!(json.get("email").is_none());
    }

    #[tokio::test]
    async fn test_project_rename_field() {
        let projections = vec![FieldProjection {
            source: "name".to_string(),
            target: Some("user_name".to_string()),
        }];
        let mut op = ProjectOperator::new(projections);
        let record = make_json_record(serde_json::json!({"name": "Bob"}), 1000);
        let results = op.process(record).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert_eq!(json.get("user_name").unwrap(), "Bob");
        assert!(json.get("name").is_none());
    }

    #[tokio::test]
    async fn test_project_missing_field_ignored() {
        let projections = vec![FieldProjection {
            source: "nonexistent".to_string(),
            target: None,
        }];
        let mut op = ProjectOperator::new(projections);
        let record = make_json_record(serde_json::json!({"name": "Alice"}), 1000);
        let results = op.process(record).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert!(json.as_object().unwrap().is_empty());
    }

    // -- WindowAggregateOperator --

    fn make_window_operator(
        window_type: WindowType,
        agg: AggregateFunction,
    ) -> WindowAggregateOperator {
        let store: Arc<Box<dyn StateStore>> =
            Arc::new(Box::new(InMemoryStateStore::new()));
        let tracker = Arc::new(WatermarkTracker::new(WatermarkConfig {
            max_out_of_orderness: Duration::from_millis(0),
            idle_timeout: Duration::from_secs(60),
            watermark_interval: Duration::from_secs(1),
        }));
        WindowAggregateOperator::new(window_type, agg, store, tracker)
    }

    #[tokio::test]
    async fn test_window_tumble_count() {
        let mut op = make_window_operator(
            WindowType::Tumble { size_ms: 1000 },
            AggregateFunction::Count,
        );

        // Send records into window [0, 1000)
        for i in 0..5 {
            let record = make_json_record(serde_json::json!({"v": i}), i * 100);
            let _ = op.process(record).await.unwrap();
        }

        // Flush to get results
        let results = op.flush().await.unwrap();
        assert_eq!(results.len(), 1);
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert_eq!(json["count"], 5);
    }

    #[tokio::test]
    async fn test_window_tumble_sum() {
        let mut op = make_window_operator(
            WindowType::Tumble { size_ms: 1000 },
            AggregateFunction::Sum {
                field: "amount".to_string(),
            },
        );

        for i in 1..=3 {
            let record =
                make_json_record(serde_json::json!({"amount": i as f64 * 10.0}), i * 100);
            let _ = op.process(record).await.unwrap();
        }

        let results = op.flush().await.unwrap();
        assert_eq!(results.len(), 1);
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert_eq!(json["result"], 60.0); // 10 + 20 + 30
    }

    #[tokio::test]
    async fn test_window_hop() {
        let mut op = make_window_operator(
            WindowType::Hop {
                size_ms: 1000,
                slide_ms: 500,
            },
            AggregateFunction::Count,
        );

        // Record at 600ms should be in windows [0,1000) and [500,1500)
        let record = make_json_record(serde_json::json!({"v": 1}), 600);
        let _ = op.process(record).await.unwrap();

        let results = op.flush().await.unwrap();
        assert!(results.len() >= 2);
    }

    #[tokio::test]
    async fn test_window_session() {
        let mut op = make_window_operator(
            WindowType::Session { gap_ms: 500 },
            AggregateFunction::Count,
        );

        // Records within gap
        for i in 0..3 {
            let record = make_json_record(serde_json::json!({"v": i}), i * 100);
            let _ = op.process(record).await.unwrap();
        }

        let results = op.flush().await.unwrap();
        // All records should be in one session window
        assert!(!results.is_empty());
    }

    // -- JoinOperator --

    fn make_join_operator(join_type: JoinType) -> JoinOperator {
        let store: Arc<Box<dyn StateStore>> =
            Arc::new(Box::new(InMemoryStateStore::new()));
        let config = JoinConfig {
            left_key: "user_id".to_string(),
            right_key: "user_id".to_string(),
            window_ms: 5000,
            join_type,
        };
        JoinOperator::new(config, store, "join-test".to_string())
    }

    #[tokio::test]
    async fn test_join_inner_match() {
        let mut op = make_join_operator(JoinType::Inner);

        let left = make_json_record(serde_json::json!({"user_id": 1, "order": "A"}), 1000);
        let right = make_json_record(serde_json::json!({"user_id": 1, "name": "Alice"}), 1000);

        let results = op.process_sided(left, JoinSide::Left).await.unwrap();
        assert!(results.is_empty()); // no match yet

        let results = op.process_sided(right, JoinSide::Right).await.unwrap();
        assert_eq!(results.len(), 1); // matched!

        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert!(json.get("left").is_some());
        assert!(json.get("right").is_some());
    }

    #[tokio::test]
    async fn test_join_inner_no_match() {
        let mut op = make_join_operator(JoinType::Inner);

        let left = make_json_record(serde_json::json!({"user_id": 1}), 1000);
        let right = make_json_record(serde_json::json!({"user_id": 2}), 1000);

        let results = op.process_sided(left, JoinSide::Left).await.unwrap();
        assert!(results.is_empty());

        let results = op.process_sided(right, JoinSide::Right).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_join_window_expiry() {
        let mut op = make_join_operator(JoinType::Inner);

        // Left record at time 1000
        let left = make_json_record(serde_json::json!({"user_id": 1}), 1000);
        op.process_sided(left, JoinSide::Left).await.unwrap();

        // Right record at time 7000 (outside 5000ms window)
        let right = make_json_record(serde_json::json!({"user_id": 1}), 7000);
        let results = op.process_sided(right, JoinSide::Right).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_join_left_flush_emits_unmatched() {
        let mut op = make_join_operator(JoinType::Left);

        let left = make_json_record(serde_json::json!({"user_id": 99}), 1000);
        op.process_sided(left, JoinSide::Left).await.unwrap();

        let flushed = op.flush().await.unwrap();
        assert!(!flushed.is_empty());
        let json: serde_json::Value = serde_json::from_slice(&flushed[0].value).unwrap();
        assert!(json.get("right").unwrap().is_null());
    }

    // -- OperatorChain --

    #[tokio::test]
    async fn test_operator_chain_filter_then_project() {
        let filter = FilterOperator::new(FilterPredicate::GreaterThan {
            field: "amount".to_string(),
            value: serde_json::json!(100),
        });
        let project = ProjectOperator::new(vec![FieldProjection {
            source: "amount".to_string(),
            target: Some("total".to_string()),
        }]);

        let mut chain = OperatorChain::new()
            .add(Box::new(filter))
            .add(Box::new(project));

        let record = make_json_record(serde_json::json!({"amount": 150, "name": "test"}), 1000);
        let results = chain.process(record).await.unwrap();
        assert_eq!(results.len(), 1);
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert_eq!(json["total"], 150);
        assert!(json.get("name").is_none());
    }

    #[tokio::test]
    async fn test_operator_chain_filter_rejects() {
        let filter = FilterOperator::new(FilterPredicate::Equals {
            field: "status".to_string(),
            value: serde_json::json!("active"),
        });
        let project = ProjectOperator::new(vec![FieldProjection {
            source: "status".to_string(),
            target: None,
        }]);

        let mut chain = OperatorChain::new()
            .add(Box::new(filter))
            .add(Box::new(project));

        let record = make_json_record(serde_json::json!({"status": "closed"}), 1000);
        let results = chain.process(record).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_operator_chain_empty() {
        let mut chain = OperatorChain::new();
        let record = make_json_record(serde_json::json!({"x": 1}), 1000);
        let results = chain.process(record).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_stream_record_new() {
        let record = StreamRecord::new(b"hello".to_vec(), 42);
        assert_eq!(record.timestamp, 42);
        assert_eq!(record.partition, 0);
        assert_eq!(record.offset, 0);
        assert!(record.key.is_none());
        assert!(record.headers.is_empty());
    }

    #[tokio::test]
    async fn test_filter_predicate_evaluate_missing_field() {
        let pred = FilterPredicate::Equals {
            field: "missing".to_string(),
            value: serde_json::json!(42),
        };
        assert!(!pred.evaluate(&serde_json::json!({"other": 1})));
    }

    #[tokio::test]
    async fn test_window_avg() {
        let mut op = make_window_operator(
            WindowType::Tumble { size_ms: 1000 },
            AggregateFunction::Avg {
                field: "score".to_string(),
            },
        );

        for score in [10.0, 20.0, 30.0] {
            let record = make_json_record(serde_json::json!({"score": score}), 100);
            let _ = op.process(record).await.unwrap();
        }

        let results = op.flush().await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        let avg = json["result"].as_f64().unwrap();
        assert!((avg - 20.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_window_min_max() {
        let mut op = make_window_operator(
            WindowType::Tumble { size_ms: 1000 },
            AggregateFunction::Min {
                field: "val".to_string(),
            },
        );

        for v in [5.0, 2.0, 8.0] {
            let record = make_json_record(serde_json::json!({"val": v}), 100);
            let _ = op.process(record).await.unwrap();
        }

        let results = op.flush().await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&results[0].value).unwrap();
        assert_eq!(json["result"], 2.0);
    }
}
