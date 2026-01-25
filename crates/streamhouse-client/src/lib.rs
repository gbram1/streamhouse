//! StreamHouse Client - Producer and Consumer APIs
//!
//! This crate provides high-level Producer and Consumer APIs for interacting with
//! StreamHouse agents. It handles agent discovery, partition routing, batching,
//! compression, and error handling.
//!
//! # Examples
//!
//! ## Producer
//!
//! ```ignore
//! use streamhouse_client::{Producer, ProducerConfig};
//!
//! let producer = Producer::builder()
//!     .metadata_store(metadata_store)
//!     .agent_group("prod")
//!     .build()
//!     .await?;
//!
//! producer.send("orders", Some(b"user123"), b"order data", None).await?;
//! ```
//!
//! ## Consumer
//!
//! ```ignore
//! use streamhouse_client::{Consumer, ConsumerConfig};
//!
//! let consumer = Consumer::builder()
//!     .metadata_store(metadata_store)
//!     .group_id("analytics")
//!     .topics(vec!["orders".to_string()])
//!     .build()
//!     .await?;
//!
//! while let Some(record) = consumer.poll(Duration::from_secs(1)).await? {
//!     println!("Received: {:?}", record);
//!     consumer.commit().await?;
//! }
//! ```

pub mod batch;
pub mod connection_pool;
pub mod error;
pub mod producer;
pub mod retry;

pub use batch::{BatchManager, BatchRecord};
pub use connection_pool::ConnectionPool;
pub use error::{ClientError, Result};
pub use producer::{Producer, ProducerBuilder, ProducerConfig, ProducerRecord, SendResult};
pub use retry::{retry_with_backoff, retry_with_jittered_backoff, RetryPolicy};
