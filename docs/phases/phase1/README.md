# Phase 1: Core Storage Layer

**Timeline:** Months 1-2 (Weeks 1-8)
**Goal:** Build an S3-native append-only log with produce/consume capabilities

---

## Overview

Phase 1 creates the foundational storage layer - a working system that can store and retrieve streaming data using S3 as the backend. By the end, you'll have a functional (though not Kafka-compatible) streaming platform.

**Key Deliverable:** "Kafka replacement" functionality via custom gRPC API

---

## Initiatives

### [1.1 Project Setup](1.1-project-setup.md) (Week 1)
Set up development environment, workspace structure, and CI/CD pipeline.

**Key Deliverables:**
- ✅ Rust workspace configured
- ✅ CI/CD running (tests, clippy, fmt)
- ✅ MinIO running locally
- ✅ README with vision

---

### [1.2 Segment Writer](1.2-segment-writer.md) (Week 1-2)
Design and implement the core data format for storing events in S3.

**Key Deliverables:**
- ✅ Custom binary format with compression
- ✅ Block-based structure (1MB blocks)
- ✅ Index for fast offset lookups
- ✅ 80%+ compression ratio

**Key Concepts:**
- LZ4 compression
- Delta encoding for offsets
- Index-based random access

---

### [1.3 Metadata Store](1.3-metadata-store.md) (Week 2-3)
Build metadata tracking system for topics, partitions, and segments.

**Key Deliverables:**
- ✅ SQLite-based metadata store
- ✅ Topic and partition management
- ✅ Segment tracking
- ✅ Consumer offset storage

**Key Concepts:**
- Metadata as "card catalog"
- Fast offset → segment lookups
- Consumer group tracking

---

### [1.4 Write Path](1.4-write-path.md) (Week 3-4)
Implement complete flow from receiving records to storing in S3.

**Key Deliverables:**
- ✅ PartitionWriter with batching
- ✅ Segment rolling (size/time based)
- ✅ S3 upload with retries
- ✅ Metadata updates

**Key Concepts:**
- In-memory buffering
- Automatic segment rotation
- Watermark tracking

---

### [1.5 Read Path](1.5-read-path.md) (Week 4-5)
Implement efficient record consumption with caching and prefetching.

**Key Deliverables:**
- ✅ LRU cache for segments
- ✅ S3 download optimization
- ✅ Sequential read prefetching
- ✅ Consumer API

**Key Concepts:**
- Cache-aware reads
- Prefetching for throughput
- Long polling support

---

### [1.6 API Server](1.6-api-server.md) (Week 5-6)
Expose functionality via gRPC services.

**Key Deliverables:**
- ✅ gRPC protocol definitions
- ✅ Producer/Consumer services
- ✅ Admin service
- ✅ Running server binary

**Key Concepts:**
- gRPC streaming
- Service separation
- Error handling

---

### [1.7 CLI Tool](1.7-cli-tool.md) (Week 6)
Build command-line interface for easy interaction.

**Key Deliverables:**
- ✅ Topic management commands
- ✅ Produce from CLI/file/stdin
- ✅ Consume with various modes
- ✅ Pretty output formatting

**Key Concepts:**
- User-friendly CLI design
- Interactive and scripting modes
- Output formatting options

---

### [1.8 Testing & Documentation](1.8-testing-and-documentation.md) (Week 6-7)
Comprehensive testing, benchmarking, and documentation.

**Key Deliverables:**
- ✅ Unit tests (75%+ coverage)
- ✅ Integration tests
- ✅ Performance benchmarks
- ✅ Complete documentation
- ✅ Blog post

**Key Concepts:**
- Test pyramid strategy
- Performance validation
- Public launch preparation

---

## Phase 1 Milestones

| Week | Milestone | Status |
|------|-----------|--------|
| 1 | Project setup complete | 🔲 |
| 2 | Segment format working | 🔲 |
| 3 | Metadata store functional | 🔲 |
| 4 | Write path operational | 🔲 |
| 5 | Read path with caching | 🔲 |
| 6 | API server + CLI ready | 🔲 |
| 7 | Testing complete | 🔲 |
| 8 | Documentation + polish | 🔲 |

---

## Success Criteria

Phase 1 is complete when:

✅ **Functional**
- Can create topics
- Can produce records (50K/sec single partition)
- Can consume from any offset
- Data survives restarts

✅ **Performance**
- Write: 50,000 records/sec
- Read (cached): <10ms p99
- Compression: 80%+ savings

✅ **Quality**
- Test coverage >75%
- CI passing
- Documentation complete

✅ **Demo Ready**
- Can show end-to-end flow
- CLI is intuitive
- Blog post published

---

## Key Technologies

| Component | Technology | Why |
|-----------|-----------|-----|
| Language | Rust | Performance, safety, great ecosystem |
| Storage | S3/MinIO | Durable, replicated, cheap |
| Metadata | SQLite | Embedded, fast, easy |
| Compression | LZ4 | Best speed/ratio balance |
| API | gRPC | Efficient, typed, streaming |
| CLI | Clap | Easy, powerful, idiomatic |

---

## Performance Targets

| Metric | Target | Achieved |
|--------|--------|----------|
| Write throughput | 50K rec/sec | 🔲 |
| Read latency (cached) | <10ms p99 | 🔲 |
| Read latency (uncached) | <200ms p99 | 🔲 |
| Compression ratio | 80%+ | 🔲 |
| Cache hit rate | >80% | 🔲 |

---

## Architecture Diagram

```
┌────────────────────────────────────────────────────────┐
│                  StreamHouse Phase 1                   │
├────────────────────────────────────────────────────────┤
│                                                        │
│  ┌──────────────────────────────────────────────────┐ │
│  │                  CLI Tool                         │ │
│  │              (streamctl)                          │ │
│  └───────────────────┬──────────────────────────────┘ │
│                      │ gRPC                            │
│  ┌──────────────────▼──────────────────────────────┐  │
│  │               API Server                         │  │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐  │  │
│  │  │ Producer   │ │ Consumer   │ │   Admin    │  │  │
│  │  │ Service    │ │ Service    │ │  Service   │  │  │
│  │  └────────────┘ └────────────┘ └────────────┘  │  │
│  └──────────────────┬──────────────────────────────┘  │
│                     │                                  │
│  ┌──────────────────▼──────────────────────────────┐  │
│  │            Storage Manager                       │  │
│  │  ┌────────────────┐    ┌───────────────────┐   │  │
│  │  │ PartitionWriter│    │ PartitionReader   │   │  │
│  │  │  (batching)    │    │  (caching)        │   │  │
│  │  └────────────────┘    └───────────────────┘   │  │
│  └──────────────────┬──────────────────────────────┘  │
│                     │                                  │
│  ┌──────────────────▼──────────────────────────────┐  │
│  │         Segment Writer/Reader                    │  │
│  │  (compression, indexing, serialization)          │  │
│  └──────────────────┬──────────────────────────────┘  │
│                     │                                  │
│        ┌────────────┴────────────┐                    │
│        ▼                         ▼                     │
│  ┌──────────┐              ┌──────────┐               │
│  │ Metadata │              │    S3    │               │
│  │ (SQLite) │              │ (MinIO)  │               │
│  └──────────┘              └──────────┘               │
└────────────────────────────────────────────────────────┘
```

---

## What's Next?

After completing Phase 1, you move to:

**Phase 2: Kafka Protocol Compatibility (Months 3-4)**
- Implement Kafka binary protocol
- Support existing Kafka clients
- Drop-in replacement capability

---

## Getting Started

1. Start with [1.1 Project Setup](1.1-project-setup.md)
2. Follow the initiatives in order
3. Track progress in the milestones table above
4. Aim to complete in 8 weeks (40-60 hours/week)

---

## Resources

- [Main Project Plan](../../streaming-platform-project-plan-v2.md)
- [Quick Reference Card](../../streaming-platform-quick-reference.md)
- [Deep Dive: Kafka Protocol](../../deep-dive-part1-kafka.md)
- [Deep Dive: S3 Format](../../deep-dive-part2-s3-format.md)
- [Deep Dive: DataFusion](../../deep-dive-part3-datafusion.md)

---

**Status:** 🚧 In Progress
**Current Initiative:** 1.1 Project Setup
**Completion:** 0/8 initiatives complete
