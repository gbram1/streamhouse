#!/bin/bash
# Start StreamHouse server in development mode with local storage

# Create required directories
mkdir -p ./data/metadata
mkdir -p ./data/storage
mkdir -p ./data/cache

# Set environment variables for local development
export USE_LOCAL_STORAGE=1
export LOCAL_STORAGE_PATH=./data/storage
export STREAMHOUSE_METADATA=./data/metadata.db
export STREAMHOUSE_CACHE=./data/cache
export STREAMHOUSE_ADDR=0.0.0.0:9090

echo "Starting StreamHouse server in development mode..."
echo "Server will be available at: localhost:9090"
echo ""
echo "Configuration:"
echo "  Metadata: ./data/metadata.db"
echo "  Storage:  ./data/storage"
echo "  Cache:    ./data/cache"
echo ""

# Run the server
cargo run -p streamhouse-server
