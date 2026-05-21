#!/usr/bin/env bash
# Phase 08 — Benchmarks
# Orchestrates Criterion benchmarks, stress tests, and custom benchmarks

phase_header "Phase 08 — Benchmarks"

TIMESTAMP=$(date +%Y-%m-%dT%H:%M:%S)
RUN_DIR="${RESULTS_DIR}/${TIMESTAMP}"
mkdir -p "$RUN_DIR"

echo -e "  ${DIM}Results directory: $RUN_DIR${NC}"
echo -e "  ${DIM}Benchmark duration: ${BENCH_DURATION}s${NC}"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# 1. Criterion Micro-Benchmarks (no server needed)
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "${BOLD}  [1/4] Criterion Micro-Benchmarks${NC}"

# Storage segment benchmarks
echo -e "  ${DIM}Running segment benchmarks...${NC}"
if cargo bench -p streamhouse-storage --bench segment_bench \
    --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
    -- --output-format bencher 2>&1 | tee "$RUN_DIR/segment_bench.txt" | tail -5; then
    pass "Criterion: segment_bench"
else
    fail "Criterion: segment_bench" "see $RUN_DIR/segment_bench.txt"
fi

# Client producer benchmarks
echo -e "  ${DIM}Running producer benchmarks...${NC}"
if cargo bench -p streamhouse-client --bench producer_bench \
    --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
    -- --output-format bencher 2>&1 | tee "$RUN_DIR/producer_bench.txt" | tail -5; then
    pass "Criterion: producer_bench"
else
    fail "Criterion: producer_bench" "see $RUN_DIR/producer_bench.txt"
fi

# Client consumer benchmarks
echo -e "  ${DIM}Running consumer benchmarks...${NC}"
if cargo bench -p streamhouse-client --bench consumer_bench \
    --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
    -- --output-format bencher 2>&1 | tee "$RUN_DIR/consumer_bench.txt" | tail -5; then
    pass "Criterion: consumer_bench"
else
    fail "Criterion: consumer_bench" "see $RUN_DIR/consumer_bench.txt"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# 2. E2E Stress Test (requires running server)
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BOLD}  [2/4] E2E Stress Test (gRPC produce at scale)${NC}"

echo -e "  ${DIM}Running stress_test_e2e for ${BENCH_DURATION}s...${NC}"

# The stress test uses hardcoded addresses — set env vars if it supports them
# For now, run it against the test server ports
if GRPC_ADDR="http://localhost:${TEST_GRPC_PORT}" \
   HTTP_ADDR="http://localhost:${TEST_HTTP_PORT}" \
   cargo run -p streamhouse-client --example stress_test_e2e --release \
    --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
    2>&1 | tee "$RUN_DIR/stress_test_e2e.txt"; then
    pass "E2E stress test completed"
else
    # The stress test may not support custom ports — try default
    fail "E2E stress test" "see $RUN_DIR/stress_test_e2e.txt (may need server on default ports)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# 3. gRPC Benchmark (ghz)
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BOLD}  [3/4] gRPC Benchmark${NC}"

BENCH_SCRIPT="${PROJECT_ROOT}/tests/benchmarks/grpc-bench.sh"
if [ -f "$BENCH_SCRIPT" ]; then
    source "$BENCH_SCRIPT"
else
    skip "gRPC benchmark" "script not found"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# 4. REST Benchmark
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BOLD}  [4/4] REST API Benchmark${NC}"

REST_BENCH_SCRIPT="${PROJECT_ROOT}/tests/benchmarks/rest-bench.sh"
if [ -f "$REST_BENCH_SCRIPT" ]; then
    source "$REST_BENCH_SCRIPT"
else
    skip "REST benchmark" "script not found"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "  ${BOLD}Benchmark results saved to:${NC}"
echo -e "    $RUN_DIR/"
ls -la "$RUN_DIR/" 2>/dev/null | while read -r line; do
    echo -e "    ${DIM}$line${NC}"
done
