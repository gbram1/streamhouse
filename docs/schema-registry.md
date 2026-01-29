# StreamHouse Schema Registry

## Overview

The StreamHouse Schema Registry provides centralized schema management, versioning, and validation for data flowing through StreamHouse topics. It ensures type safety, schema evolution, and compatibility checking across producers and consumers.

## Features

- **Multiple Schema Formats**: Avro, Protobuf, JSON Schema
- **Schema Versioning**: Automatic version management per subject
- **Compatibility Checking**: Backward, forward, and full compatibility modes
- **In-Memory Caching**: High-performance schema lookups
- **REST API**: Confluent Schema Registry-compatible HTTP API
- **Schema Evolution**: Managed schema changes without breaking consumers

## Architecture

```
Producer → Schema Registry → Validate → StreamHouse → Consumer → Schema Registry → Deserialize
              ↓                                                      ↓
         Schema ID embedded                                    Fetch schema by ID
         in message header                                     from cache/storage
```

### Components

1. **Schema Storage**: Persists schemas and metadata in metadata store
2. **Compatibility Checker**: Validates schema evolution rules
3. **Schema Cache**: In-memory caching (10,000 schemas, 1 hour TTL)
4. **REST API**: HTTP endpoints for schema management
5. **Validation Service**: Producer/consumer-side validation

## Getting Started

### Starting the Schema Registry Server

```bash
# Using environment variables
export SCHEMA_REGISTRY_PORT=8081
export METADATA_STORE=./data/schema-registry.db

cargo run --bin schema-registry
```

The server will start on `http://localhost:8081`

### Using the Web UI

StreamHouse includes a web-based UI for managing schemas. To use it:

1. **Start the Schema Registry server** (see above)

2. **Start the Web Console**:
   ```bash
   cd web
   npm install
   npm run dev
   ```

3. **Access the UI** at `http://localhost:3000/schemas`

The Schema Registry UI provides:
- **View all subjects** - Browse all registered schema subjects
- **Register new schemas** - Add new schemas with Avro, Protobuf, or JSON format
- **View schema versions** - See all versions for each subject
- **View schema details** - Inspect schema definitions with syntax highlighting
- **Delete subjects** - Remove schemas (use with caution!)

**Environment Configuration**:
Set `NEXT_PUBLIC_SCHEMA_REGISTRY_URL` to point to your schema registry server:
```bash
# In web/.env.local
NEXT_PUBLIC_SCHEMA_REGISTRY_URL=http://localhost:8081
```

### Registering a Schema

```bash
# Register an Avro schema for the "users-value" subject
curl -X POST http://localhost:8081/subjects/users-value/versions \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"User\",\"fields\":[{\"name\":\"name\",\"type\":\"string\"},{\"name\":\"age\",\"type\":\"int\"}]}",
    "schemaType": "AVRO"
  }'

# Response: {"id":1}
```

### Retrieving a Schema

```bash
# Get schema by ID
curl http://localhost:8081/schemas/ids/1

# Get latest schema for subject
curl http://localhost:8081/subjects/users-value/versions/latest

# Get specific version
curl http://localhost:8081/subjects/users-value/versions/1
```

## Schema Formats

### Avro

Avro is the recommended format for high-throughput streaming use cases.

**Example**:
```json
{
  "type": "record",
  "name": "Order",
  "namespace": "com.example",
  "fields": [
    {"name": "orderId", "type": "string"},
    {"name": "amount", "type": "double"},
    {"name": "timestamp", "type": "long"}
  ]
}
```

**Pros**:
- Compact binary format
- Strong schema evolution support
- Built-in compatibility checking

**Cons**:
- Requires schema registry
- Less human-readable than JSON

### Protobuf

Protocol Buffers (Protobuf) is ideal for cross-language compatibility.

**Example**:
```protobuf
syntax = "proto3";

message Order {
  string order_id = 1;
  double amount = 2;
  int64 timestamp = 3;
}
```

**Pros**:
- Widely supported across languages
- Efficient binary format
- Strong typing

**Cons**:
- Requires code generation
- More complex setup

### JSON Schema

JSON Schema provides validation for JSON payloads.

**Example**:
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "orderId": {"type": "string"},
    "amount": {"type": "number"},
    "timestamp": {"type": "integer"}
  },
  "required": ["orderId", "amount"]
}
```

**Pros**:
- Human-readable
- Works with existing JSON payloads
- No code generation needed

**Cons**:
- Larger payload size
- Less compact than Avro/Protobuf

## Compatibility Modes

Schema evolution is controlled by compatibility modes that determine which changes are allowed.

### Backward Compatibility (Default)

New schema can read data written with old schema.

**Allowed changes**:
- Add optional fields (with defaults)
- Delete fields

**Example**:
```json
// Version 1
{"type": "record", "name": "User", "fields": [
  {"name": "name", "type": "string"}
]}

// Version 2 (Backward compatible)
{"type": "record", "name": "User", "fields": [
  {"name": "name", "type": "string"},
  {"name": "age", "type": "int", "default": 0}
]}
```

### Forward Compatibility

Old schema can read data written with new schema.

**Allowed changes**:
- Delete fields
- Add optional fields (old readers ignore them)

### Full Compatibility

Both backward and forward compatible.

**Allowed changes**:
- Add/delete optional fields with defaults

### Transitive Modes

- **BackwardTransitive**: Backward compatible with ALL previous versions
- **ForwardTransitive**: Forward compatible with ALL previous versions
- **FullTransitive**: Full compatibility with ALL previous versions

### None

No compatibility checking (use with caution).

## REST API Reference

### Subjects

```bash
# List all subjects
GET /subjects

# List versions for a subject
GET /subjects/{subject}/versions

# Register a schema
POST /subjects/{subject}/versions
{
  "schema": "<schema_json>",
  "schemaType": "AVRO|PROTOBUF|JSON",
  "references": [],
  "metadata": {}
}

# Get schema by version
GET /subjects/{subject}/versions/{version}

# Delete subject (all versions)
DELETE /subjects/{subject}

# Delete specific version
DELETE /subjects/{subject}/versions/{version}
```

### Schemas

```bash
# Get schema by ID
GET /schemas/ids/{id}
```

### Configuration

```bash
# Get global compatibility mode
GET /config

# Set global compatibility mode
PUT /config
{
  "compatibility": "BACKWARD|FORWARD|FULL|NONE"
}

# Get subject-specific compatibility
GET /config/{subject}

# Set subject-specific compatibility
PUT /config/{subject}
{
  "compatibility": "BACKWARD|FORWARD|FULL|NONE"
}
```

### Health

```bash
# Health check
GET /health
```

## Usage Examples

### Producer with Schema Validation

```rust
use streamhouse_client::Producer;
use streamhouse_schema_registry::{SchemaRegistry, RegisterSchemaRequest, SchemaFormat};

// Create producer
let mut producer = Producer::builder()
    .metadata_store_url("./metadata.db")
    .build()
    .await?;

// Register schema
let schema = r#"{
    "type": "record",
    "name": "User",
    "fields": [
        {"name": "name", "type": "string"},
        {"name": "age", "type": "int"}
    ]
}"#;

let registry = SchemaRegistry::new(storage);
let request = RegisterSchemaRequest {
    schema: schema.to_string(),
    schema_type: Some(SchemaFormat::Avro),
    references: vec![],
    metadata: None,
};

let schema_id = registry.register_schema("users-value", request).await?;

// Send message with schema ID embedded
// (Schema ID is prepended to the payload)
producer.send("users", None, &serialize_with_schema(user, schema_id)?, None).await?;
```

### Consumer with Schema Validation

```rust
use streamhouse_client::Consumer;

let mut consumer = Consumer::builder()
    .metadata_store_url("./metadata.db")
    .group_id("my-consumer-group")
    .topics(vec!["users".to_string()])
    .build()
    .await?;

loop {
    let records = consumer.poll(Duration::from_secs(1)).await?;

    for record in records {
        // Extract schema ID from first 4 bytes
        let schema_id = extract_schema_id(&record.value);

        // Fetch schema from registry
        let schema = registry.get_schema_by_id(schema_id).await?;

        // Deserialize using schema
        let user: User = deserialize_with_schema(&record.value[4..], &schema)?;

        println!("User: {:?}", user);
    }
}
```

## Schema Evolution Examples

### Adding Optional Field

```json
// Version 1
{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"}
  ]
}

// Version 2 (Add optional field with default)
{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"},
    {"name": "email", "type": "string", "default": "unknown@example.com"}
  ]
}
```

### Removing Field

```json
// Version 1
{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"},
    {"name": "age", "type": "int"}
  ]
}

// Version 2 (Remove age field)
{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"}
  ]
}
```

### Type Promotion

Avro supports certain type promotions:

- int → long
- float → double
- string → bytes

```json
// Version 1
{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "int"}
  ]
}

// Version 2 (Promote int to long)
{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "long"}
  ]
}
```

## Best Practices

### 1. Use Meaningful Subject Names

```
<topic>-<key|value>

Examples:
- users-value (value schema for "users" topic)
- orders-key (key schema for "orders" topic)
```

### 2. Always Provide Defaults for Optional Fields

```json
{
  "name": "email",
  "type": "string",
  "default": ""
}
```

### 3. Use Backward Compatibility by Default

Backward compatibility ensures new consumers can read old data, which is the most common requirement.

### 4. Version Schema Carefully

- Test schema changes in development first
- Use schema versioning to track changes
- Document breaking changes

### 5. Cache Schemas Locally

The schema registry includes caching, but clients should also cache schemas by ID to minimize registry lookups.

### 6. Monitor Schema Usage

- Track schema registration rate
- Monitor compatibility check failures
- Alert on schema validation errors

## Configuration

### Server Configuration

```bash
# Port for HTTP API (default: 8081)
export SCHEMA_REGISTRY_PORT=8081

# Metadata store path or URL
export METADATA_STORE=./data/schema-registry.db

# Log level
export RUST_LOG=info
```

### Compatibility Settings

```bash
# Set global compatibility mode
curl -X PUT http://localhost:8081/config \
  -H "Content-Type: application/json" \
  -d '{"compatibility": "BACKWARD"}'

# Set subject-specific compatibility
curl -X PUT http://localhost:8081/config/users-value \
  -H "Content-Type: application/json" \
  -d '{"compatibility": "FULL"}'
```

## Performance

### Caching

The schema registry uses two-level caching:

1. **Schema Cache**: 10,000 schemas, 1 hour TTL
2. **Subject Cache**: 5,000 subjects, 5 minutes TTL

### Metrics

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Register Schema | ~10ms | ~100 req/sec |
| Get Schema (cached) | ~1µs | ~1M req/sec |
| Get Schema (uncached) | ~5ms | ~200 req/sec |
| Compatibility Check | ~1ms | ~1K req/sec |

### Optimization Tips

1. **Batch schema registrations** during deployment
2. **Use schema IDs** instead of full schemas in messages
3. **Enable local caching** in producers/consumers
4. **Pre-register schemas** before high-traffic events

## Troubleshooting

### Error: "Schema not compatible"

**Cause**: New schema violates compatibility rules

**Solution**:
```bash
# Check current compatibility mode
curl http://localhost:8081/config/users-value

# Try changing compatibility mode
curl -X PUT http://localhost:8081/config/users-value \
  -d '{"compatibility": "NONE"}'

# Or fix the schema to be compatible
```

### Error: "Invalid Avro schema"

**Cause**: Schema JSON is malformed or invalid Avro

**Solution**:
- Validate schema syntax
- Use Avro tools to test schema
- Check for missing required fields

### Error: "Subject not found"

**Cause**: No schema registered for subject

**Solution**:
```bash
# Register a schema first
curl -X POST http://localhost:8081/subjects/users-value/versions \
  -d '{"schema": "..."}'
```

## Comparison with Confluent Schema Registry

StreamHouse Schema Registry is API-compatible with Confluent Schema Registry:

| Feature | StreamHouse | Confluent |
|---------|-------------|-----------|
| Avro Support | ✅ | ✅ |
| Protobuf Support | ✅ | ✅ |
| JSON Schema | ✅ | ✅ |
| REST API | ✅ | ✅ |
| Compatibility Checking | ✅ | ✅ |
| Schema Evolution | ✅ | ✅ |
| High Availability | ⏳ Planned | ✅ |
| Multi-DC Replication | ⏳ Planned | ✅ |

## Future Enhancements

- **Schema References**: Support for nested/imported schemas
- **Schema Metadata**: Custom tags and properties
- **Schema Lineage**: Track schema dependencies
- **Schema Validation Rules**: Custom validation logic
- **Multi-Tenant Isolation**: Namespace-based isolation
- **Schema Migration Tools**: Automated schema migration

---

For more information, see:
- [Architecture Documentation](./architecture.md)
- [Producer Documentation](./producer.md)
- [Consumer Documentation](./consumer.md)
