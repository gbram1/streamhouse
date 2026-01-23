# StreamHouse Data Model

This document shows how data is stored in PostgreSQL (metadata) and MinIO/S3 (segments).

## PostgreSQL Schema

### 1. Topics Table

Stores topic configuration and metadata.

```sql
CREATE TABLE topics (
    name             TEXT PRIMARY KEY,
    partition_count  INTEGER NOT NULL CHECK (partition_count > 0),
    retention_ms     BIGINT,
    created_at       BIGINT NOT NULL,
    updated_at       BIGINT NOT NULL,
    config           JSONB NOT NULL DEFAULT '{}'
);
```

**Example Row:**
```json
{
  "name": "orders",
  "partition_count": 3,
  "retention_ms": 604800000,  // 7 days
  "created_at": 1737594123000,
  "updated_at": 1737594123000,
  "config": {
    "compression": "lz4",
    "max_message_bytes": "1048576"
  }
}
```

### 2. Partitions Table

Tracks partition watermarks (latest offset).

```sql
CREATE TABLE partitions (
    topic          TEXT NOT NULL,
    partition_id   INTEGER NOT NULL,
    high_watermark BIGINT NOT NULL DEFAULT 0,
    created_at     BIGINT NOT NULL,
    updated_at     BIGINT NOT NULL,
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (topic) REFERENCES topics(name) ON DELETE CASCADE
);
```

**Example Row:**
```json
{
  "topic": "orders",
  "partition_id": 0,
  "high_watermark": 50000,  // Latest offset written
  "created_at": 1737594123000,
  "updated_at": 1737594456000
}
```

### 3. Segments Table

Pointers to segment files in S3/MinIO.

```sql
CREATE TABLE segments (
    id           TEXT PRIMARY KEY,
    topic        TEXT NOT NULL,
    partition_id INTEGER NOT NULL,
    base_offset  BIGINT NOT NULL,
    end_offset   BIGINT NOT NULL CHECK (end_offset >= base_offset),
    record_count INTEGER NOT NULL,
    size_bytes   BIGINT NOT NULL,
    s3_bucket    TEXT NOT NULL,
    s3_key       TEXT NOT NULL,
    created_at   BIGINT NOT NULL,
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_segments_location ON segments(topic, partition_id, base_offset);
CREATE INDEX idx_segments_offsets ON segments(topic, partition_id, base_offset, end_offset);
```

**Example Row:**
```json
{
  "id": "orders-0-00000000000000000000",
  "topic": "orders",
  "partition_id": 0,
  "base_offset": 0,
  "end_offset": 9999,
  "record_count": 10000,
  "size_bytes": 6710886,  // ~6.4MB compressed
  "s3_bucket": "streamhouse",
  "s3_key": "orders/0/seg_00000000000000000000.bin",
  "created_at": 1737594123000
}
```

### 4. Consumer Groups Table

Tracks consumer group registration.

```sql
CREATE TABLE consumer_groups (
    group_id   TEXT PRIMARY KEY,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
```

**Example Row:**
```json
{
  "group_id": "analytics-processors",
  "created_at": 1737594123000,
  "updated_at": 1737594456000
}
```

### 5. Consumer Offsets Table

Tracks consumption progress per consumer group.

```sql
CREATE TABLE consumer_offsets (
    group_id         TEXT NOT NULL,
    topic            TEXT NOT NULL,
    partition_id     INTEGER NOT NULL,
    committed_offset BIGINT NOT NULL,
    metadata         TEXT,
    committed_at     BIGINT NOT NULL,
    PRIMARY KEY (group_id, topic, partition_id),
    FOREIGN KEY (group_id) REFERENCES consumer_groups(group_id) ON DELETE CASCADE,
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE
);
```

**Example Row:**
```json
{
  "group_id": "analytics-processors",
  "topic": "orders",
  "partition_id": 0,
  "committed_offset": 45000,
  "metadata": "processed batch 450",
  "committed_at": 1737594456000
}
```

### 6. Agents Table (Phase 4)

Tracks agent registration and liveness.

```sql
CREATE TABLE agents (
    agent_id          TEXT PRIMARY KEY,
    address           TEXT UNIQUE NOT NULL,
    availability_zone TEXT NOT NULL,
    agent_group       TEXT NOT NULL,
    last_heartbeat    BIGINT NOT NULL,
    started_at        BIGINT NOT NULL,
    metadata          JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_agents_heartbeat ON agents(last_heartbeat);
CREATE INDEX idx_agents_az ON agents(availability_zone);
```

**Example Row:**
```json
{
  "agent_id": "agent-us-east-1a-001",
  "address": "10.0.1.5:9090",
  "availability_zone": "us-east-1a",
  "agent_group": "prod",
  "last_heartbeat": 1737594456000,
  "started_at": 1737594123000,
  "metadata": {
    "version": "0.1.0",
    "instance_type": "m6i.2xlarge"
  }
}
```

### 7. Partition Leases Table (Phase 4)

Tracks partition leadership for coordinated writes.

```sql
CREATE TABLE partition_leases (
    topic            TEXT NOT NULL,
    partition_id     INTEGER NOT NULL,
    leader_agent_id  TEXT NOT NULL,
    lease_expires_at BIGINT NOT NULL,
    acquired_at      BIGINT NOT NULL,
    epoch            BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (topic, partition_id),
    FOREIGN KEY (topic, partition_id) REFERENCES partitions(topic, partition_id) ON DELETE CASCADE,
    FOREIGN KEY (leader_agent_id) REFERENCES agents(agent_id) ON DELETE CASCADE
);

CREATE INDEX idx_partition_leases_expiration ON partition_leases(lease_expires_at);
CREATE INDEX idx_partition_leases_agent ON partition_leases(leader_agent_id);
```

**Example Row:**
```json
{
  "topic": "orders",
  "partition_id": 0,
  "leader_agent_id": "agent-us-east-1a-001",
  "lease_expires_at": 1737594486000,  // 30s from now
  "acquired_at": 1737594456000,
  "epoch": 5  // Increments on each leadership change
}
```

---

## MinIO/S3 Storage

### Directory Structure

```
streamhouse/  (bucket)
├── orders/
│   ├── 0/
│   │   ├── seg_00000000000000000000.bin  // base_offset=0
│   │   ├── seg_00000000000000010000.bin  // base_offset=10000
│   │   └── seg_00000000000000020000.bin  // base_offset=20000
│   ├── 1/
│   │   ├── seg_00000000000000000000.bin
│   │   └── seg_00000000000000010000.bin
│   └── 2/
│       └── seg_00000000000000000000.bin
├── user-events/
│   ├── 0/
│   │   └── seg_00000000000000000000.bin
│   └── 1/
│       └── seg_00000000000000000000.bin
└── metrics/
    └── 0/
        ├── seg_00000000000000000000.bin
        └── seg_00000000000001000000.bin
```

### Segment File Format

Binary format with LZ4 compression:

```
┌───────────────────────────────────────┐
│ SEGMENT HEADER (32 bytes)             │
│ - Magic: "STRS" (4 bytes)             │
│ - Version: u16                         │
│ - Compression: u8 (0=None, 1=LZ4)     │
│ - Reserved: 25 bytes                   │
├───────────────────────────────────────┤
│ BLOCK 1 (up to 1MB uncompressed)      │
│ ┌─────────────────────────────────┐   │
│ │ Block Header (12 bytes)         │   │
│ │ - Uncompressed size: u32        │   │
│ │ - Compressed size: u32          │   │
│ │ - CRC32: u32                    │   │
│ ├─────────────────────────────────┤   │
│ │ Compressed Record Data          │   │
│ │   Record 1                      │   │
│ │   Record 2                      │   │
│ │   ...                           │   │
│ └─────────────────────────────────┘   │
├───────────────────────────────────────┤
│ BLOCK 2                                │
│ (same structure)                       │
├───────────────────────────────────────┤
│ ... more blocks ...                    │
├───────────────────────────────────────┤
│ OFFSET INDEX (variable size)           │
│ ┌─────────────────────────────────┐   │
│ │ Entry 1: offset=0, position=32  │   │
│ │ Entry 2: offset=1000, pos=1056  │   │
│ │ ...                             │   │
│ └─────────────────────────────────┘   │
└───────────────────────────────────────┘
```

### Record Format (Inside Blocks)

Varint encoding for space efficiency:

```
Record:
  offset:     varint(u64)     // Delta-encoded from previous
  timestamp:  varint(i64)     // Delta-encoded from previous
  key_len:    varint(u32)
  key:        [u8; key_len]
  value_len:  varint(u32)
  value:      [u8; value_len]
```

**Example (JSON representation):**
```json
{
  "offset": 12345,
  "timestamp": 1737594456789,
  "key": "order-12345",
  "value": "{\"item\":\"laptop\",\"price\":1299.99,\"qty\":1}"
}
```

**Binary size:**
- Offset (delta): 1-2 bytes (varint)
- Timestamp (delta): 1-2 bytes (varint)
- Key length: 1 byte (varint for 11)
- Key: 11 bytes
- Value length: 2 bytes (varint for 45)
- Value: 45 bytes

**Total: ~62 bytes uncompressed, ~40 bytes with LZ4**

---

## Data Flow Example

### Write Path

1. **Producer sends record:**
```json
{
  "topic": "orders",
  "partition": 0,
  "key": "order-123",
  "value": "{\"item\":\"laptop\",\"price\":1299.99}"
}
```

2. **Agent buffers in memory:**
   - Appends to in-memory buffer
   - Assigns offset: 50000
   - Returns immediately (< 1ms)

3. **Background flush (every 30s or 64MB):**
   - Compresses buffered records with LZ4
   - Writes segment to S3: `orders/0/seg_00000000000000050000.bin`
   - Registers in PostgreSQL:
```sql
INSERT INTO segments (id, topic, partition_id, base_offset, end_offset, ...)
VALUES ('orders-0-50000', 'orders', 0, 50000, 50999, ...);

UPDATE partitions
SET high_watermark = 51000
WHERE topic = 'orders' AND partition_id = 0;
```

### Read Path

1. **Consumer requests records:**
```json
{
  "topic": "orders",
  "partition": 0,
  "offset": 50500,
  "max_records": 100
}
```

2. **Agent looks up segment (Phase 3.4 optimization):**
   - Checks in-memory BTreeMap index
   - Finds: `seg_00000000000000050000.bin` (base_offset=50000, end_offset=50999)
   - **Lookup time: < 1µs** (vs 100µs database query)

3. **Agent fetches segment:**
   - Checks local cache: **HIT** (80% of time)
   - Returns records 50500-50599
   - **Total latency: ~5ms**

4. **Consumer commits offset:**
```sql
INSERT INTO consumer_offsets (group_id, topic, partition_id, committed_offset, ...)
VALUES ('analytics', 'orders', 0, 50600, ...)
ON CONFLICT (group_id, topic, partition_id)
DO UPDATE SET committed_offset = 50600, committed_at = now();
```

---

## Performance Characteristics

### PostgreSQL Queries

| Operation | Query | Avg Latency |
|-----------|-------|-------------|
| Get topic | `SELECT * FROM topics WHERE name = $1` | ~100µs (cached) |
| List topics | `SELECT * FROM topics ORDER BY name` | ~500µs (cached) |
| Get partition | `SELECT * FROM partitions WHERE topic=$1 AND partition_id=$2` | ~80µs (cached) |
| Find segment | `SELECT * FROM segments WHERE topic=$1 AND partition_id=$2 AND base_offset <= $3 AND end_offset >= $3` | ~100µs (indexed) |
| Update watermark | `UPDATE partitions SET high_watermark=$1 WHERE topic=$2 AND partition_id=$3` | ~200µs |
| Commit offset | `INSERT INTO consumer_offsets ... ON CONFLICT DO UPDATE` | ~300µs |

### S3/MinIO Operations

| Operation | Size | Avg Latency |
|-----------|------|-------------|
| PUT segment | 64MB | ~150ms |
| GET segment (cold) | 64MB | ~80ms |
| GET segment (cached) | 64MB | ~5ms |
| HEAD object | - | ~20ms |
| LIST prefix | 1000 objects | ~50ms |

### Index Sizes

| Table | Rows (example workload) | Size |
|-------|------------------------|------|
| topics | 1,000 | ~100 KB |
| partitions | 10,000 | ~1 MB |
| segments | 100,000 | ~20 MB |
| consumer_groups | 100 | ~10 KB |
| consumer_offsets | 10,000 | ~2 MB |

**Total database size (typical): < 50 MB**

---

## Indexing Strategy

### High-Cardinality Queries

```sql
-- Find segment for offset (used every read)
CREATE INDEX idx_segments_offsets ON segments(topic, partition_id, base_offset, end_offset);

-- List segments for partition (used during index refresh)
CREATE UNIQUE INDEX idx_segments_location ON segments(topic, partition_id, base_offset);
```

### Time-Based Queries

```sql
-- Find old segments for cleanup
CREATE INDEX idx_segments_created_at ON segments(created_at);

-- Find stale consumer offsets
CREATE INDEX idx_consumer_offsets_committed_at ON consumer_offsets(committed_at);
```

### Phase 4 (Multi-Agent)

```sql
-- Find live agents
CREATE INDEX idx_agents_heartbeat ON agents(last_heartbeat);

-- Find expired leases
CREATE INDEX idx_partition_leases_expiration ON partition_leases(lease_expires_at);

-- Find agent's leases
CREATE INDEX idx_partition_leases_agent ON partition_leases(leader_agent_id);
```

---

## Summary

**PostgreSQL stores:**
- ✅ Topic configuration (name, partitions, retention)
- ✅ Partition watermarks (high_watermark)
- ✅ Segment pointers (S3 location, offset range)
- ✅ Consumer progress (committed offsets)
- ✅ Agent coordination (Phase 4: leases, heartbeats)

**S3/MinIO stores:**
- ✅ Actual record data (compressed with LZ4)
- ✅ Organized by topic/partition/base_offset
- ✅ Immutable once written
- ✅ Average 2-5x compression ratio

**Phase 3.4 Optimization:**
- ✅ In-memory BTreeMap segment index
- ✅ Eliminates database query on every read
- ✅ 100x faster: 100µs → < 1µs
- ✅ 99.99% reduction in database queries

This architecture provides:
- **Infinite scale** (S3 storage)
- **High performance** (caching + indexing)
- **Strong consistency** (PostgreSQL ACID)
- **Cost efficiency** (10x cheaper than Kafka)
