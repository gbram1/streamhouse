#!/usr/bin/env bash
# StreamHouse Test Library — shared functions for all test phases

set -euo pipefail

# ── Colors ────────────────────────────────────────────────────────────────────
if [ "${NO_COLOR:-0}" = "1" ] || [ "${CI:-}" = "true" ] && [ "${FORCE_COLOR:-0}" != "1" ]; then
    RED=''
    GREEN=''
    YELLOW=''
    BOLD=''
    DIM=''
    NC=''
else
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BOLD='\033[1m'
    DIM='\033[2m'
    NC='\033[0m'
fi

# ── Test Counters ─────────────────────────────────────────────────────────────
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0
CURRENT_PHASE=""

# ── Configuration ─────────────────────────────────────────────────────────────
# Use non-default ports to avoid conflicts with any running instance
export TEST_GRPC_PORT="${TEST_GRPC_PORT:-50151}"
export TEST_HTTP_PORT="${TEST_HTTP_PORT:-8180}"
export TEST_KAFKA_PORT="${TEST_KAFKA_PORT:-9192}"
export TEST_HTTP="http://localhost:${TEST_HTTP_PORT}"
export TEST_GRPC="localhost:${TEST_GRPC_PORT}"

# Storage backend: "local" (default) or "postgres-s3"
export TEST_BACKEND="${TEST_BACKEND:-local}"

# Postgres + S3/MinIO settings (used when TEST_BACKEND=postgres-s3)
export TEST_PG_PORT="${TEST_PG_PORT:-5433}"
export TEST_MINIO_PORT="${TEST_MINIO_PORT:-9100}"
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgres://streamhouse:streamhouse@localhost:${TEST_PG_PORT}/streamhouse_test}"
export TEST_S3_ENDPOINT="${TEST_S3_ENDPOINT:-http://localhost:${TEST_MINIO_PORT}}"
export TEST_S3_BUCKET="${TEST_S3_BUCKET:-streamhouse-test}"
export TEST_AWS_ACCESS_KEY_ID="${TEST_AWS_ACCESS_KEY_ID:-minioadmin}"
export TEST_AWS_SECRET_ACCESS_KEY="${TEST_AWS_SECRET_ACCESS_KEY:-minioadmin}"
export TEST_AWS_REGION="${TEST_AWS_REGION:-us-east-1}"

# Root of the project
export PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
export BINARY="${PROJECT_ROOT}/target/test-release/unified-server"
export STREAMCTL="${PROJECT_ROOT}/target/test-release/streamctl"

# Test data directory (isolated per run)
export TEST_TMPDIR="${TEST_TMPDIR:-$(mktemp -d /tmp/streamhouse-test-XXXXXX)}"
export TEST_DATA_DIR="${TEST_TMPDIR}/data"
export TEST_LOG="${TEST_TMPDIR}/server.log"
export TEST_PID_FILE="${TEST_TMPDIR}/server.pid"

# Results directory
export RESULTS_DIR="${PROJECT_ROOT}/tests/benchmarks/results"

# ── Dependency Checks ─────────────────────────────────────────────────────────

check_required_deps() {
    local missing=()
    for cmd in curl jq python3; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done
    if [ ${#missing[@]} -gt 0 ]; then
        echo -e "${RED}Missing required dependencies: ${missing[*]}${NC}"
        echo "  Install with: brew install ${missing[*]}"
        exit 1
    fi
}

check_optional_deps() {
    if command -v grpcurl &>/dev/null; then
        export HAS_GRPCURL=1
    else
        export HAS_GRPCURL=0
        echo -e "${YELLOW}grpcurl not found — Phase 03 (gRPC API) will be skipped${NC}"
        echo -e "${DIM}  Install with: brew install grpcurl${NC}"
    fi

    if command -v ghz &>/dev/null; then
        export HAS_GHZ=1
    else
        export HAS_GHZ=0
        echo -e "${YELLOW}ghz not found — gRPC benchmark will fall back to stress_test_e2e${NC}"
        echo -e "${DIM}  Install with: brew install ghz${NC}"
    fi

    if command -v kcat &>/dev/null; then
        export HAS_KCAT=1
        export KCAT_CMD="kcat"
    elif command -v kafkacat &>/dev/null; then
        export HAS_KCAT=1
        export KCAT_CMD="kafkacat"
    else
        export HAS_KCAT=0
        echo -e "${YELLOW}kcat not found — Phase 11 (Kafka Protocol) will be skipped${NC}"
        echo -e "${DIM}  Install with: brew install kcat${NC}"
    fi
}

# ── Infrastructure Lifecycle (Postgres + MinIO) ──────────────────────────────

wait_for_postgres() {
    local max_wait="${1:-30}"
    local url="${TEST_DATABASE_URL}"
    echo -e "${DIM}Waiting for Postgres at ${url%%@*}@...${NC}"
    for i in $(seq 1 "$max_wait"); do
        # Try a simple psql connect or pg_isready; fall back to curl-based TCP probe
        if command -v pg_isready &>/dev/null; then
            # Extract host and port from DATABASE_URL
            local pg_host pg_port
            pg_host=$(echo "$url" | sed -E 's|.*@([^:/]+).*|\1|')
            pg_port=$(echo "$url" | sed -E 's|.*:([0-9]+)/.*|\1|')
            if pg_isready -h "$pg_host" -p "$pg_port" -U streamhouse -q 2>/dev/null; then
                echo -e "${GREEN}Postgres ready${NC} (${i}s)"
                return 0
            fi
        else
            # TCP probe fallback: try to connect and immediately disconnect
            local pg_host pg_port
            pg_host=$(echo "$url" | sed -E 's|.*@([^:/]+).*|\1|')
            pg_port=$(echo "$url" | sed -E 's|.*:([0-9]+)/.*|\1|')
            if (echo > /dev/tcp/"$pg_host"/"$pg_port") 2>/dev/null; then
                echo -e "${GREEN}Postgres ready${NC} (${i}s)"
                return 0
            fi
        fi
        if [ "$i" -eq "$max_wait" ]; then
            echo -e "${RED}Postgres failed to become ready after ${max_wait}s${NC}"
            return 1
        fi
        sleep 1
    done
}

wait_for_minio() {
    local max_wait="${1:-30}"
    local endpoint="${TEST_S3_ENDPOINT}"
    echo -e "${DIM}Waiting for MinIO at ${endpoint}...${NC}"
    for i in $(seq 1 "$max_wait"); do
        if curl -sf "${endpoint}/minio/health/live" > /dev/null 2>&1; then
            echo -e "${GREEN}MinIO ready${NC} (${i}s)"
            return 0
        fi
        if [ "$i" -eq "$max_wait" ]; then
            echo -e "${RED}MinIO failed to become ready after ${max_wait}s${NC}"
            return 1
        fi
        sleep 1
    done
}

setup_minio_bucket() {
    local bucket="${TEST_S3_BUCKET}"
    local endpoint="${TEST_S3_ENDPOINT}"
    local access_key="${TEST_AWS_ACCESS_KEY_ID}"
    local secret_key="${TEST_AWS_SECRET_ACCESS_KEY}"

    echo -e "${DIM}Creating MinIO bucket: ${bucket}${NC}"

    if command -v mc &>/dev/null; then
        mc alias set streamhouse-test "$endpoint" "$access_key" "$secret_key" --api s3v4 2>/dev/null || true
        mc mb "streamhouse-test/${bucket}" --ignore-existing 2>/dev/null
    else
        # Fallback: create bucket via raw S3 PUT (unsigned, works with MinIO default config)
        local date_header
        date_header=$(date -u "+%a, %d %b %Y %H:%M:%S GMT" 2>/dev/null || date -u "+%a, %d %b %Y %T GMT")

        # Use curl with AWS Signature-less approach (MinIO allows this for bucket creation with root creds)
        # Actually use the S3 CreateBucket API with basic auth header
        curl -sf -X PUT \
            "${endpoint}/${bucket}" \
            -H "Date: ${date_header}" \
            -u "${access_key}:${secret_key}" \
            > /dev/null 2>&1 || true

        # Verify bucket exists by listing it
        if curl -sf "${endpoint}/${bucket}" -u "${access_key}:${secret_key}" > /dev/null 2>&1; then
            echo -e "${GREEN}Bucket '${bucket}' ready${NC}"
        else
            # Try python-based creation as last resort
            python3 -c "
import urllib.request, urllib.error
req = urllib.request.Request('${endpoint}/${bucket}', method='PUT')
try:
    urllib.request.urlopen(req)
except urllib.error.HTTPError as e:
    if e.code not in (200, 409):  # 409 = BucketAlreadyOwnedByYou
        raise
" 2>/dev/null || echo -e "${YELLOW}Warning: could not verify bucket creation (may already exist)${NC}"
        fi
    fi
}

# ── Server Lifecycle ──────────────────────────────────────────────────────────

build_server() {
    echo -e "${BOLD}Building unified-server (test-release, no LTO)...${NC}"
    cargo build --profile test-release -p streamhouse-server --bin unified-server --manifest-path "${PROJECT_ROOT}/Cargo.toml" 2>&1
    if [ ! -f "$BINARY" ]; then
        echo -e "${RED}Build failed: binary not found at $BINARY${NC}"
        exit 1
    fi
    echo -e "${GREEN}Build complete${NC}"
}

build_server_postgres() {
    echo -e "${BOLD}Building unified-server (test-release, --features postgres)...${NC}"
    cargo build --profile test-release -p streamhouse-server --bin unified-server \
        --features postgres \
        --manifest-path "${PROJECT_ROOT}/Cargo.toml" 2>&1
    if [ ! -f "$BINARY" ]; then
        echo -e "${RED}Build failed: binary not found at $BINARY${NC}"
        exit 1
    fi
    echo -e "${GREEN}Build complete (with postgres feature)${NC}"
}

build_cli() {
    echo -e "${BOLD}Building streamctl (test-release, no LTO)...${NC}"
    cargo build --profile test-release -p streamhouse-cli --bin streamctl --manifest-path "${PROJECT_ROOT}/Cargo.toml" 2>&1
    if [ ! -f "$STREAMCTL" ]; then
        echo -e "${YELLOW}streamctl binary not found — CLI tests will be skipped${NC}"
        export HAS_STREAMCTL=0
    else
        export HAS_STREAMCTL=1
        echo -e "${GREEN}streamctl built${NC}"
    fi
}

start_server() {
    # Create data directories
    mkdir -p "${TEST_DATA_DIR}/storage" "${TEST_DATA_DIR}/cache"

    echo -e "Starting server ${DIM}(ports: gRPC=$TEST_GRPC_PORT HTTP=$TEST_HTTP_PORT Kafka=$TEST_KAFKA_PORT backend=$TEST_BACKEND)${NC}"

    # Common env vars
    local env_vars=(
        STREAMHOUSE_CACHE="${TEST_DATA_DIR}/cache"
        STREAMHOUSE_CACHE_SIZE=104857600
        GRPC_ADDR="0.0.0.0:${TEST_GRPC_PORT}"
        HTTP_ADDR="0.0.0.0:${TEST_HTTP_PORT}"
        KAFKA_ADDR="0.0.0.0:${TEST_KAFKA_PORT}"
        RUST_LOG=info
    )

    if [ "$TEST_BACKEND" = "postgres-s3" ]; then
        # Postgres + S3/MinIO backend
        env_vars+=(
            DATABASE_URL="${TEST_DATABASE_URL}"
            S3_ENDPOINT="${TEST_S3_ENDPOINT}"
            STREAMHOUSE_BUCKET="${TEST_S3_BUCKET}"
            AWS_ACCESS_KEY_ID="${TEST_AWS_ACCESS_KEY_ID}"
            AWS_SECRET_ACCESS_KEY="${TEST_AWS_SECRET_ACCESS_KEY}"
            AWS_REGION="${TEST_AWS_REGION}"
            STREAMHOUSE_METADATA=postgres
        )
    else
        # Local SQLite + filesystem backend
        env_vars+=(
            USE_LOCAL_STORAGE=1
            LOCAL_STORAGE_PATH="${TEST_DATA_DIR}/storage"
            STREAMHOUSE_METADATA="${TEST_DATA_DIR}/metadata.db"
        )
    fi

    env "${env_vars[@]}" "$BINARY" > "$TEST_LOG" 2>&1 &

    echo $! > "$TEST_PID_FILE"
    echo -e "${DIM}  PID: $(cat "$TEST_PID_FILE")  Log: $TEST_LOG${NC}"
}

wait_healthy() {
    local max_wait="${1:-30}"
    for i in $(seq 1 "$max_wait"); do
        if curl -sf "${TEST_HTTP}/health" > /dev/null 2>&1; then
            echo -e "${GREEN}Server ready${NC} (${i}s)"
            return 0
        fi
        if [ "$i" -eq "$max_wait" ]; then
            echo -e "${RED}Server failed to start after ${max_wait}s${NC}"
            echo -e "${DIM}Last 30 lines of server log:${NC}"
            tail -30 "$TEST_LOG" 2>/dev/null || true
            return 1
        fi
        sleep 1
    done
}

stop_server() {
    if [ -f "$TEST_PID_FILE" ]; then
        local pid
        pid=$(cat "$TEST_PID_FILE")
        if kill -0 "$pid" 2>/dev/null; then
            echo -e "${DIM}Stopping server (pid $pid)...${NC}"
            kill "$pid" 2>/dev/null || true
            # Wait briefly for graceful shutdown
            for _ in $(seq 1 5); do
                kill -0 "$pid" 2>/dev/null || break
                sleep 0.5
            done
            # Force kill if still running
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$TEST_PID_FILE"
    fi
}

cleanup() {
    stop_server
    if [ -n "${TEST_TMPDIR:-}" ] && [ -d "$TEST_TMPDIR" ]; then
        echo -e "${DIM}Test data: $TEST_TMPDIR${NC}"
        # Don't auto-remove — user may want to inspect logs on failure
    fi
}

# ── Test Assertions ───────────────────────────────────────────────────────────

# Run a curl request and capture status + body
# Usage: http_request METHOD URL [DATA]
# Sets: HTTP_STATUS, HTTP_BODY
http_request() {
    local method="$1"
    local url="$2"
    local data="${3:-}"

    local curl_args=(-s -w "\n%{http_code}" -X "$method")
    if [ -n "$data" ]; then
        curl_args+=(-H "Content-Type: application/json" -d "$data")
    fi

    local result
    result=$(curl "${curl_args[@]}" "$url" 2>/dev/null) || true
    HTTP_STATUS=$(echo "$result" | tail -1)
    HTTP_BODY=$(echo "$result" | sed '$d')
}

# Assert HTTP status code
# Usage: assert_status "test name" EXPECTED_CODE METHOD URL [DATA]
assert_status() {
    local name="$1"
    local expected="$2"
    local method="$3"
    local url="$4"
    local data="${5:-}"

    http_request "$method" "$url" "$data"

    if [ "$HTTP_STATUS" = "$expected" ]; then
        pass "$name"
    else
        fail "$name" "expected HTTP $expected, got $HTTP_STATUS (body: $(echo "$HTTP_BODY" | head -c 200))"
    fi
}

# Assert JSON field value in HTTP_BODY (returns 0/1, does not call pass/fail)
# Usage: assert_json_field "field_path" "expected_value"
assert_json_field() {
    local field="$1"
    local expected="$2"
    local actual

    actual=$(echo "$HTTP_BODY" | jq -r "$field" 2>/dev/null) || actual="<jq_error>"

    if [ "$actual" = "$expected" ]; then
        return 0
    else
        return 1
    fi
}

# Assert JSON field equals expected value, with pass/fail reporting
# Usage: assert_json_eq "test name" "jq_path" "expected_value"
assert_json_eq() {
    local name="$1"
    local jq_path="$2"
    local expected="$3"
    local actual

    actual=$(echo "$HTTP_BODY" | jq -r "$jq_path" 2>/dev/null) || actual="<jq_error>"

    if [ "$actual" = "$expected" ]; then
        pass "$name"
    else
        fail "$name" "expected $jq_path = '$expected', got '$actual'"
    fi
}

# Assert HTTP_BODY has JSON field at jq path with expected value (pass/fail)
# Usage: assert_body_json_field "test name" "jq_path" "expected_value"
assert_body_json_field() {
    local name="$1"
    local jq_path="$2"
    local expected="$3"
    local actual

    actual=$(echo "$HTTP_BODY" | jq -r "$jq_path" 2>/dev/null) || actual="<jq_error>"

    if [ "$actual" = "$expected" ]; then
        pass "$name"
    else
        fail "$name" "expected ${jq_path} = '${expected}', got '${actual}'"
    fi
}

# Assert numeric value >= minimum (pass/fail)
# Usage: assert_min "test name" ACTUAL MINIMUM
assert_min() {
    local name="$1"
    local actual="$2"
    local minimum="$3"

    if [ -z "$actual" ] || [ "$actual" = "null" ]; then
        fail "$name" "got empty/null value, expected >= $minimum"
        return
    fi

    # Use bc for floating point comparison if available, else awk
    local result
    if command -v bc &>/dev/null; then
        result=$(echo "$actual >= $minimum" | bc -l 2>/dev/null) || result=0
    else
        result=$(awk "BEGIN { print ($actual >= $minimum) ? 1 : 0 }" 2>/dev/null) || result=0
    fi

    if [ "$result" = "1" ]; then
        pass "$name (${actual} >= ${minimum})"
    else
        fail "$name" "expected >= $minimum, got $actual"
    fi
}

# Assert that HTTP_BODY contains a substring
assert_body_contains() {
    local substring="$1"
    if echo "$HTTP_BODY" | grep -q "$substring"; then
        return 0
    else
        return 1
    fi
}

# ── Test Output ───────────────────────────────────────────────────────────────

phase_header() {
    CURRENT_PHASE="$1"
    echo ""
    echo -e "${BOLD}================================================================${NC}"
    echo -e "${BOLD}  $1${NC}"
    echo -e "${BOLD}================================================================${NC}"
    echo ""
}

pass() {
    local name="$1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
    echo -e "  ${GREEN}pass${NC} $name"
}

fail() {
    local name="$1"
    local detail="${2:-}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo -e "  ${RED}FAIL${NC} $name"
    if [ -n "$detail" ]; then
        echo -e "    ${DIM}$detail${NC}"
    fi
}

skip() {
    local name="$1"
    local reason="${2:-}"
    TESTS_SKIPPED=$((TESTS_SKIPPED + 1))
    echo -e "  ${YELLOW}skip${NC} $name ${DIM}(skipped: $reason)${NC}"
}

test_summary() {
    local total=$((TESTS_PASSED + TESTS_FAILED + TESTS_SKIPPED))
    echo ""
    echo -e "${BOLD}----------------------------------------${NC}"
    echo -e "  ${GREEN}Passed:${NC}  $TESTS_PASSED"
    if [ "$TESTS_FAILED" -gt 0 ]; then
        echo -e "  ${RED}Failed:${NC}  $TESTS_FAILED"
    else
        echo -e "  Failed:  0"
    fi
    if [ "$TESTS_SKIPPED" -gt 0 ]; then
        echo -e "  ${YELLOW}Skipped:${NC} $TESTS_SKIPPED"
    fi
    echo -e "  Total:   $total"
    echo -e "${BOLD}----------------------------------------${NC}"

    if [ "$TESTS_FAILED" -gt 0 ]; then
        echo -e "  ${RED}SOME TESTS FAILED${NC}"
        echo -e "  ${DIM}Server log: $TEST_LOG${NC}"
        return 1
    else
        echo -e "  ${GREEN}ALL TESTS PASSED${NC}"
        return 0
    fi
}

# ── Helpers ───────────────────────────────────────────────────────────────────

# Wait for data to flush (segments are buffered)
wait_flush() {
    local secs="${1:-8}"
    echo -e "  ${DIM}Waiting ${secs}s for segment flush...${NC}"
    sleep "$secs"
}

# Produce N records to a topic+partition and wait for flush
# Usage: produce_and_flush TOPIC PARTITION COUNT [FLUSH_SECS] [PREFIX]
produce_and_flush() {
    local topic="$1"
    local partition="$2"
    local count="$3"
    local flush_secs="${4:-8}"
    local prefix="${5:-record}"

    local payload
    payload=$(make_batch_json "$topic" "$partition" "$count" "$prefix")

    http_request POST "${TEST_HTTP}/api/v1/produce/batch" "$payload"

    if [ "$HTTP_STATUS" != "200" ] && [ "$HTTP_STATUS" != "201" ]; then
        echo -e "  ${RED}produce_and_flush: produce failed (HTTP $HTTP_STATUS)${NC}"
        return 1
    fi

    wait_flush "$flush_secs"
}

# Generate a JSON produce payload
# Usage: make_produce_json topic key value [partition]
make_produce_json() {
    local topic="$1"
    local key="$2"
    local value="$3"
    local partition="${4:-}"

    if [ -n "$partition" ]; then
        jq -n --arg t "$topic" --arg k "$key" --arg v "$value" --argjson p "$partition" \
            '{topic: $t, key: $k, value: $v, partition: $p}'
    else
        jq -n --arg t "$topic" --arg k "$key" --arg v "$value" \
            '{topic: $t, key: $k, value: $v}'
    fi
}

# Portable timeout: uses 'timeout' (Linux/brew coreutils) or 'gtimeout' or perl fallback
# Usage: run_with_timeout SECONDS command [args...]
run_with_timeout() {
    local secs="$1"; shift
    if command -v timeout &>/dev/null; then
        timeout "$secs" "$@"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$secs" "$@"
    else
        # Perl-based fallback for macOS without coreutils
        perl -e 'alarm shift; exec @ARGV' "$secs" "$@"
    fi
}

# Generate a batch produce payload
# Usage: make_batch_json topic partition count [prefix]
make_batch_json() {
    local topic="$1"
    local partition="$2"
    local count="$3"
    local prefix="${4:-record}"

    python3 -c "
import json, hashlib
records = []
for i in range($count):
    key = f'${prefix}-{i}'
    payload = {'seq': i, 'data': f'payload-{i}', 'prefix': '${prefix}'}
    payload['checksum'] = hashlib.md5(f'payload-{i}'.encode()).hexdigest()
    records.append({'key': key, 'value': json.dumps(payload)})
print(json.dumps({'topic': '$topic', 'partition': $partition, 'records': records}))
"
}

# Count records across all partitions for a topic
# Usage: count_all_records topic partition_count
count_all_records() {
    local topic="$1"
    local partitions="$2"
    local total=0

    for p in $(seq 0 $((partitions - 1))); do
        local result
        result=$(curl -s "${TEST_HTTP}/api/v1/consume?topic=${topic}&partition=${p}&offset=0&maxRecords=100000" 2>/dev/null)
        local count
        count=$(echo "$result" | jq '.records | length' 2>/dev/null) || count=0
        total=$((total + count))
    done
    echo "$total"
}

# Run a curl request with a custom header
# Usage: http_request_with_header METHOD URL HEADER_NAME HEADER_VALUE [DATA]
# Sets: HTTP_STATUS, HTTP_BODY
http_request_with_header() {
    local method="$1"
    local url="$2"
    local header_name="$3"
    local header_value="$4"
    local data="${5:-}"

    local curl_args=(-s -w "\n%{http_code}" -X "$method" -H "${header_name}: ${header_value}")
    if [ -n "$data" ]; then
        curl_args+=(-H "Content-Type: application/json" -d "$data")
    fi

    local result
    result=$(curl "${curl_args[@]}" "$url" 2>/dev/null) || true
    HTTP_STATUS=$(echo "$result" | tail -1)
    HTTP_BODY=$(echo "$result" | sed '$d')
}

# Consume all records from a specific partition
# Usage: consume_partition topic partition [max_records]
consume_partition() {
    local topic="$1"
    local partition="$2"
    local max="${3:-100000}"
    curl -s "${TEST_HTTP}/api/v1/consume?topic=${topic}&partition=${partition}&offset=0&maxRecords=${max}" 2>/dev/null
}
