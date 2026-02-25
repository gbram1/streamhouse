#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Batched Durable Flush Benchmark
#
# Tests ACK_DURABLE throughput and latency with the batched flush optimization.
# Target: 40k writes/sec, <300ms p99 latency, full WAL fsync + S3 durable.
#
# Prerequisites:
#   - docker & docker-compose (for MinIO + Postgres)
#   - ghz (brew install ghz)
#   - cargo (Rust toolchain)
#
# Usage:
#   ./scripts/bench_durable_flush.sh              # MinIO (local)
#   ./scripts/bench_durable_flush.sh --real-s3    # Real AWS S3
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROTO_PATH="$PROJECT_DIR/crates/streamhouse-proto/proto/producer.proto"
AGENT_ADDR="localhost:9090"
AGENT_PID=""
TOPIC="bench-durable"
TOTAL_REQUESTS=40000
CONCURRENCY=200
CONNECTIONS=50

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    if [ -n "$AGENT_PID" ] && kill -0 "$AGENT_PID" 2>/dev/null; then
        kill "$AGENT_PID" 2>/dev/null || true
        wait "$AGENT_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

check_deps() {
    local missing=()
    command -v ghz >/dev/null 2>&1 || missing+=("ghz (brew install ghz)")
    command -v cargo >/dev/null 2>&1 || missing+=("cargo")
    command -v docker >/dev/null 2>&1 || missing+=("docker")

    if [ ${#missing[@]} -gt 0 ]; then
        echo -e "${RED}Missing dependencies:${NC}"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        exit 1
    fi
}

start_infra() {
    echo -e "${YELLOW}Starting MinIO + Postgres...${NC}"
    cd "$PROJECT_DIR"
    docker compose up -d postgres minio minio-init 2>/dev/null || docker-compose up -d postgres minio minio-init 2>/dev/null
    echo "Waiting for services to be healthy..."
    sleep 5
}

build_agent() {
    echo -e "${YELLOW}Building agent (release mode)...${NC}"
    cargo build --release -p streamhouse-agent 2>&1 | tail -3
}

start_agent_minio() {
    echo -e "${YELLOW}Starting agent with MinIO + WAL (sync=always)...${NC}"

    export AGENT_ID="bench-agent"
    export AGENT_ADDRESS="0.0.0.0:9090"
    export METADATA_STORE="$PROJECT_DIR/data/bench-metadata.db"
    export AWS_ACCESS_KEY_ID="minioadmin"
    export AWS_SECRET_ACCESS_KEY="minioadmin"
    export AWS_REGION="us-east-1"
    export S3_ENDPOINT="http://localhost:9000"
    export S3_BUCKET="streamhouse"
    export RUST_LOG="warn,streamhouse_storage::writer=info"
    export WAL_ENABLED="true"
    export WAL_SYNC_POLICY="always"
    export WAL_DIR="$PROJECT_DIR/data/bench-wal"

    mkdir -p "$PROJECT_DIR/data"

    "$PROJECT_DIR/target/release/agent" &
    AGENT_PID=$!
    echo "Agent PID: $AGENT_PID"
    sleep 3

    if ! kill -0 "$AGENT_PID" 2>/dev/null; then
        echo -e "${RED}Agent failed to start${NC}"
        exit 1
    fi
}

start_agent_real_s3() {
    echo -e "${YELLOW}Starting agent with real AWS S3 + WAL (sync=always)...${NC}"

    if [ -z "${AWS_ACCESS_KEY_ID:-}" ] || [ -z "${AWS_SECRET_ACCESS_KEY:-}" ]; then
        echo -e "${RED}AWS credentials not set. Export AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY.${NC}"
        exit 1
    fi

    export AGENT_ID="bench-agent"
    export AGENT_ADDRESS="0.0.0.0:9090"
    export METADATA_STORE="$PROJECT_DIR/data/bench-metadata.db"
    export AWS_REGION="${AWS_REGION:-us-east-1}"
    export S3_BUCKET="${S3_BUCKET:-streamhouse-bench}"
    export RUST_LOG="warn,streamhouse_storage::writer=info"
    export WAL_ENABLED="true"
    export WAL_SYNC_POLICY="always"
    export WAL_DIR="$PROJECT_DIR/data/bench-wal"

    # Unset MinIO endpoint so it uses real S3
    unset S3_ENDPOINT 2>/dev/null || true

    mkdir -p "$PROJECT_DIR/data"

    "$PROJECT_DIR/target/release/agent" &
    AGENT_PID=$!
    echo "Agent PID: $AGENT_PID"
    sleep 3

    if ! kill -0 "$AGENT_PID" 2>/dev/null; then
        echo -e "${RED}Agent failed to start${NC}"
        exit 1
    fi
}

create_topic() {
    echo -e "${YELLOW}Creating benchmark topic...${NC}"
    # Use grpcurl or the agent's built-in topic creation
    # The agent auto-creates topics on first write, so this is optional
    echo "Topic will be auto-created on first write"
}

run_benchmark() {
    local mode="$1"
    local record_size="${2:-1024}"

    echo ""
    echo -e "${GREEN}============================================${NC}"
    echo -e "${GREEN}  Benchmark: ACK_DURABLE batched flush${NC}"
    echo -e "${GREEN}  Mode: $mode${NC}"
    echo -e "${GREEN}  Total requests: $TOTAL_REQUESTS${NC}"
    echo -e "${GREEN}  Concurrency: $CONCURRENCY${NC}"
    echo -e "${GREEN}  Record size: ${record_size}B${NC}"
    echo -e "${GREEN}============================================${NC}"
    echo ""

    # Generate a base64 value of the desired size
    local value
    value=$(python3 -c "import base64; print(base64.b64encode(b'x' * $record_size).decode())")

    echo -e "${YELLOW}Running ACK_DURABLE benchmark...${NC}"
    ghz --insecure \
        --proto "$PROTO_PATH" \
        --call producer.ProducerService/Produce \
        --data "{\"topic\":\"$TOPIC\",\"partition\":0,\"ack_mode\":1,\"records\":[{\"value\":\"$value\",\"timestamp\":$(date +%s000)}]}" \
        --concurrency "$CONCURRENCY" \
        --total "$TOTAL_REQUESTS" \
        --connections "$CONNECTIONS" \
        --import-paths "$PROJECT_DIR/crates/streamhouse-proto/proto" \
        "$AGENT_ADDR" 2>&1

    echo ""
    echo -e "${YELLOW}Running ACK_BUFFERED benchmark (baseline comparison)...${NC}"
    ghz --insecure \
        --proto "$PROTO_PATH" \
        --call producer.ProducerService/Produce \
        --data "{\"topic\":\"${TOPIC}-buffered\",\"partition\":0,\"ack_mode\":0,\"records\":[{\"value\":\"$value\",\"timestamp\":$(date +%s000)}]}" \
        --concurrency "$CONCURRENCY" \
        --total "$TOTAL_REQUESTS" \
        --connections "$CONNECTIONS" \
        --import-paths "$PROJECT_DIR/crates/streamhouse-proto/proto" \
        "$AGENT_ADDR" 2>&1
}

# =============================================================================
# Main
# =============================================================================

check_deps

USE_REAL_S3=false
if [ "${1:-}" = "--real-s3" ]; then
    USE_REAL_S3=true
fi

echo -e "${GREEN}=== StreamHouse Batched Durable Flush Benchmark ===${NC}"
echo ""

build_agent

if [ "$USE_REAL_S3" = true ]; then
    # Still need Postgres for metadata
    start_infra
    start_agent_real_s3
    run_benchmark "Real AWS S3"
else
    start_infra
    start_agent_minio
    run_benchmark "MinIO (local)"
fi

echo ""
echo -e "${GREEN}=== Benchmark Complete ===${NC}"
echo ""
echo "Target:  40,000 writes/sec, p99 < 300ms"
echo "Config:  WAL sync=always, ACK_DURABLE (batched ~200ms window)"
echo ""
