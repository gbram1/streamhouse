//! StreamHouse Protocol Buffer Definitions
//!
//! This crate contains the gRPC service definitions and message types for
//! communication between StreamHouse components (producers, consumers, agents).
//!
//! ## Services
//!
//! - **ProducerService**: Agents implement this to accept records from producers
//!
//! ## Usage
//!
//! ### Server-side (Agent)
//!
//! ```ignore
//! use streamhouse_proto::producer::{
//!     producer_service_server::{ProducerService, ProducerServiceServer},
//!     ProduceRequest, ProduceResponse,
//! };
//! use tonic::{Request, Response, Status};
//!
//! struct MyProducerService;
//!
//! #[tonic::async_trait]
//! impl ProducerService for MyProducerService {
//!     async fn produce(
//!         &self,
//!         request: Request<ProduceRequest>,
//!     ) -> Result<Response<ProduceResponse>, Status> {
//!         // Implementation here
//!         Ok(Response::new(ProduceResponse {
//!             base_offset: 0,
//!             record_count: request.into_inner().records.len() as u32,
//!         }))
//!     }
//! }
//! ```
//!
//! ### Client-side (Producer)
//!
//! ```ignore
//! use streamhouse_proto::producer::{
//!     producer_service_client::ProducerServiceClient,
//!     ProduceRequest, produce_request::Record,
//! };
//!
//! let mut client = ProducerServiceClient::connect("http://localhost:9090").await?;
//!
//! let request = ProduceRequest {
//!     topic: "orders".to_string(),
//!     partition: 0,
//!     records: vec![
//!         Record {
//!             key: Some(b"key1".to_vec()),
//!             value: b"value1".to_vec(),
//!             timestamp: 1234567890,
//!         },
//!     ],
//! };
//!
//! let response = client.produce(request).await?;
//! println!("Base offset: {}", response.into_inner().base_offset);
//! ```

// Include the generated protobuf code
pub mod producer {
    tonic::include_proto!("streamhouse.producer");
}
