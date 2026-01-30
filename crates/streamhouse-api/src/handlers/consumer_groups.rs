//! Consumer group endpoints

use axum::{extract::State, http::StatusCode, Json};
use std::collections::HashSet;

use crate::models::{ConsumerGroupDetail, ConsumerGroupInfo, ConsumerGroupLag, ConsumerOffsetInfo};
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
