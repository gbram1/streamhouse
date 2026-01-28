#!/bin/bash

# StreamHouse System Health Check
# Run this to get instant system status

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Configuration
METADATA_DB="${METADATA_DB:-./data/metadata.db}"
MINIO_HOST="${MINIO_HOST:-streamhouse-minio}"
MINIO_BUCKET="${MINIO_BUCKET:-streamhouse-data}"

echo "╔══════════════════════════════════════════════════════╗"
echo "║        StreamHouse System Health Check              ║"
echo "╚══════════════════════════════════════════════════════╝"
echo

PASS=0
WARN=0
FAIL=0

# ==============================================================================
# Infrastructure Checks
# ==============================================================================

echo -e "${CYAN}${BOLD}Infrastructure:${NC}"

# PostgreSQL
if docker ps --format '{{.Names}}' | grep -q "streamhouse-postgres" 2>/dev/null; then
    LATENCY=$(docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT 1;" -t 2>&1 | grep -q "1" && echo "OK" || echo "FAIL")
    if [ "$LATENCY" = "OK" ]; then
        echo -e "  ${GREEN}✓${NC} PostgreSQL   [HEALTHY]"
        ((PASS++))
    else
        echo -e "  ${RED}✗${NC} PostgreSQL   [DEGRADED] - Queries failing"
        ((FAIL++))
    fi
else
    echo -e "  ${YELLOW}!${NC} PostgreSQL   [NOT RUNNING]"
    ((WARN++))
fi

# MinIO
if docker ps --format '{{.Names}}' | grep -q "$MINIO_HOST" 2>/dev/null; then
    MINIO_CHECK=$(docker exec $MINIO_HOST mc ls myminio/$MINIO_BUCKET 2>/dev/null && echo "OK" || echo "FAIL")
    if [ "$MINIO_CHECK" = "OK" ]; then
        MINIO_SIZE=$(docker exec $MINIO_HOST mc du myminio/$MINIO_BUCKET 2>/dev/null | awk '{print $1, $2}' || echo "unknown")
        echo -e "  ${GREEN}✓${NC} MinIO        [HEALTHY]  $MINIO_SIZE used"
        ((PASS++))
    else
        echo -e "  ${RED}✗${NC} MinIO        [DEGRADED] - Bucket not accessible"
        ((FAIL++))
    fi
else
    echo -e "  ${RED}✗${NC} MinIO        [NOT RUNNING]"
    ((FAIL++))
fi

# Prometheus
if curl -s http://localhost:9090/api/v1/query?query=up > /dev/null 2>&1; then
    echo -e "  ${GREEN}✓${NC} Prometheus   [HEALTHY]"
    ((PASS++))
else
    echo -e "  ${YELLOW}!${NC} Prometheus   [NOT ACCESSIBLE]"
    ((WARN++))
fi

# Grafana
if curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
    DASHBOARD_COUNT=$(curl -s http://localhost:3000/api/search 2>/dev/null | grep -o "title" | wc -l | tr -d ' ' || echo "0")
    echo -e "  ${GREEN}✓${NC} Grafana      [HEALTHY]  $DASHBOARD_COUNT dashboards"
    ((PASS++))
else
    echo -e "  ${YELLOW}!${NC} Grafana      [NOT ACCESSIBLE]"
    ((WARN++))
fi

echo

# ==============================================================================
# Agent Health Checks
# ==============================================================================

echo -e "${CYAN}${BOLD}Agents:${NC}"

if [ -f "$METADATA_DB" ]; then
    AGENTS=$(sqlite3 $METADATA_DB "
    SELECT
        agent_id,
        COALESCE((SELECT COUNT(*) FROM partition_leases pl WHERE pl.owner_agent_id = a.agent_id AND pl.expires_at > datetime('now')), 0) as partitions,
        CAST((julianday('now') - julianday(last_heartbeat_at)) * 86400 AS INTEGER) as seconds_ago
    FROM agents a
    WHERE last_heartbeat_at > datetime('now', '-120 seconds')
    ORDER BY agent_id;
    " 2>/dev/null)

    if [ -n "$AGENTS" ]; then
        while IFS='|' read -r agent_id partition_count seconds_ago; do
            if [ "$seconds_ago" -lt 30 ]; then
                echo -e "  ${GREEN}✓${NC} $agent_id      [HEALTHY]  $partition_count partitions | last heartbeat: ${seconds_ago}s ago"
                ((PASS++))
            elif [ "$seconds_ago" -lt 60 ]; then
                echo -e "  ${YELLOW}!${NC} $agent_id      [WARNING]  $partition_count partitions | last heartbeat: ${seconds_ago}s ago"
                ((WARN++))
            else
                echo -e "  ${RED}✗${NC} $agent_id      [STALE]    $partition_count partitions | last heartbeat: ${seconds_ago}s ago"
                ((FAIL++))
            fi
        done <<< "$AGENTS"
    else
        echo -e "  ${RED}✗${NC} No agents found with recent heartbeats"
        ((FAIL++))
    fi
else
    echo -e "  ${YELLOW}!${NC} Metadata DB not found at $METADATA_DB"
    ((WARN++))
fi

echo

# ==============================================================================
# Topic and Partition Health
# ==============================================================================

echo -e "${CYAN}${BOLD}Topics:${NC}"

if [ -f "$METADATA_DB" ]; then
    TOPICS=$(sqlite3 $METADATA_DB "
    SELECT
        t.name,
        t.partition_count,
        COALESCE(SUM(p.high_watermark), 0) as total_messages,
        COALESCE((SELECT COUNT(*) FROM partitions pt
                  LEFT JOIN partition_leases pl ON pt.topic = pl.topic AND pt.partition_id = pl.partition_id AND pl.expires_at > datetime('now')
                  WHERE pt.topic = t.name AND pl.owner_agent_id IS NULL), 0) as orphans
    FROM topics t
    LEFT JOIN partitions p ON t.name = p.topic
    GROUP BY t.name, t.partition_count;
    " 2>/dev/null)

    if [ -n "$TOPICS" ]; then
        while IFS='|' read -r topic partition_count total_messages orphans; do
            if [ "$orphans" -eq 0 ]; then
                echo -e "  ${GREEN}✓${NC} $topic          $partition_count partitions | $total_messages messages | 0 orphans"
                ((PASS++))
            else
                echo -e "  ${RED}✗${NC} $topic          $partition_count partitions | $total_messages messages | $orphans orphans"
                ((FAIL++))
            fi
        done <<< "$TOPICS"
    else
        echo -e "  ${YELLOW}!${NC} No topics found"
        ((WARN++))
    fi
fi

echo

# ==============================================================================
# Consumer Health
# ==============================================================================

echo -e "${CYAN}${BOLD}Consumers:${NC}"

if [ -f "$METADATA_DB" ]; then
    CONSUMER_GROUPS=$(sqlite3 $METADATA_DB "
    SELECT DISTINCT group_id FROM consumer_offsets ORDER BY group_id;
    " 2>/dev/null)

    if [ -n "$CONSUMER_GROUPS" ]; then
        while read -r group_id; do
            TOTAL_LAG=$(sqlite3 $METADATA_DB "
            SELECT SUM(p.high_watermark - COALESCE(co.committed_offset, 0))
            FROM partitions p
            LEFT JOIN consumer_offsets co ON p.topic = co.topic AND p.partition_id = co.partition_id AND co.group_id = '$group_id';
            " 2>/dev/null)

            if [ -z "$TOTAL_LAG" ] || [ "$TOTAL_LAG" = "" ]; then
                TOTAL_LAG=0
            fi

            if [ "$TOTAL_LAG" -lt 1000 ]; then
                echo -e "  ${GREEN}✓${NC} $group_id    lag: $TOTAL_LAG messages   (HEALTHY)"
                ((PASS++))
            elif [ "$TOTAL_LAG" -lt 10000 ]; then
                echo -e "  ${YELLOW}⚠${NC} $group_id    lag: $TOTAL_LAG messages (WARNING)"
                ((WARN++))
            else
                echo -e "  ${RED}✗${NC} $group_id    lag: $TOTAL_LAG messages (CRITICAL)"
                ((FAIL++))
            fi
        done <<< "$CONSUMER_GROUPS"
    else
        echo -e "  ${YELLOW}!${NC} No consumer groups found"
        ((WARN++))
    fi
fi

echo

# ==============================================================================
# Data Flow Metrics
# ==============================================================================

echo -e "${CYAN}${BOLD}Data Flow:${NC}"

# Check MinIO segments created recently
if docker ps --format '{{.Names}}' | grep -q "$MINIO_HOST" 2>/dev/null; then
    RECENT_SEGMENTS=$(docker exec $MINIO_HOST mc ls myminio/$MINIO_BUCKET/data/ --recursive 2>/dev/null | \
        awk -v d="$(date -u -v-5M '+%Y-%m-%d %H:%M' 2>/dev/null || date -u -d '5 minutes ago' '+%Y-%m-%d %H:%M')" '$0 > d' | \
        wc -l | tr -d ' ')

    if [ "$RECENT_SEGMENTS" -gt 0 ]; then
        echo -e "  ${GREEN}✓${NC} Storage rate:  $RECENT_SEGMENTS segments in last 5 min"
        ((PASS++))
    else
        echo -e "  ${YELLOW}!${NC} Storage rate:  No recent segments (last 5 min)"
        ((WARN++))
    fi
fi

# Total segments
TOTAL_SEGMENTS=$(docker exec $MINIO_HOST mc ls myminio/$MINIO_BUCKET/data/ --recursive 2>/dev/null | grep -c '\.seg' || echo "0")
echo -e "  ${GREEN}✓${NC} Total segments: $TOTAL_SEGMENTS"

echo

# ==============================================================================
# Summary
# ==============================================================================

echo "╔══════════════════════════════════════════════════════╗"
echo "║                   SUMMARY                            ║"
echo "╚══════════════════════════════════════════════════════╝"
echo

TOTAL=$((PASS + WARN + FAIL))
echo "Checks: $PASS passed, $WARN warnings, $FAIL failed (total: $TOTAL)"
echo

if [ $FAIL -eq 0 ] && [ $WARN -eq 0 ]; then
    echo -e "${GREEN}${BOLD}Overall Status: HEALTHY ✓${NC}"
    exit 0
elif [ $FAIL -eq 0 ]; then
    echo -e "${YELLOW}${BOLD}Overall Status: HEALTHY (with warnings) !${NC}"
    exit 0
elif [ $FAIL -lt 3 ]; then
    echo -e "${YELLOW}${BOLD}Overall Status: DEGRADED ⚠${NC}"
    exit 1
else
    echo -e "${RED}${BOLD}Overall Status: CRITICAL ✗${NC}"
    exit 2
fi
