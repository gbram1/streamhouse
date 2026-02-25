.PHONY: test coverage bench fuzz lint audit ci ci-full build clean \
       test-web test-sdks hooks docs docs-serve \
       cli-build-darwin-arm64 cli-build-darwin-x64 cli-build-linux-x64 \
       cli-build-linux-arm64 cli-build-all npm-pack npm-publish

# Run all Rust tests (uses nextest if installed, falls back to cargo test)
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

# Run web UI tests
test-web:
	cd web && npm test

# Run all SDK tests
test-sdks:
	@echo "=== Python SDK ==="
	cd sdks/python && pip install -e ".[dev]" -q && pytest -v
	@echo ""
	@echo "=== TypeScript SDK ==="
	cd sdks/typescript && npm ci --silent && npm test
	@echo ""
	@echo "=== Go SDK ==="
	cd sdks/go && go test -v ./...
	@echo ""
	@echo "=== Java SDK ==="
	cd sdks/java && mvn test -B -q

# Setup git pre-commit hooks
hooks:
	cp .githooks/pre-commit .git/hooks/pre-commit
	chmod +x .git/hooks/pre-commit
	@echo "Pre-commit hook installed."

# Run Rust CI checks
ci: lint test doctest audit build
	@echo "All Rust CI checks passed!"

# Run full CI (Rust + Web + SDKs)
ci-full: lint test doctest audit build test-web test-sdks
	@echo "All CI checks passed!"

# Build release
build:
	cargo build --workspace --release

# Build documentation site
docs:
	cd docs/book && mdbook build

# Serve documentation site locally with live reload
docs-serve:
	cd docs/book && mdbook serve --open

# Clean build artifacts
clean:
	cargo clean

# Cross-compilation targets for npm distribution
cli-build-darwin-arm64:
	cargo build --release -p streamhouse-cli --target aarch64-apple-darwin

cli-build-darwin-x64:
	cargo build --release -p streamhouse-cli --target x86_64-apple-darwin

cli-build-linux-x64:
	cargo build --release -p streamhouse-cli --target x86_64-unknown-linux-gnu

cli-build-linux-arm64:
	cargo build --release -p streamhouse-cli --target aarch64-unknown-linux-gnu

cli-build-all: cli-build-darwin-arm64 cli-build-darwin-x64 cli-build-linux-x64 cli-build-linux-arm64

npm-pack:
	cd npm/streamhouse-cli && npm pack

npm-publish:
	cd npm/streamhouse-cli && npm publish --access public
