//! # StreamHouse Kafka Protocol Compatibility
//!
//! This crate provides native Kafka binary protocol support for StreamHouse,
//! enabling any standard Kafka client to connect and interact with the system.
//!
//! ## Supported APIs
//!
//! | API | Name | Status |
//! |-----|------|--------|
//! | 18 | ApiVersions | ✅ Implemented |
//! | 3 | Metadata | ✅ Implemented |
//! | 0 | Produce | ✅ Implemented |
//! | 1 | Fetch | ✅ Implemented |
//! | 2 | ListOffsets | ✅ Implemented |
//! | 10 | FindCoordinator | ✅ Implemented |
//! | 11 | JoinGroup | ✅ Implemented |
//! | 14 | SyncGroup | ✅ Implemented |
//! | 12 | Heartbeat | ✅ Implemented |
//! | 13 | LeaveGroup | ✅ Implemented |
//! | 8 | OffsetCommit | ✅ Implemented |
//! | 9 | OffsetFetch | ✅ Implemented |
//! | 19 | CreateTopics | ✅ Implemented |
//! | 20 | DeleteTopics | ✅ Implemented |
//!
//! ## Usage
//!
//! ```rust,no_run
//! use streamhouse_kafka::{KafkaServer, KafkaServerConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = KafkaServerConfig::default();
//!     let server = KafkaServer::new(config, metadata, writer_pool, segment_cache, object_store).await.unwrap();
//!     server.run().await.unwrap();
//! }
//! ```

pub mod codec;
pub mod coordinator;
pub mod error;
pub mod handlers;
pub mod server;
pub mod tenant;
pub mod types;

pub use coordinator::GroupCoordinator;
pub use error::{KafkaError, KafkaResult};
pub use server::{BoundKafkaServer, KafkaServer, KafkaServerConfig, KafkaServerState};
pub use tenant::{AuthMethod, KafkaTenantContext, KafkaTenantResolver};
pub use types::ApiKey;
