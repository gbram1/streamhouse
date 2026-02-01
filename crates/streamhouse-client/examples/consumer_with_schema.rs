//! Consumer with Schema Registry Example
//!
//! This example demonstrates how to use the Consumer with automatic schema
//! resolution from the Schema Registry.
//!
//! ## Key Features Demonstrated
//!
//! 1. Schema registry configuration via builder
//! 2. Automatic schema resolution from message schema ID
//! 3. Schema caching for performance
//! 4. Deserialization of schema-encoded messages
//!
//! ## Usage Pattern
//!
//! ```ignore
//! use streamhouse_client::{Consumer, OffsetReset};
//! use std::time::Duration;
//!
//! // Configure consumer with schema registry
//! let consumer = Consumer::builder()
//!     .group_id("order-processors")
//!     .topics(vec!["orders".to_string()])
//!     .metadata_store(metadata)
//!     .object_store(object_store)
//!     .schema_registry("http://localhost:8081")  // Enable schema resolution
//!     .offset_reset(OffsetReset::Earliest)
//!     .build()
//!     .await?;
//!
//! // Poll for messages
//! loop {
//!     let records = consumer.poll(Duration::from_secs(1)).await?;
//!
//!     for record in records {
//!         // Schema is automatically resolved if message has schema ID
//!         if let Some(schema) = &record.schema {
//!             println!("Subject: {}", schema.subject);
//!             println!("Version: {}", schema.version);
//!             println!("Schema ID: {}", schema.id);
//!             println!("Format: {:?}", schema.schema_type);
//!
//!             // Deserialize using the schema
//!             match schema.schema_type {
//!                 SchemaFormat::Avro => {
//!                     // Parse Avro schema
//!                     let avro_schema = apache_avro::Schema::parse_str(&schema.schema)?;
//!
//!                     // Deserialize the value
//!                     let value = apache_avro::from_avro_datum(
//!                         &avro_schema,
//!                         &mut record.value.as_ref(),
//!                         None,
//!                     )?;
//!
//!                     println!("Deserialized value: {:?}", value);
//!                 }
//!                 SchemaFormat::Protobuf => {
//!                     // Deserialize protobuf message
//!                     // let msg = MyMessage::decode(record.value.as_ref())?;
//!                 }
//!                 SchemaFormat::Json => {
//!                     // Validate against JSON schema
//!                     // let value: serde_json::Value = serde_json::from_slice(&record.value)?;
//!                 }
//!             }
//!         } else {
//!             // Message doesn't have schema (legacy format or no schema validation)
//!             println!("Raw message: {:?}", record.value);
//!         }
//!     }
//!
//!     // Commit offsets
//!     consumer.commit().await?;
//! }
//! ```
//!
//! ## Wire Format
//!
//! Messages from producers using `send_with_schema()` are encoded as:
//! - Byte 0: Magic byte (0x00)
//! - Bytes 1-4: Schema ID (big-endian int32)
//! - Bytes 5+: Serialized data
//!
//! The Consumer automatically:
//! 1. Detects the magic byte (0x00)
//! 2. Extracts the schema ID (bytes 1-4)
//! 3. Looks up the schema (from cache or registry)
//! 4. Populates `record.schema_id` and `record.schema`
//! 5. Strips the magic byte + schema ID from `record.value`
//!
//! ## Performance
//!
//! - **First message with schema**: ~50-100ms (HTTP request to registry)
//! - **Subsequent messages**: ~1ms (schema from local cache)
//! - **Cache hit rate**: ~99%+ in steady state
//!
//! ## Schema Cache
//!
//! Schemas are cached in a `HashMap<i32, Schema>` where:
//!   Key: Schema ID
//!   Value: Schema metadata + definition
//!
//! The cache is read-through: schemas are fetched once and cached
//! permanently for the lifetime of the Consumer.
//!
//! ## Error Handling
//!
//! - If schema registry is unreachable, the message is still returned
//!   but `record.schema` will be `None`
//! - If schema ID is not found, an error is logged but poll continues
//! - Malformed messages (invalid magic byte) are treated as unencoded
//!
//! ## Backward Compatibility
//!
//! Consumers work with both schema-encoded and plain messages:
//! - Schema-encoded: `record.schema` is populated
//! - Plain messages: `record.schema` is None
//!
//! This allows gradual migration to schema validation without breaking
//! existing producers and consumers.

fn main() {
    println!("
🔍 Consumer with Schema Registry Example

This file demonstrates how to consume messages with automatic schema
resolution from the StreamHouse Schema Registry.

See the module documentation above for code examples.

Key Methods:
============

1. Consumer::builder().schema_registry(url)
   Configure the schema registry URL

2. consumer.poll(timeout)
   Poll for messages with automatic schema resolution
   - Extracts schema ID from message
   - Resolves schema from registry or cache
   - Populates record.schema and record.schema_id fields

3. record.schema
   Access the resolved schema metadata:
   - subject: Subject name (e.g., \"orders-value\")
   - version: Schema version number
   - id: Schema ID
   - schema_type: Format (Avro, Protobuf, Json)
   - schema: Schema definition string

Message Processing Flow:
=======================

1. Consumer polls from partitions
2. For each message:
   a. Check first byte for magic byte (0x00)
   b. If present, extract schema ID (bytes 1-4)
   c. Look up schema in cache
   d. If not cached, fetch from registry
   e. Cache schema for future use
   f. Strip magic byte + schema ID from value
   g. Populate record.schema and record.schema_id

3. User deserializes using the schema

Deserialization Examples:
========================

Avro:
-----
let avro_schema = apache_avro::Schema::parse_str(&record.schema.unwrap().schema)?;
let value = apache_avro::from_avro_datum(&avro_schema, &mut record.value.as_ref(), None)?;

Protobuf:
---------
let msg = MyMessage::decode(record.value.as_ref())?;

JSON:
-----
let value: serde_json::Value = serde_json::from_slice(&record.value)?;
// Validate against JSON schema using record.schema

Error Handling:
==============

- If schema registry is unavailable, poll() continues but record.schema is None
- If schema ID not found, error logged, poll() continues
- If message is malformed, treated as unencoded (no schema)

For complete working examples, see the integration tests.
    ");
}
