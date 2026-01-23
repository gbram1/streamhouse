# PostgreSQL Backend Guide

**Phase**: 3.2 - PostgreSQL Backend
**Status**: ✅ Production Ready
**Version**: StreamHouse v0.1.0
**Date**: 2026-01-22

## Quick Start

### Local Development with Docker

```bash
# Start PostgreSQL
docker-compose up -d postgres

# Verify connection
docker exec streamhouse-postgres pg_isready -U streamhouse

# Set environment variable
export DATABASE_URL=postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata

# Migrations run automatically on first connection
cargo run --features postgres
```

### Build Options

```bash
# SQLite only (default)
cargo build

# PostgreSQL only
cargo build --features postgres

# Both backends
cargo build --features all-backends
```

### Run Tests

```bash
# SQLite tests
cargo test --package streamhouse-metadata

# PostgreSQL tests (requires running PostgreSQL)
DATABASE_URL=postgres://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata \
  cargo test --features postgres --package streamhouse-metadata postgres::tests -- --ignored
```

## What Gets Stored in PostgreSQL?

**PostgreSQL stores METADATA ONLY.** Actual streaming events live in S3.

| Table | Purpose | Example Data | Row Size |
|-------|---------|--------------|----------|
| `topics` | Topic definitions | Name, partition count, retention policy | ~100 bytes |
| `partitions` | Partition state | High watermark offsets | ~50 bytes |
| `segments` | S3 file locations | `s3://bucket/topic/0/00000000.seg` | ~200 bytes |
| `consumer_groups` | Consumer registration | Group ID, timestamps | ~80 bytes |
| `consumer_offsets` | Consumer progress | Which offset each group consumed | ~100 bytes |
| `agents` | Agent registration (Phase 4) | Agent ID, address, heartbeat | ~200 bytes |
| `partition_leases` | Leadership leases (Phase 4) | Leader agent, expiration, epoch | ~150 bytes |

**Total metadata for 1M events**: ~20KB in PostgreSQL, 1GB in S3

## Data Flow

### Write Path (Producer → S3)

```
Producer sends records
    ↓
StreamHouse Agent buffers in memory
    ↓
Segment Writer flushes to S3
    ↓
PostgreSQL: INSERT INTO segments (s3_key, base_offset, end_offset)
    ↓
PostgreSQL: UPDATE partitions SET high_watermark = new_offset
```

**PostgreSQL operations per segment flush**: 2 queries (INSERT + UPDATE)

### Read Path (Consumer ← S3)

```
Consumer requests offset 5000
    ↓
PostgreSQL: SELECT * FROM segments
            WHERE topic = 'events' AND partition_id = 0
              AND base_offset <= 5000 AND end_offset >= 5000
    ↓
Returns: SegmentInfo { s3_key: "s3://bucket/events/0/00005000.seg" }
    ↓
Fetch from S3 → Decompress LZ4 → Return records
```

**PostgreSQL queries per consumer read**: 1 (segment lookup)

## Configuration

### Environment Variables

```bash
# PostgreSQL connection string
DATABASE_URL=postgres://user:password@host:port/database

# With SSL (production)
DATABASE_URL=postgres://user:pass@db.example.com/streamhouse?sslmode=require

# Connection pool size (default: 20)
# Set via code: PostgresMetadataStore::with_pool_options()
```

### Connection Pooling

Default configuration:
- **Max connections**: 20
- **Min connections**: 0 (lazy initialization)
- **Connection timeout**: 30 seconds
- **Idle timeout**: 10 minutes

Adjust for your workload:

```rust
use sqlx::postgres::PgPoolOptions;
use streamhouse_metadata::PostgresMetadataStore;

let pool_options = PgPoolOptions::new()
    .max_connections(50)           // Increase for high concurrency
    .min_connections(5)            // Keep warm connections
    .acquire_timeout(Duration::from_secs(10));

let store = PostgresMetadataStore::with_pool_options(
    "postgres://...",
    pool_options
).await?;
```

## Performance

### Latency Targets (PostgreSQL 16, db.t3.medium)

| Operation | Expected Latency | Notes |
|-----------|------------------|-------|
| `create_topic` | < 50ms | Transaction: INSERT topic + N partitions |
| `get_topic` | < 5ms | Primary key lookup |
| `add_segment` | < 10ms | Single INSERT |
| `find_segment_for_offset` | < 10ms | Indexed range query |
| `update_high_watermark` | < 10ms | Single UPDATE |
| `commit_offset` | < 10ms | UPSERT (INSERT ... ON CONFLICT) |
| `acquire_partition_lease` | < 50ms | CAS operation with ON CONFLICT |

**Note**: Latencies include network RTT (~1-5ms within same region)

### Throughput Targets

| Workload | Throughput | Bottleneck |
|----------|------------|------------|
| Segment writes (100MB/s ingest) | ~100 QPS | Network to S3 |
| Consumer offset commits | 5,000+ QPS | PostgreSQL write capacity |
| Metadata queries (list topics) | 10,000+ QPS | PostgreSQL read replicas |

### Scaling for 10K+ Partitions

Creating a topic with 10,000 partitions:
- **Operation**: Single transaction (INSERT topic + 10K partition rows)
- **Expected time**: < 30 seconds
- **PostgreSQL load**: Brief spike in INSERT activity

Listing 10,000 partitions:
- **Operation**: SELECT with ORDER BY partition_id
- **Expected time**: < 5 seconds
- **Optimization**: Add LIMIT for pagination

## Production Deployment

### Recommended PostgreSQL Setup

```yaml
# AWS RDS / Aurora
Instance Type: db.r6g.xlarge (4 vCPU, 32GB RAM)
Storage: 100GB gp3 (3000 IOPS, 125 MB/s)
Multi-AZ: Enabled (automatic failover)
Backups: 7-day retention, PITR enabled
Monitoring: CloudWatch enhanced monitoring
```

### High Availability

**Single Region**:
```
StreamHouse Agents (us-east-1a, us-east-1b, us-east-1c)
          ↓
  PostgreSQL Multi-AZ (primary + standby)
          ↓
  Automatic failover < 60s
```

**Multi Region** (Phase 4+):
```
Agents (us-east-1) → PostgreSQL (us-east-1)
                          ↓
                  Logical replication
                          ↓
Agents (eu-west-1) → PostgreSQL (eu-west-1) [read replica]
```

### Monitoring

**Key Metrics**:

1. **Connection pool saturation**:
   - Alert if `active_connections > 18` (90% of pool)
   - Solution: Increase max_connections or add more agents

2. **Query latency**:
   - Alert if P99 > 100ms
   - Solution: Check indexes, consider read replicas

3. **Lease expiration rate**:
   - Alert if > 5% of leases expire without renewal
   - Indicates agent health issues

4. **Database size**:
   - Expect ~100KB per 1000 topics
   - Expect ~50KB per 1000 segments

## Migration from SQLite

### Strategy

StreamHouse can't run with both backends simultaneously. Choose one:

**Option 1: Fresh Start (Recommended)**

1. Start new StreamHouse cluster with PostgreSQL
2. Recreate topics with same configuration
3. Consumers resume from latest (or specified offset)

**Option 2: Data Migration (Advanced)**

1. Export SQLite metadata:
   ```bash
   sqlite3 data/metadata.db .dump > metadata.sql
   ```

2. Transform SQL (SQLite → PostgreSQL syntax):
   - `INTEGER PRIMARY KEY` → `SERIAL PRIMARY KEY`
   - `TEXT` → `TEXT` (same)
   - JSON strings → `JSONB`

3. Import to PostgreSQL:
   ```bash
   psql $DATABASE_URL < metadata_pg.sql
   ```

4. Update StreamHouse config to use PostgreSQL

**Note**: S3 segment files don't need migration (just metadata pointers).

## Troubleshooting

### Connection Failures

```
Error: error connecting to server: Connection refused
```

**Solution**:
1. Check PostgreSQL is running: `docker ps | grep postgres`
2. Verify connection string: `psql $DATABASE_URL`
3. Check firewall rules (port 5432)

### Migration Failures

```
Error: migration 001_initial_schema.sql failed
```

**Solution**:
1. Check PostgreSQL version: `SELECT version();` (need 14+)
2. Verify permissions: `GRANT ALL ON DATABASE streamhouse TO streamhouse;`
3. Check logs: `docker logs streamhouse-postgres`

### Slow Queries

```
find_segment_for_offset taking > 100ms
```

**Solution**:
1. Verify indexes exist:
   ```sql
   SELECT * FROM pg_indexes WHERE tablename = 'segments';
   ```

2. Check query plan:
   ```sql
   EXPLAIN ANALYZE
   SELECT * FROM segments
   WHERE topic = 'events' AND partition_id = 0
     AND base_offset <= 5000 AND end_offset >= 5000;
   ```

3. Consider VACUUM ANALYZE if table bloat is high

### Pool Exhaustion

```
Error: timeout acquiring connection from pool
```

**Solution**:
1. Increase pool size: `max_connections(50)`
2. Check for connection leaks (unclosed transactions)
3. Monitor `pg_stat_activity` for long-running queries

## Schema Reference

### Topics Table

```sql
CREATE TABLE topics (
    name TEXT PRIMARY KEY,              -- Topic name (unique)
    partition_count INTEGER NOT NULL,   -- Number of partitions
    retention_ms BIGINT,                -- Retention policy (nullable)
    created_at BIGINT NOT NULL,         -- Creation timestamp (ms)
    updated_at BIGINT NOT NULL,         -- Last update timestamp (ms)
    config JSONB NOT NULL DEFAULT '{}'  -- Topic configuration
);
```

### Partitions Table

```sql
CREATE TABLE partitions (
    topic TEXT NOT NULL,                -- Foreign key to topics(name)
    partition_id INTEGER NOT NULL,      -- Partition number (0-indexed)
    high_watermark BIGINT NOT NULL,     -- Latest offset written
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (topic) REFERENCES topics(name) ON DELETE CASCADE
);
```

### Segments Table

```sql
CREATE TABLE segments (
    id TEXT PRIMARY KEY,                -- Segment UUID
    topic TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    base_offset BIGINT NOT NULL,        -- First offset in segment
    end_offset BIGINT NOT NULL,         -- Last offset in segment
    record_count INTEGER NOT NULL,      -- Number of records
    size_bytes BIGINT NOT NULL,         -- Segment file size
    s3_bucket TEXT NOT NULL,            -- S3 bucket name
    s3_key TEXT NOT NULL,               -- S3 object key
    created_at BIGINT NOT NULL,
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id)
);

-- Index for offset range queries
CREATE INDEX idx_segments_offset_range
ON segments(topic, partition_id, base_offset, end_offset);
```

## JSONB Configuration Examples

### Topic Config

```rust
use std::collections::HashMap;

let mut config = HashMap::new();
config.insert("compression".to_string(), "lz4".to_string());
config.insert("retention.policy".to_string(), "delete".to_string());

store.create_topic(TopicConfig {
    name: "events".to_string(),
    partition_count: 16,
    retention_ms: Some(86400000), // 1 day
    config,
}).await?;
```

Stored in PostgreSQL as:
```json
{"compression": "lz4", "retention.policy": "delete"}
```

### Querying JSONB (Future Use)

```sql
-- Find topics with LZ4 compression
SELECT name FROM topics WHERE config->>'compression' = 'lz4';

-- Create index on JSONB field
CREATE INDEX idx_topics_compression ON topics((config->>'compression'));
```

## Related Documentation

- [Phase 3.2 Completion Report](phases/PHASE_3.2_COMPLETE.md)
- [Migration 001: Initial Schema](../crates/streamhouse-metadata/migrations-postgres/001_initial_schema.sql)
- [Migration 002: Agent Coordination](../crates/streamhouse-metadata/migrations-postgres/002_agent_coordination.sql)
- [PostgreSQL Implementation](../crates/streamhouse-metadata/src/postgres.rs)

## FAQ

**Q: Can I use PostgreSQL for development?**
A: Yes! Just run `docker-compose up postgres` and set `DATABASE_URL`. SQLite is recommended for quick iteration, but PostgreSQL works fine.

**Q: Do I need to run migrations manually?**
A: No - migrations run automatically when `PostgresMetadataStore::new()` is called.

**Q: Can I switch from SQLite to PostgreSQL without data loss?**
A: Not automatically. You can migrate metadata (see "Migration from SQLite" section), but S3 segments are compatible (no migration needed).

**Q: How much does PostgreSQL cost?**
A: AWS RDS db.t3.medium (~$60/month) handles 1000s of topics. For production, db.r6g.xlarge (~$400/month) supports high-throughput workloads.

**Q: What about Serverless/Aurora?**
A: Aurora Serverless v2 works well. Auto-scaling helps with bursty metadata workloads. Costs scale with capacity units.

**Q: Is PostgreSQL required for production?**
A: Not strictly - SQLite works for single-node deployments. Use PostgreSQL when you need multi-node writes or HA.

---

**Next Steps**: See [Phase 3.3](phases/PHASE_3.3_PLAN.md) for Metadata Caching Layer (coming soon).
