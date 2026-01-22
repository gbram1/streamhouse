# Deep Dive Part 3: DataFusion Integration for Streaming SQL

## What is DataFusion?

Apache DataFusion is a Rust query engine built on Apache Arrow. It provides:
- SQL parser and planner
- Query optimization
- Parallel execution
- Extensibility via traits

**Why use it?** Don't rewrite SQL parsing and optimization. Focus on streaming-specific parts.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    YOUR SYSTEM                              │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │                  DataFusion Core                      │  │
│  │  SQL Parser ──▶ Planner ──▶ Optimizer ──▶ Executor   │  │
│  └──────────────────────────────────────────────────────┘  │
│                           │                                 │
│           You extend these points:                          │
│                           ▼                                 │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Your Custom Components                   │  │
│  │                                                       │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │  │
│  │  │ Stream      │  │ Window      │  │ Aggregate   │  │  │
│  │  │ Table       │  │ Functions   │  │ With State  │  │  │
│  │  │ Provider    │  │ (TUMBLE,    │  │ (RocksDB)   │  │  │
│  │  │             │  │  HOP)       │  │             │  │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## Step 1: Custom Table Provider

This is how DataFusion reads from your storage layer.

```rust
use datafusion::catalog::{TableProvider, TableType};
use datafusion::physical_plan::ExecutionPlan;
use datafusion::arrow::datatypes::SchemaRef;
use async_trait::async_trait;

pub struct StreamTableProvider {
    name: String,
    schema: SchemaRef,
    topic: String,
    storage: Arc<dyn Storage>,
}

#[async_trait]
impl TableProvider for StreamTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    
    fn table_type(&self) -> TableType {
        TableType::Base
    }
    
    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Ok(Arc::new(StreamScanExec::new(
            self.schema.clone(),
            self.topic.clone(),
            self.storage.clone(),
            projection.cloned(),
            limit,
        )))
    }
}
```

---

## Step 2: Streaming Execution Plan

Unlike batch queries, streaming queries run forever:

```rust
pub struct StreamScanExec {
    schema: SchemaRef,
    topic: String,
    storage: Arc<dyn Storage>,
    projection: Option<Vec<usize>>,
}

impl ExecutionPlan for StreamScanExec {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    
    fn output_partitioning(&self) -> Partitioning {
        Partitioning::UnknownPartitioning(1)
    }
    
    fn children(&self) -> Vec<Arc<dyn ExecutionPlan>> {
        vec![]  // Leaf node
    }
    
    fn execute(
        &self,
        _partition: usize,
        _context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        Ok(Box::pin(StreamScanStream::new(
            self.schema.clone(),
            self.topic.clone(),
            self.storage.clone(),
        )))
    }
}

pub struct StreamScanStream {
    schema: SchemaRef,
    topic: String,
    storage: Arc<dyn Storage>,
    current_offset: u64,
}

impl Stream for StreamScanStream {
    type Item = Result<RecordBatch>;
    
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // Try to read next batch
        match self.storage.try_read(&self.topic, self.current_offset, 1000) {
            Some((batch, new_offset)) => {
                self.current_offset = new_offset;
                Poll::Ready(Some(Ok(batch)))
            }
            None => {
                // No data yet - schedule wake up and return Pending
                // This is how streaming works: we wait for new data
                let waker = cx.waker().clone();
                self.storage.notify_on_data(self.current_offset, waker);
                Poll::Pending
            }
        }
    }
}
```

---

## Step 3: Window Functions

Streaming SQL needs special window functions like TUMBLE and HOP.

### SQL Syntax

```sql
-- Tumbling window: non-overlapping, fixed size
SELECT
    TUMBLE_START(event_time, INTERVAL '1 hour') as window_start,
    TUMBLE_END(event_time, INTERVAL '1 hour') as window_end,
    SUM(amount) as total
FROM orders
GROUP BY TUMBLE(event_time, INTERVAL '1 hour');

-- Hopping window: overlapping windows
SELECT
    HOP_START(event_time, INTERVAL '5 min', INTERVAL '1 hour') as window_start,
    AVG(amount) as avg_amount
FROM orders
GROUP BY HOP(event_time, INTERVAL '5 min', INTERVAL '1 hour');
```

### Register Custom Functions

```rust
use datafusion::logical_expr::{ScalarUDF, Volatility};

pub fn register_streaming_functions(ctx: &SessionContext) {
    // TUMBLE function
    let tumble = ScalarUDF::new(
        "tumble",
        &Signature::exact(
            vec![DataType::Timestamp(TimeUnit::Millisecond, None), DataType::Interval(IntervalUnit::DayTime)],
            Volatility::Immutable,
        ),
        &|args| {
            // Returns struct with window_start and window_end
            let timestamps = args[0].as_primitive::<TimestampMillisecondType>();
            let interval_ms = extract_interval_ms(&args[1]);
            
            let mut starts = Vec::with_capacity(timestamps.len());
            let mut ends = Vec::with_capacity(timestamps.len());
            
            for ts in timestamps.iter() {
                if let Some(ts) = ts {
                    let window_start = (ts / interval_ms) * interval_ms;
                    let window_end = window_start + interval_ms;
                    starts.push(Some(window_start));
                    ends.push(Some(window_end));
                } else {
                    starts.push(None);
                    ends.push(None);
                }
            }
            
            Ok(ColumnarValue::Array(Arc::new(StructArray::from(vec![
                ("window_start", Arc::new(TimestampMillisecondArray::from(starts)) as ArrayRef),
                ("window_end", Arc::new(TimestampMillisecondArray::from(ends)) as ArrayRef),
            ]))))
        },
    );
    
    ctx.register_udf(tumble);
    
    // TUMBLE_START helper
    ctx.register_udf(ScalarUDF::new(
        "tumble_start",
        &Signature::exact(
            vec![DataType::Timestamp(TimeUnit::Millisecond, None), DataType::Interval(IntervalUnit::DayTime)],
            Volatility::Immutable,
        ),
        &|args| {
            let timestamps = args[0].as_primitive::<TimestampMillisecondType>();
            let interval_ms = extract_interval_ms(&args[1]);
            
            let result: TimestampMillisecondArray = timestamps
                .iter()
                .map(|ts| ts.map(|t| (t / interval_ms) * interval_ms))
                .collect();
            
            Ok(ColumnarValue::Array(Arc::new(result)))
        },
    ));
}
```

---

## Step 4: Stateful Aggregation

Streaming aggregations need to maintain state across batches.

### The Problem

```sql
SELECT customer_id, SUM(amount) 
FROM orders 
GROUP BY customer_id;
```

In batch: process all data, emit results.
In streaming: data never ends. Must maintain running totals.

### Solution: State Store

```rust
pub trait StateStore: Send + Sync {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn put(&self, key: &[u8], value: &[u8]);
    fn delete(&self, key: &[u8]);
    fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)>;
}

pub struct RocksDBStateStore {
    db: rocksdb::DB,
}

impl StateStore for RocksDBStateStore {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.db.get(key).ok().flatten()
    }
    
    fn put(&self, key: &[u8], value: &[u8]) {
        self.db.put(key, value).unwrap();
    }
    
    fn delete(&self, key: &[u8]) {
        self.db.delete(key).unwrap();
    }
    
    fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut results = vec![];
        let iter = self.db.prefix_iterator(prefix);
        for item in iter {
            if let Ok((k, v)) = item {
                if k.starts_with(prefix) {
                    results.push((k.to_vec(), v.to_vec()));
                } else {
                    break;
                }
            }
        }
        results
    }
}
```

### Stateful Aggregate Operator

```rust
pub struct StreamingAggregateExec {
    input: Arc<dyn ExecutionPlan>,
    group_by: Vec<Expr>,
    aggregates: Vec<AggregateExpr>,
    state_store: Arc<dyn StateStore>,
}

impl StreamingAggregateExec {
    pub fn process_batch(&self, batch: RecordBatch) -> Result<RecordBatch> {
        // Group records by key
        let groups = self.group_by_key(&batch)?;
        
        for (group_key, indices) in groups {
            // Load existing state
            let state_key = self.make_state_key(&group_key);
            let mut accumulators = match self.state_store.get(&state_key) {
                Some(bytes) => self.deserialize_accumulators(&bytes),
                None => self.create_accumulators(),
            };
            
            // Update accumulators with new data
            for idx in indices {
                for (acc, agg) in accumulators.iter_mut().zip(&self.aggregates) {
                    let value = agg.evaluate_row(&batch, idx)?;
                    acc.update(&value)?;
                }
            }
            
            // Save state
            let bytes = self.serialize_accumulators(&accumulators);
            self.state_store.put(&state_key, &bytes);
        }
        
        // Emit current results (or wait for window close)
        self.emit_results()
    }
}
```

---

## Step 5: Watermarks

Watermarks track event-time progress and trigger window emission.

### Concept

```
Event Time:  |--1--|--2--|--3--|--4--|--5--|--6--|--7--|
             ^                    ^
             |                    |
           Events               Watermark = 4
           arrive               "I've seen all events up to 4"
           out of               
           order                Windows for times <= 4 can close
```

### Implementation

```rust
pub struct WatermarkTracker {
    current_watermark: i64,
    max_out_of_orderness: Duration,
    max_observed_time: i64,
}

impl WatermarkTracker {
    pub fn observe(&mut self, event_time: i64) {
        self.max_observed_time = self.max_observed_time.max(event_time);
    }
    
    pub fn advance(&mut self) -> Option<i64> {
        let potential = self.max_observed_time - self.max_out_of_orderness.as_millis() as i64;
        
        if potential > self.current_watermark {
            self.current_watermark = potential;
            Some(self.current_watermark)
        } else {
            None
        }
    }
    
    pub fn watermark(&self) -> i64 {
        self.current_watermark
    }
}
```

### Triggering Window Emission

```rust
pub struct TumblingWindowOperator {
    window_size_ms: i64,
    group_by: Vec<String>,
    aggregates: Vec<AggregateExpr>,
    state_store: Arc<dyn StateStore>,
    watermark: WatermarkTracker,
}

impl TumblingWindowOperator {
    pub fn process(&mut self, batch: RecordBatch) -> Vec<RecordBatch> {
        let mut output = vec![];
        
        // Update watermark
        let event_times = batch.column_by_name("event_time").unwrap();
        for ts in event_times.as_primitive::<TimestampMillisecondType>().iter() {
            if let Some(ts) = ts {
                self.watermark.observe(ts);
            }
        }
        
        // Assign records to windows and update state
        for row_idx in 0..batch.num_rows() {
            let event_time = get_timestamp(&batch, "event_time", row_idx);
            let window_start = (event_time / self.window_size_ms) * self.window_size_ms;
            let window_end = window_start + self.window_size_ms;
            
            let group_key = self.extract_group_key(&batch, row_idx);
            let window_key = WindowKey { window_start, group_key };
            
            // Update aggregates for this window
            self.update_window_state(&window_key, &batch, row_idx);
        }
        
        // Check for windows to emit
        if let Some(new_watermark) = self.watermark.advance() {
            // Emit all windows with window_end <= watermark
            let completed_windows = self.find_completed_windows(new_watermark);
            for window_key in completed_windows {
                let result = self.emit_window(&window_key);
                output.push(result);
                self.delete_window_state(&window_key);
            }
        }
        
        output
    }
}
```

---

## Step 6: Putting It Together

### CREATE STREAM Statement

```rust
// Custom statement
pub struct CreateStream {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub topic: String,
    pub format: String,
    pub timestamp_field: Option<String>,
}

// Parse it
fn parse_create_stream(sql: &str) -> Result<CreateStream> {
    // CREATE STREAM orders (
    //     order_id VARCHAR,
    //     amount DECIMAL,
    //     event_time TIMESTAMP
    // ) WITH (
    //     topic = 'orders',
    //     format = 'json',
    //     timestamp_field = 'event_time'
    // );
    
    // Use sqlparser to get the basic structure, then extract custom parts
    // ...
}

// Execute it
async fn execute_create_stream(stmt: CreateStream, ctx: &mut Context) {
    let schema = build_schema(&stmt.columns);
    
    let table = StreamTableProvider::new(
        stmt.name.clone(),
        Arc::new(schema),
        stmt.topic,
        ctx.storage.clone(),
    );
    
    ctx.register_table(&stmt.name, Arc::new(table));
}
```

### Full Query Flow

```sql
-- User writes:
CREATE STREAM orders (
    order_id VARCHAR,
    customer_id VARCHAR,
    amount DECIMAL,
    event_time TIMESTAMP
) WITH (topic = 'orders', format = 'json');

CREATE MATERIALIZED VIEW hourly_sales AS
SELECT
    TUMBLE_START(event_time, INTERVAL '1 hour') as hour,
    SUM(amount) as total_sales,
    COUNT(*) as order_count
FROM orders
GROUP BY TUMBLE(event_time, INTERVAL '1 hour');
```

```rust
// Your system:
// 1. Parse CREATE STREAM -> register StreamTableProvider
// 2. Parse CREATE MATERIALIZED VIEW -> build streaming pipeline
// 3. Pipeline runs continuously:
//    - StreamScanExec reads from topic
//    - TumblingWindowOperator maintains state
//    - Results written to output topic or queryable view
```

---

## Key Takeaways

1. **Use DataFusion for SQL parsing and optimization** - don't reinvent the wheel
2. **Implement custom TableProvider** - connects DataFusion to your storage
3. **Implement streaming ExecutionPlan** - returns Pending when no data
4. **Add window functions** - TUMBLE, HOP for time-based aggregation
5. **Use RocksDB for state** - survives restarts, enables exactly-once
6. **Track watermarks** - trigger window emission correctly

---

## Resources

- DataFusion docs: https://datafusion.apache.org/
- DataFusion examples: https://github.com/apache/datafusion/tree/main/datafusion-examples
- RisingWave (similar approach): https://github.com/risingwavelabs/risingwave
- Arroyo (similar approach): https://github.com/ArroyoSystems/arroyo
