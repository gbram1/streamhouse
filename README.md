# StreamHouse

**Kafka is an operational nightmare.** Broker fleets, disk replication, ZooKeeper (or KRaft), JVM tuning — and for what? Most workloads don't need single-digit-millisecond latency. They need reliable event streaming that doesn't cost a fortune to run.

StreamHouse is an open-source event streaming platform that stores everything in S3. One Rust binary. No brokers to manage. Kafka protocol compatible. 1 TB of retention costs ~$23/month instead of thousands.

```
Producers (Kafka, gRPC, REST)
        ↓
  StreamHouse Server (single binary)
        ↓
  WAL (local disk) → Segment Buffer → S3
        ↑
  Consumers (Kafka, gRPC, REST, SQL)
```

## Quick Start

```bash
git clone https://github.com/gbram1/streamhouse
cd streamhouse
./quickstart.sh
```

That's it. Server starts locally (no cloud services needed), creates a demo topic, produces messages, and consumes them back.

Or use Docker Compose for the full stack (PostgreSQL, MinIO, Grafana):

```bash
docker compose up -d
```

| Service | URL |
|---------|-----|
| REST API | http://localhost:8080 |
| gRPC | localhost:50051 |
| Kafka protocol | localhost:9092 |
| Swagger UI | http://localhost:8080/swagger-ui/ |
| Grafana | http://localhost:3001 |

> **[Full setup guide →](docs/getting-started.md)** — manual builds, CLI tool, Docker options, S3 configuration

## Why StreamHouse?

### Cost
Kafka stores data on broker disks with 3x replication. Retention is expensive. StreamHouse stores data in S3 — retention is nearly free, and you don't pay for idle broker compute.

### Simplicity
No JVM. No ZooKeeper. No KRaft. No broker fleet. One Rust binary with SQLite (dev) or PostgreSQL (prod) for metadata. Everything else is in S3.

### Flexibility
Not every workload needs sub-millisecond latency. StreamHouse lets you choose your durability/speed tradeoff per-write:

| Mode | Latency | What happens | Best for |
|------|---------|-------------|----------|
| `acks=buffered` | ~1ms | Record hits WAL on local disk, fsync every 100ms | High-throughput ingestion, analytics |
| `acks=durable` | ~150ms | Batched S3 flush (200ms window) before ack | Production workloads needing S3 durability |

Process crashes? WAL recovers — zero data loss. Agent dies? Another agent picks up the partition and recovers the WAL. Disk destroyed? You lose at most ~100ms of unflushed writes (unless you're using `acks=durable`, in which case nothing is lost).

> **[Full durability model →](docs/overview.md#write-ahead-log-wal)** — fsync policies, crash recovery, cross-agent WAL replay

## Features

### Streaming
- **Kafka protocol** — 23 APIs including consumer groups, transactions (exactly-once), SASL auth. Use kcat, kafka-python, any Kafka client.
- **gRPC + REST** — Modern APIs. OpenAPI docs at `/swagger-ui/`.
- **SQL engine** — Query streams with SQL. `SELECT * FROM orders WHERE value->>'status' = 'paid'`. Window aggregations (`TUMBLE`, `HOP`, `SESSION`), JSON operators, `SHOW TOPICS`, `DESCRIBE`.
- **Schema Registry** — JSON Schema, Avro, Protobuf. Compatibility checking (forward, backward, full). Produce-time validation.
- **Consumer groups** — Offset tracking, rebalancing, lag monitoring, offset reset (earliest/latest/timestamp).

### Storage
- **S3-native segments** — Custom binary format (STRM). LZ4 compression, delta-encoded offsets/timestamps, block-based indexing. ~80% compression ratio.
- **Log compaction** — Tombstone handling, configurable dirty ratio, background compaction threads.
- **Multipart uploads** — Segments >8MB use parallel multipart upload with retry and circuit breaker for S3 throttling.

### Operations
- **Multi-tenancy** — Organizations, API keys with permissions/scopes, org-scoped isolation for all resources.
- **Disaster recovery** — Automatic metadata snapshots to S3, reconcile-from-S3 recovery, self-healing on startup if metadata is lost.
- **S3 orphan reconciliation** — Background job diffs S3 listings against metadata, cleans up orphans with 1-hour grace period.
- **CDC connectors** — Sources: Debezium (Postgres/MySQL CDC), Kafka. Sinks: S3 (Parquet/JSON/CSV), PostgreSQL, Elasticsearch.
- **Observability** — Prometheus metrics, Grafana dashboards included, WebSocket real-time metrics streaming.
- **Lease-based coordination** — Epoch fencing prevents split-brain. 30s leases, 10s renewal, graceful partition transfer.

## Examples

### Kafka Protocol

```bash
# Produce with kcat
echo '{"event":"signup","user":"alice"}' | kcat -P -b localhost:9092 -t events -k user-1

# Consume
kcat -C -b localhost:9092 -t events -o beginning

# Works with any Kafka client
from kafka import KafkaProducer
producer = KafkaProducer(bootstrap_servers='localhost:9092')
producer.send('events', key=b'user-1', value=b'{"event":"signup"}')
```

### REST API

```bash
# Create topic
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partitions": 4}'

# Produce
curl -X POST http://localhost:8080/api/v1/produce \
  -H "Content-Type: application/json" \
  -d '{"topic": "events", "key": "user-1", "value": "{\"event\": \"signup\"}"}'

# Consume
curl "http://localhost:8080/api/v1/consume?topic=events&partition=0&offset=0"
```

### SQL

```bash
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM events WHERE value->>'\''type'\'' = '\''signup'\'' LIMIT 10"}'
```

Or via the interactive CLI:

```console
$ cargo run --bin stm

> sql query SELECT * FROM events WHERE value->>'type' = 'signup' LIMIT 5
┌────────┬──────────────────────────────────┬─────────────────────────┐
│ offset │ value                            │ timestamp               │
├────────┼──────────────────────────────────┼─────────────────────────┤
│ 0      │ {"type":"signup","user":"alice"} │ 2024-03-03T12:34:56Z    │
└────────┴──────────────────────────────────┴─────────────────────────┘
```

## Architecture

```
                 ┌─────────────────────────────────────────────────┐
                 │              Unified Server                      │
  Producers ────►│  REST (:8080)  gRPC (:50051)  Kafka (:9092)    │
                 └──────────────────────┬──────────────────────────┘
                                        │
                 ┌──────────────────────▼──────────────────────────┐
                 │                    Agent                         │
                 │                                                  │
                 │  LeaseManager ──► Epoch fencing (Postgres)       │
                 │       │                                          │
                 │       ▼                                          │
                 │  PartitionWriter                                 │
                 │       │                                          │
                 │       ├── WAL (local disk, configurable fsync)   │
                 │       ├── SegmentBuffer (batching, LZ4)          │
                 │       └── S3 Upload (multipart, retry, circuit   │
                 │                       breaker)                   │
                 │                                                  │
                 │  S3 Reconciler (background orphan cleanup)       │
                 │  MetadataSnapshots (periodic S3 backup)          │
                 └─────────────────────────────────────────────────┘
                        │                              │
                        ▼                              ▼
                 ┌─────────────┐              ┌──────────────┐
                 │  Metadata   │              │      S3      │
                 │  (SQLite or │              │  (segments)  │
                 │  PostgreSQL)│              └──────────────┘
                 └─────────────┘                     ▲
                                                     │
                                             Consumers read
                                             via offset index
```

> **[Full architecture deep dive →](docs/overview.md)** — segment format, write/read paths, lease protocol, rebalancing, disaster recovery

## Performance

| Metric | Throughput |
|--------|-----------|
| WAL writes | 2.21M records/sec |
| Full path (WAL → S3) | 769K records/sec |
| gRPC ProduceBatch | 100K+ messages/sec sustained |
| Segment read (LZ4) | 3.10M records/sec |
| Offset seek | 640us |

Benchmarks run with default segment settings (1MB blocks, LZ4 compression). See **[testing docs →](docs/testing.md)** for reproduction scripts.

## Documentation

| Doc | What's in it |
|-----|-------------|
| **[Getting Started](docs/getting-started.md)** | Installation, first topic, CLI, all three protocols, Docker |
| **[Architecture](docs/overview.md)** | Internals — segments, WAL, leases, rebalancing, DR, durability model |
| **[API Reference](docs/api-reference.md)** | Every REST, gRPC, and Kafka endpoint |
| **[Configuration](docs/configuration.md)** | All environment variables, performance tuning, durability knobs |
| **[Authentication](docs/authentication.md)** | API keys, SASL/PLAIN, Bearer tokens, multi-tenancy |
| **[Testing](docs/testing.md)** | E2E suites, benchmarks, load testing |

## Compared to Alternatives

| | StreamHouse | Kafka | WarpStream | AutoMQ |
|---|---|---|---|---|
| Storage | S3 | Broker disks (3x replication) | S3 | EBS + S3 tiering |
| Runtime | Single Rust binary | JVM + ZooKeeper/KRaft | Go agents + S3 | Kafka fork (JVM) |
| Durability knob | WAL ↔ S3 per-write | Replication factor | S3 only (~250ms) | EBS WAL |
| Retention cost (1 TB) | ~$23/mo (S3) | ~$3K+/mo (EBS) | ~$23/mo (S3) | EBS + S3 |
| Kafka compatibility | 23 APIs + transactions | Native | Full | Full (fork) |
| Built-in SQL | Yes (DataFusion) | No (needs KSQL) | No | No |
| Schema Registry | Built-in | Separate (Confluent) | No | Separate |
| CDC Connectors | Built-in | Separate (Connect) | No | Separate (Connect) |

## Contributing

Issues and PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[Apache License, Version 2.0](LICENSE)
