# Phase 10: REST API + Web Console - KICKOFF

## Status Update

**Web Console**: ✅ **COMPLETE** (Foundation ready)
**REST API Backend**: 🎯 **NEXT** (Week 11-12, ~800 LOC)

## What Just Happened

We've successfully created the **web console foundation** ahead of schedule to get the UI designed and ready for Phase 10's REST API backend.

### Completed (Just Now)
- ✅ Next.js 15 app with TypeScript
- ✅ shadcn/ui component library (12 components)
- ✅ Landing page with hero and features
- ✅ Dashboard with real-time metrics (mock data)
- ✅ TypeScript API client with full type definitions
- ✅ Responsive design with dark mode
- ✅ Production build verified
- ✅ Documentation complete

**Total**: ~2100 LOC including UI components
**Files**: [WEB_CONSOLE_FOUNDATION.md](WEB_CONSOLE_FOUNDATION.md)

## What's Next: REST API Backend

### Overview
Create a Rust-based REST API server to power the web console. This will expose StreamHouse metadata and operations via HTTP/JSON instead of gRPC.

### Timeline
**Phase 10, Week 11-12** (2 weeks)
- Estimated: ~800 LOC
- Technology: Axum 0.7 (Rust HTTP framework)
- Integration: MetadataStore, ConnectionPool, Producer

### Architecture

```
┌─────────────────────────────────────────────────────┐
│                Web Console (Next.js)                │
│                 Port 3000                           │
└───────────────────┬─────────────────────────────────┘
                    │ HTTP/JSON
                    │
┌───────────────────▼─────────────────────────────────┐
│          REST API Server (Axum)                     │
│               Port 3001                             │
│                                                     │
│  Routes:                                            │
│  ┌─────────────────────────────────────────────┐  │
│  │ GET  /api/v1/topics                         │  │
│  │ POST /api/v1/topics                         │  │
│  │ GET  /api/v1/topics/:name                   │  │
│  │ GET  /api/v1/topics/:name/partitions        │  │
│  │ GET  /api/v1/agents                         │  │
│  │ GET  /api/v1/consumer-groups                │  │
│  │ POST /api/v1/produce                        │  │
│  │ GET  /api/v1/metrics                        │  │
│  │ GET  /health                                │  │
│  └─────────────────────────────────────────────┘  │
└───────────────────┬─────────────────────────────────┘
                    │
    ┌───────────────┼───────────────┐
    │               │               │
    ▼               ▼               ▼
┌─────────┐  ┌──────────────┐  ┌──────────────┐
│Metadata │  │ ConnectionPool│  │  Producer    │
│  Store  │  │  (gRPC)       │  │   Client     │
└─────────┘  └──────────────┘  └──────────────┘
```

### Implementation Steps

#### Step 1: Create API Crate (~50 LOC)

```bash
cargo new --lib crates/streamhouse-api
```

**`crates/streamhouse-api/Cargo.toml`**:
```toml
[package]
name = "streamhouse-api"
version = "0.1.0"
edition = "2021"

[dependencies]
# HTTP framework
axum = "0.7"
tokio = { version = "1.0", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# OpenAPI docs
utoipa = { version = "4.0", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "4.0", features = ["axum"] }

# Logging
tracing = "0.1"

# StreamHouse dependencies
streamhouse-metadata = { path = "../streamhouse-metadata" }
streamhouse-client = { path = "../streamhouse-client" }
```

#### Step 2: Define API Models (~100 LOC)

**`crates/streamhouse-api/src/models.rs`**:
```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Topic {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub created_at: String,
    #[serde(flatten)]
    pub config: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTopicRequest {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Agent {
    pub agent_id: String,
    pub address: String,
    pub availability_zone: String,
    pub agent_group: String,
    pub last_heartbeat: i64,
    pub started_at: i64,
    pub active_leases: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Partition {
    pub topic: String,
    pub partition_id: u32,
    pub leader_agent_id: Option<String>,
    pub high_watermark: u64,
    pub low_watermark: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConsumerGroup {
    pub group_id: String,
    pub topic: String,
    pub members: u32,
    pub state: String,
    pub total_lag: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProduceRequest {
    pub topic: String,
    pub key: Option<String>,
    pub value: String,
    pub partition: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProduceResponse {
    pub offset: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MetricsSnapshot {
    pub topics_count: u64,
    pub agents_count: u64,
    pub throughput_per_sec: f64,
    pub total_storage_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
```

#### Step 3: Implement Handlers (~400 LOC)

**`crates/streamhouse-api/src/handlers/topics.rs`**:
```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use crate::{AppState, models::*};

#[utoipa::path(
    get,
    path = "/api/v1/topics",
    responses(
        (status = 200, description = "List all topics", body = Vec<Topic>)
    )
)]
pub async fn list_topics(
    State(state): State<AppState>,
) -> Result<Json<Vec<Topic>>, StatusCode> {
    let topics = state.metadata
        .list_topics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = topics.into_iter().map(|t| Topic {
        name: t.name,
        partitions: t.partition_count,
        replication_factor: t.replication_factor,
        created_at: format_timestamp(t.created_at),
        config: Default::default(),
    }).collect();

    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/v1/topics",
    request_body = CreateTopicRequest,
    responses(
        (status = 201, description = "Topic created", body = Topic)
    )
)]
pub async fn create_topic(
    State(state): State<AppState>,
    Json(req): Json<CreateTopicRequest>,
) -> Result<(StatusCode, Json<Topic>), StatusCode> {
    state.metadata
        .create_topic(&req.name, req.partitions, req.replication_factor)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let topic = Topic {
        name: req.name,
        partitions: req.partitions,
        replication_factor: req.replication_factor,
        created_at: chrono::Utc::now().to_rfc3339(),
        config: Default::default(),
    };

    Ok((StatusCode::CREATED, Json(topic)))
}

#[utoipa::path(
    get,
    path = "/api/v1/topics/{name}",
    params(
        ("name" = String, Path, description = "Topic name")
    ),
    responses(
        (status = 200, description = "Topic details", body = Topic)
    )
)]
pub async fn get_topic(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Topic>, StatusCode> {
    let topic = state.metadata
        .get_topic(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(Topic {
        name: topic.name,
        partitions: topic.partition_count,
        replication_factor: topic.replication_factor,
        created_at: format_timestamp(topic.created_at),
        config: Default::default(),
    }))
}

// Similar handlers for:
// - list_partitions
// - get_partition
// - delete_topic
```

**`crates/streamhouse-api/src/handlers/agents.rs`**:
```rust
#[utoipa::path(
    get,
    path = "/api/v1/agents",
    responses(
        (status = 200, description = "List all agents", body = Vec<Agent>)
    )
)]
pub async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<Vec<Agent>>, StatusCode> {
    let agents = state.metadata
        .list_agents()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get active lease count for each agent
    let response = agents.into_iter().map(|a| Agent {
        agent_id: a.agent_id,
        address: a.address,
        availability_zone: a.availability_zone,
        agent_group: a.agent_group,
        last_heartbeat: a.last_heartbeat,
        started_at: a.started_at,
        active_leases: 0, // TODO: Query partition_leases table
    }).collect();

    Ok(Json(response))
}
```

**`crates/streamhouse-api/src/handlers/produce.rs`**:
```rust
#[utoipa::path(
    post,
    path = "/api/v1/produce",
    request_body = ProduceRequest,
    responses(
        (status = 200, description = "Message produced", body = ProduceResponse)
    )
)]
pub async fn produce(
    State(state): State<AppState>,
    Json(req): Json<ProduceRequest>,
) -> Result<Json<ProduceResponse>, StatusCode> {
    let result = state.producer
        .send(
            &req.topic,
            req.key.as_deref().map(|k| k.as_bytes()),
            req.value.as_bytes(),
            req.partition,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ProduceResponse {
        offset: result.offset,
    }))
}
```

#### Step 4: Create Server (~150 LOC)

**`crates/streamhouse-api/src/lib.rs`**:
```rust
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use streamhouse_metadata::MetadataStore;
use streamhouse_client::Producer;
use tower_http::cors::CorsLayer;

pub mod models;
pub mod handlers;

#[derive(Clone)]
pub struct AppState {
    pub metadata: Arc<dyn MetadataStore>,
    pub producer: Arc<Producer>,
}

pub fn create_router(state: AppState) -> Router {
    // API routes
    let api_routes = Router::new()
        .route("/topics", get(handlers::topics::list_topics).post(handlers::topics::create_topic))
        .route("/topics/:name", get(handlers::topics::get_topic).delete(handlers::topics::delete_topic))
        .route("/topics/:name/partitions", get(handlers::topics::list_partitions))
        .route("/agents", get(handlers::agents::list_agents))
        .route("/agents/:id", get(handlers::agents::get_agent))
        .route("/consumer-groups", get(handlers::consumer_groups::list_groups))
        .route("/produce", post(handlers::produce::produce))
        .route("/metrics", get(handlers::metrics::get_metrics))
        .with_state(state.clone());

    // Swagger UI
    let swagger = utoipa_swagger_ui::SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", ApiDoc::openapi());

    // Main router
    Router::new()
        .nest("/api/v1", api_routes)
        .merge(swagger)
        .route("/health", get(handlers::health::health_check))
        .layer(CorsLayer::permissive())
}

pub async fn serve(router: Router, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("REST API server listening on {}", addr);

    axum::serve(listener, router).await?;
    Ok(())
}

// OpenAPI spec
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::topics::list_topics,
        handlers::topics::create_topic,
        handlers::topics::get_topic,
        handlers::agents::list_agents,
        handlers::produce::produce,
    ),
    components(schemas(
        models::Topic,
        models::CreateTopicRequest,
        models::Agent,
        models::ProduceRequest,
        models::ProduceResponse,
    ))
)]
struct ApiDoc;
```

#### Step 5: Create Binary (~100 LOC)

**`crates/streamhouse-api/src/bin/api.rs`**:
```rust
use std::sync::Arc;
use streamhouse_api::{AppState, create_router, serve};
use streamhouse_metadata::SqliteMetadataStore;
use streamhouse_client::Producer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Connect to metadata store
    let metadata_url = std::env::var("METADATA_STORE")
        .expect("METADATA_STORE environment variable required");

    let metadata = Arc::new(SqliteMetadataStore::new(&metadata_url).await?);

    // Create producer
    let producer = Producer::builder()
        .metadata_store(metadata.clone())
        .build()
        .await?;

    // Create app state
    let state = AppState {
        metadata,
        producer: Arc::new(producer),
    };

    // Create router
    let router = create_router(state);

    // Start server
    let port = std::env::var("API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);

    serve(router, port).await?;

    Ok(())
}
```

### Testing the API

#### Start API Server
```bash
export METADATA_STORE=./data/metadata.db
export AWS_ENDPOINT_URL=http://localhost:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export API_PORT=3001

cargo run --bin api
```

#### Test Endpoints
```bash
# Health check
curl http://localhost:3001/health

# List topics
curl http://localhost:3001/api/v1/topics

# Create topic
curl -X POST http://localhost:3001/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name":"test","partitions":3,"replication_factor":1}'

# List agents
curl http://localhost:3001/api/v1/agents

# Produce message
curl -X POST http://localhost:3001/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic":"test","key":"user1","value":"hello world"}'

# OpenAPI docs
open http://localhost:3001/swagger-ui
```

#### Connect Web Console
```bash
cd web
echo "NEXT_PUBLIC_API_URL=http://localhost:3001" > .env.local
npm run dev
# Open http://localhost:3000/dashboard
```

### Integration Points

The REST API will integrate with existing StreamHouse components:

1. **MetadataStore** - Read topics, partitions, agents, consumer groups
2. **Producer** - Send messages via REST instead of gRPC
3. **ConnectionPool** - (Future) Consumer via REST
4. **Metrics** - Aggregate cluster-wide metrics

### Security (Week 18)

For production, add JWT authentication:

```rust
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

#[derive(Deserialize, Serialize)]
struct Claims {
    sub: String,
    org_id: String,
    exp: usize,
}

async fn verify_jwt(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Claims, StatusCode> {
    let token = auth.token();
    let key = DecodingKey::from_secret("secret".as_ref());

    decode::<Claims>(token, &key, &Validation::default())
        .map(|data| data.claims)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}
```

Apply to routes:
```rust
let protected = Router::new()
    .route("/topics", post(create_topic))
    .layer(middleware::from_fn(auth_middleware));
```

## Timeline

### Week 11: Core API (Days 1-5)
- Day 1-2: Create crate, define models
- Day 3-4: Implement handlers (topics, agents)
- Day 5: Server setup, CORS, testing

### Week 12: Advanced Features (Days 6-10)
- Day 6-7: Produce endpoint, consumer groups
- Day 8: Metrics aggregation
- Day 9: OpenAPI docs, Swagger UI
- Day 10: Integration tests, documentation

## Success Criteria

- ✅ REST API server runs on port 3001
- ✅ All endpoints return correct JSON
- ✅ CORS enabled for web console
- ✅ OpenAPI/Swagger UI available
- ✅ Integration tests pass
- ✅ Web console connects successfully
- ✅ Dashboard shows real data (not mocks)

## Files to Create

1. `crates/streamhouse-api/Cargo.toml`
2. `crates/streamhouse-api/src/lib.rs`
3. `crates/streamhouse-api/src/models.rs`
4. `crates/streamhouse-api/src/handlers/topics.rs`
5. `crates/streamhouse-api/src/handlers/agents.rs`
6. `crates/streamhouse-api/src/handlers/produce.rs`
7. `crates/streamhouse-api/src/handlers/metrics.rs`
8. `crates/streamhouse-api/src/handlers/health.rs`
9. `crates/streamhouse-api/src/bin/api.rs`
10. `crates/streamhouse-api/tests/integration.rs`

**Total Estimated**: ~800 LOC

## Summary

✅ **Web Console**: Complete and ready
🎯 **REST API**: Next step, 2 weeks
📊 **Progress**: Phase 10 started (Week 13-18 follow)

The foundation is laid. Now let's build the REST API backend to power it! 🚀
