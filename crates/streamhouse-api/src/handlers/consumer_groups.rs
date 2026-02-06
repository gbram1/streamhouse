//! Consumer group endpoints

use axum::{extract::State, http::StatusCode, Json};
use std::collections::HashSet;

use crate::models::{
    CommitOffsetRequest, CommitOffsetResponse, ConsumerGroupDetail, ConsumerGroupInfo,
    ConsumerGroupLag, ConsumerOffsetInfo, DeleteConsumerGroupResponse, ResetOffsetDetail,
    ResetOffsetsRequest, ResetOffsetsResponse, ResetStrategy, SeekOffsetDetail,
    SeekToTimestampRequest, SeekToTimestampResponse,
};
use crate::AppState;

#[utoipa::path(
    get,
    path = "/api/v1/consumer-groups",
    responses(
        (status = 200, description = "List all consumer groups", body = Vec<ConsumerGroupInfo>)
    ),
    tag = "consumer-groups"
)]
pub async fn list_consumer_groups(
    State(state): State<AppState>,
) -> Result<Json<Vec<ConsumerGroupInfo>>, StatusCode> {
    // Get all consumer group IDs
    let group_ids = state
        .metadata
        .list_consumer_groups()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Collect information for each group
    let mut groups = Vec::new();

    for group_id in group_ids {
        // Get consumer offsets for this group
        let offsets = state
            .metadata
            .get_consumer_offsets(&group_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if offsets.is_empty() {
            continue;
        }

        let partition_count = offsets.len();

        // Collect topics and calculate total lag
        let mut topics_set: HashSet<String> = HashSet::new();
        let mut total_lag = 0i64;

        for offset in offsets {
            topics_set.insert(offset.topic.clone());

            // Get partition to calculate lag
            if let Ok(Some(partition)) = state
                .metadata
                .get_partition(&offset.topic, offset.partition_id)
                .await
            {
                let lag = partition.high_watermark as i64 - offset.committed_offset as i64;
                total_lag += lag;
            }
        }

        groups.push(ConsumerGroupInfo {
            group_id,
            topics: topics_set.into_iter().collect(),
            total_lag,
            partition_count,
        });
    }

    Ok(Json(groups))
}

#[utoipa::path(
    get,
    path = "/api/v1/consumer-groups/{group_id}",
    params(
        ("group_id" = String, Path, description = "Consumer group ID")
    ),
    responses(
        (status = 200, description = "Consumer group details", body = ConsumerGroupDetail),
        (status = 404, description = "Consumer group not found")
    ),
    tag = "consumer-groups"
)]
pub async fn get_consumer_group(
    State(state): State<AppState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> Result<Json<ConsumerGroupDetail>, StatusCode> {
    // Get consumer offsets for this group
    let consumer_offsets = state
        .metadata
        .get_consumer_offsets(&group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if consumer_offsets.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // For each offset, calculate lag
    let mut offset_infos = Vec::new();

    for offset in consumer_offsets {
        // Get partition metadata to find high watermark
        let partition = state
            .metadata
            .get_partition(&offset.topic, offset.partition_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        let lag = partition.high_watermark as i64 - offset.committed_offset as i64;

        offset_infos.push(ConsumerOffsetInfo {
            topic: offset.topic,
            partition_id: offset.partition_id,
            committed_offset: offset.committed_offset,
            high_watermark: partition.high_watermark,
            lag,
        });
    }

    Ok(Json(ConsumerGroupDetail {
        group_id,
        offsets: offset_infos,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/consumer-groups/{group_id}/lag",
    params(
        ("group_id" = String, Path, description = "Consumer group ID")
    ),
    responses(
        (status = 200, description = "Consumer group lag summary", body = ConsumerGroupLag),
        (status = 404, description = "Consumer group not found")
    ),
    tag = "consumer-groups"
)]
pub async fn get_consumer_group_lag(
    State(state): State<AppState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> Result<Json<ConsumerGroupLag>, StatusCode> {
    // Get consumer offsets for this group
    let consumer_offsets = state
        .metadata
        .get_consumer_offsets(&group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if consumer_offsets.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut total_lag = 0i64;
    let mut topics: HashSet<String> = HashSet::new();
    let partition_count = consumer_offsets.len();

    for offset in consumer_offsets {
        topics.insert(offset.topic.clone());

        // Get partition metadata to calculate lag
        if let Ok(Some(partition)) = state
            .metadata
            .get_partition(&offset.topic, offset.partition_id)
            .await
        {
            let lag = partition.high_watermark as i64 - offset.committed_offset as i64;
            total_lag += lag;
        }
    }

    Ok(Json(ConsumerGroupLag {
        group_id,
        total_lag,
        partition_count,
        topics: topics.into_iter().collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/consumer-groups/commit",
    request_body = CommitOffsetRequest,
    responses(
        (status = 200, description = "Offset committed successfully", body = CommitOffsetResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Topic or partition not found")
    ),
    tag = "consumer-groups"
)]
pub async fn commit_offset(
    State(state): State<AppState>,
    Json(req): Json<CommitOffsetRequest>,
) -> Result<Json<CommitOffsetResponse>, StatusCode> {
    // Validate topic exists
    state
        .metadata
        .get_topic(&req.topic)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Validate partition exists
    state
        .metadata
        .get_partition(&req.topic, req.partition)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Commit the offset
    state
        .metadata
        .commit_offset(&req.group_id, &req.topic, req.partition, req.offset, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(CommitOffsetResponse { success: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/consumer-groups/{group_id}/reset",
    params(
        ("group_id" = String, Path, description = "Consumer group ID")
    ),
    request_body = ResetOffsetsRequest,
    responses(
        (status = 200, description = "Offsets reset successfully", body = ResetOffsetsResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Consumer group not found")
    ),
    tag = "consumer-groups"
)]
pub async fn reset_offsets(
    State(state): State<AppState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    Json(req): Json<ResetOffsetsRequest>,
) -> Result<Json<ResetOffsetsResponse>, StatusCode> {
    // Get current offsets for this consumer group
    let current_offsets = state
        .metadata
        .get_consumer_offsets(&group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if current_offsets.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Filter offsets by topic/partition if specified
    let offsets_to_reset: Vec<_> = current_offsets
        .iter()
        .filter(|o| {
            let topic_match = req.topic.as_ref().map_or(true, |t| &o.topic == t);
            let partition_match = req.partition.map_or(true, |p| o.partition_id == p);
            topic_match && partition_match
        })
        .collect();

    if offsets_to_reset.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut details = Vec::new();

    for offset in offsets_to_reset {
        // Calculate new offset based on strategy
        let new_offset = match req.strategy {
            ResetStrategy::Earliest => {
                // Reset to beginning (offset 0)
                // Note: In a full implementation, this could use the first segment's base_offset
                0
            }
            ResetStrategy::Latest => {
                // Get partition's high watermark
                let partition = state
                    .metadata
                    .get_partition(&offset.topic, offset.partition_id)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    .ok_or(StatusCode::NOT_FOUND)?;
                partition.high_watermark
            }
            ResetStrategy::Specific => req.offset.ok_or(StatusCode::BAD_REQUEST)?,
            ResetStrategy::Timestamp => {
                // For timestamp, we need to find the offset at or after the timestamp
                // This requires scanning segments - for now, use a simplified approach
                let timestamp_ms = req.timestamp.ok_or(StatusCode::BAD_REQUEST)?;
                find_offset_for_timestamp(&state, &offset.topic, offset.partition_id, timestamp_ms)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            }
        };

        // Commit the new offset
        state
            .metadata
            .commit_offset(
                &group_id,
                &offset.topic,
                offset.partition_id,
                new_offset,
                None,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        details.push(ResetOffsetDetail {
            topic: offset.topic.clone(),
            partition: offset.partition_id,
            old_offset: offset.committed_offset,
            new_offset,
        });
    }

    Ok(Json(ResetOffsetsResponse {
        success: true,
        partitions_reset: details.len(),
        details,
    }))
}

/// Find the offset at or after a given timestamp
async fn find_offset_for_timestamp(
    state: &AppState,
    topic: &str,
    partition_id: u32,
    timestamp_ms: i64,
) -> Result<u64, StatusCode> {
    // Get segments for this partition
    let segments = state
        .metadata
        .get_segments(topic, partition_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if segments.is_empty() {
        // No segments, return 0
        return Ok(0);
    }

    // Find the first segment that might contain our timestamp
    // Segments are ordered by base_offset
    // We use created_at as an approximation for the segment's timestamp range
    for segment in &segments {
        // If segment was created at or after our target timestamp,
        // the offset is at or before this segment's base_offset
        if segment.created_at >= timestamp_ms {
            return Ok(segment.base_offset);
        }
    }

    // If no segment found after our timestamp, return the end of the last segment
    // This means all segments are before our timestamp
    if let Some(last_segment) = segments.last() {
        Ok(last_segment.end_offset + 1)
    } else {
        Ok(0)
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/consumer-groups/{group_id}/seek",
    params(
        ("group_id" = String, Path, description = "Consumer group ID")
    ),
    request_body = SeekToTimestampRequest,
    responses(
        (status = 200, description = "Seek successful", body = SeekToTimestampResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Consumer group or topic not found")
    ),
    tag = "consumer-groups"
)]
pub async fn seek_to_timestamp(
    State(state): State<AppState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    Json(req): Json<SeekToTimestampRequest>,
) -> Result<Json<SeekToTimestampResponse>, StatusCode> {
    // Validate topic exists
    let topic = state
        .metadata
        .get_topic(&req.topic)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Get current offsets
    let current_offsets = state
        .metadata
        .get_consumer_offsets(&group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Filter to the specified topic and partition
    let offsets_to_update: Vec<_> = current_offsets
        .iter()
        .filter(|o| o.topic == req.topic && req.partition.map_or(true, |p| o.partition_id == p))
        .collect();

    // If no existing offsets for this topic, create them for all partitions
    let partitions_to_seek: Vec<u32> = if offsets_to_update.is_empty() {
        if let Some(p) = req.partition {
            vec![p]
        } else {
            (0..topic.partition_count).collect()
        }
    } else {
        offsets_to_update.iter().map(|o| o.partition_id).collect()
    };

    let mut details = Vec::new();

    for partition_id in partitions_to_seek {
        let old_offset = offsets_to_update
            .iter()
            .find(|o| o.partition_id == partition_id)
            .map(|o| o.committed_offset)
            .unwrap_or(0);

        // Find offset for timestamp
        let new_offset = find_offset_for_timestamp(&state, &req.topic, partition_id, req.timestamp)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Commit the new offset
        state
            .metadata
            .commit_offset(&group_id, &req.topic, partition_id, new_offset, None)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        details.push(SeekOffsetDetail {
            topic: req.topic.clone(),
            partition: partition_id,
            old_offset,
            new_offset,
            timestamp_found: req.timestamp, // Approximate
        });
    }

    Ok(Json(SeekToTimestampResponse {
        success: true,
        partitions_updated: details.len(),
        details,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/consumer-groups/{group_id}",
    params(
        ("group_id" = String, Path, description = "Consumer group ID")
    ),
    responses(
        (status = 200, description = "Consumer group deleted", body = DeleteConsumerGroupResponse),
        (status = 404, description = "Consumer group not found")
    ),
    tag = "consumer-groups"
)]
pub async fn delete_consumer_group(
    State(state): State<AppState>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> Result<Json<DeleteConsumerGroupResponse>, StatusCode> {
    // Get current offsets to count partitions
    let current_offsets = state
        .metadata
        .get_consumer_offsets(&group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if current_offsets.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let partitions_count = current_offsets.len();

    // Delete the consumer group
    state
        .metadata
        .delete_consumer_group(&group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DeleteConsumerGroupResponse {
        success: true,
        group_id,
        partitions_deleted: partitions_count,
    }))
}
