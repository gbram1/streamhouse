#!/bin/bash
# StreamHouse DigitalOcean Deployment Script
# Run this on a fresh Ubuntu 22.04+ Droplet (4GB+ RAM recommended)

set -e

echo "🚀 StreamHouse DigitalOcean Deployment"
echo "======================================"

# Check if running as root
if [ "$EUID" -eq 0 ]; then
    echo "Please run as a non-root user with sudo privileges"
    exit 1
fi

# Install Docker if not present
if ! command -v docker &> /dev/null; then
    echo "📦 Installing Docker..."
    curl -fsSL https://get.docker.com | sh
    sudo usermod -aG docker $USER
    echo "Docker installed. Please log out and back in, then re-run this script."
    exit 0
fi

# Install Docker Compose plugin if not present
if ! docker compose version &> /dev/null; then
    echo "📦 Installing Docker Compose plugin..."
    sudo apt-get update
    sudo apt-get install -y docker-compose-plugin
fi

# Check for .env file
if [ ! -f .env ]; then
    echo ""
    echo "❌ No .env file found!"
    echo ""
    echo "Before deploying, you need to:"
    echo "1. Create a DigitalOcean Managed PostgreSQL database"
    echo "2. Create a DigitalOcean Spaces bucket"
    echo "3. Copy .env.production.example to .env and fill in your values"
    echo ""
    echo "Run: cp .env.production.example .env"
    echo "Then edit .env with your values"
    exit 1
fi

# Source env file to check required vars
source .env

# Validate required environment variables
REQUIRED_VARS="DATABASE_URL S3_ENDPOINT S3_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY"
MISSING_VARS=""

for var in $REQUIRED_VARS; do
    if [ -z "${!var}" ]; then
        MISSING_VARS="$MISSING_VARS $var"
    fi
done

if [ -n "$MISSING_VARS" ]; then
    echo "❌ Missing required environment variables:$MISSING_VARS"
    echo "Please edit your .env file and set these values"
    exit 1
fi

echo "✅ Environment variables validated"

# Build Docker images locally (or pull from registry if available)
echo ""
echo "🔨 Building Docker images..."
echo "This may take 10-15 minutes on first run..."

# Build the server image
docker build -t streamhouse/agent:0.1.0 .

# Build the UI image
docker build -t streamhouse/ui:0.1.0 ./web

echo "✅ Images built successfully"

# Start services
echo ""
echo "🚀 Starting StreamHouse..."
docker compose -f docker-compose.prod.yml up -d

# Wait for health checks
echo ""
echo "⏳ Waiting for services to be healthy..."
sleep 10

# Check health
if curl -sf http://localhost:8080/health > /dev/null; then
    echo ""
    echo "✅ StreamHouse is running!"
    echo ""
    echo "🌐 Access points:"
    echo "   - API:     http://$(hostname -I | awk '{print $1}'):8080"
    echo "   - UI:      http://$(hostname -I | awk '{print $1}'):3000"
    echo "   - gRPC:    $(hostname -I | awk '{print $1}'):9090"
    echo ""
    echo "📊 To enable monitoring (Prometheus + Grafana):"
    echo "   docker compose -f docker-compose.prod.yml --profile monitoring up -d"
    echo ""
    echo "📝 View logs:"
    echo "   docker compose -f docker-compose.prod.yml logs -f"
else
    echo ""
    echo "❌ Health check failed. Check logs:"
    echo "   docker compose -f docker-compose.prod.yml logs"
    exit 1
fi
