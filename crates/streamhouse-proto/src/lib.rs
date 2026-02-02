//! StreamHouse Protocol Buffer Definitions
//!
//! This crate contains the gRPC service definitions and message types for
//! communication between StreamHouse components (producers, consumers, agents).
//!
//! ## Services
//!
//! - **StreamHouse**: Main gRPC API exposed by the unified server (port 50051)
//!   - Admin: CreateTopic, GetTopic, ListTopics, DeleteTopic
//!   - Producer: Produce, ProduceBatch
//!   - Consumer: Consume, CommitOffset, GetOffset
//!
//! - **ProducerService**: Legacy service (kept for compatibility)
//!
//! ## Usage
//!
//! ### Client-side (High-Performance Producer)
//!
//! ```ignore
//! use streamhouse_proto::streamhouse::{
//!     stream_house_client::StreamHouseClient,
//!     ProduceBatchRequest, Record,
//! };
//!
//! let mut client = StreamHouseClient::connect("http://localhost:50051").await?;
//!
//! let request = ProduceBatchRequest {
//!     topic: "orders".to_string(),
//!     partition: 0,
//!     records: vec![
//!         Record {
//!             key: b"key1".to_vec(),
//!             value: b"value1".to_vec(),
//!             headers: Default::default(),
//!         },
//!     ],
//! };
//!
//! let response = client.produce_batch(request).await?;
//! println!("First offset: {}", response.into_inner().first_offset);
//! ```
//!
//! ### Server-side (Unified Server)
//!
//! ```ignore
//! use streamhouse_proto::streamhouse::{
//!     stream_house_server::{StreamHouse, StreamHouseServer},
//!     ProduceBatchRequest, ProduceBatchResponse,
//! };
//! ```

/// Main StreamHouse gRPC API (unified server)
///
/// This is the primary API for high-performance producers and consumers.
/// Exposed on port 50051 by the unified server.
pub mod streamhouse {
    tonic::include_proto!("streamhouse");
}

/// Legacy ProducerService (kept for compatibility)
///
/// New code should use the `streamhouse` module instead.
pub mod producer {
    tonic::include_proto!("streamhouse.producer");
}
