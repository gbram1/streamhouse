# StreamHouse

**A unified S3-native streaming platform that replaces Kafka + Flink**

StreamHouse is a next-generation streaming data platform built in Rust that stores data natively in S3 (or S3-compatible storage) and provides integrated SQL stream processing. It offers the transport capabilities of Kafka and the processing power of Flink in a single, simplified system.

## Vision

- **Month 4**: Kafka replacement, 80% cheaper
- **Month 10**: Kafka + Flink in one system
- **Month 14**: Drop-in replacement with built-in SQL processing

## Status

✅ **Phase 1 Complete** - Core Storage & API (Initiatives 1.1-1.6)

**Completed:**
- ✅ Binary segment format with LZ4 compression
- ✅ SQLite metadata store (topics, partitions, segments, offsets)
- ✅ Write path with automatic segment rolling and S3 upload
- ✅ Read path with LRU caching and prefetching
- ✅ gRPC API server with 9 endpoints
- ✅ Consumer group offset management
- ✅ 29 automated tests (all passing)
- ✅ Integration tests and manual testing scripts

**Next:** Phase 1.7 (CLI Tool), Phase 1.8 (Performance Testing & Documentation)

## Quick Start

### Prerequisites

- Rust 1.75+ (`rustup install stable`)
- Protocol Buffers compiler (`brew install protobuf`)
- grpcurl for testing (`brew install grpcurl`)

### Development Setup

```bash
# Start server with local storage
./start-dev.sh

# In another terminal, run tests
./test-server.sh

# Or run automated tests
cargo test --workspace

# Run tests
cargo test --workspace

# Run formatting check
cargo fmt --all -- --check

# Run linter
cargo clippy --workspace --all-features
```

### Running the Server (when available)

```bash
# Build release version
cargo build --release

# Start server
./target/release/streamhouse-server

# In another terminal, use CLI
./target/release/streamctl topic create orders --partitions 3
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  StreamHouse                        │
├─────────────────────────────────────────────────────┤
│  Phase 1: Storage Layer (S3-native log)            │
│  Phase 2: Kafka Protocol (drop-in compatibility)   │
│  Phase 3: SQL Processing (streaming queries)       │
│  Phase 4: Distributed (multi-node fault-tolerant)  │
└─────────────────────────────────────────────────────┘
```

See [docs/phases/](docs/phases/) for detailed phase documentation.

## Project Structure

```
streamhouse/
├── crates/
│   ├── streamhouse-core/       # Shared types and traits
│   ├── streamhouse-storage/    # S3 segment management
│   ├── streamhouse-metadata/   # Topic/partition metadata
│   ├── streamhouse-kafka/      # Kafka protocol (Phase 2)
│   ├── streamhouse-sql/        # SQL processing (Phase 3)
│   ├── streamhouse-server/     # Main server binary
│   └── streamhouse-cli/        # Command-line tool
├── docs/                       # Documentation
│   └── phases/                 # Phase-by-phase guides
├── scripts/                    # Helper scripts
└── tests/                      # Integration tests
```

## Development Workflow

### Code Style

```bash
# Format code
cargo fmt --all

# Run linter
cargo clippy --workspace --all-features

# Fix clippy suggestions
cargo clippy --workspace --all-features --fix
```

### Testing

```bash
# Unit tests
cargo test --workspace

# Specific crate
cargo test -p streamhouse-storage

# With logging
RUST_LOG=debug cargo test
```

### Building

```bash
# Debug build (fast compilation)
cargo build --workspace

# Release build (optimized)
cargo build --workspace --release

# Specific binary
cargo build --bin streamhouse-server --release
```

## Phase 1 Roadmap

- [x] Week 1: Project setup
- [ ] Week 2: Segment format
- [ ] Week 3: Metadata store
- [ ] Week 4: Write path
- [ ] Week 5: Read path
- [ ] Week 6: API server
- [ ] Week 7: CLI tool
- [ ] Week 8: Testing & docs

See [docs/streaming-platform-project-plan-v2.md](docs/streaming-platform-project-plan-v2.md) for full roadmap.

## Documentation

- [Phase 1 Documentation](docs/phases/phase1/)
- [Project Plan](docs/streaming-platform-project-plan-v2.md)
- [Quick Reference](docs/streaming-platform-quick-reference.md)
- [Segment Format Deep Dive](docs/deep-dive-part2-s3-format.md)

## Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests and formatting (`cargo test && cargo fmt`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

Built on the shoulders of giants:
- [Apache Arrow](https://arrow.apache.org/) - Columnar data format
- [DataFusion](https://arrow.apache.org/datafusion/) - SQL query engine
- [Tokio](https://tokio.rs/) - Async runtime
- [object_store](https://docs.rs/object_store/) - Unified storage API

## Why StreamHouse?

### The Problem

Current streaming architectures are complex and expensive:
- Kafka for transport: Complex to operate, expensive to run
- Flink for processing: Separate system, more complexity
- Total cost: Infrastructure + operational overhead

### Our Solution

Single unified system:
- S3-native storage (no local disks to manage)
- Integrated SQL processing (no separate Flink cluster)
- 80%+ cost reduction
- Simpler operations

### Key Innovation

S3 is already durable and replicated. Why manage replication yourself?

## Contact

- GitHub Issues: For bug reports and feature requests
- Discussions: For questions and community chat

---

**Status:** Phase 1 in progress - contributions welcome!
