//! SQL query executor

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

        // Collect rows
        let mut rows: Vec<Row> = Vec::new();
        let limit = query.limit.unwrap_or(MAX_ROWS).min(MAX_ROWS);
        let skip = query.offset.unwrap_or(0);

        'outer: for partition_id in partitions {
            // Check timeout
            if start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(SqlError::Timeout(timeout_ms));
            }

            // Get partition info
            let partition = match self.metadata.get_partition(&query.topic, partition_id).await? {
                Some(p) => p,
                None => continue,
            };

            // Determine scan range
            let scan_start = offset_start.unwrap_or(0);
            let scan_end = offset_end.unwrap_or(partition.high_watermark);

            // Get segments covering this range
            let segments = self
                .metadata
                .get_segments(&query.topic, partition_id)
                .await?;

            for segment in segments {
                // Check if segment overlaps with our range
                if segment.end_offset < scan_start || segment.base_offset > scan_end {
                    continue;
                }

                // Read messages from segment
                let messages = self.read_segment_messages(&segment).await?;

                for msg in messages {
                    // Apply filters
                    if !msg.matches_filters(&query.filters) {
                        continue;
                    }

                    // Skip offset
                    if rows.len() < skip {
                        rows.push(vec![]); // placeholder for skip counting
                        continue;
                    }

                    // Add row
                    let row = msg.to_row(&query.columns);
                    rows.push(row);

                    // Check limit
                    if rows.len() >= skip + limit {
                        break 'outer;
                    }
                }
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
        }
    }

    result
}
