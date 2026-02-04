# StreamHouse Streaming SQL Reference

StreamHouse provides a powerful SQL interface for querying streaming data with built-in support for anomaly detection, window aggregations, and vector similarity search.

## Overview

The SQL engine enables real-time analytics on streaming data without requiring a separate analytical database. Key capabilities include:

- **Standard SQL queries** on message topics
- **Anomaly detection** with z-score and statistical functions
- **Window aggregations** with TUMBLE, HOP, and SESSION windows
- **Vector similarity search** for RAG and semantic search applications

## Basic Queries

### Select Messages

```sql
-- Select all columns
SELECT * FROM orders LIMIT 100;

-- Select specific columns
SELECT key, offset, timestamp FROM orders LIMIT 100;

-- Filter by partition and offset
SELECT * FROM orders
WHERE partition = 0 AND offset >= 100 AND offset < 200;

-- Filter by key
SELECT * FROM orders WHERE key = 'customer-123' LIMIT 50;
```

### JSON Extraction

Messages are stored as JSON. Use `json_extract` to access nested fields:

```sql
SELECT
  json_extract(value, '$.customer_id') as customer_id,
  json_extract(value, '$.amount') as amount,
  json_extract(value, '$.items[0].name') as first_item
FROM orders
LIMIT 100;
```

### Topic Commands

```sql
-- List all topics
SHOW TOPICS;

-- Describe a topic's partitions
DESCRIBE orders;

-- Count messages
SELECT COUNT(*) FROM orders WHERE partition = 0;
```

---

## Anomaly Detection

StreamHouse includes statistical functions for detecting anomalies in streaming data. These functions compute statistics across the result set and identify outliers.

### Z-Score Calculation

The `zscore()` function calculates how many standard deviations a value is from the mean:

```sql
SELECT
  offset,
  json_extract(value, '$.amount') as amount,
  zscore(json_extract(value, '$.amount')) as z_score
FROM orders
LIMIT 1000;
```

**Interpretation:**
- Z-score of 0 = exactly at the mean
- Z-score of ±1 = within 1 standard deviation (68% of data)
- Z-score of ±2 = within 2 standard deviations (95% of data)
- Z-score > ±2 = potential outlier
- Z-score > ±3 = likely anomaly (99.7% confidence)

### Anomaly Detection

The `anomaly()` function returns `true` if a value's absolute z-score exceeds a threshold:

```sql
SELECT
  offset,
  json_extract(value, '$.latency') as latency,
  anomaly(json_extract(value, '$.latency'), 2.0) as is_anomaly
FROM metrics
LIMIT 1000;
```

### Filtering Anomalies

Find only the outliers:

```sql
-- Find high z-score values
SELECT * FROM orders
WHERE zscore(json_extract(value, '$.amount')) > 2.0
LIMIT 100;

-- Find low z-score values
SELECT * FROM orders
WHERE zscore(json_extract(value, '$.amount')) < -2.0
LIMIT 100;

-- Find anomalies (high or low)
SELECT * FROM metrics
WHERE anomaly(json_extract(value, '$.response_time'), 2.5) = true
LIMIT 100;
```

### Moving Average

Detect trends and smooth out noise with moving averages:

```sql
SELECT
  offset,
  json_extract(value, '$.price') as price,
  moving_avg(json_extract(value, '$.price'), 10) as ma_10,
  moving_avg(json_extract(value, '$.price'), 50) as ma_50
FROM stock_prices
LIMIT 500;
```

### Statistical Aggregates

Compute statistics across the dataset:

```sql
SELECT
  avg(json_extract(value, '$.latency')) as avg_latency,
  stddev(json_extract(value, '$.latency')) as stddev_latency
FROM metrics
LIMIT 10000;
```

### Use Cases

| Function | Use Case |
|----------|----------|
| `zscore()` | Fraud detection, outlier identification |
| `anomaly()` | Alerting on unusual values |
| `moving_avg()` | Trend analysis, noise reduction |
| `stddev()` | Volatility measurement |
| `avg()` | Baseline calculation |

---

## Window Aggregations

Window functions group streaming data into time-based buckets for aggregation. This enables real-time analytics like "orders per minute" or "average latency per hour".

### Tumbling Windows (TUMBLE)

Fixed-size, non-overlapping windows. Each event belongs to exactly one window.

```sql
SELECT
  COUNT(*) as order_count,
  SUM(json_extract(value, '$.amount')) as total_amount,
  AVG(json_extract(value, '$.amount')) as avg_amount
FROM orders
GROUP BY TUMBLE(timestamp, '5 minutes');
```

**Result columns:**
- `window_start` - Start timestamp of the window
- `window_end` - End timestamp of the window
- Aggregation columns

### Hopping/Sliding Windows (HOP)

Overlapping windows that slide at a specified interval. Events may belong to multiple windows.

```sql
-- 10-minute windows, sliding every 1 minute
SELECT
  AVG(json_extract(value, '$.latency')) as avg_latency,
  MAX(json_extract(value, '$.latency')) as max_latency,
  MIN(json_extract(value, '$.latency')) as min_latency
FROM metrics
GROUP BY HOP(timestamp, '10 minutes', '1 minute');
```

**Parameters:**
- First: timestamp column
- Second: window size
- Third: slide interval

### Session Windows (SESSION)

Dynamic windows based on activity gaps. A new session starts when there's no activity for the specified gap duration.

```sql
SELECT
  COUNT(*) as events,
  FIRST(json_extract(value, '$.action')) as first_action,
  LAST(json_extract(value, '$.action')) as last_action
FROM user_events
GROUP BY SESSION(timestamp, '30 minutes'), key;
```

**Use cases:**
- User session analysis
- Click stream analytics
- Activity tracking

### Interval Syntax

| Format | Example | Milliseconds |
|--------|---------|--------------|
| Milliseconds | `'100 ms'` | 100 |
| Seconds | `'30 seconds'` | 30,000 |
| Minutes | `'5 minutes'` | 300,000 |
| Hours | `'1 hour'` | 3,600,000 |
| Days | `'1 day'` | 86,400,000 |

### Aggregation Functions

| Function | Description |
|----------|-------------|
| `COUNT(*)` | Count of events in window |
| `COUNT(DISTINCT col)` | Count of unique values |
| `SUM(col)` | Sum of numeric values |
| `AVG(col)` | Average of numeric values |
| `MIN(col)` | Minimum value |
| `MAX(col)` | Maximum value |
| `FIRST(col)` | First value in window |
| `LAST(col)` | Last value in window |

### Grouped Windows

Combine window functions with GROUP BY for per-key aggregations:

```sql
-- Orders per customer per hour
SELECT
  COUNT(*) as orders,
  SUM(json_extract(value, '$.amount')) as total
FROM orders
GROUP BY TUMBLE(timestamp, '1 hour'), key;
```

---

## Stream JOINs

StreamHouse supports joining data between topics using standard SQL JOIN syntax. JOINs operate on time-windowed buffers to handle the streaming nature of the data.

### INNER JOIN

Return only matching records from both topics:

```sql
SELECT
  o.key as order_key,
  json_extract(o.value, '$.amount') as amount,
  json_extract(u.value, '$.name') as customer_name
FROM orders o
INNER JOIN users u ON json_extract(o.value, '$.user_id') = u.key
LIMIT 100;
```

### LEFT JOIN

Return all records from the left topic, with NULLs for non-matching right records:

```sql
SELECT
  o.key,
  json_extract(o.value, '$.amount') as amount,
  json_extract(p.value, '$.status') as payment_status
FROM orders o
LEFT JOIN payments p ON o.key = p.key
LIMIT 100;
```

### RIGHT JOIN

Return all records from the right topic, with NULLs for non-matching left records:

```sql
SELECT
  o.key as order_key,
  u.key as user_key,
  json_extract(u.value, '$.email') as email
FROM orders o
RIGHT JOIN users u ON json_extract(o.value, '$.user_id') = u.key
LIMIT 100;
```

### FULL OUTER JOIN

Return all records from both topics, with NULLs where there are no matches:

```sql
SELECT
  o.key as order_key,
  u.key as user_key,
  COALESCE(json_extract(o.value, '$.amount'), 0) as amount
FROM orders o
FULL OUTER JOIN users u ON o.key = u.key
LIMIT 100;
```

### Join Conditions

JOINs support equality conditions on keys and JSON-extracted fields:

```sql
-- Join on message key
FROM orders o
INNER JOIN users u ON o.key = u.key

-- Join on JSON field
FROM orders o
INNER JOIN users u ON json_extract(o.value, '$.user_id') = u.key

-- Join on two JSON fields
FROM orders o
INNER JOIN inventory i ON json_extract(o.value, '$.product_id') = json_extract(i.value, '$.id')
```

### Qualified Column References

When joining topics, use table aliases to reference columns:

```sql
SELECT
  o.key,           -- key from orders
  o.offset,        -- offset from orders
  o.value,         -- value from orders
  u.key,           -- key from users
  u.value,         -- value from users
  json_extract(o.value, '$.amount') as order_amount,
  json_extract(u.value, '$.name') as user_name
FROM orders o
INNER JOIN users u ON o.key = u.key;
```

### Wildcards in JOINs

Select all columns from one or both topics:

```sql
-- All columns from both topics
SELECT * FROM orders o
INNER JOIN users u ON o.key = u.key;

-- All columns from one topic
SELECT o.*, json_extract(u.value, '$.name') as user_name
FROM orders o
INNER JOIN users u ON o.key = u.key;
```

### Join Implementation

Stream-stream JOINs use a hash join strategy:
- Both topics are loaded into memory buffers
- An index is built on the right-side join key for O(1) lookups
- Results are produced by iterating the left side and probing the right-side index
- Default time window: 1 hour (configurable)

### Join Optimizations

StreamHouse applies several optimizations to JOIN queries:

**Predicate Pushdown**
- WHERE filters on partition, offset, and timestamp are pushed down
- Messages are filtered before the join, reducing memory usage
- Example: `WHERE partition = 0` filters both topics early

**Timeout Handling**
- Queries check for timeout throughout execution
- Timeouts are checked after loading each topic and during projection
- Prevents runaway queries on large datasets

**Example with filters:**
```sql
SELECT o.key, u.key
FROM orders o
JOIN users u ON o.key = u.key
WHERE partition = 0 AND offset >= 1000
LIMIT 100;
```

### Use Cases

| Join Type | Use Case |
|-----------|----------|
| INNER JOIN | Enrich orders with user data |
| LEFT JOIN | Find orders without payments |
| RIGHT JOIN | Find users without orders |
| FULL OUTER JOIN | Complete reconciliation between systems |

---

## Stream-Table JOINs

Stream-Table JOINs allow joining a stream of events against a compacted "table" where only the latest value per key is kept. This enables efficient O(1) lookups for enrichment scenarios.

### TABLE() Syntax

Use `TABLE(topic)` to treat a topic as a compacted table (key→latest value):

```sql
-- Enrich orders with latest user data
SELECT
  o.key as order_key,
  json_extract(o.value, '$.amount') as amount,
  json_extract(u.value, '$.name') as user_name,
  json_extract(u.value, '$.email') as email
FROM orders o
INNER JOIN TABLE(users) u ON json_extract(o.value, '$.user_id') = u.key
LIMIT 100;
```

### Stream-Table vs Stream-Stream

| Feature | Stream-Stream | Stream-Table |
|---------|---------------|--------------|
| Syntax | `JOIN topic` | `JOIN TABLE(topic)` |
| Right side | All messages | Latest per key |
| Duplicates | Multiple matches possible | One match per key |
| Use case | Event correlation | Dimension lookups |
| Performance | Hash index (fast) | O(1) lookups (faster) |

### Examples

**Lookup user details for each order:**
```sql
SELECT
  o.key,
  json_extract(u.value, '$.name') as customer
FROM orders o
JOIN TABLE(users) u ON json_extract(o.value, '$.user_id') = u.key
LIMIT 50;
```

**Enrich events with product information:**
```sql
SELECT
  e.key,
  json_extract(e.value, '$.action') as action,
  json_extract(p.value, '$.name') as product_name,
  json_extract(p.value, '$.price') as price
FROM events e
LEFT JOIN TABLE(products) p ON json_extract(e.value, '$.product_id') = p.key
LIMIT 100;
```

**TABLE() on either side:**
```sql
-- Table on left side
SELECT u.key, o.key
FROM TABLE(users) u
LEFT JOIN orders o ON u.key = json_extract(o.value, '$.user_id')
LIMIT 50;
```

### Table Semantics

When using `TABLE(topic)`:
- Messages are processed in order (by offset)
- Later messages with the same key overwrite earlier ones
- The result is a key→value map with one entry per unique key
- Join lookups are O(1) using this map

---

## Materialized Views

Materialized views provide pre-computed aggregations that are automatically maintained as new data arrives. This enables fast queries on streaming data without re-computing aggregations every time.

### Create Materialized View

```sql
CREATE MATERIALIZED VIEW hourly_sales AS
SELECT
  COUNT(*) as order_count,
  SUM(json_extract(value, '$.amount')) as total_amount,
  AVG(json_extract(value, '$.amount')) as avg_amount
FROM orders
GROUP BY TUMBLE(timestamp, '1 hour');
```

### Create with Refresh Mode

```sql
-- Continuous refresh (default) - update as messages arrive
CREATE MATERIALIZED VIEW realtime_metrics AS
SELECT COUNT(*) FROM events
GROUP BY TUMBLE(timestamp, '5 minutes');

-- Periodic refresh - update on schedule
CREATE MATERIALIZED VIEW daily_summary
WITH (refresh_mode = 'periodic', interval = '1 hour')
AS SELECT COUNT(*) FROM orders
GROUP BY TUMBLE(timestamp, '1 day');

-- Manual refresh - only update on command
CREATE MATERIALIZED VIEW monthly_report
WITH (refresh_mode = 'manual')
AS SELECT SUM(json_extract(value, '$.amount')) FROM orders
GROUP BY TUMBLE(timestamp, '30 days');
```

### Replace Existing View

```sql
CREATE OR REPLACE MATERIALIZED VIEW hourly_sales AS
SELECT COUNT(*), SUM(json_extract(value, '$.amount')) as total
FROM orders
GROUP BY TUMBLE(timestamp, '1 hour');
```

### Show Materialized Views

```sql
SHOW MATERIALIZED VIEWS;
```

Returns: name, source_topic, refresh_mode, status, row_count

### Describe View

```sql
DESCRIBE MATERIALIZED VIEW hourly_sales;
```

Returns: detailed view information including query definition, refresh settings, and status.

### Refresh View

```sql
REFRESH MATERIALIZED VIEW hourly_sales;
```

Manually triggers a refresh for views with `manual` refresh mode.

### Drop View

```sql
DROP MATERIALIZED VIEW hourly_sales;
```

### Refresh Modes

| Mode | Behavior |
|------|----------|
| `continuous` | Updates as new messages arrive (default) |
| `periodic` | Refreshes on a schedule (e.g., every 5 minutes) |
| `manual` | Only refreshes when REFRESH command is issued |

### Use Cases

| Use Case | Refresh Mode | Window Size |
|----------|--------------|-------------|
| Real-time dashboards | continuous | 5-15 minutes |
| Hourly reports | periodic | 1 hour |
| Daily summaries | periodic | 1 day |
| On-demand analytics | manual | varies |

---

## Vector Similarity Search

StreamHouse supports vector operations for semantic search, RAG (Retrieval Augmented Generation), and recommendation systems.

### Cosine Similarity

Find semantically similar documents:

```sql
SELECT
  key,
  json_extract(value, '$.title') as title,
  cosine_similarity(json_extract(value, '$.embedding'), '[0.1, 0.2, 0.3, ...]') as score
FROM documents
ORDER BY score DESC
LIMIT 10;
```

**Properties:**
- Returns values between -1 and 1
- 1.0 = identical direction
- 0.0 = orthogonal (unrelated)
- -1.0 = opposite direction

### Euclidean Distance

Find nearest neighbors in embedding space:

```sql
SELECT
  key,
  json_extract(value, '$.content') as content,
  euclidean_distance(json_extract(value, '$.vector'), '[1.0, 2.0, 3.0]') as distance
FROM embeddings
ORDER BY distance ASC
LIMIT 5;
```

**Properties:**
- Returns values >= 0
- 0.0 = identical vectors
- Lower = more similar

### Dot Product

Useful for recommendation systems with normalized vectors:

```sql
SELECT
  key,
  dot_product(json_extract(value, '$.features'), '[0.5, 0.5, 0.5]') as relevance
FROM items
ORDER BY relevance DESC
LIMIT 10;
```

### Vector Norm

Calculate vector magnitude:

```sql
SELECT
  key,
  vector_norm(json_extract(value, '$.embedding')) as magnitude
FROM documents
LIMIT 100;
```

### Vector Format

Vectors are stored as JSON arrays and queried using string representation:

```json
{"embedding": [0.1, 0.2, 0.3, 0.4, 0.5]}
```

Query vector syntax:
```sql
'[0.1, 0.2, 0.3, 0.4, 0.5]'
```

### RAG Pipeline Example

Build a retrieval-augmented generation pipeline:

```sql
-- 1. Find relevant documents for a query embedding
SELECT
  key,
  json_extract(value, '$.content') as content,
  cosine_similarity(json_extract(value, '$.embedding'), '[query_embedding_here]') as relevance
FROM knowledge_base
ORDER BY relevance DESC
LIMIT 5;

-- 2. Use results as context for LLM generation
```

### Use Cases

| Function | Use Case |
|----------|----------|
| `cosine_similarity()` | Semantic search, document similarity |
| `euclidean_distance()` | K-nearest neighbors, clustering |
| `dot_product()` | Recommendations, ranking |
| `vector_norm()` | Normalization, validation |

---

## Function Reference

### Anomaly Detection Functions

| Function | Syntax | Description |
|----------|--------|-------------|
| `zscore` | `zscore(json_extract(value, '$.field'))` | Z-score (standard deviations from mean) |
| `anomaly` | `anomaly(json_extract(value, '$.field'), threshold)` | Returns true if \|zscore\| > threshold |
| `moving_avg` | `moving_avg(json_extract(value, '$.field'), window_size)` | Moving average over N rows |
| `stddev` | `stddev(json_extract(value, '$.field'))` | Standard deviation |
| `avg` | `avg(json_extract(value, '$.field'))` | Arithmetic mean |

### Window Functions

| Function | Syntax | Description |
|----------|--------|-------------|
| `TUMBLE` | `GROUP BY TUMBLE(timestamp, 'interval')` | Fixed non-overlapping windows |
| `HOP` | `GROUP BY HOP(timestamp, 'size', 'slide')` | Sliding/hopping windows |
| `SESSION` | `GROUP BY SESSION(timestamp, 'gap')` | Activity-based sessions |

### Vector Functions

| Function | Syntax | Description |
|----------|--------|-------------|
| `cosine_similarity` | `cosine_similarity(path, '[vec]')` | Cosine similarity (-1 to 1) |
| `euclidean_distance` | `euclidean_distance(path, '[vec]')` | Euclidean distance (>= 0) |
| `dot_product` | `dot_product(path, '[vec]')` | Dot product |
| `vector_norm` | `vector_norm(path)` | L2 norm (magnitude) |

### Materialized View Commands

| Command | Syntax | Description |
|---------|--------|-------------|
| `CREATE` | `CREATE MATERIALIZED VIEW name AS SELECT...` | Create a new view |
| `CREATE OR REPLACE` | `CREATE OR REPLACE MATERIALIZED VIEW name AS...` | Create or replace view |
| `SHOW` | `SHOW MATERIALIZED VIEWS` | List all views |
| `DESCRIBE` | `DESCRIBE MATERIALIZED VIEW name` | Show view details |
| `REFRESH` | `REFRESH MATERIALIZED VIEW name` | Manually refresh view |
| `DROP` | `DROP MATERIALIZED VIEW name` | Delete a view |

---

## Limitations

- Maximum 10,000 rows per query
- 30 second query timeout
- Read-only queries (no INSERT/UPDATE/DELETE)
- Window functions require GROUP BY
- JOINs use 1-hour time window (in-memory buffer)

## Performance Tips

1. **Filter early**: Use partition and offset filters to reduce scan scope
2. **Limit results**: Always use LIMIT to avoid large result sets
3. **Use specific columns**: Select only needed columns instead of `*`
4. **Window size**: Smaller windows = faster queries
5. **Vector dimensions**: Higher dimensions = slower similarity calculations
