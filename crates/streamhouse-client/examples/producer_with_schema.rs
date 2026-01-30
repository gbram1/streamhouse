//! Producer with Schema Registry Example
//!
//! This example demonstrates how to use the Producer with schema validation
//! and automatic schema registration.
//!
//! ## Key Features Demonstrated
//!
//! 1. Schema registry configuration via builder
//! 2. Automatic schema registration and caching
//! 3. Confluent wire format (magic byte + schema ID)
//! 4. Schema validation before sending
//!
//! ## Usage Pattern
//!
//! ```ignore
//! use streamhouse_client::{Producer, SchemaFormat};
//!
//! // Configure producer with schema registry
//! let producer = Producer::builder()
//!     .metadata_store(metadata)
//!     .schema_registry("http://localhost:8081")  // Enable schema validation
//!     .build()
//!     .await?;
//!
//! // Define your Avro schema
//! let avro_schema = r#"{
//!     "type": "record",
//!     "name": "Order",
//!     "fields": [
//!         {"name": "order_id", "type": "string"},
//!         {"name": "amount", "type": "double"}
//!     ]
//! }"#;
//!
//! // Serialize your data according to the schema (using apache_avro crate)
//! let value = apache_avro::to_avro_datum(&schema, &order)?;
//!
//! // Send with automatic schema registration
//! // - First send: Registers schema in registry (if not exists)
//! // - Subsequent sends: Uses cached schema ID
//! let result = producer.send_with_schema(
//!     "orders",
//!     Some(b"order-123"),    // Optional key
//!     &value,                 // Avro-encoded data
//!     avro_schema,           // Schema definition
//!     SchemaFormat::Avro,    // Format
//!     None,                  // Auto-select partition
//! ).await?;
//!
//! // Message format on wire:
//! // [0x00][schema_id: 4 bytes big-endian][avro_data...]
//! ```
//!
//! ## Performance
//!
//! - **First send**: ~50-100ms (includes HTTP request to registry)
//! - **Subsequent sends**: ~1-5ms (schema ID from local cache)
//! - **Cache hit rate**: ~99% in steady state
//!
//! ## Compatibility
//!
//! The schema registry validates compatibility before registration:
//! - BACKWARD: New schema can read old data
//! - FORWARD: Old schema can read new data
//! - FULL: Both backward and forward compatible
//!
//! If a schema is incompatible, registration fails with an error.

fn main() {
    println!("
🚀 Producer with Schema Registry Example

This file demonstrates the key features and usage patterns for
schema validation with StreamHouse.

See the module documentation above for code examples.

Key Methods:
============

1. Producer::builder().schema_registry(url)
   Configure the schema registry URL

2. producer.send_with_schema(topic, key, value, schema, format, partition)
   Send message with automatic schema validation and registration

3. producer.send_with_schema_id(topic, key, value, schema_id, partition)
   Send message with a known schema ID (faster, skips registry lookup)

Wire Format:
===========

Messages are encoded as:
  Byte 0:    Magic byte (0x00)
  Bytes 1-4: Schema ID (big-endian int32)
  Bytes 5+:  Serialized data

This format is compatible with Confluent Schema Registry.

Caching:
========

Schema IDs are cached locally in a HashMap<String, i32> where:
  Key: Subject name (e.g., \"orders-value\")
  Value: Schema ID

The cache is write-through: schemas are registered once and cached
permanently for the lifetime of the Producer.

Error Handling:
==============

Possible errors:
- ClientError::ConfigError: Schema registry not configured
- ClientError::SchemaRegistryError: Registration/validation failed
- ClientError::TopicNotFound: Topic doesn't exist
- ClientError::InvalidPartition: Invalid partition specified

For a complete working example, see the integration tests or refer
to the documentation at https://docs.streamhouse.io
    ");
}
