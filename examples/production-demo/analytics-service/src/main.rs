//! Analytics Service - Demo Consumer
//!
//! Consumes orders from StreamHouse and calculates real-time metrics.
//! This demonstrates how a consumer service would integrate with StreamHouse.
//!
//! ## Metrics Calculated
//!
//! - Total revenue
//! - Order count
//! - Average order value
//! - Orders per customer
//! - Top products
//!
//! ## Run
//!
//! ```bash
//! cargo run -p analytics-service
//! ```

use apache_avro::{from_value, Reader, Schema};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_proto::streamhouse::{stream_house_client::StreamHouseClient, ConsumeRequest};
use tokio::sync::RwLock;
use tonic::transport::Channel;

const GRPC_ADDR: &str = "http://localhost:50051";
const SCHEMA_REGISTRY_URL: &str = "http://localhost:8080";
const TOPIC_NAME: &str = "orders";
const CONSUMER_GROUP: &str = "analytics-pipeline";
const POLL_INTERVAL_MS: u64 = 500;

// ============================================================================
// Domain Models (same as order-service)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub sku: String,
    pub quantity: i32,  // Avro uses i32
    pub price: f64,
}

// Schema v1: order_id, customer_id, amount, status, created_at
// For simplicity, we'll just extract the key fields we need
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderEvent {
    pub order_id: String,
    pub customer_id: String,
    pub amount: f64,
    pub status: String,
    #[serde(default)]
    pub created_at: i64,  // May fail due to logicalType, use default
}

// ============================================================================
// Schema Registry Client
// ============================================================================

struct SchemaCache {
    schemas: HashMap<i32, Schema>,
    http_client: reqwest::Client,
}

impl SchemaCache {
    fn new() -> Self {
        Self {
            schemas: HashMap::new(),
            http_client: reqwest::Client::new(),
        }
    }

    async fn get_schema(&mut self, schema_id: i32) -> Option<Schema> {
        if let Some(schema) = self.schemas.get(&schema_id) {
            return Some(schema.clone());
        }

        // Fetch from Schema Registry (note: schema registry is nested under /schemas)
        let url = format!("{}/schemas/schemas/ids/{}", SCHEMA_REGISTRY_URL, schema_id);
        if let Ok(resp) = self.http_client.get(&url).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(schema_str) = body.get("schema").and_then(|s| s.as_str()) {
                    if let Ok(schema) = Schema::parse_str(schema_str) {
                        self.schemas.insert(schema_id, schema.clone());
                        return Some(schema);
                    }
                }
            }
        }
        None
    }
}

fn decode_avro_container(data: &[u8]) -> Option<OrderEvent> {
    use apache_avro::types::Value;

    // Check for Avro Object Container magic bytes "Obj\x01"
    if data.len() < 4 || &data[0..3] != b"Obj" {
        return None;
    }

    // Decode using Avro Reader (schema is embedded in container)
    let reader = Reader::new(data).ok()?;
    for value in reader {
        if let Ok(Value::Record(fields)) = value {
            // Extract fields manually
            let mut order_id = String::new();
            let mut customer_id = String::new();
            let mut amount = 0.0f64;
            let mut status = String::new();

            for (name, val) in fields {
                match (name.as_str(), val) {
                    ("order_id", Value::String(s)) => order_id = s,
                    ("customer_id", Value::String(s)) => customer_id = s,
                    ("amount", Value::Double(d)) => amount = d,
                    ("status", Value::String(s)) => status = s,
                    _ => {}
                }
            }

            if !order_id.is_empty() {
                return Some(OrderEvent {
                    order_id,
                    customer_id,
                    amount,
                    status,
                    created_at: 0,
                });
            }
        }
    }
    None
}

fn decode_confluent_avro(data: &[u8], schema: &Schema) -> Option<OrderEvent> {
    // Skip 5-byte Confluent wire format header (magic byte + 4-byte schema ID)
    if data.len() < 5 || data[0] != 0 {
        return None;
    }
    let avro_data = &data[5..];

    // Decode Avro
    let reader = Reader::with_schema(schema, avro_data).ok()?;
    for value in reader {
        if let Ok(v) = value {
            // Convert Avro value to OrderEvent
            return from_value::<OrderEvent>(&v).ok();
        }
    }
    None
}

// ============================================================================
// Analytics State
// ============================================================================

#[derive(Debug, Default)]
pub struct AnalyticsState {
    pub total_revenue: f64,
    pub order_count: u64,
    pub orders_by_customer: HashMap<String, u64>,
    pub revenue_by_customer: HashMap<String, f64>,
    pub product_quantities: HashMap<String, u64>,
    pub last_order_id: Option<String>,
    pub last_updated: Option<DateTime<Utc>>,
}

impl AnalyticsState {
    pub fn process_order(&mut self, order: &OrderEvent) {
        self.total_revenue += order.amount;
        self.order_count += 1;

        // Track by customer
        *self.orders_by_customer.entry(order.customer_id.clone()).or_insert(0) += 1;
        *self.revenue_by_customer.entry(order.customer_id.clone()).or_insert(0.0) += order.amount;

        // Note: v1 schema doesn't include items, so product tracking is limited
        // In production, you'd handle both v1 and v2 schemas differently

        self.last_order_id = Some(order.order_id.clone());
        self.last_updated = Some(chrono::Utc::now());
    }

    pub fn average_order_value(&self) -> f64 {
        if self.order_count == 0 {
            0.0
        } else {
            self.total_revenue / self.order_count as f64
        }
    }

    pub fn top_customers(&self, n: usize) -> Vec<(&String, &f64)> {
        let mut customers: Vec<_> = self.revenue_by_customer.iter().collect();
        customers.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        customers.into_iter().take(n).collect()
    }

    pub fn top_products(&self, n: usize) -> Vec<(&String, &u64)> {
        let mut products: Vec<_> = self.product_quantities.iter().collect();
        products.sort_by(|a, b| b.1.cmp(a.1));
        products.into_iter().take(n).collect()
    }

    pub fn print_dashboard(&self) {
        // Clear screen and move cursor to top
        print!("\x1B[2J\x1B[1;1H");

        println!("╔════════════════════════════════════════════════════════════╗");
        println!("║           Analytics Dashboard (Real-Time)                  ║");
        println!("╚════════════════════════════════════════════════════════════╝");
        println!();
        println!("┌────────────────────────────────────────────────────────────┐");
        println!("│  KEY METRICS                                               │");
        println!("├────────────────────────────────────────────────────────────┤");
        println!("│  Total Revenue:     ${:>12.2}                         │", self.total_revenue);
        println!("│  Total Orders:      {:>12}                          │", self.order_count);
        println!("│  Avg Order Value:   ${:>12.2}                         │", self.average_order_value());
        println!("│  Unique Customers:  {:>12}                          │", self.orders_by_customer.len());
        println!("└────────────────────────────────────────────────────────────┘");
        println!();

        // Top Customers
        println!("┌────────────────────────────────────────────────────────────┐");
        println!("│  TOP CUSTOMERS BY REVENUE                                  │");
        println!("├────────────────────────────────────────────────────────────┤");
        for (customer, revenue) in self.top_customers(5) {
            println!("│  {:20} ${:>10.2}                        │", customer, revenue);
        }
        if self.orders_by_customer.is_empty() {
            println!("│  (no data yet)                                             │");
        }
        println!("└────────────────────────────────────────────────────────────┘");
        println!();

        // Top Products
        println!("┌────────────────────────────────────────────────────────────┐");
        println!("│  TOP PRODUCTS BY QUANTITY                                  │");
        println!("├────────────────────────────────────────────────────────────┤");
        for (sku, quantity) in self.top_products(5) {
            println!("│  {:20} {:>10} units                     │", sku, quantity);
        }
        if self.product_quantities.is_empty() {
            println!("│  (no data yet)                                             │");
        }
        println!("└────────────────────────────────────────────────────────────┘");
        println!();

        if let Some(ref order_id) = self.last_order_id {
            println!("Last order: {} at {}", order_id, self.last_updated.unwrap().format("%H:%M:%S"));
        }
        println!();
        println!("Consumer Group: {} | Topic: {}", CONSUMER_GROUP, TOPIC_NAME);
        println!("Press Ctrl+C to exit");
    }
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
                .add_directive("analytics_service=info".parse()?)
        )
        .with_target(false)
        .init();

    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║        Analytics Service (StreamHouse Consumer)            ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

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

    let mut client = StreamHouseClient::new(channel)
        .max_decoding_message_size(64 * 1024 * 1024)
        .max_encoding_message_size(64 * 1024 * 1024);

    tracing::info!("Connected to StreamHouse");
    println!("Starting to consume from topic '{}' with group '{}'", TOPIC_NAME, CONSUMER_GROUP);
    println!();

    // Analytics state
    let state = Arc::new(RwLock::new(AnalyticsState::default()));

    // Schema cache for Avro decoding
    let mut schema_cache = SchemaCache::new();

    // Track offsets per partition
    let mut partition_offsets: HashMap<u32, u64> = HashMap::new();
    let num_partitions = 6; // orders topic has 6 partitions

    // Initial dashboard
    state.read().await.print_dashboard();

    // Consume loop
    loop {
        let mut orders_received = 0;

        // Poll each partition
        for partition in 0..num_partitions {
            let offset = partition_offsets.get(&partition).copied().unwrap_or(0);

            let request = ConsumeRequest {
                topic: TOPIC_NAME.to_string(),
                partition,
                offset,
                max_records: 100,
                consumer_group: Some(CONSUMER_GROUP.to_string()),
            };

            match client.consume(request).await {
                Ok(response) => {
                    let consume_response = response.into_inner();
                    let records_count = consume_response.records.len() as u64;

                    if records_count > 0 {
                        eprintln!("Got {} records from partition {}", records_count, partition);
                    }

                    for record in consume_response.records {
                        let mut order_opt: Option<OrderEvent> = None;

                        // Check for Confluent wire format with embedded Avro container
                        // Format: [0x00][4-byte schema ID][Avro Object Container]
                        if record.value.len() >= 9 && record.value[0] == 0 {
                            // Skip 5-byte Confluent header, check for "Obj"
                            if &record.value[5..8] == b"Obj" {
                                order_opt = decode_avro_container(&record.value[5..]);
                            } else {
                                // Try standard Confluent format (raw Avro, not container)
                                let schema_id = i32::from_be_bytes([
                                    record.value[1],
                                    record.value[2],
                                    record.value[3],
                                    record.value[4],
                                ]);
                                if let Some(schema) = schema_cache.get_schema(schema_id).await {
                                    order_opt = decode_confluent_avro(&record.value, &schema);
                                }
                            }
                        }
                        // Try standalone Avro Object Container (starts with "Obj")
                        else if record.value.len() >= 4 && &record.value[0..3] == b"Obj" {
                            order_opt = decode_avro_container(&record.value);
                        }

                        // Fallback to JSON
                        if order_opt.is_none() {
                            order_opt = serde_json::from_slice::<OrderEvent>(&record.value).ok();
                        }

                        // Debug: show first bytes if decode failed
                        if order_opt.is_none() && record.value.len() > 0 {
                            eprintln!("Failed to decode. First 10 bytes: {:?}", &record.value[..record.value.len().min(10)]);
                        }

                        // Process order if decoded
                        if let Some(order) = order_opt {
                            let mut state = state.write().await;
                            state.process_order(&order);
                            orders_received += 1;
                        }
                    }

                    // Update offset based on records consumed
                    if records_count > 0 {
                        partition_offsets.insert(partition, offset + records_count);
                    }
                }
                Err(e) => {
                    // Log all errors for debugging
                    let msg = e.message();
                    eprintln!("Consume error on partition {}: {}", partition, msg);
                }
            }
        }

        // Update dashboard if we received any orders
        if orders_received > 0 {
            state.read().await.print_dashboard();
        }

        // Small delay between polls
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}
