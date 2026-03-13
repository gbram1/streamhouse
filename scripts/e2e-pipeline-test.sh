#!/bin/bash
set -e

API="http://localhost:8080/api/v1"
PG_CMD="docker exec -i streamhouse-postgres psql -U streamhouse -d streamhouse"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

step() { echo -e "\n${CYAN}=== $1 ===${NC}"; }
info() { echo -e "${YELLOW}$1${NC}"; }
ok() { echo -e "${GREEN}✓ $1${NC}"; }

# -----------------------------------------------------------
# Step 1: Create 6 Postgres tables
# -----------------------------------------------------------
step "1. Creating 6 Postgres sink tables"

$PG_CMD <<'SQL'
DROP TABLE IF EXISTS user_signups;
DROP TABLE IF EXISTS user_activity;
DROP TABLE IF EXISTS completed_orders;
DROP TABLE IF EXISTS order_summary;
DROP TABLE IF EXISTS error_logs;
DROP TABLE IF EXISTS log_overview;

CREATE TABLE user_signups (
  id TEXT, name TEXT, email TEXT
);
CREATE TABLE user_activity (
  id TEXT, name TEXT, action TEXT
);
CREATE TABLE completed_orders (
  order_id TEXT, product TEXT, amount TEXT, status TEXT
);
CREATE TABLE order_summary (
  order_id TEXT, product TEXT, amount TEXT
);
CREATE TABLE error_logs (
  ts TEXT, level TEXT, message TEXT, service TEXT
);
CREATE TABLE log_overview (
  ts TEXT, level TEXT, message TEXT
);
SQL
ok "6 tables created"

# -----------------------------------------------------------
# Step 2: Create 3 topics
# -----------------------------------------------------------
step "2. Creating 3 topics"

for topic in user-events orders app-logs; do
  curl -sf -X POST "$API/topics" \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"$topic\", \"partitions\": 1, \"retention_ms\": 86400000}" > /dev/null 2>&1 || true
  ok "Topic: $topic"
done

# -----------------------------------------------------------
# Step 3: Create 6 pipeline targets (one per table)
# -----------------------------------------------------------
step "3. Creating 6 pipeline targets"

create_target() {
  local table=$1
  resp=$(curl -sf -X POST "$API/pipeline-targets" \
    -H "Content-Type: application/json" \
    -d "{
      \"name\": \"pg-$table\",
      \"targetType\": \"postgres\",
      \"connectionConfig\": {
        \"connection_url\": \"postgres://streamhouse:streamhouse@postgres:5432/streamhouse\",
        \"table_name\": \"$table\",
        \"insert_mode\": \"insert\"
      }
    }")
  echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])"
}

TID_USER_SIGNUPS=$(create_target user_signups)
ok "Target: pg-user_signups -> $TID_USER_SIGNUPS"
TID_USER_ACTIVITY=$(create_target user_activity)
ok "Target: pg-user_activity -> $TID_USER_ACTIVITY"
TID_COMPLETED_ORDERS=$(create_target completed_orders)
ok "Target: pg-completed_orders -> $TID_COMPLETED_ORDERS"
TID_ORDER_SUMMARY=$(create_target order_summary)
ok "Target: pg-order_summary -> $TID_ORDER_SUMMARY"
TID_ERROR_LOGS=$(create_target error_logs)
ok "Target: pg-error_logs -> $TID_ERROR_LOGS"
TID_LOG_OVERVIEW=$(create_target log_overview)
ok "Target: pg-log_overview -> $TID_LOG_OVERVIEW"

# -----------------------------------------------------------
# Step 4: Create 6 pipelines with different transforms
# -----------------------------------------------------------
step "4. Creating 6 pipelines"

# Pipeline 1: user-events -> user_signups (only signups)
curl -sf -X POST "$API/pipelines" \
  -H "Content-Type: application/json" \
  -d "{
    \"name\": \"pipe-user-signups\",
    \"sourceTopic\": \"user-events\",
    \"targetId\": \"$TID_USER_SIGNUPS\",
    \"transformSql\": \"SELECT key, value FROM input WHERE value LIKE '%\\\"action\\\":\\\"signup\\\"%'\"
  }" > /dev/null
ok "pipe-user-signups: user-events -> user_signups (WHERE action=signup)"

# Pipeline 2: user-events -> user_activity (all events, table drops email/_trace)
curl -sf -X POST "$API/pipelines" \
  -H "Content-Type: application/json" \
  -d "{
    \"name\": \"pipe-user-activity\",
    \"sourceTopic\": \"user-events\",
    \"targetId\": \"$TID_USER_ACTIVITY\",
    \"transformSql\": \"SELECT key, value FROM input\"
  }" > /dev/null
ok "pipe-user-activity: user-events -> user_activity (all events, drops extra cols)"

# Pipeline 3: orders -> completed_orders (only completed)
curl -sf -X POST "$API/pipelines" \
  -H "Content-Type: application/json" \
  -d "{
    \"name\": \"pipe-completed-orders\",
    \"sourceTopic\": \"orders\",
    \"targetId\": \"$TID_COMPLETED_ORDERS\",
    \"transformSql\": \"SELECT key, value FROM input WHERE value LIKE '%\\\"status\\\":\\\"completed\\\"%'\"
  }" > /dev/null
ok "pipe-completed-orders: orders -> completed_orders (WHERE status=completed)"

# Pipeline 4: orders -> order_summary (all orders, table drops status/_internal/metadata)
curl -sf -X POST "$API/pipelines" \
  -H "Content-Type: application/json" \
  -d "{
    \"name\": \"pipe-order-summary\",
    \"sourceTopic\": \"orders\",
    \"targetId\": \"$TID_ORDER_SUMMARY\",
    \"transformSql\": \"SELECT key, value FROM input\"
  }" > /dev/null
ok "pipe-order-summary: orders -> order_summary (all orders, drops extra cols)"

# Pipeline 5: app-logs -> error_logs (only errors)
curl -sf -X POST "$API/pipelines" \
  -H "Content-Type: application/json" \
  -d "{
    \"name\": \"pipe-error-logs\",
    \"sourceTopic\": \"app-logs\",
    \"targetId\": \"$TID_ERROR_LOGS\",
    \"transformSql\": \"SELECT key, value FROM input WHERE value LIKE '%\\\"level\\\":\\\"error\\\"%'\"
  }" > /dev/null
ok "pipe-error-logs: app-logs -> error_logs (WHERE level=error)"

# Pipeline 6: app-logs -> log_overview (all logs, table drops service/_debug/trace_id)
curl -sf -X POST "$API/pipelines" \
  -H "Content-Type: application/json" \
  -d "{
    \"name\": \"pipe-log-overview\",
    \"sourceTopic\": \"app-logs\",
    \"targetId\": \"$TID_LOG_OVERVIEW\",
    \"transformSql\": \"SELECT key, value FROM input\"
  }" > /dev/null
ok "pipe-log-overview: app-logs -> log_overview (all logs, drops extra cols)"

# -----------------------------------------------------------
# Step 5: Start all pipelines
# -----------------------------------------------------------
step "5. Starting all 6 pipelines"

for pipe in pipe-user-signups pipe-user-activity pipe-completed-orders pipe-order-summary pipe-error-logs pipe-log-overview; do
  curl -sf -X PATCH "$API/pipelines/$pipe" \
    -H "Content-Type: application/json" \
    -d '{"state": "running"}' > /dev/null
  ok "Started: $pipe"
done

# Give pipelines a moment to initialize
sleep 3

# -----------------------------------------------------------
# Step 6: Produce messages to all 3 topics
# -----------------------------------------------------------
step "6. Producing messages to 3 topics"

# --- user-events: 5 messages (mix of signup, login, purchase) ---
info "Producing 5 user-events..."
curl -sf -X POST "$API/produce/batch" \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "user-events",
    "records": [
      {"key": "u1", "value": "{\"id\":\"u1\",\"name\":\"Alice\",\"email\":\"alice@example.com\",\"action\":\"signup\",\"_trace\":\"t1\"}"},
      {"key": "u2", "value": "{\"id\":\"u2\",\"name\":\"Bob\",\"email\":\"bob@example.com\",\"action\":\"login\",\"_trace\":\"t2\"}"},
      {"key": "u3", "value": "{\"id\":\"u3\",\"name\":\"Charlie\",\"email\":\"charlie@example.com\",\"action\":\"signup\",\"_trace\":\"t3\"}"},
      {"key": "u4", "value": "{\"id\":\"u4\",\"name\":\"Diana\",\"email\":\"diana@example.com\",\"action\":\"purchase\",\"_trace\":\"t4\"}"},
      {"key": "u5", "value": "{\"id\":\"u5\",\"name\":\"Eve\",\"email\":\"eve@example.com\",\"action\":\"signup\",\"_trace\":\"t5\"}"}
    ]
  }' > /dev/null
ok "5 user-events produced"

# --- orders: 6 messages (mix of completed, pending, cancelled) ---
info "Producing 6 orders..."
curl -sf -X POST "$API/produce/batch" \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "orders",
    "records": [
      {"key": "ord-1", "value": "{\"order_id\":\"ord-1\",\"product\":\"Laptop\",\"amount\":\"1299.99\",\"status\":\"completed\",\"_internal\":\"x\",\"metadata\":\"batch-1\"}"},
      {"key": "ord-2", "value": "{\"order_id\":\"ord-2\",\"product\":\"Mouse\",\"amount\":\"29.99\",\"status\":\"pending\",\"_internal\":\"x\",\"metadata\":\"batch-1\"}"},
      {"key": "ord-3", "value": "{\"order_id\":\"ord-3\",\"product\":\"Keyboard\",\"amount\":\"89.99\",\"status\":\"completed\",\"_internal\":\"x\",\"metadata\":\"batch-1\"}"},
      {"key": "ord-4", "value": "{\"order_id\":\"ord-4\",\"product\":\"Monitor\",\"amount\":\"499.99\",\"status\":\"cancelled\",\"_internal\":\"x\",\"metadata\":\"batch-2\"}"},
      {"key": "ord-5", "value": "{\"order_id\":\"ord-5\",\"product\":\"Headphones\",\"amount\":\"199.99\",\"status\":\"completed\",\"_internal\":\"x\",\"metadata\":\"batch-2\"}"},
      {"key": "ord-6", "value": "{\"order_id\":\"ord-6\",\"product\":\"Webcam\",\"amount\":\"79.99\",\"status\":\"pending\",\"_internal\":\"x\",\"metadata\":\"batch-2\"}"}
    ]
  }' > /dev/null
ok "6 orders produced"

# --- app-logs: 8 messages (mix of error, warn, info, debug) ---
info "Producing 8 app-logs..."
curl -sf -X POST "$API/produce/batch" \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "app-logs",
    "records": [
      {"key": "log-1", "value": "{\"ts\":\"2026-03-13T10:00:01Z\",\"level\":\"info\",\"message\":\"Server started\",\"service\":\"api\",\"_debug\":\"d1\",\"trace_id\":\"abc\"}"},
      {"key": "log-2", "value": "{\"ts\":\"2026-03-13T10:00:05Z\",\"level\":\"error\",\"message\":\"Database connection failed\",\"service\":\"api\",\"_debug\":\"d2\",\"trace_id\":\"def\"}"},
      {"key": "log-3", "value": "{\"ts\":\"2026-03-13T10:00:10Z\",\"level\":\"warn\",\"message\":\"High memory usage\",\"service\":\"worker\",\"_debug\":\"d3\",\"trace_id\":\"ghi\"}"},
      {"key": "log-4", "value": "{\"ts\":\"2026-03-13T10:00:15Z\",\"level\":\"error\",\"message\":\"Timeout on upstream call\",\"service\":\"gateway\",\"_debug\":\"d4\",\"trace_id\":\"jkl\"}"},
      {"key": "log-5", "value": "{\"ts\":\"2026-03-13T10:00:20Z\",\"level\":\"info\",\"message\":\"Request processed\",\"service\":\"api\",\"_debug\":\"d5\",\"trace_id\":\"mno\"}"},
      {"key": "log-6", "value": "{\"ts\":\"2026-03-13T10:00:25Z\",\"level\":\"debug\",\"message\":\"Cache hit ratio 0.95\",\"service\":\"cache\",\"_debug\":\"d6\",\"trace_id\":\"pqr\"}"},
      {"key": "log-7", "value": "{\"ts\":\"2026-03-13T10:00:30Z\",\"level\":\"error\",\"message\":\"Null pointer in handler\",\"service\":\"api\",\"_debug\":\"d7\",\"trace_id\":\"stu\"}"},
      {"key": "log-8", "value": "{\"ts\":\"2026-03-13T10:00:35Z\",\"level\":\"info\",\"message\":\"Batch job completed\",\"service\":\"worker\",\"_debug\":\"d8\",\"trace_id\":\"vwx\"}"}
    ]
  }' > /dev/null
ok "8 app-logs produced"

# -----------------------------------------------------------
# Step 7: Wait for pipelines to process
# -----------------------------------------------------------
step "7. Waiting 15 seconds for pipelines to process..."
sleep 15

# -----------------------------------------------------------
# Step 8: Verify all 6 tables
# -----------------------------------------------------------
step "8. Verifying all 6 Postgres tables"

echo ""
info "--- user_signups (expect 3: Alice, Charlie, Eve) ---"
info "--- columns: id, name, email (drops: action, _trace) ---"
$PG_CMD -c "SELECT * FROM user_signups ORDER BY id;"

echo ""
info "--- user_activity (expect 5: all users) ---"
info "--- columns: id, name, action (drops: email, _trace) ---"
$PG_CMD -c "SELECT * FROM user_activity ORDER BY id;"

echo ""
info "--- completed_orders (expect 3: Laptop, Keyboard, Headphones) ---"
info "--- columns: order_id, product, amount, status (drops: _internal, metadata) ---"
$PG_CMD -c "SELECT * FROM completed_orders ORDER BY order_id;"

echo ""
info "--- order_summary (expect 6: all orders) ---"
info "--- columns: order_id, product, amount (drops: status, _internal, metadata) ---"
$PG_CMD -c "SELECT * FROM order_summary ORDER BY order_id;"

echo ""
info "--- error_logs (expect 3: DB fail, Timeout, Null pointer) ---"
info "--- columns: ts, level, message, service (drops: _debug, trace_id) ---"
$PG_CMD -c "SELECT * FROM error_logs ORDER BY ts;"

echo ""
info "--- log_overview (expect 8: all logs) ---"
info "--- columns: ts, level, message (drops: service, _debug, trace_id) ---"
$PG_CMD -c "SELECT * FROM log_overview ORDER BY ts;"

# -----------------------------------------------------------
# Step 9: Summary
# -----------------------------------------------------------
step "9. Row count summary"

$PG_CMD <<'SQL'
SELECT 'user_signups' as table_name, count(*) as rows FROM user_signups
UNION ALL SELECT 'user_activity', count(*) FROM user_activity
UNION ALL SELECT 'completed_orders', count(*) FROM completed_orders
UNION ALL SELECT 'order_summary', count(*) FROM order_summary
UNION ALL SELECT 'error_logs', count(*) FROM error_logs
UNION ALL SELECT 'log_overview', count(*) FROM log_overview
ORDER BY table_name;
SQL

echo ""
info "Expected:"
info "  user_signups:     3 (signup events only)"
info "  user_activity:    5 (all user events, fewer columns)"
info "  completed_orders: 3 (completed orders only)"
info "  order_summary:    6 (all orders, fewer columns)"
info "  error_logs:       3 (error logs only)"
info "  log_overview:     8 (all logs, fewer columns)"
echo ""
ok "E2E pipeline test complete!"
