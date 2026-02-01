# Runbook: Schema Registry Errors

**Severity**: MEDIUM
**Component**: Schema Registry (HTTP API)
**Impact**: Schema validation failures, producer/consumer incompatibility, message deserialization errors

---

## Symptoms

### Producer Side
- Schema registration fails: `"Failed to register schema: IncompatibleSchema"`
- Schema validation errors: `"Schema not found for subject X"`
- Backward compatibility violations: `"Breaking change detected: removed required field"`

### Consumer Side
- Deserialization failures: `"Schema ID 123 not found"`
- Schema resolution errors: `"Invalid schema format"`
- Unknown schema IDs in messages

### Schema Registry Logs
```
ERROR Schema registration failed: Incompatible schema for subject orders-value
WARN  Schema not found: ID 123 (cache miss)
ERROR Database error: connection timeout acquiring schema
INFO  Compatibility check failed: field 'email' removed (BACKWARD mode)
```

### HTTP API Errors
```bash
# Registration failure
POST /subjects/orders-value/versions
HTTP 409 Conflict: "Schema incompatible with version 2"

# Schema not found
GET /schemas/ids/123
HTTP 404 Not Found: "Schema ID 123 not found"

# Compatibility check failure
POST /compatibility/subjects/orders-value/versions/latest
HTTP 200 OK: {"is_compatible": false}
```

---

## Root Causes

Schema registry errors occur due to:

1. **Incompatible Schema Changes**: Breaking changes violate configured compatibility mode
2. **Missing Schema Registration**: Producer sends messages without registering schema first
3. **Database Connectivity**: PostgreSQL connection failures prevent schema storage/retrieval
4. **Cache Inconsistency**: Moka cache out of sync with database
5. **Invalid Schema Syntax**: Malformed Avro/Protobuf/JSON schema definitions
6. **Schema ID Mismatch**: Consumer receives unknown schema ID (missing registration)
7. **Compatibility Mode Misconfiguration**: Wrong mode (e.g., FULL when only BACKWARD needed)

---

## Investigation Steps

### Step 1: Identify the Failing Subject/Schema

Check error logs for subject and version:
```bash
# Schema registry logs
kubectl logs -n streamhouse deployment/streamhouse-schema-registry --tail=200 | grep -i "error\|failed"

# Look for:
# - Subject name (e.g., "orders-value")
# - Version number (e.g., "version 3")
# - Error type (IncompatibleSchema, SchemaNotFound, etc.)
```

Example error pattern:
```
[2026-01-31T12:00:15Z] ERROR Schema registration failed for orders-value
[2026-01-31T12:00:15Z] ERROR Compatibility check failed: Removed required field 'email' (BACKWARD mode)
[2026-01-31T12:00:15Z] ERROR New schema version: {...}, Latest version: {...}
```

### Step 2: Review Schema Compatibility Mode

Check configured compatibility for subject:
```bash
# Get subject-specific compatibility mode
curl -s http://localhost:8081/config/orders-value | jq '.'

# Expected output:
# {"compatibilityLevel": "BACKWARD"}

# Get global compatibility mode (fallback)
curl -s http://localhost:8081/config | jq '.'
```

Compatibility modes:
- **BACKWARD**: New schema can read old data (safe to add optional fields)
- **FORWARD**: Old schema can read new data (safe to remove optional fields)
- **FULL**: Both backward and forward compatible
- **NONE**: No compatibility checks (risky!)

### Step 3: Compare Schema Versions

Retrieve and compare failing schema versions:
```bash
# Get latest registered version
curl -s http://localhost:8081/subjects/orders-value/versions/latest | jq '.schema | fromjson'

# Get specific version (e.g., version 2)
curl -s http://localhost:8081/subjects/orders-value/versions/2 | jq '.schema | fromjson'

# Check for breaking changes:
# - Removed required fields
# - Changed field types (e.g., int → string)
# - Added new required fields (no default)
```

### Step 4: Test Compatibility Manually

Test if new schema is compatible before registration:
```bash
# POST new schema to compatibility endpoint
curl -X POST http://localhost:8081/compatibility/subjects/orders-value/versions/latest \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"id\",\"type\":\"int\"}]}"
  }'

# Expected responses:
# {"is_compatible": true}  → Safe to register
# {"is_compatible": false} → Breaking change, registration will fail
```

### Step 5: Check Database Health

Verify PostgreSQL connectivity and schema tables:
```bash
# Check schema registry database connection
psql -h YOUR_DB_HOST -U streamhouse -d streamhouse -c "\dt schema_registry*"

# Expected: 4 tables (schemas, versions, subject_config, global_config)

# Check schema count
psql -h YOUR_DB_HOST -U streamhouse -c "
SELECT COUNT(*) AS total_schemas FROM schema_registry_schemas;
SELECT COUNT(*) AS total_versions FROM schema_registry_versions;
"

# Check for orphaned versions (schema_id with no matching schema)
psql -h YOUR_DB_HOST -U streamhouse -c "
SELECT v.subject, v.version
FROM schema_registry_versions v
LEFT JOIN schema_registry_schemas s ON v.schema_id = s.id
WHERE s.id IS NULL;
"
```

### Step 6: Inspect Cache State

Check if cache is out of sync with database:
```bash
# Schema registry exposes cache stats on internal metrics endpoint
curl -s http://localhost:9090/metrics | grep schema_cache

# Look for:
# schema_cache_hits_total
# schema_cache_misses_total
# schema_cache_size

# High miss rate (>20%) may indicate cache issues
```

---

## Resolution

### Option 1: Fix Incompatible Schema (Breaking Changes)

If new schema violates compatibility, fix the schema:

#### Example: Adding Required Field (BACKWARD violation)

**Problem**:
```json
// Old schema (version 2)
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}

// New schema (FAILS - added required field)
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "email", "type": "string"}  // ❌ Required field
  ]
}
```

**Solution**: Add field with default value:
```json
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "email", "type": "string", "default": ""}  // ✅ Optional with default
  ]
}
```

#### Example: Removing Required Field (BACKWARD violation)

**Problem**:
```json
// Old schema (version 2)
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "email", "type": "string"}
  ]
}

// New schema (FAILS - removed required field)
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}
```

**Solution**: Use FORWARD or FULL compatibility mode, or keep field as optional:
```bash
# Change subject compatibility to FORWARD
curl -X PUT http://localhost:8081/config/orders-value \
  -H "Content-Type: application/json" \
  -d '{"compatibility": "FORWARD"}'
```

---

### Option 2: Change Compatibility Mode

If compatibility mode is too strict for your use case:

**Steps**:
```bash
# 1. Understand current mode
curl -s http://localhost:8081/config/orders-value | jq '.compatibilityLevel'

# 2. Choose appropriate mode
# - BACKWARD: Most common (add optional fields, remove optional fields)
# - FORWARD: Rare (consumer schema can evolve independently)
# - FULL: Strictest (both backward and forward)
# - NONE: Dangerous (no checks, allows breaking changes)

# 3. Update subject compatibility
curl -X PUT http://localhost:8081/config/orders-value \
  -H "Content-Type: application/json" \
  -d '{"compatibility": "FULL"}'

# 4. Retry schema registration
curl -X POST http://localhost:8081/subjects/orders-value/versions \
  -H "Content-Type: application/json" \
  -d '{"schema": "..."}'
```

**Trade-offs**:
- **BACKWARD** (recommended): Consumers can read old and new data
- **FORWARD**: Producers can evolve schema independently
- **FULL**: Maximum safety, but restrictive (hard to evolve schema)
- **NONE**: No safety, risk of deserialization failures

---

### Option 3: Force Register Schema (Emergency Override)

If schema must be registered despite compatibility violations:

**⚠️ Warning**: This **bypasses compatibility checks** and can break consumers. Only use if:
- You control all producers and consumers
- You can redeploy all consumers immediately with new schema
- Acceptable to cause temporary deserialization failures

**Steps**:
```bash
# 1. Temporarily disable compatibility for subject
curl -X PUT http://localhost:8081/config/orders-value \
  -H "Content-Type: application/json" \
  -d '{"compatibility": "NONE"}'

# 2. Register incompatible schema
curl -X POST http://localhost:8081/subjects/orders-value/versions \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Order\",\"fields\":[...]}"
  }'

# 3. Re-enable compatibility checks
curl -X PUT http://localhost:8081/config/orders-value \
  -H "Content-Type: application/json" \
  -d '{"compatibility": "BACKWARD"}'

# 4. Immediately redeploy consumers with new schema
kubectl rollout restart deployment/consumer-app
```

**Impact**: Existing consumers may crash until redeployed

---

### Option 4: Clear Schema Cache (Cache Inconsistency)

If cache is serving stale schemas:

**Steps**:
```bash
# 1. Restart schema registry pod (clears in-memory cache)
kubectl rollout restart -n streamhouse deployment/streamhouse-schema-registry

# 2. Wait for restart
kubectl rollout status -n streamhouse deployment/streamhouse-schema-registry

# 3. Verify cache is fresh
# First request should be cache miss (loads from DB)
curl -s http://localhost:8081/schemas/ids/123

# Second request should be cache hit (faster)
curl -s http://localhost:8081/schemas/ids/123
```

**Alternative**: Implement cache invalidation API (Phase 13 enhancement)

---

### Option 5: Fix Database Connectivity

If database errors prevent schema storage/retrieval:

**Steps**:
```bash
# 1. Check PostgreSQL connectivity
psql -h YOUR_DB_HOST -U streamhouse -d streamhouse -c "SELECT 1;"

# 2. Check connection pool exhaustion
psql -h YOUR_DB_HOST -U streamhouse -c "
SELECT count(*), state
FROM pg_stat_activity
WHERE datname = 'streamhouse'
GROUP BY state;
"

# If active connections > max_connections → increase pool size

# 3. Restart schema registry with increased pool
# In schema registry configuration:
PgPoolOptions::new()
    .max_connections(20)  // Increase from 10
    .connect(database_url)
    .await?;

# 4. Redeploy
kubectl rollout restart -n streamhouse deployment/streamhouse-schema-registry
```

---

### Option 6: Delete and Re-Register Subject (Nuclear Option)

If subject is completely broken (corrupted schemas, orphaned versions):

**⚠️ Warning**: This **deletes all schema history** for the subject. Only use if:
- Subject is test/development (not production)
- You have backups of all schema versions
- Acceptable to lose schema version history

**Steps**:
```bash
# 1. Backup subject schemas
curl -s http://localhost:8081/subjects/orders-value/versions | jq '.' > backup-orders-value.json

# 2. Delete subject (soft delete)
curl -X DELETE http://localhost:8081/subjects/orders-value

# Expected: [1, 2, 3] (deleted version IDs)

# 3. Permanent delete (hard delete)
curl -X DELETE "http://localhost:8081/subjects/orders-value?permanent=true"

# 4. Re-register schema from scratch
curl -X POST http://localhost:8081/subjects/orders-value/versions \
  -H "Content-Type: application/json" \
  -d '{"schema": "..."}'

# New schema will get version 1 (fresh start)
```

---

## Verification

After resolution, verify schema registry is healthy:

### 1. Test Schema Registration
```bash
# Register test schema
curl -X POST http://localhost:8081/subjects/test-subject/versions \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Test\",\"fields\":[{\"name\":\"id\",\"type\":\"int\"}]}"
  }'

# Expected: {"id": 1}
```

### 2. Test Schema Retrieval
```bash
# Get schema by ID
curl -s http://localhost:8081/schemas/ids/1 | jq '.'

# Expected: Schema definition returned
```

### 3. Test Compatibility Check
```bash
# Test compatible change (add optional field)
curl -X POST http://localhost:8081/compatibility/subjects/test-subject/versions/latest \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Test\",\"fields\":[{\"name\":\"id\",\"type\":\"int\"},{\"name\":\"name\",\"type\":\"string\",\"default\":\"\"}]}"
  }'

# Expected: {"is_compatible": true}
```

### 4. Monitor Producer/Consumer
```bash
# Producer should register schemas successfully
cargo run --example producer_with_schema

# Consumer should resolve schemas
cargo run --example consumer_with_schema

# No schema errors in logs
kubectl logs -n streamhouse deployment/producer-app | grep -i schema
```

---

## Prevention

### 1. Enforce Compatibility Checks in CI/CD

Validate schema changes before deployment:
```bash
# In CI pipeline (e.g., GitHub Actions)
- name: Test schema compatibility
  run: |
    # Register test schema in staging registry
    SCHEMA=$(cat schemas/orders.avsc)
    curl -X POST http://staging-registry:8081/compatibility/subjects/orders-value/versions/latest \
      -H "Content-Type: application/json" \
      -d "{\"schema\": \"$SCHEMA\"}"

    # Fail build if incompatible
    if [ $? -ne 0 ]; then
      echo "Schema compatibility check failed!"
      exit 1
    fi
```

### 2. Use Schema Evolution Best Practices

**Safe Changes (BACKWARD compatible)**:
- ✅ Add optional fields (with default values)
- ✅ Remove optional fields
- ✅ Add enum values (at end)
- ✅ Change field documentation

**Unsafe Changes (BACKWARD incompatible)**:
- ❌ Remove required fields
- ❌ Add required fields (no default)
- ❌ Change field types
- ❌ Rename fields

### 3. Version Control Schemas

Track schema evolution in Git:
```
schemas/
  orders-v1.avsc
  orders-v2.avsc
  orders-v3.avsc
  CHANGELOG.md
```

### 4. Set Up Schema Registry Monitoring

Alert on schema errors:
```yaml
# Prometheus alert rules
- alert: SchemaRegistrationFailures
  expr: rate(schema_registry_errors_total{type="registration"}[5m]) > 0.01
  for: 2m
  annotations:
    summary: "Schema registration failures detected"

- alert: SchemaNotFoundErrors
  expr: rate(schema_registry_errors_total{type="not_found"}[5m]) > 0.05
  for: 2m
  annotations:
    summary: "High rate of schema not found errors"
```

### 5. Implement Schema Validation in Unit Tests

Catch schema issues before production:
```rust
#[test]
fn test_schema_backward_compatibility() {
    let old_schema = r#"{"type":"record","name":"Order","fields":[{"name":"id","type":"int"}]}"#;
    let new_schema = r#"{"type":"record","name":"Order","fields":[{"name":"id","type":"int"},{"name":"email","type":"string","default":""}]}"#;

    let checker = AvroCompatibilityChecker::new();
    let result = checker.check_backward_compatibility(old_schema, new_schema);

    assert!(result.is_ok(), "Schema should be backward compatible");
}
```

### 6. Document Schema Evolution Guidelines

Create team guidelines:
```markdown
# Schema Evolution Guidelines

## DO
- Always add optional fields (with defaults)
- Version schemas (orders-v1, orders-v2)
- Test compatibility before merging
- Document breaking changes

## DON'T
- Remove required fields without migration plan
- Change field types
- Add required fields without defaults
- Bypass compatibility checks in production
```

---

## Related Runbooks

- [Circuit Breaker Open](./circuit-breaker-open.md) - If database failures affect schema registry
- [Resource Pressure](./resource-pressure.md) - Memory pressure affects schema cache

---

## Common Error Codes

| HTTP Code | Error | Cause | Resolution |
|-----------|-------|-------|------------|
| 404 | Schema not found | Schema ID not registered | Check producer registered schema |
| 409 | Incompatible schema | Breaking change detected | Fix schema or change compatibility mode |
| 422 | Invalid schema | Malformed Avro/Protobuf syntax | Validate schema syntax |
| 500 | Internal server error | Database connectivity issue | Check PostgreSQL health |
| 503 | Service unavailable | Schema registry down | Restart schema registry pod |

---

## Schema Format Examples

### Valid Avro Schema
```json
{
  "type": "record",
  "name": "Order",
  "namespace": "com.example",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "email", "type": "string", "default": ""},
    {"name": "amount", "type": "double"},
    {"name": "status", "type": {"type": "enum", "name": "Status", "symbols": ["PENDING", "COMPLETED"]}}
  ]
}
```

### Invalid Avro Schema (Common Mistakes)
```json
// ❌ Missing "type" field
{
  "name": "Order",
  "fields": [...]
}

// ❌ Required field without default
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "email", "type": "string"}  // Should have "default": ""
  ]
}

// ❌ Invalid field type
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "integer"}  // Should be "int", not "integer"
  ]
}
```

---

## References

- **Code**: [schema-registry/](../../crates/streamhouse-schema-registry/src/)
- **API Docs**: [REST API](../../crates/streamhouse-schema-registry/src/api.rs)
- **Compatibility**: [Avro Spec - Schema Evolution](https://avro.apache.org/docs/current/spec.html#Schema+Resolution)

---

**Last Updated**: January 31, 2026
**Version**: 1.0
