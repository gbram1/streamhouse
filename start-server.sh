#!/bin/bash
# StreamHouse Unified Server Startup Script
#
# Usage:
#   ./start-server.sh          # Start with PostgreSQL + MinIO
#   ./start-server.sh --local  # Start with local storage (SQLite + filesystem)
#   ./start-server.sh --stop   # Stop the server

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Log file
LOG_FILE="$SCRIPT_DIR/unified-server.log"

stop_server() {
    echo -e "${YELLOW}Stopping StreamHouse server...${NC}"
    pkill -f unified-server 2>/dev/null || true
    sleep 1

    if pgrep -f unified-server > /dev/null; then
        echo -e "${RED}Server still running, force killing...${NC}"
        pkill -9 -f unified-server 2>/dev/null || true
    fi

    echo -e "${GREEN}Server stopped${NC}"
}

check_docker() {
    if ! docker ps > /dev/null 2>&1; then
        echo -e "${RED}Docker is not running. Please start Docker first.${NC}"
        exit 1
    fi

    # Check if containers are running
    if ! docker ps | grep -q streamhouse-postgres; then
        echo -e "${YELLOW}Starting PostgreSQL container...${NC}"
        docker-compose -f docker/docker-compose.yml up -d postgres 2>/dev/null || \
        docker compose -f docker/docker-compose.yml up -d postgres
    fi

    if ! docker ps | grep -q streamhouse-minio; then
        echo -e "${YELLOW}Starting MinIO container...${NC}"
        docker-compose -f docker/docker-compose.yml up -d minio 2>/dev/null || \
        docker compose -f docker/docker-compose.yml up -d minio
    fi

    # Wait for services to be ready
    echo -e "${YELLOW}Waiting for services to be ready...${NC}"
    sleep 3
}

clean_stale_agents() {
    echo -e "${YELLOW}Cleaning up stale agent registrations...${NC}"
    PGPASSWORD=streamhouse_dev docker exec streamhouse-postgres \
        psql -U streamhouse -d streamhouse_metadata \
        -c "DELETE FROM agents WHERE last_heartbeat < NOW() - INTERVAL '1 hour';" \
        > /dev/null 2>&1 || true
}

start_with_postgres() {
    echo -e "${GREEN}Starting StreamHouse with PostgreSQL + MinIO...${NC}"

    check_docker
    clean_stale_agents

    # Build if needed
    if [ ! -f "$SCRIPT_DIR/target/release/unified-server" ]; then
        echo -e "${YELLOW}Building server (first time)...${NC}"
        cargo build -p streamhouse-server --bin unified-server --features postgres --release
    fi

    # Export environment variables
    export DATABASE_URL="postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
    export S3_ENDPOINT="http://localhost:9000"
    export AWS_ACCESS_KEY_ID="minioadmin"
    export AWS_SECRET_ACCESS_KEY="minioadmin"
    export AWS_REGION="us-east-1"

    # Start server
    ./target/release/unified-server > "$LOG_FILE" 2>&1 &
    SERVER_PID=$!

    sleep 2

    if ps -p $SERVER_PID > /dev/null 2>&1; then
        echo -e "${GREEN}StreamHouse server started (PID: $SERVER_PID)${NC}"
        echo ""
        echo "Services:"
        echo "  gRPC:           localhost:50051"
        echo "  REST API:       http://localhost:8080/api/v1"
        echo "  Web Console:    http://localhost:8080"
        echo "  Swagger UI:     http://localhost:8080/swagger-ui"
        echo ""
        echo "Logs: tail -f $LOG_FILE"
    else
        echo -e "${RED}Server failed to start. Check logs:${NC}"
        tail -20 "$LOG_FILE"
        exit 1
    fi
}

start_with_local() {
    echo -e "${GREEN}Starting StreamHouse with local storage (SQLite + filesystem)...${NC}"

    # Build if needed (without postgres feature)
    if [ ! -f "$SCRIPT_DIR/target/release/unified-server" ]; then
        echo -e "${YELLOW}Building server (first time)...${NC}"
        cargo build -p streamhouse-server --bin unified-server --release
    fi

    # Create data directories
    mkdir -p "$SCRIPT_DIR/data/storage"
    mkdir -p "$SCRIPT_DIR/data/cache"

    # Export environment variables
    export USE_LOCAL_STORAGE=1
    export LOCAL_STORAGE_PATH="$SCRIPT_DIR/data/storage"
    export STREAMHOUSE_METADATA="$SCRIPT_DIR/data/metadata.db"
    export STREAMHOUSE_CACHE="$SCRIPT_DIR/data/cache"

    # Start server
    ./target/release/unified-server > "$LOG_FILE" 2>&1 &
    SERVER_PID=$!

    sleep 2

    if ps -p $SERVER_PID > /dev/null 2>&1; then
        echo -e "${GREEN}StreamHouse server started (PID: $SERVER_PID)${NC}"
        echo ""
        echo "Services:"
        echo "  gRPC:           localhost:50051"
        echo "  REST API:       http://localhost:8080/api/v1"
        echo "  Web Console:    http://localhost:8080"
        echo ""
        echo "Logs: tail -f $LOG_FILE"
    else
        echo -e "${RED}Server failed to start. Check logs:${NC}"
        tail -20 "$LOG_FILE"
        exit 1
    fi
}

# Main
case "${1:-}" in
    --stop|-s)
        stop_server
        ;;
    --local|-l)
        stop_server
        start_with_local
        ;;
    --help|-h)
        echo "StreamHouse Server Startup Script"
        echo ""
        echo "Usage:"
        echo "  ./start-server.sh          Start with PostgreSQL + MinIO (production-like)"
        echo "  ./start-server.sh --local  Start with local storage (development)"
        echo "  ./start-server.sh --stop   Stop the server"
        echo "  ./start-server.sh --help   Show this help"
        ;;
    *)
        stop_server
        start_with_postgres
        ;;
esac
