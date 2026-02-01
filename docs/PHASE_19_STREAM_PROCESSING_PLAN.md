# Phase 19: SQL Stream Processing Engine

**Priority:** MEDIUM-HIGH
**Effort:** 120-200 hours (varies by approach)
**Status:** PLANNED
**Prerequisite:** Phase 13-15 (UI, CLI, K8s) complete

---

## Overview

Add SQL-based stream processing capabilities to StreamHouse, enabling real-time transformations, aggregations, joins, and analytics directly on streaming data without external dependencies.

**Goal:** Allow users to write declarative SQL queries that continuously process streams, maintaining stateful aggregations and producing derived streams.

---

## Business Value

### Use Cases
1. **Real-time Analytics**
   - Page view aggregations (sessions, clicks per user)
   - Financial metrics (order totals, average transaction value)
   - IoT sensor aggregations (temperature averages, anomaly detection)

2. **Stream Enrichment**
   - Join order stream with customer reference data
   - Enrich events with lookup tables
   - Correlate multiple event streams

3. **Alerting & Monitoring**
   - Detect anomalies (sudden traffic spikes, error rate increases)
   - Threshold-based alerts (temp > 100°C, latency > 1s)
   - Pattern detection (3 failed logins → alert)

4. **ETL Pipelines**
   - Filter, transform, and route events
   - Data cleansing and normalization
   - Schema evolution and migration

### Competitive Positioning
- **Kafka Streams:** Requires JVM, complex deployment
- **ksqlDB:** Separate service, additional ops burden
- **Flink:** Heavy infrastructure, steep learning curve
- **StreamHouse:** Embedded SQL engine, no extra services, Rust performance

---

## SQL Feature Set

### Phase 19.1: Basic Queries (40h)

**DDL - Stream Definitions:**
```sql
-- Define a stream (maps to existing topic)
CREATE STREAM orders (
  order_id BIGINT,
  customer_id VARCHAR,
  amount DECIMAL(10,2),
  status VARCHAR,
  created_at TIMESTAMP
) WITH (
  kafka_topic = 'orders',
  value_format = 'AVRO',
  key = 'order_id',
  timestamp = 'created_at'
);

-- Define a table (changelog stream)
CREATE TABLE customers (
  customer_id VARCHAR PRIMARY KEY,
  name VARCHAR,
  email VARCHAR,
  tier VARCHAR
) WITH (
  kafka_topic = 'customers',
  value_format = 'AVRO'
);
```

**DML - Basic Queries:**
```sql
-- Simple filter
CREATE STREAM high_value_orders AS
SELECT * FROM orders
WHERE amount > 1000;

-- Projection
CREATE STREAM order_summary AS
SELECT order_id, customer_id, amount, status
FROM orders;

-- Expression evaluation
CREATE STREAM orders_with_fee AS
SELECT 
  order_id,
  amount,
  amount * 0.03 AS processing_fee,
  amount + (amount * 0.03) AS total
FROM orders;
```

### Phase 19.2: Aggregations & Windows (50h)

**Tumbling Windows:**
```sql
-- Count orders per 5-minute window
CREATE STREAM order_counts AS
SELECT 
  TUMBLE_START(created_at, INTERVAL '5' MINUTE) AS window_start,
  COUNT(*) AS order_count,
  SUM(amount) AS total_amount
FROM orders
GROUP BY TUMBLE(created_at, INTERVAL '5' MINUTE);
```

**Sliding Windows:**
```sql
-- Average order value over last 10 minutes, updated every minute
CREATE STREAM avg_order_value AS
SELECT 
  HOP_START(created_at, INTERVAL '1' MINUTE, INTERVAL '10' MINUTE) AS window_start,
  AVG(amount) AS avg_value
FROM orders
GROUP BY HOP(created_at, INTERVAL '1' MINUTE, INTERVAL '10' MINUTE);
```

**Session Windows:**
```sql
-- Group user events into sessions (30-min inactivity gap)
CREATE STREAM user_sessions AS
SELECT 
  user_id,
  SESSION_START(event_time, INTERVAL '30' MINUTE) AS session_start,
  COUNT(*) AS event_count,
  COLLECT_LIST(event_type) AS events
FROM user_events
GROUP BY user_id, SESSION(event_time, INTERVAL '30' MINUTE);
```

**Stateful Aggregations:**
```sql
-- Running total per customer (stateful)
CREATE TABLE customer_totals AS
SELECT 
  customer_id,
  COUNT(*) AS order_count,
  SUM(amount) AS total_spent,
  AVG(amount) AS avg_order_value
FROM orders
GROUP BY customer_id;
```

### Phase 19.3: Joins (40h)

**Stream-Table Join:**
```sql
-- Enrich orders with customer info
CREATE STREAM enriched_orders AS
SELECT 
  o.order_id,
  o.amount,
  c.name AS customer_name,
  c.tier AS customer_tier
FROM orders o
LEFT JOIN customers c
  ON o.customer_id = c.customer_id;
```

**Stream-Stream Join (Windowed):**
```sql
-- Join orders with shipments within 1-hour window
CREATE STREAM order_shipments AS
SELECT 
  o.order_id,
  o.amount,
  s.tracking_number,
  s.carrier
FROM orders o
INNER JOIN shipments s
  WITHIN INTERVAL '1' HOUR
  ON o.order_id = s.order_id;
```

**Multi-way Join:**
```sql
-- Join orders, customers, and products
CREATE STREAM full_order_details AS
SELECT 
  o.order_id,
  c.name AS customer_name,
  p.name AS product_name,
  o.quantity,
  p.price * o.quantity AS line_total
FROM orders o
JOIN customers c ON o.customer_id = c.customer_id
JOIN products p ON o.product_id = p.product_id;
```

### Phase 19.4: Advanced Features (30h)

**User-Defined Functions:**
```sql
-- Register UDF
CREATE FUNCTION detect_fraud(amount DECIMAL, velocity INT)
  RETURNS BOOLEAN
  LANGUAGE RUST
  AS 'fraud_detection.so';

-- Use UDF
CREATE STREAM potential_fraud AS
SELECT * FROM orders
WHERE detect_fraud(amount, order_velocity) = TRUE;
```

**Pattern Matching (CEP):**
```sql
-- Detect 3 failed logins followed by success
CREATE STREAM suspicious_logins AS
SELECT * FROM login_events
MATCH_RECOGNIZE (
  PARTITION BY user_id
  ORDER BY event_time
  MEASURES
    FIRST(event_time) AS pattern_start,
    LAST(event_time) AS pattern_end
  PATTERN (FAIL{3,} SUCCESS)
  DEFINE
    FAIL AS status = 'failed',
    SUCCESS AS status = 'success'
);
```

**Anomaly Detection:**
```sql
-- Detect outliers using moving average
CREATE STREAM anomalies AS
SELECT 
  sensor_id,
  temperature,
  AVG(temperature) OVER (
    PARTITION BY sensor_id
    ORDER BY event_time
    RANGE INTERVAL '1' HOUR PRECEDING
  ) AS avg_temp
FROM sensor_readings
WHERE ABS(temperature - avg_temp) > 10;
```

---

## Implementation Approaches

### Option 1: DataFusion-Based (RECOMMENDED)

**Pros:**
- Built in Rust (native performance, no FFI)
- Apache Arrow columnar format (efficient memory)
- Mature SQL parser and optimizer
- Extensible (custom functions, operators)
- Active development (Apache project)

**Cons:**
- Batch-oriented (needs streaming adapter)
- State management requires custom implementation
- Windowing logic needs to be built

**Implementation:**
```rust
use datafusion::prelude::*;
use datafusion::sql::parser::DFParser;
use datafusion::logical_plan::LogicalPlan;

pub struct StreamProcessor {
    ctx: SessionContext,
    state_store: Arc<StateStore>,
}

impl StreamProcessor {
    pub async fn execute_query(&self, sql: &str) -> Result<StreamPlan> {
        // Parse SQL
        let statements = DFParser::parse_sql(sql)?;
        
        // Build logical plan
        let plan = self.ctx.state().create_logical_plan(&statements[0]).await?;
        
        // Optimize
        let optimized = self.ctx.state().optimize(&plan)?;
        
        // Create physical plan with streaming operators
        let stream_plan = self.build_stream_plan(optimized)?;
        
        Ok(stream_plan)
    }
}
```

**Effort:** 120 hours
**LOC:** ~3,000

---

### Option 2: ksqlDB-Compatible Layer

**Pros:**
- Proven SQL dialect (familiar to users)
- Existing tooling and documentation
- Community resources available

**Cons:**
- Need to implement ksqlDB semantics exactly
- Complex compatibility requirements
- Tied to Confluent's API design

**Implementation:**
- Build HTTP API compatible with ksqlDB REST interface
- Implement ksqlDB query language parser
- Map to internal execution engine

**Effort:** 150 hours
**LOC:** ~3,500

---

### Option 3: Apache Flink Integration

**Pros:**
- Battle-tested stream processing
- Rich feature set (CEP, ML, graph)
- Horizontal scalability built-in

**Cons:**
- JVM dependency (operational complexity)
- Heavy resource requirements
- Complex deployment and configuration
- Separate cluster to manage

**Implementation:**
- Flink source/sink connectors for StreamHouse
- Deploy Flink cluster alongside
- Bridge topics to Flink jobs

**Effort:** 80 hours (integration only)
**Ongoing Ops:** HIGH

---

### Option 4: Custom Engine (Full Control)

**Pros:**
- Complete control over features and performance
- No external dependencies
- Optimized for StreamHouse architecture
- Rust all the way down

**Cons:**
- Longest development time
- Need to build everything from scratch
- Higher maintenance burden
- Risk of bugs in complex logic

**Implementation:**
- Custom SQL parser (using nom or pest)
- Query planner and optimizer
- Streaming operators (map, filter, aggregate, join)
- State management (RocksDB-backed)
- Watermark and late data handling

**Effort:** 200 hours
**LOC:** ~5,000

---

## Recommended Approach: DataFusion + Custom Streaming Layer

**Strategy:** Use DataFusion for SQL parsing and batch operations, build custom streaming layer on top.

### Architecture

```
┌─────────────────────────────────────────────┐
│            SQL Query Interface               │
│  (CREATE STREAM, SELECT, GROUP BY, JOIN)    │
└─────────────┬───────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────┐
│         DataFusion SQL Parser                │
│      (Parse SQL → Logical Plan)              │
└─────────────┬───────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────┐
│       Streaming Query Planner                │
│  (Logical Plan → Stream Execution Plan)      │
│   - Identify windows, joins, aggregations    │
│   - Inject watermark operators                │
│   - Plan state management                     │
└─────────────┬───────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────┐
│      Stream Execution Engine                 │
│  ┌──────────────────────────────────────┐   │
│  │  Streaming Operators:                 │   │
│  │  - Map, Filter, FlatMap               │   │
│  │  - KeyBy, Window, Aggregate           │   │
│  │  - Join (Stream-Stream, Stream-Table) │   │
│  │  - Union, Split                        │   │
│  └──────────────────────────────────────┘   │
└─────────────┬───────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────┐
│          State Management                    │
│  - RocksDB for stateful aggregations         │
│  - Checkpointing for fault tolerance         │
│  - TTL for windowed state cleanup            │
└─────────────┬───────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────┐
│        Input/Output Connectors               │
│  - Read from StreamHouse topics              │
│  - Write to StreamHouse topics               │
│  - Schema Registry integration               │
└─────────────────────────────────────────────┘
```

### Core Components

**1. Query Coordinator (20h, ~500 LOC)**
```rust
// crates/streamhouse-query/src/coordinator.rs

pub struct QueryCoordinator {
    queries: Arc<RwLock<HashMap<String, RunningQuery>>>,
    metadata: Arc<dyn MetadataStore>,
    consumer: Consumer,
    producer: Producer,
}

impl QueryCoordinator {
    pub async fn submit_query(&self, sql: &str) -> Result<QueryId> {
        // Parse and validate
        let plan = self.parse_query(sql).await?;
        
        // Allocate resources
        let query_id = self.allocate_query().await?;
        
        // Start execution
        let handle = self.execute_plan(query_id.clone(), plan).await?;
        
        // Track query
        self.queries.write().await.insert(query_id.clone(), handle);
        
        Ok(query_id)
    }
}
```

**2. Streaming Operators (40h, ~1,000 LOC)**
```rust
// crates/streamhouse-query/src/operators/mod.rs

pub trait StreamOperator: Send + Sync {
    async fn process(&mut self, record: Record) -> Result<Vec<Record>>;
}

pub struct FilterOperator {
    predicate: Box<dyn Fn(&Record) -> bool + Send + Sync>,
}

pub struct MapOperator {
    transform: Box<dyn Fn(Record) -> Record + Send + Sync>,
}

pub struct WindowAggregateOperator {
    window_type: WindowType,
    window_size: Duration,
    aggregates: Vec<AggregateFunction>,
    state: Arc<RwLock<WindowState>>,
}

pub struct JoinOperator {
    join_type: JoinType,
    left_key: String,
    right_key: String,
    window: Option<Duration>,
    left_buffer: Arc<RwLock<HashMap<String, Vec<Record>>>>,
    right_buffer: Arc<RwLock<HashMap<String, Vec<Record>>>>,
}
```

**3. State Management (30h, ~800 LOC)**
```rust
// crates/streamhouse-query/src/state/mod.rs

pub struct StateStore {
    db: Arc<rocksdb::DB>,
}

impl StateStore {
    pub async fn get<K: Serialize, V: DeserializeOwned>(
        &self,
        key: &K,
    ) -> Result<Option<V>> {
        let key_bytes = bincode::serialize(key)?;
        let value_bytes = self.db.get(&key_bytes)?;
        
        match value_bytes {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }
    
    pub async fn put<K: Serialize, V: Serialize>(
        &self,
        key: &K,
        value: &V,
    ) -> Result<()> {
        let key_bytes = bincode::serialize(key)?;
        let value_bytes = bincode::serialize(value)?;
        self.db.put(&key_bytes, &value_bytes)?;
        Ok(())
    }
}
```

**4. Watermark & Time (20h, ~400 LOC)**
```rust
// crates/streamhouse-query/src/time/mod.rs

pub struct WatermarkGenerator {
    watermark: Arc<AtomicU64>,
    max_out_of_order: Duration,
}

impl WatermarkGenerator {
    pub fn update(&self, event_time: u64) {
        let watermark = event_time.saturating_sub(self.max_out_of_order.as_millis() as u64);
        self.watermark.store(watermark, Ordering::Relaxed);
    }
    
    pub fn current(&self) -> u64 {
        self.watermark.load(Ordering::Relaxed)
    }
}
```

**5. REST API (10h, ~300 LOC)**
```rust
// REST endpoints for query management

POST   /api/v1/queries              // Submit query
GET    /api/v1/queries              // List queries
GET    /api/v1/queries/:id          // Get query status
DELETE /api/v1/queries/:id          // Stop query
GET    /api/v1/queries/:id/results  // Get query results (if materialized)
```

---

## Implementation Plan

### Phase 19.1: Foundation (40h)
- [ ] Set up streamhouse-query crate
- [ ] Integrate DataFusion for SQL parsing
- [ ] Build basic query coordinator
- [ ] Implement Map and Filter operators
- [ ] Add input/output connectors
- [ ] Create REST API endpoints

**Deliverable:** Basic SELECT with WHERE clause works

### Phase 19.2: Aggregations (50h)
- [ ] Implement tumbling window operator
- [ ] Add sliding window support
- [ ] Build state management with RocksDB
- [ ] Implement common aggregates (COUNT, SUM, AVG, MIN, MAX)
- [ ] Add watermark generation
- [ ] Handle late data

**Deliverable:** Windowed aggregations work

### Phase 19.3: Joins (40h)
- [ ] Implement stream-table join
- [ ] Add windowed stream-stream join
- [ ] Build join state buffering
- [ ] Add left/right/inner/outer join types
- [ ] Optimize join performance

**Deliverable:** All join types work

### Phase 19.4: Polish & Testing (30h)
- [ ] Add session windows
- [ ] Implement HAVING clause
- [ ] Add DISTINCT support
- [ ] Build comprehensive test suite
- [ ] Performance benchmarks
- [ ] Documentation and examples

**Deliverable:** Production-ready stream processing

---

## Testing Strategy

### Unit Tests
```rust
#[tokio::test]
async fn test_filter_operator() {
    let mut op = FilterOperator::new(|r| r.get_i64("amount") > 1000);
    
    let record = Record::new()
        .with_field("amount", 1500);
    
    let results = op.process(record).await.unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_window_aggregate() {
    let mut op = WindowAggregateOperator::new(
        WindowType::Tumbling,
        Duration::from_secs(60),
        vec![AggregateFunction::Count],
    );
    
    // Send records spanning 2 minutes
    // Verify 2 window results produced
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_end_to_end_query() {
    let coordinator = setup_test_coordinator().await;
    
    // Submit query
    let query_id = coordinator.submit_query(
        "CREATE STREAM high_value AS SELECT * FROM orders WHERE amount > 1000"
    ).await.unwrap();
    
    // Produce test data
    produce_test_orders(100).await;
    
    // Verify output stream
    let results = consume_stream("high_value", 10).await;
    assert!(results.iter().all(|r| r.amount > 1000));
}
```

---

## Performance Targets

- **Throughput:** >50K events/sec per query
- **Latency:** <100ms p99 for simple queries
- **State Size:** Support 10GB+ state per query
- **Queries:** Support 100+ concurrent queries per node

---

## Success Criteria

- [ ] Can execute basic SELECT queries on streams
- [ ] Tumbling window aggregations work correctly
- [ ] Stream-table joins produce correct results
- [ ] State survives process restart
- [ ] Watermarks handle out-of-order data
- [ ] REST API for query management works
- [ ] Performance meets targets
- [ ] Documentation complete with 10+ examples

---

## Future Enhancements

- Exactly-once processing guarantees
- Distributed query execution
- SQL IDE / query builder UI
- Machine learning integration
- Graph processing
- Change data capture (CDC) support

---

**Recommendation:** Start with DataFusion + custom streaming layer approach
**Effort:** 120 hours over 3-4 weeks
**Priority after:** Phase 13-15 complete
