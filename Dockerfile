# StreamHouse Unified Server Dockerfile
# Multi-stage build for minimal image size

# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY .sqlx ./.sqlx

# Enable SQLx offline mode (uses cached query metadata)
ENV SQLX_OFFLINE=true

# Use sparse protocol for faster crate downloads
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

# Increase network timeouts for slow connections
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV CARGO_HTTP_TIMEOUT=120

# Build release binary with postgres feature
# Disable LTO and use more codegen units to reduce memory usage in Docker
ENV CARGO_PROFILE_RELEASE_LTO=false
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8
RUN cargo build --release --bin unified-server --features postgres

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 streamhouse

# Copy binary from builder
COPY --from=builder /app/target/release/unified-server /app/unified-server

# Create directories for WAL
RUN mkdir -p /data/wal && chown -R streamhouse:streamhouse /data

# Switch to non-root user
USER streamhouse

# Environment defaults
ENV RUST_LOG=info
ENV HTTP_PORT=8080
ENV GRPC_PORT=9090
ENV WAL_DIR=/data/wal

# Expose ports
EXPOSE 8080 9090

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run the server
ENTRYPOINT ["/app/unified-server"]
