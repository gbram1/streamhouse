# Contributing to StreamHouse

Thank you for your interest in contributing to StreamHouse! This document provides guidelines and instructions for contributing.

## Getting Started

### Prerequisites

- **Rust** 1.75+ (install via [rustup](https://rustup.rs/))
- **Protobuf compiler**: `brew install protobuf` (macOS) or `apt install protobuf-compiler` (Linux)

### Building

```bash
# Clone the repo
git clone https://github.com/gbram1/streamhouse.git
cd streamhouse

# Build the workspace
cargo build --workspace

# Run the server locally (no Docker needed)
mkdir -p ./data/storage
USE_LOCAL_STORAGE=1 cargo run --release --bin unified-server
```

### Running Tests

```bash
# Run all Rust tests
cargo test --workspace

# Run web UI tests
cd web && npm test
```

## How to Contribute

### Reporting Bugs

Open an issue with:
- A clear description of the bug
- Steps to reproduce
- Expected vs actual behavior
- Rust version (`rustc --version`) and OS

### Suggesting Features

Open an issue describing:
- The problem you're trying to solve
- Your proposed solution
- Any alternatives you've considered

### Submitting Code

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Ensure all checks pass:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test --workspace
   ```
5. Commit with a clear message describing the change
6. Push and open a Pull Request

### Code Style

- Run `cargo fmt` before committing
- Fix all `cargo clippy` warnings
- Follow existing patterns in the codebase
- Add tests for new functionality

## Project Structure

```
crates/
  streamhouse-core/        # Core types and traits
  streamhouse-storage/     # S3/local storage layer
  streamhouse-metadata/    # SQLite + PostgreSQL metadata backends
  streamhouse-server/      # Unified server (gRPC + REST + Kafka protocol)
  streamhouse-client/      # Rust client (Producer + Consumer)
  streamhouse-cli/         # CLI tool (streamctl)
  streamhouse-kafka/       # Kafka protocol compatibility
  streamhouse-sql/         # SQL query engine
  streamhouse-api/         # REST API layer
  streamhouse-proto/       # Protobuf definitions
  streamhouse-schema-registry/  # Schema registry
  streamhouse-observability/    # Metrics and tracing
  streamhouse-connectors/       # Source/sink connectors
web/                       # Next.js web UI
```

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
