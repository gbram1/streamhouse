//! Pipeline Background Maintenance
//!
//! Starts and stops pipeline consume loops based on pipeline metadata state.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use streamhouse_connectors::pipeline::{
    PipelineConsumeLoop, PipelineConsumeLoopConfig, TransformFn,
};
use streamhouse_connectors::sinks::clickhouse::ClickHouseSinkConnector;
use streamhouse_connectors::sinks::elasticsearch::ElasticsearchSinkConnector;
#[cfg(feature = "postgres")]
use streamhouse_connectors::sinks::postgres::PostgresSinkConnector;
use streamhouse_connectors::sinks::s3::S3SinkConnector;
use streamhouse_connectors::traits::SinkConnector;
use streamhouse_metadata::MetadataStore;
use streamhouse_sql::transform::TransformEngine;
use streamhouse_storage::SegmentCache;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub struct PipelineMaintenance {
    metadata: Arc<dyn MetadataStore>,
    object_store: Arc<dyn object_store::ObjectStore>,
    segment_cache: Arc<SegmentCache>,
}

struct RunningPipeline {
    handle: JoinHandle<()>,
    shutdown_tx: oneshot::Sender<()>,
}

impl PipelineMaintenance {
    pub fn new(
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn object_store::ObjectStore>,
        segment_cache: Arc<SegmentCache>,
    ) -> Self {
        Self {
            metadata,
            object_store,
            segment_cache,
        }
    }

    /// Start the background maintenance task.
    pub fn start(self: Arc<Self>, shutdown_rx: oneshot::Receiver<()>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut shutdown_rx = shutdown_rx;
            let mut running: HashMap<String, RunningPipeline> = HashMap::new();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = self.reconcile(&mut running).await {
                            tracing::error!(error = %e, "Pipeline maintenance tick failed");
                        }
                    }
                    _ = &mut shutdown_rx => {
                        tracing::info!("Pipeline maintenance shutting down");
                        // Send shutdown to all running pipelines
                        for (name, rp) in running.drain() {
                            tracing::info!(pipeline = %name, "Stopping pipeline");
                            let _ = rp.shutdown_tx.send(());
                            let _ = rp.handle.await;
                        }
                        break;
                    }
                }
            }
        })
    }

    /// Reconcile running pipelines with desired state from metadata.
    async fn reconcile(
        &self,
        running: &mut HashMap<String, RunningPipeline>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // List all pipelines across all organizations
        let mut pipelines = Vec::new();
        if let Ok(orgs) = self.metadata.list_organizations().await {
            for org in orgs {
                if let Ok(org_pipelines) = self.metadata.list_pipelines_for_org(&org.id).await {
                    pipelines.extend(org_pipelines);
                }
            }
        }

        // Determine which should be running
        let desired_running: HashMap<String, _> = pipelines
            .iter()
            .filter(|p| p.state == "running")
            .map(|p| (p.id.clone(), p))
            .collect();

        // Stop pipelines that should no longer be running
        let to_stop: Vec<String> = running
            .keys()
            .filter(|id| !desired_running.contains_key(*id))
            .cloned()
            .collect();
        for id in to_stop {
            if let Some(rp) = running.remove(&id) {
                tracing::info!(pipeline_id = %id, "Stopping pipeline");
                let _ = rp.shutdown_tx.send(());
                let _ = rp.handle.await;
            }
        }

        // Start pipelines that should be running but aren't
        for (id, pipeline) in &desired_running {
            if running.contains_key(id.as_str()) {
                continue;
            }

            // Look up the target
            let target = match self
                .metadata
                .get_pipeline_target_by_id_for_org(&pipeline.organization_id, &pipeline.target_id)
                .await?
            {
                Some(t) => t,
                None => {
                    tracing::warn!(pipeline = %pipeline.name, target_id = %pipeline.target_id, "Target not found");
                    continue;
                }
            };

            // Look up the source topic to get partition count
            let topic = match self
                .metadata
                .get_topic_for_org(&pipeline.organization_id, &pipeline.source_topic)
                .await?
            {
                Some(t) => t,
                None => {
                    tracing::warn!(pipeline = %pipeline.name, topic = %pipeline.source_topic, "Source topic not found");
                    continue;
                }
            };

            // Create the appropriate sink connector based on target_type
            let sink: Box<dyn SinkConnector> = match target.target_type.as_str() {
                "clickhouse" => {
                    match ClickHouseSinkConnector::new(&pipeline.name, &target.connection_config) {
                        Ok(c) => Box::new(c),
                        Err(e) => {
                            tracing::warn!(pipeline = %pipeline.name, error = %e, "Failed to create ClickHouse sink");
                            continue;
                        }
                    }
                }
                "elasticsearch" => {
                    match ElasticsearchSinkConnector::new(&pipeline.name, &target.connection_config)
                    {
                        Ok(c) => Box::new(c),
                        Err(e) => {
                            tracing::warn!(pipeline = %pipeline.name, error = %e, "Failed to create Elasticsearch sink");
                            continue;
                        }
                    }
                }
                "s3" => match S3SinkConnector::new(&pipeline.name, &target.connection_config) {
                    Ok(c) => Box::new(c),
                    Err(e) => {
                        tracing::warn!(pipeline = %pipeline.name, error = %e, "Failed to create S3 sink");
                        continue;
                    }
                },
                #[cfg(feature = "postgres")]
                "postgres" => {
                    match PostgresSinkConnector::new(&pipeline.name, &target.connection_config) {
                        Ok(c) => Box::new(c),
                        Err(e) => {
                            tracing::warn!(pipeline = %pipeline.name, error = %e, "Failed to create Postgres sink");
                            continue;
                        }
                    }
                }
                other => {
                    tracing::warn!(pipeline = %pipeline.name, target_type = %other, "Unsupported target type");
                    continue;
                }
            };

            let config = PipelineConsumeLoopConfig {
                org_id: pipeline.organization_id.clone(),
                pipeline_name: pipeline.name.clone(),
                source_topic: pipeline.source_topic.clone(),
                partition_count: topic.partition_count,
                consumer_group: pipeline.consumer_group.clone(),
                max_records_per_poll: 1000,
            };

            // Build the transform function if SQL is configured
            let transform: Option<TransformFn> = pipeline.transform_sql.as_ref().map(|sql| {
                let sql = sql.clone();
                Box::new(move |records: Vec<streamhouse_connectors::SinkRecord>| {
                    let sql = sql.clone();
                    Box::pin(async move {
                        TransformEngine::apply_transform(&records, &sql)
                            .await
                            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                                Box::new(e)
                            })
                    })
                        as std::pin::Pin<
                            Box<
                                dyn std::future::Future<
                                        Output = std::result::Result<
                                            Vec<streamhouse_connectors::SinkRecord>,
                                            Box<dyn std::error::Error + Send + Sync>,
                                        >,
                                    > + Send,
                            >,
                        >
                }) as TransformFn
            });

            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let consume_loop = PipelineConsumeLoop::new(
                config,
                sink,
                transform,
                self.metadata.clone(),
                self.object_store.clone(),
                self.segment_cache.clone(),
            );

            let handle = tokio::spawn(async move {
                consume_loop.run(shutdown_rx).await;
            });

            tracing::info!(pipeline = %pipeline.name, pipeline_id = %id, "Started pipeline consume loop");
            running.insert(
                id.clone(),
                RunningPipeline {
                    handle,
                    shutdown_tx,
                },
            );
        }

        Ok(())
    }
}
