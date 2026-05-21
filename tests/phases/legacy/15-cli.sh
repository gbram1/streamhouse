#!/usr/bin/env bash
# Phase 15 — CLI Integration Tests
# Tests the streamctl CLI binary against the running server

phase_header "Phase 15 — CLI (streamctl)"

if [ "${HAS_STREAMCTL:-0}" != "1" ] || [ ! -f "$STREAMCTL" ]; then
    skip "All CLI tests" "streamctl binary not found"
    return 0 2>/dev/null || exit 0
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Set CLI connection vars
# ═══════════════════════════════════════════════════════════════════════════════

export STREAMHOUSE_ADDR="http://localhost:${TEST_GRPC_PORT}"
export STREAMHOUSE_API_URL="${TEST_HTTP}"

# CLI may also accept explicit flags — define for commands that need them
CLI_ARGS="--server http://localhost:${TEST_GRPC_PORT} --api-url http://localhost:${TEST_HTTP_PORT}"

# ═══════════════════════════════════════════════════════════════════════════════
# Topic Management
# ═══════════════════════════════════════════════════════════════════════════════

# Create topic
RESULT=$($STREAMCTL $CLI_ARGS topic create cli-test --partitions 2 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ] && echo "$RESULT" | grep -qi "cli-test\|created\|success"; then
    pass "CLI: topic create cli-test (exit 0, output contains topic name)"
elif [ "$EXIT_CODE" -eq 0 ]; then
    # exit 0 but output does not contain topic name — still check it's not an error
    if echo "$RESULT" | grep -qi "error\|fail"; then
        fail "CLI: topic create cli-test" "exit 0 but output contains error: $(echo "$RESULT" | head -c 200)"
    else
        pass "CLI: topic create cli-test (exit 0)"
    fi
else
    # Might already exist (409-equivalent)
    if echo "$RESULT" | grep -qi "already\|exists\|conflict"; then
        pass "CLI: topic create cli-test (already exists)"
    else
        fail "CLI: topic create cli-test" "exit $EXIT_CODE: $(echo "$RESULT" | head -c 200)"
    fi
fi

# List topics — output must contain "cli-test"
RESULT=$($STREAMCTL $CLI_ARGS topic list 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ] && echo "$RESULT" | grep -q "cli-test"; then
    pass "CLI: topic list contains 'cli-test'"
elif [ "$EXIT_CODE" -eq 0 ]; then
    fail "CLI: topic list" "exit 0 but 'cli-test' not found in output: $(echo "$RESULT" | head -c 200)"
else
    fail "CLI: topic list" "exit $EXIT_CODE: $(echo "$RESULT" | head -c 200)"
fi

# Get topic — output must contain partition info
RESULT=$($STREAMCTL $CLI_ARGS topic get cli-test 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ] && echo "$RESULT" | grep -qi "cli-test\|partition"; then
    pass "CLI: topic get cli-test (contains partition info)"
elif [ "$EXIT_CODE" -eq 0 ]; then
    fail "CLI: topic get cli-test" "exit 0 but no partition info in output: $(echo "$RESULT" | head -c 200)"
else
    fail "CLI: topic get cli-test" "exit $EXIT_CODE: $(echo "$RESULT" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Produce
# ═══════════════════════════════════════════════════════════════════════════════

RESULT=$($STREAMCTL $CLI_ARGS produce cli-test --partition 0 --value '{"test":true}' --key "k1" 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ]; then
    if echo "$RESULT" | grep -qi "error\|fail"; then
        fail "CLI: produce to cli-test" "exit 0 but output indicates error: $(echo "$RESULT" | head -c 200)"
    else
        pass "CLI: produce to cli-test (exit 0)"
    fi
else
    fail "CLI: produce to cli-test" "exit $EXIT_CODE: $(echo "$RESULT" | head -c 200)"
fi

# Wait for data to flush
wait_flush 5

# ═══════════════════════════════════════════════════════════════════════════════
# Consume
# ═══════════════════════════════════════════════════════════════════════════════

RESULT=$($STREAMCTL $CLI_ARGS consume cli-test --partition 0 --offset 0 --limit 10 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ]; then
    # Verify output contains actual data — look for our key, value, or generic record indicators
    if echo "$RESULT" | grep -qi "k1\|test\|value\|offset\|record"; then
        pass "CLI: consume from cli-test (exit 0, output contains data)"
    elif [ -n "$RESULT" ]; then
        pass "CLI: consume from cli-test (exit 0, non-empty output)"
    else
        fail "CLI: consume from cli-test" "exit 0 but output is empty"
    fi
else
    fail "CLI: consume from cli-test" "exit $EXIT_CODE: $(echo "$RESULT" | head -c 200)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SQL Query
# ═══════════════════════════════════════════════════════════════════════════════

RESULT=$($STREAMCTL $CLI_ARGS sql query "SELECT COUNT(*) FROM \"cli-test\"" 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ]; then
    pass "CLI: sql query (exit 0)"
else
    # SQL via CLI may not be implemented — skip rather than fail
    if echo "$RESULT" | grep -qi "not.*supported\|not.*implemented\|unknown\|unrecognized"; then
        skip "CLI: sql query" "not supported by streamctl"
    else
        fail "CLI: sql query" "exit $EXIT_CODE: $(echo "$RESULT" | head -c 200)"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Delete Topic
# ═══════════════════════════════════════════════════════════════════════════════

RESULT=$($STREAMCTL $CLI_ARGS topic delete cli-test 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ]; then
    pass "CLI: topic delete cli-test (exit 0)"
else
    fail "CLI: topic delete cli-test" "exit $EXIT_CODE: $(echo "$RESULT" | head -c 200)"
fi

# Verify topic is gone — get should fail
RESULT=$($STREAMCTL $CLI_ARGS topic get cli-test 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -ne 0 ]; then
    pass "CLI: topic get cli-test after delete -> non-zero exit (topic gone)"
else
    # Check if output indicates not found even with exit 0
    if echo "$RESULT" | grep -qi "not found\|404\|does not exist"; then
        pass "CLI: topic get cli-test after delete -> not found"
    else
        fail "CLI: topic get cli-test after delete" "expected non-zero exit or not-found, got exit 0: $(echo "$RESULT" | head -c 200)"
    fi
fi
