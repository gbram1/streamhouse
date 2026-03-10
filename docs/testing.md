# Testing

---

## Quick Start

```bash
# Unit tests
cargo test --workspace

# Full e2e (Docker Compose)
docker compose up -d
./tests/e2e_full.sh

# Throughput benchmark
./tests/bench_throughput.sh
```

---

## Unit Tests

```bash
# All crates
cargo test --workspace

# Specific crate
cargo test -p streamhouse-storage
cargo test -p streamhouse-kafka
cargo test -p streamhouse-metadata

# With Postgres backend
DATABASE_URL=postgres://postgres:streamhouse@localhost/streamhouse \
  cargo test --features postgres --workspace
```

---

## E2E Test Suites

All e2e tests use Docker Compose (Postgres, MinIO, 3 agents, Prometheus, Grafana).

### `e2e_full.sh` — Comprehensive Feature Test

```bash
./tests/e2e_full.sh              # build + test
./tests/e2e_full.sh --no-build   # skip build
./tests/e2e_full.sh --cleanup    # tear down
```

Tests: multi-agent coordination, org isolation, topic CRUD, schema validation, produce/consume, consumer groups, SQL queries, agent failure/rebalance, REST API.

~3-5 minutes.

### `e2e_observability.sh` — Production Path with Monitoring

```bash
./tests/e2e_observability.sh
```

Tests: real Postgres + S3 (MinIO), multi-agent, gRPC produce, REST consume, reconciler, streamctl CLI. Leaves services running for dashboard exploration.

Services available after:
- Grafana: http://localhost:3001 (admin / admin)
- Prometheus: http://localhost:9091
- MinIO: http://localhost:9001 (minioadmin / minioadmin)

~5-10 minutes.

### `e2e_dr_snapshot.sh` — Disaster Recovery

```bash
./tests/e2e_dr_snapshot.sh
```

**22 assertions across 5 phases:**

| Phase | What it tests |
|-------|---------------|
| 1 | Automatic snapshot creation after S3 flush |
| 2 | Partial metadata loss + reconcile-from-s3 |
| 3 | Full DR — delete all metadata, restore from snapshot |
| 4 | Reconcile idempotency (running twice changes nothing) |
| 5 | Automatic self-healing (delete metadata, start server normally, verify data recovered) |

~3-5 minutes.

### `e2e_multi_agent.sh` — Agent Coordination

```bash
./tests/e2e_multi_agent.sh
```

Tests: 3 agents claim partitions, even distribution, kill agent → survivors absorb, new agent → rebalance.

~2-3 minutes (includes 90s rebalance wait).

### `e2e_schema_validation.sh` — Schema Enforcement

```bash
./tests/e2e_schema_validation.sh
```

Tests: register JSON schema, valid produce accepted, invalid produce rejected, unschemaed topic accepted.

~1 minute. Requires Docker Compose running.

---

## Load Testing

Multi-protocol load test framework with Grafana dashboard.

```bash
# Build
cargo build --release -p streamhouse-loadtest

# Run (default: 3 orgs, 8 topics/org, 100 msg/s, all protocols)
./target/release/streamhouse-loadtest

# Custom config
./target/release/streamhouse-loadtest \
  --http-addr http://localhost:8080 \
  --kafka-addr 127.0.0.1:9092 \
  --grpc-addr http://localhost:50051 \
  --organizations 5 \
  --topics-per-org 4 \
  --produce-rate 500 \
  --duration 300
```

### What it runs

- REST producers (2 per org, batch mode)
- Kafka producers (2 topics, SASL/PLAIN auth, LZ4 compression)
- gRPC producers (batched)
- Consumers (1 per topic, offset tracking)
- SQL query workloads
- Schema evolution workloads
- Data integrity validation
- S3 storage verification

### Metrics

Load test exposes Prometheus metrics on `:9100`. Grafana dashboard at `grafana/dashboards/streamhouse-loadtest.json`.

---

## Benchmarks

### `bench_throughput.sh` — Raw Performance

```bash
./tests/bench_throughput.sh
```

Runs `cargo run --release --bin bench-throughput`. Tests WAL-only and full write path.

Expected results:
- WAL: ~2.21M records/sec
- Full path (WAL → SegmentBuffer → S3): ~769K records/sec

### `benchmarks/grpc-bench.sh` — gRPC Throughput

```bash
./tests/benchmarks/grpc-bench.sh
```

Batches of 100 messages via gRPC ProduceBatch. Reports messages/sec and p50/p99 latency.

### `benchmarks/rest-bench.sh` — REST Throughput

```bash
./tests/benchmarks/rest-bench.sh
```

Batch produce via `POST /api/v1/produce/batch`.

---

## Phase Tests

17 individual feature tests in `tests/phases/`:

```bash
./tests/run-all.sh   # runs all phases
```

| Phase | Focus |
|-------|-------|
| 01 | Server health |
| 02 | REST API (topics, produce, consume) |
| 03 | gRPC API |
| 04 | Schema registry |
| 05 | Producer lifecycle, transactions |
| 06 | Error handling, invalid inputs |
| 07 | Concurrent writes |
| 08 | Performance |
| 09 | streamctl CLI |
| 10 | SQL queries |
| 11 | Kafka wire protocol |
| 12 | Cross-protocol (REST + gRPC + Kafka interop) |
| 13 | Multi-tenancy / org isolation |
| 14 | Connectors |
| 15 | Dashboard metrics |
| 16 | Advanced SQL |
| 17 | S3/local storage verification |

Requires Docker Compose running.

---

## CI/CD

```bash
# Quick (5-10 min) — every PR
cargo test --workspace
./tests/e2e_full.sh --no-build

# Full (20-30 min) — merge to main
./tests/e2e_full.sh
./tests/e2e_multi_agent.sh
./tests/e2e_dr_snapshot.sh
./tests/e2e_schema_validation.sh

# Nightly
./tests/run-all.sh   # all 17 phase tests
./tests/bench_throughput.sh
```

---

## Known Test Issues

The following tests have pre-existing issues and may fail:

| Test | Issue |
|------|-------|
| `streamhouse-kafka::codec::tests::test_request_header_parse_empty_client_id` | Codec edge case |
| `streamhouse-storage::chaos_test::chaos_stress_combined_cb_and_rl` | Flaky — timing-sensitive chaos test |
| `tenant::tests::test_generate_api_key` | Key length assertion |
| `chaos_failover_fencing_token_monotonic` | Fencing token assertion under high contention |

These do not affect normal operation. If you see other test failures, please open an issue.

---

## Troubleshooting

**Server not starting**: Check `docker compose logs streamhouse-server`. Common: port conflict, missing data dirs (`mkdir -p data/storage data/cache data/wal`).

**Rebalance failures**: Wait 60-90 seconds. Check agent logs: `docker compose logs agent-1 agent-2 agent-3`.

**Low benchmark throughput**: Build with `--release`. Check Docker Desktop CPU/memory limits. Set `RUST_LOG=error`.

**Messages not appearing**: Segments buffer before S3 upload. Wait 5-10 seconds or set `SEGMENT_MAX_AGE_MS=1000`.
