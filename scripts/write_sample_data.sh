#!/bin/bash
#
# Write Sample Data using SQLite (works immediately)
#

set -e

echo "🚀 Writing sample data to StreamHouse (SQLite)"
echo "=============================================="
echo ""

# Use SQLite for immediate testing
export DATABASE_URL=sqlite://streamhouse_demo.db
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_REGION=us-east-1
export AWS_ENDPOINT=http://localhost:9000

# Create bucket in MinIO if it doesn't exist
echo "🪣 Setting up MinIO bucket..."
docker exec streamhouse-minio mc alias set local http://localhost:9000 minioadmin minioadmin 2>/dev/null || true
docker exec streamhouse-minio mc mb local/streamhouse 2>/dev/null || echo "  Bucket already exists"
echo "✅ MinIO bucket ready"
echo ""

# Clean up old database
rm -f streamhouse_demo.db

# Build CLI if needed
if [ ! -f "target/debug/streamctl" ]; then
    echo "📦 Building StreamHouse CLI..."
    cargo build -p streamhouse-cli --quiet
    echo "✅ CLI built"
    echo ""
fi

echo "📝 Creating topic: demo-orders (3 partitions)"
./target/debug/streamctl topic create demo-orders --partitions 3
echo ""

echo "✍️  Writing sample records..."

# Write to partition 0
./target/debug/streamctl produce demo-orders 0 \
    --key order-1 \
    --value '{"item":"laptop","price":1299.99,"qty":1}'

./target/debug/streamctl produce demo-orders 0 \
    --key order-2 \
    --value '{"item":"mouse","price":29.99,"qty":2}'

./target/debug/streamctl produce demo-orders 0 \
    --key order-3 \
    --value '{"item":"keyboard","price":89.99,"qty":1}'

echo "✅ 3 records written to partition 0"
echo ""

# Write to partition 1
./target/debug/streamctl produce demo-orders 1 \
    --key user-1 \
    --value '{"action":"login","user_id":1}'

./target/debug/streamctl produce demo-orders 1 \
    --key user-2 \
    --value '{"action":"logout","user_id":2}'

echo "✅ 2 records written to partition 1"
echo ""

echo "📊 Listing topics:"
./target/debug/streamctl topic list
echo ""

echo "📖 Reading back records from partition 0:"
./target/debug/streamctl consume demo-orders 0 --offset 0 --count 3
echo ""

echo "✨ Sample data written successfully!"
echo ""
echo "Data locations:"
echo "  📊 SQLite: ./streamhouse_demo.db"
echo "  🪣 MinIO: streamhouse/demo-orders/"
echo ""
echo "Inspect with:"
echo "  • sqlite3 streamhouse_demo.db 'SELECT * FROM topics;'"
echo "  • docker exec streamhouse-minio mc ls local/streamhouse/demo-orders/ --recursive"
