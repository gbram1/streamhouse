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

## Response Format

```json
{
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
  ]
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

## Tips

1. **Be specific**: "Show me the 5 most recent orders" works better than "show orders"

2. **Mention topics**: Include topic names in your question for better accuracy

3. **Check the SQL**: The response includes the generated SQL so you can verify it's correct

4. **Use suggestions**: The AI provides suggestions for refining your query

5. **JSON fields**: Mention specific field names if you know them: "Show the user_id field from orders"
