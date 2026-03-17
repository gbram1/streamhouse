#!/usr/bin/env bash
# StreamHouse Comprehensive Test Suite — Master Runner
#
# Usage:
#   ./tests/run-all.sh                              # Run all tests (local backend)
#   ./tests/run-all.sh --backend postgres-s3        # Run with Postgres + MinIO
#   ./tests/run-all.sh --no-server                  # Use already-running server
#   ./tests/run-all.sh --backend postgres-s3 --ci   # CI mode (no color)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Parse Arguments (before sourcing common.sh so env vars are set) ──────────
NO_SERVER=false
CI_MODE=false
REQUESTED_BACKEND=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --backend)
            REQUESTED_BACKEND="$2"
            shift 2
            ;;
        --no-server)
            NO_SERVER=true
            shift
            ;;
        --ci)
            CI_MODE=true
            export NO_COLOR=1
            export CI=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --backend BACKEND     Storage backend: 'local' (default) or 'postgres-s3'"
            echo "  --no-server           Skip server start/stop (use running server)"
            echo "  --ci                  CI mode: no color"
            echo "  -h, --help            Show this help"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

# Apply backend from flag (overrides env var)
if [ -n "$REQUESTED_BACKEND" ]; then
    export TEST_BACKEND="$REQUESTED_BACKEND"
fi

# Source shared library (after env vars are set)
source "$SCRIPT_DIR/lib/common.sh"

# ── Docker Compose for test infrastructure ───────────────────────────────────
TEST_COMPOSE_FILE="$SCRIPT_DIR/docker-compose.test.yml"
COMPOSE_STARTED=false

start_test_infra() {
    if [ "$TEST_BACKEND" != "postgres-s3" ]; then
        return 0
    fi

    # Skip docker compose if services are already reachable (e.g., CI service containers)
    local pg_host pg_port
    pg_host=$(echo "$TEST_DATABASE_URL" | sed -E 's|.*@([^:/]+).*|\1|')
    pg_port=$(echo "$TEST_DATABASE_URL" | sed -E 's|.*:([0-9]+)/.*|\1|')

    local pg_up=false minio_up=false

    if command -v pg_isready &>/dev/null; then
        pg_isready -h "$pg_host" -p "$pg_port" -U streamhouse -q 2>/dev/null && pg_up=true
    elif (echo > /dev/tcp/"$pg_host"/"$pg_port") 2>/dev/null; then
        pg_up=true
    fi

    curl -sf "${TEST_S3_ENDPOINT}/minio/health/live" > /dev/null 2>&1 && minio_up=true

    if [ "$pg_up" = true ] && [ "$minio_up" = true ]; then
        echo -e "${GREEN}Test infrastructure already running (skipping docker compose)${NC}"
        setup_minio_bucket
        return 0
    fi

    if [ ! -f "$TEST_COMPOSE_FILE" ]; then
        echo -e "${RED}Missing $TEST_COMPOSE_FILE — cannot start test infrastructure${NC}"
        exit 1
    fi

    echo -e "${BOLD}Starting test infrastructure (Postgres + MinIO)...${NC}"
    export TEST_PG_PORT
    export TEST_MINIO_PORT
    docker compose -f "$TEST_COMPOSE_FILE" up -d --wait 2>&1 || {
        echo -e "${RED}Failed to start test infrastructure${NC}"
        docker compose -f "$TEST_COMPOSE_FILE" logs 2>&1 || true
        exit 1
    }
    COMPOSE_STARTED=true

    wait_for_postgres 30
    wait_for_minio 30
    setup_minio_bucket
}

stop_test_infra() {
    if [ "$COMPOSE_STARTED" = true ] && [ -f "$TEST_COMPOSE_FILE" ]; then
        echo -e "${DIM}Stopping test infrastructure...${NC}"
        docker compose -f "$TEST_COMPOSE_FILE" down -v 2>&1 || true
    fi
}

# ── Banner ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}================================================================${NC}"
echo -e "${BOLD}  StreamHouse Comprehensive Test Suite${NC}"
echo -e "${BOLD}================================================================${NC}"
echo ""
echo -e "  Project:    $PROJECT_ROOT"
echo -e "  Temp dir:   $TEST_TMPDIR"
echo -e "  Backend:    ${BOLD}${TEST_BACKEND}${NC}"
echo -e "  Ports:      gRPC=$TEST_GRPC_PORT  HTTP=$TEST_HTTP_PORT  Kafka=$TEST_KAFKA_PORT"
if [ "$TEST_BACKEND" = "postgres-s3" ]; then
    echo -e "  Postgres:   ${TEST_DATABASE_URL}"
    echo -e "  MinIO:      ${TEST_S3_ENDPOINT} (bucket: ${TEST_S3_BUCKET})"
fi
if [ "$CI_MODE" = true ]; then
    echo -e "  CI mode:    enabled"
fi
echo ""

# ── Setup ─────────────────────────────────────────────────────────────────────
START_TIME=$(date +%s)

# Check dependencies
check_required_deps
check_optional_deps
echo ""

# Trap cleanup (stop server + infra)
full_cleanup() {
    stop_server
    stop_test_infra
    if [ -n "${TEST_TMPDIR:-}" ] && [ -d "$TEST_TMPDIR" ]; then
        echo -e "${DIM}Test data: $TEST_TMPDIR${NC}"
    fi
}
trap full_cleanup EXIT

# Start test infrastructure if needed
start_test_infra

if [ "$NO_SERVER" = false ]; then
    # Build
    if [ "$TEST_BACKEND" = "postgres-s3" ]; then
        build_server_postgres
    else
        build_server
    fi
    build_cli
    echo ""

    # Start server
    start_server
    if ! wait_healthy 30; then
        echo -e "${RED}Aborting: server did not become healthy${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}Using existing server (--no-server)${NC}"
    if ! curl -sf "${TEST_HTTP}/health" > /dev/null 2>&1; then
        echo -e "${RED}No server running at ${TEST_HTTP}/health${NC}"
        exit 1
    fi
    echo -e "${GREEN}Server is healthy${NC}"

    # Check if CLI exists
    if [ -f "$STREAMCTL" ]; then
        export HAS_STREAMCTL=1
    else
        export HAS_STREAMCTL=0
    fi
fi

# ── Run E2E Tests ────────────────────────────────────────────────────────────

E2E_TESTS="$SCRIPT_DIR/e2e-tests.sh"

if [ ! -f "$E2E_TESTS" ]; then
    echo -e "${RED}E2E test file not found: $E2E_TESTS${NC}"
    exit 1
fi

source "$E2E_TESTS"

# ── Summary ───────────────────────────────────────────────────────────────────
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo ""
echo -e "${BOLD}================================================================${NC}"
echo -e "${BOLD}  Test Suite Summary${NC}"
echo -e "${BOLD}================================================================${NC}"
echo ""
echo -e "  Backend:        ${BOLD}${TEST_BACKEND}${NC}"
echo -e "  Duration:       ${DURATION}s"
echo ""

test_summary

echo ""
echo -e "  ${DIM}Server log: $TEST_LOG${NC}"
echo -e "  ${DIM}Test data:  $TEST_TMPDIR${NC}"
echo ""
