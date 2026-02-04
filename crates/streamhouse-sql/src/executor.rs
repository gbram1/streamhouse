//! SQL query executor

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use streamhouse_metadata::MetadataStore;
use streamhouse_storage::SegmentCache;

use crate::error::SqlError;
use crate::types::*;
use crate::Result;

/// Maximum number of rows per query
const MAX_ROWS: usize = 10_000;

/// Default query timeout in milliseconds
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// SQL query executor
pub struct SqlExecutor {
    metadata: Arc<dyn MetadataStore>,
    segment_cache: Arc<SegmentCache>,
    object_store: Arc<dyn object_store::ObjectStore>,
}

impl SqlExecutor {
    /// Create a new SQL executor
    pub fn new(
        metadata: Arc<dyn MetadataStore>,
        segment_cache: Arc<SegmentCache>,
        object_store: Arc<dyn object_store::ObjectStore>,
    ) -> Self {
        Self {
            metadata,
            segment_cache,
            object_store,
        }
    }

    /// Execute a SQL query
    pub async fn execute(&self, sql: &str, timeout_ms: Option<u64>) -> Result<QueryResult> {
        let start = Instant::now();
        let timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        let query = crate::parse_query(sql)?;

        let result = match query {
            SqlQuery::Select(q) => self.execute_select(q, start, timeout).await?,
            SqlQuery::ShowTopics => self.execute_show_topics().await?,
            SqlQuery::DescribeTopic(topic) => self.execute_describe(&topic).await?,
            SqlQuery::Count(q) => self.execute_count(q, start, timeout).await?,
            SqlQuery::WindowAggregate(q) => self.execute_window_aggregate(q, start, timeout).await?,
        };

        Ok(result)
    }

    async fn execute_select(
        &self,
        query: SelectQuery,
        start: Instant,
        timeout_ms: u64,
    ) -> Result<QueryResult> {
        // Validate topic exists
        let topic = self
            .metadata
            .get_topic(&query.topic)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(query.topic.clone()))?;

        // Determine which partitions to scan
        let partition_filter = query
            .filters
            .iter()
            .find_map(|f| {
                if let Filter::PartitionEquals(p) = f {
                    Some(*p)
                } else {
                    None
                }
            });

        let partitions: Vec<u32> = if let Some(p) = partition_filter {
            vec![p]
        } else {
            (0..topic.partition_count).collect()
        };

        // Determine offset range filters
        let offset_start = query.filters.iter().find_map(|f| {
            if let Filter::OffsetGte(o) = f {
                Some(*o)
            } else {
                None
            }
        });

        let offset_end = query.filters.iter().find_map(|f| {
            if let Filter::OffsetLt(o) = f {
                Some(*o)
            } else {
                None
            }
        });

        // Build column metadata
        let columns = build_column_info(&query.columns);

        // Check if we need statistics for anomaly detection
        let needs_stats = requires_statistics(&query.columns, &query.filters);
        let stat_paths = collect_stat_paths(&query.columns, &query.filters);

        // Collect all messages first if we need statistics
        let mut all_messages: Vec<MessageRow> = Vec::new();

        for partition_id in &partitions {
            // Check timeout
            if start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(SqlError::Timeout(timeout_ms));
            }

            // Get partition info
            let partition = match self.metadata.get_partition(&query.topic, *partition_id).await? {
                Some(p) => p,
                None => continue,
            };

            // Determine scan range
            let scan_start = offset_start.unwrap_or(0);
            let scan_end = offset_end.unwrap_or(partition.high_watermark);

            // Get segments covering this range
            let segments = self
                .metadata
                .get_segments(&query.topic, *partition_id)
                .await?;

            for segment in segments {
                // Check if segment overlaps with our range
                if segment.end_offset < scan_start || segment.base_offset > scan_end {
                    continue;
                }

                // Read messages from segment
                let messages = self.read_segment_messages(&segment).await?;

                // Apply basic filters (non-statistical)
                for msg in messages {
                    let basic_filters: Vec<_> = query.filters.iter()
                        .filter(|f| !is_statistical_filter(f))
                        .cloned()
                        .collect();

                    if msg.matches_filters(&basic_filters) {
                        all_messages.push(msg);
                    }
                }
            }
        }

        // Build statistics context if needed
        let ctx = if needs_stats {
            let mut ctx = RowContext::new();
            for path in &stat_paths {
                let mut stats = FieldStatistics::new();
                for msg in &all_messages {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                        let extracted = extract_json_path_for_stats(&json, path);
                        if let Some(v) = extracted.as_f64() {
                            stats.add(v);
                        }
                    }
                }
                ctx.stats.insert(path.clone(), stats);
            }
            Some(ctx)
        } else {
            None
        };

        // Apply statistical filters and build rows
        let limit = query.limit.unwrap_or(MAX_ROWS).min(MAX_ROWS);
        let skip = query.offset.unwrap_or(0);
        let mut rows: Vec<Row> = Vec::new();

        let stat_filters: Vec<_> = query.filters.iter()
            .filter(|f| is_statistical_filter(f))
            .cloned()
            .collect();

        for (idx, msg) in all_messages.iter().enumerate() {
            // Update row index for moving averages
            let ctx_with_idx = ctx.as_ref().map(|c| {
                let mut new_ctx = c.clone();
                new_ctx.row_index = idx;
                new_ctx
            });

            // Apply statistical filters if any
            if !stat_filters.is_empty() {
                if !msg.matches_filters_with_context(&stat_filters, ctx_with_idx.as_ref()) {
                    continue;
                }
            }

            // Skip offset
            if rows.len() < skip {
                rows.push(vec![]); // placeholder for skip counting
                continue;
            }

            // Add row with context
            let row = msg.to_row_with_context(&query.columns, ctx_with_idx.as_ref());
            rows.push(row);

            // Check limit
            if rows.len() >= skip + limit {
                break;
            }
        }

        // Remove skip placeholders
        let rows: Vec<Row> = rows.into_iter().skip(skip).collect();
        let truncated = rows.len() >= limit;

        Ok(QueryResult {
            columns,
            row_count: rows.len(),
            rows,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated,
        })
    }

    async fn execute_count(
        &self,
        query: CountQuery,
        start: Instant,
        timeout_ms: u64,
    ) -> Result<QueryResult> {
        // Validate topic exists
        let topic = self
            .metadata
            .get_topic(&query.topic)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(query.topic.clone()))?;

        // Determine which partitions to scan
        let partition_filter = query.filters.iter().find_map(|f| {
            if let Filter::PartitionEquals(p) = f {
                Some(*p)
            } else {
                None
            }
        });

        let partitions: Vec<u32> = if let Some(p) = partition_filter {
            vec![p]
        } else {
            (0..topic.partition_count).collect()
        };

        let mut count: u64 = 0;

        // If no filters other than partition, we can use high watermarks
        let only_partition_filter = query.filters.iter().all(|f| {
            matches!(f, Filter::PartitionEquals(_))
        });

        if only_partition_filter {
            // Fast path: just sum high watermarks
            for partition_id in partitions {
                if let Some(partition) = self.metadata.get_partition(&query.topic, partition_id).await? {
                    count += partition.high_watermark;
                }
            }
        } else {
            // Slow path: scan messages and count matches
            for partition_id in partitions {
                if start.elapsed().as_millis() as u64 > timeout_ms {
                    return Err(SqlError::Timeout(timeout_ms));
                }

                let segments = self
                    .metadata
                    .get_segments(&query.topic, partition_id)
                    .await?;

                for segment in segments {
                    let messages = self.read_segment_messages(&segment).await?;
                    for msg in messages {
                        if msg.matches_filters(&query.filters) {
                            count += 1;
                        }
                    }
                }
            }
        }

        let columns = vec![ColumnInfo {
            name: "count".to_string(),
            data_type: "bigint".to_string(),
        }];

        Ok(QueryResult {
            columns,
            rows: vec![vec![serde_json::Value::Number(count.into())]],
            row_count: 1,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    async fn execute_show_topics(&self) -> Result<QueryResult> {
        let start = Instant::now();
        let topics = self.metadata.list_topics().await?;

        let columns = vec![
            ColumnInfo {
                name: "name".to_string(),
                data_type: "string".to_string(),
            },
            ColumnInfo {
                name: "partitions".to_string(),
                data_type: "integer".to_string(),
            },
        ];

        let rows: Vec<Row> = topics
            .iter()
            .map(|t| {
                vec![
                    serde_json::Value::String(t.name.clone()),
                    serde_json::Value::Number(t.partition_count.into()),
                ]
            })
            .collect();

        Ok(QueryResult {
            columns,
            row_count: rows.len(),
            rows,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    async fn execute_describe(&self, topic_name: &str) -> Result<QueryResult> {
        let start = Instant::now();

        let topic = self
            .metadata
            .get_topic(topic_name)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(topic_name.to_string()))?;

        let mut total_messages: u64 = 0;
        let mut partition_rows: Vec<Row> = Vec::new();

        for partition_id in 0..topic.partition_count {
            let partition = self
                .metadata
                .get_partition(topic_name, partition_id)
                .await?;

            let segments = self
                .metadata
                .get_segments(topic_name, partition_id)
                .await?;

            let hwm = partition.map(|p| p.high_watermark).unwrap_or(0);
            total_messages += hwm;

            partition_rows.push(vec![
                serde_json::Value::Number(partition_id.into()),
                serde_json::Value::Number(hwm.into()),
                serde_json::Value::Number(segments.len().into()),
            ]);
        }

        // Build a composite result showing topic info + partition details
        let columns = vec![
            ColumnInfo {
                name: "partition_id".to_string(),
                data_type: "integer".to_string(),
            },
            ColumnInfo {
                name: "high_watermark".to_string(),
                data_type: "bigint".to_string(),
            },
            ColumnInfo {
                name: "segment_count".to_string(),
                data_type: "integer".to_string(),
            },
        ];

        // Add summary row at the end
        partition_rows.push(vec![
            serde_json::Value::String(format!("TOTAL ({} partitions)", topic.partition_count)),
            serde_json::Value::Number(total_messages.into()),
            serde_json::Value::String("-".to_string()),
        ]);

        Ok(QueryResult {
            columns,
            row_count: partition_rows.len(),
            rows: partition_rows,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    /// Execute a window aggregation query
    async fn execute_window_aggregate(
        &self,
        query: WindowAggregateQuery,
        start: Instant,
        timeout_ms: u64,
    ) -> Result<QueryResult> {
        // Validate topic exists
        let topic = self
            .metadata
            .get_topic(&query.topic)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(query.topic.clone()))?;

        // Get all partitions
        let partitions: Vec<u32> = {
            let partition_filter = query.filters.iter().find_map(|f| {
                if let Filter::PartitionEquals(p) = f {
                    Some(*p)
                } else {
                    None
                }
            });
            if let Some(p) = partition_filter {
                vec![p]
            } else {
                (0..topic.partition_count).collect()
            }
        };

        // Collect all messages
        let mut all_messages: Vec<MessageRow> = Vec::new();

        for partition_id in partitions {
            if start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(SqlError::Timeout(timeout_ms));
            }

            let segments = self
                .metadata
                .get_segments(&query.topic, partition_id)
                .await?;

            for segment in segments {
                let messages = self.read_segment_messages(&segment).await?;
                for msg in messages {
                    if msg.matches_filters(&query.filters) {
                        all_messages.push(msg);
                    }
                }
            }
        }

        // Sort by timestamp for window processing
        all_messages.sort_by_key(|m| m.timestamp);

        // Group messages into windows
        let windows = match &query.window {
            WindowType::Tumble { size_ms } => {
                group_into_tumble_windows(&all_messages, *size_ms, &query.group_by)
            }
            WindowType::Hop { size_ms, slide_ms } => {
                group_into_hop_windows(&all_messages, *size_ms, *slide_ms, &query.group_by)
            }
            WindowType::Session { gap_ms } => {
                group_into_session_windows(&all_messages, *gap_ms, &query.group_by)
            }
        };

        // Compute aggregations for each window
        let mut rows: Vec<Row> = Vec::new();

        for (window_key, window_messages) in windows {
            let (window_start, window_end, group_key) = window_key;

            let mut row: Row = vec![
                serde_json::Value::Number(window_start.into()),
                serde_json::Value::Number(window_end.into()),
            ];

            // Add group key if present
            if let Some(key) = &group_key {
                row.push(serde_json::Value::String(key.clone()));
            }

            // Compute each aggregation
            for agg in &query.aggregations {
                let value = compute_aggregation(agg, &window_messages);
                row.push(value);
            }

            rows.push(row);
        }

        // Apply limit
        let limit = query.limit.unwrap_or(MAX_ROWS).min(MAX_ROWS);
        let truncated = rows.len() > limit;
        rows.truncate(limit);

        // Build column info
        let columns = build_window_column_info(&query.aggregations, !query.group_by.is_empty());

        Ok(QueryResult {
            columns,
            row_count: rows.len(),
            rows,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated,
        })
    }

    /// Read messages from a segment
    async fn read_segment_messages(
        &self,
        segment: &streamhouse_metadata::SegmentInfo,
    ) -> Result<Vec<MessageRow>> {
        // Build the object store path
        let path = object_store::path::Path::from(segment.s3_key.clone());

        // Try to read from cache first
        let cache_key = &segment.id;
        let data = if let Ok(Some(cached)) = self.segment_cache.get(cache_key).await {
            cached
        } else {
            // Fetch from object store
            let result = self
                .object_store
                .get(&path)
                .await
                .map_err(|e| SqlError::StorageError(e.to_string()))?;

            let bytes = result
                .bytes()
                .await
                .map_err(|e| SqlError::StorageError(e.to_string()))?;

            // Cache it for future use
            let _ = self.segment_cache.put(cache_key, bytes.clone()).await;

            bytes
        };

        // Parse segment data into messages
        // For now, use a simple line-based format (one JSON message per line)
        // In production, this would use the actual segment format
        let mut messages = Vec::new();

        // Try to parse as NDJSON (newline-delimited JSON)
        let text = String::from_utf8_lossy(&data);
        for (idx, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            // Try to parse as a message record
            if let Ok(record) = serde_json::from_str::<serde_json::Value>(line) {
                let msg = MessageRow {
                    topic: segment.topic.clone(),
                    partition: segment.partition_id,
                    offset: segment.base_offset + idx as u64,
                    key: record
                        .get("key")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    value: record
                        .get("value")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| line.to_string()),
                    timestamp: record
                        .get("timestamp")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(segment.created_at),
                };
                messages.push(msg);
            } else {
                // If not JSON, treat the whole line as value
                messages.push(MessageRow {
                    topic: segment.topic.clone(),
                    partition: segment.partition_id,
                    offset: segment.base_offset + idx as u64,
                    key: None,
                    value: line.to_string(),
                    timestamp: segment.created_at,
                });
            }
        }

        Ok(messages)
    }
}

fn build_column_info(columns: &[SelectColumn]) -> Vec<ColumnInfo> {
    let mut result = Vec::new();

    for col in columns {
        match col {
            SelectColumn::All => {
                result.extend(default_columns());
            }
            SelectColumn::Column(name) => {
                result.push(ColumnInfo {
                    name: name.clone(),
                    data_type: match name.as_str() {
                        "partition" => "integer",
                        "offset" | "timestamp" => "bigint",
                        _ => "string",
                    }
                    .to_string(),
                });
            }
            SelectColumn::JsonExtract { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| path.clone()),
                    data_type: "json".to_string(),
                });
            }
            SelectColumn::ZScore { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("zscore({})", path)),
                    data_type: "float".to_string(),
                });
            }
            SelectColumn::MovingAvg { alias, path, window_size, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("moving_avg({}, {})", path, window_size)),
                    data_type: "float".to_string(),
                });
            }
            SelectColumn::Stddev { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("stddev({})", path)),
                    data_type: "float".to_string(),
                });
            }
            SelectColumn::Avg { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("avg({})", path)),
                    data_type: "float".to_string(),
                });
            }
            SelectColumn::Anomaly { alias, path, threshold, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("anomaly({}, {})", path, threshold)),
                    data_type: "boolean".to_string(),
                });
            }
            SelectColumn::CosineSimilarity { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("cosine_similarity({})", path)),
                    data_type: "float".to_string(),
                });
            }
            SelectColumn::EuclideanDistance { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("euclidean_distance({})", path)),
                    data_type: "float".to_string(),
                });
            }
            SelectColumn::DotProduct { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("dot_product({})", path)),
                    data_type: "float".to_string(),
                });
            }
            SelectColumn::VectorNorm { alias, path, .. } => {
                result.push(ColumnInfo {
                    name: alias.clone().unwrap_or_else(|| format!("vector_norm({})", path)),
                    data_type: "float".to_string(),
                });
            }
        }
    }

    result
}

/// Check if any column or filter requires statistics computation
fn requires_statistics(columns: &[SelectColumn], filters: &[Filter]) -> bool {
    let has_stat_columns = columns.iter().any(|c| matches!(c,
        SelectColumn::ZScore { .. } |
        SelectColumn::MovingAvg { .. } |
        SelectColumn::Stddev { .. } |
        SelectColumn::Avg { .. } |
        SelectColumn::Anomaly { .. }
    ));

    let has_stat_filters = filters.iter().any(is_statistical_filter);

    has_stat_columns || has_stat_filters
}

/// Check if a filter requires statistics
fn is_statistical_filter(filter: &Filter) -> bool {
    matches!(filter,
        Filter::ZScoreGt { .. } |
        Filter::ZScoreLt { .. } |
        Filter::AnomalyThreshold { .. }
    )
}

/// Collect all JSON paths that need statistics computation
fn collect_stat_paths(columns: &[SelectColumn], filters: &[Filter]) -> HashSet<String> {
    let mut paths = HashSet::new();

    for col in columns {
        match col {
            SelectColumn::ZScore { path, .. } |
            SelectColumn::MovingAvg { path, .. } |
            SelectColumn::Stddev { path, .. } |
            SelectColumn::Avg { path, .. } |
            SelectColumn::Anomaly { path, .. } => {
                paths.insert(path.clone());
            }
            _ => {}
        }
    }

    for filter in filters {
        match filter {
            Filter::ZScoreGt { path, .. } |
            Filter::ZScoreLt { path, .. } |
            Filter::AnomalyThreshold { path, .. } => {
                paths.insert(path.clone());
            }
            _ => {}
        }
    }

    paths
}

/// Extract a value from JSON using a simple path ($.field.subfield)
fn extract_json_path_for_stats(json: &serde_json::Value, path: &str) -> serde_json::Value {
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

// Window grouping types
type WindowKey = (i64, i64, Option<String>); // (window_start, window_end, group_key)
type WindowGroups = std::collections::BTreeMap<WindowKey, Vec<MessageRow>>;

/// Group messages into tumbling windows
fn group_into_tumble_windows(
    messages: &[MessageRow],
    size_ms: i64,
    group_by: &[String],
) -> WindowGroups {
    let mut windows: WindowGroups = std::collections::BTreeMap::new();

    for msg in messages {
        let window_start = (msg.timestamp / size_ms) * size_ms;
        let window_end = window_start + size_ms;
        let group_key = extract_group_key(msg, group_by);
        let key = (window_start, window_end, group_key);

        windows.entry(key).or_default().push(msg.clone());
    }

    windows
}

/// Group messages into hopping (sliding) windows
fn group_into_hop_windows(
    messages: &[MessageRow],
    size_ms: i64,
    slide_ms: i64,
    group_by: &[String],
) -> WindowGroups {
    let mut windows: WindowGroups = std::collections::BTreeMap::new();

    if messages.is_empty() {
        return windows;
    }

    // Find the time range
    let min_ts = messages.iter().map(|m| m.timestamp).min().unwrap_or(0);
    let max_ts = messages.iter().map(|m| m.timestamp).max().unwrap_or(0);

    // Generate all windows that could contain messages
    let mut window_start = (min_ts / slide_ms) * slide_ms;

    while window_start <= max_ts {
        let window_end = window_start + size_ms;

        for msg in messages {
            if msg.timestamp >= window_start && msg.timestamp < window_end {
                let group_key = extract_group_key(msg, group_by);
                let key = (window_start, window_end, group_key);
                windows.entry(key).or_default().push(msg.clone());
            }
        }

        window_start += slide_ms;
    }

    windows
}

/// Group messages into session windows
fn group_into_session_windows(
    messages: &[MessageRow],
    gap_ms: i64,
    group_by: &[String],
) -> WindowGroups {
    let mut windows: WindowGroups = std::collections::BTreeMap::new();

    if messages.is_empty() {
        return windows;
    }

    // Group messages by group key first
    let mut by_group: std::collections::HashMap<Option<String>, Vec<&MessageRow>> =
        std::collections::HashMap::new();

    for msg in messages {
        let group_key = extract_group_key(msg, group_by);
        by_group.entry(group_key).or_default().push(msg);
    }

    // Process each group separately
    for (group_key, mut group_msgs) in by_group {
        group_msgs.sort_by_key(|m| m.timestamp);

        let mut session_start = group_msgs[0].timestamp;
        let mut session_end = session_start;
        let mut session_messages: Vec<MessageRow> = Vec::new();

        for msg in group_msgs {
            if msg.timestamp - session_end > gap_ms {
                // Close current session and start new one
                if !session_messages.is_empty() {
                    let key = (session_start, session_end, group_key.clone());
                    windows.insert(key, session_messages);
                    session_messages = Vec::new();
                }
                session_start = msg.timestamp;
            }
            session_end = msg.timestamp;
            session_messages.push(msg.clone());
        }

        // Don't forget the last session
        if !session_messages.is_empty() {
            let key = (session_start, session_end, group_key);
            windows.insert(key, session_messages);
        }
    }

    windows
}

/// Extract group key from message
fn extract_group_key(msg: &MessageRow, group_by: &[String]) -> Option<String> {
    if group_by.is_empty() {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();

    for col in group_by {
        let value = match col.as_str() {
            "key" => msg.key.clone().unwrap_or_default(),
            "partition" => msg.partition.to_string(),
            path if path.starts_with("$.") => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                    let extracted = extract_json_path_for_stats(&json, path);
                    extracted.to_string()
                } else {
                    "null".to_string()
                }
            }
            _ => "".to_string(),
        };
        parts.push(value);
    }

    Some(parts.join("|"))
}

/// Compute aggregation for a window
fn compute_aggregation(agg: &WindowAggregation, messages: &[MessageRow]) -> serde_json::Value {
    match agg {
        WindowAggregation::Count { .. } => {
            serde_json::Value::Number((messages.len() as i64).into())
        }
        WindowAggregation::CountDistinct { column, .. } => {
            let mut distinct: HashSet<String> = HashSet::new();
            for msg in messages {
                let value = match column.as_str() {
                    "key" => msg.key.clone().unwrap_or_default(),
                    "*" => format!("{}:{}:{}", msg.topic, msg.partition, msg.offset),
                    _ => msg.value.clone(),
                };
                distinct.insert(value);
            }
            serde_json::Value::Number((distinct.len() as i64).into())
        }
        WindowAggregation::Sum { path, .. } => {
            let sum: f64 = messages
                .iter()
                .filter_map(|m| extract_numeric_from_msg(m, path))
                .sum();
            serde_json::json!(sum)
        }
        WindowAggregation::Avg { path, .. } => {
            let values: Vec<f64> = messages
                .iter()
                .filter_map(|m| extract_numeric_from_msg(m, path))
                .collect();
            if values.is_empty() {
                serde_json::Value::Null
            } else {
                let avg = values.iter().sum::<f64>() / values.len() as f64;
                serde_json::json!(avg)
            }
        }
        WindowAggregation::Min { path, .. } => {
            messages
                .iter()
                .filter_map(|m| extract_numeric_from_msg(m, path))
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|v| serde_json::json!(v))
                .unwrap_or(serde_json::Value::Null)
        }
        WindowAggregation::Max { path, .. } => {
            messages
                .iter()
                .filter_map(|m| extract_numeric_from_msg(m, path))
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|v| serde_json::json!(v))
                .unwrap_or(serde_json::Value::Null)
        }
        WindowAggregation::First { path, .. } => {
            messages
                .first()
                .map(|m| extract_value_from_msg(m, path))
                .unwrap_or(serde_json::Value::Null)
        }
        WindowAggregation::Last { path, .. } => {
            messages
                .last()
                .map(|m| extract_value_from_msg(m, path))
                .unwrap_or(serde_json::Value::Null)
        }
    }
}

/// Extract numeric value from message
fn extract_numeric_from_msg(msg: &MessageRow, path: &str) -> Option<f64> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
        let extracted = extract_json_path_for_stats(&json, path);
        extracted.as_f64()
    } else {
        None
    }
}

/// Extract any value from message
fn extract_value_from_msg(msg: &MessageRow, path: &str) -> serde_json::Value {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
        extract_json_path_for_stats(&json, path)
    } else {
        serde_json::Value::Null
    }
}

/// Build column info for window aggregation results
fn build_window_column_info(aggregations: &[WindowAggregation], has_group_by: bool) -> Vec<ColumnInfo> {
    let mut columns = vec![
        ColumnInfo {
            name: "window_start".to_string(),
            data_type: "bigint".to_string(),
        },
        ColumnInfo {
            name: "window_end".to_string(),
            data_type: "bigint".to_string(),
        },
    ];

    if has_group_by {
        columns.push(ColumnInfo {
            name: "group_key".to_string(),
            data_type: "string".to_string(),
        });
    }

    for agg in aggregations {
        let (name, data_type) = match agg {
            WindowAggregation::Count { alias } => {
                (alias.clone().unwrap_or_else(|| "count".to_string()), "bigint")
            }
            WindowAggregation::CountDistinct { alias, column } => {
                (alias.clone().unwrap_or_else(|| format!("count_distinct_{}", column)), "bigint")
            }
            WindowAggregation::Sum { alias, path } => {
                (alias.clone().unwrap_or_else(|| format!("sum_{}", path)), "float")
            }
            WindowAggregation::Avg { alias, path } => {
                (alias.clone().unwrap_or_else(|| format!("avg_{}", path)), "float")
            }
            WindowAggregation::Min { alias, path } => {
                (alias.clone().unwrap_or_else(|| format!("min_{}", path)), "float")
            }
            WindowAggregation::Max { alias, path } => {
                (alias.clone().unwrap_or_else(|| format!("max_{}", path)), "float")
            }
            WindowAggregation::First { alias, path } => {
                (alias.clone().unwrap_or_else(|| format!("first_{}", path)), "json")
            }
            WindowAggregation::Last { alias, path } => {
                (alias.clone().unwrap_or_else(|| format!("last_{}", path)), "json")
            }
        };

        columns.push(ColumnInfo {
            name,
            data_type: data_type.to_string(),
        });
    }

    columns
}
