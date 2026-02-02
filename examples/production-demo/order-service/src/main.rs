//! Order Service - Demo Producer with Schema Validation
//!
//! A REST API that receives orders and produces them to StreamHouse
//! with Avro schema validation via the Schema Registry.
//!
//! ## Schema Versions
//!
//! - **v1**: Basic order (order_id, customer_id, amount, status, created_at)
//! - **v2**: Extended order (adds items array, shipping_address, total_items)
//!
//! ## Endpoints
//!
//! - `POST /orders` - Create order (uses latest schema)
//! - `POST /orders/v1` - Create order using schema v1
//! - `POST /orders/v2` - Create order using schema v2
//! - `GET /health` - Health check
//! - `GET /schemas` - List registered schemas
//!
//! ## Run
//!
//! ```bash
//! cargo run -p order-service --release
//! ```

use apache_avro::{types::Record as AvroRecord, Schema as AvroSchema, Writer};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use streamhouse_proto::streamhouse::{stream_house_client::StreamHouseClient, ProduceBatchRequest, Record};
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tower_http::cors::CorsLayer;

const GRPC_ADDR: &str = "http://localhost:50051";
const SCHEMA_REGISTRY_URL: &str = "http://localhost:8080";
const TOPIC_NAME: &str = "orders";
const HTTP_PORT: u16 = 3001;

// ============================================================================
// Avro Schemas
// ============================================================================

/// Schema v1: Basic order structure
const ORDER_SCHEMA_V1: &str = r#"{
    "type": "record",
    "name": "Order",
    "namespace": "com.streamhouse.orders",
    "fields": [
        {"name": "order_id", "type": "string"},
        {"name": "customer_id", "type": "string"},
        {"name": "amount", "type": "double"},
        {"name": "status", "type": "string"},
        {"name": "created_at", "type": "long", "logicalType": "timestamp-millis"}
    ]
}"#;

/// Schema v2: Extended order with items (backward compatible - new fields have defaults)
const ORDER_SCHEMA_V2: &str = r#"{
    "type": "record",
    "name": "Order",
    "namespace": "com.streamhouse.orders",
    "fields": [
        {"name": "order_id", "type": "string"},
        {"name": "customer_id", "type": "string"},
        {"name": "amount", "type": "double"},
        {"name": "status", "type": "string"},
        {"name": "created_at", "type": "long", "logicalType": "timestamp-millis"},
        {"name": "total_items", "type": "int", "default": 0},
        {"name": "shipping_address", "type": ["null", "string"], "default": null},
        {"name": "items", "type": {
            "type": "array",
            "items": {
                "type": "record",
                "name": "OrderItem",
                "fields": [
                    {"name": "sku", "type": "string"},
                    {"name": "quantity", "type": "int"},
                    {"name": "price", "type": "double"}
                ]
            }
        }, "default": []}
    ]
}"#;

// ============================================================================
// Domain Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub sku: String,
    pub quantity: u32,
    pub price: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOrderRequest {
    pub customer_id: String,
    pub items: Vec<OrderItem>,
    #[serde(default)]
    pub shipping_address: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateOrderResponse {
    pub order_id: String,
    pub status: String,
    pub total_amount: f64,
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub streamhouse_connected: bool,
    pub schema_registry_connected: bool,
    pub schemas_registered: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaInfo {
    pub subject: String,
    pub version: u32,
    pub schema_type: String,
}

// ============================================================================
// Schema Registry Client
// ============================================================================

#[derive(Clone)]
pub struct SchemaRegistryClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl SchemaRegistryClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// Register a schema for a subject
    pub async fn register_schema(&self, subject: &str, schema: &str) -> anyhow::Result<u32> {
        let url = format!("{}/schemas/subjects/{}/versions", self.base_url, subject);

        #[derive(Serialize)]
        struct RegisterRequest {
            schema: String,
            #[serde(rename = "schemaType")]
            schema_type: String,
        }

        #[derive(Deserialize)]
        struct RegisterResponse {
            id: u32,
        }

        let response = self
            .http_client
            .post(&url)
            .json(&RegisterRequest {
                schema: schema.to_string(),
                schema_type: "AVRO".to_string(),
            })
            .send()
            .await?;

        if response.status().is_success() {
            let result: RegisterResponse = response.json().await?;
            Ok(result.id)
        } else if response.status() == 409 {
            // Schema already exists, get the ID
            self.get_schema_id(subject).await
        } else {
            let error = response.text().await?;
            anyhow::bail!("Failed to register schema: {}", error)
        }
    }

    /// Get the schema ID for a subject
    pub async fn get_schema_id(&self, subject: &str) -> anyhow::Result<u32> {
        let url = format!("{}/schemas/subjects/{}/versions/latest", self.base_url, subject);

        #[derive(Deserialize)]
        struct SchemaResponse {
            id: u32,
        }

        let response = self.http_client.get(&url).send().await?;
        if response.status().is_success() {
            let result: SchemaResponse = response.json().await?;
            Ok(result.id)
        } else {
            anyhow::bail!("Schema not found for subject: {}", subject)
        }
    }

    /// Get all versions for a subject
    pub async fn get_versions(&self, subject: &str) -> anyhow::Result<Vec<u32>> {
        let url = format!("{}/schemas/subjects/{}/versions", self.base_url, subject);

        let response = self.http_client.get(&url).send().await?;
        if response.status().is_success() {
            let versions: Vec<u32> = response.json().await?;
            Ok(versions)
        } else {
            Ok(vec![])
        }
    }

    /// Check if schema registry is reachable
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/schemas/subjects", self.base_url);
        self.http_client.get(&url).send().await.is_ok()
    }
}

// ============================================================================
// Avro Serialization
// ============================================================================

/// Serialize order to Avro bytes using v1 schema
fn serialize_order_v1(
    order_id: &str,
    customer_id: &str,
    amount: f64,
    status: &str,
    created_at: DateTime<Utc>,
    schema: &AvroSchema,
) -> anyhow::Result<Vec<u8>> {
    let mut record = AvroRecord::new(schema).ok_or_else(|| anyhow::anyhow!("Failed to create Avro record"))?;

    record.put("order_id", order_id);
    record.put("customer_id", customer_id);
    record.put("amount", amount);
    record.put("status", status);
    record.put("created_at", created_at.timestamp_millis());

    let mut writer = Writer::new(schema, Vec::new());
    writer.append(record)?;
    Ok(writer.into_inner()?)
}

/// Serialize order to Avro bytes using v2 schema
fn serialize_order_v2(
    order_id: &str,
    customer_id: &str,
    amount: f64,
    status: &str,
    created_at: DateTime<Utc>,
    items: &[OrderItem],
    shipping_address: Option<&str>,
    schema: &AvroSchema,
) -> anyhow::Result<Vec<u8>> {
    let mut record = AvroRecord::new(schema).ok_or_else(|| anyhow::anyhow!("Failed to create Avro record"))?;

    record.put("order_id", order_id);
    record.put("customer_id", customer_id);
    record.put("amount", amount);
    record.put("status", status);
    record.put("created_at", created_at.timestamp_millis());
    record.put("total_items", items.iter().map(|i| i.quantity as i32).sum::<i32>());

    // Shipping address (union type: null or string)
    match shipping_address {
        Some(addr) => record.put("shipping_address", Some(addr.to_string())),
        None => record.put("shipping_address", None::<String>),
    }

    // Items array
    let items_schema = schema
        .clone();
    let items_values: Vec<apache_avro::types::Value> = items
        .iter()
        .map(|item| {
            let mut item_map = std::collections::HashMap::new();
            item_map.insert("sku".to_string(), apache_avro::types::Value::String(item.sku.clone()));
            item_map.insert("quantity".to_string(), apache_avro::types::Value::Int(item.quantity as i32));
            item_map.insert("price".to_string(), apache_avro::types::Value::Double(item.price));
            apache_avro::types::Value::Record(item_map.into_iter().collect())
        })
        .collect();
    record.put("items", apache_avro::types::Value::Array(items_values));

    let mut writer = Writer::new(schema, Vec::new());
    writer.append(record)?;
    Ok(writer.into_inner()?)
}

// ============================================================================
// Application State
// ============================================================================

#[derive(Clone)]
pub struct AppState {
    grpc_client: Arc<Mutex<StreamHouseClient<Channel>>>,
    schema_client: SchemaRegistryClient,
    schema_v1: AvroSchema,
    schema_v2: AvroSchema,
    schema_v1_id: u32,
    schema_v2_id: u32,
    orders_produced: Arc<std::sync::atomic::AtomicU64>,
    orders_v1_count: Arc<std::sync::atomic::AtomicU64>,
    orders_v2_count: Arc<std::sync::atomic::AtomicU64>,
}

// ============================================================================
// Handlers
// ============================================================================

async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let schema_healthy = state.schema_client.health_check().await;

    Json(HealthResponse {
        status: "healthy".to_string(),
        service: "order-service".to_string(),
        streamhouse_connected: true,
        schema_registry_connected: schema_healthy,
        schemas_registered: vec![
            format!("orders-value-v1 (id: {})", state.schema_v1_id),
            format!("orders-value-v2 (id: {})", state.schema_v2_id),
        ],
    })
}

async fn list_schemas(State(state): State<AppState>) -> Json<Vec<SchemaInfo>> {
    Json(vec![
        SchemaInfo {
            subject: "orders-value-v1".to_string(),
            version: 1,
            schema_type: "AVRO".to_string(),
        },
        SchemaInfo {
            subject: "orders-value-v2".to_string(),
            version: 2,
            schema_type: "AVRO".to_string(),
        },
    ])
}

/// Create order using v1 schema (basic fields only)
async fn create_order_v1(
    State(state): State<AppState>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, (StatusCode, String)> {
    create_order_with_version(state, request, 1).await
}

/// Create order using v2 schema (with items)
async fn create_order_v2(
    State(state): State<AppState>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, (StatusCode, String)> {
    create_order_with_version(state, request, 2).await
}

/// Create order using latest schema (v2)
async fn create_order(
    State(state): State<AppState>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, (StatusCode, String)> {
    create_order_with_version(state, request, 2).await
}

async fn create_order_with_version(
    state: AppState,
    request: CreateOrderRequest,
    version: u32,
) -> Result<Json<CreateOrderResponse>, (StatusCode, String)> {
    // Generate order ID
    let order_id = format!("ORD-{}", uuid::Uuid::new_v4().to_string()[..8].to_uppercase());

    // Calculate total
    let total_amount: f64 = request
        .items
        .iter()
        .map(|item| item.price * item.quantity as f64)
        .sum();

    let created_at = Utc::now();
    let status = "created";

    // Serialize based on schema version
    let (avro_bytes, schema_id) = match version {
        1 => {
            let bytes = serialize_order_v1(
                &order_id,
                &request.customer_id,
                total_amount,
                status,
                created_at,
                &state.schema_v1,
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Avro serialization error: {}", e)))?;
            state.orders_v1_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (bytes, state.schema_v1_id)
        }
        2 => {
            let bytes = serialize_order_v2(
                &order_id,
                &request.customer_id,
                total_amount,
                status,
                created_at,
                &request.items,
                request.shipping_address.as_deref(),
                &state.schema_v2,
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Avro serialization error: {}", e)))?;
            state.orders_v2_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (bytes, state.schema_v2_id)
        }
        _ => return Err((StatusCode::BAD_REQUEST, format!("Unknown schema version: {}", version))),
    };

    // Prepend schema ID (4 bytes, big-endian) - Confluent wire format
    let mut value = vec![0u8]; // Magic byte
    value.extend_from_slice(&schema_id.to_be_bytes());
    value.extend_from_slice(&avro_bytes);

    // Create record for StreamHouse
    let record = Record {
        key: request.customer_id.clone().into_bytes(),
        value,
        headers: Default::default(),
    };

    // Calculate partition based on customer_id hash
    let partition = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        request.customer_id.hash(&mut hasher);
        (hasher.finish() % 6) as u32
    };

    // Send to StreamHouse
    let produce_request = ProduceBatchRequest {
        topic: TOPIC_NAME.to_string(),
        partition,
        records: vec![record],
    };

    let mut client = state.grpc_client.lock().await;
    client
        .produce_batch(produce_request)
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, format!("StreamHouse error: {}", e)))?;

    // Update metrics
    state.orders_produced.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let count = state.orders_produced.load(std::sync::atomic::Ordering::Relaxed);

    tracing::info!(
        order_id = %order_id,
        customer_id = %request.customer_id,
        total = %total_amount,
        schema_version = %version,
        schema_id = %schema_id,
        partition = %partition,
        total_orders = %count,
        "Order created with schema validation"
    );

    Ok(Json(CreateOrderResponse {
        order_id,
        status: status.to_string(),
        total_amount,
        schema_version: version,
        created_at,
    }))
}

async fn get_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let count = state.orders_produced.load(std::sync::atomic::Ordering::Relaxed);
    let v1_count = state.orders_v1_count.load(std::sync::atomic::Ordering::Relaxed);
    let v2_count = state.orders_v2_count.load(std::sync::atomic::Ordering::Relaxed);

    Json(serde_json::json!({
        "orders_produced": count,
        "orders_v1": v1_count,
        "orders_v2": v2_count,
        "topic": TOPIC_NAME,
        "schemas": {
            "v1_id": state.schema_v1_id,
            "v2_id": state.schema_v2_id,
        }
    }))
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("order_service=info".parse()?)
                .add_directive("tower_http=debug".parse()?),
        )
        .with_target(false)
        .init();

    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║     Order Service (with Schema Validation)                 ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Parse schemas
    let schema_v1 = AvroSchema::parse_str(ORDER_SCHEMA_V1)?;
    let schema_v2 = AvroSchema::parse_str(ORDER_SCHEMA_V2)?;
    tracing::info!("Parsed Avro schemas v1 and v2");

    // Connect to Schema Registry
    let schema_client = SchemaRegistryClient::new(SCHEMA_REGISTRY_URL);

    // Register schemas
    tracing::info!("Registering schemas with Schema Registry...");

    let schema_v1_id = schema_client
        .register_schema("orders-value-v1", ORDER_SCHEMA_V1)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to register v1 schema: {}, using default ID", e);
            100
        });
    tracing::info!("Schema v1 registered with ID: {}", schema_v1_id);

    let schema_v2_id = schema_client
        .register_schema("orders-value-v2", ORDER_SCHEMA_V2)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to register v2 schema: {}, using default ID", e);
            101
        });
    tracing::info!("Schema v2 registered with ID: {}", schema_v2_id);

    // Connect to StreamHouse
    tracing::info!("Connecting to StreamHouse at {}", GRPC_ADDR);

    let channel = Channel::from_static(GRPC_ADDR)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .http2_keep_alive_interval(Duration::from_secs(20))
        .keep_alive_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true)
        .connect()
        .await?;

    let grpc_client = StreamHouseClient::new(channel)
        .max_decoding_message_size(64 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024);

    tracing::info!("Connected to StreamHouse");

    let state = AppState {
        grpc_client: Arc::new(Mutex::new(grpc_client)),
        schema_client,
        schema_v1,
        schema_v2,
        schema_v1_id,
        schema_v2_id,
        orders_produced: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        orders_v1_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        orders_v2_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    // Build router
    let app = Router::new()
        .route("/orders", post(create_order))       // Latest schema (v2)
        .route("/orders/v1", post(create_order_v1)) // Schema v1
        .route("/orders/v2", post(create_order_v2)) // Schema v2
        .route("/health", get(health_check))
        .route("/schemas", get(list_schemas))
        .route("/stats", get(get_stats))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let addr = format!("0.0.0.0:{}", HTTP_PORT);
    tracing::info!("Order Service listening on http://{}", addr);
    println!();
    println!("Schema Versions:");
    println!("  v1 (ID: {}) - Basic: order_id, customer_id, amount, status", schema_v1_id);
    println!("  v2 (ID: {}) - Extended: + items[], shipping_address, total_items", schema_v2_id);
    println!();
    println!("Endpoints:");
    println!("  POST /orders     - Create order (uses v2 schema)");
    println!("  POST /orders/v1  - Create order with v1 schema");
    println!("  POST /orders/v2  - Create order with v2 schema");
    println!("  GET  /health     - Health check");
    println!("  GET  /schemas    - List schemas");
    println!("  GET  /stats      - Statistics");
    println!();
    println!("Test v1 (basic):");
    println!("  curl -X POST http://localhost:{}/orders/v1 \\", HTTP_PORT);
    println!("    -H 'Content-Type: application/json' \\");
    println!("    -d '{{\"customer_id\": \"cust-1\", \"items\": [{{\"sku\": \"A\", \"quantity\": 1, \"price\": 10}}]}}'");
    println!();
    println!("Test v2 (extended):");
    println!("  curl -X POST http://localhost:{}/orders/v2 \\", HTTP_PORT);
    println!("    -H 'Content-Type: application/json' \\");
    println!("    -d '{{\"customer_id\": \"cust-1\", \"items\": [{{\"sku\": \"A\", \"quantity\": 2, \"price\": 25}}], \"shipping_address\": \"123 Main St\"}}'");
    println!();

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
