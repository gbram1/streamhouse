# Deep Dive Part 1: Kafka Protocol Specifics

## Overview

Kafka uses a binary protocol over TCP. All messages are size-delimited request/response pairs. The protocol is versioned per-API, allowing backward compatibility.

---

## Message Frame Structure

```
Request:
┌─────────────────────────────────────────────────────────┐
│ Size (4 bytes)         - Total message size             │
├─────────────────────────────────────────────────────────┤
│ API Key (2 bytes)      - Which API (0=Produce, 1=Fetch) │
│ API Version (2 bytes)  - Version of this API            │
│ Correlation ID (4 bytes) - Client-provided request ID   │
│ Client ID (string)     - Client identifier              │
├─────────────────────────────────────────────────────────┤
│ Request Body           - Varies by API                  │
└─────────────────────────────────────────────────────────┘

Response:
┌─────────────────────────────────────────────────────────┐
│ Size (4 bytes)         - Total message size             │
├─────────────────────────────────────────────────────────┤
│ Correlation ID (4 bytes) - Matches request              │
├─────────────────────────────────────────────────────────┤
│ Response Body          - Varies by API                  │
└─────────────────────────────────────────────────────────┘
```

## Primitive Types

| Type | Description |
|------|-------------|
| int8 | Signed 8-bit integer |
| int16 | Signed 16-bit integer (big-endian) |
| int32 | Signed 32-bit integer (big-endian) |
| int64 | Signed 64-bit integer (big-endian) |
| varint | Variable-length integer (ZigZag encoded) |
| string | int16 length + UTF-8 bytes |
| nullable_string | int16 length (-1 for null) + UTF-8 bytes |
| bytes | int32 length + raw bytes |
| array | int32 count + repeated elements |

---

## APIs to Implement (Priority Order)

### Phase 1: Minimum Viable (Week 9-11)

| API Key | Name | Purpose | Priority |
|---------|------|---------|----------|
| 18 | ApiVersions | First request from any client | **CRITICAL** |
| 3 | Metadata | Discover topics/partitions/brokers | **CRITICAL** |
| 0 | Produce | Write records | **CRITICAL** |
| 1 | Fetch | Read records | **CRITICAL** |
| 2 | ListOffsets | Find earliest/latest offset | **CRITICAL** |

### Phase 2: Consumer Groups (Week 12)

| API Key | Name | Purpose |
|---------|------|---------|
| 10 | FindCoordinator | Locate group coordinator |
| 11 | JoinGroup | Join consumer group |
| 14 | SyncGroup | Get partition assignment |
| 12 | Heartbeat | Keep session alive |
| 8 | OffsetCommit | Save consumer position |
| 9 | OffsetFetch | Load consumer position |
| 13 | LeaveGroup | Leave consumer group |

### Phase 3: Admin APIs (Week 13-14)

| API Key | Name | Purpose |
|---------|------|---------|
| 19 | CreateTopics | Create topics |
| 20 | DeleteTopics | Delete topics |

### APIs to Skip Initially

| API Key | Name | Why Skip |
|---------|------|----------|
| 4-6 | Replication | Internal broker-to-broker |
| 21-31 | Admin/ACL/Transactions | Complex, rarely needed |

---

## ApiVersions (API Key 18) - IMPLEMENT FIRST

Every Kafka client sends this first. Return what you support.

```rust
fn handle_api_versions() -> ApiVersionsResponse {
    ApiVersionsResponse {
        error_code: 0,
        api_versions: vec![
            ApiVersion { api_key: 0, min_version: 0, max_version: 8 },   // Produce
            ApiVersion { api_key: 1, min_version: 0, max_version: 11 },  // Fetch
            ApiVersion { api_key: 2, min_version: 0, max_version: 5 },   // ListOffsets
            ApiVersion { api_key: 3, min_version: 0, max_version: 9 },   // Metadata
            ApiVersion { api_key: 8, min_version: 0, max_version: 6 },   // OffsetCommit
            ApiVersion { api_key: 9, min_version: 0, max_version: 5 },   // OffsetFetch
            ApiVersion { api_key: 10, min_version: 0, max_version: 2 },  // FindCoordinator
            ApiVersion { api_key: 11, min_version: 0, max_version: 5 },  // JoinGroup
            ApiVersion { api_key: 12, min_version: 0, max_version: 3 },  // Heartbeat
            ApiVersion { api_key: 13, min_version: 0, max_version: 2 },  // LeaveGroup
            ApiVersion { api_key: 14, min_version: 0, max_version: 3 },  // SyncGroup
            ApiVersion { api_key: 18, min_version: 0, max_version: 2 },  // ApiVersions
            ApiVersion { api_key: 19, min_version: 0, max_version: 3 },  // CreateTopics
            ApiVersion { api_key: 20, min_version: 0, max_version: 3 },  // DeleteTopics
        ],
    }
}
```

---

## Metadata (API Key 3)

Returns cluster topology. For single-node, you're everything.

```rust
fn handle_metadata(request: MetadataRequest, state: &State) -> MetadataResponse {
    MetadataResponse {
        brokers: vec![BrokerMetadata {
            node_id: 0,
            host: "localhost".to_string(),
            port: 9092,
        }],
        controller_id: 0,
        topics: request.topics.iter().map(|name| {
            match state.get_topic(name) {
                Some(topic) => TopicMetadata {
                    error_code: 0,
                    name: name.clone(),
                    partitions: (0..topic.partition_count).map(|p| {
                        PartitionMetadata {
                            error_code: 0,
                            partition_index: p,
                            leader_id: 0,        // You're the leader
                            replica_nodes: vec![0],
                            isr_nodes: vec![0],
                        }
                    }).collect(),
                },
                None => TopicMetadata {
                    error_code: 3, // UNKNOWN_TOPIC_OR_PARTITION
                    name: name.clone(),
                    partitions: vec![],
                },
            }
        }).collect(),
    }
}
```

---

## Produce (API Key 0)

Write records. The RecordBatch format is complex.

**RecordBatch Structure:**
```
RecordBatch =>
  BaseOffset: int64
  BatchLength: int32
  PartitionLeaderEpoch: int32
  Magic: int8 (must be 2)
  CRC: int32
  Attributes: int16 (compression in bits 0-2)
  LastOffsetDelta: int32
  FirstTimestamp: int64
  MaxTimestamp: int64
  ProducerId: int64
  ProducerEpoch: int16
  BaseSequence: int32
  RecordCount: int32
  Records: [Record]
```

**Compression (from Attributes bits 0-2):**
- 0 = None
- 1 = GZIP
- 2 = Snappy
- 3 = LZ4
- 4 = ZSTD

```rust
fn handle_produce(request: ProduceRequest, state: &mut State) -> ProduceResponse {
    let mut responses = vec![];
    
    for topic_data in request.topics {
        let mut partition_responses = vec![];
        
        for partition_data in topic_data.partitions {
            // Parse and decompress record batch
            let records = parse_record_batch(&partition_data.records)?;
            
            // Write to your storage
            let base_offset = state.storage
                .append(&topic_data.name, partition_data.index, records)?;
            
            partition_responses.push(PartitionProduceResponse {
                index: partition_data.index,
                error_code: 0,
                base_offset,
                log_append_time_ms: -1,
            });
        }
        
        responses.push(TopicProduceResponse {
            name: topic_data.name,
            partitions: partition_responses,
        });
    }
    
    ProduceResponse { responses, throttle_time_ms: 0 }
}
```

---

## Fetch (API Key 1)

Read records. Supports long polling.

```rust
async fn handle_fetch(request: FetchRequest, state: &State) -> FetchResponse {
    let deadline = Instant::now() + Duration::from_millis(request.max_wait_ms);
    
    loop {
        let mut responses = vec![];
        let mut has_data = false;
        
        for topic_req in &request.topics {
            let mut partitions = vec![];
            
            for partition_req in &topic_req.partitions {
                let (records, high_watermark) = state.storage.read(
                    &topic_req.name,
                    partition_req.index,
                    partition_req.fetch_offset,
                    partition_req.max_bytes,
                )?;
                
                if !records.is_empty() {
                    has_data = true;
                }
                
                partitions.push(PartitionFetchResponse {
                    index: partition_req.index,
                    error_code: 0,
                    high_watermark,
                    records,
                });
            }
            
            responses.push(TopicFetchResponse {
                name: topic_req.name.clone(),
                partitions,
            });
        }
        
        // Return if we have data OR timeout reached
        if has_data || Instant::now() >= deadline {
            return FetchResponse { responses, throttle_time_ms: 0 };
        }
        
        // Wait a bit and try again (long polling)
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
```

---

## Consumer Group Protocol

The most complex part. Here's the flow:

```
Consumer                    Coordinator (you)
   │                              │
   │──FindCoordinator────────────▶│  "Who manages group X?"
   │◀────(you: broker 0)──────────│
   │                              │
   │──JoinGroup──────────────────▶│  "I want to join group X"
   │        (wait for others...)  │
   │◀────JoinResponse─────────────│  "You're the leader" or "You're a follower"
   │                              │
   │──SyncGroup──────────────────▶│  Leader: "Here's the assignment"
   │◀────SyncResponse─────────────│  "Here are your partitions"
   │                              │
   │──Heartbeat──────────────────▶│  (every few seconds)
   │◀────(ok)─────────────────────│
   │                              │
   │──OffsetCommit───────────────▶│  "I processed up to offset X"
   │◀────(ok)─────────────────────│
```

**State to Track:**

```rust
struct ConsumerGroup {
    group_id: String,
    generation_id: i32,
    leader_id: Option<String>,
    members: HashMap<String, GroupMember>,
    state: GroupState,
}

enum GroupState {
    Empty,
    PreparingRebalance,  // Waiting for JoinGroup
    CompletingRebalance, // Waiting for SyncGroup
    Stable,              // Normal operation
}

struct GroupMember {
    member_id: String,
    subscriptions: Vec<String>,
    assignment: Vec<TopicPartition>,
    session_timeout_ms: i32,
    last_heartbeat: Instant,
}
```

---

## Error Codes

| Code | Name | When |
|------|------|------|
| 0 | NONE | Success |
| 1 | OFFSET_OUT_OF_RANGE | Bad offset |
| 3 | UNKNOWN_TOPIC_OR_PARTITION | Doesn't exist |
| 25 | UNKNOWN_MEMBER_ID | Not in group |
| 27 | REBALANCE_IN_PROGRESS | Try again |

---

## Testing

```bash
# Install kcat (formerly kafkacat)
brew install kcat  # or apt install kafkacat

# Test metadata
kcat -L -b localhost:9092

# Test produce
echo "hello" | kcat -P -b localhost:9092 -t test

# Test consume
kcat -C -b localhost:9092 -t test -o beginning
```

---

## Implementation Order

1. **Week 9:** TCP server + ApiVersions + Metadata
2. **Week 10:** Produce API (parse RecordBatch)
3. **Week 11:** Fetch API (long polling)
4. **Week 12:** Consumer groups (JoinGroup, SyncGroup, Heartbeat)
5. **Week 13:** OffsetCommit/Fetch, CreateTopics
6. **Week 14:** Testing, edge cases, compatibility
