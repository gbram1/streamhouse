#!/bin/bash
# StreamHouse startup script with SQLite (simplest setup)

# IMPORTANT: Skip sqlx compile-time verification
# This allows the build to succeed without a database file at compile time
export SQLX_OFFLINE=true

# Use SQLite database
export DATABASE_URL="sqlite://streamhouse.db"

# MinIO (S3-compatible) configuration
export S3_ENDPOINT="http://localhost:9000"
export STREAMHOUSE_BUCKET="streamhouse"
export AWS_ACCESS_KEY_ID="minioadmin"
export AWS_SECRET_ACCESS_KEY="minioadmin"
export AWS_REGION="us-east-1"

# Disable local storage (use MinIO instead)
unset USE_LOCAL_STORAGE

# Optional: Logging
export RUST_LOG="info"

echo "🚀 Starting StreamHouse with SQLite + MinIO..."
echo "   Database:   SQLite (streamhouse.db)"
echo "   MinIO:      localhost:9000 (bucket: streamhouse)"
echo "   MinIO UI:   http://localhost:9001"
echo "   gRPC API:   localhost:9090"
echo "   REST API:   localhost:8080"
echo "   Schema Reg: localhost:8081"
echo ""

# Build and run (default features = sqlite)
cargo run -p streamhouse-server --bin unified-server
