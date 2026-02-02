//! Inventory Service - Demo Consumer + Producer
//!
//! Consumes orders from StreamHouse, checks/updates inventory,
//! and produces inventory update events.
//!
//! This demonstrates the Consumer-Producer pattern where a service
//! consumes from one topic and produces to another.
//!
//! ## Topics
//!
//! - Consumes: `orders`
//! - Produces: `inventory-updates`
//!
//! ## Run
//!
//! ```bash
//! cargo run -p inventory-service
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_proto::streamhouse::{
    stream_house_client::StreamHouseClient, ConsumeRequest, ProduceBatchRequest, Record,
};
use tokio::sync::RwLock;
use tonic::transport::Channel;

const GRPC_ADDR: &str = "http://localhost:50051";
const ORDERS_TOPIC: &str = "orders";
const INVENTORY_TOPIC: &str = "inventory-updates";
const CONSUMER_GROUP: &str = "inventory-service";
const POLL_INTERVAL_MS: u64 = 500;

// ============================================================================
// Domain Models
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryUpdateEvent {
    pub event_id: String,
    pub sku: String,
    pub order_id: String,
    pub change: i32,          // negative for deduction
    pub new_quantity: u32,
    pub status: InventoryStatus,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InventoryStatus {
    Reserved,
    Shipped,
    OutOfStock,
    LowStock,
    InStock,
}

// ============================================================================
// Inventory State
// ============================================================================

#[derive(Debug)]
pub struct InventoryState {
    // SKU -> quantity in stock
    pub stock: HashMap<String, u32>,
    pub orders_processed: u64,
    pub updates_produced: u64,
    pub out_of_stock_events: u64,
}

impl InventoryState {
    pub fn new() -> Self {
        // Initialize with some demo inventory
        let mut stock = HashMap::new();
        stock.insert("WIDGET-1".to_string(), 100);
        stock.insert("WIDGET-2".to_string(), 50);
        stock.insert("GADGET-A".to_string(), 200);
        stock.insert("GADGET-B".to_string(), 75);
        stock.insert("TOOL-X".to_string(), 30);
        stock.insert("TOOL-Y".to_string(), 150);

        Self {
            stock,
            orders_processed: 0,
            updates_produced: 0,
            out_of_stock_events: 0,
        }
    }

    pub fn reserve_item(&mut self, sku: &str, quantity: u32, order_id: &str) -> InventoryUpdateEvent {
        let current = self.stock.get(sku).copied().unwrap_or(0);
        let event_id = format!("INV-{}", uuid::Uuid::new_v4().to_string()[..8].to_uppercase());

        let (new_quantity, status) = if current >= quantity {
            let new_qty = current - quantity;
            self.stock.insert(sku.to_string(), new_qty);

            let status = if new_qty == 0 {
                self.out_of_stock_events += 1;
                InventoryStatus::OutOfStock
            } else if new_qty < 10 {
                InventoryStatus::LowStock
            } else {
                InventoryStatus::Reserved
            };

            (new_qty, status)
        } else {
            self.out_of_stock_events += 1;
            // Partial reservation or none
            self.stock.insert(sku.to_string(), 0);
            (0, InventoryStatus::OutOfStock)
        };

        InventoryUpdateEvent {
            event_id,
            sku: sku.to_string(),
            order_id: order_id.to_string(),
            change: -(quantity as i32),
            new_quantity,
            status,
            timestamp: Utc::now(),
        }
    }

    pub fn print_status(&self) {
        println!();
        println!("┌────────────────────────────────────────────────────────────┐");
        println!("│  INVENTORY STATUS                                          │");
        println!("├────────────────────────────────────────────────────────────┤");
        println!("│  Orders Processed:    {:>10}                          │", self.orders_processed);
        println!("│  Updates Produced:    {:>10}                          │", self.updates_produced);
        println!("│  Out of Stock Events: {:>10}                          │", self.out_of_stock_events);
        println!("├────────────────────────────────────────────────────────────┤");
        println!("│  CURRENT STOCK LEVELS                                      │");
        println!("├────────────────────────────────────────────────────────────┤");

        let mut items: Vec<_> = self.stock.iter().collect();
        items.sort_by(|a, b| a.0.cmp(b.0));

        for (sku, qty) in items {
            let status = if *qty == 0 {
                "OUT OF STOCK"
            } else if *qty < 10 {
                "LOW STOCK   "
            } else {
                "IN STOCK    "
            };
            println!("│  {:15} {:>6} units  [{}]                │", sku, qty, status);
        }
        println!("└────────────────────────────────────────────────────────────┘");
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
                .add_directive("inventory_service=info".parse()?)
        )
        .with_target(false)
        .init();

    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║     Inventory Service (Consumer + Producer)                ║");
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
    println!("Consumes: {} → Produces: {}", ORDERS_TOPIC, INVENTORY_TOPIC);
    println!("Consumer Group: {}", CONSUMER_GROUP);
    println!();

    // Inventory state
    let state = Arc::new(RwLock::new(InventoryState::new()));

    // Show initial inventory
    state.read().await.print_status();

    // Track offsets per partition
    let mut partition_offsets: HashMap<u32, u64> = HashMap::new();
    let num_partitions = 6;

    // Consume loop
    loop {
        let mut orders_received = 0;

        // Poll each partition
        for partition in 0..num_partitions {
            let offset = partition_offsets.get(&partition).copied().unwrap_or(0);

            let request = ConsumeRequest {
                topic: ORDERS_TOPIC.to_string(),
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
                                orders_received += 1;

                                // Process each item in the order
                                let mut inventory_updates = Vec::new();
                                {
                                    let mut state = state.write().await;
                                    state.orders_processed += 1;

                                    for item in &order.items {
                                        let update = state.reserve_item(
                                            &item.sku,
                                            item.quantity,
                                            &order.order_id,
                                        );
                                        inventory_updates.push(update);
                                    }
                                }

                                // Produce inventory updates
                                for update in inventory_updates {
                                    let update_json = serde_json::to_vec(&update)?;

                                    // Partition by SKU for ordering
                                    let partition = {
                                        use std::collections::hash_map::DefaultHasher;
                                        use std::hash::{Hash, Hasher};
                                        let mut hasher = DefaultHasher::new();
                                        update.sku.hash(&mut hasher);
                                        (hasher.finish() % 4) as u32 // inventory-updates has 4 partitions
                                    };

                                    let record = Record {
                                        key: update.sku.clone().into_bytes(),
                                        value: update_json,
                                        headers: Default::default(),
                                    };

                                    let produce_request = ProduceBatchRequest {
                                        topic: INVENTORY_TOPIC.to_string(),
                                        partition,
                                        records: vec![record],
                                    };

                                    match client.produce_batch(produce_request).await {
                                        Ok(_) => {
                                            let mut state = state.write().await;
                                            state.updates_produced += 1;

                                            let status_str = match update.status {
                                                InventoryStatus::OutOfStock => "OUT OF STOCK",
                                                InventoryStatus::LowStock => "LOW STOCK",
                                                _ => "reserved",
                                            };

                                            tracing::info!(
                                                sku = %update.sku,
                                                order_id = %update.order_id,
                                                change = %update.change,
                                                new_qty = %update.new_quantity,
                                                status = %status_str,
                                                "Inventory updated"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to produce inventory update: {}", e);
                                        }
                                    }
                                }
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
                    let msg = e.message();
                    if !msg.contains("No records") && !msg.contains("not found") {
                        tracing::debug!("Consume error on partition {}: {}", partition, msg);
                    }
                }
            }
        }

        // Update display if we processed any orders
        if orders_received > 0 {
            state.read().await.print_status();
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}
