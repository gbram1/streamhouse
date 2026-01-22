#!/bin/bash

set -e

echo "Starting StreamHouse development environment..."

# Start Docker services
docker-compose up -d

# Wait for MinIO
echo "Waiting for MinIO..."
until curl -f http://localhost:9000/minio/health/live 2>/dev/null; do
    sleep 1
done

# Install MinIO client if not present
if ! command -v mc &> /dev/null; then
    echo "MinIO client (mc) not found. Please install it:"
    echo "  macOS: brew install minio/stable/mc"
    echo "  Linux: wget https://dl.min.io/client/mc/release/linux-amd64/mc && chmod +x mc"
    echo ""
    echo "Then run this script again to create the bucket."
    exit 0
fi

# Configure MinIO client
mc alias set local http://localhost:9000 minioadmin minioadmin 2>/dev/null || true

# Create bucket if not exists
mc mb local/streamhouse-dev 2>/dev/null || echo "Bucket already exists"

echo "✅ Development environment ready!"
echo "   MinIO API: http://localhost:9000"
echo "   MinIO Console: http://localhost:9001"
echo "   Username: minioadmin"
echo "   Password: minioadmin"
echo "   Bucket: streamhouse-dev"
