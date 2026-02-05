# StreamHouse TypeScript/JavaScript Client

Type-safe client for the StreamHouse streaming data platform.

## Installation

```bash
npm install streamhouse
# or
yarn add streamhouse
# or
pnpm add streamhouse
```

## Requirements

- Node.js 18+ (for native fetch support) or provide a custom fetch implementation
- TypeScript 4.7+ (optional, for type support)

## Quick Start

```typescript
import { StreamHouseClient } from 'streamhouse';

const client = new StreamHouseClient('http://localhost:8080');

// Create a topic
const topic = await client.createTopic('events', 3);
console.log(`Created topic: ${topic.name}`);

// Produce a message
const result = await client.produce('events', '{"event": "click", "user": "123"}');
console.log(`Produced to partition ${result.partition} at offset ${result.offset}`);

// Produce batch
const batchResult = await client.produceBatch('events', [
  '{"event": "click"}',
  '{"event": "scroll"}',
  { key: 'user-123', value: '{"event": "purchase"}' },
]);
console.log(`Produced ${batchResult.count} messages`);

// Consume messages
const messages = await client.consume('events', 0, { offset: 0, maxRecords: 10 });
for (const record of messages.records) {
  console.log(`[${record.partition}:${record.offset}] ${record.value}`);
}

// Execute SQL query
const queryResult = await client.query('SELECT * FROM events LIMIT 10');
for (const row of queryResult.rows) {
  console.log(row);
}

// List topics
const topics = await client.listTopics();
for (const t of topics) {
  console.log(`${t.name}: ${t.partitions} partitions`);
}
```

## With Authentication

```typescript
import { StreamHouseClient } from 'streamhouse';

const client = new StreamHouseClient({
  baseUrl: 'http://localhost:8080',
  apiKey: 'sk_live_your_api_key_here',
});

// All operations will use the API key
const topics = await client.listTopics();
```

## Custom Fetch (Node.js < 18)

For Node.js versions without native fetch:

```typescript
import fetch from 'node-fetch';
import { StreamHouseClient } from 'streamhouse';

const client = new StreamHouseClient({
  baseUrl: 'http://localhost:8080',
  fetch: fetch as unknown as typeof globalThis.fetch,
});
```

## API Reference

### Topic Operations

```typescript
// List all topics
const topics = await client.listTopics();

// Create a new topic
const topic = await client.createTopic('events', 3, 1);

// Get topic details
const topic = await client.getTopic('events');

// Delete a topic
await client.deleteTopic('events');

// List partitions
const partitions = await client.listPartitions('events');
```

### Producer Operations

```typescript
// Produce a single message
const result = await client.produce('events', '{"data": "value"}');

// Produce with key and partition
const result = await client.produce('events', '{"data": "value"}', {
  key: 'user-123',
  partition: 0,
});

// Batch produce
const result = await client.produceBatch('events', [
  '{"event": "a"}',
  { value: '{"event": "b"}', key: 'key1' },
]);
```

### Consumer Operations

```typescript
// Consume messages
const result = await client.consume('events', 0);

// Consume with options
const result = await client.consume('events', 0, {
  offset: 100,
  maxRecords: 50,
});

// Access consumed records
for (const record of result.records) {
  console.log(record.partition, record.offset, record.key, record.value);
}
```

### Consumer Group Operations

```typescript
// List consumer groups
const groups = await client.listConsumerGroups();

// Get consumer group details
const group = await client.getConsumerGroup('my-group');

// Commit offset
await client.commitOffset('my-group', 'events', 0, 100);

// Delete consumer group
await client.deleteConsumerGroup('my-group');
```

### SQL Operations

```typescript
// Execute SQL query
const result = await client.query('SELECT * FROM events WHERE key = "user-123"');

// With timeout
const result = await client.query('SELECT COUNT(*) FROM events', {
  timeoutMs: 60000,
});

// Access results
console.log(result.columns); // Column info
console.log(result.rows);    // Data rows
console.log(result.rowCount);
console.log(result.executionTimeMs);
```

### Cluster Operations

```typescript
// List agents
const agents = await client.listAgents();

// Get metrics
const metrics = await client.getMetrics();

// Health check
const isHealthy = await client.healthCheck();
```

## Error Handling

```typescript
import {
  StreamHouseClient,
  NotFoundError,
  ValidationError,
  ConflictError,
  TimeoutError,
} from 'streamhouse';

const client = new StreamHouseClient('http://localhost:8080');

try {
  const topic = await client.getTopic('nonexistent');
} catch (error) {
  if (error instanceof NotFoundError) {
    console.log('Topic not found');
  }
}

try {
  await client.createTopic('events', 0); // Invalid
} catch (error) {
  if (error instanceof ValidationError) {
    console.log('Validation error:', error.message);
  }
}

try {
  await client.createTopic('events', 3);
  await client.createTopic('events', 3); // Already exists
} catch (error) {
  if (error instanceof ConflictError) {
    console.log('Topic already exists');
  }
}
```

## Types

The client exports all TypeScript types:

```typescript
import type {
  Topic,
  Partition,
  ProduceResult,
  BatchRecord,
  BatchProduceResult,
  ConsumedRecord,
  ConsumeResult,
  ConsumerGroup,
  ConsumerGroupDetail,
  Agent,
  SqlResult,
  MetricsSnapshot,
} from 'streamhouse';
```

## Development

```bash
# Install dependencies
npm install

# Build
npm run build

# Run tests
npm test

# Lint
npm run lint

# Format
npm run format
```

## License

Apache 2.0
