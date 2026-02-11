.PHONY: test coverage bench fuzz lint audit ci build clean

# Run all tests (uses nextest if installed, falls back to cargo test)
test:
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cargo nextest run --workspace --all-features; \
	else \
		cargo test --workspace --all-features; \
	fi

# Run doctests (nextest doesn't support these)
doctest:
	cargo test --workspace --doc

# Generate HTML coverage report
coverage:
	cargo llvm-cov --workspace --all-features --html
	@echo "Coverage report: target/llvm-cov/html/index.html"

# Print coverage summary to terminal
coverage-summary:
	cargo llvm-cov --workspace --all-features --summary-only

# Run all benchmarks
bench:
	cargo bench --workspace

# Run fuzz targets for 60 seconds each
fuzz:
	@echo "Running fuzz targets (60s each)..."
	cd fuzz && cargo +nightly fuzz run fuzz_segment_reader -- -max_total_time=60
	cd fuzz && cargo +nightly fuzz run fuzz_kafka_protocol -- -max_total_time=60
	cd fuzz && cargo +nightly fuzz run fuzz_sql_parser -- -max_total_time=60
	cd fuzz && cargo +nightly fuzz run fuzz_record_serde -- -max_total_time=60

# Run a single fuzz target (usage: make fuzz-one TARGET=fuzz_segment_reader)
fuzz-one:
	cd fuzz && cargo +nightly fuzz run $(TARGET) -- -max_total_time=60

# Lint: format check + clippy
lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-features -- -D warnings

# Format code
fmt:
	cargo fmt --all

# Security audit
audit:
	cargo audit
	cargo deny check

# Run everything CI would run
ci: lint test doctest audit build
	@echo "All CI checks passed!"

# Build release
build:
	cargo build --workspace --release

# Clean build artifacts
clean:
	cargo clean
