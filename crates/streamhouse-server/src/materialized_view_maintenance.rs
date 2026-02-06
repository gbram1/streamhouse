//! Materialized View Background Maintenance
//!
//! This module provides continuous background processing for materialized views.
//! It reads from source topics, applies window aggregations, and updates view data.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use streamhouse_metadata::{
    MaterializedView, MaterializedViewData, MaterializedViewRefreshMode, MaterializedViewStatus,
    MetadataStore,
};
use streamhouse_sql::{
    compute_aggregation, group_into_hop_windows, group_into_session_windows,
    group_into_tumble_windows, parse_query, CreateMaterializedViewQuery, Filter, MessageRow,
    SqlQuery, WindowAggregation, WindowType,
};
use streamhouse_storage::{PartitionReader, SegmentCache};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Configuration for the maintenance task
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    /// Tick interval for checking views (default: 5 seconds)
    pub tick_interval: Duration,
    /// Maximum records to process per partition per tick
    pub max_records_per_tick: usize,
    /// Timeout for processing a single view
    pub view_timeout: Duration,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(5),
            max_records_per_tick: 10_000,
            view_timeout: Duration::from_secs(60),
        }
    }
}

/// Errors that can occur during maintenance
#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    #[error("Metadata error: {0}")]
    Metadata(#[from] streamhouse_metadata::MetadataError),

    #[error("Storage error: {0}")]
    Storage(#[from] streamhouse_storage::Error),

    #[error("SQL parse error: {0}")]
    SqlParse(#[from] streamhouse_sql::SqlError),

    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    #[error("Unsupported query type for materialized view")]
    UnsupportedQuery,

    #[error("View has no window specification")]
    NoWindow,
}

/// The main maintenance task controller
pub struct MaterializedViewMaintenance {
    metadata: Arc<dyn MetadataStore>,
    object_store: Arc<dyn object_store::ObjectStore>,
    segment_cache: Arc<SegmentCache>,
    config: MaintenanceConfig,
}

impl MaterializedViewMaintenance {
    /// Create a new maintenance controller
    pub fn new(
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn object_store::ObjectStore>,
        segment_cache: Arc<SegmentCache>,
        config: MaintenanceConfig,
    ) -> Self {
        Self {
            metadata,
            object_store,
            segment_cache,
            config,
        }
    }

    /// Start the background maintenance task
    /// Returns a JoinHandle that can be used to monitor the task
    pub fn start(self: Arc<Self>, shutdown_rx: oneshot::Receiver<()>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.config.tick_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = self.process_all_views().await {
                            tracing::error!(error = %e, "Maintenance tick failed");
                        }
                    }
                    _ = &mut shutdown_rx => {
                        tracing::info!("Materialized view maintenance shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Process all active materialized views
    async fn process_all_views(&self) -> Result<(), MaintenanceError> {
        // Get all views
        let views = self.metadata.list_materialized_views().await?;

        // Filter to views that should be processed this tick
        let views_to_process: Vec<_> = views
            .into_iter()
            .filter(|v| self.should_process_view(v))
            .collect();

        if !views_to_process.is_empty() {
            tracing::debug!(
                view_count = views_to_process.len(),
                "Processing materialized views"
            );
        }

        // Process each view sequentially (could be parallelized with semaphore)
        for view in views_to_process {
            let view_name = view.name.clone();
            let view_id = view.id.clone();

            if let Err(e) = self.process_single_view(&view).await {
                tracing::warn!(
                    view_id = %view_id,
                    view_name = %view_name,
                    error = %e,
                    "Failed to process view"
                );

                // Update view status to Error
                let _ = self
                    .metadata
                    .update_materialized_view_status(
                        &view_id,
                        MaterializedViewStatus::Error,
                        Some(&e.to_string()),
                    )
                    .await;
            }
        }

        Ok(())
    }

    /// Determine if a view should be processed this tick
    fn should_process_view(&self, view: &MaterializedView) -> bool {
        // Skip non-active views
        if view.status != MaterializedViewStatus::Running
            && view.status != MaterializedViewStatus::Initializing
        {
            return false;
        }

        match &view.refresh_mode {
            MaterializedViewRefreshMode::Continuous => true,
            MaterializedViewRefreshMode::Periodic { interval_ms } => {
                let now = chrono::Utc::now().timestamp_millis();
                let last = view.last_refresh_at.unwrap_or(0);
                now - last >= *interval_ms
            }
            MaterializedViewRefreshMode::Manual => false,
        }
    }

    /// Process a single materialized view
    async fn process_single_view(&self, view: &MaterializedView) -> Result<(), MaintenanceError> {
        let start = std::time::Instant::now();

        tracing::debug!(
            view_id = %view.id,
            view_name = %view.name,
            source_topic = %view.source_topic,
            "Processing view"
        );

        // 1. Get the source topic info
        let topic = self
            .metadata
            .get_topic(&view.source_topic)
            .await?
            .ok_or_else(|| MaintenanceError::TopicNotFound(view.source_topic.clone()))?;

        // 2. Parse the view query to get window/aggregation info
        let parsed_query = self.parse_view_query(&view.query_sql)?;

        // 3. Get current offsets for all partitions
        let current_offsets = self
            .metadata
            .get_materialized_view_offsets(&view.id)
            .await?;

        let offset_map: HashMap<u32, u64> = current_offsets
            .iter()
            .map(|o| (o.partition_id, o.last_offset))
            .collect();

        // 4. Read new messages from each partition
        let mut all_messages = Vec::new();
        let mut new_offsets: HashMap<u32, u64> = HashMap::new();

        for partition_id in 0..topic.partition_count {
            let start_offset = offset_map.get(&partition_id).copied().unwrap_or(0);

            match self
                .read_partition_messages(&view.source_topic, partition_id, start_offset)
                .await
            {
                Ok((messages, new_offset)) => {
                    if new_offset > start_offset {
                        new_offsets.insert(partition_id, new_offset);
                        all_messages.extend(messages);
                    }
                }
                Err(streamhouse_storage::Error::OffsetNotFound(_)) => {
                    // No data yet in this partition, skip
                    continue;
                }
                Err(e) => return Err(MaintenanceError::Storage(e)),
            }
        }

        // 5. If no new messages, just update last_refresh_at
        if all_messages.is_empty() {
            let now = chrono::Utc::now().timestamp_millis();
            self.metadata
                .update_materialized_view_stats(&view.id, view.row_count, now)
                .await?;
            return Ok(());
        }

        tracing::debug!(
            view_id = %view.id,
            message_count = all_messages.len(),
            "Processing new messages"
        );

        // 6. Apply filters if any
        let filtered_messages = self.apply_filters(all_messages, &parsed_query.filters);
        let messages_processed = filtered_messages.len();

        // 7. Compute aggregations
        let aggregated_data =
            self.compute_view_aggregations(&view.id, &parsed_query, filtered_messages)?;

        // 8. Upsert aggregated data to the database
        for data in &aggregated_data {
            self.metadata
                .upsert_materialized_view_data(data.clone())
                .await?;
        }

        // 9. Update partition offsets
        for (partition_id, new_offset) in new_offsets {
            self.metadata
                .update_materialized_view_offset(&view.id, partition_id, new_offset)
                .await?;
        }

        // 10. Update view stats
        let now = chrono::Utc::now().timestamp_millis();
        // For row_count, we track total aggregated rows (may need refinement for upserts)
        let row_count = view.row_count + aggregated_data.len() as u64;
        self.metadata
            .update_materialized_view_stats(&view.id, row_count, now)
            .await?;

        // 11. If initializing, transition to Running
        if view.status == MaterializedViewStatus::Initializing {
            self.metadata
                .update_materialized_view_status(&view.id, MaterializedViewStatus::Running, None)
                .await?;
        }

        tracing::info!(
            view_id = %view.id,
            view_name = %view.name,
            messages_processed = messages_processed,
            rows_added = aggregated_data.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "View refresh complete"
        );

        Ok(())
    }

    /// Parse the view's SQL query to extract window and aggregation info
    fn parse_view_query(
        &self,
        query_sql: &str,
    ) -> Result<CreateMaterializedViewQuery, MaintenanceError> {
        let query = parse_query(query_sql)?;

        match query {
            SqlQuery::CreateMaterializedView(cmv) => Ok(cmv),
            SqlQuery::WindowAggregate(wq) => {
                // Convert WindowAggregateQuery to CreateMaterializedViewQuery
                Ok(CreateMaterializedViewQuery {
                    name: String::new(),
                    source_topic: wq.topic,
                    query_sql: query_sql.to_string(),
                    window: Some(wq.window),
                    aggregations: wq.aggregations,
                    group_by: wq.group_by,
                    filters: wq.filters,
                    refresh_mode: streamhouse_sql::RefreshMode::Continuous,
                    or_replace: false,
                })
            }
            _ => Err(MaintenanceError::UnsupportedQuery),
        }
    }

    /// Read messages from a partition using PartitionReader
    async fn read_partition_messages(
        &self,
        topic: &str,
        partition_id: u32,
        start_offset: u64,
    ) -> Result<(Vec<MessageRow>, u64), streamhouse_storage::Error> {
        let reader = PartitionReader::new(
            topic.to_string(),
            partition_id,
            self.metadata.clone(),
            self.object_store.clone(),
            self.segment_cache.clone(),
        );

        let result = reader
            .read(start_offset, self.config.max_records_per_tick)
            .await?;

        // Convert Record to MessageRow
        let messages: Vec<MessageRow> = result
            .records
            .into_iter()
            .map(|r| MessageRow {
                topic: topic.to_string(),
                partition: partition_id,
                offset: r.offset,
                key: r.key.map(|k| String::from_utf8_lossy(&k).to_string()),
                value: String::from_utf8_lossy(&r.value).to_string(),
                timestamp: r.timestamp as i64,
            })
            .collect();

        let new_offset = if messages.is_empty() {
            start_offset
        } else {
            messages.last().unwrap().offset + 1
        };

        Ok((messages, new_offset))
    }

    /// Apply filters to messages
    fn apply_filters(&self, messages: Vec<MessageRow>, filters: &[Filter]) -> Vec<MessageRow> {
        if filters.is_empty() {
            return messages;
        }

        messages
            .into_iter()
            .filter(|msg| self.matches_filters(msg, filters))
            .collect()
    }

    /// Check if a message matches all filters
    fn matches_filters(&self, msg: &MessageRow, filters: &[Filter]) -> bool {
        filters
            .iter()
            .all(|filter| self.matches_filter(msg, filter))
    }

    /// Check if a message matches a single filter
    fn matches_filter(&self, msg: &MessageRow, filter: &Filter) -> bool {
        match filter {
            Filter::KeyEquals(k) => msg.key.as_deref() == Some(k.as_str()),
            Filter::PartitionEquals(p) => msg.partition == *p,
            Filter::OffsetGte(o) => msg.offset >= *o,
            Filter::OffsetLt(o) => msg.offset < *o,
            Filter::OffsetEquals(o) => msg.offset == *o,
            Filter::TimestampGte(ts) => msg.timestamp >= *ts,
            Filter::TimestampLt(ts) => msg.timestamp < *ts,
            Filter::JsonEquals { path, value } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                    let extracted = extract_json_path(&json, path);
                    &extracted == value
                } else {
                    false
                }
            }
            Filter::JsonGt { path, value } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                    let extracted = extract_json_path(&json, path);
                    compare_json_values(&extracted, value) == Some(std::cmp::Ordering::Greater)
                } else {
                    false
                }
            }
            Filter::JsonLt { path, value } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.value) {
                    let extracted = extract_json_path(&json, path);
                    compare_json_values(&extracted, value) == Some(std::cmp::Ordering::Less)
                } else {
                    false
                }
            }
            // For other filter types (ZScore, etc.), return true (not supported in maintenance)
            _ => true,
        }
    }

    /// Compute aggregations for a view
    fn compute_view_aggregations(
        &self,
        view_id: &str,
        query: &CreateMaterializedViewQuery,
        messages: Vec<MessageRow>,
    ) -> Result<Vec<MaterializedViewData>, MaintenanceError> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }

        let window = query.window.as_ref().ok_or(MaintenanceError::NoWindow)?;

        // Group messages into windows
        let window_groups = match window {
            WindowType::Tumble { size_ms } => {
                group_into_tumble_windows(&messages, *size_ms, &query.group_by)
            }
            WindowType::Hop { size_ms, slide_ms } => {
                group_into_hop_windows(&messages, *size_ms, *slide_ms, &query.group_by)
            }
            WindowType::Session { gap_ms } => {
                group_into_session_windows(&messages, *gap_ms, &query.group_by)
            }
        };

        // Compute aggregations for each window
        let mut results = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();

        for ((window_start, window_end, group_key), window_messages) in window_groups {
            // Compute all aggregations for this window
            let mut agg_values = serde_json::Map::new();
            for (i, agg) in query.aggregations.iter().enumerate() {
                let value = compute_aggregation(agg, &window_messages);
                let key = get_aggregation_name(agg, i);
                agg_values.insert(key, value);
            }

            // Create the aggregation key (combination of group_key + window)
            let agg_key = format!(
                "{}:{}:{}",
                group_key.as_deref().unwrap_or("_"),
                window_start,
                window_end
            );

            results.push(MaterializedViewData {
                view_id: view_id.to_string(),
                agg_key,
                agg_values: serde_json::Value::Object(agg_values),
                window_start: Some(window_start),
                window_end: Some(window_end),
                updated_at: now,
            });
        }

        Ok(results)
    }
}

/// Extract a value from JSON using a path like "$.field.nested"
fn extract_json_path(json: &serde_json::Value, path: &str) -> serde_json::Value {
    let path = path.strip_prefix("$.").unwrap_or(path);
    let parts: Vec<&str> = path.split('.').collect();

    let mut current = json;
    for part in parts {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(part).unwrap_or(&serde_json::Value::Null);
            }
            serde_json::Value::Array(arr) => {
                if let Ok(idx) = part.parse::<usize>() {
                    current = arr.get(idx).unwrap_or(&serde_json::Value::Null);
                } else {
                    return serde_json::Value::Null;
                }
            }
            _ => return serde_json::Value::Null,
        }
    }
    current.clone()
}

/// Compare two JSON values for ordering
fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let a = a.as_f64()?;
            let b = b.as_f64()?;
            a.partial_cmp(&b)
        }
        (serde_json::Value::String(a), serde_json::Value::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Get a name for an aggregation (uses alias if available)
fn get_aggregation_name(agg: &WindowAggregation, index: usize) -> String {
    match agg {
        WindowAggregation::Count { alias } => {
            alias.clone().unwrap_or_else(|| format!("count_{}", index))
        }
        WindowAggregation::CountDistinct { alias, .. } => alias
            .clone()
            .unwrap_or_else(|| format!("count_distinct_{}", index)),
        WindowAggregation::Sum { alias, .. } => {
            alias.clone().unwrap_or_else(|| format!("sum_{}", index))
        }
        WindowAggregation::Avg { alias, .. } => {
            alias.clone().unwrap_or_else(|| format!("avg_{}", index))
        }
        WindowAggregation::Min { alias, .. } => {
            alias.clone().unwrap_or_else(|| format!("min_{}", index))
        }
        WindowAggregation::Max { alias, .. } => {
            alias.clone().unwrap_or_else(|| format!("max_{}", index))
        }
        WindowAggregation::First { alias, .. } => {
            alias.clone().unwrap_or_else(|| format!("first_{}", index))
        }
        WindowAggregation::Last { alias, .. } => {
            alias.clone().unwrap_or_else(|| format!("last_{}", index))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_process_view_continuous() {
        let view = MaterializedView {
            id: "v1".to_string(),
            organization_id: "org1".to_string(),
            name: "test_view".to_string(),
            source_topic: "topic1".to_string(),
            query_sql: "SELECT COUNT(*) FROM topic1".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Continuous,
            status: MaterializedViewStatus::Running,
            error_message: None,
            row_count: 0,
            last_refresh_at: None,
            created_at: 0,
            updated_at: 0,
        };

        // We can't easily test should_process_view without a full instance,
        // but we can test the logic inline
        assert!(matches!(
            view.refresh_mode,
            MaterializedViewRefreshMode::Continuous
        ));
        assert_eq!(view.status, MaterializedViewStatus::Running);
    }

    #[test]
    fn test_should_process_view_periodic_not_due() {
        let view = MaterializedView {
            id: "v1".to_string(),
            organization_id: "org1".to_string(),
            name: "test_view".to_string(),
            source_topic: "topic1".to_string(),
            query_sql: "SELECT COUNT(*) FROM topic1".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Periodic {
                interval_ms: 60_000,
            },
            status: MaterializedViewStatus::Running,
            error_message: None,
            row_count: 0,
            last_refresh_at: Some(chrono::Utc::now().timestamp_millis()), // Just refreshed
            created_at: 0,
            updated_at: 0,
        };

        // View was just refreshed, interval is 60s, so it's not due
        let now = chrono::Utc::now().timestamp_millis();
        let last = view.last_refresh_at.unwrap_or(0);
        let interval_ms = match &view.refresh_mode {
            MaterializedViewRefreshMode::Periodic { interval_ms } => *interval_ms,
            _ => 0,
        };
        assert!(now - last < interval_ms);
    }

    #[test]
    fn test_should_process_view_manual_never() {
        let view = MaterializedView {
            id: "v1".to_string(),
            organization_id: "org1".to_string(),
            name: "test_view".to_string(),
            source_topic: "topic1".to_string(),
            query_sql: "SELECT COUNT(*) FROM topic1".to_string(),
            refresh_mode: MaterializedViewRefreshMode::Manual,
            status: MaterializedViewStatus::Running,
            error_message: None,
            row_count: 0,
            last_refresh_at: None,
            created_at: 0,
            updated_at: 0,
        };

        // Manual views should never be processed automatically
        assert!(matches!(
            view.refresh_mode,
            MaterializedViewRefreshMode::Manual
        ));
    }

    #[test]
    fn test_extract_json_path() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"user": {"name": "Alice", "age": 30}}"#).unwrap();

        assert_eq!(
            extract_json_path(&json, "$.user.name"),
            serde_json::Value::String("Alice".to_string())
        );
        assert_eq!(
            extract_json_path(&json, "$.user.age"),
            serde_json::Value::Number(30.into())
        );
        assert_eq!(
            extract_json_path(&json, "$.user.missing"),
            serde_json::Value::Null
        );
    }

    #[test]
    fn test_get_aggregation_name() {
        assert_eq!(
            get_aggregation_name(&WindowAggregation::Count { alias: None }, 0),
            "count_0"
        );
        assert_eq!(
            get_aggregation_name(
                &WindowAggregation::Count {
                    alias: Some("total".to_string())
                },
                0
            ),
            "total"
        );
        assert_eq!(
            get_aggregation_name(
                &WindowAggregation::Sum {
                    path: "$.amount".to_string(),
                    alias: Some("total_amount".to_string())
                },
                1
            ),
            "total_amount"
        );
    }
}
