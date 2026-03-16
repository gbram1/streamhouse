//! Pipeline Consume Loop
//!
//! Reads from topic partitions, applies optional SQL transforms, and sinks records
//! to an external system via a SinkConnector.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;

use streamhouse_metadata::MetadataStore;
use streamhouse_storage::{PartitionReader, SegmentCache};

use crate::traits::{SinkConnector, SinkRecord};

/// A transform function that takes a batch of records and returns transformed records.
/// This allows the consume loop to apply SQL transforms without depending on streamhouse-sql.
pub type TransformFn = Box<
    dyn Fn(
            Vec<SinkRecord>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = std::result::Result<
                            Vec<SinkRecord>,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

/// Configuration for the pipeline consume loop.
pub struct PipelineConsumeLoopConfig {
    pub org_id: String,
    pub pipeline_name: String,
    pub source_topic: String,
    pub partition_count: u32,
    pub consumer_group: String,
    pub max_records_per_poll: usize,
}

impl Default for PipelineConsumeLoopConfig {
    fn default() -> Self {
        Self {
            org_id: String::new(),
            pipeline_name: String::new(),
            source_topic: String::new(),
            partition_count: 1,
            consumer_group: String::new(),
            max_records_per_poll: 1000,
        }
    }
}

/// The pipeline consume loop.
pub struct PipelineConsumeLoop {
    config: PipelineConsumeLoopConfig,
    sink: Box<dyn SinkConnector>,
    transform: Option<TransformFn>,
    metadata: Arc<dyn MetadataStore>,
    object_store: Arc<dyn object_store::ObjectStore>,
    segment_cache: Arc<SegmentCache>,
}

impl PipelineConsumeLoop {
    pub fn new(
        config: PipelineConsumeLoopConfig,
        sink: Box<dyn SinkConnector>,
        transform: Option<TransformFn>,
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn object_store::ObjectStore>,
        segment_cache: Arc<SegmentCache>,
    ) -> Self {
        Self {
            config,
            sink,
            transform,
            metadata,
            object_store,
            segment_cache,
        }
    }

    /// Run the consume loop until shutdown is signaled.
    pub async fn run(mut self, shutdown_rx: oneshot::Receiver<()>) {
        // Start the sink connector
        if let Err(e) = self.sink.start().await {
            tracing::error!(
                pipeline = %self.config.pipeline_name,
                error = %e,
                "Failed to start sink connector"
            );
            return;
        }

        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut shutdown_rx = shutdown_rx;
        let mut total_records: i64 = 0;
        let mut last_offset: i64;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.poll_and_sink().await {
                        Ok((count, offset)) => {
                            if count > 0 {
                                total_records += count as i64;
                                last_offset = offset;
                                // Update progress in metadata
                                let _ = self.metadata.update_pipeline_progress_for_org(
                                    &self.config.org_id,
                                    &self.config.pipeline_name,
                                    total_records,
                                    last_offset,
                                ).await;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                pipeline = %self.config.pipeline_name,
                                error = %e,
                                "Pipeline poll error"
                            );
                            // Update state to error
                            let _ = self.metadata.update_pipeline_state_for_org(
                                &self.config.org_id,
                                &self.config.pipeline_name,
                                "error",
                                Some(&e.to_string()),
                            ).await;
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    tracing::info!(
                        pipeline = %self.config.pipeline_name,
                        "Pipeline consume loop shutting down"
                    );
                    break;
                }
            }
        }

        // Stop the sink
        if let Err(e) = self.sink.stop().await {
            tracing::warn!(
                pipeline = %self.config.pipeline_name,
                error = %e,
                "Error stopping sink connector"
            );
        }
    }

    /// Poll each partition and sink records. Returns (records_count, max_offset).
    async fn poll_and_sink(
        &mut self,
    ) -> Result<(usize, i64), Box<dyn std::error::Error + Send + Sync>> {
        let mut all_records = Vec::new();
        let mut max_offset: i64 = -1;

        for partition_id in 0..self.config.partition_count {
            // Get committed offset for this partition
            let committed = self
                .metadata
                .get_committed_offset(
                    &self.config.consumer_group,
                    &self.config.source_topic,
                    partition_id,
                )
                .await?;
            let start_offset = committed.map(|o| o + 1).unwrap_or(0);

            let reader = PartitionReader::new(
                self.config.org_id.clone(),
                self.config.source_topic.clone(),
                partition_id,
                self.metadata.clone(),
                self.object_store.clone(),
                self.segment_cache.clone(),
            );

            let result = match reader
                .read(start_offset, self.config.max_records_per_poll)
                .await
            {
                Ok(result) => result,
                Err(streamhouse_storage::Error::OffsetNotFound(_)) => continue,
                Err(e) => return Err(Box::new(e)),
            };

            for record in &result.records {
                let sink_record = SinkRecord {
                    topic: self.config.source_topic.clone(),
                    partition: partition_id,
                    offset: record.offset,
                    timestamp: record.timestamp,
                    key: record.key.clone(),
                    value: record.value.clone(),
                };
                all_records.push(sink_record);

                let offset_i64 = record.offset as i64;
                if offset_i64 > max_offset {
                    max_offset = offset_i64;
                }
            }

            // Commit offset for this partition
            if let Some(last_record) = result.records.last() {
                self.metadata
                    .commit_offset(
                        &self.config.consumer_group,
                        &self.config.source_topic,
                        partition_id,
                        last_record.offset,
                        None,
                    )
                    .await?;
            }
        }

        if all_records.is_empty() {
            return Ok((0, -1));
        }

        // Apply transform if configured
        let all_records = if let Some(ref transform) = self.transform {
            transform(all_records).await?
        } else {
            all_records
        };

        if all_records.is_empty() {
            return Ok((0, -1));
        }

        // Sink the records
        self.sink
            .put(&all_records)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        self.sink
            .flush()
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

        let count = all_records.len();
        tracing::debug!(
            pipeline = %self.config.pipeline_name,
            records = count,
            "Sunk records"
        );

        Ok((count, max_offset))
    }
}
