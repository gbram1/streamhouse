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
//! use streamhouse_client::{Consumer, OffsetReset};
//! use std::time::Duration;
//!
//! let consumer = Consumer::builder()
//!     .group_id("analytics")
//!     .topics(vec!["orders".to_string()])
//!     .metadata_store(metadata_store)
//!     .object_store(object_store)
//!     .offset_reset(OffsetReset::Earliest)
//!     .build()
//!     .await?;
//!
//! loop {
//!     let records = consumer.poll(Duration::from_secs(1)).await?;
//!     for record in records {
//!         println!("Received: {:?}", record);
//!     }
//!     consumer.commit().await?;
//! }
//! ```

pub mod batch;
pub mod connection_pool;
pub mod consumer;
pub mod error;
pub mod producer;
pub mod retry;

pub use batch::{BatchManager, BatchRecord};
pub use connection_pool::ConnectionPool;
pub use consumer::{ConsumedRecord, Consumer, ConsumerBuilder, ConsumerConfig, OffsetReset};
pub use error::{ClientError, Result};
pub use producer::{Producer, ProducerBuilder, ProducerConfig, ProducerRecord, SendResult};
pub use retry::{retry_with_backoff, retry_with_jittered_backoff, RetryPolicy};
