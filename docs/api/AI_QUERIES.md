# AI-Powered Natural Language Queries

StreamHouse supports natural language queries that are automatically converted to SQL using Claude AI.

## Setup

1. Get an API key from [Anthropic Console](https://console.anthropic.com)

2. Set the environment variable:
   ```bash
   export ANTHROPIC_API_KEY=sk-ant-...
   ```

3. For Docker, add to `.env`:
   ```
   ANTHROPIC_API_KEY=sk-ant-...
   ```

## Endpoints

### Check AI Status

```bash
curl http://localhost:8080/api/v1/ai/health
```

Response:
```json
{
  "status": "configured",
  "provider": "anthropic",
  "model": "claude-sonnet-4-20250514"
}
```

### Ask a Question

```bash
curl -X POST http://localhost:8080/api/v1/query/ask \
  -H "Content-Type: application/json" \
  -d '{
    "question": "How many orders do we have?",
    "topics": ["orders"]
  }'
```

## Request Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `question` | string | Yes | Natural language question about your data |
| `topics` | string[] | No | Topics to query (defaults to all topics) |
| `execute` | boolean | No | Execute the generated SQL (default: true) |
| `timeout_ms` | number | No | Query timeout in milliseconds (default: 30000) |
| `saveHistory` | boolean | No | Save query to history (default: true) |

## Response Format

```json
{
  "queryId": "550e8400-e29b-41d4-a716-446655440000",
  "question": "How many orders do we have?",
  "sql": "SELECT COUNT(*) as total_orders FROM orders LIMIT 10000",
  "explanation": "This query counts the total number of messages in the orders topic.",
  "results": {
    "columns": ["count"],
    "rows": [[20004]],
    "rowCount": 1,
    "executionTimeMs": 5,
    "truncated": false
  },
  "topicsUsed": ["orders"],
  "confidence": 0.95,
  "suggestions": [
    "Consider filtering by timestamp if you want orders from a specific time period"
  ],
  "costEstimate": {
    "estimatedRows": 20004,
    "estimatedBytes": 400336,
    "topics": ["orders"],
    "estimatedTimeMs": 10,
    "costTier": "low",
    "warnings": [],
    "suggestions": ["Consider filtering by partition for better performance"]
  }
}
```

## Examples

### Count Records

```bash
curl -X POST http://localhost:8080/api/v1/query/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "How many events are there?"}'
```

### Filter by Partition

```bash
curl -X POST http://localhost:8080/api/v1/query/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "Show me the first 10 events from partition 0", "topics": ["events"]}'
```

### Recent Records

```bash
curl -X POST http://localhost:8080/api/v1/query/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "Show me the 5 most recent orders"}'
```

### Extract JSON Fields

```bash
curl -X POST http://localhost:8080/api/v1/query/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "Show order IDs and amounts from the last 10 orders", "topics": ["orders"]}'
```

### Generate SQL Only (No Execution)

```bash
curl -X POST http://localhost:8080/api/v1/query/ask \
  -H "Content-Type: application/json" \
  -d '{
    "question": "Group orders by region and count them",
    "execute": false
  }'
```

## Available SQL Features

The AI generates SQL compatible with StreamHouse's query engine:

| Feature | Supported | Example |
|---------|-----------|---------|
| SELECT | Yes | `SELECT key, value, timestamp` |
| WHERE | Yes | `WHERE partition = 0` |
| ORDER BY | Yes | `ORDER BY timestamp DESC` |
| LIMIT | Yes (required) | `LIMIT 100` |
| JSON extraction | Yes | `json_extract(value, '$.field')` |
| COUNT/SUM/AVG | Not yet | Coming in future release |
| GROUP BY | Not yet | Coming in future release |
| JOIN | Not yet | Coming in future release |

## Column Reference

All StreamHouse topics have these standard columns:

| Column | Type | Description |
|--------|------|-------------|
| `key` | STRING | Message key |
| `value` | JSON | Message value (use `json_extract` for fields) |
| `partition` | INTEGER | Partition ID |
| `offset` | BIGINT | Message offset within partition |
| `timestamp` | TIMESTAMP | Message timestamp (epoch ms) |
| `headers` | JSON | Message headers |

## Error Handling

### AI Not Configured

```json
{
  "error": "ai_not_configured",
  "message": "ANTHROPIC_API_KEY environment variable not set",
  "suggestions": [
    "Set ANTHROPIC_API_KEY environment variable",
    "Get an API key from https://console.anthropic.com"
  ]
}
```

### SQL Execution Error

```json
{
  "error": "sql_execution_error",
  "message": "Generated SQL failed to execute: Unsupported function: COUNT",
  "sql": "SELECT partition, COUNT(*) FROM events GROUP BY partition",
  "suggestions": [
    "The generated SQL may have syntax errors",
    "Try rephrasing your question"
  ]
}
```

## Query History

Queries are automatically saved to history (unless `saveHistory: false`). You can retrieve, refine, and manage your query history.

### List Query History

```bash
curl http://localhost:8080/api/v1/query/history?limit=10
```

Response:
```json
{
  "queries": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "question": "How many orders do we have?",
      "sql": "SELECT COUNT(*) FROM orders LIMIT 10000",
      "explanation": "Counts all orders",
      "topicsUsed": ["orders"],
      "confidence": 0.95,
      "createdAt": 1738674000000,
      "parentId": null,
      "refinementCount": 0
    }
  ],
  "total": 1
}
```

### Get Specific Query

```bash
curl http://localhost:8080/api/v1/query/history/550e8400-e29b-41d4-a716-446655440000
```

### Delete Query from History

```bash
curl -X DELETE http://localhost:8080/api/v1/query/history/550e8400-e29b-41d4-a716-446655440000
```

### Clear All History

```bash
curl -X DELETE http://localhost:8080/api/v1/query/history
```

## Query Refinement

Refine a previous query by ID. The AI will modify the original query based on your instructions.

```bash
curl -X POST http://localhost:8080/api/v1/query/history/550e8400-e29b-41d4-a716-446655440000/refine \
  -H "Content-Type: application/json" \
  -d '{
    "refinement": "Add a filter for orders over $100"
  }'
```

The response includes the refined query with a new `queryId`. Refinements track their parent query via `parentId` and increment `refinementCount`.

## Cost Estimation

Estimate query cost before execution:

```bash
curl -X POST http://localhost:8080/api/v1/query/estimate \
  -H "Content-Type: application/json" \
  -d '{
    "query": "Show me all orders from today",
    "isSql": false,
    "topics": ["orders"]
  }'
```

For SQL queries:
```bash
curl -X POST http://localhost:8080/api/v1/query/estimate \
  -H "Content-Type: application/json" \
  -d '{
    "query": "SELECT * FROM orders WHERE timestamp > 1738600000000 LIMIT 1000",
    "isSql": true
  }'
```

Response:
```json
{
  "sql": "SELECT * FROM orders WHERE timestamp > 1738600000000 LIMIT 1000",
  "estimatedRows": 1000,
  "estimatedBytes": 50000,
  "topics": ["orders"],
  "estimatedTimeMs": 10,
  "costTier": "low",
  "warnings": [],
  "suggestions": ["Consider filtering by partition for better performance"]
}
```

### Cost Tiers

| Tier | Estimated Rows | Description |
|------|---------------|-------------|
| `low` | < 10,000 | Fast queries, minimal resource usage |
| `medium` | 10,000 - 100,000 | Moderate queries |
| `high` | > 100,000 | Heavy queries, consider adding filters |

## Tips

1. **Be specific**: "Show me the 5 most recent orders" works better than "show orders"

2. **Mention topics**: Include topic names in your question for better accuracy

3. **Check the SQL**: The response includes the generated SQL so you can verify it's correct

4. **Use suggestions**: The AI provides suggestions for refining your query

5. **JSON fields**: Mention specific field names if you know them: "Show the user_id field from orders"

6. **Use refinement**: Start with a simple query, then refine it iteratively

7. **Check cost first**: Use `/query/estimate` before running expensive queries

## Schema Inference

Automatically infer the JSON schema from topic messages with AI-powered field descriptions and index recommendations.

### Infer Schema

```bash
curl -X POST http://localhost:8080/api/v1/schema/infer \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "orders",
    "sampleSize": 100,
    "generateDescriptions": true
  }'
```

### Request Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `topic` | string | Yes | Topic name to analyze |
| `sampleSize` | number | No | Number of messages to sample (default: 100, max: 1000) |
| `generateDescriptions` | boolean | No | Generate AI descriptions for fields (default: true) |

### Response Format

```json
{
  "topic": "orders",
  "sampleCount": 100,
  "fields": [
    {
      "path": "user_id",
      "jsonType": "string",
      "nullable": false,
      "occurrenceRate": 1.0,
      "uniqueValues": 45,
      "sampleValues": ["usr_123", "usr_456", "usr_789"],
      "description": "Unique identifier for the user who placed the order",
      "suggestedSqlType": "VARCHAR"
    },
    {
      "path": "amount",
      "jsonType": "number",
      "nullable": false,
      "occurrenceRate": 1.0,
      "uniqueValues": 78,
      "sampleValues": [99.99, 150.00, 24.50],
      "description": "Order total amount in dollars",
      "suggestedSqlType": "DOUBLE"
    },
    {
      "path": "items",
      "jsonType": "array<object>",
      "nullable": false,
      "occurrenceRate": 1.0,
      "uniqueValues": null,
      "sampleValues": [],
      "description": "List of items in the order",
      "suggestedSqlType": "JSON"
    }
  ],
  "indexRecommendations": [
    {
      "field": "user_id",
      "reason": "ID field - commonly used for lookups",
      "priority": "high",
      "exampleQuery": "SELECT * FROM orders WHERE json_extract(value, '$.user_id') = 'usr_123' LIMIT 100"
    },
    {
      "field": "status",
      "reason": "Low cardinality field - good for filtering",
      "priority": "medium",
      "exampleQuery": "SELECT * FROM orders WHERE json_extract(value, '$.status') = 'completed' LIMIT 100"
    },
    {
      "field": "created_at",
      "reason": "Timestamp field - useful for time-range queries",
      "priority": "high",
      "exampleQuery": "SELECT * FROM orders WHERE json_extract(value, '$.created_at') >= '2026-01-01' LIMIT 100"
    }
  ],
  "confidence": 0.95,
  "summary": "Orders topic containing e-commerce order data with user information, order items, amounts, and status tracking."
}
```

### Field Types Detected

| JSON Type | SQL Type | Description |
|-----------|----------|-------------|
| `string` | VARCHAR | Text values |
| `integer` | BIGINT | Whole numbers |
| `number` | DOUBLE | Decimal numbers |
| `boolean` | BOOLEAN | True/false values |
| `array` | JSON | Arrays of values |
| `object` | JSON | Nested objects |
| `null` | VARCHAR | Null values only |

### Index Priority Levels

| Priority | Meaning |
|----------|---------|
| `high` | Strongly recommended - ID fields, timestamps |
| `medium` | Useful - status/type fields with low cardinality |
| `low` | Optional - other frequently accessed fields |
