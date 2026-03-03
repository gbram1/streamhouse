# Testing Guide

This guide covers all test suites, benchmarks, and validation scripts in StreamHouse.

## Quick Start

```bash
# Unit tests
cargo test --workspace

# Unit tests with PostgreSQL
DATABASE_URL=postgres://user:pass@localhost/streamhouse cargo test --features postgres --workspace

# Quick start validation (local dev mode)
./quickstart.sh

# Full e2e test suite (Docker Compose)
./tests/e2e_full.sh

# Observability demo (leaves services running)
./tests/e2e_observability.sh
```

## Unit Tests

Standard Rust unit and integration tests across all crates:

```bash
cargo test --workspace
```

**PostgreSQL backend tests:**

```bash
# Start PostgreSQL
docker run -d --name postgres-test \
  -e POSTGRES_PASSWORD=streamhouse \
  -e POSTGRES_DB=streamhouse \
  -p 5432:5432 postgres:15

# Run tests
DATABASE_URL=postgres://postgres:streamhouse@localhost/streamhouse \
  cargo test --features postgres --workspace
```

## E2E Test Suites

End-to-end tests use Docker Compose to spin up full infrastructure (PostgreSQL, MinIO, multi-agent setup) and validate production workflows.

### `e2e_full.sh`

**Comprehensive feature test** — exercises every major feature:

```bash
./tests/e2e_full.sh              # full run (build + test)
./tests/e2e_full.sh --no-build   # skip build
./tests/e2e_full.sh --cleanup    # tear down services
```

**What it tests:**
1. Multi-agent coordination (3 agents, partition distribution)
2. Organization isolation (multi-tenant topic scoping)
3. Topic creation via streamctl CLI
4. Partition assignment verification
5. JSON schema registration and validation
6. Produce messages (with schema validation)
7. Consume messages via streamctl
8. Consumer group offset management
9. SQL queries over streams
10. Agent failure and rebalance (kill agent-3, verify survivors absorb partitions)
11. Agent join and rebalance (start agent-4, verify partition redistribution)
12. REST API browsing

**Duration:** ~3-5 minutes

### `e2e_observability.sh`

**Full production path with monitoring** — uses real gRPC, S3, agents, reconciler:

```bash
./tests/e2e_observability.sh              # full run (build + demo)
./tests/e2e_observability.sh --no-build   # skip build
./tests/e2e_observability.sh --cleanup    # tear down services
```

**What it tests:**
- streamctl CLI for all operations (topic mgmt, produce, consume, offsets, SQL)
- REST batch endpoint for throughput benchmarking
- Real PostgreSQL metadata store
- Real S3 (MinIO) storage
- Multi-agent coordination
- Background reconciler

**What you get:**
- Grafana dashboard at http://localhost:3001 (admin / admin)
- Prometheus metrics at http://localhost:9091
- MinIO console at http://localhost:9001 (minioadmin / minioadmin)
- Swagger UI at http://localhost:8080/swagger-ui/

**Duration:** ~5-10 minutes (leaves services running for exploration)

### `e2e_multi_agent.sh`

**Agent coordination test** — verifies partition load splitting:

```bash
./tests/e2e_multi_agent.sh              # full run (build + test)
./tests/e2e_multi_agent.sh --no-build   # skip build
./tests/e2e_multi_agent.sh --cleanup    # tear down services
```

**What it tests:**
1. 3 agents register and claim partitions
2. Partitions are distributed across agents (approximately even)
3. Produce data to all partitions
4. Kill an agent, verify survivors absorb its partitions (rebalance)
5. Start a replacement agent, verify new rebalance includes it

**Duration:** ~2-3 minutes (includes 90s rebalance wait)

### `e2e_schema_validation.sh`

**Schema enforcement test** — validates JSON/Avro schema validation on produce:

```bash
./tests/e2e_schema_validation.sh
```

**What it tests:**
- Register JSON schema via streamctl
- Produce valid message (accepted)
- Produce invalid message (rejected with validation error)
- Produce to topic without schema (accepted)

**Prerequisites:** Docker Compose services must be running

**Duration:** ~1 minute

## Benchmarks

Performance benchmarking scripts for throughput and latency testing.

### `benchmarks/grpc-bench.sh`

**gRPC throughput benchmark** — measures ProduceBatch performance:

```bash
./tests/benchmarks/grpc-bench.sh
```

**What it does:**
- Creates benchmark topic with 4 partitions
- Sends batches of 100 messages each
- Uses Rust gRPC client (compiled binary)
- Reports throughput (messages/sec) and latency (p50, p99)

**Prerequisites:** Server running at localhost:50051

### `benchmarks/rest-bench.sh`

**REST throughput benchmark** — measures REST API batch endpoint:

```bash
./tests/benchmarks/rest-bench.sh
```

**What it does:**
- Creates benchmark topic with 4 partitions
- Sends batches via `POST /api/v1/produce/batch`
- Uses `curl` for HTTP calls
- Reports throughput (messages/sec)

**Prerequisites:** Server running at localhost:8080

### `bench_throughput.sh`

**WAL and full-path throughput test** — raw performance measurement:

```bash
./tests/bench_throughput.sh
```

**What it does:**
- Builds release binary
- Runs `cargo run --release --bin bench-throughput`
- Tests both WAL-only and full write path (WAL → SegmentBuffer → S3)

**Expected results:**
- WAL throughput: ~2.21M records/sec
- Full path throughput: ~769K records/sec

## Phase Tests

The `tests/phases/` directory contains 17 individual test scripts that cover specific features. These are orchestrated by `tests/run-all.sh`:

```bash
./tests/run-all.sh
```

**Phase test breakdown:**

| Phase | Script | Focus |
|-------|--------|-------|
| 01 | `01-smoke.sh` | Basic server health check |
| 02 | `02-rest-api.sh` | REST endpoints (topics, produce, consume) |
| 03 | `03-grpc-api.sh` | gRPC service methods |
| 04 | `04-schema-registry.sh` | Schema registration, validation |
| 05 | `05-lifecycle.sh` | Producer lifecycle, transactions |
| 06 | `06-negative.sh` | Error handling, invalid inputs |
| 07 | `07-concurrent-load.sh` | Concurrent writes, multi-partition |
| 08 | `08-benchmarks.sh` | Performance tests |
| 09 | `09-cli.sh` | streamctl CLI commands |
| 10 | `10-sql-verification.sh` | SQL query engine |
| 11 | `11-kafka-protocol.sh` | Kafka wire protocol compatibility |
| 12 | `12-cross-protocol.sh` | REST + gRPC + Kafka interop |
| 13 | `13-multi-tenancy.sh` | Organization isolation |
| 14 | `14-connectors.sh` | External integrations |
| 15 | `15-dashboard-metrics.sh` | Observability exports |
| 16 | `16-advanced-sql.sh` | Complex SQL queries |
| 17 | `17-resource-storage.sh` | S3/local storage verification |

**Note:** `run-all.sh` requires Docker Compose services to be running first.

## Interpreting Results

### Pass/Fail Output

All e2e and phase tests use a consistent pass/fail format:

```
✓ PASS: Test description
✗ FAIL: Test description
       Error details
```

At the end of each test:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Summary
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  PASS: 42
  FAIL: 0

  All checks passed! ✨
```

### Troubleshooting Failures

**Server not starting:**
```bash
# Check logs
docker compose logs streamhouse-server

# Check local storage mode
USE_LOCAL_STORAGE=1 ./target/release/unified-server
tail -f /tmp/streamhouse-quickstart.log
```

**Rebalance failures (multi-agent tests):**
- Wait longer — rebalance can take 60-90 seconds
- Check agent logs: `docker compose logs agent-1 agent-2 agent-3`
- Verify PostgreSQL is healthy: `docker compose ps postgres`

**Schema validation failures:**
- Verify schema was registered: `./target/release/streamctl schema get <subject>`
- Check schema format (JSON must be valid JSON Schema, not raw JSON)

**Benchmark low throughput:**
- Ensure server is running in release mode: `cargo build --release`
- Check CPU/memory limits (Docker Desktop settings)
- Disable debug logging: `RUST_LOG=error ./target/release/unified-server`

### Known Flaky Tests

From project memory, these tests have known issues:

- `streamhouse-kafka::codec::tests::test_request_header_parse_empty_client_id` — parsing edge case
- `streamhouse-storage::chaos_test::chaos_stress_combined_cb_and_rl` — timing-dependent stress test

These are pre-existing and can be ignored if they fail in isolation.

## CI/CD Usage

For continuous integration pipelines:

```bash
# Quick validation (5-10 minutes)
cargo test --workspace
./tests/e2e_full.sh --no-build

# Full test matrix (20-30 minutes)
./tests/e2e_full.sh
./tests/e2e_multi_agent.sh
./tests/e2e_schema_validation.sh
./tests/benchmarks/grpc-bench.sh
./tests/benchmarks/rest-bench.sh

# All phase tests (15-20 minutes)
docker compose up -d
./tests/run-all.sh
docker compose down
```

**Recommended CI strategy:**
1. Unit tests on every PR
2. `e2e_full.sh` on every PR
3. All e2e tests + benchmarks on main branch
4. All phase tests nightly
