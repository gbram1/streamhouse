# Phase 14: Enhanced CLI - Implementation Guide

**Status:** IN PROGRESS
**Effort:** 20 hours
**Priority:** HIGH

---

## Overview

Enhanced the `streamctl` CLI with interactive REPL mode, REST API support, schema registry commands, consumer group management, and flexible output formatting.

---

## What's Being Added

### 1. Interactive REPL Mode (5h) ✅ STARTED

**Features:**
- Command history (up/down arrows)
- Auto-completion (tab)
- Colorized output
- Multi-line input support
- Exit with Ctrl+D or `exit` command

**Usage:**
```bash
$ streamctl
streamhouse> topic list
streamhouse> produce orders --value '{"test": 1}'
streamhouse> exit
```

**Implementation:**
- Use `rustyline` crate for line editing
- Command parsing via `shell words` splitting
- Persistent history in `~/.streamhouse/history`

---

### 2. REST API Support (2h)

**Current:** Uses gRPC only
**New:** Support both gRPC and REST API

```bash
# Use REST API (new default)
streamctl --api-url http://localhost:8080 topic list

# Use gRPC (legacy)
streamctl --grpc-url http://localhost:9090 topic list
```

---

### 3. Schema Registry Commands (5h)

```bash
# List all subjects
streamctl schema list

# Register schema
streamctl schema register orders-value schema.avsc
# Output: Registered as ID 42

# Get schema
streamctl schema get orders-value
streamctl schema get orders-value --version 3
streamctl schema get --id 42

# Check compatibility
streamctl schema check orders-value new-schema.avsc
# Output: ✓ Compatible (BACKWARD)

# Evolve schema
streamctl schema evolve orders-value new-schema.avsc
# Output: Registered as version 4 (ID 43)

# Delete schema version
streamctl schema delete orders-value --version 3

# Get compatibility mode
streamctl schema config get orders-value
streamctl schema config set orders-value --compatibility FULL
```

---

### 4. Consumer Group Commands (5h)

```bash
# List all consumer groups
streamctl group list
# Output:
# Consumer Groups (2):
#   analytics (3 members, lag: 1,234 records)
#   processing (1 member, lag: 0 records)

# Describe group
streamctl group describe my-consumer-group
# Output:
# Group: my-consumer-group
# Members: 3
# State: Stable
# Lag: 1,234 records (2.3 seconds)
#
# Partition Assignment:
# orders-0: consumer-1 (lag: 100)
# orders-1: consumer-1 (lag: 234)
# orders-2: consumer-2 (lag: 400)

# Reset offsets
streamctl group reset-offsets my-group \
  --topic orders \
  --to earliest

streamctl group reset-offsets my-group \
  --topic orders \
  --to-offset 12345 \
  --partition 0

# Delete group
streamctl group delete my-group --confirm
```

---

### 5. Output Formatting (3h)

**Formats:**
- `--output table` (default) - ASCII tables
- `--output json` - Machine-readable JSON
- `--output yaml` - YAML format
- `--output text` - Plain text

**Examples:**
```bash
# Table format (default)
streamctl topic list
# Topics (3):
# ┌──────────┬────────────┬────────────┐
# │ Name     │ Partitions │ Retention  │
# ├──────────┼────────────┼────────────┤
# │ orders   │ 12         │ 7d         │
# │ events   │ 6          │ 24h        │
# │ logs     │ 3          │ 1h         │
# └──────────┴────────────┴────────────┘

# JSON format
streamctl topic list --output json
# [
#   {"name": "orders", "partitions": 12, "retention": "7d"},
#   {"name": "events", "partitions": 6, "retention": "24h"}
# ]

# Text format (simple)
streamctl topic list --output text
# orders (12 partitions, 7d retention)
# events (6 partitions, 24h retention)
```

---

## Configuration File

**Location:** `~/.streamhouse/config.toml`

```toml
# StreamHouse CLI Configuration

# REST API URL (default)
rest_api_url = "http://localhost:8080"

# gRPC URL (legacy support)
grpc_url = "http://localhost:9090"

# Default output format: table, json, yaml, text
output_format = "table"

# Enable colored output
colored = true

# REPL settings
[repl]
history_file = "~/.streamhouse/history"
history_size = 1000
auto_complete = true
```

---

## Command Structure

```
streamctl
├── topic
│   ├── create <name> [--partitions N] [--retention MS]
│   ├── list [--filter PATTERN]
│   ├── describe <name>
│   ├── update <name> [--retention MS]
│   └── delete <name> [--confirm]
│
├── group
│   ├── list
│   ├── describe <group>
│   ├── reset-offsets <group> --topic <topic> [--to earliest|latest|offset]
│   └── delete <group> [--confirm]
│
├── schema
│   ├── list
│   ├── register <subject> <schema-file>
│   ├── get <subject> [--version V] | [--id ID]
│   ├── check <subject> <schema-file>
│   ├── evolve <subject> <schema-file>
│   ├── delete <subject> --version V
│   └── config
│       ├── get [<subject>]
│       └── set <subject> --compatibility MODE
│
├── produce <topic> [--partition N] [--key KEY] --value VALUE
│
├── consume <topic> [--partition N] [--offset N] [--limit N]
│
└── config
    ├── show
    ├── set <key> <value>
    └── edit
```

---

## Implementation Files

### New Files
1. **src/config.rs** - Configuration management
2. **src/repl.rs** - Interactive REPL mode
3. **src/commands/schema.rs** - Schema registry commands
4. **src/commands/group.rs** - Consumer group commands
5. **src/commands/mod.rs** - Command modules
6. **src/client.rs** - REST API client
7. **src/format.rs** - Output formatting utilities

### Modified Files
1. **src/main.rs** - Add REPL mode, new commands, formatting
2. **Cargo.toml** - Add dependencies (rustyline, reqwest, tabled, colored, toml)

---

## Dependencies Added

```toml
[dependencies]
# Interactive REPL
rustyline = "14.0"
rustyline-derive = "0.10"

# HTTP client
reqwest = { version = "0.12", features = ["json"] }

# Output formatting
tabled = "0.15"  # ASCII tables
colored = "2.1"  # Colored output
toml = "0.8"     # Config file parsing

# Additional
serde = { version = "1.0", features = ["derive"] }
```

---

## Testing

```bash
# Build CLI
cargo build -p streamhouse-cli

# Test interactive mode
./target/debug/streamctl
streamhouse> help
streamhouse> topic list
streamhouse> exit

# Test REST API
streamctl --api-url http://localhost:8080 schema list

# Test formatting
streamctl topic list --output json
streamctl topic list --output table

# Test consumer groups
streamctl group list
streamctl group describe analytics

# Test schema commands
streamctl schema list
streamctl schema get orders-value
```

---

## Next Steps

After Phase 14 completion:
1. **Phase 13: Web UI Dashboard** (60h)
2. **Phase 15: Kubernetes Deployment** (60h)

---

**Status:** Configuration module created, REPL implementation in progress
**Remaining:** 15 hours
