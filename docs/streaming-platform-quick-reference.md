# Quick Reference Card

Print this. Put it on your wall.

---

## The Vision (One Sentence)

**Kafka + Flink replaced by one S3-native system in Rust.**

---

## The Pitch (By Phase)

| Month | What You Say |
|-------|--------------|
| 4 | "Kafka replacement, 80% cheaper" |
| 10 | "Kafka + Flink in one system" |
| 14 | "Drop-in replacement with built-in SQL" |

---

## Roadmap At A Glance

```
Month:  1    2    3    4    5    6    7    8    9    10   11   12   13   14
        ├────┴────┴────┴────┼────┴────┴────┴────┴────┴────┼────┴────┴────┴────┤
        │  STORAGE LAYER    │      PROCESSING LAYER       │   PRODUCT POLISH  │
        │  + Kafka Protocol │      + State + Windows      │   + UI + Cloud    │
        ├───────────────────┼─────────────────────────────┼───────────────────┤
        │ Milestone: v0.2   │ Milestone: v0.5             │ Milestone: v1.0   │
        │ "It works"        │ "It's differentiated"       │ "It's a product"  │
```

---

## Phase Breakdown

| Phase | Months | What You Build | Outcome |
|-------|--------|----------------|---------|
| 1 | 1-2 | S3 storage, gRPC API | Internal MVP |
| 2 | 3-4 | Kafka protocol | Drop-in Kafka replacement |
| 3 | 5-7 | SQL processing | Differentiated |
| 4 | 8-10 | Distributed, fault-tolerant | Production-grade |
| 5 | 11-14 | UI, cloud deploy | Real product |
| 6 | 15+ | Launch, customers | Business |

---

## First 4 Weeks (Memorize This)

| Week | Focus | Deliverable |
|------|-------|-------------|
| 1 | Setup + Segment format | Can serialize/deserialize segments |
| 2 | S3 read/write | Segments flow to/from S3 |
| 3 | Metadata store | Topics and partitions managed |
| 4 | Write path | Records flow from API to S3 |

---

## Tech Stack

```
Language:     Rust
Async:        Tokio
Data:         Apache Arrow
SQL:          DataFusion
State:        RocksDB
Storage:      S3 / MinIO / R2
Protocol:     gRPC (internal), Kafka (external)
UI:           React
Deploy:       Docker, Kubernetes
```

---

## Key Crates

```toml
tokio = "1.0"           # Async runtime
arrow = "50.0"          # Columnar data
datafusion = "35.0"     # SQL engine
object_store = "0.9"    # S3 abstraction
rocksdb = "0.21"        # State storage
tonic = "0.11"          # gRPC
bytes = "1.0"           # Protocol parsing
```

---

## Directory Structure

```
streaming-platform/
├── crates/
│   ├── core/           # Types, errors
│   ├── storage/        # S3 segments
│   ├── metadata/       # SQLite metadata
│   ├── kafka-protocol/ # Kafka wire protocol
│   ├── sql/            # SQL parsing
│   ├── execution/      # Stream operators
│   ├── state/          # RocksDB state
│   ├── server/         # Main binary
│   └── cli/            # CLI tool
├── web/                # React UI
└── deploy/             # Docker, K8s, Terraform
```

---

## Success Metrics

| Phase | Metric | Target |
|-------|--------|--------|
| 1-2 | Write throughput | 50K rec/sec |
| 3-4 | Kafka API coverage | 80% |
| 5 | Query latency | <100ms p99 |
| 6 | Recovery time | <60 sec |
| 7 | Paying customers | 10+ |

---

## When You're Stuck

1. **Technical block?** → Prototype the smallest version first
2. **Motivation low?** → Write a blog post about what you learned
3. **Scope creeping?** → Ask "Does this help Month 4 launch?"
4. **No users?** → Post progress update, ask for feedback
5. **Competitor news?** → They validated the market, keep building

---

## Weekly Rhythm

```
Monday:    Plan the week, biggest task first
Tuesday:   Deep work
Wednesday: Deep work
Thursday:  Deep work, start wrapping up
Friday:    Tests, docs, blog/tweet, plan next week
Weekend:   Off (seriously)
```

---

## Public Checkpoints

| When | What to Share |
|------|---------------|
| Week 4 | "Built an S3-native log in Rust" |
| Week 8 | "v0.1: Working storage layer" |
| Week 12 | "Kafka-compatible in 10K lines of Rust" |
| Week 16 | "v0.2: Drop-in Kafka replacement" |
| Month 7 | "Added SQL processing" |

---

## Commands You'll Run Daily

```bash
# Build and test
cargo build
cargo test
cargo clippy

# Run locally
docker-compose up -d          # MinIO
cargo run --bin server        # Your server

# Test with Kafka tools
kafka-console-producer --broker-list localhost:9092 --topic test
kafka-console-consumer --bootstrap-server localhost:9092 --topic test

# Your CLI
streamctl topic create test --partitions 3
streamctl produce test --value "hello"
streamctl consume test --from-beginning
```

---

## Competitors to Watch

| Company | What They Do | Your Edge |
|---------|--------------|-----------|
| Confluent | Kafka + Flink | Unified, simpler |
| WarpStream | S3 Kafka | Has processing too |
| Redpanda | Fast Kafka | Has processing too |
| RisingWave | Streaming SQL | No Kafka needed |

---

## First Day Checklist

```bash
# 1. Create repo
mkdir streaming-platform && cd streaming-platform
git init
cargo init --lib

# 2. Set up workspace (Cargo.toml)
# 3. Create crate structure
# 4. Add CI (.github/workflows/ci.yml)
# 5. Start MinIO
docker run -d -p 9000:9000 minio/minio server /data

# 6. Write first test
# 7. Make it pass
# 8. Commit and push
```

---

## Motivation Reminders

- WarpStream: 2 founders → $220M exit in 18 months
- Arroyo: YC seed → Cloudflare acquisition
- The integrated approach hasn't been tried
- You like hard problems, this is one
- No deadline means you can do it right

---

*Start Week 1. Build the segment format. Ship something.*
