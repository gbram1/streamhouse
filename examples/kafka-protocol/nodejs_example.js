/**
 * StreamHouse Kafka Protocol - Node.js Examples
 *
 * Requirements:
 *   npm install kafkajs
 *
 * Usage:
 *   # Start StreamHouse first:
 *   USE_LOCAL_STORAGE=1 ./target/release/unified-server
 *
 *   # Run examples:
 *   node nodejs_example.js <example>
 */

const { Kafka, logLevel } = require('kafkajs');

const BOOTSTRAP_SERVERS = 'localhost:9092';

// Create Kafka client
const kafka = new Kafka({
  clientId: 'streamhouse-nodejs-example',
  brokers: [BOOTSTRAP_SERVERS],
  logLevel: logLevel.WARN,
});

// =============================================================================
// BASIC PRODUCER
// =============================================================================

async function basicProducer() {
  const producer = kafka.producer();
  await producer.connect();

  const result = await producer.send({
    topic: 'nodejs-topic',
    messages: [
      { value: 'Hello from Node.js!' },
    ],
  });

  console.log('Sent message:', result);
  await producer.disconnect();
}

// =============================================================================
// PRODUCER WITH KEYS AND HEADERS
// =============================================================================

async function producerWithKeys() {
  const producer = kafka.producer();
  await producer.connect();

  await producer.send({
    topic: 'keyed-topic',
    messages: [
      {
        key: 'user-123',
        value: JSON.stringify({ action: 'login', timestamp: Date.now() }),
        headers: {
          'content-type': 'application/json',
          'source': 'web-app',
        },
      },
      {
        key: 'user-456',
        value: JSON.stringify({ action: 'purchase', item: 'widget' }),
        headers: {
          'content-type': 'application/json',
        },
      },
    ],
  });

  console.log('Sent keyed messages with headers');
  await producer.disconnect();
}

// =============================================================================
// BATCH PRODUCER
// =============================================================================

async function batchProducer() {
  const producer = kafka.producer({
    allowAutoTopicCreation: true,
    transactionTimeout: 30000,
  });

  await producer.connect();

  // Send many messages in batch
  const messages = Array.from({ length: 100 }, (_, i) => ({
    key: `key-${i}`,
    value: `Message ${i}`,
  }));

  await producer.send({
    topic: 'batch-topic',
    messages,
  });

  console.log('Sent 100 messages in batch');
  await producer.disconnect();
}

// =============================================================================
// BASIC CONSUMER
// =============================================================================

async function basicConsumer() {
  const consumer = kafka.consumer({ groupId: 'nodejs-group' });

  await consumer.connect();
  await consumer.subscribe({ topic: 'nodejs-topic', fromBeginning: true });

  console.log('Consuming messages (Ctrl+C to stop)...');

  await consumer.run({
    eachMessage: async ({ topic, partition, message }) => {
      console.log({
        topic,
        partition,
        offset: message.offset,
        key: message.key?.toString(),
        value: message.value.toString(),
      });
    },
  });
}

// =============================================================================
// CONSUMER WITH BATCH PROCESSING
// =============================================================================

async function batchConsumer() {
  const consumer = kafka.consumer({ groupId: 'batch-consumer-group' });

  await consumer.connect();
  await consumer.subscribe({ topic: 'batch-topic', fromBeginning: true });

  await consumer.run({
    eachBatch: async ({ batch, resolveOffset, heartbeat }) => {
      console.log(`Received batch of ${batch.messages.length} messages`);

      for (const message of batch.messages) {
        console.log(`  Processing: ${message.value.toString()}`);
        resolveOffset(message.offset);
        await heartbeat();
      }
    },
  });
}

// =============================================================================
// CONSUMER WITH MANUAL COMMITS
// =============================================================================

async function manualCommitConsumer() {
  const consumer = kafka.consumer({
    groupId: 'manual-commit-group',
  });

  await consumer.connect();
  await consumer.subscribe({ topic: 'important-topic', fromBeginning: true });

  await consumer.run({
    autoCommit: false,  // Disable auto-commit
    eachMessage: async ({ topic, partition, message }) => {
      try {
        // Process the message
        console.log(`Processing: ${message.value.toString()}`);

        // Commit only after successful processing
        await consumer.commitOffsets([
          { topic, partition, offset: (parseInt(message.offset) + 1).toString() },
        ]);
      } catch (error) {
        console.error('Processing failed:', error);
        // Don't commit - message will be reprocessed
      }
    },
  });
}

// =============================================================================
// PRODUCER + CONSUMER ROUND-TRIP
// =============================================================================

async function roundtrip() {
  const topic = 'roundtrip-nodejs';

  // 1. Produce messages
  console.log('1. Producing messages...');
  const producer = kafka.producer();
  await producer.connect();

  const messages = Array.from({ length: 5 }, (_, i) => ({
    key: `key-${i}`,
    value: JSON.stringify({ index: i, timestamp: Date.now() }),
  }));

  await producer.send({ topic, messages });
  await producer.disconnect();
  console.log('   Produced 5 messages');

  // 2. Wait for flush
  console.log('2. Waiting for flush to storage (8s)...');
  await new Promise(resolve => setTimeout(resolve, 8000));

  // 3. Consume messages
  console.log('3. Consuming messages...');
  const consumer = kafka.consumer({ groupId: 'roundtrip-group-' + Date.now() });
  await consumer.connect();
  await consumer.subscribe({ topic, fromBeginning: true });

  let count = 0;
  const timeout = setTimeout(async () => {
    console.log(`   Consumed ${count} messages`);
    await consumer.disconnect();
    process.exit(0);
  }, 5000);

  await consumer.run({
    eachMessage: async ({ message }) => {
      count++;
      console.log(`   Received: key=${message.key}, value=${message.value.toString()}`);
    },
  });
}

// =============================================================================
// ADMIN OPERATIONS
// =============================================================================

async function adminOperations() {
  const admin = kafka.admin();
  await admin.connect();

  // List topics
  const topics = await admin.listTopics();
  console.log('Existing topics:', topics);

  // Create a topic
  try {
    await admin.createTopics({
      topics: [
        {
          topic: 'admin-created-topic',
          numPartitions: 3,
          replicationFactor: 1,
        },
      ],
    });
    console.log('Created topic: admin-created-topic');
  } catch (error) {
    console.log('Topic already exists or error:', error.message);
  }

  // Get topic metadata
  const metadata = await admin.fetchTopicMetadata({ topics: ['admin-created-topic'] });
  console.log('Topic metadata:', JSON.stringify(metadata, null, 2));

  // List consumer groups
  const groups = await admin.listGroups();
  console.log('Consumer groups:', groups);

  await admin.disconnect();
}

// =============================================================================
// SEEK TO OFFSET
// =============================================================================

async function seekExample() {
  const consumer = kafka.consumer({ groupId: 'seek-group-' + Date.now() });

  await consumer.connect();
  await consumer.subscribe({ topic: 'nodejs-topic', fromBeginning: true });

  // Seek to specific offset after subscription
  consumer.on(consumer.events.GROUP_JOIN, async () => {
    // Seek to offset 0 on partition 0
    await consumer.seek({ topic: 'nodejs-topic', partition: 0, offset: '0' });
    console.log('Seeked to offset 0');
  });

  await consumer.run({
    eachMessage: async ({ partition, message }) => {
      console.log(`Partition ${partition}, Offset ${message.offset}: ${message.value.toString()}`);
    },
  });
}

// =============================================================================
// MAIN
// =============================================================================

const examples = {
  'basic-producer': basicProducer,
  'producer-keys': producerWithKeys,
  'batch-producer': batchProducer,
  'basic-consumer': basicConsumer,
  'batch-consumer': batchConsumer,
  'manual-commit': manualCommitConsumer,
  'roundtrip': roundtrip,
  'admin': adminOperations,
  'seek': seekExample,
};

async function main() {
  const example = process.argv[2];

  if (!example) {
    console.log('StreamHouse Kafka Protocol - Node.js Examples');
    console.log('='.repeat(50));
    console.log('\nUsage: node nodejs_example.js <example>');
    console.log('\nAvailable examples:');
    Object.keys(examples).forEach(name => {
      console.log(`  - ${name}`);
    });
    console.log('\nExample: node nodejs_example.js roundtrip');
    process.exit(0);
  }

  if (!examples[example]) {
    console.log(`Unknown example: ${example}`);
    console.log(`Available: ${Object.keys(examples).join(', ')}`);
    process.exit(1);
  }

  console.log(`\nRunning example: ${example}`);
  console.log('-'.repeat(50));

  try {
    await examples[example]();
  } catch (error) {
    console.error('Error:', error);
    process.exit(1);
  }
}

main();
