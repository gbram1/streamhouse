//! HTTP Webhook to StreamHouse Integration
//!
//! This example shows how to receive webhooks from external services
//! (like Stripe, GitHub, Slack) and forward them to StreamHouse.
//!
//! ## Usage
//!
//! ```bash
//! # Start StreamHouse
//! docker compose up -d
//!
//! # Create the webhooks topic
//! curl -X POST http://localhost:8080/api/v1/topics \
//!   -H "Content-Type: application/json" \
//!   -d '{"name": "webhooks", "partitions": 3}'
//!
//! # Start this webhook server
//! cargo run --release
//!
//! # Send a test webhook
//! curl -X POST http://localhost:3030/webhook \
//!   -H "Content-Type: application/json" \
//!   -H "X-Webhook-Source: stripe" \
//!   -d '{"event": "payment.completed", "data": {"id": "pay_123", "amount": 9999}}'
//! ```

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{error, info};

const STREAMHOUSE_URL: &str = "http://localhost:8080";
const WEBHOOK_TOPIC: &str = "webhooks";

#[derive(Clone)]
struct AppState {
    http_client: Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct WebhookEvent {
    source: String,
    event_type: String,
    timestamp: i64,
    payload: Value,
}

#[derive(Debug, Serialize)]
struct ProduceRequest {
    topic: String,
    key: String,
    value: String,
}

async fn handle_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    // Extract webhook source from header (or default to "unknown")
    let source = headers
        .get("X-Webhook-Source")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Extract event type from payload
    let event_type = payload
        .get("event")
        .or_else(|| payload.get("type"))
        .or_else(|| payload.get("event_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Create structured webhook event
    let webhook_event = WebhookEvent {
        source: source.clone(),
        event_type: event_type.clone(),
        timestamp: chrono_timestamp(),
        payload,
    };

    // Use source as the key for partitioning (events from same source go to same partition)
    let key = format!("{}:{}", source, event_type);

    info!(
        source = %source,
        event_type = %event_type,
        "Received webhook"
    );

    // Forward to StreamHouse
    let produce_request = json!({
        "topic": WEBHOOK_TOPIC,
        "key": key,
        "value": serde_json::to_string(&webhook_event).unwrap()
    });

    match state
        .http_client
        .post(format!("{}/api/v1/produce", STREAMHOUSE_URL))
        .json(&produce_request)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            info!("Webhook forwarded to StreamHouse");
            Ok(StatusCode::OK)
        }
        Ok(response) => {
            error!(
                status = %response.status(),
                "Failed to forward webhook to StreamHouse"
            );
            Err(StatusCode::BAD_GATEWAY)
        }
        Err(e) => {
            error!(error = %e, "Failed to connect to StreamHouse");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

async fn health_check() -> &'static str {
    "OK"
}

fn chrono_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create HTTP client for StreamHouse
    let http_client = Client::builder()
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to create HTTP client");

    let state = Arc::new(AppState { http_client });

    // Build router
    let app = Router::new()
        .route("/webhook", post(handle_webhook))
        .route("/health", axum::routing::get(health_check))
        .with_state(state);

    let addr = "0.0.0.0:3030";
    info!("Starting webhook server on {}", addr);
    info!("Forwarding webhooks to {} topic '{}'", STREAMHOUSE_URL, WEBHOOK_TOPIC);
    info!("");
    info!("Test with:");
    info!("  curl -X POST http://localhost:3030/webhook \\");
    info!("    -H 'Content-Type: application/json' \\");
    info!("    -H 'X-Webhook-Source: stripe' \\");
    info!("    -d '{{\"event\": \"payment.completed\", \"amount\": 9999}}'");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
