# Phase 7: Observability & Metrics

**Status**: 🔄 IN PROGRESS
**Started**: January 29, 2026
**Goal**: Add comprehensive metrics, logging, and monitoring to StreamHouse

---

## Overview

Phase 7 adds production-grade observability to StreamHouse, enabling operators to monitor system health, diagnose issues, and understand performance characteristics.

**Key Components**:
- Prometheus metrics exporter
- Structured logging with tracing
- Grafana dashboards
- Health check endpoints
- Alert rules

---

## Sub-Phases

### 7.1: Prometheus Metrics ⏳ IN PROGRESS

**Goal**: Export metrics in Prometheus format

**Metrics Categories**:

1. **Producer Metrics**
   - `streamhouse_producer_records_total` (counter) - Total records produced
   - `streamhouse_producer_bytes_total` (counter) - Total bytes produced
   - `streamhouse_producer_latency_seconds` (histogram) - Produce latency
   - `streamhouse_producer_batch_size` (histogram) - Batch sizes
   - `streamhouse_producer_errors_total` (counter) - Producer errors

2. **Consumer Metrics**
   - `streamhouse_consumer_records_total` (counter) - Total records consumed
   - `streamhouse_consumer_lag` (gauge) - Consumer lag per partition
   - `streamhouse_consumer_rebalances_total` (counter) - Rebalance count
   - `streamhouse_consumer_errors_total` (counter) - Consumer errors

3. **Storage Metrics**
   - `streamhouse_segment_writes_total` (counter) - Segments written
   - `streamhouse_segment_flushes_total` (counter) - Segment flushes to S3
   - `streamhouse_s3_requests_total` (counter) - S3 API calls by type
   - `streamhouse_s3_errors_total` (counter) - S3 errors by type
   - `streamhouse_cache_hits_total` (counter) - Cache hits
   - `streamhouse_cache_misses_total` (counter) - Cache misses

4. **System Metrics**
   - `streamhouse_connections_active` (gauge) - Active connections
   - `streamhouse_partitions_total` (gauge) - Total partitions
   - `streamhouse_topics_total` (gauge) - Total topics
   - `streamhouse_agents_active` (gauge) - Active agents

**Deliverables**:
- [ ] `streamhouse-observability` crate
- [ ] Prometheus registry and metrics
- [ ] HTTP `/metrics` endpoint
- [ ] Instrumentation in Producer API
- [ ] Instrumentation in Consumer API
- [ ] Instrumentation in WriterPool
- [ ] Instrumentation in unified server

**Estimated Effort**: 40 hours

---

### 7.2: Structured Logging ⏳ PLANNED

**Goal**: Replace ad-hoc logging with structured tracing

**Implementation**:
- Use `tracing` crate for structured logging
- Add span context (request IDs, partition IDs)
- Configure log levels (ERROR, WARN, INFO, DEBUG, TRACE)
- JSON output for production
- Human-readable output for development

**Key Spans**:
- `produce_request` - Track produce operations
- `consume_request` - Track consume operations
- `segment_flush` - Track S3 flushes
- `partition_write` - Track partition writes

**Deliverables**:
- [ ] Replace `println!` with `tracing` macros
- [ ] Add span instrumentation
- [ ] Configure `tracing-subscriber`
- [ ] Add log level control (env var)
- [ ] JSON formatter for production

**Estimated Effort**: 20 hours

---

### 7.3: Dashboards & Alerts ⏳ PLANNED

**Goal**: Provide pre-built Grafana dashboards and alert rules

**Dashboards**:

1. **Overview Dashboard**
   - System health at a glance
   - Request rate (produce/consume)
   - Error rates
   - Active connections
   - Topic/partition counts

2. **Producer Dashboard**
   - Throughput (records/sec, bytes/sec)
   - Latency (p50, p95, p99)
   - Batch sizes
   - Error rates by topic

3. **Consumer Dashboard**
   - Consumer lag by group and partition
   - Throughput (records/sec)
   - Rebalance frequency
   - Error rates

4. **Storage Dashboard**
   - S3 request rates (GET/PUT/DELETE)
   - S3 error rates
   - Segment flush rate
   - Cache hit/miss ratio
   - Storage growth

5. **System Dashboard**
   - CPU usage
   - Memory usage
   - Network I/O
   - Disk I/O
   - Connection counts

**Alert Rules**:

```yaml
- alert: HighConsumerLag
  expr: streamhouse_consumer_lag > 1000000
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Consumer lag exceeded 1M messages"

- alert: HighErrorRate
  expr: rate(streamhouse_producer_errors_total[5m]) > 0.01
  for: 2m
  labels:
    severity: critical
  annotations:
    summary: "Error rate > 1%"

- alert: S3Throttling
  expr: rate(streamhouse_s3_errors_total{error="throttling"}[1m]) > 0
  for: 1m
  labels:
    severity: warning
  annotations:
    summary: "S3 throttling detected"

- alert: LowCacheHitRate
  expr: rate(streamhouse_cache_hits_total[5m]) / (rate(streamhouse_cache_hits_total[5m]) + rate(streamhouse_cache_misses_total[5m])) < 0.5
  for: 10m
  labels:
    severity: info
  annotations:
    summary: "Cache hit rate < 50%"
```

**Deliverables**:
- [ ] 5 Grafana dashboard JSON files
- [ ] Prometheus alert rules YAML
- [ ] Dashboard screenshots
- [ ] Setup documentation

**Estimated Effort**: 20 hours

---

### 7.4: Health Endpoints ⏳ PLANNED

**Goal**: Add health check endpoints for load balancers and monitoring

**Endpoints**:

1. **`GET /health`** (Basic)
   ```json
   {
     "status": "ok",
     "timestamp": "2026-01-29T19:00:00Z"
   }
   ```

2. **`GET /health/ready`** (Detailed)
   ```json
   {
     "status": "ready",
     "checks": {
       "metadata_store": "ok",
       "object_store": "ok",
       "cache": "ok",
       "writer_pool": "ok"
     },
     "timestamp": "2026-01-29T19:00:00Z"
   }
   ```

3. **`GET /health/live`** (Liveness)
   ```json
   {
     "status": "alive",
     "uptime_seconds": 3600
   }
   ```

**Deliverables**:
- [ ] Health check handlers
- [ ] Component health checks
- [ ] Integration with unified server
- [ ] Tests

**Estimated Effort**: 10 hours

---

## Implementation Plan

### Week 1: Core Metrics (Days 1-5)

**Day 1: Setup**
- [ ] Create `streamhouse-observability` crate
- [ ] Add dependencies (prometheus, tracing)
- [ ] Define metrics registry structure

**Day 2-3: Producer/Consumer Metrics**
- [ ] Instrument Producer API
- [ ] Instrument Consumer API
- [ ] Add `/metrics` endpoint

**Day 4-5: Storage Metrics**
- [ ] Instrument WriterPool
- [ ] Instrument S3 operations
- [ ] Instrument cache

### Week 2: Logging & Dashboards (Days 6-10)

**Day 6-7: Structured Logging**
- [ ] Add tracing throughout codebase
- [ ] Configure tracing-subscriber
- [ ] Test log output formats

**Day 8-9: Dashboards**
- [ ] Create 5 Grafana dashboards
- [ ] Write alert rules
- [ ] Test with Prometheus/Grafana

**Day 10: Polish & Documentation**
- [ ] Health endpoints
- [ ] Documentation
- [ ] Examples

---

## Dependencies

**Rust Crates**:
```toml
[dependencies]
prometheus = "0.13"
lazy_static = "1.4"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
axum = { workspace = true }
tokio = { workspace = true }
```

**External Tools**:
- Prometheus (metrics collection)
- Grafana (dashboards)
- AlertManager (alerts, optional)

---

## Testing Strategy

**Unit Tests**:
- [ ] Metrics registration
- [ ] Metric value updates
- [ ] Health check logic

**Integration Tests**:
- [ ] `/metrics` endpoint returns Prometheus format
- [ ] Metrics update during operations
- [ ] Health checks reflect component status

**Manual Testing**:
- [ ] Prometheus scrapes metrics successfully
- [ ] Grafana dashboards display data
- [ ] Alert rules trigger correctly

---

## Success Criteria

- ✅ All key metrics exported in Prometheus format
- ✅ `/metrics` endpoint accessible and valid
- ✅ Structured logging throughout codebase
- ✅ 5 Grafana dashboards created and tested
- ✅ Alert rules defined and tested
- ✅ Health endpoints working
- ✅ Documentation complete with examples

---

## Estimated Effort

| Sub-Phase | Effort |
|-----------|--------|
| 7.1: Prometheus Metrics | 40 hours |
| 7.2: Structured Logging | 20 hours |
| 7.3: Dashboards & Alerts | 20 hours |
| 7.4: Health Endpoints | 10 hours |
| **Total** | **90 hours (~2 weeks)** |

---

## Next Steps

1. Create `streamhouse-observability` crate
2. Define metrics registry
3. Instrument Producer API
4. Add `/metrics` endpoint to unified server
5. Test with Prometheus locally

---

## Related Documentation

- [Phase 7 Status](PHASE_7_STATUS.md) - Already exists (empty, will update)
- [Prometheus Best Practices](https://prometheus.io/docs/practices/naming/)
- [Tracing Documentation](https://docs.rs/tracing/)
