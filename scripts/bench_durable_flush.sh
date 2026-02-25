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
#   ./scripts/bench_durable_flush.sh                          # 1 partition, MinIO
#   ./scripts/bench_durable_flush.sh -p 10                    # 10 partitions, MinIO
#   ./scripts/bench_durable_flush.sh -p 10 --real-s3          # 10 partitions, real S3
#   ./scripts/bench_durable_flush.sh -p 50 -n 200000          # 50 partitions, 200k requests
#
# Real S3 setup:
#   export AWS_ACCESS_KEY_ID="your-key"
#   export AWS_SECRET_ACCESS_KEY="your-secret"
#   export AWS_REGION="us-east-1"
#   export STREAMHOUSE_BUCKET="your-bucket"  # must already exist
#   ./scripts/bench_durable_flush.sh -p 10 --real-s3
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROTO_PATH="$PROJECT_DIR/crates/streamhouse-proto/proto/producer.proto"
AGENT_ADDR="localhost:9090"
AGENT_PID=""
TOPIC="bench-durable"

# Defaults (overridable via CLI args)
PARTITIONS=1
TOTAL_REQUESTS=40000
CONCURRENCY=200
CONNECTIONS=50
RECORD_SIZE=1024
USE_REAL_S3=false

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Parse CLI args
while [[ $# -gt 0 ]]; do
    case $1 in
        -p|--partitions) PARTITIONS="$2"; shift 2 ;;
        -n|--requests)   TOTAL_REQUESTS="$2"; shift 2 ;;
        -c|--concurrency) CONCURRENCY="$2"; shift 2 ;;
        -s|--record-size) RECORD_SIZE="$2"; shift 2 ;;
        --real-s3)       USE_REAL_S3=true; shift ;;
        -h|--help)
            echo "Usage: $0 [-p partitions] [-n requests] [-c concurrency] [-s record_size] [--real-s3]"
            echo ""
            echo "Options:"
            echo "  -p, --partitions   Number of partitions (default: 1)"
            echo "  -n, --requests     Total requests (default: 40000)"
            echo "  -c, --concurrency  Concurrent workers (default: 200)"
            echo "  -s, --record-size  Record size in bytes (default: 1024)"
            echo "  --real-s3          Use real AWS S3 instead of MinIO"
            exit 0 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

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

# Run SQL against the Dockerized Postgres
run_psql() {
    docker exec -i streamhouse-postgres psql -U streamhouse -d streamhouse -q "$@"
}

build_agent() {
    echo -e "${YELLOW}Building agent (release mode, with postgres)...${NC}"
    cargo build --release -p streamhouse-agent --features postgres 2>&1 | tail -3
}

setup_agent_env_minio() {
    export AGENT_ID="bench-agent"
    export AGENT_ADDRESS="0.0.0.0:9090"
    export METADATA_STORE="postgres://streamhouse:streamhouse@localhost:5432/streamhouse"
    export AWS_ACCESS_KEY_ID="minioadmin"
    export AWS_SECRET_ACCESS_KEY="minioadmin"
    export AWS_REGION="us-east-1"
    export S3_ENDPOINT="http://localhost:9000"
    export AWS_ENDPOINT_URL="http://localhost:9000"
    export STREAMHOUSE_BUCKET="streamhouse"
    export RUST_LOG="warn,streamhouse_storage::writer=info"
    export WAL_ENABLED="true"
    export WAL_SYNC_POLICY="always"
    export WAL_DIR="$PROJECT_DIR/data/bench-wal"
    export MANAGED_TOPICS="$TOPIC,${TOPIC}-buffered"
    export THROTTLE_ENABLED="false"

    mkdir -p "$PROJECT_DIR/data"
    rm -rf "$PROJECT_DIR/data/bench-wal"

    # Reset Postgres tables for clean benchmark
    echo -e "${YELLOW}Resetting Postgres metadata...${NC}"
    run_psql -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;" 2>/dev/null || true
}

setup_agent_env_real_s3() {
    if [ -z "${AWS_ACCESS_KEY_ID:-}" ] || [ -z "${AWS_SECRET_ACCESS_KEY:-}" ]; then
        echo -e "${RED}AWS credentials not set.${NC}"
        echo "  export AWS_ACCESS_KEY_ID=\"your-key\""
        echo "  export AWS_SECRET_ACCESS_KEY=\"your-secret\""
        echo "  export AWS_REGION=\"us-east-1\""
        echo "  export STREAMHOUSE_BUCKET=\"your-bucket\""
        exit 1
    fi

    export AGENT_ID="bench-agent"
    export AGENT_ADDRESS="0.0.0.0:9090"
    export METADATA_STORE="postgres://streamhouse:streamhouse@localhost:5432/streamhouse"
    export AWS_REGION="${AWS_REGION:-us-east-1}"
    export STREAMHOUSE_BUCKET="${STREAMHOUSE_BUCKET:-streamhouse-bench}"
    export RUST_LOG="warn,streamhouse_storage::writer=info"
    export WAL_ENABLED="true"
    export WAL_SYNC_POLICY="always"
    export WAL_DIR="$PROJECT_DIR/data/bench-wal"
    export MANAGED_TOPICS="$TOPIC,${TOPIC}-buffered"
    export THROTTLE_ENABLED="false"

    # Unset MinIO endpoint so it uses real S3
    unset S3_ENDPOINT 2>/dev/null || true
    unset AWS_ENDPOINT_URL 2>/dev/null || true

    mkdir -p "$PROJECT_DIR/data"
    rm -rf "$PROJECT_DIR/data/bench-wal"

    # Reset Postgres tables for clean benchmark
    echo -e "${YELLOW}Resetting Postgres metadata...${NC}"
    run_psql -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;" 2>/dev/null || true
}

start_agent() {
    # Kill any existing agent on port 9090
    local old_pid
    old_pid=$(lsof -ti :9090 2>/dev/null || true)
    if [ -n "$old_pid" ]; then
        echo -e "${YELLOW}Killing stale agent on port 9090 (PID: $old_pid)...${NC}"
        kill -9 "$old_pid" 2>/dev/null || true
        sleep 2
    fi

    local log_file="$PROJECT_DIR/data/agent.log"
    echo -e "${YELLOW}Starting agent (logs → data/agent.log)...${NC}"
    "$PROJECT_DIR/target/release/agent" > "$log_file" 2>&1 &
    AGENT_PID=$!
    echo "Agent PID: $AGENT_PID"

    # Wait for gRPC port to be ready
    echo "Waiting for gRPC port 9090..."
    for i in $(seq 1 15); do
        if lsof -ti :9090 >/dev/null 2>&1 && kill -0 "$AGENT_PID" 2>/dev/null; then
            echo "  Port 9090 ready"
            sleep 1
            return
        fi
        sleep 1
    done

    echo -e "${RED}Agent failed to start (port 9090 not ready after 15s)${NC}"
    exit 1
}

seed_topics() {
    echo -e "${YELLOW}Seeding topics (${PARTITIONS} partitions each)...${NC}"
    local now_epoch
    now_epoch=$(date +%s)
    local org_id="00000000-0000-0000-0000-000000000000"

    for t in "$TOPIC" "${TOPIC}-buffered"; do
        # Build all partition inserts into a single SQL batch
        local partition_sql=""
        for ((p=0; p<PARTITIONS; p++)); do
            partition_sql+="INSERT INTO partitions (topic, partition_id, high_watermark, created_at, updated_at, organization_id) VALUES ('$t', $p, 0, $now_epoch, $now_epoch, '$org_id') ON CONFLICT (topic, partition_id) DO NOTHING;"
        done

        run_psql <<SQL
INSERT INTO topics (name, partition_count, retention_ms, created_at, updated_at, config, organization_id)
VALUES ('$t', $PARTITIONS, NULL, $now_epoch, $now_epoch, '{}', '$org_id')
ON CONFLICT (organization_id, name) DO NOTHING;
$partition_sql
SQL
        echo "  Created topic: $t ($PARTITIONS partitions)"
    done
}

run_benchmark() {
    local mode="$1"
    local requests_per_partition=$((TOTAL_REQUESTS / PARTITIONS))
    local concurrency_per_partition=$((CONCURRENCY / PARTITIONS))
    # Ensure at least 1
    [ "$concurrency_per_partition" -lt 1 ] && concurrency_per_partition=1
    [ "$requests_per_partition" -lt 100 ] && requests_per_partition=100

    echo ""
    echo -e "${GREEN}============================================${NC}"
    echo -e "${GREEN}  Benchmark: ACK_DURABLE batched flush${NC}"
    echo -e "${GREEN}  Mode: $mode${NC}"
    echo -e "${GREEN}  Partitions: $PARTITIONS${NC}"
    echo -e "${GREEN}  Total requests: $TOTAL_REQUESTS ($requests_per_partition/partition)${NC}"
    echo -e "${GREEN}  Concurrency: $CONCURRENCY ($concurrency_per_partition/partition)${NC}"
    echo -e "${GREEN}  Record size: ${RECORD_SIZE}B${NC}"
    echo -e "${GREEN}============================================${NC}"
    echo ""

    local value
    value=$(python3 -c "import base64; print(base64.b64encode(b'x' * $RECORD_SIZE).decode())")

    local tmpdir
    tmpdir=$(mktemp -d)

    # --- ACK_DURABLE ---
    echo -e "${YELLOW}Running ACK_DURABLE benchmark ($PARTITIONS partitions)...${NC}"
    local durable_start
    durable_start=$(python3 -c "import time; print(time.time())")

    # Launch one ghz per partition in parallel
    local ghz_pids=()
    for ((p=0; p<PARTITIONS; p++)); do
        ghz --insecure \
            --proto "$PROTO_PATH" \
            --call streamhouse.producer.ProducerService/Produce \
            --data "{\"topic\":\"$TOPIC\",\"partition\":$p,\"ack_mode\":1,\"records\":[{\"value\":\"$value\",\"timestamp\":$(date +%s000)}]}" \
            --concurrency "$concurrency_per_partition" \
            --total "$requests_per_partition" \
            --connections "$((CONNECTIONS / PARTITIONS > 0 ? CONNECTIONS / PARTITIONS : 1))" \
            --connect-timeout 5s \
            --import-paths "$PROJECT_DIR/crates/streamhouse-proto/proto" \
            --format json \
            "$AGENT_ADDR" > "$tmpdir/durable_p${p}.json" 2>&1 &
        ghz_pids+=($!)
    done
    for pid in "${ghz_pids[@]}"; do wait "$pid"; done

    local durable_end
    durable_end=$(python3 -c "import time; print(time.time())")
    local durable_elapsed
    durable_elapsed=$(python3 -c "print(f'{$durable_end - $durable_start:.2f}')")

    # Aggregate results
    echo -e "${CYAN}--- ACK_DURABLE Results (aggregated across $PARTITIONS partitions) ---${NC}"
    python3 -c "
import json, glob, sys

files = sorted(glob.glob('$tmpdir/durable_p*.json'))
total_count = 0
total_ok = 0
total_errors = 0
all_latencies = []
total_rps = 0

for f in files:
    with open(f) as fh:
        data = json.load(fh)
    total_count += data.get('count', 0)
    total_rps += data.get('rps', 0)
    # statusCodeDistribution
    dist = data.get('statusCodeDistribution', {})
    total_ok += dist.get('OK', 0)
    for k, v in dist.items():
        if k != 'OK':
            total_errors += v
    # latencyDistribution
    for ld in data.get('latencyDistribution', []):
        all_latencies.append(ld)

print(f'  Total requests:  {total_count}')
print(f'  OK responses:    {total_ok}')
print(f'  Errors:          {total_errors}')
print(f'  Wall clock time: $durable_elapsed s')
print(f'  Aggregate RPS:   {total_rps:.0f}')
print(f'  Effective RPS:   {total_count / float($durable_elapsed):.0f}')
print()

# Show per-partition latency from first and last partition
for i, f in enumerate(files):
    with open(f) as fh:
        data = json.load(fh)
    avg_ms = data.get('average', 0) / 1e6
    fastest_ms = data.get('fastest', 0) / 1e6
    slowest_ms = data.get('slowest', 0) / 1e6
    ld_list = data.get('latencyDistribution') or []
    lats = {ld['percentage']: ld['latency']/1e6 for ld in ld_list}
    p50 = lats.get(50, 0)
    p99 = lats.get(99, 0)
    rps = data.get('rps', 0)
    print(f'  Partition {i}: avg={avg_ms:.0f}ms p50={p50:.0f}ms p99={p99:.0f}ms rps={rps:.0f}')
"

    echo ""

    # --- ACK_BUFFERED ---
    echo -e "${YELLOW}Running ACK_BUFFERED benchmark ($PARTITIONS partitions, baseline)...${NC}"
    local buffered_start
    buffered_start=$(python3 -c "import time; print(time.time())")

    ghz_pids=()
    for ((p=0; p<PARTITIONS; p++)); do
        ghz --insecure \
            --proto "$PROTO_PATH" \
            --call streamhouse.producer.ProducerService/Produce \
            --data "{\"topic\":\"${TOPIC}-buffered\",\"partition\":$p,\"ack_mode\":0,\"records\":[{\"value\":\"$value\",\"timestamp\":$(date +%s000)}]}" \
            --concurrency "$concurrency_per_partition" \
            --total "$requests_per_partition" \
            --connections "$((CONNECTIONS / PARTITIONS > 0 ? CONNECTIONS / PARTITIONS : 1))" \
            --connect-timeout 5s \
            --import-paths "$PROJECT_DIR/crates/streamhouse-proto/proto" \
            --format json \
            "$AGENT_ADDR" > "$tmpdir/buffered_p${p}.json" 2>&1 &
        ghz_pids+=($!)
    done
    for pid in "${ghz_pids[@]}"; do wait "$pid"; done

    local buffered_end
    buffered_end=$(python3 -c "import time; print(time.time())")
    local buffered_elapsed
    buffered_elapsed=$(python3 -c "print(f'{$buffered_end - $buffered_start:.2f}')")

    echo -e "${CYAN}--- ACK_BUFFERED Results (aggregated across $PARTITIONS partitions) ---${NC}"
    python3 -c "
import json, glob

files = sorted(glob.glob('$tmpdir/buffered_p*.json'))
total_count = 0
total_ok = 0
total_rps = 0

for f in files:
    with open(f) as fh:
        data = json.load(fh)
    total_count += data.get('count', 0)
    total_rps += data.get('rps', 0)
    dist = data.get('statusCodeDistribution', {})
    total_ok += dist.get('OK', 0)

print(f'  Total requests:  {total_count}')
print(f'  OK responses:    {total_ok}')
print(f'  Wall clock time: $buffered_elapsed s')
print(f'  Aggregate RPS:   {total_rps:.0f}')
print(f'  Effective RPS:   {total_count / float($buffered_elapsed):.0f}')
print()

for i, f in enumerate(files):
    with open(f) as fh:
        data = json.load(fh)
    avg_ms = data.get('average', 0) / 1e6
    ld_list = data.get('latencyDistribution') or []
    lats = {ld['percentage']: ld['latency']/1e6 for ld in ld_list}
    p50 = lats.get(50, 0)
    p99 = lats.get(99, 0)
    rps = data.get('rps', 0)
    print(f'  Partition {i}: avg={avg_ms:.0f}ms p50={p50:.0f}ms p99={p99:.0f}ms rps={rps:.0f}')
"

    # Cleanup temp files
    rm -rf "$tmpdir"
}

# =============================================================================
# Main
# =============================================================================

check_deps

echo -e "${GREEN}=== StreamHouse Batched Durable Flush Benchmark ===${NC}"
echo -e "  Partitions: $PARTITIONS"
echo -e "  Requests:   $TOTAL_REQUESTS"
echo -e "  S3 backend: $([ "$USE_REAL_S3" = true ] && echo "Real AWS S3" || echo "MinIO (local)")"
echo ""

build_agent

if [ "$USE_REAL_S3" = true ]; then
    # Still need Postgres for metadata store (or use SQLite)
    setup_agent_env_real_s3
else
    start_infra
    setup_agent_env_minio
fi

# Bootstrap: start agent to create DB schema, stop, seed topics, restart
echo -e "${YELLOW}Bootstrapping database schema...${NC}"
start_agent
sleep 1
kill "$AGENT_PID" 2>/dev/null || true; wait "$AGENT_PID" 2>/dev/null || true; AGENT_PID=""
sleep 3  # Wait for port 9090 to be released

seed_topics

echo -e "${YELLOW}Restarting agent with seeded topics...${NC}"
start_agent
sleep 3

if [ "$USE_REAL_S3" = true ]; then
    run_benchmark "Real AWS S3"
else
    run_benchmark "MinIO (local)"
fi

echo ""
echo -e "${GREEN}=== Benchmark Complete ===${NC}"
echo ""
echo "Target:  40,000 writes/sec, p99 < 300ms"
echo "Config:  WAL sync=always, ACK_DURABLE (batched ~200ms window)"
echo "         $PARTITIONS partitions, $([ "$USE_REAL_S3" = true ] && echo "Real S3" || echo "MinIO")"
echo ""
