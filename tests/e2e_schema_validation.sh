#!/usr/bin/env bash
set -euo pipefail

# E2E test for schema validation on produce
# Requires: docker compose services running, binaries built

STREAMCTL="./target/release/streamctl"
API_URL="http://localhost:8080"
PASS=0
FAIL=0
ERRORS=()

green() { printf "\033[32m%s\033[0m\n" "$1"; }
red()   { printf "\033[31m%s\033[0m\n" "$1"; }
bold()  { printf "\033[1m%s\033[0m\n" "$1"; }

expect_success() {
    local desc="$1"; shift
    if output=$("$@" 2>&1); then
        green "  PASS: $desc"
        PASS=$((PASS + 1))
    else
        red "  FAIL: $desc"
        echo "        $output"
        ERRORS+=("$desc")
        FAIL=$((FAIL + 1))
    fi
}

expect_failure() {
    local desc="$1"; shift
    if output=$("$@" 2>&1); then
        red "  FAIL (expected error): $desc"
        echo "        Got success: $output"
        ERRORS+=("$desc")
        FAIL=$((FAIL + 1))
    else
        green "  PASS (rejected): $desc"
        PASS=$((PASS + 1))
    fi
}

# -------------------------------------------------------------------
bold "=== Setup ==="
# -------------------------------------------------------------------

echo "Creating test topics..."
$STREAMCTL topic create schema-json-e2e --partitions 1 2>/dev/null || true
$STREAMCTL topic create schema-avro-e2e --partitions 1 2>/dev/null || true
$STREAMCTL topic create no-schema-e2e   --partitions 1 2>/dev/null || true

echo "Registering JSON schema (schema-json-e2e-value)..."
curl -sf -X POST "$API_URL/schemas/subjects/schema-json-e2e-value/versions" \
  -H "Content-Type: application/json" \
  -d '{"schemaType":"JSON","schema":"{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\"},\"age\":{\"type\":\"integer\"}},\"required\":[\"name\",\"age\"]}"}' \
  > /dev/null

echo "Registering Avro schema (schema-avro-e2e-value)..."
curl -sf -X POST "$API_URL/schemas/subjects/schema-avro-e2e-value/versions" \
  -H "Content-Type: application/json" \
  -d '{"schemaType":"AVRO","schema":"{\"type\":\"record\",\"name\":\"User\",\"fields\":[{\"name\":\"name\",\"type\":\"string\"},{\"name\":\"age\",\"type\":\"int\"}]}"}' \
  > /dev/null

echo ""

# -------------------------------------------------------------------
bold "=== 1. Valid produces (should succeed) ==="
# -------------------------------------------------------------------

expect_success "JSON schema - valid record" \
    $STREAMCTL produce schema-json-e2e --partition 0 --value '{"name":"alice","age":30}'

expect_success "Avro schema - valid record" \
    $STREAMCTL produce schema-avro-e2e --partition 0 --value '{"name":"bob","age":25}'

echo ""

# -------------------------------------------------------------------
bold "=== 2. Invalid produces (should be rejected) ==="
# -------------------------------------------------------------------

expect_failure "JSON schema - missing required field 'age'" \
    $STREAMCTL produce schema-json-e2e --partition 0 --value '{"name":"charlie"}'

expect_failure "JSON schema - wrong type (age is string)" \
    $STREAMCTL produce schema-json-e2e --partition 0 --value '{"name":"dave","age":"nope"}'

expect_failure "Avro schema - wrong shape" \
    $STREAMCTL produce schema-avro-e2e --partition 0 --value '{"bad":"data"}'

echo ""

# -------------------------------------------------------------------
bold "=== 3. No-schema topic (should succeed, no validation) ==="
# -------------------------------------------------------------------

expect_success "No schema registered - any data accepted" \
    $STREAMCTL produce no-schema-e2e --partition 0 --value '{"anything":"goes"}'

echo ""

# -------------------------------------------------------------------
bold "=== 4. --skip-validation flag (bypasses CLI validation) ==="
# -------------------------------------------------------------------

# CLI validation is skipped, but server-side validation still applies.
# So this should still be rejected by the server.
expect_failure "skip-validation - server still rejects invalid data" \
    $STREAMCTL produce schema-json-e2e --partition 0 --value '{"bad":"data"}' --skip-validation

echo ""

# -------------------------------------------------------------------
bold "=== 5. REST API validation ==="
# -------------------------------------------------------------------

desc="REST produce - valid JSON schema record"
if output=$(curl -sf -X POST "$API_URL/api/v1/produce" \
    -H "Content-Type: application/json" \
    -d '{"topic":"schema-json-e2e","value":"{\"name\":\"eve\",\"age\":28}","partition":0}' 2>&1); then
    green "  PASS: $desc"
    PASS=$((PASS + 1))
else
    red "  FAIL: $desc"
    echo "        $output"
    ERRORS+=("$desc")
    FAIL=$((FAIL + 1))
fi

desc="REST produce - invalid JSON schema record"
status=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$API_URL/api/v1/produce" \
    -H "Content-Type: application/json" \
    -d '{"topic":"schema-json-e2e","value":"{\"name\":\"frank\"}","partition":0}')
if [ "$status" -ge 400 ]; then
    green "  PASS (rejected $status): $desc"
    PASS=$((PASS + 1))
else
    red "  FAIL (expected 4xx, got $status): $desc"
    ERRORS+=("$desc")
    FAIL=$((FAIL + 1))
fi

desc="REST batch produce - one valid, one invalid"
status=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$API_URL/api/v1/produce/batch" \
    -H "Content-Type: application/json" \
    -d '{"topic":"schema-json-e2e","records":[{"value":"{\"name\":\"grace\",\"age\":35}","partition":0},{"value":"{\"name\":\"hank\"}","partition":0}]}')
if [ "$status" -ge 400 ]; then
    green "  PASS (rejected $status): $desc"
    PASS=$((PASS + 1))
else
    red "  FAIL (expected 4xx, got $status): $desc"
    ERRORS+=("$desc")
    FAIL=$((FAIL + 1))
fi

echo ""

# -------------------------------------------------------------------
bold "=== 6. Query results (only valid records should exist) ==="
# -------------------------------------------------------------------

echo "Records in schema-json-e2e:"
$STREAMCTL sql query "SELECT * FROM \"schema-json-e2e\" LIMIT 20" 2>&1 || true
echo ""
echo "Records in schema-avro-e2e:"
$STREAMCTL sql query "SELECT * FROM \"schema-avro-e2e\" LIMIT 20" 2>&1 || true
echo ""
echo "Records in no-schema-e2e:"
$STREAMCTL sql query "SELECT * FROM \"no-schema-e2e\" LIMIT 20" 2>&1 || true
echo ""

# -------------------------------------------------------------------
bold "=== Results ==="
# -------------------------------------------------------------------

echo ""
green "Passed: $PASS"
if [ "$FAIL" -gt 0 ]; then
    red "Failed: $FAIL"
    echo ""
    red "Failures:"
    for err in "${ERRORS[@]}"; do
        red "  - $err"
    done
    exit 1
else
    green "All tests passed!"
fi
