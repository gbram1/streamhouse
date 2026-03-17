#!/usr/bin/env bash
# Phase 07 — Concurrent Load Test
# Multi-topic, multi-partition concurrent writes simulating multi-org workload

phase_header "Phase 07 — Concurrent Load Test"

API="${TEST_HTTP}/api/v1"
NUM_ORGS=5
PARTITIONS_PER_ORG=8
BATCHES_PER_ORG=100
RECORDS_PER_BATCH=50
EXPECTED_PER_ORG=$((BATCHES_PER_ORG * RECORDS_PER_BATCH))  # 5000
EXPECTED_TOTAL=$((NUM_ORGS * EXPECTED_PER_ORG))              # 25000

echo -e "  ${DIM}Config: $NUM_ORGS orgs × $PARTITIONS_PER_ORG partitions × $BATCHES_PER_ORG batches × $RECORDS_PER_BATCH records${NC}"
echo -e "  ${DIM}Expected total: $EXPECTED_TOTAL records${NC}"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# Step 1: Create topics for each org
# ═══════════════════════════════════════════════════════════════════════════════

CREATE_ERRORS=0
for org in $(seq 1 $NUM_ORGS); do
    TOPIC="load-org-${org}"
    http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS_PER_ORG}"
    if [ "$HTTP_STATUS" != "201" ] && [ "$HTTP_STATUS" != "409" ]; then
        CREATE_ERRORS=$((CREATE_ERRORS + 1))
    fi
done

if [ "$CREATE_ERRORS" -eq 0 ]; then
    pass "Created $NUM_ORGS topics (${PARTITIONS_PER_ORG} partitions each)"
else
    fail "Topic creation" "$CREATE_ERRORS topics failed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 2: Concurrent produce (one background loop per org)
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}Starting concurrent producers...${NC}"
LOAD_START=$(date +%s)

PIDS=()
ERROR_FILES=()

for org in $(seq 1 $NUM_ORGS); do
    TOPIC="load-org-${org}"
    ERROR_FILE="${TEST_TMPDIR}/load-errors-${org}.txt"
    echo "0" > "$ERROR_FILE"
    ERROR_FILES+=("$ERROR_FILE")

    (
        errors=0
        for batch in $(seq 1 $BATCHES_PER_ORG); do
            partition=$(( (batch - 1) % PARTITIONS_PER_ORG ))
            BATCH_JSON=$(python3 -c "
import json
records = []
for i in range($RECORDS_PER_BATCH):
    seq = ($batch - 1) * $RECORDS_PER_BATCH + i
    records.append({
        'key': f'org${org}-rec-{seq}',
        'value': json.dumps({'org': $org, 'seq': seq, 'batch': $batch, 'type': 'load-test'})
    })
print(json.dumps({'topic': '$TOPIC', 'partition': $partition, 'records': records}))
")
            RESULT=$(curl -s -w "\n%{http_code}" -X POST "$API/produce/batch" \
                -H "Content-Type: application/json" \
                -d "$BATCH_JSON" 2>/dev/null)
            STATUS=$(echo "$RESULT" | tail -1)
            if [ "$STATUS" != "200" ]; then
                errors=$((errors + 1))
            fi
        done
        echo "$errors" > "$ERROR_FILE"
    ) &
    PIDS+=($!)
done

# Wait for all producers to complete
for pid in "${PIDS[@]}"; do
    wait "$pid" 2>/dev/null || true
done

LOAD_END=$(date +%s)
LOAD_DURATION=$((LOAD_END - LOAD_START))

# Count errors
TOTAL_ERRORS=0
for ef in "${ERROR_FILES[@]}"; do
    ORG_ERRORS=$(cat "$ef" 2>/dev/null) || ORG_ERRORS=0
    TOTAL_ERRORS=$((TOTAL_ERRORS + ORG_ERRORS))
done

if [ "$TOTAL_ERRORS" -eq 0 ]; then
    pass "Concurrent produce completed in ${LOAD_DURATION}s (0 errors)"
else
    fail "Concurrent produce" "$TOTAL_ERRORS batch errors across $NUM_ORGS orgs"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 3: Wait for flush
# ═══════════════════════════════════════════════════════════════════════════════

wait_flush 15

# ═══════════════════════════════════════════════════════════════════════════════
# Step 4: Verify all records consumed
# ═══════════════════════════════════════════════════════════════════════════════

GRAND_TOTAL=0
ORG_COUNTS=()

for org in $(seq 1 $NUM_ORGS); do
    TOPIC="load-org-${org}"
    ORG_TOTAL=0
    for p in $(seq 0 $((PARTITIONS_PER_ORG - 1))); do
        RESULT=$(curl -s "${API}/consume?topic=${TOPIC}&partition=${p}&offset=0&maxRecords=100000" 2>/dev/null) || RESULT="{}"
        COUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || COUNT=0
        ORG_TOTAL=$((ORG_TOTAL + COUNT))
    done
    ORG_COUNTS+=("$ORG_TOTAL")
    GRAND_TOTAL=$((GRAND_TOTAL + ORG_TOTAL))
done

# Report per-org counts
echo ""
echo -e "  ${DIM}Per-org record counts:${NC}"
for org in $(seq 1 $NUM_ORGS); do
    IDX=$((org - 1))
    echo -e "    ${DIM}org-${org}: ${ORG_COUNTS[$IDX]} records${NC}"
done

if [ "$GRAND_TOTAL" -ge "$EXPECTED_TOTAL" ]; then
    pass "Consumed $GRAND_TOTAL records (expected >= $EXPECTED_TOTAL)"
else
    fail "Record count" "got $GRAND_TOTAL, expected >= $EXPECTED_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 5: Throughput Report
# ═══════════════════════════════════════════════════════════════════════════════

if [ "$LOAD_DURATION" -gt 0 ]; then
    THROUGHPUT=$((EXPECTED_TOTAL / LOAD_DURATION))
    echo ""
    echo -e "  ${BOLD}Throughput:${NC} ~${THROUGHPUT} records/sec (wall-clock ${LOAD_DURATION}s)"
    echo -e "  ${DIM}(This includes Python JSON generation overhead — see Phase 08 for pure benchmarks)${NC}"
fi

# ── Cleanup ───────────────────────────────────────────────────────────────────
for org in $(seq 1 $NUM_ORGS); do
    http_request DELETE "$API/topics/load-org-${org}" &>/dev/null || true
done
