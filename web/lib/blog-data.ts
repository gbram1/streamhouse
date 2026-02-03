export interface BlogPost {
  slug: string;
  title: string;
  excerpt: string;
  content: string;
  author: {
    name: string;
    avatar: string;
    role: string;
  };
  publishedAt: string;
  readingTime: string;
  category: string;
  tags: string[];
  featured?: boolean;
}

export const blogPosts: BlogPost[] = [
  {
    slug: "phase-7-release",
    title: "StreamHouse Phase 7: SQL Stream Processing and Beyond",
    excerpt:
      "Announcing Phase 7 with built-in SQL stream processing, enhanced observability, and 40% performance improvements. Transform your streaming data with familiar SQL syntax.",
    content: `
## Introducing Phase 7

We're thrilled to announce StreamHouse Phase 7, our most significant release yet. This update brings SQL stream processing directly into the core platform, eliminating the need for separate processing clusters like Flink or Spark Streaming.

### SQL Stream Processing

Write familiar SQL to transform your streaming data in real-time:

\`\`\`sql
CREATE STREAM user_events_enriched AS
SELECT
  u.user_id,
  u.event_type,
  p.plan_name,
  p.mrr
FROM user_events u
JOIN profiles p ON u.user_id = p.user_id
WHERE u.event_type IN ('purchase', 'upgrade')
\`\`\`

The SQL engine runs directly within StreamHouse agents, processing events with sub-millisecond latency. No external dependencies, no additional infrastructure to manage.

### Performance Improvements

Phase 7 delivers substantial performance gains across the board:

- **40% faster writes** through optimized segment buffering
- **60% reduction in S3 API calls** via intelligent batching
- **25% lower memory usage** with improved LRU cache eviction

These improvements translate directly to cost savings—our benchmark tests show an average 35% reduction in infrastructure costs for high-throughput workloads.

### Enhanced Observability

New Grafana dashboards provide unprecedented visibility into your streaming infrastructure:

- Real-time throughput and latency metrics
- Consumer lag visualization with alerting
- S3 operation breakdowns
- Agent health and resource utilization

### Breaking Changes

Phase 7 maintains full backward compatibility with existing topics and consumers. However, if you're using the experimental SQL features from Phase 6, please review the migration guide.

### Getting Started

Upgrade to Phase 7 today:

\`\`\`bash
cargo install streamhouse --version 0.7.0
\`\`\`

Or pull the latest Docker image:

\`\`\`bash
docker pull streamhouse/agent:0.7.0
\`\`\`

Check out the [documentation](/docs) for detailed upgrade instructions and new feature guides.

---

We're incredibly grateful to our community for their feedback and contributions. Phase 7 wouldn't be possible without you. Join us on [Discord](https://discord.gg/streamhouse) to share your thoughts and get help from the team.
    `,
    author: {
      name: "Alex Chen",
      avatar: "AC",
      role: "Lead Engineer",
    },
    publishedAt: "2024-01-15",
    readingTime: "5 min read",
    category: "Release",
    tags: ["release", "sql", "performance", "observability"],
    featured: true,
  },
  {
    slug: "why-s3-native-streaming",
    title: "Why We Built an S3-Native Streaming Platform",
    excerpt:
      "Traditional message brokers weren't designed for the cloud. Here's why we rethought streaming from first principles with object storage at the core.",
    content: `
## The Problem with Traditional Brokers

When we started building StreamHouse, we asked ourselves a simple question: if you were designing a streaming platform today, with cloud-native infrastructure available, would you still use local disks?

The answer was clearly no.

### The Hidden Costs of Disk-Based Streaming

Traditional brokers like Kafka were designed in an era when local SSDs were the fastest storage option. But this architecture carries significant hidden costs in cloud environments:

**1. Replication Overhead**

Kafka replicates data 3x across brokers for durability. In AWS, this means:
- 3x the EBS storage costs
- Significant inter-AZ network traffic (expensive)
- Complex leader election and ISR management

**2. Capacity Planning Nightmares**

With disk-attached brokers, you must provision for peak load:
- Over-provision to handle traffic spikes
- Under-utilize most of the time
- Manual rebalancing when adding capacity

**3. Operational Complexity**

Disk-based brokers require constant attention:
- Monitor disk usage and IOPS
- Plan partition migrations
- Handle broker failures and recovery

### The S3 Insight

Amazon S3 changed everything. For $0.023/GB/month, you get:
- 11 nines of durability (99.999999999%)
- Unlimited capacity
- No replication management
- Pay only for what you store

The question became: can we build a streaming platform that leverages S3 as the source of truth?

### How StreamHouse Works

StreamHouse uses a disaggregated architecture:

\`\`\`
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Producers  │────▶│   Agents    │────▶│     S3      │
└─────────────┘     └─────────────┘     └─────────────┘
                          │
                          ▼
                    ┌─────────────┐
                    │  Metadata   │
                    │  (Postgres) │
                    └─────────────┘
\`\`\`

**Agents** are stateless compute nodes that:
- Buffer incoming events in memory
- Flush segments to S3 periodically
- Serve reads from cache or S3

**Metadata Store** (PostgreSQL) tracks only:
- Topic and partition configurations
- Consumer group offsets
- Agent leases and watermarks

No event data touches the metadata store. This keeps it lightweight and fast.

### The Results

StreamHouse delivers:
- **80% lower costs** vs. traditional Kafka deployments
- **Zero disk management** - no provisioning, no monitoring
- **Instant scaling** - add agents without data migration
- **Built-in durability** - S3's 11 nines out of the box

### Trade-offs

S3-native streaming isn't without trade-offs:

**Latency**: S3 writes add 50-100ms latency vs. local disk. For most use cases, this is acceptable. For ultra-low-latency requirements, hybrid architectures work well.

**S3 API costs**: High-throughput workloads can incur S3 API charges. StreamHouse mitigates this with intelligent batching and caching.

### Conclusion

The cloud changed the economics of storage. StreamHouse embraces this reality by building on S3 from day one. The result is a simpler, cheaper, and more reliable streaming platform.

Try it yourself—[get started free](/console).
    `,
    author: {
      name: "Sarah Kim",
      avatar: "SK",
      role: "Co-founder & CTO",
    },
    publishedAt: "2024-01-08",
    readingTime: "7 min read",
    category: "Architecture",
    tags: ["architecture", "s3", "cloud-native", "design"],
  },
  {
    slug: "migrating-from-kafka",
    title: "Migrating from Kafka to StreamHouse: A Step-by-Step Guide",
    excerpt:
      "A practical guide to migrating your existing Kafka workloads to StreamHouse with zero downtime and minimal code changes.",
    content: `
## Overview

Migrating from Kafka to StreamHouse is straightforward thanks to our Kafka-compatible protocol. This guide walks through the process step by step.

### Prerequisites

Before starting, ensure you have:
- StreamHouse 0.6+ deployed
- Access to your Kafka cluster
- Producer/consumer applications ready to update

### Step 1: Deploy StreamHouse

Start with a minimal StreamHouse deployment:

\`\`\`bash
# Start infrastructure
docker compose up -d minio postgres

# Run an agent
cargo run --bin agent
\`\`\`

### Step 2: Create Topics

Mirror your Kafka topic configuration:

\`\`\`bash
streamctl topics create orders --partitions 8
streamctl topics create events --partitions 16
\`\`\`

### Step 3: Set Up Dual-Write

Update producers to write to both Kafka and StreamHouse:

\`\`\`rust
// Pseudo-code for dual-write
async fn produce(event: Event) {
    // Write to Kafka (existing)
    kafka_producer.send(event.clone()).await?;

    // Write to StreamHouse (new)
    streamhouse_producer.send(event).await?;
}
\`\`\`

Monitor both systems to ensure data consistency.

### Step 4: Migrate Consumers

Once dual-write is stable, migrate consumers one at a time:

\`\`\`rust
// Update connection string
let consumer = StreamHouseConsumer::new(
    "streamhouse://agent:9090",
    "orders",
    "order-processor"
).await?;
\`\`\`

### Step 5: Disable Kafka Writes

After all consumers are migrated, remove Kafka from producers:

\`\`\`rust
async fn produce(event: Event) {
    streamhouse_producer.send(event).await?;
}
\`\`\`

### Step 6: Decommission Kafka

Once confident in StreamHouse:
1. Stop Kafka consumers
2. Verify no active producers
3. Backup Kafka data if needed
4. Shut down Kafka cluster

### Common Issues

**Consumer Lag After Migration**

If you see consumer lag after migration, it's likely due to offset differences. Reset offsets to earliest:

\`\`\`bash
streamctl consumer-groups reset order-processor --topic orders --to-earliest
\`\`\`

**Missing Messages**

Enable dual-read temporarily to verify data consistency:

\`\`\`rust
let kafka_msg = kafka_consumer.poll().await?;
let sh_msg = streamhouse_consumer.poll().await?;
assert_eq!(kafka_msg.payload, sh_msg.payload);
\`\`\`

### Performance Tuning

StreamHouse defaults work well for most workloads, but you may want to tune:

\`\`\`toml
[agent]
segment_flush_interval = "5s"  # Increase for better batching
cache_size_mb = 512            # Increase for read-heavy workloads
\`\`\`

### Conclusion

Migration from Kafka to StreamHouse can be completed in a few hours for simple deployments, or a few days for complex production systems. The key is the dual-write phase—take your time to verify data consistency before cutting over.

Questions? Join our [Discord](https://discord.gg/streamhouse) for help.
    `,
    author: {
      name: "Marcus Johnson",
      avatar: "MJ",
      role: "Developer Advocate",
    },
    publishedAt: "2024-01-02",
    readingTime: "6 min read",
    category: "Tutorial",
    tags: ["kafka", "migration", "tutorial", "getting-started"],
  },
  {
    slug: "real-time-analytics-pipeline",
    title: "Building a Real-Time Analytics Pipeline with StreamHouse",
    excerpt:
      "Learn how to build an end-to-end real-time analytics pipeline using StreamHouse SQL processing, from ingestion to dashboard.",
    content: `
## Introduction

Real-time analytics pipelines traditionally require multiple systems: Kafka for ingestion, Flink for processing, and a database for serving. StreamHouse consolidates this into a single platform.

### Architecture Overview

\`\`\`
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Events    │────▶│ StreamHouse │────▶│  Dashboard  │
│   (API)     │     │    SQL      │     │  (Grafana)  │
└─────────────┘     └─────────────┘     └─────────────┘
\`\`\`

### Step 1: Set Up Event Ingestion

Create a topic for raw events:

\`\`\`bash
streamctl topics create raw_events --partitions 4
\`\`\`

Produce events from your API:

\`\`\`javascript
// Express middleware
app.use((req, res, next) => {
  producer.send('raw_events', {
    timestamp: Date.now(),
    path: req.path,
    method: req.method,
    user_id: req.user?.id,
    response_time_ms: res.responseTime
  });
  next();
});
\`\`\`

### Step 2: Create Aggregation Streams

Use SQL to compute real-time aggregates:

\`\`\`sql
-- Requests per minute by endpoint
CREATE STREAM rpm_by_endpoint AS
SELECT
  path,
  COUNT(*) as request_count,
  AVG(response_time_ms) as avg_latency,
  TUMBLE_START(timestamp, INTERVAL '1' MINUTE) as window_start
FROM raw_events
GROUP BY
  path,
  TUMBLE(timestamp, INTERVAL '1' MINUTE);
\`\`\`

### Step 3: Connect to Grafana

StreamHouse exposes a Prometheus endpoint for metrics:

\`\`\`yaml
# prometheus.yml
scrape_configs:
  - job_name: 'streamhouse'
    static_configs:
      - targets: ['agent:8080']
\`\`\`

Create Grafana dashboards to visualize your streams.

### Step 4: Add Alerting

Configure alerts for anomalies:

\`\`\`sql
CREATE STREAM high_latency_alerts AS
SELECT *
FROM rpm_by_endpoint
WHERE avg_latency > 500;
\`\`\`

Connect alerts to PagerDuty or Slack via webhooks.

### Performance Considerations

For high-throughput analytics:

1. **Partition wisely**: Use high-cardinality keys for even distribution
2. **Window size**: Larger windows reduce output volume but increase latency
3. **Materialized views**: Cache expensive aggregations

### Conclusion

StreamHouse SQL processing eliminates the need for separate stream processing infrastructure. Build real-time analytics pipelines with familiar SQL syntax and minimal operational overhead.

[Read the full tutorial →](/docs/tutorials/analytics-pipeline)
    `,
    author: {
      name: "Emily Zhang",
      avatar: "EZ",
      role: "Solutions Architect",
    },
    publishedAt: "2023-12-20",
    readingTime: "8 min read",
    category: "Tutorial",
    tags: ["analytics", "sql", "grafana", "tutorial"],
  },
];

export function getBlogPost(slug: string): BlogPost | undefined {
  return blogPosts.find((post) => post.slug === slug);
}

export function getAllBlogPosts(): BlogPost[] {
  return blogPosts.sort(
    (a, b) => new Date(b.publishedAt).getTime() - new Date(a.publishedAt).getTime()
  );
}

export function getFeaturedPost(): BlogPost | undefined {
  return blogPosts.find((post) => post.featured);
}

export function getPostsByCategory(category: string): BlogPost[] {
  return blogPosts.filter((post) => post.category === category);
}
