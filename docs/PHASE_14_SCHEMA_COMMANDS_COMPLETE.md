# Phase 14.4: Schema Registry Commands - COMPLETE

**Status:** ✅ COMPLETE
**Implemented:** Full suite of schema registry CLI commands
**LOC Added:** ~400 lines

---

## Overview

The StreamHouse CLI now supports complete schema registry operations through intuitive commands for managing schemas, subjects, versions, and compatibility configurations.

---

## Commands Implemented

### 1. List Subjects

```bash
$ streamctl schema list
Subjects (2):
  orders-value
  events-value
```

### 2. Register Schema

```bash
$ streamctl schema register orders-value schema.avsc
✅ Schema registered:
  Subject: orders-value
  Schema ID: 1
  Type: AVRO

$ streamctl schema register users-value schema.proto --schema-type PROTOBUF
✅ Schema registered:
  Subject: users-value
  Schema ID: 2
  Type: PROTOBUF
```

### 3. Get Schema

**By subject and version:**
```bash
$ streamctl schema get orders-value
Schema Details:
  Subject: orders-value
  Version: 1
  ID: 1
  Type: AVRO

Schema:
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "amount", "type": "double"}
  ]
}

$ streamctl schema get orders-value --version 2
# Gets specific version

$ streamctl schema get orders-value --version latest
# Gets latest version (default)
```

**By schema ID:**
```bash
$ streamctl schema get --id 1
Schema Details:
  Subject: orders-value
  Version: 1
  ID: 1
  Type: AVRO
  ...
```

### 4. Check Compatibility

```bash
$ streamctl schema check orders-value new-schema.avsc
Checking compatibility for subject: orders-value
Current version: 1
Compatibility mode: BACKWARD

✓ Schema is valid
⚠️  Full compatibility check not yet implemented
   (Evolution will still fail if incompatible)
```

### 5. Evolve Schema

```bash
$ streamctl schema evolve orders-value new-schema.avsc
Checking compatibility...
✓ Schema is valid

Registering new schema...
✅ Schema registered:
  Subject: orders-value
  Schema ID: 2
  Type: AVRO
```

### 6. Delete Schema

**Delete specific version:**
```bash
$ streamctl schema delete orders-value --version 2
✅ Schema version deleted:
  Subject: orders-value
  Version: 2
```

**Delete entire subject:**
```bash
$ streamctl schema delete orders-value
✅ Subject deleted:
  Subject: orders-value
  Versions deleted: [1, 2, 3]
```

### 7. Configuration Management

**Get global config:**
```bash
$ streamctl schema config get
Global compatibility: BACKWARD
```

**Get subject-specific config:**
```bash
$ streamctl schema config get orders-value
Subject: orders-value
Compatibility: FULL
```

**Set compatibility mode:**
```bash
$ streamctl schema config set orders-value --compatibility FULL
✅ Compatibility updated:
  Subject: orders-value
  Compatibility: FULL
```

---

## Implementation Details

### Files Created

1. **[crates/streamhouse-cli/src/commands/mod.rs](crates/streamhouse-cli/src/commands/mod.rs)** (NEW, ~10 LOC)
   - Module declaration
   - Re-exports

2. **[crates/streamhouse-cli/src/commands/schema.rs](crates/streamhouse-cli/src/commands/schema.rs)** (NEW, ~400 LOC)
   - `SchemaCommands` enum
   - `ConfigCommands` enum
   - `handle_schema_command()` dispatcher
   - 8 command handlers
   - Schema pretty-printing

### Files Modified

3. **[crates/streamhouse-cli/src/main.rs](crates/streamhouse-cli/src/main.rs)** (~15 LOC changes)
   - Added `mod commands;`
   - Added `--schema-registry-url` flag
   - Added `Schema` command variant
   - Added schema command handler in match

---

## Command Structure

```
streamctl schema
├── list                                    # List all subjects
├── register <subject> <schema-file>        # Register new schema
│   └── [--schema-type AVRO|PROTOBUF|JSON]
├── get [<subject>] [--version V]           # Get schema
│   └── [--id ID]                          # Or get by ID
├── check <subject> <schema-file>           # Check compatibility
├── evolve <subject> <schema-file>          # Check + register
│   └── [--schema-type AVRO|PROTOBUF|JSON]
├── delete <subject> [--version V]          # Delete version/subject
└── config
    ├── get [<subject>]                    # Get config
    └── set <subject> --compatibility MODE  # Set config
```

---

## Usage Examples

### Complete Workflow

```bash
# 1. Start with empty registry
$ streamctl schema list
No subjects found

# 2. Register initial schema
$ cat > order-v1.avsc <<EOF
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "amount", "type": "double"}
  ]
}
EOF

$ streamctl schema register orders-value order-v1.avsc
✅ Schema registered:
  Subject: orders-value
  Schema ID: 1
  Type: AVRO

# 3. View registered schema
$ streamctl schema get orders-value
Schema Details:
  Subject: orders-value
  Version: 1
  ID: 1
  Type: AVRO

# 4. Set compatibility mode
$ streamctl schema config set orders-value --compatibility FULL
✅ Compatibility updated:
  Subject: orders-value
  Compatibility: FULL

# 5. Evolve schema (add optional field)
$ cat > order-v2.avsc <<EOF
{
  "type": "record",
  "name": "Order",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "amount", "type": "double"},
    {"name": "customer", "type": ["null", "string"], "default": null}
  ]
}
EOF

$ streamctl schema evolve orders-value order-v2.avsc
Checking compatibility...
✓ Schema is valid

Registering new schema...
✅ Schema registered:
  Subject: orders-value
  Schema ID: 2
  Type: AVRO

# 6. List subjects
$ streamctl schema list
Subjects (1):
  orders-value

# 7. Clean up
$ streamctl schema delete orders-value
✅ Subject deleted:
  Subject: orders-value
  Versions deleted: [1, 2]
```

### Environment Variables

```bash
# Set default schema registry URL
export SCHEMA_REGISTRY_URL=http://prod-registry:8081

# Now all commands use production registry
streamctl schema list

# Override for specific command
streamctl --schema-registry-url http://localhost:8081 schema list
```

---

## Schema File Formats

### Avro Schema

```json
{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "name", "type": "string"},
    {"name": "email", "type": ["null", "string"], "default": null}
  ]
}
```

### Protobuf Schema

```protobuf
syntax = "proto3";

package com.example;

message User {
  int32 id = 1;
  string name = 2;
  string email = 3;
}
```

### JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "id": {"type": "integer"},
    "name": {"type": "string"},
    "email": {"type": "string"}
  },
  "required": ["id", "name"]
}
```

---

## Compatibility Modes

| Mode | Description |
|------|-------------|
| `BACKWARD` | New schema can read data written with old schema |
| `FORWARD` | Old schema can read data written with new schema |
| `FULL` | Both backward and forward compatible |
| `BACKWARD_TRANSITIVE` | Backward compatible with ALL previous versions |
| `FORWARD_TRANSITIVE` | Forward compatible with ALL previous versions |
| `FULL_TRANSITIVE` | Full compatibility with ALL previous versions |
| `NONE` | No compatibility checking |

---

## Error Handling

### Subject Not Found

```bash
$ streamctl schema get nonexistent
Error: HTTP 404 Not Found: {"error_code":40401,"message":"Subject not found"}
```

### Invalid Schema

```bash
$ streamctl schema register orders-value invalid.json
Error: HTTP 422 Unprocessable Entity: {"error_code":42201,"message":"Invalid Avro schema"}
```

### Compatibility Violation

```bash
$ streamctl schema register orders-value breaking-change.avsc
Error: HTTP 409 Conflict: {"error_code":409,"message":"Schema not compatible"}
```

### File Not Found

```bash
$ streamctl schema register orders-value missing.avsc
Error: Failed to read schema file: missing.avsc: No such file or directory
```

---

## Integration with REPL

All schema commands work in REPL mode:

```bash
$ streamctl
StreamHouse Interactive Shell
Type 'help' for available commands, 'exit' or Ctrl+D to quit

streamhouse> schema list
Subjects (2):
  orders-value
  events-value

streamhouse> schema get orders-value
Schema Details:
  Subject: orders-value
  Version: 1
  ...

streamhouse> exit
Goodbye!
```

---

## Limitations & Future Enhancements

### Current Limitations

1. **Compatibility Checking**: `check` command validates JSON syntax but doesn't perform full Avro/Protobuf compatibility checking
   - Server-side checking still works during `register`
   - Future: Implement client-side Avro/Protobuf compatibility validation

2. **Schema References**: Not supported in CLI (advanced feature)
   - Future: Add `--reference` flag for nested schemas

3. **Metadata**: Not supported in CLI
   - Future: Add `--metadata` flag for schema metadata

### Future Enhancements

1. **Auto-completion**
   - Tab-complete subject names
   - Tab-complete schema types

2. **Schema Diff**
   - `streamctl schema diff orders-value 1 2`
   - Show differences between versions

3. **Schema Validation**
   - `streamctl schema validate data.json --schema orders-value`
   - Validate data against schema

4. **Schema Generation**
   - `streamctl schema generate --from-json sample.json`
   - Generate schema from sample data

---

## Testing

### Manual Testing

```bash
# Terminal 1: Start services
./start-with-postgres-minio.sh

# Terminal 2: Test schema commands
cargo run -p streamhouse-cli -- schema list
cargo run -p streamhouse-cli -- schema register test-value examples/test-schema.avsc
cargo run -p streamhouse-cli -- schema get test-value
cargo run -p streamhouse-cli -- schema delete test-value
```

### Integration Test Script

```bash
#!/bin/bash
set -e

echo "Testing schema commands..."

# Create test schema
cat > /tmp/test-schema.avsc <<EOF
{"type": "record", "name": "Test", "fields": [{"name": "id", "type": "int"}]}
EOF

# Register
streamctl schema register test-value /tmp/test-schema.avsc
echo "✓ Register passed"

# List
streamctl schema list | grep test-value
echo "✓ List passed"

# Get
streamctl schema get test-value | grep "Schema Details"
echo "✓ Get passed"

# Delete
streamctl schema delete test-value
echo "✓ Delete passed"

# Cleanup
rm /tmp/test-schema.avsc

echo "All tests passed!"
```

---

## Success Criteria

✅ All 7 main commands implemented (list, register, get, check, evolve, delete, config)
✅ Config subcommands (get, set) implemented
✅ Schema file reading from disk
✅ JSON schema pretty-printing
✅ Error handling with context
✅ Integration with main CLI
✅ `--schema-registry-url` flag
✅ Environment variable support
✅ Build succeeds

---

**Phase 14.4 Complete** - Remaining effort: ~5 hours (consumer group commands + docs)

Next: Consumer group commands
