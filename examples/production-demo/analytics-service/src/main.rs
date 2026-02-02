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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_proto::streamhouse::{stream_house_client::StreamHouseClient, ConsumeRequest};
use tokio::sync::RwLock;
use tonic::transport::Channel;

const GRPC_ADDR: &str = "http://localhost:50051";
const TOPIC_NAME: &str = "orders";
const CONSUMER_GROUP: &str = "analytics-pipeline";
const POLL_INTERVAL_MS: u64 = 500;

// ============================================================================
// Domain Models (same as order-service)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub sku: String,
    pub quantity: u32,
    pub price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderEvent {
    pub order_id: String,
    pub customer_id: String,
    pub items: Vec<OrderItem>,
    pub total_amount: f64,
    pub shipping_address: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
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
        self.total_revenue += order.total_amount;
        self.order_count += 1;

        // Track by customer
        *self.orders_by_customer.entry(order.customer_id.clone()).or_insert(0) += 1;
        *self.revenue_by_customer.entry(order.customer_id.clone()).or_insert(0.0) += order.total_amount;

        // Track product quantities
        for item in &order.items {
            *self.product_quantities.entry(item.sku.clone()).or_insert(0) += item.quantity as u64;
        }

        self.last_order_id = Some(order.order_id.clone());
        self.last_updated = Some(Utc::now());
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

                    for record in consume_response.records {
                        // Deserialize order event
                        match serde_json::from_slice::<OrderEvent>(&record.value) {
                            Ok(order) => {
                                let mut state = state.write().await;
                                state.process_order(&order);
                                orders_received += 1;
                            }
                            Err(e) => {
                                tracing::warn!("Failed to deserialize order: {}", e);
                            }
                        }
                    }

                    // Update offset based on records consumed
                    if records_count > 0 {
                        partition_offsets.insert(partition, offset + records_count);
                    }
                }
                Err(e) => {
                    // Ignore "no records" errors, log others
                    let msg = e.message();
                    if !msg.contains("No records") && !msg.contains("not found") {
                        tracing::debug!("Consume error on partition {}: {}", partition, msg);
                    }
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
