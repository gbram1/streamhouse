#!/bin/bash
set -e

echo "Setting up MinIO for StreamHouse..."
echo

# Configure MinIO client
echo "Configuring MinIO client..."
docker exec streamhouse-minio mc alias set myminio http://localhost:9000 minioadmin minioadmin

# Create bucket if it doesn't exist
echo "Creating bucket 'streamhouse-data'..."
docker exec streamhouse-minio mc mb myminio/streamhouse-data --ignore-existing

# Set public policy for the bucket (for development)
echo "Setting bucket policy to allow read/write..."
docker exec streamhouse-minio mc anonymous set download myminio/streamhouse-data
docker exec streamhouse-minio mc anonymous set upload myminio/streamhouse-data

# Verify
echo
echo "MinIO setup complete!"
echo "Bucket: streamhouse-data"
echo "Access: minioadmin / minioadmin"
echo "Endpoint: http://localhost:9000"
echo "Console: http://localhost:9001"
