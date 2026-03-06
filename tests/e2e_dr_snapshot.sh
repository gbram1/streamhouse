#!/usr/bin/env bash
# e2e_dr_snapshot.sh — End-to-end test for DR Snapshot Ordering
#
# Tests:
#   1. Automatic metadata snapshots after S3 flushes
#   2. reconcile-from-s3 re-registers segments missing from metadata
#   3. Full DR flow: lose metadata, restore via reconcile, verify data
#
# Usage:
#   ./tests/e2e_dr_snapshot.sh
#
# Requires: curl, jq, gunzip, sqlite3

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ── Configuration ────────────────────────────────────────────────────────────
SNAPSHOT_INTERVAL=10  # seconds — short for testing
FLUSH_INTERVAL=3      # seconds
NUM_RECORDS=20
TOPIC="dr-test"

# ── Dependency Check ─────────────────────────────────────────────────────────
check_required_deps

for cmd in gunzip sqlite3; do
    if ! command -v "$cmd" &>/dev/null; then
        echo -e "${RED}Missing required dependency: $cmd${NC}"
        exit 1
    fi
done

# ── Build ────────────────────────────────────────────────────────────────────
build_server

# ── Helpers ──────────────────────────────────────────────────────────────────

start_server_with_snapshots() {
    mkdir -p "${TEST_DATA_DIR}/storage" "${TEST_DATA_DIR}/cache"

    USE_LOCAL_STORAGE=1 \
    LOCAL_STORAGE_PATH="${TEST_DATA_DIR}/storage" \
    STREAMHOUSE_METADATA="${TEST_DATA_DIR}/metadata.db" \
    STREAMHOUSE_CACHE="${TEST_DATA_DIR}/cache" \
    STREAMHOUSE_CACHE_SIZE=104857600 \
    GRPC_ADDR="0.0.0.0:${TEST_GRPC_PORT}" \
    HTTP_ADDR="0.0.0.0:${TEST_HTTP_PORT}" \
    KAFKA_ADDR="0.0.0.0:${TEST_KAFKA_PORT}" \
    FLUSH_INTERVAL_SECS="${FLUSH_INTERVAL}" \
    SNAPSHOT_INTERVAL_SECS="${SNAPSHOT_INTERVAL}" \
    RUST_LOG=info \
    "$BINARY" > "$TEST_LOG" 2>&1 &

    echo $! > "$TEST_PID_FILE"
    echo -e "${DIM}  PID: $(cat "$TEST_PID_FILE")  Log: $TEST_LOG${NC}"
}

# Start server without snapshot loop (normal mode after recovery)
start_server_plain() {
    mkdir -p "${TEST_DATA_DIR}/storage" "${TEST_DATA_DIR}/cache"

    USE_LOCAL_STORAGE=1 \
    LOCAL_STORAGE_PATH="${TEST_DATA_DIR}/storage" \
    STREAMHOUSE_METADATA="${TEST_DATA_DIR}/metadata.db" \
    STREAMHOUSE_CACHE="${TEST_DATA_DIR}/cache" \
    STREAMHOUSE_CACHE_SIZE=104857600 \
    GRPC_ADDR="0.0.0.0:${TEST_GRPC_PORT}" \
    HTTP_ADDR="0.0.0.0:${TEST_HTTP_PORT}" \
    KAFKA_ADDR="0.0.0.0:${TEST_KAFKA_PORT}" \
    FLUSH_INTERVAL_SECS="${FLUSH_INTERVAL}" \
    SNAPSHOT_INTERVAL_SECS=0 \
    RUST_LOG=info \
    "$BINARY" > "$TEST_LOG" 2>&1 &

    echo $! > "$TEST_PID_FILE"
}

# Run reconcile-from-s3 (blocks until done, then server exits)
run_reconcile_from_s3() {
    local log="${TEST_TMPDIR}/reconcile.log"

    USE_LOCAL_STORAGE=1 \
    LOCAL_STORAGE_PATH="${TEST_DATA_DIR}/storage" \
    STREAMHOUSE_METADATA="${TEST_DATA_DIR}/metadata.db" \
    STREAMHOUSE_CACHE="${TEST_DATA_DIR}/cache" \
    GRPC_ADDR="0.0.0.0:${TEST_GRPC_PORT}" \
    HTTP_ADDR="0.0.0.0:${TEST_HTTP_PORT}" \
    KAFKA_ADDR="0.0.0.0:${TEST_KAFKA_PORT}" \
    RECONCILE_FROM_S3=1 \
    RUST_LOG=info \
    "$BINARY" > "$log" 2>&1 || true

    cat "$log"
    RECONCILE_LOG="$log"
}

trap cleanup EXIT

# ═════════════════════════════════════════════════════════════════════════════
# PHASE 1: Automatic Snapshot Creation
# ═════════════════════════════════════════════════════════════════════════════
phase_header "Phase 1 — Automatic Snapshot Creation"

echo -e "Starting server with ${SNAPSHOT_INTERVAL}s snapshot interval..."
start_server_with_snapshots
wait_healthy 30

# Create topic
http_request POST "${TEST_HTTP}/api/v1/topics" \
    "{\"name\":\"${TOPIC}\",\"partitions\":1}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Create topic '${TOPIC}'"
else
    fail "Create topic '${TOPIC}'" "got HTTP $HTTP_STATUS"
fi

# Produce records
echo -e "  ${DIM}Producing ${NUM_RECORDS} records...${NC}"
for i in $(seq 1 "$NUM_RECORDS"); do
    http_request POST "${TEST_HTTP}/api/v1/produce" \
        "{\"topic\":\"${TOPIC}\",\"key\":\"k${i}\",\"value\":\"DR-test-record-${i}\"}"
done
pass "Produced ${NUM_RECORDS} records"

# Wait for flush + snapshot
WAIT_SECS=$((SNAPSHOT_INTERVAL + FLUSH_INTERVAL + 5))
echo -e "  ${DIM}Waiting ${WAIT_SECS}s for flush + snapshot...${NC}"
sleep "$WAIT_SECS"

# Verify snapshot file exists
SNAPSHOT_DIR="${TEST_DATA_DIR}/storage/_snapshots"
if [ -d "$SNAPSHOT_DIR" ]; then
    SNAPSHOT_COUNT=$(find "$SNAPSHOT_DIR" -name "*.json.gz" | wc -l | tr -d ' ')
    if [ "$SNAPSHOT_COUNT" -gt 0 ]; then
        pass "Snapshot file created (found ${SNAPSHOT_COUNT} snapshot(s))"
        SNAPSHOT_FILE=$(find "$SNAPSHOT_DIR" -name "*.json.gz" | sort | tail -1)
        echo -e "    ${DIM}Latest: $(basename "$SNAPSHOT_FILE")${NC}"
    else
        fail "Snapshot file created" "no .json.gz files in ${SNAPSHOT_DIR}"
    fi
else
    fail "Snapshot directory exists" "${SNAPSHOT_DIR} not found"
    SNAPSHOT_COUNT=0
fi

# Verify snapshot contents
if [ "${SNAPSHOT_COUNT:-0}" -gt 0 ]; then
    SNAPSHOT_JSON=$(gunzip -c "$SNAPSHOT_FILE")
    SNAP_TOPICS=$(echo "$SNAPSHOT_JSON" | jq '.topics | length')
    SNAP_VERSION=$(echo "$SNAPSHOT_JSON" | jq '.version')
    SNAP_SEGMENTS=$(echo "$SNAPSHOT_JSON" | jq '[.topics[].segments | length] | add // 0')

    if [ "$SNAP_VERSION" = "1" ]; then
        pass "Snapshot version is 1"
    else
        fail "Snapshot version" "got $SNAP_VERSION"
    fi

    if [ "$SNAP_TOPICS" -ge 1 ]; then
        pass "Snapshot contains ${SNAP_TOPICS} topic(s)"
    else
        fail "Snapshot contains topics" "got $SNAP_TOPICS"
    fi

    if [ "$SNAP_SEGMENTS" -ge 1 ]; then
        pass "Snapshot contains ${SNAP_SEGMENTS} segment(s)"
    else
        fail "Snapshot contains segments" "got $SNAP_SEGMENTS"
    fi
fi

# Verify records are consumable before DR
CONSUME_RESULT=$(curl -s "${TEST_HTTP}/api/v1/consume?topic=${TOPIC}&partition=0&offset=0&maxRecords=100" 2>/dev/null)
PRE_DR_COUNT=$(echo "$CONSUME_RESULT" | jq '.records | length' 2>/dev/null) || PRE_DR_COUNT=0
if [ "$PRE_DR_COUNT" -ge "$NUM_RECORDS" ]; then
    pass "All ${PRE_DR_COUNT} records consumable before DR"
else
    fail "Records consumable before DR" "got $PRE_DR_COUNT, expected >= $NUM_RECORDS"
fi

# Save segment count from storage for later comparison
SEG_FILES_IN_S3=$(find "${TEST_DATA_DIR}/storage/data" -name "*.seg" 2>/dev/null | wc -l | tr -d ' ')
echo -e "  ${DIM}Segment files in storage: ${SEG_FILES_IN_S3}${NC}"

stop_server
sleep 2

# ═════════════════════════════════════════════════════════════════════════════
# PHASE 2: Reconcile-from-S3 (partial metadata loss)
# ═════════════════════════════════════════════════════════════════════════════
phase_header "Phase 2 — Reconcile-from-S3 (Partial Metadata Loss)"

# Delete some segment rows from metadata to simulate partial loss
# Keep topics and partitions intact, just remove segment entries
BEFORE_SEGMENTS=$(sqlite3 "${TEST_DATA_DIR}/metadata.db" "SELECT count(*) FROM segments;")
echo -e "  ${DIM}Segments in metadata before: ${BEFORE_SEGMENTS}${NC}"

sqlite3 "${TEST_DATA_DIR}/metadata.db" "DELETE FROM segments;"
AFTER_SEGMENTS=$(sqlite3 "${TEST_DATA_DIR}/metadata.db" "SELECT count(*) FROM segments;")
echo -e "  ${DIM}Segments in metadata after delete: ${AFTER_SEGMENTS}${NC}"

if [ "$AFTER_SEGMENTS" = "0" ]; then
    pass "Cleared all segment metadata (${BEFORE_SEGMENTS} -> 0)"
else
    fail "Clear segment metadata" "still has $AFTER_SEGMENTS"
fi

# Run reconcile-from-s3
echo -e "  ${DIM}Running reconcile-from-s3...${NC}"
run_reconcile_from_s3

# Check reconcile results from log
if grep -q "Registered segment from S3" "$RECONCILE_LOG" 2>/dev/null; then
    REGISTERED=$(grep -c "Registered segment from S3" "$RECONCILE_LOG")
    pass "Reconcile registered ${REGISTERED} segment(s) from S3"
else
    fail "Reconcile registered segments" "no 'Registered segment from S3' in log"
fi

if grep -q "Reconcile-from-S3 complete" "$RECONCILE_LOG" 2>/dev/null; then
    pass "Reconcile completed successfully"
else
    fail "Reconcile completed" "no completion message in log"
fi

# Verify segment metadata was restored
RESTORED_SEGMENTS=$(sqlite3 "${TEST_DATA_DIR}/metadata.db" "SELECT count(*) FROM segments;")
echo -e "  ${DIM}Segments in metadata after reconcile: ${RESTORED_SEGMENTS}${NC}"

if [ "$RESTORED_SEGMENTS" -ge "$BEFORE_SEGMENTS" ]; then
    pass "Segment metadata restored (${RESTORED_SEGMENTS} >= ${BEFORE_SEGMENTS})"
else
    fail "Segment metadata restored" "got $RESTORED_SEGMENTS, expected >= $BEFORE_SEGMENTS"
fi

# Clear stale agent registrations left by reconcile mode
sqlite3 "${TEST_DATA_DIR}/metadata.db" "DELETE FROM agents;" 2>/dev/null || true

# Start server and verify data is consumable
echo -e "  ${DIM}Starting server to verify data...${NC}"
start_server_plain
wait_healthy 30

CONSUME_RESULT=$(curl -s "${TEST_HTTP}/api/v1/consume?topic=${TOPIC}&partition=0&offset=0&maxRecords=100" 2>/dev/null)
POST_RECONCILE_COUNT=$(echo "$CONSUME_RESULT" | jq '.records | length' 2>/dev/null) || POST_RECONCILE_COUNT=0

if [ "$POST_RECONCILE_COUNT" -ge "$NUM_RECORDS" ]; then
    pass "All ${POST_RECONCILE_COUNT} records consumable after reconcile"
else
    fail "Records consumable after reconcile" "got $POST_RECONCILE_COUNT, expected >= $NUM_RECORDS"
fi

stop_server
sleep 2

# ═════════════════════════════════════════════════════════════════════════════
# PHASE 3: Full DR Simulation (total metadata loss)
# ═════════════════════════════════════════════════════════════════════════════
phase_header "Phase 3 — Full DR (Total Metadata Loss + Recovery)"

# Completely delete metadata DB
rm -f "${TEST_DATA_DIR}/metadata.db"

if [ ! -f "${TEST_DATA_DIR}/metadata.db" ]; then
    pass "Metadata DB deleted (simulating total loss)"
else
    fail "Delete metadata DB" "file still exists"
fi

echo -e "  ${DIM}Segment files still in storage: ${SEG_FILES_IN_S3}${NC}"

# Step 1: Start a fresh server briefly to recreate the DB schema + topic
echo -e "  ${DIM}Step 1: Starting fresh server to recreate topic...${NC}"
# Fresh DB has no stale agents
start_server_plain
wait_healthy 30

http_request POST "${TEST_HTTP}/api/v1/topics" \
    "{\"name\":\"${TOPIC}\",\"partitions\":1}"
if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
    pass "Topic '${TOPIC}' available on fresh DB (HTTP $HTTP_STATUS — auto-recovery may have restored it)"
else
    fail "Re-create topic" "got HTTP $HTTP_STATUS"
fi

stop_server
sleep 2

# Step 2: Run reconcile-from-s3 to fill any remaining gaps
# (auto-recovery from snapshot may have already restored everything)
sqlite3 "${TEST_DATA_DIR}/metadata.db" "DELETE FROM agents;" 2>/dev/null || true
echo -e "  ${DIM}Step 2: Running reconcile-from-s3 on fresh DB...${NC}"
run_reconcile_from_s3

if grep -q "Reconcile-from-S3 complete" "$RECONCILE_LOG" 2>/dev/null; then
    REGISTERED=$(grep -c "Registered segment from S3" "$RECONCILE_LOG" 2>/dev/null) || REGISTERED=0
    pass "Full DR: reconcile complete (${REGISTERED} new segment(s) registered)"
else
    fail "Full DR: reconcile completed" "no completion message in log"
fi

# Step 3: Start server and verify data is back
# Clear stale agents from reconcile mode
sqlite3 "${TEST_DATA_DIR}/metadata.db" "DELETE FROM agents;" 2>/dev/null || true
echo -e "  ${DIM}Step 3: Starting server to verify recovered data...${NC}"
start_server_plain
wait_healthy 30

CONSUME_RESULT=$(curl -s "${TEST_HTTP}/api/v1/consume?topic=${TOPIC}&partition=0&offset=0&maxRecords=100" 2>/dev/null)
DR_COUNT=$(echo "$CONSUME_RESULT" | jq '.records | length' 2>/dev/null) || DR_COUNT=0

if [ "$DR_COUNT" -ge "$NUM_RECORDS" ]; then
    pass "Full DR: all ${DR_COUNT} records recovered"
else
    fail "Full DR: records recovered" "got $DR_COUNT, expected >= $NUM_RECORDS"
fi

# Verify a specific record value to ensure data integrity
FIRST_VALUE=$(echo "$CONSUME_RESULT" | jq -r '.records[0].value' 2>/dev/null) || FIRST_VALUE=""
if echo "$FIRST_VALUE" | grep -q "DR-test-record"; then
    pass "Full DR: record content intact"
else
    fail "Full DR: record content" "first value: $FIRST_VALUE"
fi

stop_server

# ═════════════════════════════════════════════════════════════════════════════
# PHASE 4: Idempotency (re-running reconcile is safe)
# ═════════════════════════════════════════════════════════════════════════════
phase_header "Phase 4 — Reconcile Idempotency"

SEGMENTS_BEFORE=$(sqlite3 "${TEST_DATA_DIR}/metadata.db" "SELECT count(*) FROM segments;")

sqlite3 "${TEST_DATA_DIR}/metadata.db" "DELETE FROM agents;" 2>/dev/null || true
echo -e "  ${DIM}Running reconcile again (should be a no-op)...${NC}"
run_reconcile_from_s3

SEGMENTS_AFTER=$(sqlite3 "${TEST_DATA_DIR}/metadata.db" "SELECT count(*) FROM segments;")

if [ "$SEGMENTS_AFTER" = "$SEGMENTS_BEFORE" ]; then
    pass "Re-running reconcile is idempotent (${SEGMENTS_BEFORE} segments unchanged)"
else
    fail "Reconcile idempotency" "segments changed: $SEGMENTS_BEFORE -> $SEGMENTS_AFTER"
fi

# ═════════════════════════════════════════════════════════════════════════════
# PHASE 5: Automatic Recovery (no RECONCILE_FROM_S3, server self-heals)
# ═════════════════════════════════════════════════════════════════════════════
phase_header "Phase 5 — Automatic Recovery (Server Self-Heals)"

# Save segment count before deleting metadata
SEGMENTS_BEFORE_AUTO=$(sqlite3 "${TEST_DATA_DIR}/metadata.db" "SELECT count(*) FROM segments;")
echo -e "  ${DIM}Segments before: ${SEGMENTS_BEFORE_AUTO}${NC}"

stop_server 2>/dev/null || true
sleep 2

# Completely delete metadata DB — simulating total loss
rm -f "${TEST_DATA_DIR}/metadata.db"
rm -f "${TEST_DATA_DIR}/metadata.db-shm" "${TEST_DATA_DIR}/metadata.db-wal"

if [ ! -f "${TEST_DATA_DIR}/metadata.db" ]; then
    pass "Metadata DB deleted for auto-recovery test"
else
    fail "Delete metadata DB" "file still exists"
fi

echo -e "  ${DIM}Snapshots still in storage:${NC}"
find "${TEST_DATA_DIR}/storage/_snapshots" -name "*.json.gz" 2>/dev/null | while read -r f; do
    echo -e "    ${DIM}$(basename "$f")${NC}"
done

# Start server normally (no RECONCILE_FROM_S3) — it should auto-recover
echo -e "  ${DIM}Starting server (should self-heal from snapshot)...${NC}"
start_server_plain
wait_healthy 30

# Verify the server auto-recovered topics
CONSUME_RESULT=$(curl -s "${TEST_HTTP}/api/v1/consume?topic=${TOPIC}&partition=0&offset=0&maxRecords=100" 2>/dev/null)
AUTO_COUNT=$(echo "$CONSUME_RESULT" | jq '.records | length' 2>/dev/null) || AUTO_COUNT=0

if [ "$AUTO_COUNT" -ge "$NUM_RECORDS" ]; then
    pass "Auto-recovery: all ${AUTO_COUNT} records recovered without manual intervention"
else
    fail "Auto-recovery: records recovered" "got $AUTO_COUNT, expected >= $NUM_RECORDS"
fi

# Verify record content
AUTO_VALUE=$(echo "$CONSUME_RESULT" | jq -r '.records[0].value' 2>/dev/null) || AUTO_VALUE=""
if echo "$AUTO_VALUE" | grep -q "DR-test-record"; then
    pass "Auto-recovery: record content intact"
else
    fail "Auto-recovery: record content" "first value: $AUTO_VALUE"
fi

# Verify segments were restored
AUTO_SEGMENTS=$(sqlite3 "${TEST_DATA_DIR}/metadata.db" "SELECT count(*) FROM segments;" 2>/dev/null) || AUTO_SEGMENTS=0
if [ "$AUTO_SEGMENTS" -ge 1 ]; then
    pass "Auto-recovery: ${AUTO_SEGMENTS} segment(s) in metadata"
else
    fail "Auto-recovery: segments in metadata" "got $AUTO_SEGMENTS"
fi

stop_server

# ═════════════════════════════════════════════════════════════════════════════
# Summary
# ═════════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${DIM}Test data: $TEST_TMPDIR${NC}"
echo -e "${DIM}Server log: $TEST_LOG${NC}"

test_summary
