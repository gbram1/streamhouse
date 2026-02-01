//! WebSocket handlers for real-time metrics streaming

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::time::interval;

use crate::AppState;

/// Upgrade HTTP to WebSocket for real-time metrics
pub async fn metrics_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_metrics_stream(socket, state))
}

/// Stream cluster metrics every 5 seconds
async fn handle_metrics_stream(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Send metrics every 5 seconds
    let mut tick = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                // Collect current metrics
                let metrics = match collect_realtime_metrics(&state).await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to collect metrics: {}", e);
                        break;
                    }
                };

                // Send as JSON
                let msg = Message::Text(serde_json::to_string(&metrics).unwrap());
                if sender.send(msg).await.is_err() {
                    break; // Client disconnected
                }
            }

            // Handle client messages (ping/pong)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Collect current metrics snapshot
async fn collect_realtime_metrics(
    state: &AppState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let topics = state.metadata.list_topics().await?;
    let agents = state.metadata.list_agents(None, None).await?;

    // Calculate aggregates
    let partitions_count: u32 = topics.iter().map(|t| t.partition_count).sum();

    Ok(json!({
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "throughput": {
            "messagesPerSecond": 0,
            "bytesPerSecond": 0
        },
        "latency": {
            "p50": 0,
            "p95": 0,
            "p99": 0,
            "avg": 0
        },
        "errors": {
            "errorRate": 0.0,
            "errorCount": 0
        },
        "storage": {
            "totalSizeBytes": 0,
            "cacheHitRate": 0.0
        },
        "consumerLag": {
            "totalLag": 0,
            "groupsWithLag": 0
        },
        "counts": {
            "topics": topics.len(),
            "agents": agents.len(),
            "partitions": partitions_count
        }
    }))
}

/// Topic-specific metrics WebSocket
pub async fn topic_websocket(
    ws: WebSocketUpgrade,
    Path(topic_name): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_topic_stream(socket, state, topic_name))
}

async fn handle_topic_stream(socket: WebSocket, state: AppState, topic_name: String) {
    let (mut sender, mut receiver) = socket.split();
    let mut tick = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                // Get topic metrics
                let metrics = match collect_topic_metrics(&state, &topic_name).await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to collect topic metrics: {}", e);
                        break;
                    }
                };

                let msg = Message::Text(serde_json::to_string(&metrics).unwrap());
                if sender.send(msg).await.is_err() {
                    break;
                }
            }

            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn collect_topic_metrics(
    state: &AppState,
    topic_name: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let topic = state
        .metadata
        .get_topic(topic_name)
        .await?
        .ok_or("Topic not found")?;

    Ok(json!({
        "topic": topic_name,
        "throughput": 0,
        "lag": 0,
        "partitionCount": topic.partition_count
    }))
}

/// Consumer group metrics WebSocket
pub async fn consumer_websocket(
    ws: WebSocketUpgrade,
    Path(group_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_consumer_stream(socket, state, group_id))
}

async fn handle_consumer_stream(socket: WebSocket, state: AppState, group_id: String) {
    let (mut sender, mut receiver) = socket.split();
    let mut tick = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let metrics = match collect_consumer_metrics(&state, &group_id).await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to collect consumer metrics: {}", e);
                        break;
                    }
                };

                let msg = Message::Text(serde_json::to_string(&metrics).unwrap());
                if sender.send(msg).await.is_err() {
                    break;
                }
            }

            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn collect_consumer_metrics(
    state: &AppState,
    group_id: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let _offsets = state.metadata.get_consumer_offsets(group_id).await?;

    Ok(json!({
        "groupId": group_id,
        "totalLag": 0,
        "memberCount": 0
    }))
}
