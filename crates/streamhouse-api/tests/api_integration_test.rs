//! Integration tests for the StreamHouse REST API
//!
//! Tests the HTTP endpoints by creating a real router with in-memory
//! metadata store and object store, then sending requests via tower::ServiceExt.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use streamhouse_api::auth::AuthConfig;
use streamhouse_api::models::{CreateTopicRequest, Topic};
use streamhouse_api::{create_router, AppState};
use streamhouse_metadata::SqliteMetadataStore;

/// Create a test app with in-memory stores and auth disabled
async fn test_app() -> axum::Router {
    let metadata = Arc::new(SqliteMetadataStore::new_in_memory().await.unwrap())
        as Arc<dyn streamhouse_metadata::MetadataStore>;

    let object_store =
        Arc::new(object_store::memory::InMemory::new()) as Arc<dyn object_store::ObjectStore>;

    let temp_dir = tempfile::tempdir().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let segment_cache = Arc::new(
        streamhouse_storage::SegmentCache::new(&cache_dir, 10 * 1024 * 1024).unwrap(),
    );

    let state = AppState {
        metadata,
        producer: None,
        writer_pool: None,
        object_store,
        segment_cache,
        prometheus: None,
        auth_config: AuthConfig {
            enabled: false,
            ..Default::default()
        },
    };

    create_router(state)
}

/// Helper to read response body as bytes
async fn body_bytes(body: Body) -> Vec<u8> {
    body.collect().await.unwrap().to_bytes().to_vec()
}

// ---------------------------------------------------------------
// Health endpoints
// ---------------------------------------------------------------

#[tokio::test]
async fn test_health_check() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_liveness_check() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_readiness_check() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------
// Topic CRUD
// ---------------------------------------------------------------

#[tokio::test]
async fn test_list_topics_empty() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/topics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp.into_body()).await;
    let topics: Vec<Topic> = serde_json::from_slice(&body).unwrap();
    assert!(topics.is_empty());
}

#[tokio::test]
async fn test_create_topic() {
    let app = test_app().await;

    let req_body = serde_json::to_string(&CreateTopicRequest {
        name: "test-orders".to_string(),
        partitions: 4,
        replication_factor: 1,
    })
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/topics")
                .header("content-type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = body_bytes(resp.into_body()).await;
    let topic: Topic = serde_json::from_slice(&body).unwrap();
    assert_eq!(topic.name, "test-orders");
    assert_eq!(topic.partitions, 4);
}

#[tokio::test]
async fn test_create_and_get_topic() {
    let app = test_app().await;

    // Create
    let req_body = serde_json::to_string(&CreateTopicRequest {
        name: "events".to_string(),
        partitions: 2,
        replication_factor: 1,
    })
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/topics")
                .header("content-type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Get
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/topics/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp.into_body()).await;
    let topic: Topic = serde_json::from_slice(&body).unwrap();
    assert_eq!(topic.name, "events");
    assert_eq!(topic.partitions, 2);
}

#[tokio::test]
async fn test_get_topic_not_found() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/topics/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_and_delete_topic() {
    let app = test_app().await;

    // Create
    let req_body = serde_json::to_string(&CreateTopicRequest {
        name: "to-delete".to_string(),
        partitions: 1,
        replication_factor: 1,
    })
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/topics")
                .header("content-type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Delete
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/topics/to-delete")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status() == StatusCode::OK || resp.status() == StatusCode::NO_CONTENT,
        "Expected 200 or 204, got {}",
        resp.status()
    );

    // Verify it's gone
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/topics/to-delete")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_topic_duplicate() {
    let app = test_app().await;

    let req_body = serde_json::to_string(&CreateTopicRequest {
        name: "dup-topic".to_string(),
        partitions: 2,
        replication_factor: 1,
    })
    .unwrap();

    // First create — should succeed
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/topics")
                .header("content-type", "application/json")
                .body(Body::from(req_body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Second create — should fail with conflict
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/topics")
                .header("content-type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status() == StatusCode::CONFLICT || resp.status() == StatusCode::BAD_REQUEST,
        "Expected 409 or 400 for duplicate topic, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_list_topics_after_create() {
    let app = test_app().await;

    // Create two topics
    for name in &["alpha", "beta"] {
        let req_body = serde_json::to_string(&CreateTopicRequest {
            name: name.to_string(),
            partitions: 1,
            replication_factor: 1,
        })
        .unwrap();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/topics")
                    .header("content-type", "application/json")
                    .body(Body::from(req_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // List
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/topics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp.into_body()).await;
    let topics: Vec<Topic> = serde_json::from_slice(&body).unwrap();
    assert_eq!(topics.len(), 2);
}

// ---------------------------------------------------------------
// Consumer Groups (empty state)
// ---------------------------------------------------------------

#[tokio::test]
async fn test_list_consumer_groups_empty() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/consumer-groups")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp.into_body()).await;
    let groups: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(groups.is_empty());
}

// ---------------------------------------------------------------
// Invalid requests
// ---------------------------------------------------------------

#[tokio::test]
async fn test_create_topic_invalid_json() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/topics")
                .header("content-type", "application/json")
                .body(Body::from("not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        resp.status().is_client_error(),
        "Expected 4xx for invalid JSON, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_unknown_route_returns_404() {
    let app = test_app().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
