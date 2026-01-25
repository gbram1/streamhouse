# StreamHouse Testing Guide

This guide covers how to test StreamHouse locally.

## Quick Start

Run the comprehensive workflow test to see everything in action:

```bash
cargo run --package streamhouse-agent --example local_workflow_test
```

**Duration:** ~2.5 minutes  
**What it tests:** Full multi-agent workflow with failover and recovery

---

## Unit Tests

### Run All Tests

```bash
# Run all tests in the project
cargo test

# Run tests for specific package
cargo test --package streamhouse-agent
cargo test --package streamhouse-metadata
cargo test --package streamhouse-storage
```

### Agent Tests

```bash
# Unit tests (8 tests - agent, heartbeat, lease_manager)
cargo test --package streamhouse-agent --lib

# Lease coordination tests (8 tests)
cargo test --package streamhouse-agent --test lease_coordination_test

# Multi-agent tests (9 tests)
cargo test --package streamhouse-agent --test multi_agent_test
```

**Total:** 25 tests (all passing ✅)

---

## Demo Scripts

See [examples/README.md](crates/streamhouse-agent/examples/README.md) for full details.

### 1. Local Workflow Test ⭐ **Recommended**

```bash
cargo run --package streamhouse-agent --example local_workflow_test
```

**What it tests:**
- Topic creation (3 topics, 21 partitions)
- Multi-agent startup (3 agents)
- Automatic partition assignment
- Lease management and epochs
- Agent failure simulation
- Automatic rebalancing
- Agent recovery
- Health monitoring
- Discovery queries
- Graceful shutdown
- Resource cleanup

### 2. Multi-Agent Demo

```bash
cargo run --package streamhouse-agent --example demo_phase_4_multi_agent
```

### 3. Simple Agent

```bash
cargo run --package streamhouse-agent --example simple_agent
```

---

## Test Results

All tests passing:

```bash
$ cargo test --package streamhouse-agent

running 8 tests (unit tests)
test result: ok. 8 passed

running 8 tests (lease coordination)
test result: ok. 8 passed

running 9 tests (multi-agent)
test result: ok. 9 passed

Total: 25 tests passing ✅
```

---

## Quick Verification

Before committing:

```bash
# 1. Run all tests
cargo test

# 2. Check for warnings
cargo clippy

# 3. Format code
cargo fmt

# 4. Run workflow demo
cargo run --package streamhouse-agent --example local_workflow_test
```

**Expected:** All green ✅

---

## Documentation

- [Phase 4 Overview](docs/phases/PHASE_4_OVERVIEW.md)
- [Phase 4.1 Complete](docs/phases/PHASE_4.1_COMPLETE.md)
- [Phase 4.2 Complete](docs/phases/PHASE_4.2_COMPLETE.md)
- [Phase 4.3 Complete](docs/phases/PHASE_4.3_COMPLETE.md)
- [Examples README](crates/streamhouse-agent/examples/README.md)

---

**All Systems Go!** 🚀
