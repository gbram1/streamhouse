# Streamhouse Testing Guide

**Status**: All services running, clean environment  
**Date**: January 31, 2026

---

## Quick Reference

```bash
# Run comprehensive feature test
./test-phase12-features.sh

# Check health
curl http://localhost:8080/health

# View metrics  
curl http://localhost:8080/metrics | grep streamhouse

# View logs
tail -f /tmp/streamhouse-server.log
```

---

## Services Running

| Service | URL | Credentials |
|---------|-----|-------------|
| **REST API** | http://localhost:8080/api/v1 | - |
| **Metrics** | http://localhost:8080/metrics | - |
| **MinIO Console** | http://localhost:9001 | minioadmin/minioadmin |

---

## REST API Quick Start

```bash
# Create topic (use "partitions" not "partition_count")
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "my-topic", "partitions": 3}'

# List topics
curl http://localhost:8080/api/v1/topics | jq '.'

# Send test messages
cargo build --release --example producer_simple
echo "Hello" | ./target/release/producer_simple my-topic
```

---

## Phase 12 Features Available

✅ **S3 Throttling** - Rate limiting + circuit breaker  
✅ **47 Prometheus Metrics** - Full observability  
✅ **22 Alert Rules** - Proactive monitoring  
✅ **6 Runbooks** - Incident response guides  
✅ **3 Grafana Dashboards** - Visual monitoring

See full details in `docs/` and `monitoring/` directories.
