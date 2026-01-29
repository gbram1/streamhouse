#!/bin/bash
# Verify PostgreSQL and MinIO are running and configured correctly

echo "🔍 Verifying StreamHouse Setup..."
echo ""

# Check Docker containers
echo "1. Checking Docker containers..."
if docker ps | grep -q streamhouse-postgres; then
    echo "   ✅ PostgreSQL is running"
else
    echo "   ❌ PostgreSQL is NOT running"
    echo "      Run: docker-compose up -d"
fi

if docker ps | grep -q streamhouse-minio; then
    echo "   ✅ MinIO is running"
else
    echo "   ❌ MinIO is NOT running"
    echo "      Run: docker-compose up -d"
fi
echo ""

# Check PostgreSQL connection
echo "2. Checking PostgreSQL connection..."
if docker exec streamhouse-postgres pg_isready -U streamhouse &>/dev/null; then
    echo "   ✅ PostgreSQL is accepting connections"

    # Check if database exists
    if docker exec streamhouse-postgres psql -U streamhouse -lqt | cut -d \| -f 1 | grep -qw streamhouse_metadata; then
        echo "   ✅ Database 'streamhouse_metadata' exists"
    else
        echo "   ⚠️  Database 'streamhouse_metadata' does not exist (will be created on first run)"
    fi
else
    echo "   ❌ Cannot connect to PostgreSQL"
fi
echo ""

# Check MinIO
echo "3. Checking MinIO..."
if curl -sf http://localhost:9000/minio/health/live &>/dev/null; then
    echo "   ✅ MinIO API is healthy (http://localhost:9000)"
    echo "   ✅ MinIO Console: http://localhost:9001"

    # Check bucket
    if docker exec streamhouse-minio mc ls local/streamhouse &>/dev/null; then
        echo "   ✅ Bucket 'streamhouse' exists"
    else
        echo "   ⚠️  Bucket 'streamhouse' does not exist"
        echo "      Create it at: http://localhost:9001"
        echo "      Or run: docker exec streamhouse-minio mc mb local/streamhouse"
    fi
else
    echo "   ❌ Cannot reach MinIO"
fi
echo ""

# Check environment variables
echo "4. Checking environment variables..."
if [ -n "$DATABASE_URL" ]; then
    echo "   ✅ DATABASE_URL is set"
else
    echo "   ❌ DATABASE_URL is not set"
    echo "      Run: export DATABASE_URL='postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata'"
fi

if [ -n "$S3_ENDPOINT" ]; then
    echo "   ✅ S3_ENDPOINT is set"
else
    echo "   ❌ S3_ENDPOINT is not set"
    echo "      Run: export S3_ENDPOINT='http://localhost:9000'"
fi

if [ -n "$AWS_ACCESS_KEY_ID" ]; then
    echo "   ✅ AWS credentials are set"
else
    echo "   ❌ AWS credentials are not set"
    echo "      Run: export AWS_ACCESS_KEY_ID='minioadmin'"
    echo "           export AWS_SECRET_ACCESS_KEY='minioadmin'"
fi
echo ""

# Summary
echo "📋 Summary:"
echo "   To start with PostgreSQL and MinIO:"
echo "   1. docker-compose up -d"
echo "   2. ./start-with-postgres-minio.sh"
echo "   3. cd web && npm run dev"
echo ""
echo "   Or manually set environment variables and run:"
echo "   cargo run -p streamhouse-server --bin unified-server --features postgres"
