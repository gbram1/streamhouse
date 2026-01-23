# StreamHouse Scripts

Clean, production-ready scripts for demonstrating and inspecting StreamHouse.

---

## Main Demo Script

### `demo.sh` - Complete Pipeline Demo

**What it does:** Runs the complete end-to-end StreamHouse pipeline using REAL APIs.

```bash
./scripts/demo.sh              # Full demo (wipes data, writes fresh, shows results)
./scripts/demo.sh --skip-wipe  # Keep existing data, just show it
```

**Pipeline flow:**
1. **Wipes data** (PostgreSQL + MinIO) for clean slate
2. **Writes data** through `PartitionWriter` API (not fake inserts!)
   - 3 topics: orders, user-events, metrics
   - 1,650 total records
   - Real LZ4 compression
   - Real S3 uploads to MinIO
3. **Shows metadata** in PostgreSQL (topics, partitions, segments)
4. **Shows segments** in MinIO object storage
5. **Reads data back** through `PartitionReader` API with Phase 3.4 index

**Output:** Complete demonstration of write → store → read pipeline.

---

## Inspection Scripts

### `interactive_query.sh` - Find Segment for Offset

**What it does:** Shows which segment contains a specific offset and simulates Phase 3.4 BTreeMap lookup.

```bash
./scripts/interactive_query.sh <topic> <partition> <offset>

# Examples
./scripts/interactive_query.sh orders 0 50
./scripts/interactive_query.sh user-events 0 150
./scripts/interactive_query.sh metrics 2 200
```

**Output:**
- Segment metadata (offset range, size, S3 location)
- BTreeMap lookup simulation
- MinIO file verification
- Complete read flow explanation

---

### `inspect_segment.sh` - View Binary Segment File

**What it does:** Downloads and shows the raw binary format of a segment file.

```bash
./scripts/inspect_segment.sh <topic> <partition> [base_offset]

# Examples
./scripts/inspect_segment.sh orders 0 0
./scripts/inspect_segment.sh user-events 0
```

**Output:**
- File information (size, date, ETag)
- Hex dump of header
- Magic bytes verification (STRM)
- Compression type
- Binary structure breakdown
- Readable strings (if uncompressed)

---

### `inspect_minio.sh` - Browse MinIO Storage

**What it does:** Shows MinIO bucket contents and provides useful commands.

```bash
./scripts/inspect_minio.sh
```

**Output:**
- Available buckets
- Objects in streamhouse bucket
- Directory tree
- Disk usage
- Useful mc commands for inspection

---

### `show_schema.sh` - Display PostgreSQL Schema

**What it does:** Shows the complete database schema with all tables and indexes.

```bash
./scripts/show_schema.sh
```

**Output:**
- Topics table structure
- Partitions table structure
- Segments table structure
- Consumer groups and offsets tables
- Agents and partition leases tables (Phase 4)
- All indexes

---

## Infrastructure Setup

### `dev-env.sh` - Start Development Environment

**What it does:** Starts PostgreSQL and MinIO in Docker.

```bash
./scripts/dev-env.sh
```

**What it creates:**
- PostgreSQL on port 5432
  - Database: streamhouse_metadata
  - User: streamhouse/streamhouse
- MinIO on ports 9000 (API) and 9001 (Web UI)
  - Credentials: minioadmin/minioadmin
  - Bucket: streamhouse

---

## Reading Data with Rust

### Read Records from Topics

Use the `simple_reader` example to read actual records:

```bash
cargo run --package streamhouse-storage --example simple_reader -- <topic> <partition> <offset> <count>

# Examples
cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 10
cargo run --package streamhouse-storage --example simple_reader -- user-events 0 50 20
cargo run --package streamhouse-storage --example simple_reader -- metrics 2 100 5
```

**What it does:**
- Uses actual `PartitionReader` API
- Phase 3.4 BTreeMap index finds segment (< 1µs)
- Loads from cache or downloads from MinIO
- Decompresses LZ4 blocks
- Decodes binary record format
- Pretty-prints JSON values

---

## Quick Start

```bash
# 1. Start infrastructure
./scripts/dev-env.sh

# 2. Run complete demo
./scripts/demo.sh

# 3. Read some records
cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 5

# 4. Query specific offset
./scripts/interactive_query.sh orders 0 50

# 5. Inspect binary format
./scripts/inspect_segment.sh orders 0 0

# 6. Browse MinIO web UI
open http://localhost:9001
```

---

## What Makes This "Real"

**NOT fake data:**
- ❌ No direct SQL `INSERT INTO segments ...`
- ❌ No dummy files created with `dd if=/dev/urandom`
- ❌ No mock data

**REAL pipeline:**
- ✅ `PartitionWriter.append(key, value, timestamp)` - actual API
- ✅ `SegmentWriter.finish()` - real LZ4 compression and indexing
- ✅ Upload to MinIO via S3 API - real network calls
- ✅ Metadata registration - real database transactions
- ✅ `PartitionReader.read(offset, count)` - actual reads with Phase 3.4 index

**Binary format:**
- Proper StreamHouse segment format
- Magic bytes: `STRM`
- LZ4 compression
- Varint encoding with delta coding
- Offset index for O(log n) seeks
- CRC32 checksums

---

## File Structure

```
scripts/
├── demo.sh                  # Main demo (run this first)
├── interactive_query.sh     # Find segment for offset
├── inspect_segment.sh       # View binary format
├── inspect_minio.sh         # Browse MinIO storage
├── show_schema.sh           # Display PostgreSQL schema
├── dev-env.sh              # Start infrastructure
└── README.md               # This file
```

---

## Troubleshooting

**Q: Demo fails with "connection refused"**
- Run `./scripts/dev-env.sh` first to start PostgreSQL and MinIO

**Q: No data shown in PostgreSQL/MinIO**
- Run `./scripts/demo.sh` to generate data
- Check containers: `docker ps | grep streamhouse`

**Q: Can't read records**
- Make sure you ran `demo.sh` first to write data
- Check topic/partition exist: `docker exec streamhouse-postgres psql -U streamhouse -d streamhouse_metadata -c "SELECT * FROM topics;"`

**Q: Want to see more records**
- Use `simple_reader` with larger count: `cargo run --package streamhouse-storage --example simple_reader -- orders 0 0 100`

**Q: Where is the data actually stored?**
- Metadata: PostgreSQL database in Docker container
- Segments: MinIO object storage in Docker container
- Cache: `/tmp/streamhouse-*` directories on host

---

## Documentation

- [How to Inspect Data](../docs/HOW_TO_INSPECT_DATA.md) - Detailed guide
- [Phase 3.4 Proof](../docs/phases/PHASE_3_4_FINAL_PROOF.md) - Complete proof with 560K records
- [Data Model](../docs/DATA_MODEL.md) - Schema and format details

---

**All scripts use the REAL StreamHouse APIs - no shortcuts, no fake data!**
