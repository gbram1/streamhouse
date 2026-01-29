#!/bin/bash
# StreamHouse startup script with PostgreSQL and MinIO

# PostgreSQL configuration
export DATABASE_URL="postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"

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

# Start the unified server
echo "🚀 Starting StreamHouse Unified Server with PostgreSQL and MinIO..."
echo "   PostgreSQL: localhost:5432 (database: streamhouse_metadata)"
echo "   MinIO:      localhost:9000 (bucket: streamhouse)"
echo "   MinIO UI:   http://localhost:9001"
echo ""

cargo run -p streamhouse-server --bin unified-server --features postgres
