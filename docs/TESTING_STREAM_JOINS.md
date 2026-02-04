# Testing Stream JOINs

This guide covers how to test the Stream JOIN functionality in StreamHouse.

## Running Unit Tests

```bash
# Run all SQL crate tests (includes JOIN tests)
cargo test -p streamhouse-sql --lib

# Run only JOIN-related tests
cargo test -p streamhouse-sql --lib join

# Run with output for debugging
cargo test -p streamhouse-sql --lib join -- --nocapture
```

**Expected output:** 52 tests passing (as of Phase 24 completion)

---

## Test Coverage

### Parser Tests (`parser.rs`)

| Test | What it verifies |
|------|-----------------|
| `test_parse_inner_join` | INNER JOIN parsing with aliases |
| `test_parse_left_join` | LEFT JOIN type detection |
| `test_parse_right_join` | RIGHT JOIN type detection |
| `test_parse_full_join` | FULL OUTER JOIN parsing |
| `test_parse_join_with_json_extract` | JSON paths in ON clause |
| `test_parse_join_on_key` | Simple key-based joins |
| `test_parse_join_with_wildcard` | SELECT * in JOIN |
| `test_parse_join_with_qualified_wildcard` | SELECT o.* in JOIN |
| `test_parse_stream_table_join` | TABLE() syntax on right |
| `test_parse_table_stream_join` | TABLE() syntax on left |
| `test_parse_stream_table_join_with_json` | TABLE() with json_extract |
| `test_parse_join_with_where_filter` | WHERE clause with JOIN |

---

## Manual Testing with SQL Workbench

### 1. Start the System

```bash
# Start all services
docker-compose up -d

# Or run locally
cargo run --bin streamhouse-server
```

### 2. Create Test Topics and Data

```bash
# Create topics
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "orders", "partitions": 1}'

curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "users", "partitions": 1}'

# Produce test messages to orders
curl -X POST http://localhost:8080/api/v1/topics/orders/produce \
  -H "Content-Type: application/json" \
  -d '{"messages": [
    {"key": "order-1", "value": "{\"user_id\": \"user-1\", \"amount\": 100}"},
    {"key": "order-2", "value": "{\"user_id\": \"user-2\", \"amount\": 250}"},
    {"key": "order-3", "value": "{\"user_id\": \"user-1\", \"amount\": 75}"}
  ]}'

# Produce test messages to users
curl -X POST http://localhost:8080/api/v1/topics/users/produce \
  -H "Content-Type: application/json" \
  -d '{"messages": [
    {"key": "user-1", "value": "{\"name\": \"Alice\", \"email\": \"alice@example.com\"}"},
    {"key": "user-2", "value": "{\"name\": \"Bob\", \"email\": \"bob@example.com\"}"}
  ]}'
```

### 3. Test Queries via SQL API

```bash
# Stream-Stream INNER JOIN
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "SELECT o.key, json_extract(o.value, '\''$.amount'\'') as amount, json_extract(u.value, '\''$.name'\'') as customer FROM orders o INNER JOIN users u ON json_extract(o.value, '\''$.user_id'\'') = u.key LIMIT 10",
    "timeoutMs": 30000
  }'

# LEFT JOIN (shows orders without matching users)
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "SELECT o.key, u.key as user_key FROM orders o LEFT JOIN users u ON json_extract(o.value, '\''$.user_id'\'') = u.key LIMIT 10",
    "timeoutMs": 30000
  }'

# Stream-Table JOIN (O(1) lookups)
curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "SELECT o.key, json_extract(u.value, '\''$.name'\'') as customer FROM orders o JOIN TABLE(users) u ON json_extract(o.value, '\''$.user_id'\'') = u.key LIMIT 10",
    "timeoutMs": 30000
  }'
```

### 4. Test via Web UI

1. Navigate to `http://localhost:3000/sql`
2. Click example queries: "INNER JOIN", "LEFT JOIN", "Stream-Table JOIN"
3. Modify and execute custom queries

---

## Test Scenarios

### Stream-Stream JOIN Scenarios

```sql
-- Basic INNER JOIN
SELECT o.key, u.key
FROM orders o
INNER JOIN users u ON o.key = u.key
LIMIT 100;

-- LEFT JOIN (all orders, matched users)
SELECT o.key, u.key
FROM orders o
LEFT JOIN users u ON json_extract(o.value, '$.user_id') = u.key
LIMIT 100;

-- RIGHT JOIN (all users, matched orders)
SELECT o.key, u.key
FROM orders o
RIGHT JOIN users u ON json_extract(o.value, '$.user_id') = u.key
LIMIT 100;

-- FULL OUTER JOIN (all from both)
SELECT o.key, u.key
FROM orders o
FULL OUTER JOIN users u ON o.key = u.key
LIMIT 100;

-- JOIN with json_extract on both sides
SELECT o.key, u.key
FROM orders o
JOIN users u ON json_extract(o.value, '$.user_id') = json_extract(u.value, '$.id')
LIMIT 50;
```

### Stream-Table JOIN Scenarios

```sql
-- TABLE() on right side (typical enrichment)
SELECT o.key, json_extract(u.value, '$.name') as customer
FROM orders o
JOIN TABLE(users) u ON json_extract(o.value, '$.user_id') = u.key
LIMIT 50;

-- TABLE() on left side
SELECT u.key, o.key as order_key
FROM TABLE(users) u
LEFT JOIN orders o ON u.key = json_extract(o.value, '$.user_id')
LIMIT 50;
```

### Predicate Pushdown Scenarios

```sql
-- Filter by partition (pushdown optimization)
SELECT o.key, u.key
FROM orders o
JOIN users u ON o.key = u.key
WHERE partition = 0
LIMIT 100;

-- Filter by offset range
SELECT o.key, u.key
FROM orders o
JOIN users u ON o.key = u.key
WHERE offset >= 0 AND offset < 1000
LIMIT 100;
```

---

## Expected Results

### INNER JOIN
- Returns only rows where both sides match
- No NULL values in result

### LEFT JOIN
- Returns all left rows
- NULL in right columns when no match

### RIGHT JOIN
- Returns all right rows
- NULL in left columns when no match

### FULL OUTER JOIN
- Returns all rows from both sides
- NULL where no match on either side

### Stream-Table JOIN
- Same join semantics as stream-stream
- Right side keeps only latest value per key
- O(1) lookups instead of index scan

---

## Debugging

### Check Parser Output

```rust
// In tests, add debug output:
let query = parse_query("SELECT o.key FROM orders o JOIN users u ON o.key = u.key").unwrap();
println!("{:?}", query);
```

### Check Executor Behavior

```bash
# Enable debug logging
RUST_LOG=streamhouse_sql=debug cargo test -p streamhouse-sql -- --nocapture
```

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "TopicNotFound" | Topic doesn't exist | Create topic first |
| "Timeout" | Query taking too long | Add LIMIT, reduce data |
| Empty results | No matching keys | Check join condition |
| Parse error | Invalid SQL syntax | Check TABLE() syntax |

---

## Performance Testing

### Large Dataset Test

```bash
# Generate 10K orders
for i in $(seq 1 10000); do
  curl -X POST http://localhost:8080/api/v1/topics/orders/produce \
    -H "Content-Type: application/json" \
    -d "{\"messages\": [{\"key\": \"order-$i\", \"value\": \"{\\\"user_id\\\": \\\"user-$((i % 100))\\\", \\\"amount\\\": $i}\"}]}"
done

# Time a JOIN query
time curl -X POST http://localhost:8080/api/v1/sql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT COUNT(*) FROM orders o JOIN users u ON json_extract(o.value, '\''$.user_id'\'') = u.key", "timeoutMs": 30000}'
```

### Metrics to Monitor

- `executionTimeMs` in response
- Memory usage during large joins
- Timeout behavior with 30s limit

---

## Adding New Tests

### Parser Test Template

```rust
#[test]
fn test_parse_new_join_feature() {
    let query = parse_query(
        "SELECT o.key FROM orders o JOIN users u ON o.key = u.key"
    ).unwrap();
    match query {
        SqlQuery::Join(q) => {
            assert_eq!(q.left.topic, "orders");
            assert_eq!(q.right.topic, "users");
            // Add assertions
        }
        _ => panic!("Expected Join query"),
    }
}
```

### Integration Test Template

```rust
#[tokio::test]
async fn test_join_execution() {
    let executor = setup_test_executor().await;

    // Create test topics and data
    // ...

    let result = executor.execute(
        "SELECT o.key FROM orders o JOIN users u ON o.key = u.key LIMIT 10",
        30000
    ).await.unwrap();

    assert!(result.row_count > 0);
}
```

---

## Related Files

| File | Purpose |
|------|---------|
| `crates/streamhouse-sql/src/types.rs` | JOIN types (JoinType, TableRef, etc.) |
| `crates/streamhouse-sql/src/parser.rs` | SQL parsing for JOINs |
| `crates/streamhouse-sql/src/executor.rs` | JOIN execution engine |
| `web/app/sql/page.tsx` | SQL Workbench UI |
| `docs/STREAMING_SQL.md` | Full SQL documentation |
