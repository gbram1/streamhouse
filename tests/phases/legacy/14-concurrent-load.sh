#!/usr/bin/env bash
# Phase 14 — Concurrent Load Test
# Stress test with parallel producers across multiple topics

phase_header "Phase 14 — Concurrent Load Test"

API="${TEST_HTTP}/api/v1"
NUM_TOPICS=5
PARTITIONS=4
BATCHES_PER_TOPIC=100
RECORDS_PER_BATCH=50
EXPECTED_PER_TOPIC=$((BATCHES_PER_TOPIC * RECORDS_PER_BATCH))  # 5000
EXPECTED_TOTAL=$((NUM_TOPICS * EXPECTED_PER_TOPIC))              # 25000

echo -e "  ${DIM}Config: $NUM_TOPICS topics x $PARTITIONS partitions x $BATCHES_PER_TOPIC batches x $RECORDS_PER_BATCH records${NC}"
echo -e "  ${DIM}Expected total: $EXPECTED_TOTAL records${NC}"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# Step 1: Create topics
# ═══════════════════════════════════════════════════════════════════════════════

CREATE_ERRORS=0
for t in $(seq 1 $NUM_TOPICS); do
    TOPIC="load-topic-${t}"
    http_request POST "$API/topics" "{\"name\":\"$TOPIC\",\"partitions\":$PARTITIONS}"
    if [ "$HTTP_STATUS" != "201" ] && [ "$HTTP_STATUS" != "409" ]; then
        CREATE_ERRORS=$((CREATE_ERRORS + 1))
    fi
done

if [ "$CREATE_ERRORS" -eq 0 ]; then
    pass "Created $NUM_TOPICS topics ($PARTITIONS partitions each)"
else
    fail "Topic creation" "$CREATE_ERRORS topics failed to create"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 2: Launch concurrent producers (one per topic)
# ═══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}Starting $NUM_TOPICS concurrent producers ($BATCHES_PER_TOPIC batches x $RECORDS_PER_BATCH records each)...${NC}"
LOAD_START=$(date +%s)

PIDS=()
ERROR_FILES=()

for t in $(seq 1 $NUM_TOPICS); do
    TOPIC="load-topic-${t}"
    ERROR_FILE="${TEST_TMPDIR}/load-errors-topic-${t}.txt"
    echo "0" > "$ERROR_FILE"
    ERROR_FILES+=("$ERROR_FILE")

    (
        errors=0
        for batch in $(seq 1 $BATCHES_PER_TOPIC); do
            partition=$(( (batch - 1) % PARTITIONS ))
            BATCH_JSON=$(python3 -c "
import json
records = []
for i in range($RECORDS_PER_BATCH):
    seq = ($batch - 1) * $RECORDS_PER_BATCH + i
    records.append({
        'key': f'topic${t}-rec-{seq}',
        'value': json.dumps({'topic': $t, 'seq': seq, 'batch': $batch, 'type': 'load-test'})
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
if [ "$LOAD_DURATION" -eq 0 ]; then
    LOAD_DURATION=1
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 3: Report error counts per topic
# ═══════════════════════════════════════════════════════════════════════════════

TOTAL_ERRORS=0
echo ""
echo -e "  ${DIM}Error counts per topic:${NC}"
for t in $(seq 1 $NUM_TOPICS); do
    IDX=$((t - 1))
    TOPIC_ERRORS=$(cat "${ERROR_FILES[$IDX]}" 2>/dev/null) || TOPIC_ERRORS=0
    echo -e "    ${DIM}load-topic-${t}: $TOPIC_ERRORS batch errors${NC}"
    TOTAL_ERRORS=$((TOTAL_ERRORS + TOPIC_ERRORS))
done

if [ "$TOTAL_ERRORS" -eq 0 ]; then
    pass "All producers completed with 0 errors (${LOAD_DURATION}s)"
else
    fail "Concurrent produce" "$TOTAL_ERRORS total batch errors across $NUM_TOPICS topics"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 4: Wait for flush
# ═══════════════════════════════════════════════════════════════════════════════

wait_flush 15

# ═══════════════════════════════════════════════════════════════════════════════
# Step 5: Consume and count all records
# ═══════════════════════════════════════════════════════════════════════════════

GRAND_TOTAL=0
TOPIC_COUNTS=()

for t in $(seq 1 $NUM_TOPICS); do
    TOPIC="load-topic-${t}"
    TOPIC_TOTAL=0
    for p in $(seq 0 $((PARTITIONS - 1))); do
        RESULT=$(curl -s "${API}/consume?topic=${TOPIC}&partition=${p}&offset=0&maxRecords=100000" 2>/dev/null) || RESULT="{}"
        COUNT=$(echo "$RESULT" | jq '.records | length' 2>/dev/null) || COUNT=0
        TOPIC_TOTAL=$((TOPIC_TOTAL + COUNT))
    done
    TOPIC_COUNTS+=("$TOPIC_TOTAL")
    GRAND_TOTAL=$((GRAND_TOTAL + TOPIC_TOTAL))
done

# Report per-topic counts
echo ""
echo -e "  ${DIM}Per-topic record counts:${NC}"
for t in $(seq 1 $NUM_TOPICS); do
    IDX=$((t - 1))
    echo -e "    ${DIM}load-topic-${t}: ${TOPIC_COUNTS[$IDX]} records (expected $EXPECTED_PER_TOPIC)${NC}"
done

if [ "$GRAND_TOTAL" -ge "$EXPECTED_TOTAL" ]; then
    pass "Total records: $GRAND_TOTAL (expected >= $EXPECTED_TOTAL)"
else
    fail "Total records" "got $GRAND_TOTAL, expected >= $EXPECTED_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Step 6: Throughput report
# ═══════════════════════════════════════════════════════════════════════════════

THROUGHPUT=$((EXPECTED_TOTAL / LOAD_DURATION))
echo ""
echo -e "  ${BOLD}Throughput:${NC} ~${THROUGHPUT} records/sec (wall-clock ${LOAD_DURATION}s)"
echo -e "  ${DIM}(Includes Python JSON generation overhead — see benchmarks for raw throughput)${NC}"

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════════════════════════════════════════

for t in $(seq 1 $NUM_TOPICS); do
    http_request DELETE "$API/topics/load-topic-${t}" &>/dev/null || true
done
