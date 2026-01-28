# StreamHouse Architecture: How Everything Works Together

## Overview

StreamHouse is a distributed event streaming platform similar to Apache Kafka. This document explains how all the pieces fit together.

## Core Concepts with Real-World Analogies

### 1. **Topics** = Netflix Categories

**What it is**: A topic is a category or feed name to which messages are published.

**Simple Analogy**: Think of Netflix categories like "Action Movies", "Documentaries", "Sci-Fi"
- Each topic is like a category that groups related content
- Examples: `orders`, `user-events`, `logs`
- Just like Netflix has categories to organize content, StreamHouse has topics to organize messages

**Why we need it**: Without topics, all your messages would be in one giant pile. Imagine Netflix with no categories - you'd have to scroll through everything to find what you want! Topics let you separate different types of data:
- `orders` topic = all order-related messages
- `user-events` topic = all user activity messages
- `logs` topic = all system logs

---

### 2. **Partitions** = Multiple Checkout Lanes at a Store

**What it is**: A partition is an ordered, immutable sequence of messages within a topic.

**Simple Analogy**: Think of a grocery store with multiple checkout lanes
- Each lane (partition) processes customers independently
- Customers in Lane 1 don't affect Lane 2's speed
- More lanes = faster overall checkout
- Each lane maintains order (first customer in line checks out first)

**Example**:
```
Topic: "orders" (3 partitions) = Store with 3 checkout lanes
  Partition 0: [order1, order2, order3] ← Lane 1
  Partition 1: [order4, order5]         ← Lane 2
  Partition 2: [order6, order7, order8] ← Lane 3
```

**Why we need it**:
- **Speed**: One lane (partition) can only process so fast. Multiple lanes = parallel processing = higher throughput
- **Scalability**: If you have 1 million orders/second, one partition can't handle it. But 100 partitions can each handle 10,000 orders/second
- **Without partitions**: Everyone waits in ONE line. Bottleneck!
- **With partitions**: Traffic spreads across multiple lanes. Much faster!

**Real numbers**:
- 1 partition might handle 10,000 msgs/sec
- 10 partitions can handle 100,000 msgs/sec
- 100 partitions can handle 1,000,000 msgs/sec

---

### 3. **Offsets** = Page Numbers in a Book

**What it is**: An offset is a unique sequential ID for each message within a partition.

**Simple Analogy**: Think of page numbers in a book
- Page 1, Page 2, Page 3, Page 4...
- You can bookmark your spot (offset 42)
- Next time you open the book, you know exactly where you left off
- Page numbers never change - page 5 is always page 5

**Example**:
```
Partition 0 (like Book #1):
  offset 0: {"user": "alice", "action": "login"}     ← Page 0
  offset 1: {"user": "bob", "action": "purchase"}    ← Page 1
  offset 2: {"user": "alice", "action": "logout"}    ← Page 2

Partition 1 (like Book #2 - separate numbering!):
  offset 0: {"user": "charlie", "action": "login"}   ← Page 0 in different book
  offset 1: {"user": "dave", "action": "view"}       ← Page 1 in different book
```

**Why we need it**:
- **Resume reading**: Consumer can say "I've read up to offset 42, give me everything after that"
- **No duplicates**: If processing fails, you can restart from offset 42 instead of re-processing everything
- **Tracking progress**: Like a bookmark - you know exactly where you are
- **Each partition = separate book**: Partition 0's offset 0 is different from Partition 1's offset 0

**Without offsets**: You'd have to remember "I processed the message about Alice's login at 3:42pm on Tuesday" - impossible to track!

---

### 4. **Agents** = Warehouse Workers

**What it is**: An agent is a stateless server process that handles partition operations.

**Simple Analogy**: Think of Amazon warehouse workers
- Each worker (agent) is assigned specific aisles (partitions) to manage
- Workers receive orders, put items on shelves (write to S3), and retrieve items (read for consumers)
- Workers are interchangeable - if one is sick, another can take over their aisles
- Workers don't keep items in their pockets (stateless) - everything goes on the shelves (S3)

**Agent Responsibilities**:
1. Accept messages via gRPC (like receiving orders at the warehouse)
2. Write messages to S3 in batches (like putting items on shelves)
3. Serve messages to consumers (like picking items for delivery)
4. Send heartbeats to metadata store (like checking in: "I'm still working!")

**Why we need it**:
- **High Availability**: If one worker (agent) crashes, another can take over their aisles (partitions)
- **Scalability**: More workers = more partitions handled = higher capacity
- **Stateless**: Workers don't need special knowledge. Any worker can manage any aisle. Makes replacing them easy.

**Without agents**: Messages would just pile up with no one to organize them!

---

### 5. **Leases** = Work Shift Assignments

**What it is**: A lease is a temporary assignment of a partition to an agent.

**Simple Analogy**: Think of work shifts at a 24/7 store
- "Alice is assigned to Register 1 from 9am-5pm"
- Only ONE person can operate Register 1 at a time (prevents chaos)
- If Alice doesn't show up (heartbeat fails), the manager (metadata store) reassigns Register 1 to someone else
- Shifts have expiration times - at 5pm, Alice's shift ends and someone else can take over

**Lease Table**:
```
partition    agent       expires_at           (like shift schedule)
-------------------------------------------------------------------
orders/0     agent-001   2026-01-28 12:00:00  ← Agent-001 works this "register" until noon
orders/1     agent-002   2026-01-28 12:00:00  ← Agent-002 works this "register" until noon
orders/2     agent-001   2026-01-28 12:00:00  ← Agent-001 also works this "register"
```

**Why we need it**:
- **Prevents conflicts**: Only ONE agent writes to a partition at a time. Otherwise, offsets would get messed up!
  - Like two cashiers trying to use the same register = chaos
  - With leases: Clear ownership. No conflicts.

- **Automatic failover**: If agent-001 crashes (stops sending heartbeats), its lease expires, and agent-002 takes over
  - Like: "Alice didn't show up, Bob takes her register"

- **Load balancing**: Leases can be redistributed to balance work
  - If one agent is overloaded, some leases can move to idle agents

**Without leases**:
- Two agents might try to write offset 42 simultaneously → data corruption!
- If an agent crashes, no one knows who should take over → downtime!

---

### 6. **Agent Groups** = Department Teams

**What it is**: Agent groups enable multi-tenancy and workload isolation.

**Simple Analogy**: Think of departments in a company
- **Engineering Team** (agent group "eng"): Only handles engineering work
- **Sales Team** (agent group "sales"): Only handles sales work
- **Interns** (agent group "interns"): Only handles low-priority test work

You wouldn't assign a sales task to an engineering team member, and vice versa.

**Example**:
```
Agent Group: "production"
  - Agents: agent-prod-1, agent-prod-2, agent-prod-3
  - Handles: Critical production topics (orders, payments)
  - Resources: High CPU, lots of memory

Agent Group: "staging"
  - Agents: agent-staging-1
  - Handles: Test topics
  - Resources: Small, cheap machine

Agent Group: "analytics"
  - Agents: agent-analytics-1, agent-analytics-2
  - Handles: Analytics topics (logs, metrics)
  - Resources: Optimized for batch processing
```

**Why we need it**:
- **Resource isolation**: Production workload won't be affected by someone's test script
  - Like: Sales team has their own office space, engineers don't distract them

- **Multi-tenancy**: Company A's agents never see Company B's data
  - Like: Different companies in the same office building have separate locked floors

- **Priority control**: Critical topics get dedicated agents, low-priority topics share agents
  - Like: Emergency room doctors vs. routine checkup doctors

**Without agent groups**:
- Your production traffic could be slowed down by a developer's test
- Multiple companies would need entirely separate StreamHouse installations (expensive!)

---

## Why ALL These Parts Are Needed - The Big Picture

**Imagine you're building Twitter's message feed system**:

1. **Topics** = Different feed types
   - `tweets` topic
   - `likes` topic
   - `retweets` topic
   - `direct_messages` topic

   Without topics: All tweets, likes, DMs in one giant pile → impossible to manage

2. **Partitions** = Parallel processing lanes
   - Twitter gets 500 million tweets/day
   - 1 partition can't handle that → need 1000 partitions
   - Each partition handles 500,000 tweets

   Without partitions: One checkout lane for 500 million customers → everyone waits forever

3. **Offsets** = Tracking what you've read
   - Your phone crashes while scrolling
   - Offset lets you resume from "tweet #42" instead of starting over

   Without offsets: Every time your app crashes, you start from the beginning → terrible UX

4. **Agents** = The workers doing the actual work
   - Need multiple agents to handle 1000 partitions
   - If one agent crashes, others take over its partitions

   Without agents: No one to actually store/retrieve messages → system doesn't work

5. **Leases** = Making sure only ONE agent writes to each partition
   - Prevents two agents from both writing "offset 42" → data corruption
   - Automatic failover when an agent crashes

   Without leases: Data corruption, no failover, chaos

6. **Agent Groups** = Keeping production separate from testing
   - Production tweets → production agents (fast, reliable)
   - Test environment → test agents (separate, won't affect production)

   Without agent groups: Someone's test could slow down Twitter for everyone

---

**The Restaurant Analogy - Putting It All Together**:

Imagine a restaurant chain (StreamHouse):

- **Topics** = Menu categories (Appetizers, Entrees, Desserts)
- **Partitions** = Multiple kitchens working in parallel
- **Offsets** = Order ticket numbers (Order #1, #2, #3...)
- **Agents** = Chefs assigned to specific stations
- **Leases** = Shift assignments ("Chef Bob works Station 3 from 9am-5pm")
- **Agent Groups** = Different restaurant locations (Downtown location, Airport location)

Without all these parts:
- No menu categories = chaos
- One kitchen = huge backlog, slow service
- No order numbers = lost orders, duplicates
- No chefs = no food
- No shift assignments = chefs fighting over stations
- No separate locations = one restaurant trying to serve entire city

**With all these parts working together**:
- ✅ Organized (topics)
- ✅ Fast (partitions)
- ✅ Trackable (offsets)
- ✅ Reliable (agents + leases)
- ✅ Scalable (agent groups)

This is why every piece is essential!

---

## How It All Works Together

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        PRODUCERS                                │
│  (Send messages to topics)                                      │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ 1. Send(topic, key, value)
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                    METADATA STORE                               │
│  (SQLite or PostgreSQL)                                         │
│                                                                 │
│  Tables:                                                        │
│  ┌──────────────────────────────────────────────────────┐     │
│  │  topics: name, partition_count, created_at           │     │
│  │  partitions: topic, partition_id, high_watermark     │     │
│  │  partition_leases: topic, partition_id, agent_id     │     │
│  │  agents: agent_id, address, last_heartbeat           │     │
│  └──────────────────────────────────────────────────────┘     │
│                                                                 │
│  2. Query: Which agent leads partition 0 of "orders"?          │
│     Answer: agent-001 at 10.0.1.15:9090                        │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ 3. Connect to agent-001
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                      AGENT (agent-001)                          │
│  Address: 10.0.1.15:9090                                        │
│  Group: "default"                                               │
│                                                                 │
│  Active Leases:                                                 │
│    - orders/partition-0                                         │
│    - orders/partition-2                                         │
│    - logs/partition-1                                           │
│                                                                 │
│  Operations:                                                    │
│  1. Receive message via gRPC                                    │
│  2. Assign offset (read current watermark, increment)           │
│  3. Write to S3 in batches                                      │
│  4. Update high_watermark in metadata store                     │
│  5. Return offset to producer                                   │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ 4. Write to S3
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                       S3 / MinIO                                │
│  Bucket: streamhouse-data                                       │
│                                                                 │
│  Objects:                                                       │
│  ┌──────────────────────────────────────────────────────┐     │
│  │  orders/partition-0/segment-0000000000.data          │     │
│  │  orders/partition-0/segment-0000001000.data          │     │
│  │  orders/partition-1/segment-0000000000.data          │     │
│  │  logs/partition-0/segment-0000000000.data            │     │
│  └──────────────────────────────────────────────────────┘     │
│                                                                 │
│  Each segment contains batches of messages                      │
└─────────────────────────────────────────────────────────────────┘
```

---

## Message Flow: Producer to Storage

### Step-by-Step Example

**Scenario**: Producer sends message to topic "orders"

```
1. Producer calls:
   producer.send("orders", key="user123", value='{"amount": 99.99}')

2. Producer queries metadata store:
   "Which agent leads partition 0 of 'orders'?"
   Response: agent-001 at 10.0.1.15:9090

3. Producer batches messages (for efficiency)
   Batch: [message1, message2, message3]

4. Producer sends gRPC request to agent-001:
   ProduceRequest {
     topic: "orders",
     partition: 0,
     records: [...]
   }

5. Agent-001 receives batch:
   a. Checks it owns the lease for orders/partition-0
   b. Reads current high_watermark from metadata: 1250
   c. Assigns offsets: 1250, 1251, 1252
   d. Writes batch to S3: orders/partition-0/segment-0000001000.data
   e. Updates metadata: high_watermark = 1253
   f. Returns: ProduceResponse { offsets: [1250, 1251, 1252] }

6. Producer receives response:
   SendResult { offset: 1250, partition: 0 }

7. Message now available for consumers to read!
```

---

## Partition Assignment & Load Balancing

### How Partitions Get Assigned to Agents

```
┌─────────────────────────────────────────────────────────────────┐
│  Topic: "orders" (6 partitions)                                 │
│  Agents: agent-001, agent-002, agent-003                        │
└─────────────────────────────────────────────────────────────────┘

Initial State (no assignments):
  orders/partition-0: (no leader)
  orders/partition-1: (no leader)
  orders/partition-2: (no leader)
  orders/partition-3: (no leader)
  orders/partition-4: (no leader)
  orders/partition-5: (no leader)

Agent Coordination Algorithm:
  1. All agents register in metadata store with heartbeats
  2. Each agent periodically checks for unleased partitions
  3. Agent attempts to acquire lease (atomic operation)
  4. If successful, agent becomes leader for that partition

Result (balanced):
  orders/partition-0: agent-001
  orders/partition-1: agent-002
  orders/partition-2: agent-003
  orders/partition-3: agent-001
  orders/partition-4: agent-002
  orders/partition-5: agent-003

Load: Each agent handles 2 partitions
```

### Lease Renewal (Heartbeats)

```
Every 10 seconds:
  agent-001 sends heartbeat to metadata store
    UPDATE agents SET last_heartbeat = NOW() WHERE agent_id = 'agent-001'

  agent-001 renews leases:
    UPDATE partition_leases
    SET expires_at = NOW() + 30 seconds
    WHERE agent_id = 'agent-001'
```

### Failover

```
Scenario: agent-002 crashes

Before:
  orders/partition-1: agent-002 (healthy)
  orders/partition-4: agent-002 (healthy)

Agent-002 crashes (no more heartbeats)

After 30 seconds (lease expires):
  orders/partition-1: (no leader) - lease expired
  orders/partition-4: (no leader) - lease expired

Agent-001 and agent-003 detect unleased partitions:
  agent-001 acquires orders/partition-1
  agent-003 acquires orders/partition-4

New State:
  orders/partition-1: agent-001
  orders/partition-4: agent-003

Producers/consumers automatically redirect to new leaders!
```

---

## Multi-Tenancy with Agent Groups

### Scenario: Multiple Organizations

```
Organization A (prod workload):
  agents: agent-prod-1, agent-prod-2
  group: "org-a-prod"

Organization B (prod workload):
  agents: agent-prod-3, agent-prod-4
  group: "org-b-prod"

Staging Environment:
  agents: agent-staging-1
  group: "staging"

Topic: "orders" (created for org-a-prod)
  - Partitions only assigned to agents in "org-a-prod" group
  - agent-prod-1 and agent-prod-2 handle all partitions
  - agent-prod-3/4 and agent-staging-1 never see these partitions

Result:
  ✓ Workload isolation
  ✓ Resource guarantees
  ✓ Security boundaries
```

---

## Key Invariants & Guarantees

### 1. **One Leader Per Partition**
```
✓ At most ONE agent can lead a partition at any time
✗ Never two agents writing to same partition simultaneously
```

### 2. **Sequential Offsets**
```
✓ Offsets within a partition: 0, 1, 2, 3, ...
✗ Never gaps or duplicates
```

### 3. **Partition Ordering**
```
✓ Messages in same partition are ordered
✗ Messages across partitions have no ordering
```

### 4. **Availability**
```
✓ If ANY agent is alive, partitions can be assigned
✗ If ALL agents die, system is unavailable
```

### 5. **Durability**
```
✓ Messages written to S3 are durable
✓ Metadata in SQLite/PostgreSQL is durable
✗ In-flight batches (not yet flushed) may be lost on agent crash
```

---

## Practical Examples

### Example 1: Sending 100 Messages

```bash
# Topic: "test" (3 partitions)
# Agents: agent-001

for i in {1..100}; do
  curl -X POST http://localhost:3001/api/v1/produce \
    -d "{\"topic\":\"test\",\"key\":\"user$i\",\"value\":\"message $i\"}"
done
```

**What Happens**:
1. Messages are hashed by key to determine partition
2. `user1`, `user4`, `user7`, ... → partition 0
3. `user2`, `user5`, `user8`, ... → partition 1
4. `user3`, `user6`, `user9`, ... → partition 2
5. All messages go to agent-001 (only agent)
6. Agent writes ~33 messages to each partition
7. Offsets increment: 0→32 per partition

**Result**:
```
Partition 0: 33 messages (offsets 0-32)
Partition 1: 33 messages (offsets 0-32)
Partition 2: 34 messages (offsets 0-33)
```

### Example 2: Adding a Second Agent

```bash
# Start agent-002
export AGENT_ID=agent-002
export AGENT_ADDRESS=10.0.1.16:9090
./agent

# Agent-002 registers in metadata store
# Agent-002 looks for unleased partitions
# Agent-002 acquires leases for some partitions
```

**Before**:
```
Partition 0: agent-001
Partition 1: agent-001
Partition 2: agent-001
```

**After** (rebalance):
```
Partition 0: agent-001
Partition 1: agent-002  ← taken over
Partition 2: agent-001
```

**Producer** automatically routes to correct agent:
- Messages for partition 0 → agent-001
- Messages for partition 1 → agent-002
- Messages for partition 2 → agent-001

### Example 3: Agent Failure

```bash
# Kill agent-001
kill <agent-001-pid>

# Agent-001 stops sending heartbeats
# Leases expire after 30 seconds
# Agent-002 acquires orphaned partitions
```

**Timeline**:
```
T+0s:  agent-001 crashes
T+10s: Last heartbeat was at T-10s (still valid)
T+30s: Lease expires (no heartbeat for 30s)
T+31s: agent-002 detects unleased partition 0
T+32s: agent-002 acquires lease for partition 0
T+33s: Producers redirect to agent-002 for partition 0
```

**Downtime**: ~30 seconds for affected partitions

---

## Summary: The Big Picture

1. **Topics** are divided into **partitions** for parallelism
2. **Partitions** store ordered messages with sequential **offsets**
3. **Agents** are stateless servers that manage partitions
4. **Leases** assign each partition to exactly one agent
5. **Agent groups** enable multi-tenancy and isolation
6. **Metadata store** coordinates everything
7. **S3** stores all message data durably

**Data Flow**:
```
Producer → Metadata (find agent) → Agent (assign offset) → S3 (store) → Consumer
```

**Coordination**:
```
Agents ← Heartbeats → Metadata Store → Lease Management → Partition Assignment
```

**Fault Tolerance**:
```
Agent Dies → Lease Expires → Another Agent Takes Over → Service Restored
```

---

## Visualizing Your Current Setup

When you run the web console at http://localhost:3002:

**Dashboard** shows:
- Number of topics you've created
- Number of agents running
- Total messages stored
- Total partitions

**Topics Page** shows:
- All topics with partition counts
- When you create a topic, partitions are created but not yet assigned

**Agents Page** shows:
- All running agents
- How many leases (partitions) each agent is handling
- Last heartbeat time (health indicator)

**Console Page** lets you:
- Send messages to any topic
- See which partition and offset the message was assigned
- Test the full pipeline end-to-end

---

## FAQ

**Q: Why can offset be 0 for multiple messages?**
A: Each partition has its own offset sequence. offset=0 in partition-0 is different from offset=0 in partition-1.

**Q: What if two producers send to the same partition simultaneously?**
A: The agent serializes all writes. Messages arrive sequentially, each gets the next offset.

**Q: What happens if the metadata store goes down?**
A: System becomes unavailable. Metadata store is the source of truth. (Use PostgreSQL with replication for HA)

**Q: Can I have more partitions than agents?**
A: Yes! Each agent can handle multiple partitions. Common to have 100 partitions and 5 agents.

**Q: What determines which partition a message goes to?**
A:
- If you specify `partition=N`: Goes to partition N
- If you specify a `key`: hash(key) % partition_count determines partition
- If neither: Round-robin across partitions

**Q: What is the "key" in a message?**
A: The key is an optional identifier that serves two purposes:

1. **Partition Routing**: Messages with the same key always go to the same partition
   - Example: All messages for `user123` go to partition 0
   - Example: All messages for `user456` go to partition 1
   - This ensures **ordered processing** per user/entity

2. **Compaction**: In compacted topics (future feature), only the latest message per key is kept
   - Example: User profile updates - only keep latest state per user_id

**Key Examples**:
```json
// Order messages - key is order_id
{"key": "order-12345", "value": {"status": "pending", "amount": 99.99}}
{"key": "order-12345", "value": {"status": "paid", "amount": 99.99}}
// Both messages go to same partition, guaranteeing order

// User events - key is user_id
{"key": "user-123", "value": {"action": "login", "timestamp": 1234567890}}
{"key": "user-123", "value": {"action": "purchase", "timestamp": 1234567891}}
// Both events for user-123 go to same partition, preserving order

// No key - round-robin across partitions
{"value": {"temperature": 72.5, "sensor": "living-room"}}
// Goes to any partition (no ordering guarantee)
```

**When to use a key**:
- ✅ You need ordered processing per entity (user, order, device, etc.)
- ✅ You want related messages together (same partition = same agent)
- ✅ You want predictable partition assignment
- ❌ Don't use a key if you just want maximum parallelism

**Q: What's the point of partitions?**
A: Parallelism! More partitions = more throughput. Each partition can be handled by a different agent.

---

This architecture is production-ready and scales to:
- Millions of messages per second
- Thousands of partitions
- Hundreds of agents
- Multiple datacenters (with agent groups)

🚀 **Welcome to distributed event streaming!**
