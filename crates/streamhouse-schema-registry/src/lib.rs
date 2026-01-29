//! StreamHouse Schema Registry
//!
//! Provides schema validation, versioning, and compatibility checking for StreamHouse.
//!
//! # Features
//!
//! - **Schema Formats**: Avro, Protobuf, JSON Schema
//! - **Versioning**: Automatic version management per subject
//! - **Compatibility**: Backward, forward, full compatibility checking
//! - **Caching**: In-memory caching for performance
//! - **REST API**: HTTP API compatible with Confluent Schema Registry
//!
//! # Usage
//!
//! ```ignore
//! use streamhouse_schema_registry::{SchemaRegistry, RegisterSchemaRequest, SchemaFormat};
//!
//! // Create registry
//! let registry = SchemaRegistry::new(storage);
//!
//! // Register a schema
//! let schema = r#"{"type": "record", "name": "User", "fields": [{"name": "name", "type": "string"}]}"#;
//! let request = RegisterSchemaRequest {
//!     schema: schema.to_string(),
//!     schema_type: Some(SchemaFormat::Avro),
//!     references: vec![],
//!     metadata: None,
//! };
//!
//! let schema_id = registry.register_schema("users-value", request).await?;
//!
//! // Get schema by ID
//! let schema = registry.get_schema_by_id(schema_id).await?;
//! ```

pub mod api;
pub mod compatibility;
pub mod error;
pub mod registry;
pub mod serde;
pub mod storage;
pub mod types;

pub use api::SchemaRegistryApi;
pub use error::{Result, SchemaError};
pub use registry::SchemaRegistry;
pub use serde::{deserialize_with_schema_id, serialize_with_schema_id};
pub use storage::{MemorySchemaStorage, SchemaStorage};
pub use types::*;
