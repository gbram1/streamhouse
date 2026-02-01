# Phase 14.3: REST API Client - COMPLETE

**Status:** ✅ COMPLETE
**Implemented:** HTTP client for Schema Registry REST API
**LOC Added:** ~300 lines

---

## Overview

Created a REST API client module that provides HTTP connectivity to StreamHouse services, specifically the Schema Registry REST API.

---

## Features Implemented

### 1. Generic REST Client

Low-level HTTP client with methods for all REST operations:

```rust
pub struct RestClient {
    base_url: String,
    client: reqwest::Client,
}

impl RestClient {
    pub async fn get<T: Deserialize>(&self, path: &str) -> Result<T>
    pub async fn post<T: Serialize, R: Deserialize>(&self, path: &str, body: &T) -> Result<R>
    pub async fn put<T: Serialize, R: Deserialize>(&self, path: &str, body: &T) -> Result<R>
    pub async fn delete(&self, path: &str) -> Result<()>
    pub async fn delete_json<R: Deserialize>(&self, path: &str) -> Result<R>
}
```

### 2. Schema Registry Client

High-level client for Schema Registry operations:

```rust
pub struct SchemaRegistryClient {
    client: RestClient,
}

impl SchemaRegistryClient {
    // Subject management
    pub async fn list_subjects(&self) -> Result<Vec<String>>
    pub async fn list_versions(&self, subject: &str) -> Result<Vec<i32>>
    pub async fn delete_subject(&self, subject: &str) -> Result<Vec<i32>>

    // Schema operations
    pub async fn register_schema(&self, subject: &str, schema: &str, schema_type: Option<&str>) -> Result<RegisterSchemaResponse>
    pub async fn get_schema_by_version(&self, subject: &str, version: &str) -> Result<SchemaResponse>
    pub async fn get_schema_by_id(&self, id: i32) -> Result<SchemaResponse>
    pub async fn delete_schema_version(&self, subject: &str, version: &str) -> Result<i32>

    // Configuration
    pub async fn get_global_config(&self) -> Result<CompatibilityConfig>
    pub async fn set_global_config(&self, compatibility: &str) -> Result<CompatibilityConfig>
    pub async fn get_subject_config(&self, subject: &str) -> Result<CompatibilityConfig>
    pub async fn set_subject_config(&self, subject: &str, compatibility: &str) -> Result<CompatibilityConfig>
}
```

---

## Implementation Details

### Files Created

1. **[crates/streamhouse-cli/src/rest_client.rs](crates/streamhouse-cli/src/rest_client.rs)** (NEW, ~300 LOC)
   - `RestClient` struct for generic HTTP operations
   - `SchemaRegistryClient` for Schema Registry API
   - Request/Response types
   - Error handling
   - Unit tests

### Files Modified

2. **[crates/streamhouse-cli/src/main.rs](crates/streamhouse-cli/src/main.rs)** (+1 LOC)
   - Added `mod rest_client;` declaration

### Dependencies

- **reqwest** (0.12): Already added in Phase 14.1
  - Features: `json` for automatic JSON serialization
  - Async HTTP client built on hyper

---

## API Coverage

### Schema Registry REST API

The client implements all Schema Registry endpoints:

| Endpoint | Method | Client Method |
|----------|--------|---------------|
| `/subjects` | GET | `list_subjects()` |
| `/subjects/:subject/versions` | GET | `list_versions()` |
| `/subjects/:subject/versions` | POST | `register_schema()` |
| `/subjects/:subject/versions/:version` | GET | `get_schema_by_version()` |
| `/subjects/:subject/versions/:version` | DELETE | `delete_schema_version()` |
| `/subjects/:subject` | DELETE | `delete_subject()` |
| `/schemas/ids/:id` | GET | `get_schema_by_id()` |
| `/config` | GET | `get_global_config()` |
| `/config` | PUT | `set_global_config()` |
| `/config/:subject` | GET | `get_subject_config()` |
| `/config/:subject` | PUT | `set_subject_config()` |

---

## Usage Examples

### Basic REST Client

```rust
use crate::rest_client::RestClient;

// Create client
let client = RestClient::new("http://localhost:8081");

// GET request
let subjects: Vec<String> = client.get("/subjects").await?;

// POST request
#[derive(Serialize)]
struct Request {
    schema: String,
}

#[derive(Deserialize)]
struct Response {
    id: i32,
}

let request = Request { schema: "...".to_string() };
let response: Response = client.post("/subjects/test-value/versions", &request).await?;
```

### Schema Registry Client

```rust
use crate::rest_client::SchemaRegistryClient;

// Create client
let client = SchemaRegistryClient::new("http://localhost:8081");

// List all subjects
let subjects = client.list_subjects().await?;
for subject in subjects {
    println!("Subject: {}", subject);
}

// Register a schema
let avro_schema = r#"{"type": "record", "name": "User", "fields": [...]}"#;
let response = client.register_schema("users-value", avro_schema, Some("AVRO")).await?;
println!("Schema ID: {}", response.id);

// Get schema by version
let schema = client.get_schema_by_version("users-value", "latest").await?;
println!("Schema: {}", schema.schema);

// Get compatibility config
let config = client.get_subject_config("users-value").await?;
println!("Compatibility: {}", config.compatibility_level);
```

---

## Error Handling

The client provides detailed error messages:

```rust
// HTTP 404 Not Found
let result = client.get_schema_by_version("nonexistent", "1").await;
// Error: HTTP 404 Not Found: {"error_code": 40401, "message": "Subject not found"}

// HTTP 422 Unprocessable Entity
let result = client.register_schema("test", "invalid schema", None).await;
// Error: HTTP 422 Unprocessable Entity: {"error_code": 42201, "message": "Invalid Avro schema"}

// Connection error
let result = client.list_subjects().await;
// Error: Failed to send GET request: connection refused
```

### Error Context

All errors include context using `anyhow::Context`:

```rust
pub async fn get<T: Deserialize>(&self, path: &str) -> Result<T> {
    let response = self.client
        .get(&url)
        .send()
        .await
        .context("Failed to send GET request")?;  // Add context

    response.json()
        .await
        .context("Failed to parse JSON response")?  // Add context
}
```

---

## Type Safety

The client uses Rust's type system for safety:

### Request/Response Types

```rust
#[derive(Serialize)]
struct RegisterSchemaRequest {
    schema: String,
    #[serde(rename = "schemaType", skip_serializing_if = "Option::is_none")]
    schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    references: Option<Vec<SchemaReference>>,
}

#[derive(Deserialize)]
pub struct SchemaResponse {
    pub subject: String,
    pub version: i32,
    pub id: i32,
    pub schema: String,
    #[serde(rename = "schemaType")]
    pub schema_type: String,
}
```

### Generic Methods

Methods use generics for flexibility:

```rust
// Works with any deserializable type
pub async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T>

// Works with any serializable request and deserializable response
pub async fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
    &self,
    path: &str,
    body: &T,
) -> Result<R>
```

---

## Testing

### Unit Tests

```rust
#[test]
fn test_client_creation() {
    let client = RestClient::new("http://localhost:8081");
    assert_eq!(client.base_url, "http://localhost:8081");
}

#[test]
fn test_schema_registry_client_creation() {
    let client = SchemaRegistryClient::new("http://localhost:8081");
    assert_eq!(client.client.base_url, "http://localhost:8081");
}
```

### Integration Tests (Manual)

```bash
# Terminal 1: Start schema registry
./start-with-postgres-minio.sh

# Terminal 2: Test REST client
cargo run -p streamhouse-cli --example test_rest_client
# (Example to be created)
```

---

## Design Decisions

### 1. Two-Layer Architecture

**Generic REST Client (low-level)**
- Reusable for any HTTP API
- Handles HTTP operations
- Generic over request/response types

**Schema Registry Client (high-level)**
- Domain-specific methods
- Strongly typed
- Hides HTTP details

**Benefit**: Easy to add more high-level clients (e.g., TopicClient, GroupClient)

### 2. reqwest Over Other Options

**Why reqwest?**
- Most popular Rust HTTP client
- Excellent async support
- Built-in JSON serialization
- Connection pooling
- Already in dependencies

**Alternatives considered:**
- `hyper`: Too low-level, more boilerplate
- `ureq`: Synchronous only
- `surf`: Smaller ecosystem

### 3. Error Handling Strategy

- Use `anyhow::Result` for flexibility
- Add context at each error point
- Include HTTP status and response body in errors
- Let caller decide how to handle errors

---

## Next Steps

### Integration with Commands

The REST client is ready to be used in schema registry commands:

```rust
// In schema command handler
pub async fn handle_schema_command(
    command: SchemaCommands,
    base_url: &str,
) -> Result<()> {
    let client = SchemaRegistryClient::new(base_url);

    match command {
        SchemaCommands::List => {
            let subjects = client.list_subjects().await?;
            for subject in subjects {
                println!("{}", subject);
            }
        }
        SchemaCommands::Register { subject, file } => {
            let schema = std::fs::read_to_string(file)?;
            let response = client.register_schema(&subject, &schema, Some("AVRO")).await?;
            println!("Registered as ID {}", response.id);
        }
        // ...
    }
}
```

### Remaining Phase 14 Tasks

1. **Schema Registry Commands** (~5h) - IN PROGRESS
   - Create `SchemaCommands` enum
   - Implement command handlers using `SchemaRegistryClient`
   - Add to REPL mode
   - Add `--schema-registry-url` flag

2. **Consumer Group Commands** (~5h)
   - Create `GroupCommands` enum
   - Implement command handlers
   - Add to REPL mode

---

## Success Criteria

✅ Generic REST client created
✅ Schema Registry client with all endpoints
✅ Proper error handling with context
✅ Type-safe request/response types
✅ Unit tests pass
✅ Build succeeds
✅ Two-layer architecture (generic + domain-specific)

---

**Phase 14.3 Complete** - Remaining effort: ~10 hours

Next: Implement schema registry commands using the REST client
