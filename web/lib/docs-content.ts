export interface DocSection {
  id: string;
  title: string;
  content: string;
  code?: string;
  language?: string;
  items?: string[];
}

export interface DocPage {
  title: string;
  description: string;
  readTime: string;
  sectionGroup: string;
  sections: DocSection[];
}

export interface NavSection {
  title: string;
  links: { title: string; href: string }[];
}

export const navigation: NavSection[] = [
  {
    title: "Getting Started",
    links: [
      { title: "Quick Start", href: "/docs/quick-start" },
      { title: "Installation", href: "/docs/installation" },
      { title: "First Topic", href: "/docs/first-topic" },
      { title: "Docker Setup", href: "/docs/docker" },
    ],
  },
  {
    title: "Core Concepts",
    links: [
      { title: "Architecture Overview", href: "/docs/architecture" },
      { title: "Topics & Partitions", href: "/docs/topics" },
      { title: "Producers", href: "/docs/producers" },
      { title: "Consumers", href: "/docs/consumers" },
    ],
  },
  {
    title: "Agents",
    links: [
      { title: "Agent Configuration", href: "/docs/agents/config" },
      { title: "Scaling Agents", href: "/docs/agents/scaling" },
      { title: "Health & Metrics", href: "/docs/agents/metrics" },
      { title: "Lease Management", href: "/docs/agents/leases" },
    ],
  },
  {
    title: "Storage",
    links: [
      { title: "S3 Configuration", href: "/docs/storage/s3" },
      { title: "Segment Format", href: "/docs/storage/segments" },
      { title: "Compression", href: "/docs/storage/compression" },
      { title: "Retention Policies", href: "/docs/storage/retention" },
    ],
  },
  {
    title: "SQL Processing",
    links: [
      { title: "SQL Overview", href: "/docs/sql/overview" },
      { title: "Creating Streams", href: "/docs/sql/streams" },
      { title: "Windowed Aggregations", href: "/docs/sql/windows" },
      { title: "Joins", href: "/docs/sql/joins" },
    ],
  },
  {
    title: "Operations",
    links: [
      { title: "Monitoring", href: "/docs/ops/monitoring" },
      { title: "Alerting", href: "/docs/ops/alerting" },
      { title: "Backup & Recovery", href: "/docs/ops/backup" },
      { title: "Security", href: "/docs/ops/security" },
    ],
  },
  {
    title: "Reference",
    links: [
      { title: "API Reference", href: "/docs/api" },
      { title: "CLI Reference", href: "/docs/cli" },
      { title: "Examples", href: "/docs/examples" },
    ],
  },
];

export const docsContent: Record<string, DocPage> = {
  // ─── Getting Started ───────────────────────────────────────────
  "quick-start": {
    title: "Quick Start",
    description: "Get StreamHouse running in under 5 minutes.",
    readTime: "5 min",
    sectionGroup: "Getting Started",
    sections: [
      {
        id: "prerequisites",
        title: "Prerequisites",
        content:
          "Before you begin, make sure you have the following installed on your system.",
        items: [
          "Rust 1.75+ (for building from source)",
          "Docker and Docker Compose (for containerized setup)",
          "PostgreSQL 15+ (for metadata storage)",
          "AWS CLI configured with S3 access (or MinIO for local development)",
        ],
      },
      {
        id: "install",
        title: "Install StreamHouse",
        content:
          "The fastest way to get started is with Docker Compose, which sets up StreamHouse along with all its dependencies.",
        code: `# Clone the repository
git clone https://github.com/streamhouse/streamhouse.git
cd streamhouse

# Start everything with Docker Compose
docker compose up -d

# Verify the server is running
curl http://localhost:8080/api/health`,
        language: "bash",
      },
      {
        id: "create-topic",
        title: "Create Your First Topic",
        content:
          "Once StreamHouse is running, create a topic using the REST API or the CLI tool.",
        code: `# Using the CLI
streamctl topic create --name events --partitions 3 --replication-factor 1

# Or using the REST API
curl -X POST http://localhost:8080/api/topics \\
  -H "Content-Type: application/json" \\
  -d '{"name": "events", "partitions": 3}'`,
        language: "bash",
      },
      {
        id: "produce-consume",
        title: "Produce and Consume Messages",
        content:
          "Now you can produce messages to your topic and consume them back.",
        code: `# Produce a message
streamctl produce --topic events --message '{"user": "alice", "action": "login"}'

# Consume messages from the beginning
streamctl consume --topic events --from-beginning`,
        language: "bash",
      },
      {
        id: "next-steps",
        title: "Next Steps",
        content:
          "You now have a running StreamHouse instance. Here are some suggested next steps to continue exploring.",
        items: [
          "Read the Architecture Overview to understand how StreamHouse works under the hood",
          "Set up the Web Console at localhost:3000 for a visual interface",
          "Configure S3 storage for production-grade durability",
          "Explore the SQL processing engine for real-time stream transformations",
        ],
      },
    ],
  },

  installation: {
    title: "Installation",
    description: "Detailed installation instructions for all platforms.",
    readTime: "3 min",
    sectionGroup: "Getting Started",
    sections: [
      {
        id: "from-source",
        title: "Building from Source",
        content:
          "StreamHouse is written in Rust and can be compiled from source on any platform with a Rust toolchain.",
        code: `# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/streamhouse/streamhouse.git
cd streamhouse
cargo build --release

# The binary will be at target/release/streamhouse-server
./target/release/streamhouse-server --help`,
        language: "bash",
      },
      {
        id: "docker",
        title: "Docker Image",
        content:
          "Pre-built Docker images are available on Docker Hub for quick deployment.",
        code: `# Pull the latest image
docker pull streamhouse/streamhouse:latest

# Run with default configuration
docker run -p 8080:8080 -p 9092:9092 streamhouse/streamhouse:latest`,
        language: "bash",
      },
      {
        id: "configuration",
        title: "Configuration",
        content:
          "StreamHouse is configured through environment variables or a TOML configuration file. The most important settings are the metadata store connection and S3 bucket configuration.",
        code: `# config.toml
[server]
host = "0.0.0.0"
port = 8080
grpc_port = 9092

[metadata]
backend = "postgres"
url = "postgresql://localhost:5432/streamhouse"

[storage]
backend = "s3"
bucket = "streamhouse-data"
region = "us-east-1"

[storage.cache]
enabled = true
max_size_mb = 512`,
        language: "toml",
      },
      {
        id: "verify",
        title: "Verify Installation",
        content: "After installation, verify that StreamHouse is running correctly.",
        code: `# Check health endpoint
curl http://localhost:8080/api/health

# Expected response:
# {"status":"healthy","version":"0.1.0","uptime_seconds":42}`,
        language: "bash",
      },
    ],
  },

  "first-topic": {
    title: "Your First Topic",
    description: "Learn how to create topics, produce messages, and consume them.",
    readTime: "5 min",
    sectionGroup: "Getting Started",
    sections: [
      {
        id: "what-is-topic",
        title: "What is a Topic?",
        content:
          "A topic is a named, ordered log of events. Topics are divided into partitions for parallel processing. Each message in a topic has an offset that uniquely identifies it within its partition. Unlike traditional message queues, messages in a topic are persisted and can be replayed by consumers.",
      },
      {
        id: "create",
        title: "Creating a Topic",
        content:
          "Topics can be created via the REST API, gRPC, CLI, or the web console. When creating a topic, you specify the number of partitions and optional configuration like retention period and compression.",
        code: `# Create a topic with 6 partitions and 7-day retention
streamctl topic create \\
  --name user-events \\
  --partitions 6 \\
  --retention 7d \\
  --compression lz4`,
        language: "bash",
      },
      {
        id: "produce",
        title: "Producing Messages",
        content:
          "Producers send messages to a specific topic. Messages can optionally include a key, which determines which partition receives the message. Messages with the same key are always routed to the same partition, preserving ordering for related events.",
        code: `# Produce with a key (ensures ordering per user)
streamctl produce --topic user-events \\
  --key "user-123" \\
  --message '{"event": "page_view", "page": "/pricing"}'

# Produce without a key (round-robin across partitions)
streamctl produce --topic user-events \\
  --message '{"event": "heartbeat"}'`,
        language: "bash",
      },
      {
        id: "consume",
        title: "Consuming Messages",
        content:
          "Consumers read messages from topics. They can start from the beginning, the end, or a specific offset. Consumer groups allow multiple consumers to share the work of processing a topic, with each partition assigned to exactly one consumer in the group.",
        code: `# Consume from the beginning
streamctl consume --topic user-events --from-beginning

# Consume as part of a consumer group
streamctl consume --topic user-events \\
  --group analytics-pipeline \\
  --auto-commit`,
        language: "bash",
      },
    ],
  },

  docker: {
    title: "Docker Setup",
    description: "Run StreamHouse with Docker and Docker Compose.",
    readTime: "10 min",
    sectionGroup: "Getting Started",
    sections: [
      {
        id: "compose",
        title: "Docker Compose Setup",
        content:
          "The recommended way to run StreamHouse locally is with Docker Compose. This starts all required services including PostgreSQL for metadata, MinIO for S3-compatible storage, and the StreamHouse server.",
        code: `# docker-compose.yml
version: '3.8'

services:
  streamhouse:
    image: streamhouse/streamhouse:latest
    ports:
      - "8080:8080"
      - "9092:9092"
    environment:
      - METADATA_URL=postgresql://postgres:postgres@postgres:5432/streamhouse
      - S3_ENDPOINT=http://minio:9000
      - S3_BUCKET=streamhouse
      - S3_ACCESS_KEY=minioadmin
      - S3_SECRET_KEY=minioadmin
    depends_on:
      - postgres
      - minio

  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: streamhouse
      POSTGRES_PASSWORD: postgres
    volumes:
      - pgdata:/var/lib/postgresql/data

  minio:
    image: minio/minio
    command: server /data --console-address ":9001"
    ports:
      - "9000:9000"
      - "9001:9001"
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin

  web:
    image: streamhouse/web:latest
    ports:
      - "3000:3000"
    environment:
      - INTERNAL_API_URL=http://streamhouse:8080

volumes:
  pgdata:`,
        language: "yaml",
      },
      {
        id: "start",
        title: "Starting the Stack",
        content: "Start all services with a single command.",
        code: `# Start in background
docker compose up -d

# Check that all services are running
docker compose ps

# View logs
docker compose logs -f streamhouse`,
        language: "bash",
      },
      {
        id: "minio-setup",
        title: "MinIO Bucket Setup",
        content:
          "MinIO provides S3-compatible storage for local development. After starting the stack, create the required bucket.",
        code: `# Create the bucket using MinIO client
docker compose exec minio mc alias set local http://localhost:9000 minioadmin minioadmin
docker compose exec minio mc mb local/streamhouse

# Or use the MinIO console at http://localhost:9001`,
        language: "bash",
      },
      {
        id: "production",
        title: "Production Considerations",
        content:
          "For production deployments, consider using managed services for PostgreSQL and S3 instead of running them in containers.",
        items: [
          "Use Amazon RDS or Aurora for PostgreSQL metadata storage",
          "Use Amazon S3 directly instead of MinIO",
          "Deploy StreamHouse agents behind a load balancer",
          "Configure resource limits and health checks in your orchestrator",
          "Enable TLS for all connections",
        ],
      },
    ],
  },

  // ─── Core Concepts ─────────────────────────────────────────────
  architecture: {
    title: "Architecture Overview",
    description:
      "Understand StreamHouse's disaggregated architecture and how components interact.",
    readTime: "10 min",
    sectionGroup: "Core Concepts",
    sections: [
      {
        id: "overview",
        title: "Design Philosophy",
        content:
          "StreamHouse follows a disaggregated architecture where compute, storage, and metadata are separated into independent layers. This design enables independent scaling of each layer, fast recovery from failures (no local state to rebuild), and dramatic cost reduction by leveraging S3 for storage at $0.023/GB/month compared to $0.10-0.30/GB for Kafka's attached disks.",
      },
      {
        id: "components",
        title: "Core Components",
        content:
          "StreamHouse consists of three main components that work together to provide a complete streaming platform.",
        items: [
          "Stateless Agents: Handle produce and consume requests, buffer writes in memory, and flush segments to S3. Agents can be added or removed without data rebalancing.",
          "S3 Storage: All event data is stored as immutable segments in S3 with 99.999999999% durability. Segments are compressed with LZ4 and typically 64MB in size.",
          "Metadata Store: PostgreSQL stores topic definitions, partition assignments, segment locations, and consumer group offsets. This is the only stateful component.",
        ],
      },
      {
        id: "data-flow",
        title: "Data Flow",
        content:
          "When a producer sends a message, it reaches a StreamHouse agent via gRPC or HTTP. The agent buffers the message in an in-memory write pool organized by partition. When the buffer reaches 64MB or a time threshold, the agent compresses the segment with LZ4 and uploads it to S3. The segment's location and offset range are then registered in the metadata store. When a consumer requests messages, the agent looks up which segments contain the requested offset range, fetches and decompresses them from S3 (with local caching), and streams the records back to the consumer.",
      },
      {
        id: "caching",
        title: "Caching Architecture",
        content:
          "StreamHouse uses a multi-level caching strategy to minimize latency and reduce load on external dependencies.",
        items: [
          "Metadata Cache: LRU cache with topic metadata (5 min TTL) and partition info (30s TTL) to reduce PostgreSQL queries",
          "Segment Cache: Local disk and memory cache for recently accessed S3 segments, critical for consumer tail reads",
          "Schema Cache: In-memory cache for schema registry lookups (10,000 schemas, 1 hour TTL)",
        ],
      },
      {
        id: "comparison",
        title: "Comparison with Apache Kafka",
        content:
          "Unlike Apache Kafka, which couples compute and storage on broker nodes with local disks, StreamHouse separates these concerns. Kafka brokers must replicate data across multiple disks for durability, requiring expensive attached storage and careful partition rebalancing when scaling. StreamHouse eliminates this by writing directly to S3, which handles durability and replication transparently. The trade-off is slightly higher produce latency (50-100ms vs Kafka's 5-10ms) in exchange for dramatically lower cost and operational complexity.",
      },
    ],
  },

  topics: {
    title: "Topics & Partitions",
    description: "How data is organized and distributed in StreamHouse.",
    readTime: "8 min",
    sectionGroup: "Core Concepts",
    sections: [
      {
        id: "topics",
        title: "Topics",
        content:
          "A topic is a named, append-only log that stores a sequence of events. Topics are the primary unit of organization in StreamHouse. Each topic has a unique name and can be configured with retention policies, compression settings, and partition counts. Topics are created through the API, CLI, or web console.",
      },
      {
        id: "partitions",
        title: "Partitions",
        content:
          "Each topic is divided into one or more partitions. Partitions enable parallel processing and horizontal scaling. Each partition is an independent, ordered sequence of messages. A message's position within a partition is called its offset. Messages with the same key are always routed to the same partition via consistent hashing, guaranteeing ordering for related events.",
      },
      {
        id: "segments",
        title: "Segments",
        content:
          "Within each partition, messages are grouped into segments. A segment is an immutable file stored in S3, typically 64MB in size. Segments are compressed with LZ4 and contain a sequence of records along with an index for fast offset lookups. When a segment reaches its target size, it is sealed, uploaded to S3, and a new segment begins.",
        code: `# Segment path format in S3
s3://streamhouse-data/topics/{topic}/partitions/{partition}/segments/{start_offset}-{end_offset}.seg

# Example
s3://streamhouse-data/topics/user-events/partitions/0/segments/00000000-00000999.seg`,
        language: "text",
      },
      {
        id: "replication",
        title: "Replication & Durability",
        content:
          "StreamHouse relies on S3 for data durability rather than Kafka-style partition replication. S3 provides 99.999999999% (11 nines) durability by automatically replicating data across multiple availability zones. This eliminates the need for ISR (in-sync replica) management and partition reassignment that adds operational complexity in traditional Kafka deployments.",
      },
    ],
  },

  producers: {
    title: "Producers",
    description: "How to produce messages to StreamHouse topics.",
    readTime: "6 min",
    sectionGroup: "Core Concepts",
    sections: [
      {
        id: "overview",
        title: "Producer Overview",
        content:
          "Producers are client applications that write messages to StreamHouse topics. Messages are sent to the StreamHouse agent via gRPC or the REST API. Each message consists of an optional key, a value (the payload), optional headers, and a timestamp.",
      },
      {
        id: "partitioning",
        title: "Partition Assignment",
        content:
          "When producing a message, the partition is determined by the message key. If a key is provided, it is hashed using murmur2 to determine the target partition (consistent with Kafka's default partitioner). If no key is provided, messages are distributed round-robin across all partitions.",
        code: `// Partition assignment pseudocode
if message.key != null {
    partition = murmur2(message.key) % topic.num_partitions
} else {
    partition = round_robin_counter++ % topic.num_partitions
}`,
        language: "text",
      },
      {
        id: "batching",
        title: "Write Batching",
        content:
          "For performance, the StreamHouse agent buffers incoming messages in memory before flushing to S3. The writer pool maintains per-partition buffers that are flushed when either the buffer reaches 64MB or 10 seconds have elapsed since the first message in the batch. This batching amortizes the cost of S3 uploads across many messages.",
      },
      {
        id: "schemas",
        title: "Schema Validation",
        content:
          "If a topic has an associated schema in the Schema Registry, producers can opt into schema validation. The producer sends the schema ID in the message header, and the agent validates the message payload against the registered schema before accepting it. This catches data quality issues at write time.",
        code: `# Produce with schema validation
streamctl produce --topic user-events \\
  --schema-id 1 \\
  --key "user-123" \\
  --message '{"user_id": "123", "event": "login", "timestamp": 1706000000}'`,
        language: "bash",
      },
    ],
  },

  consumers: {
    title: "Consumers",
    description: "How to consume messages from StreamHouse topics.",
    readTime: "8 min",
    sectionGroup: "Core Concepts",
    sections: [
      {
        id: "overview",
        title: "Consumer Overview",
        content:
          "Consumers are client applications that read messages from StreamHouse topics. A consumer subscribes to one or more topics and reads messages starting from a specified offset. Consumers track their position (offset) so they can resume where they left off after restarts.",
      },
      {
        id: "consumer-groups",
        title: "Consumer Groups",
        content:
          "A consumer group is a set of consumers that cooperatively consume from a topic. Each partition is assigned to exactly one consumer in the group, allowing multiple consumers to process a topic in parallel. If a consumer joins or leaves the group, partitions are rebalanced automatically. Consumer group offsets are stored in the metadata store.",
        code: `# Start multiple consumers in a group
# Terminal 1
streamctl consume --topic user-events --group analytics --auto-commit

# Terminal 2 (gets different partitions)
streamctl consume --topic user-events --group analytics --auto-commit`,
        language: "bash",
      },
      {
        id: "offset-management",
        title: "Offset Management",
        content:
          "Consumers can manage offsets in two ways: automatic commit (offsets are committed periodically) or manual commit (the application explicitly commits offsets after processing). Manual commit provides exactly-once processing semantics when combined with idempotent downstream writes.",
        items: [
          "Auto-commit: Offsets committed every 5 seconds (configurable). Simple but may result in at-least-once delivery.",
          "Manual commit: Application calls commit after processing. Enables exactly-once when combined with transactional writes.",
          "Seek: Consumers can seek to a specific offset, the beginning, or the end of a partition at any time.",
        ],
      },
      {
        id: "fetching",
        title: "Fetch Behavior",
        content:
          "When a consumer requests messages, the StreamHouse agent determines which S3 segments contain the requested offset range. Segments are fetched from S3 (or the local cache), decompressed, and the relevant records are streamed back to the consumer. For tail reads (consuming near the head of the partition), the agent often serves directly from the in-memory write buffer, avoiding S3 entirely.",
      },
    ],
  },

  // ─── Agents ────────────────────────────────────────────────────
  "agents/config": {
    title: "Agent Configuration",
    description: "Configure StreamHouse agents for your workload.",
    readTime: "5 min",
    sectionGroup: "Agents",
    sections: [
      {
        id: "overview",
        title: "Overview",
        content:
          "StreamHouse agents are stateless compute nodes that handle all client requests. Because they hold no persistent state, agents can be started, stopped, and scaled freely. Configuration is provided via environment variables or a TOML config file.",
      },
      {
        id: "config-file",
        title: "Configuration File",
        content: "The agent configuration file controls server behavior, connection settings, and performance tuning parameters.",
        code: `# streamhouse.toml
[server]
host = "0.0.0.0"
http_port = 8080
grpc_port = 9092
worker_threads = 8

[metadata]
backend = "postgres"
url = "postgresql://user:pass@db:5432/streamhouse"
pool_size = 20
cache_ttl_secs = 300

[storage]
backend = "s3"
bucket = "streamhouse-data"
region = "us-east-1"
endpoint = ""  # Leave empty for AWS, set for MinIO

[writer]
segment_target_bytes = 67108864  # 64MB
flush_interval_secs = 10
max_concurrent_uploads = 4

[cache]
segment_cache_size_mb = 512
metadata_cache_size = 10000`,
        language: "toml",
      },
      {
        id: "env-vars",
        title: "Environment Variables",
        content: "All configuration options can be set via environment variables. Environment variables take precedence over the config file.",
        items: [
          "STREAMHOUSE_HOST: Bind address (default: 0.0.0.0)",
          "STREAMHOUSE_HTTP_PORT: HTTP API port (default: 8080)",
          "STREAMHOUSE_GRPC_PORT: gRPC port (default: 9092)",
          "METADATA_URL: PostgreSQL connection string",
          "S3_BUCKET: S3 bucket for data storage",
          "S3_REGION: AWS region (default: us-east-1)",
          "S3_ENDPOINT: Custom S3 endpoint (for MinIO)",
        ],
      },
    ],
  },

  "agents/scaling": {
    title: "Scaling Agents",
    description: "Horizontally scale StreamHouse agents for your throughput needs.",
    readTime: "6 min",
    sectionGroup: "Agents",
    sections: [
      {
        id: "horizontal",
        title: "Horizontal Scaling",
        content:
          "Because agents are stateless, scaling is as simple as adding more instances behind a load balancer. There is no data rebalancing, no leader election, and no partition reassignment required. Each agent independently handles requests by reading from and writing to shared S3 storage and the metadata store.",
      },
      {
        id: "load-balancing",
        title: "Load Balancing",
        content:
          "Agents should be deployed behind a layer 4 (TCP) or layer 7 (HTTP/gRPC) load balancer. Any agent can handle any request, so simple round-robin load balancing works well. For gRPC clients, use client-side load balancing or an envoy proxy.",
        code: `# Example: Kubernetes HPA for auto-scaling agents
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: streamhouse-agents
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: streamhouse
  minReplicas: 3
  maxReplicas: 20
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70`,
        language: "yaml",
      },
      {
        id: "sizing",
        title: "Sizing Guidelines",
        content: "Use these guidelines as a starting point for agent sizing.",
        items: [
          "Each agent can handle ~100MB/s throughput with default settings",
          "Memory: 2-4GB per agent (primarily for write buffers and segment cache)",
          "CPU: 2-4 cores per agent (compression and request handling)",
          "Start with 3 agents for high availability and scale based on throughput",
          "Monitor P99 latency and scale up when it exceeds your SLO",
        ],
      },
    ],
  },

  "agents/metrics": {
    title: "Health & Metrics",
    description: "Monitor agent health and performance with Prometheus metrics.",
    readTime: "5 min",
    sectionGroup: "Agents",
    sections: [
      {
        id: "health",
        title: "Health Endpoints",
        content:
          "Each agent exposes health check endpoints for liveness and readiness probes, compatible with Kubernetes health checks.",
        code: `# Liveness probe - agent is running
GET /health/live
# Response: {"status": "ok"}

# Readiness probe - agent can serve requests
GET /health/ready
# Response: {"status": "ready", "metadata": "connected", "storage": "connected"}

# Detailed health with metrics
GET /api/health
# Response: {"status": "healthy", "version": "0.1.0", "uptime_seconds": 3600}`,
        language: "text",
      },
      {
        id: "prometheus",
        title: "Prometheus Metrics",
        content:
          "Agents export Prometheus metrics at the /metrics endpoint. These metrics cover request rates, latencies, storage operations, and cache hit rates.",
        code: `# Key metrics to monitor
streamhouse_produce_requests_total          # Total produce requests
streamhouse_produce_latency_seconds         # Produce latency histogram
streamhouse_consume_requests_total          # Total consume requests
streamhouse_consume_latency_seconds         # Consume latency histogram
streamhouse_s3_upload_bytes_total           # Total bytes uploaded to S3
streamhouse_s3_download_bytes_total         # Total bytes downloaded from S3
streamhouse_segment_cache_hit_ratio         # Cache hit rate (aim for >90%)
streamhouse_metadata_cache_hit_ratio        # Metadata cache hit rate
streamhouse_active_connections              # Current active connections
streamhouse_writer_buffer_bytes             # Current write buffer usage`,
        language: "text",
      },
      {
        id: "grafana",
        title: "Grafana Dashboard",
        content:
          "StreamHouse includes pre-built Grafana dashboards for monitoring. Import the dashboard JSON from the repository, or use the web console's built-in monitoring page.",
        items: [
          "Overview: Request rate, error rate, latency P50/P95/P99",
          "Producers: Produce throughput, batch sizes, flush rates",
          "Consumers: Consumer lag, fetch latency, partition assignments",
          "Storage: S3 upload/download rates, segment sizes, cache hit ratios",
        ],
      },
    ],
  },

  "agents/leases": {
    title: "Lease Management",
    description: "How StreamHouse agents coordinate through leases.",
    readTime: "7 min",
    sectionGroup: "Agents",
    sections: [
      {
        id: "overview",
        title: "What Are Leases?",
        content:
          "Leases are time-bounded locks that coordinate work among agents. When an agent needs exclusive access to a resource (like flushing a partition's write buffer to S3), it acquires a lease from the metadata store. Leases prevent duplicate work and ensure exactly-once segment uploads even when multiple agents are running.",
      },
      {
        id: "how-it-works",
        title: "How Lease Coordination Works",
        content:
          "Each agent periodically heartbeats its active leases. If an agent crashes or becomes unresponsive, its leases expire after the TTL (default: 30 seconds), and another agent can take over. This provides automatic failover without manual intervention.",
        items: [
          "Lease acquisition: Agent requests a lease from PostgreSQL using a SELECT FOR UPDATE query",
          "Heartbeat: Active leases are renewed every 10 seconds (configurable)",
          "Expiry: Leases expire after 30 seconds without renewal",
          "Takeover: Any agent can acquire an expired lease and resume the work",
        ],
      },
      {
        id: "failure-detection",
        title: "Failure Detection",
        content:
          "StreamHouse uses lease expiry as the primary mechanism for detecting agent failures. When an agent stops heartbeating (due to crash, network partition, or GC pause), its leases expire and other agents detect this during their next coordination cycle. The detection latency is bounded by the lease TTL, providing predictable recovery times.",
      },
    ],
  },

  // ─── Storage ───────────────────────────────────────────────────
  "storage/s3": {
    title: "S3 Configuration",
    description: "Configure S3 or S3-compatible storage for StreamHouse.",
    readTime: "5 min",
    sectionGroup: "Storage",
    sections: [
      {
        id: "aws-s3",
        title: "Amazon S3",
        content:
          "For production deployments, Amazon S3 is the recommended storage backend. StreamHouse uses the AWS SDK and supports all standard authentication methods.",
        code: `# Environment variables for AWS S3
export S3_BUCKET=streamhouse-production
export S3_REGION=us-east-1
export AWS_ACCESS_KEY_ID=AKIA...
export AWS_SECRET_ACCESS_KEY=...

# Or use IAM roles (recommended for EC2/EKS)
# No credentials needed - the SDK auto-discovers IAM roles`,
        language: "bash",
      },
      {
        id: "minio",
        title: "MinIO (Local Development)",
        content:
          "MinIO provides S3-compatible storage for local development and testing. Set the S3_ENDPOINT to point to your MinIO instance.",
        code: `export S3_BUCKET=streamhouse
export S3_ENDPOINT=http://localhost:9000
export S3_ACCESS_KEY=minioadmin
export S3_SECRET_KEY=minioadmin
export S3_FORCE_PATH_STYLE=true`,
        language: "bash",
      },
      {
        id: "bucket-policy",
        title: "Bucket Configuration",
        content:
          "StreamHouse requires specific S3 bucket permissions and recommends certain configurations for optimal performance.",
        items: [
          "Required permissions: s3:GetObject, s3:PutObject, s3:ListBucket, s3:DeleteObject",
          "Enable S3 Transfer Acceleration for cross-region deployments",
          "Use S3 Intelligent-Tiering for automatic cost optimization on older segments",
          "Configure lifecycle rules to transition old segments to S3 Glacier if long retention is needed",
        ],
      },
    ],
  },

  "storage/segments": {
    title: "Segment Format",
    description: "Internal format of StreamHouse data segments.",
    readTime: "8 min",
    sectionGroup: "Storage",
    sections: [
      {
        id: "overview",
        title: "Segment Structure",
        content:
          "A segment is an immutable, compressed file stored in S3 that contains a sequence of records for a single partition. Each segment has a header, a data section containing the compressed records, and a footer with an offset index for fast lookups.",
      },
      {
        id: "format",
        title: "Binary Format",
        content: "The segment binary format is designed for fast reads and efficient storage.",
        code: `┌─────────────────────────────────┐
│ Header (32 bytes)               │
│  - Magic bytes: "STRM"         │
│  - Version: u16                │
│  - Compression: u8 (LZ4=1)    │
│  - Record count: u64           │
│  - Start offset: u64           │
│  - End offset: u64             │
├─────────────────────────────────┤
│ Data Section (variable)         │
│  - LZ4-compressed records      │
│  - Each record:                │
│    - Key length (u32)          │
│    - Key bytes                 │
│    - Value length (u32)        │
│    - Value bytes               │
│    - Timestamp (i64)           │
│    - Headers count (u16)       │
│    - Header entries            │
├─────────────────────────────────┤
│ Index Section                   │
│  - Sparse offset index         │
│  - Every 1000th record offset  │
│  - Byte position in data       │
├─────────────────────────────────┤
│ Footer (16 bytes)               │
│  - Index offset: u64           │
│  - CRC32: u32                  │
│  - Magic bytes: "ENDS"         │
└─────────────────────────────────┘`,
        language: "text",
      },
      {
        id: "sizing",
        title: "Segment Sizing",
        content:
          "The target segment size is 64MB (configurable). Smaller segments increase metadata overhead but reduce read amplification. Larger segments are more efficient for storage but increase the minimum granularity for reads. For most workloads, 64MB provides a good balance.",
      },
    ],
  },

  "storage/compression": {
    title: "Compression",
    description: "How StreamHouse compresses data for efficient storage.",
    readTime: "4 min",
    sectionGroup: "Storage",
    sections: [
      {
        id: "overview",
        title: "Compression in StreamHouse",
        content:
          "StreamHouse compresses segment data before uploading to S3. Compression reduces storage costs and improves read performance by reducing the amount of data transferred from S3. The default compression algorithm is LZ4, chosen for its excellent balance of speed and ratio.",
      },
      {
        id: "algorithms",
        title: "Supported Algorithms",
        content: "StreamHouse supports multiple compression algorithms to suit different workloads.",
        items: [
          "LZ4 (default): Fast compression and decompression. ~2:1 ratio for JSON data. Best for low-latency workloads.",
          "Zstd: Higher compression ratio (~4:1 for JSON) at the cost of slightly higher CPU usage. Best for cost-sensitive workloads with high data volumes.",
          "None: No compression. Use when data is already compressed or CPU is constrained.",
        ],
      },
      {
        id: "configuration",
        title: "Configuration",
        content: "Compression is configured per-topic. Different topics can use different algorithms.",
        code: `# Set compression when creating a topic
streamctl topic create --name logs --partitions 6 --compression zstd

# Change compression for an existing topic (applies to new segments only)
streamctl topic update --name logs --compression lz4`,
        language: "bash",
      },
    ],
  },

  "storage/retention": {
    title: "Retention Policies",
    description: "Configure how long StreamHouse retains data.",
    readTime: "5 min",
    sectionGroup: "Storage",
    sections: [
      {
        id: "overview",
        title: "Data Retention",
        content:
          "Retention policies control how long messages are kept in a topic before being deleted. StreamHouse supports time-based retention, size-based retention, or both. When a retention limit is reached, the oldest segments are deleted from S3 and their metadata is removed.",
      },
      {
        id: "time-based",
        title: "Time-Based Retention",
        content:
          "Time-based retention deletes segments older than the specified duration. This is the most common retention strategy.",
        code: `# Set 30-day retention
streamctl topic create --name events --retention 30d

# Supported units: m (minutes), h (hours), d (days)
# Examples: 1h, 7d, 90d

# Set infinite retention (never delete)
streamctl topic create --name audit-log --retention infinite`,
        language: "bash",
      },
      {
        id: "size-based",
        title: "Size-Based Retention",
        content:
          "Size-based retention caps the total size of a topic. When the limit is exceeded, the oldest segments are deleted.",
        code: `# Cap topic at 100GB
streamctl topic create --name metrics --retention-bytes 100GB

# Combine time and size (whichever triggers first)
streamctl topic create --name logs \\
  --retention 7d \\
  --retention-bytes 500GB`,
        language: "bash",
      },
      {
        id: "compaction",
        title: "Log Compaction",
        content:
          "For topics that represent state (like a changelog), log compaction keeps only the latest value for each key. This is useful for maintaining a materialized view that can be rebuilt from the topic.",
        items: [
          "Compaction runs as a background process on the agent",
          "Only the most recent record per key is retained",
          "Tombstone records (null value) mark a key for deletion",
          "Compacted topics can still have time-based retention for tombstones",
        ],
      },
    ],
  },

  // ─── SQL Processing ────────────────────────────────────────────
  "sql/overview": {
    title: "SQL Overview",
    description: "Process streaming data using familiar SQL syntax.",
    readTime: "6 min",
    sectionGroup: "SQL Processing",
    sections: [
      {
        id: "overview",
        title: "Streaming SQL Engine",
        content:
          "StreamHouse includes a built-in SQL engine powered by Apache DataFusion. It allows you to query streaming data using standard SQL, create materialized views, and run continuous queries. This eliminates the need for a separate stream processing framework like Flink or ksqlDB for many common use cases.",
      },
      {
        id: "capabilities",
        title: "What You Can Do",
        content:
          "The SQL engine supports a wide range of operations on streaming data.",
        items: [
          "Query topics as SQL tables with SELECT, WHERE, GROUP BY, and ORDER BY",
          "Create continuous queries that run indefinitely and write results to new topics",
          "Define windowed aggregations (tumbling, hopping, session windows)",
          "Join multiple streams together in real-time",
          "Use built-in functions for JSON parsing, timestamp manipulation, and math",
        ],
      },
      {
        id: "getting-started",
        title: "Getting Started",
        content:
          "You can run SQL queries through the web console's SQL Workbench, the CLI, or the REST API.",
        code: `# Using the CLI
streamctl sql "SELECT * FROM user_events WHERE event = 'purchase' LIMIT 10"

# Interactive SQL shell
streamctl sql --interactive

# Using the REST API
curl -X POST http://localhost:8080/api/sql \\
  -H "Content-Type: application/json" \\
  -d '{"query": "SELECT count(*) FROM user_events"}'`,
        language: "bash",
      },
    ],
  },

  "sql/streams": {
    title: "Creating Streams",
    description: "Define SQL streams over StreamHouse topics.",
    readTime: "8 min",
    sectionGroup: "SQL Processing",
    sections: [
      {
        id: "create-stream",
        title: "Creating a Stream",
        content:
          "A stream is a SQL view over a StreamHouse topic. Streams define the schema of the data in a topic, enabling typed queries and validation.",
        code: `-- Create a stream over an existing topic
CREATE STREAM user_events (
  user_id VARCHAR,
  event VARCHAR,
  page VARCHAR,
  timestamp TIMESTAMP,
  metadata JSON
) WITH (
  topic = 'user-events',
  format = 'json',
  timestamp_field = 'timestamp'
);

-- Query the stream
SELECT user_id, event, count(*)
FROM user_events
WHERE timestamp > NOW() - INTERVAL '1 hour'
GROUP BY user_id, event;`,
        language: "sql",
      },
      {
        id: "continuous",
        title: "Continuous Queries",
        content:
          "Continuous queries run indefinitely, processing each new message as it arrives and writing results to an output topic.",
        code: `-- Create a continuous query that counts events per minute
CREATE CONTINUOUS QUERY event_counts AS
  SELECT
    event,
    count(*) as event_count,
    window_start,
    window_end
  FROM TUMBLE(user_events, timestamp, INTERVAL '1 minute')
  GROUP BY event, window_start, window_end
  OUTPUT TO 'event-counts-per-minute';`,
        language: "sql",
      },
      {
        id: "formats",
        title: "Data Formats",
        content: "Streams support multiple data serialization formats.",
        items: [
          "JSON: Self-describing format, flexible schema. Best for development and mixed-type data.",
          "Avro: Binary format with schema registry integration. Best for production with strict schemas.",
          "Protobuf: Google's binary format. Best for gRPC-heavy environments.",
          "CSV: Simple text format. Useful for log data and integration with legacy systems.",
        ],
      },
    ],
  },

  "sql/windows": {
    title: "Windowed Aggregations",
    description: "Aggregate streaming data over time windows.",
    readTime: "10 min",
    sectionGroup: "SQL Processing",
    sections: [
      {
        id: "overview",
        title: "What Are Windows?",
        content:
          "Windows group streaming events into finite sets based on time, enabling aggregations like counts, sums, and averages over defined time periods. Without windows, aggregations over an unbounded stream would never complete.",
      },
      {
        id: "tumbling",
        title: "Tumbling Windows",
        content:
          "Tumbling windows are fixed-size, non-overlapping time intervals. Each event belongs to exactly one window.",
        code: `-- Count events per 5-minute tumbling window
SELECT
  event,
  count(*) as cnt,
  window_start,
  window_end
FROM TUMBLE(user_events, timestamp, INTERVAL '5 minutes')
GROUP BY event, window_start, window_end;`,
        language: "sql",
      },
      {
        id: "hopping",
        title: "Hopping Windows",
        content:
          "Hopping windows have a fixed size but advance by a smaller step, creating overlapping windows. An event may belong to multiple windows.",
        code: `-- 10-minute windows that advance every 2 minutes
SELECT
  event,
  count(*) as cnt,
  avg(duration) as avg_duration
FROM HOP(user_events, timestamp, INTERVAL '2 minutes', INTERVAL '10 minutes')
GROUP BY event, window_start, window_end;`,
        language: "sql",
      },
      {
        id: "session",
        title: "Session Windows",
        content:
          "Session windows group events that are close together in time, separated by a gap of inactivity. They are useful for tracking user sessions or bursts of activity.",
        code: `-- Group events into sessions with 30-minute timeout
SELECT
  user_id,
  count(*) as events_in_session,
  min(timestamp) as session_start,
  max(timestamp) as session_end
FROM SESSION(user_events, timestamp, INTERVAL '30 minutes')
GROUP BY user_id, session_start, session_end;`,
        language: "sql",
      },
    ],
  },

  "sql/joins": {
    title: "Stream Joins",
    description: "Join multiple streams together in real-time.",
    readTime: "8 min",
    sectionGroup: "SQL Processing",
    sections: [
      {
        id: "overview",
        title: "Joining Streams",
        content:
          "StreamHouse supports joining two or more streams together based on matching keys and time constraints. Stream joins are useful for enriching events with context, correlating events from different sources, and detecting patterns across streams.",
      },
      {
        id: "windowed-join",
        title: "Windowed Joins",
        content:
          "Windowed joins match events from two streams that fall within the same time window.",
        code: `-- Join clicks with impressions within a 1-hour window
SELECT
  c.user_id,
  c.ad_id,
  i.campaign_id,
  c.timestamp as click_time,
  i.timestamp as impression_time
FROM clicks c
JOIN impressions i
  ON c.ad_id = i.ad_id
  AND c.timestamp BETWEEN i.timestamp AND i.timestamp + INTERVAL '1 hour';`,
        language: "sql",
      },
      {
        id: "table-join",
        title: "Stream-Table Joins",
        content:
          "A stream can be joined with a compacted topic (treated as a table). The table represents the latest state for each key, and each stream event is enriched with the current table value.",
        code: `-- Enrich orders with latest customer data
CREATE TABLE customers (
  customer_id VARCHAR PRIMARY KEY,
  name VARCHAR,
  tier VARCHAR
) WITH (topic = 'customers', format = 'json');

SELECT
  o.order_id,
  o.amount,
  c.name as customer_name,
  c.tier as customer_tier
FROM orders o
JOIN customers c ON o.customer_id = c.customer_id;`,
        language: "sql",
      },
    ],
  },

  // ─── Operations ────────────────────────────────────────────────
  "ops/monitoring": {
    title: "Monitoring",
    description: "Set up monitoring for your StreamHouse deployment.",
    readTime: "8 min",
    sectionGroup: "Operations",
    sections: [
      {
        id: "overview",
        title: "Monitoring Overview",
        content:
          "Monitoring is critical for running StreamHouse in production. StreamHouse exposes comprehensive Prometheus metrics, structured logs, and health endpoints. The web console provides built-in dashboards for real-time monitoring.",
      },
      {
        id: "key-metrics",
        title: "Key Metrics to Monitor",
        content: "Focus on these metrics for a healthy StreamHouse deployment.",
        items: [
          "Produce/consume latency P99: Should be under 200ms for produce, 100ms for consume",
          "Consumer lag: The difference between the latest offset and the consumer's committed offset. Rising lag indicates consumers can't keep up.",
          "S3 error rate: Should be near zero. Spikes indicate S3 throttling or network issues.",
          "Segment cache hit ratio: Aim for >90%. Low hit rates mean more S3 reads and higher latency.",
          "Agent CPU/memory: Scale out when agents consistently exceed 70% CPU utilization.",
          "Metadata query latency: PostgreSQL query time. Should be under 10ms P99.",
        ],
      },
      {
        id: "prometheus",
        title: "Prometheus Setup",
        content: "Configure Prometheus to scrape StreamHouse agents.",
        code: `# prometheus.yml
scrape_configs:
  - job_name: 'streamhouse'
    scrape_interval: 15s
    static_configs:
      - targets:
        - 'agent-1:8080'
        - 'agent-2:8080'
        - 'agent-3:8080'
    metrics_path: '/metrics'`,
        language: "yaml",
      },
      {
        id: "logs",
        title: "Structured Logging",
        content:
          "StreamHouse outputs structured JSON logs that can be collected by any log aggregation system. Log levels can be configured per-module for targeted debugging.",
        code: `# Set log level via environment variable
export RUST_LOG=streamhouse=info,streamhouse::storage=debug

# Example log output
{"timestamp":"2026-01-15T10:30:00Z","level":"INFO","module":"streamhouse::agent","message":"Segment flushed","topic":"events","partition":0,"offset_range":"1000-1999","size_bytes":67108864,"duration_ms":450}`,
        language: "bash",
      },
    ],
  },

  "ops/alerting": {
    title: "Alerting",
    description: "Configure alerts for StreamHouse operational issues.",
    readTime: "6 min",
    sectionGroup: "Operations",
    sections: [
      {
        id: "overview",
        title: "Alert Strategy",
        content:
          "Effective alerting focuses on symptoms (what users experience) rather than causes. Alert on high latency, rising consumer lag, and error rates. Investigate causes using dashboards and logs after being paged.",
      },
      {
        id: "rules",
        title: "Recommended Alert Rules",
        content: "Start with these Prometheus alerting rules for a production StreamHouse deployment.",
        code: `groups:
  - name: streamhouse
    rules:
      - alert: HighProduceLatency
        expr: histogram_quantile(0.99, rate(streamhouse_produce_latency_seconds_bucket[5m])) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P99 produce latency above 500ms"

      - alert: ConsumerLagRising
        expr: increase(streamhouse_consumer_lag[10m]) > 10000
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Consumer lag rising rapidly"

      - alert: S3ErrorRate
        expr: rate(streamhouse_s3_errors_total[5m]) > 0.01
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "S3 error rate above 1%"

      - alert: AgentDown
        expr: up{job="streamhouse"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "StreamHouse agent is down"`,
        language: "yaml",
      },
      {
        id: "runbooks",
        title: "Runbook Links",
        content: "Each alert should link to a runbook with investigation steps.",
        items: [
          "HighProduceLatency: Check S3 upload times, agent CPU, and metadata query latency. May need to scale agents.",
          "ConsumerLagRising: Check if consumers are healthy, if processing is slow, or if produce rate has spiked.",
          "S3ErrorRate: Check AWS Service Health Dashboard, verify IAM permissions, and check for S3 throttling (429 responses).",
          "AgentDown: Check container/pod status, review logs for crash reasons, verify network connectivity.",
        ],
      },
    ],
  },

  "ops/backup": {
    title: "Backup & Recovery",
    description: "Disaster recovery strategies for StreamHouse.",
    readTime: "7 min",
    sectionGroup: "Operations",
    sections: [
      {
        id: "overview",
        title: "Backup Strategy",
        content:
          "StreamHouse's disaggregated architecture simplifies backup and recovery. Event data in S3 is inherently durable (11 nines). The critical component to back up is the metadata store (PostgreSQL), which contains topic definitions, partition state, segment locations, and consumer group offsets.",
      },
      {
        id: "metadata-backup",
        title: "Metadata Store Backup",
        content: "Regular PostgreSQL backups ensure you can recover topic and consumer state.",
        code: `# Automated daily backup with pg_dump
pg_dump -h localhost -U streamhouse -d streamhouse -F c -f backup_$(date +%Y%m%d).dump

# For managed databases (RDS/Aurora), enable automated backups
# with point-in-time recovery (PITR) for the most protection

# Restore from backup
pg_restore -h localhost -U streamhouse -d streamhouse -c backup_20260115.dump`,
        language: "bash",
      },
      {
        id: "recovery",
        title: "Recovery Procedures",
        content: "Recovery procedures depend on the type of failure.",
        items: [
          "Agent failure: Automatic. Other agents detect via lease expiry and take over work. No data loss.",
          "S3 outage: Agents buffer writes in memory and retry. Consumers may experience increased latency. Recovery is automatic when S3 returns.",
          "Metadata store failure: Agents cannot accept new writes or update consumer offsets. Restore from backup or failover to replica.",
          "Full disaster recovery: Restore metadata from backup, point agents at the same S3 bucket, and restart. All data is recovered.",
        ],
      },
      {
        id: "cross-region",
        title: "Cross-Region Replication",
        content:
          "For multi-region disaster recovery, enable S3 Cross-Region Replication (CRR) on your data bucket and set up a standby PostgreSQL replica in the target region. In a regional failure, promote the standby replica and point agents at the replicated S3 bucket.",
      },
    ],
  },

  "ops/security": {
    title: "Security",
    description: "Secure your StreamHouse deployment.",
    readTime: "10 min",
    sectionGroup: "Operations",
    sections: [
      {
        id: "overview",
        title: "Security Model",
        content:
          "StreamHouse supports multiple layers of security: network-level access control, TLS encryption for all connections, authentication via API keys or mTLS, and authorization through topic-level access control lists (ACLs).",
      },
      {
        id: "tls",
        title: "TLS Configuration",
        content: "Enable TLS to encrypt all client-to-agent and agent-to-storage communications.",
        code: `# streamhouse.toml TLS configuration
[tls]
enabled = true
cert_file = "/etc/streamhouse/tls/server.crt"
key_file = "/etc/streamhouse/tls/server.key"
ca_file = "/etc/streamhouse/tls/ca.crt"  # For mTLS

# Or via environment variables
export TLS_CERT_FILE=/path/to/server.crt
export TLS_KEY_FILE=/path/to/server.key`,
        language: "toml",
      },
      {
        id: "authentication",
        title: "Authentication",
        content: "StreamHouse supports API key authentication and mutual TLS (mTLS).",
        items: [
          "API Keys: Generate keys via the web console or CLI. Include the key in the Authorization header.",
          "mTLS: Clients present a certificate signed by the trusted CA. Best for service-to-service communication.",
          "SASL/SCRAM: Compatible with Kafka clients using SASL authentication.",
        ],
      },
      {
        id: "authorization",
        title: "Authorization & ACLs",
        content:
          "Topic-level ACLs control which clients can produce to or consume from specific topics.",
        code: `# Grant produce access to a service account
streamctl acl add --principal "service:payment-processor" \\
  --topic "transactions" \\
  --operation produce \\
  --permission allow

# Grant consume access to a consumer group
streamctl acl add --principal "group:analytics" \\
  --topic "transactions" \\
  --operation consume \\
  --permission allow

# List ACLs for a topic
streamctl acl list --topic transactions`,
        language: "bash",
      },
    ],
  },

  // ─── Reference ─────────────────────────────────────────────────
  api: {
    title: "API Reference",
    description: "Complete REST and gRPC API documentation.",
    readTime: "15 min",
    sectionGroup: "Reference",
    sections: [
      {
        id: "rest-api",
        title: "REST API",
        content:
          "The StreamHouse REST API is available on port 8080 by default. All endpoints accept and return JSON.",
      },
      {
        id: "topics-api",
        title: "Topics API",
        content: "CRUD operations for topic management.",
        code: `# List all topics
GET /api/topics

# Get topic details
GET /api/topics/{name}

# Create a topic
POST /api/topics
Body: {
  "name": "events",
  "partitions": 6,
  "config": {
    "retention_ms": 604800000,
    "compression": "lz4"
  }
}

# Delete a topic
DELETE /api/topics/{name}`,
        language: "text",
      },
      {
        id: "produce-api",
        title: "Produce API",
        content: "Send messages to topics.",
        code: `# Produce a single message
POST /api/topics/{name}/produce
Body: {
  "key": "user-123",
  "value": {"event": "login"},
  "headers": {"source": "web-app"}
}

# Produce a batch of messages
POST /api/topics/{name}/produce/batch
Body: {
  "messages": [
    {"key": "k1", "value": {"a": 1}},
    {"key": "k2", "value": {"a": 2}}
  ]
}`,
        language: "text",
      },
      {
        id: "consume-api",
        title: "Consume API",
        content: "Read messages from topics.",
        code: `# Consume messages
GET /api/topics/{name}/consume?partition=0&offset=0&limit=100

# Consume with consumer group
POST /api/consumer-groups/{group}/subscribe
Body: {"topics": ["events"]}

# Commit offsets
POST /api/consumer-groups/{group}/commit
Body: {"offsets": {"events": {"0": 1000, "1": 500}}}`,
        language: "text",
      },
      {
        id: "grpc-api",
        title: "gRPC API",
        content:
          "The gRPC API is available on port 9092 and provides higher throughput for produce and consume operations. The protobuf definitions are available in the repository under proto/streamhouse.proto.",
      },
    ],
  },

  cli: {
    title: "CLI Reference",
    description: "The streamctl command-line tool.",
    readTime: "10 min",
    sectionGroup: "Reference",
    sections: [
      {
        id: "install",
        title: "Installation",
        content:
          "The streamctl CLI is included in the StreamHouse release. You can also install it separately.",
        code: `# Install from source
cargo install --path crates/streamhouse-cli

# Or download from releases
curl -L https://github.com/streamhouse/streamhouse/releases/latest/download/streamctl -o /usr/local/bin/streamctl
chmod +x /usr/local/bin/streamctl

# Configure the server address
export STREAMHOUSE_URL=http://localhost:8080`,
        language: "bash",
      },
      {
        id: "topic-commands",
        title: "Topic Commands",
        content: "Manage topics with the topic subcommand.",
        code: `streamctl topic list                          # List all topics
streamctl topic create --name NAME [options]   # Create a topic
streamctl topic describe --name NAME           # Show topic details
streamctl topic delete --name NAME             # Delete a topic
streamctl topic update --name NAME [options]   # Update topic config

# Options for create/update:
#   --partitions N          Number of partitions
#   --retention DURATION    Retention period (e.g., 7d, 24h)
#   --retention-bytes SIZE  Max topic size (e.g., 100GB)
#   --compression ALGO      Compression (lz4, zstd, none)`,
        language: "bash",
      },
      {
        id: "produce-commands",
        title: "Produce Commands",
        content: "Send messages to topics.",
        code: `# Produce a single message
streamctl produce --topic NAME --message 'JSON'

# Produce with a key
streamctl produce --topic NAME --key KEY --message 'JSON'

# Produce from a file (one message per line)
streamctl produce --topic NAME --file messages.jsonl

# Interactive produce mode
streamctl produce --topic NAME --interactive`,
        language: "bash",
      },
      {
        id: "consume-commands",
        title: "Consume Commands",
        content: "Read messages from topics.",
        code: `# Consume from the beginning
streamctl consume --topic NAME --from-beginning

# Consume from a specific offset
streamctl consume --topic NAME --partition 0 --offset 100

# Consume with a consumer group
streamctl consume --topic NAME --group GROUP --auto-commit

# Consume and output as JSON
streamctl consume --topic NAME --format json`,
        language: "bash",
      },
      {
        id: "sql-command",
        title: "SQL Commands",
        content: "Run SQL queries on streaming data.",
        code: `# Run a one-off query
streamctl sql "SELECT * FROM events LIMIT 10"

# Start interactive SQL shell
streamctl sql --interactive

# Run query from a file
streamctl sql --file query.sql`,
        language: "bash",
      },
    ],
  },

  examples: {
    title: "Examples",
    description: "Sample applications and code snippets for common use cases.",
    readTime: "12 min",
    sectionGroup: "Reference",
    sections: [
      {
        id: "realtime-analytics",
        title: "Real-Time Analytics Pipeline",
        content:
          "This example shows how to build a real-time analytics pipeline that ingests click events, aggregates them per page per minute, and writes the results to a summary topic.",
        code: `-- Create the source stream
CREATE STREAM clickstream (
  user_id VARCHAR,
  page VARCHAR,
  referrer VARCHAR,
  timestamp TIMESTAMP
) WITH (topic = 'clicks', format = 'json');

-- Create a continuous query for page views per minute
CREATE CONTINUOUS QUERY page_views_per_minute AS
  SELECT
    page,
    count(*) as views,
    count(DISTINCT user_id) as unique_visitors,
    window_start,
    window_end
  FROM TUMBLE(clickstream, timestamp, INTERVAL '1 minute')
  GROUP BY page, window_start, window_end
  OUTPUT TO 'page-views-per-minute';`,
        language: "sql",
      },
      {
        id: "cdc-pipeline",
        title: "Change Data Capture (CDC)",
        content:
          "Use StreamHouse to capture database changes and propagate them to downstream systems.",
        code: `# 1. Set up CDC from PostgreSQL using Debezium
# debezium-connector.json
{
  "name": "pg-source",
  "config": {
    "connector.class": "io.debezium.connector.postgresql.PostgresConnector",
    "database.hostname": "postgres",
    "database.port": "5432",
    "database.user": "replicator",
    "database.dbname": "myapp",
    "topic.prefix": "cdc",
    "plugin.name": "pgoutput"
  }
}

# 2. Consume CDC events in StreamHouse
streamctl consume --topic cdc.public.users --from-beginning --format json`,
        language: "json",
      },
      {
        id: "log-aggregation",
        title: "Log Aggregation",
        content:
          "Collect logs from multiple services into StreamHouse and query them with SQL.",
        code: `# Produce logs from your application
echo '{"service":"api","level":"error","message":"Connection timeout","timestamp":"2026-01-15T10:30:00Z"}' | \\
  streamctl produce --topic app-logs

# Query errors in the last hour
streamctl sql "
  SELECT service, level, message, timestamp
  FROM app_logs
  WHERE level = 'error'
    AND timestamp > NOW() - INTERVAL '1 hour'
  ORDER BY timestamp DESC
  LIMIT 50
"`,
        language: "bash",
      },
      {
        id: "python-client",
        title: "Python Client Example",
        content:
          "Use the StreamHouse Python client for programmatic access.",
        code: `import requests
import json

STREAMHOUSE_URL = "http://localhost:8080"

# Produce messages
def produce(topic: str, key: str, value: dict):
    resp = requests.post(
        f"{STREAMHOUSE_URL}/api/topics/{topic}/produce",
        json={"key": key, "value": value}
    )
    return resp.json()

# Consume messages
def consume(topic: str, partition: int = 0, offset: int = 0, limit: int = 100):
    resp = requests.get(
        f"{STREAMHOUSE_URL}/api/topics/{topic}/consume",
        params={"partition": partition, "offset": offset, "limit": limit}
    )
    return resp.json()

# Example usage
produce("user-events", "user-42", {"event": "signup", "plan": "pro"})
messages = consume("user-events", partition=0, offset=0)
for msg in messages:
    print(f"Offset {msg['offset']}: {msg['value']}")`,
        language: "python",
      },
    ],
  },
};

// Helper to get previous/next page navigation
export function getPageNavigation(slug: string): {
  prev?: { title: string; href: string };
  next?: { title: string; href: string };
} {
  const allLinks = navigation.flatMap((s) => s.links);
  const currentIndex = allLinks.findIndex((l) => l.href === `/docs/${slug}`);

  if (currentIndex === -1) return {};

  return {
    prev: currentIndex > 0 ? allLinks[currentIndex - 1] : undefined,
    next:
      currentIndex < allLinks.length - 1
        ? allLinks[currentIndex + 1]
        : undefined,
  };
}
