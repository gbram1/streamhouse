# Deep Dive Part 2: S3 Segment Format Design

## Design Goals

1. **Append-only:** Write once, read many times
2. **S3-friendly:** Large sequential writes, minimize PUTs
3. **Efficient reads:** Index for quick offset lookups
4. **Compression:** Reduce storage costs (80%+ savings)
5. **Simple:** Easy to implement and debug

---

## Segment File Layout

```
┌─────────────────────────────────────────────────────────────┐
│                    SEGMENT FILE                             │
├─────────────────────────────────────────────────────────────┤
│  HEADER (64 bytes)                                          │
│  ├── Magic (4 bytes): "STRM"                               │
│  ├── Version (2 bytes): 1                                  │
│  ├── Flags (2 bytes): compression type                     │
│  ├── Topic Hash (8 bytes)                                  │
│  ├── Partition (4 bytes)                                   │
│  ├── Base Offset (8 bytes): first offset                   │
│  ├── Max Offset (8 bytes): last offset                     │
│  ├── Record Count (4 bytes)                                │
│  ├── Created At (8 bytes): timestamp                       │
│  ├── Min Timestamp (8 bytes)                               │
│  ├── Max Timestamp (8 bytes)                               │
│  └── Reserved (0 bytes)                                    │
├─────────────────────────────────────────────────────────────┤
│  RECORD BLOCKS                                              │
│  ├── Block 1 (target: 1MB uncompressed)                    │
│  │   ├── Block Header (16 bytes)                           │
│  │   │   ├── Uncompressed Size (4 bytes)                   │
│  │   │   ├── Compressed Size (4 bytes)                     │
│  │   │   ├── Record Count (4 bytes)                        │
│  │   │   └── CRC32 (4 bytes)                               │
│  │   └── Compressed Records                                │
│  ├── Block 2...                                            │
│  └── Block N                                               │
├─────────────────────────────────────────────────────────────┤
│  INDEX                                                      │
│  ├── Entry Count (4 bytes)                                 │
│  └── Entries: [Offset (8) + Position (8)] × N             │
├─────────────────────────────────────────────────────────────┤
│  FOOTER (32 bytes)                                          │
│  ├── Index Offset (8 bytes)                                │
│  ├── Index Size (4 bytes)                                  │
│  ├── File CRC32 (4 bytes)                                  │
│  ├── Reserved (12 bytes)                                   │
│  └── Magic (4 bytes): "MRTS"                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Record Format (within blocks)

Use varints and deltas for compression:

```
Record =>
  ├── Offset Delta (varint): offset - block_base_offset
  ├── Timestamp Delta (varint): timestamp - block_base_timestamp  
  ├── Key Length (varint): -1 for null
  ├── Key (bytes): if length >= 0
  ├── Value Length (varint)
  └── Value (bytes)
```

**Why deltas?** Sequential offsets become 1, 1, 1... instead of 1000000, 1000001, 1000002...

---

## Rust Implementation

### Types

```rust
const MAGIC: &[u8; 4] = b"STRM";
const MAGIC_FOOTER: &[u8; 4] = b"MRTS";
const VERSION: u16 = 1;
const HEADER_SIZE: usize = 64;
const FOOTER_SIZE: usize = 32;
const BLOCK_HEADER_SIZE: usize = 16;

#[derive(Debug, Clone)]
pub struct Record {
    pub offset: u64,
    pub timestamp: u64,
    pub key: Option<Bytes>,
    pub value: Bytes,
}

#[derive(Debug, Clone, Copy)]
pub enum Compression {
    None = 0,
    Lz4 = 1,
    Zstd = 2,
}

pub struct SegmentWriter {
    topic: String,
    partition: u32,
    base_offset: u64,
    next_offset: u64,
    
    // Current block being built
    current_block: Vec<Record>,
    current_block_size: usize,
    
    // Completed blocks
    blocks: Vec<Bytes>,
    index_entries: Vec<IndexEntry>,
    
    // Config
    block_size_target: usize,  // 1MB default
    compression: Compression,
}

struct IndexEntry {
    offset: u64,
    position: u64,
}
```

### Writer

```rust
impl SegmentWriter {
    pub fn new(topic: String, partition: u32, base_offset: u64) -> Self {
        Self {
            topic,
            partition,
            base_offset,
            next_offset: base_offset,
            current_block: Vec::new(),
            current_block_size: 0,
            blocks: Vec::new(),
            index_entries: Vec::new(),
            block_size_target: 1024 * 1024, // 1MB
            compression: Compression::Lz4,
        }
    }
    
    pub fn append(&mut self, key: Option<Bytes>, value: Bytes, timestamp: u64) -> u64 {
        let offset = self.next_offset;
        self.next_offset += 1;
        
        let record = Record { offset, timestamp, key, value };
        let record_size = self.estimate_size(&record);
        
        // Flush block if full
        if self.current_block_size + record_size > self.block_size_target 
           && !self.current_block.is_empty() 
        {
            self.flush_block();
        }
        
        self.current_block_size += record_size;
        self.current_block.push(record);
        
        offset
    }
    
    fn flush_block(&mut self) {
        if self.current_block.is_empty() {
            return;
        }
        
        let block_base_offset = self.current_block[0].offset;
        let block_position = HEADER_SIZE as u64 
            + self.blocks.iter().map(|b| b.len()).sum::<usize>() as u64;
        
        // Add index entry
        self.index_entries.push(IndexEntry {
            offset: block_base_offset,
            position: block_position,
        });
        
        // Serialize records with deltas
        let mut data = BytesMut::new();
        let base_ts = self.current_block[0].timestamp;
        
        for record in &self.current_block {
            put_varint(&mut data, (record.offset - block_base_offset) as i64);
            put_varint(&mut data, (record.timestamp as i64) - (base_ts as i64));
            
            match &record.key {
                Some(k) => {
                    put_varint(&mut data, k.len() as i64);
                    data.extend_from_slice(k);
                }
                None => put_varint(&mut data, -1),
            }
            
            put_varint(&mut data, record.value.len() as i64);
            data.extend_from_slice(&record.value);
        }
        
        // Compress
        let uncompressed_size = data.len();
        let compressed = match self.compression {
            Compression::None => data.freeze(),
            Compression::Lz4 => Bytes::from(lz4_flex::compress_prepend_size(&data)),
            Compression::Zstd => Bytes::from(zstd::encode_all(&data[..], 3).unwrap()),
        };
        
        // Build block with header
        let mut block = BytesMut::new();
        block.put_u32(uncompressed_size as u32);
        block.put_u32(compressed.len() as u32);
        block.put_u32(self.current_block.len() as u32);
        block.put_u32(crc32fast::hash(&compressed));
        block.extend_from_slice(&compressed);
        
        self.blocks.push(block.freeze());
        self.current_block.clear();
        self.current_block_size = 0;
    }
    
    pub fn finish(mut self) -> Bytes {
        self.flush_block();
        
        let mut output = BytesMut::new();
        
        // === HEADER ===
        output.put_slice(MAGIC);
        output.put_u16(VERSION);
        output.put_u16(self.compression as u16);
        output.put_u64(hash(&self.topic));
        output.put_u32(self.partition);
        output.put_u64(self.base_offset);
        output.put_u64(self.next_offset.saturating_sub(1));
        output.put_u32((self.next_offset - self.base_offset) as u32);
        output.put_u64(now_ms());
        output.put_u64(0); // min_timestamp (TODO)
        output.put_u64(0); // max_timestamp (TODO)
        while output.len() < HEADER_SIZE {
            output.put_u8(0);
        }
        
        // === BLOCKS ===
        for block in &self.blocks {
            output.extend_from_slice(block);
        }
        
        // === INDEX ===
        let index_offset = output.len() as u64;
        output.put_u32(self.index_entries.len() as u32);
        for entry in &self.index_entries {
            output.put_u64(entry.offset);
            output.put_u64(entry.position);
        }
        let index_size = output.len() - index_offset as usize;
        
        // === FOOTER ===
        output.put_u64(index_offset);
        output.put_u32(index_size as u32);
        output.put_u32(crc32fast::hash(&output));
        output.put_slice(&[0u8; 12]);
        output.put_slice(MAGIC_FOOTER);
        
        output.freeze()
    }
    
    fn estimate_size(&self, record: &Record) -> usize {
        10 + record.key.as_ref().map(|k| k.len()).unwrap_or(0) + record.value.len()
    }
}
```

### Reader

```rust
pub struct SegmentReader {
    data: Bytes,
    index: Vec<IndexEntry>,
    compression: Compression,
    base_offset: u64,
    max_offset: u64,
}

impl SegmentReader {
    pub fn open(data: Bytes) -> Result<Self> {
        // Validate magic
        if &data[0..4] != MAGIC {
            return Err(Error::InvalidMagic);
        }
        
        // Parse header
        let compression = match data[6..8].get_u16() {
            0 => Compression::None,
            1 => Compression::Lz4,
            2 => Compression::Zstd,
            _ => return Err(Error::InvalidCompression),
        };
        
        let base_offset = (&data[18..26]).get_u64();
        let max_offset = (&data[26..34]).get_u64();
        
        // Parse footer to find index
        let footer_start = data.len() - FOOTER_SIZE;
        let index_offset = (&data[footer_start..]).get_u64() as usize;
        
        // Parse index
        let mut cursor = &data[index_offset..];
        let entry_count = cursor.get_u32() as usize;
        let mut index = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            index.push(IndexEntry {
                offset: cursor.get_u64(),
                position: cursor.get_u64(),
            });
        }
        
        Ok(Self { data, index, compression, base_offset, max_offset })
    }
    
    pub fn read_from(&self, start_offset: u64, limit: usize) -> Result<Vec<Record>> {
        // Binary search for block containing offset
        let block_idx = self.index
            .binary_search_by_key(&start_offset, |e| e.offset)
            .unwrap_or_else(|i| i.saturating_sub(1));
        
        let mut records = Vec::new();
        
        for idx in block_idx..self.index.len() {
            if records.len() >= limit {
                break;
            }
            
            let block_records = self.read_block(idx)?;
            for record in block_records {
                if record.offset >= start_offset && records.len() < limit {
                    records.push(record);
                }
            }
        }
        
        Ok(records)
    }
    
    fn read_block(&self, idx: usize) -> Result<Vec<Record>> {
        let pos = self.index[idx].position as usize;
        let base_offset = self.index[idx].offset;
        
        // Parse block header
        let mut cursor = &self.data[pos..];
        let uncompressed_size = cursor.get_u32() as usize;
        let compressed_size = cursor.get_u32() as usize;
        let record_count = cursor.get_u32() as usize;
        let expected_crc = cursor.get_u32();
        
        let compressed = &self.data[pos + BLOCK_HEADER_SIZE..pos + BLOCK_HEADER_SIZE + compressed_size];
        
        // Verify CRC
        if crc32fast::hash(compressed) != expected_crc {
            return Err(Error::CrcMismatch);
        }
        
        // Decompress
        let decompressed = match self.compression {
            Compression::None => compressed.to_vec(),
            Compression::Lz4 => lz4_flex::decompress_size_prepended(compressed)?,
            Compression::Zstd => zstd::decode_all(compressed)?,
        };
        
        // Parse records
        let mut cursor = &decompressed[..];
        let mut records = Vec::with_capacity(record_count);
        let mut base_timestamp = 0i64;
        
        for i in 0..record_count {
            let offset_delta = get_varint(&mut cursor);
            let ts_delta = get_varint(&mut cursor);
            
            let offset = base_offset + offset_delta as u64;
            let timestamp = if i == 0 {
                base_timestamp = ts_delta;
                ts_delta as u64
            } else {
                (base_timestamp + ts_delta) as u64
            };
            
            let key_len = get_varint(&mut cursor);
            let key = if key_len >= 0 {
                let k = cursor[..key_len as usize].to_vec();
                cursor = &cursor[key_len as usize..];
                Some(Bytes::from(k))
            } else {
                None
            };
            
            let value_len = get_varint(&mut cursor) as usize;
            let value = Bytes::from(cursor[..value_len].to_vec());
            cursor = &cursor[value_len..];
            
            records.push(Record { offset, timestamp, key, value });
        }
        
        Ok(records)
    }
}
```

---

## S3 Integration

### File Naming

```
s3://bucket/data/{topic}/{partition}/{base_offset:020}.seg

Example:
s3://my-bucket/data/orders/0/00000000000000000000.seg
s3://my-bucket/data/orders/0/00000000000000100000.seg
```

### Upload

```rust
impl S3Storage {
    pub async fn upload_segment(
        &self,
        topic: &str,
        partition: u32,
        base_offset: u64,
        data: Bytes,
    ) -> Result<String> {
        let key = format!(
            "data/{}/{}/{:020}.seg",
            topic, partition, base_offset
        );
        
        // Retry with backoff
        for attempt in 0..3 {
            match self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(data.clone().into())
                .send()
                .await
            {
                Ok(_) => return Ok(key),
                Err(e) if attempt < 2 => {
                    tokio::time::sleep(Duration::from_millis(100 << attempt)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
        unreachable!()
    }
}
```

---

## Design Decisions Summary

| Decision | Choice | Why |
|----------|--------|-----|
| Compression | LZ4 | Best speed/ratio balance |
| Block size | 1MB | Good compression, fits in memory |
| Index | Per-block | Simple, fast binary search |
| Varints | ZigZag | Handles deltas efficiently |
| File size | 64-256MB | Balances S3 cost vs freshness |
| Magic bytes | Yes | Easy corruption detection |
| CRC | Per-block + file | Catch corruption early |

---

## Comparison to Kafka's Format

| Aspect | Kafka | Your Format |
|--------|-------|-------------|
| Storage | Local disk | S3 |
| Segment size | 1GB default | 64-256MB |
| Index | Sparse (every 4KB) | Per-block (~1MB) |
| Compression | Per-batch | Per-block |
| Replication | Broker-to-broker | S3 handles it |

**Key insight:** S3 is already replicated and durable. You don't need Kafka's replication complexity.
