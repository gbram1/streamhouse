# StreamHouse Unified Server Dockerfile
# Multi-stage build with cargo-chef for dependency caching

FROM rust:1.85-bookworm AS chef
RUN cargo install cargo-chef
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libssl-dev \
    pkg-config \
    mold \
    clang \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

# Planner stage — analyzes dependencies to create a recipe
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage — cook dependencies first (cached), then build app
FROM chef AS builder

ENV SQLX_OFFLINE=true
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV CARGO_HTTP_TIMEOUT=120
ENV CARGO_PROFILE_RELEASE_LTO=false
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8

# Cook dependencies — this layer is cached as long as Cargo.toml/Cargo.lock don't change
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --profile test-release --recipe-path recipe.json --features postgres

# Now copy source and build — only this layer rebuilds on code changes
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY .sqlx ./.sqlx
RUN cargo build --profile test-release --bin unified-server --bin agent --features postgres

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 streamhouse

COPY --from=builder /app/target/test-release/unified-server /app/unified-server
COPY --from=builder /app/target/test-release/agent /app/agent

RUN mkdir -p /data/wal /data/cache && chown -R streamhouse:streamhouse /data

USER streamhouse

ENV RUST_LOG=info
ENV HTTP_PORT=8080
ENV GRPC_PORT=50051
ENV WAL_DIR=/data/wal
ENV STREAMHOUSE_CACHE=/data/cache

EXPOSE 8080 50051

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["/app/unified-server"]
