#!/bin/bash
#
# Inspect MinIO Storage
#

echo "🪣 MinIO Storage Inspector"
echo "=========================="
echo ""

# Setup alias
docker exec streamhouse-minio mc alias set local http://localhost:9000 minioadmin minioadmin 2>/dev/null

echo "1. Available Buckets:"
docker exec streamhouse-minio mc ls local/ 2>/dev/null || echo "   No buckets found"
echo ""

echo "2. Objects in 'streamhouse' bucket:"
docker exec streamhouse-minio mc ls local/streamhouse/ 2>/dev/null || echo "   Bucket doesn't exist or is empty"
echo ""

echo "3. Full directory tree:"
docker exec streamhouse-minio mc tree local/streamhouse/ 2>/dev/null || docker exec streamhouse-minio mc ls local/streamhouse/ --recursive 2>/dev/null || echo "   No objects found"
echo ""

echo "4. Disk usage by prefix:"
docker exec streamhouse-minio mc du local/streamhouse/ 2>/dev/null || echo "   No data"
echo ""

echo "=========================="
echo "💡 Useful commands:"
echo ""
echo "  List all objects:"
echo "    docker exec streamhouse-minio mc ls local/streamhouse/ --recursive"
echo ""
echo "  Download a segment:"
echo "    docker exec streamhouse-minio mc cp local/streamhouse/TOPIC/PARTITION/SEGMENT.bin /tmp/"
echo ""
echo "  Get object info:"
echo "    docker exec streamhouse-minio mc stat local/streamhouse/TOPIC/PARTITION/SEGMENT.bin"
echo ""
echo "  View in browser:"
echo "    open http://localhost:9001"
echo "    (Login: minioadmin / minioadmin)"
