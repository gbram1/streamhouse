//! SQL query executor

use std::collections::{HashMap, HashSet};
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
            SqlQuery::Join(q) => self.execute_join(q, start, timeout).await?,
            // Materialized View commands
            SqlQuery::CreateMaterializedView(q) => self.execute_create_materialized_view(q, start).await?,
            SqlQuery::DropMaterializedView(name) => self.execute_drop_materialized_view(&name, start).await?,
            SqlQuery::RefreshMaterializedView(name) => self.execute_refresh_materialized_view(&name, start).await?,
            SqlQuery::ShowMaterializedViews => self.execute_show_materialized_views(start).await?,
            SqlQuery::DescribeMaterializedView(name) => self.execute_describe_materialized_view(&name, start).await?,
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

    /// Execute a JOIN query
    async fn execute_join(
        &self,
        query: JoinQuery,
        start: Instant,
        timeout_ms: u64,
    ) -> Result<QueryResult> {
        // Validate both topics exist
        let _left_topic = self
            .metadata
            .get_topic(&query.left.topic)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(query.left.topic.clone()))?;

        let _right_topic = self
            .metadata
            .get_topic(&query.right.topic)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(query.right.topic.clone()))?;

        // Check timeout before loading
        if start.elapsed().as_millis() as u64 > timeout_ms {
            return Err(SqlError::Timeout(timeout_ms));
        }

        // Load messages from both topics
        let left_messages = self.load_topic_messages(&query.left.topic, start, timeout_ms).await?;

        // Check timeout after loading left
        if start.elapsed().as_millis() as u64 > timeout_ms {
            return Err(SqlError::Timeout(timeout_ms));
        }

        let right_messages = self.load_topic_messages(&query.right.topic, start, timeout_ms).await?;

        // Check timeout after loading right
        if start.elapsed().as_millis() as u64 > timeout_ms {
            return Err(SqlError::Timeout(timeout_ms));
        }

        // OPTIMIZATION: Predicate pushdown - filter messages before joining
        // Partition filters by which side they apply to based on qualifier
        let (left_filters, right_filters): (Vec<_>, Vec<_>) = query.filters.iter()
            .partition(|f| filter_applies_to_side(f, &query.left));

        let left_messages: Vec<_> = if left_filters.is_empty() {
            left_messages
        } else {
            left_messages.into_iter()
                .filter(|msg| apply_pushdown_filters(msg, &left_filters))
                .collect()
        };

        let right_messages: Vec<_> = if right_filters.is_empty() {
            right_messages
        } else {
            right_messages.into_iter()
                .filter(|msg| apply_pushdown_filters(msg, &right_filters))
                .collect()
        };

        // Check if this is a stream-table join (one side uses TABLE())
        let joined_rows = if query.right.is_table {
            // Stream-Table JOIN: Right side is a TABLE (compacted key→value)
            // Build key→latest_value map for O(1) lookups
            let right_table = build_table_state(&right_messages, &query.condition.right);
            perform_stream_table_join(
                &left_messages,
                &right_table,
                &query.condition,
                &query.join_type,
            )
        } else if query.left.is_table {
            // Table-Stream JOIN: Left side is a TABLE
            // Swap and perform reversed join
            let left_table = build_table_state(&left_messages, &query.condition.left);
            let swapped_rows = perform_stream_table_join(
                &right_messages,
                &left_table,
                &JoinCondition {
                    left: query.condition.right.clone(),
                    right: query.condition.left.clone(),
                },
                &swap_join_type(&query.join_type),
            );
            // Swap back the results
            swapped_rows.into_iter().map(|(r, l)| (l, r)).collect()
        } else {
            // Stream-Stream JOIN: Both sides are streams
            // Build hash index on right side (for hash join)
            let right_index = build_join_index(&right_messages, &query.condition.right);
            perform_join(
                &left_messages,
                &right_messages,
                &right_index,
                &query.condition,
                &query.join_type,
                &query.left,
                &query.right,
            )
        };

        // Check timeout after join
        if start.elapsed().as_millis() as u64 > timeout_ms {
            return Err(SqlError::Timeout(timeout_ms));
        }

        // Project to output columns
        let columns = build_join_column_info(&query.columns, &query.left, &query.right);
        let limit = query.limit.unwrap_or(MAX_ROWS).min(MAX_ROWS);

        let mut rows: Vec<Row> = Vec::new();
        for (left_msg, right_msg) in joined_rows {
            if rows.len() >= limit {
                break;
            }
            // Periodic timeout check during projection
            if rows.len() % 1000 == 0 && start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(SqlError::Timeout(timeout_ms));
            }
            let row = project_join_row(&query.columns, left_msg.as_ref(), right_msg.as_ref(), &query.left, &query.right);
            rows.push(row);
        }

        let truncated = rows.len() >= limit;

        Ok(QueryResult {
            columns,
            row_count: rows.len(),
            rows,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated,
        })
    }

    /// Load all messages from a topic
    async fn load_topic_messages(
        &self,
        topic_name: &str,
        start: Instant,
        timeout_ms: u64,
    ) -> Result<Vec<MessageRow>> {
        let topic = self
            .metadata
            .get_topic(topic_name)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(topic_name.to_string()))?;

        let mut all_messages: Vec<MessageRow> = Vec::new();

        for partition_id in 0..topic.partition_count {
            if start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(SqlError::Timeout(timeout_ms));
            }

            let segments = self
                .metadata
                .get_segments(topic_name, partition_id)
                .await?;

            for segment in segments {
                let messages = self.read_segment_messages(&segment).await?;
                all_messages.extend(messages);
            }
        }

        Ok(all_messages)
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

    // ========================================================================
    // Materialized View Methods
    // ========================================================================

    /// Execute CREATE MATERIALIZED VIEW command
    async fn execute_create_materialized_view(
        &self,
        query: CreateMaterializedViewQuery,
        start: Instant,
    ) -> Result<QueryResult> {
        // Validate source topic exists
        let _topic = self
            .metadata
            .get_topic(&query.source_topic)
            .await?
            .ok_or_else(|| SqlError::TopicNotFound(query.source_topic.clone()))?;

        // For now, return a success message
        // TODO: Persist view definition to metadata store
        let row = vec![
            serde_json::Value::String(query.name.clone()),
            serde_json::Value::String(query.source_topic.clone()),
            serde_json::Value::String(format!("{:?}", query.refresh_mode)),
            serde_json::Value::String("created".to_string()),
        ];

        Ok(QueryResult {
            columns: vec![
                ColumnInfo { name: "name".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "source_topic".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "refresh_mode".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "status".to_string(), data_type: "string".to_string() },
            ],
            rows: vec![row],
            row_count: 1,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    /// Execute DROP MATERIALIZED VIEW command
    async fn execute_drop_materialized_view(
        &self,
        name: &str,
        start: Instant,
    ) -> Result<QueryResult> {
        // TODO: Actually drop from metadata store
        Ok(QueryResult {
            columns: vec![
                ColumnInfo { name: "name".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "status".to_string(), data_type: "string".to_string() },
            ],
            rows: vec![vec![
                serde_json::Value::String(name.to_string()),
                serde_json::Value::String("dropped".to_string()),
            ]],
            row_count: 1,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    /// Execute REFRESH MATERIALIZED VIEW command
    async fn execute_refresh_materialized_view(
        &self,
        name: &str,
        start: Instant,
    ) -> Result<QueryResult> {
        // TODO: Trigger actual refresh from metadata store
        Ok(QueryResult {
            columns: vec![
                ColumnInfo { name: "name".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "status".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "rows_processed".to_string(), data_type: "bigint".to_string() },
            ],
            rows: vec![vec![
                serde_json::Value::String(name.to_string()),
                serde_json::Value::String("refreshed".to_string()),
                serde_json::Value::Number(0.into()),
            ]],
            row_count: 1,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    /// Execute SHOW MATERIALIZED VIEWS command
    async fn execute_show_materialized_views(
        &self,
        start: Instant,
    ) -> Result<QueryResult> {
        // TODO: Fetch from metadata store
        Ok(QueryResult {
            columns: vec![
                ColumnInfo { name: "name".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "source_topic".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "refresh_mode".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "status".to_string(), data_type: "string".to_string() },
                ColumnInfo { name: "row_count".to_string(), data_type: "bigint".to_string() },
            ],
            rows: vec![], // No views yet
            row_count: 0,
            execution_time_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    /// Execute DESCRIBE MATERIALIZED VIEW command
    async fn execute_describe_materialized_view(
        &self,
        name: &str,
        start: Instant,
    ) -> Result<QueryResult> {
        // TODO: Fetch from metadata store
        // For now, return view not found
        Err(SqlError::ViewNotFound(name.to_string()))
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

// Window grouping types - exported for use by materialized view maintenance
/// Window key: (window_start, window_end, group_key)
pub type WindowKey = (i64, i64, Option<String>);
/// Map of window key to messages in that window
pub type WindowGroups = std::collections::BTreeMap<WindowKey, Vec<MessageRow>>;

/// Group messages into tumbling windows
pub fn group_into_tumble_windows(
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
pub fn group_into_hop_windows(
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
pub fn group_into_session_windows(
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
pub fn extract_group_key(msg: &MessageRow, group_by: &[String]) -> Option<String> {
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
pub fn compute_aggregation(agg: &WindowAggregation, messages: &[MessageRow]) -> serde_json::Value {
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

// ============================================================================
// JOIN Helper Functions
// ============================================================================

/// Build a hash index on messages keyed by join key
fn build_join_index(
    messages: &[MessageRow],
    key_spec: &(String, String), // (qualifier, path)
) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    let (_, path) = key_spec;

    for (idx, msg) in messages.iter().enumerate() {
        if let Some(key_value) = extract_join_key(msg, path) {
            index.entry(key_value).or_default().push(idx);
        }
    }

    index
}

/// Extract join key value from a message
fn extract_join_key(msg: &MessageRow, path: &str) -> Option<String> {
    match path {
        "key" => msg.key.clone(),
        "offset" => Some(msg.offset.to_string()),
        "partition" => Some(msg.partition.to_string()),
        "timestamp" => Some(msg.timestamp.to_string()),
        _ if path.starts_with("$.") => {
            // JSON path extraction
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                let extracted = extract_json_path_for_stats(&json, path);
                match extracted {
                    serde_json::Value::String(s) => Some(s),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    serde_json::Value::Null => None,
                    _ => Some(extracted.to_string()),
                }
            } else {
                None
            }
        }
        _ => {
            // Try as JSON path without $. prefix
            let full_path = format!("$.{}", path);
            extract_join_key(msg, &full_path)
        }
    }
}

// ============================================================================
// Predicate Pushdown Helpers
// ============================================================================

/// Determine if a filter can be pushed down to a specific side of the join
/// Returns true if the filter applies to simple partition/offset/timestamp filters
fn filter_applies_to_side(filter: &Filter, _table_ref: &TableRef) -> bool {
    // Partition, offset, and timestamp filters can be pushed down to either side
    // since they're not qualified with table alias
    matches!(
        filter,
        Filter::PartitionEquals(_)
            | Filter::OffsetGte(_)
            | Filter::OffsetLt(_)
            | Filter::OffsetEquals(_)
            | Filter::TimestampGte(_)
            | Filter::TimestampLt(_)
    )
}

/// Apply pushdown filters to a message
/// Returns true if the message passes all filters
fn apply_pushdown_filters(msg: &MessageRow, filters: &[&Filter]) -> bool {
    for filter in filters {
        match filter {
            Filter::PartitionEquals(p) => {
                if msg.partition != *p {
                    return false;
                }
            }
            Filter::OffsetGte(offset) => {
                if msg.offset < *offset {
                    return false;
                }
            }
            Filter::OffsetLt(offset) => {
                if msg.offset >= *offset {
                    return false;
                }
            }
            Filter::OffsetEquals(offset) => {
                if msg.offset != *offset {
                    return false;
                }
            }
            Filter::TimestampGte(ts) => {
                if msg.timestamp < *ts {
                    return false;
                }
            }
            Filter::TimestampLt(ts) => {
                if msg.timestamp >= *ts {
                    return false;
                }
            }
            // Other filters (KeyEquals, JsonEquals, etc.) are applied post-join
            _ => {}
        }
    }
    true
}

/// Perform the actual join operation
fn perform_join(
    left_messages: &[MessageRow],
    right_messages: &[MessageRow],
    right_index: &HashMap<String, Vec<usize>>,
    condition: &JoinCondition,
    join_type: &JoinType,
    _left_ref: &TableRef,
    _right_ref: &TableRef,
) -> Vec<(Option<MessageRow>, Option<MessageRow>)> {
    let mut results: Vec<(Option<MessageRow>, Option<MessageRow>)> = Vec::new();
    let left_path = &condition.left.1;

    // Track which right rows have been matched (for FULL JOIN)
    let mut right_matched: HashSet<usize> = HashSet::new();

    // For each left row, find matching right rows
    for left_msg in left_messages {
        if let Some(left_key) = extract_join_key(left_msg, left_path) {
            if let Some(right_indices) = right_index.get(&left_key) {
                // Found matches
                for &right_idx in right_indices {
                    right_matched.insert(right_idx);
                    results.push((
                        Some(left_msg.clone()),
                        Some(right_messages[right_idx].clone()),
                    ));
                }
            } else {
                // No match on right side
                match join_type {
                    JoinType::Left | JoinType::Full => {
                        results.push((Some(left_msg.clone()), None));
                    }
                    JoinType::Inner | JoinType::Right => {
                        // Don't include unmatched left rows for INNER or RIGHT join
                    }
                }
            }
        } else {
            // Left key is null
            match join_type {
                JoinType::Left | JoinType::Full => {
                    results.push((Some(left_msg.clone()), None));
                }
                _ => {}
            }
        }
    }

    // For RIGHT and FULL joins, include unmatched right rows
    if matches!(join_type, JoinType::Right | JoinType::Full) {
        for (idx, right_msg) in right_messages.iter().enumerate() {
            if !right_matched.contains(&idx) {
                results.push((None, Some(right_msg.clone())));
            }
        }
    }

    results
}

// ============================================================================
// Stream-Table JOIN Helper Functions
// ============================================================================

/// Build table state: key→latest_value map for compacted table semantics
/// This keeps only the latest value for each key, simulating a changelog table
fn build_table_state(
    messages: &[MessageRow],
    key_spec: &(String, String), // (qualifier, path)
) -> HashMap<String, MessageRow> {
    let mut table: HashMap<String, MessageRow> = HashMap::new();
    let (_, path) = key_spec;

    // Process messages in order, later messages overwrite earlier ones
    for msg in messages {
        if let Some(key_value) = extract_join_key(msg, path) {
            table.insert(key_value, msg.clone());
        }
    }

    table
}

/// Perform stream-table join with O(1) lookups
/// Left side is the stream, right side is the table (compacted key→value)
fn perform_stream_table_join(
    stream_messages: &[MessageRow],
    table: &HashMap<String, MessageRow>,
    condition: &JoinCondition,
    join_type: &JoinType,
) -> Vec<(Option<MessageRow>, Option<MessageRow>)> {
    let mut results: Vec<(Option<MessageRow>, Option<MessageRow>)> = Vec::new();
    let stream_path = &condition.left.1;

    // Track which table rows have been matched (for FULL/RIGHT joins)
    let mut table_matched: HashSet<String> = HashSet::new();

    // For each stream row, do O(1) lookup in table
    for stream_msg in stream_messages {
        if let Some(stream_key) = extract_join_key(stream_msg, stream_path) {
            if let Some(table_msg) = table.get(&stream_key) {
                // Found match - O(1) lookup
                table_matched.insert(stream_key);
                results.push((Some(stream_msg.clone()), Some(table_msg.clone())));
            } else {
                // No match in table
                match join_type {
                    JoinType::Left | JoinType::Full => {
                        results.push((Some(stream_msg.clone()), None));
                    }
                    JoinType::Inner | JoinType::Right => {
                        // Don't include unmatched stream rows
                    }
                }
            }
        } else {
            // Stream key is null
            match join_type {
                JoinType::Left | JoinType::Full => {
                    results.push((Some(stream_msg.clone()), None));
                }
                _ => {}
            }
        }
    }

    // For RIGHT and FULL joins, include unmatched table rows
    if matches!(join_type, JoinType::Right | JoinType::Full) {
        for (key, table_msg) in table {
            if !table_matched.contains(key) {
                results.push((None, Some(table_msg.clone())));
            }
        }
    }

    results
}

/// Swap join type when reversing left/right sides
fn swap_join_type(join_type: &JoinType) -> JoinType {
    match join_type {
        JoinType::Inner => JoinType::Inner,
        JoinType::Left => JoinType::Right,
        JoinType::Right => JoinType::Left,
        JoinType::Full => JoinType::Full,
    }
}

/// Build column info for JOIN results
fn build_join_column_info(
    columns: &[JoinSelectColumn],
    left: &TableRef,
    right: &TableRef,
) -> Vec<ColumnInfo> {
    let mut result = Vec::new();

    for col in columns {
        match col {
            JoinSelectColumn::AllFrom(qualifier) => {
                match qualifier {
                    None => {
                        // All columns from both tables
                        for prefix in [left.qualifier(), right.qualifier()] {
                            result.extend(vec![
                                ColumnInfo { name: format!("{}.topic", prefix), data_type: "string".to_string() },
                                ColumnInfo { name: format!("{}.partition", prefix), data_type: "integer".to_string() },
                                ColumnInfo { name: format!("{}.offset", prefix), data_type: "bigint".to_string() },
                                ColumnInfo { name: format!("{}.key", prefix), data_type: "string".to_string() },
                                ColumnInfo { name: format!("{}.value", prefix), data_type: "string".to_string() },
                                ColumnInfo { name: format!("{}.timestamp", prefix), data_type: "bigint".to_string() },
                            ]);
                        }
                    }
                    Some(qual) => {
                        // All columns from one table
                        result.extend(vec![
                            ColumnInfo { name: format!("{}.topic", qual), data_type: "string".to_string() },
                            ColumnInfo { name: format!("{}.partition", qual), data_type: "integer".to_string() },
                            ColumnInfo { name: format!("{}.offset", qual), data_type: "bigint".to_string() },
                            ColumnInfo { name: format!("{}.key", qual), data_type: "string".to_string() },
                            ColumnInfo { name: format!("{}.value", qual), data_type: "string".to_string() },
                            ColumnInfo { name: format!("{}.timestamp", qual), data_type: "bigint".to_string() },
                        ]);
                    }
                }
            }
            JoinSelectColumn::QualifiedColumn { qualifier, column, alias } => {
                let name = alias.clone().unwrap_or_else(|| {
                    if qualifier.is_empty() {
                        column.clone()
                    } else {
                        format!("{}.{}", qualifier, column)
                    }
                });
                let data_type = match column.as_str() {
                    "partition" => "integer",
                    "offset" | "timestamp" => "bigint",
                    _ => "string",
                };
                result.push(ColumnInfo { name, data_type: data_type.to_string() });
            }
            JoinSelectColumn::QualifiedJsonExtract { qualifier, path, alias } => {
                let name = alias.clone().unwrap_or_else(|| {
                    if qualifier.is_empty() {
                        path.clone()
                    } else {
                        format!("{}.{}", qualifier, path)
                    }
                });
                result.push(ColumnInfo { name, data_type: "json".to_string() });
            }
        }
    }

    result
}

/// Project join result to output row
fn project_join_row(
    columns: &[JoinSelectColumn],
    left_msg: Option<&MessageRow>,
    right_msg: Option<&MessageRow>,
    left_ref: &TableRef,
    right_ref: &TableRef,
) -> Row {
    let mut row = Vec::new();

    for col in columns {
        match col {
            JoinSelectColumn::AllFrom(qualifier) => {
                match qualifier {
                    None => {
                        // All columns from both tables
                        row.extend(message_to_row_values(left_msg));
                        row.extend(message_to_row_values(right_msg));
                    }
                    Some(qual) => {
                        // All columns from one table
                        let msg = if qual == left_ref.qualifier() {
                            left_msg
                        } else {
                            right_msg
                        };
                        row.extend(message_to_row_values(msg));
                    }
                }
            }
            JoinSelectColumn::QualifiedColumn { qualifier, column, .. } => {
                let msg = resolve_qualifier(qualifier, left_msg, right_msg, left_ref, right_ref);
                let value = extract_column_value(msg, column);
                row.push(value);
            }
            JoinSelectColumn::QualifiedJsonExtract { qualifier, path, .. } => {
                let msg = resolve_qualifier(qualifier, left_msg, right_msg, left_ref, right_ref);
                let value = if let Some(m) = msg {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&m.value) {
                        extract_json_path_for_stats(&json, path)
                    } else {
                        serde_json::Value::Null
                    }
                } else {
                    serde_json::Value::Null
                };
                row.push(value);
            }
        }
    }

    row
}

/// Convert a message to row values
fn message_to_row_values(msg: Option<&MessageRow>) -> Vec<serde_json::Value> {
    match msg {
        Some(m) => vec![
            serde_json::Value::String(m.topic.clone()),
            serde_json::Value::Number(m.partition.into()),
            serde_json::Value::Number(m.offset.into()),
            m.key.as_ref()
                .map(|k| serde_json::Value::String(k.clone()))
                .unwrap_or(serde_json::Value::Null),
            serde_json::Value::String(m.value.clone()),
            serde_json::Value::Number(m.timestamp.into()),
        ],
        None => vec![
            serde_json::Value::Null,
            serde_json::Value::Null,
            serde_json::Value::Null,
            serde_json::Value::Null,
            serde_json::Value::Null,
            serde_json::Value::Null,
        ],
    }
}

/// Resolve which message to use based on qualifier
fn resolve_qualifier<'a>(
    qualifier: &str,
    left_msg: Option<&'a MessageRow>,
    right_msg: Option<&'a MessageRow>,
    left_ref: &TableRef,
    right_ref: &TableRef,
) -> Option<&'a MessageRow> {
    if qualifier.is_empty() {
        // Try left first, then right
        left_msg.or(right_msg)
    } else if qualifier == left_ref.qualifier() {
        left_msg
    } else if qualifier == right_ref.qualifier() {
        right_msg
    } else {
        None
    }
}

/// Extract a column value from a message
fn extract_column_value(msg: Option<&MessageRow>, column: &str) -> serde_json::Value {
    match msg {
        Some(m) => match column {
            "topic" => serde_json::Value::String(m.topic.clone()),
            "partition" => serde_json::Value::Number(m.partition.into()),
            "offset" => serde_json::Value::Number(m.offset.into()),
            "key" => m.key.as_ref()
                .map(|k| serde_json::Value::String(k.clone()))
                .unwrap_or(serde_json::Value::Null),
            "value" => serde_json::Value::String(m.value.clone()),
            "timestamp" => serde_json::Value::Number(m.timestamp.into()),
            _ => {
                // Try as JSON path
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&m.value) {
                    let path = if column.starts_with("$.") {
                        column.to_string()
                    } else {
                        format!("$.{}", column)
                    };
                    extract_json_path_for_stats(&json, &path)
                } else {
                    serde_json::Value::Null
                }
            }
        },
        None => serde_json::Value::Null,
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
