use crate::pb::{stream_house_server::StreamHouse, *};
use std::collections::HashMap;
use std::sync::Arc;
use streamhouse_metadata::{MetadataStore, TopicConfig};
use streamhouse_storage::{
    PartitionReader, PartitionWriter, SegmentCache, WriteConfig, WriterPool,
};
use tonic::{Request, Response, Status};

/// StreamHouse gRPC service implementation
pub struct StreamHouseService {
    metadata: Arc<dyn MetadataStore>,
    object_store: Arc<dyn object_store::ObjectStore>,
    cache: Arc<SegmentCache>,
    writer_pool: Arc<WriterPool>,
    #[allow(dead_code)]
    config: WriteConfig,
}

impl StreamHouseService {
    pub fn new(
        metadata: Arc<dyn MetadataStore>,
        object_store: Arc<dyn object_store::ObjectStore>,
        cache: Arc<SegmentCache>,
        writer_pool: Arc<WriterPool>,
        config: WriteConfig,
    ) -> Self {
        Self {
            metadata,
            object_store,
            cache,
            writer_pool,
            config,
        }
    }

    /// Get partition writer from pool
    async fn get_writer(
        &self,
        topic: &str,
        partition_id: u32,
    ) -> Result<Arc<tokio::sync::Mutex<PartitionWriter>>, Status> {
        // Verify topic exists
        let topic_info = self
            .metadata
            .get_topic(topic)
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Topic not found: {}", topic)))?;

        // Verify partition exists
        if partition_id >= topic_info.partition_count {
            return Err(Status::invalid_argument(format!(
                "Invalid partition: {} (topic has {} partitions)",
                partition_id, topic_info.partition_count
            )));
        }

        // Get writer from pool
        self.writer_pool
            .get_writer(topic, partition_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get writer: {}", e)))
    }

    /// Get a partition reader
    async fn get_reader(&self, topic: &str, partition_id: u32) -> Result<PartitionReader, Status> {
        // Verify topic exists
        let topic_info = self
            .metadata
            .get_topic(topic)
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Topic not found: {}", topic)))?;

        // Verify partition exists
        if partition_id >= topic_info.partition_count {
            return Err(Status::invalid_argument(format!(
                "Invalid partition: {} (topic has {} partitions)",
                partition_id, topic_info.partition_count
            )));
        }

        // Create reader
        let reader = PartitionReader::new(
            topic.to_string(),
            partition_id,
            self.metadata.clone(),
            self.object_store.clone(),
            self.cache.clone(),
        );

        Ok(reader)
    }
}

#[tonic::async_trait]
impl StreamHouse for StreamHouseService {
    // ========================================================================
    // Admin Operations
    // ========================================================================

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().name, partitions = request.get_ref().partition_count))]
    async fn create_topic(
        &self,
        request: Request<CreateTopicRequest>,
    ) -> Result<Response<CreateTopicResponse>, Status> {
        let req = request.into_inner();

        let config = TopicConfig {
            name: req.name.clone(),
            partition_count: req.partition_count,
            retention_ms: req.retention_ms.map(|v| v as i64),
            cleanup_policy: Default::default(),
            config: req.config,
        };

        self.metadata
            .create_topic(config)
            .await
            .map_err(|e| Status::internal(format!("Failed to create topic: {}", e)))?;

        tracing::info!(
            topic = %req.name,
            partitions = req.partition_count,
            "Topic created"
        );

        Ok(Response::new(CreateTopicResponse {
            topic_id: req.name,
            partition_count: req.partition_count,
        }))
    }

    async fn get_topic(
        &self,
        request: Request<GetTopicRequest>,
    ) -> Result<Response<GetTopicResponse>, Status> {
        let req = request.into_inner();

        let topic = self
            .metadata
            .get_topic(&req.name)
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Topic not found: {}", req.name)))?;

        Ok(Response::new(GetTopicResponse {
            topic: Some(Topic {
                name: topic.name,
                partition_count: topic.partition_count,
                retention_ms: topic.retention_ms.map(|v| v as u64),
                config: topic.config,
                created_at: topic.created_at as u64,
            }),
        }))
    }

    #[tracing::instrument(skip(self, _request))]
    async fn list_topics(
        &self,
        _request: Request<ListTopicsRequest>,
    ) -> Result<Response<ListTopicsResponse>, Status> {
        let topics = self
            .metadata
            .list_topics()
            .await
            .map_err(|e| Status::internal(format!("Metadata error: {}", e)))?;

        let topic_list = topics
            .into_iter()
            .map(|t| Topic {
                name: t.name,
                partition_count: t.partition_count,
                retention_ms: t.retention_ms.map(|v| v as u64),
                config: t.config,
                created_at: t.created_at as u64,
            })
            .collect();

        Ok(Response::new(ListTopicsResponse { topics: topic_list }))
    }

    async fn delete_topic(
        &self,
        request: Request<DeleteTopicRequest>,
    ) -> Result<Response<DeleteTopicResponse>, Status> {
        let req = request.into_inner();

        self.metadata
            .delete_topic(&req.name)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete topic: {}", e)))?;

        tracing::info!(topic = %req.name, "Topic deleted");

        Ok(Response::new(DeleteTopicResponse { success: true }))
    }

    // ========================================================================
    // Producer Operations
    // ========================================================================

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().topic, partition = request.get_ref().partition, value_len = request.get_ref().value.len()))]
    async fn produce(
        &self,
        request: Request<ProduceRequest>,
    ) -> Result<Response<ProduceResponse>, Status> {
        let req = request.into_inner();

        let writer = self.get_writer(&req.topic, req.partition).await?;
        let mut writer_guard = writer.lock().await;

        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let key = if req.key.is_empty() {
            None
        } else {
            Some(bytes::Bytes::from(req.key))
        };

        let offset = writer_guard
            .append(key, bytes::Bytes::from(req.value), timestamp)
            .await
            .map_err(|e| Status::internal(format!("Write failed: {}", e)))?;

        Ok(Response::new(ProduceResponse { offset, timestamp }))
    }

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().topic, partition = request.get_ref().partition, record_count = request.get_ref().records.len()))]
    async fn produce_batch(
        &self,
        request: Request<ProduceBatchRequest>,
    ) -> Result<Response<ProduceBatchResponse>, Status> {
        let req = request.into_inner();

        let writer = self.get_writer(&req.topic, req.partition).await?;
        let mut writer_guard = writer.lock().await;

        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let mut first_offset = None;
        let mut last_offset = 0;

        for proto_record in req.records {
            let key = if proto_record.key.is_empty() {
                None
            } else {
                Some(bytes::Bytes::from(proto_record.key))
            };

            let offset = writer_guard
                .append(key, bytes::Bytes::from(proto_record.value), timestamp)
                .await
                .map_err(|e| Status::internal(format!("Write failed: {}", e)))?;

            if first_offset.is_none() {
                first_offset = Some(offset);
            }
            last_offset = offset;
        }

        let count = if let Some(first) = first_offset {
            (last_offset - first + 1) as u32
        } else {
            0
        };

        Ok(Response::new(ProduceBatchResponse {
            first_offset: first_offset.unwrap_or(0),
            last_offset,
            count,
        }))
    }

    // ========================================================================
    // Consumer Operations
    // ========================================================================

    #[tracing::instrument(skip(self, request), fields(topic = %request.get_ref().topic, partition = request.get_ref().partition, offset = request.get_ref().offset, max_records = request.get_ref().max_records))]
    async fn consume(
        &self,
        request: Request<ConsumeRequest>,
    ) -> Result<Response<ConsumeResponse>, Status> {
        let req = request.into_inner();

        let reader = self.get_reader(&req.topic, req.partition).await?;

        let result = reader
            .read(req.offset, req.max_records as usize)
            .await
            .map_err(|e| Status::internal(format!("Read failed: {}", e)))?;

        let records = result
            .records
            .into_iter()
            .map(|r| ConsumedRecord {
                offset: r.offset,
                timestamp: r.timestamp,
                key: r.key.map(|k| k.to_vec()).unwrap_or_default(),
                value: r.value.to_vec(),
                headers: HashMap::new(), // Headers not supported yet
            })
            .collect::<Vec<_>>();

        let has_more =
            !records.is_empty() && records.last().unwrap().offset + 1 < result.high_watermark;

        Ok(Response::new(ConsumeResponse {
            records,
            high_watermark: result.high_watermark,
            has_more,
        }))
    }

    async fn commit_offset(
        &self,
        request: Request<CommitOffsetRequest>,
    ) -> Result<Response<CommitOffsetResponse>, Status> {
        let req = request.into_inner();

        self.metadata
            .commit_offset(
                &req.consumer_group,
                &req.topic,
                req.partition,
                req.offset,
                req.metadata,
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to commit offset: {}", e)))?;

        Ok(Response::new(CommitOffsetResponse { success: true }))
    }

    async fn get_offset(
        &self,
        request: Request<GetOffsetRequest>,
    ) -> Result<Response<GetOffsetResponse>, Status> {
        let req = request.into_inner();

        let offset = self
            .metadata
            .get_committed_offset(&req.consumer_group, &req.topic, req.partition)
            .await
            .map_err(|e| Status::internal(format!("Failed to get offset: {}", e)))?
            .unwrap_or(0);

        Ok(Response::new(GetOffsetResponse {
            offset,
            metadata: None, // TODO: Store and retrieve metadata
        }))
    }
}
