#!/usr/bin/env bash
# StreamHouse Comprehensive Test Suite — Master Runner
#
# Usage:
#   ./tests/run-all.sh                        # Run all functional tests (phases 01-10)
#   ./tests/run-all.sh --bench                # Also run benchmarks (phase 08)
#   ./tests/run-all.sh --bench-duration 60    # Benchmark for 60s (default: 30s)
#   ./tests/run-all.sh --phase 05             # Run single phase
#   ./tests/run-all.sh --no-server            # Use already-running server (set TEST_HTTP_PORT etc)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Source shared library
source "$SCRIPT_DIR/lib/common.sh"

# ── Parse Arguments ───────────────────────────────────────────────────────────
RUN_BENCH=false
BENCH_DURATION=30
SINGLE_PHASE=""
NO_SERVER=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --bench)
            RUN_BENCH=true
            shift
            ;;
        --bench-duration)
            BENCH_DURATION="$2"
            shift 2
            ;;
        --phase)
            SINGLE_PHASE="$2"
            shift 2
            ;;
        --no-server)
            NO_SERVER=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --bench               Also run benchmarks (phase 08)"
            echo "  --bench-duration N    Benchmark duration in seconds (default: 30)"
            echo "  --phase NN            Run only phase NN (e.g., 05)"
            echo "  --no-server           Skip server start/stop (use running server)"
            echo "  -h, --help            Show this help"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

export BENCH_DURATION

# ── Banner ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}================================================================${NC}"
echo -e "${BOLD}  StreamHouse Comprehensive Test Suite${NC}"
echo -e "${BOLD}================================================================${NC}"
echo ""
echo -e "  Project:    $PROJECT_ROOT"
echo -e "  Temp dir:   $TEST_TMPDIR"
echo -e "  Ports:      gRPC=$TEST_GRPC_PORT  HTTP=$TEST_HTTP_PORT  Kafka=$TEST_KAFKA_PORT"
if [ "$RUN_BENCH" = true ]; then
    echo -e "  Benchmarks: ${GREEN}enabled${NC} (${BENCH_DURATION}s)"
else
    echo -e "  Benchmarks: ${DIM}disabled (use --bench to enable)${NC}"
fi
echo ""

# ── Setup ─────────────────────────────────────────────────────────────────────
START_TIME=$(date +%s)

# Check dependencies
check_required_deps
check_optional_deps
echo ""

# Trap cleanup
trap cleanup EXIT

if [ "$NO_SERVER" = false ]; then
    # Build
    build_server
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

# ── Run Phases ────────────────────────────────────────────────────────────────

PHASE_DIR="$SCRIPT_DIR/phases"
PHASES_PASSED=0
PHASES_FAILED=0
PHASES_SKIPPED=0

run_phase() {
    local phase_file="$1"
    local phase_name
    phase_name=$(basename "$phase_file" .sh)

    # Reset per-phase counters (but keep global totals)
    local before_passed=$TESTS_PASSED
    local before_failed=$TESTS_FAILED

    if [ -f "$phase_file" ]; then
        source "$phase_file"
    else
        echo -e "${RED}Phase file not found: $phase_file${NC}"
        PHASES_FAILED=$((PHASES_FAILED + 1))
        return
    fi

    local phase_passed=$((TESTS_PASSED - before_passed))
    local phase_failed=$((TESTS_FAILED - before_failed))

    if [ "$phase_failed" -eq 0 ]; then
        PHASES_PASSED=$((PHASES_PASSED + 1))
        echo -e "\n  ${GREEN}Phase $phase_name: $phase_passed passed${NC}"
    else
        PHASES_FAILED=$((PHASES_FAILED + 1))
        echo -e "\n  ${RED}Phase $phase_name: $phase_failed failed, $phase_passed passed${NC}"
    fi
}

if [ -n "$SINGLE_PHASE" ]; then
    # Run single phase
    phase_file="$PHASE_DIR/${SINGLE_PHASE}*.sh"
    # Expand glob
    for f in $phase_file; do
        if [ -f "$f" ]; then
            run_phase "$f"
        else
            echo -e "${RED}No phase matching: $SINGLE_PHASE${NC}"
            exit 1
        fi
    done
else
    # Run all phases
    for phase_file in "$PHASE_DIR"/*.sh; do
        phase_num=$(basename "$phase_file" | cut -c1-2)

        # Skip benchmarks unless --bench flag
        if [ "$phase_num" = "08" ] && [ "$RUN_BENCH" = false ]; then
            phase_header "Phase 08 — Benchmarks (skipped, use --bench)"
            PHASES_SKIPPED=$((PHASES_SKIPPED + 1))
            continue
        fi

        run_phase "$phase_file"
    done
fi

# ── Summary ───────────────────────────────────────────────────────────────────
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo ""
echo -e "${BOLD}================================================================${NC}"
echo -e "${BOLD}  Test Suite Summary${NC}"
echo -e "${BOLD}================================================================${NC}"
echo ""
echo -e "  Duration:       ${DURATION}s"
echo -e "  Phases passed:  ${GREEN}$PHASES_PASSED${NC}"
if [ "$PHASES_FAILED" -gt 0 ]; then
    echo -e "  Phases failed:  ${RED}$PHASES_FAILED${NC}"
fi
if [ "$PHASES_SKIPPED" -gt 0 ]; then
    echo -e "  Phases skipped: ${YELLOW}$PHASES_SKIPPED${NC}"
fi
echo ""

# Overall test totals
test_summary

echo ""
echo -e "  ${DIM}Server log: $TEST_LOG${NC}"
echo -e "  ${DIM}Test data:  $TEST_TMPDIR${NC}"
echo ""
