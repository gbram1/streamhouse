# Deployment Guide

StreamHouse supports three deployment modes: **Self-Hosted**, **BYOC** (Bring Your Own Cloud), and **Fully Managed**. Each mode gives you the same core platform — S3-native streaming with REST, Kafka, and gRPC protocols — with different levels of operational responsibility.

---

## Quick Comparison

| | Self-Hosted | BYOC | Managed |
|---|---|---|---|
| **Infrastructure** | You own it all | Your AWS account, our control plane | We run everything |
| **Data location** | Your S3/MinIO | Your S3 bucket | Our S3 bucket |
| **Auth** | Optional (API keys + OIDC) | API keys via platform | API keys via platform |
| **Setup time** | ~5 minutes | ~15 minutes | ~1 minute |
| **Cost** | Infrastructure only | Infrastructure + platform fee | Platform fee |

---

## Option 1: Self-Hosted

You run StreamHouse on your own infrastructure. No external dependencies, fully offline, open-source.

### Quick Start (Single Node)

```bash
git clone https://github.com/gbram1/streamhouse
cd streamhouse
./quickstart.sh
```

This starts a local server with SQLite metadata and local filesystem storage. Good for evaluation and development.

### Docker Compose (Production-Like)

For a multi-agent setup with Postgres, MinIO (S3-compatible), and monitoring:

```bash
git clone https://github.com/gbram1/streamhouse
cd streamhouse
docker compose up -d
```

This starts:
- 1 server (REST :8080, gRPC :50051, Kafka :9092)
- 3 agents (partition owners)
- PostgreSQL (metadata)
- MinIO (S3-compatible segment storage)
- Prometheus + Grafana (monitoring)

### Production (AWS)

For production on AWS with real S3 and RDS:

**1. Build the server:**

```bash
cargo build --release --features postgres -p streamhouse-server
```

**2. Set up infrastructure:**
- An RDS PostgreSQL instance (or any Postgres 14+)
- An S3 bucket
- EC2 instances or ECS tasks for server + agents

**3. Configure the server:**

```bash
# Metadata
export STREAMHOUSE_METADATA=postgres
export DATABASE_URL=postgres://user:pass@your-rds:5432/streamhouse

# Storage
export AWS_REGION=us-east-1
export STREAMHOUSE_BUCKET=your-bucket

# Server
export HTTP_ADDR=0.0.0.0:8080
export GRPC_ADDR=0.0.0.0:50051
export KAFKA_ADDR=0.0.0.0:9092

# Write durability
export WAL_ENABLED=true
export WAL_DIR=/data/wal

# Disaster recovery
export SNAPSHOT_INTERVAL_SECS=3600
export RECONCILE_INTERVAL=3600

# Logging
export RUST_LOG=info
export LOG_FORMAT=json
```

**4. Start the server (API-only mode):**

```bash
DISABLE_EMBEDDED_AGENT=true ./target/release/unified-server
```

**5. Start agents (on separate instances or as separate processes):**

```bash
# Agent 1
AGENT_ID=agent-1 \
AGENT_ZONE=us-east-1a \
METADATA_STORE=postgres://user:pass@your-rds:5432/streamhouse \
./target/release/agent

# Agent 2
AGENT_ID=agent-2 \
AGENT_ZONE=us-east-1b \
METADATA_STORE=postgres://user:pass@your-rds:5432/streamhouse \
./target/release/agent
```

**6. (Optional) Enable authentication:**

```bash
export STREAMHOUSE_AUTH_ENABLED=true
export STREAMHOUSE_ADMIN_KEY=your-secret-admin-key
```

Then bootstrap your first org and API key:

```bash
# Create organization
curl -X POST http://localhost:8080/api/v1/organizations \
  -H "Authorization: Bearer your-secret-admin-key" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-org", "slug": "my-org"}'

# Create API key (use the org_id from the response above)
curl -X POST http://localhost:8080/api/v1/organizations/{org_id}/api-keys \
  -H "Authorization: Bearer your-secret-admin-key" \
  -H "Content-Type: application/json" \
  -d '{"name": "production", "permissions": ["read", "write"], "scopes": ["*"]}'
```

**7. (Optional) Add OIDC single sign-on:**

If you use an OIDC-compatible identity provider (Auth0, Okta, Keycloak, etc.), you can wire it up so users authenticate via SSO:

```bash
export OIDC_ISSUER_URL=https://your-idp.example.com
```

The server will fetch the JWKS from `{OIDC_ISSUER_URL}/.well-known/jwks.json` and validate JWTs. The JWT `sub` claim is used to resolve the user's organization.

### Self-Hosted with Terraform

A reference Terraform configuration is available at `infra/terraform/environments/self-hosted/`:

```bash
cd infra/terraform/environments/self-hosted
cp terraform.tfvars.example terraform.tfvars  # Edit with your values
terraform init
terraform plan
terraform apply
```

This provisions a single EC2 instance with Docker Compose, an S3 bucket, and a security group.

### Self-Hosted Reference Architecture

```
┌─────────────────────────────────────────────┐
│  Your Infrastructure                         │
│                                              │
│  ┌──────────────┐   ┌──────────────────────┐│
│  │  PostgreSQL   │   │  Unified Server      ││
│  │  (metadata)   │   │  :8080 REST          ││
│  │              │   │  :50051 gRPC         ││
│  └──────────────┘   │  :9092 Kafka         ││
│                      └──────────────────────┘│
│  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Agent 1  │  │ Agent 2  │  │ Agent 3  │   │
│  │ us-east-1a│ │ us-east-1b│ │ us-east-1a│  │
│  └──────────┘  └──────────┘  └──────────┘   │
│                                              │
│  ┌──────────────────────────────────────────┐│
│  │  S3 / MinIO (segment storage)            ││
│  └──────────────────────────────────────────┘│
└─────────────────────────────────────────────┘
```

---

## Option 2: BYOC (Bring Your Own Cloud)

Your data stays in your AWS account. StreamHouse's control plane manages the server and agents, but reads/writes go to your S3 bucket via cross-account IAM roles.

### Prerequisites

- An AWS account
- An S3 bucket for StreamHouse data
- Permission to create IAM roles in your account

### Step 1: Sign Up on StreamHouse Cloud

Go to the StreamHouse Cloud dashboard and select **BYOC** during onboarding.

### Step 2: Provide Your AWS Details

Enter:
- **AWS Account ID** (12-digit number)
- **S3 Bucket Name** (must already exist in your account)

The platform generates a unique **External ID** for your organization.

### Step 3: Create the IAM Role

StreamHouse needs an IAM role in your account that grants access to your S3 bucket. You have two options:

**Option A: Terraform (recommended)**

Apply the provided Terraform module in your AWS account:

```hcl
module "streamhouse_byoc" {
  source = "github.com/gbram1/streamhouse//infra/terraform/modules/byoc-customer-role"

  streamhouse_control_plane_role_arn = "arn:aws:iam::role/streamhouse-control-plane"
  customer_bucket_name              = "your-streamhouse-bucket"
  external_id                       = "ext_abc123..."  # From Step 2
}

output "role_arn" {
  value = module.streamhouse_byoc.role_arn
}
```

```bash
terraform init
terraform apply
```

**Option B: Manual IAM Setup**

Create an IAM role with this trust policy:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "AWS": "arn:aws:iam::role/streamhouse-control-plane"
      },
      "Action": "sts:AssumeRole",
      "Condition": {
        "StringEquals": {
          "sts:ExternalId": "ext_abc123..."
        }
      }
    }
  ]
}
```

Attach this permissions policy:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::your-streamhouse-bucket",
        "arn:aws:s3:::your-streamhouse-bucket/*"
      ]
    }
  ]
}
```

### Step 4: Validate the Connection

Back in the StreamHouse dashboard, enter the **Role ARN** from Step 3 and click **Validate Connection**. StreamHouse will:

1. Call `sts:AssumeRole` with your role ARN and external ID
2. Attempt a `ListBucket` on your S3 bucket
3. Write and read a test object

If validation succeeds, your BYOC setup is complete.

### Step 5: Start Using StreamHouse

Your organization is created with `deployment_mode: byoc`. All data is stored in your S3 bucket. You interact with StreamHouse through the same APIs (REST, Kafka, gRPC) — the only difference is where the data lives.

Get your API key from the dashboard at **Settings > API Keys**, then:

```bash
# REST
curl -H "Authorization: Bearer sk_live_..." \
  https://your-org.streamhouse.cloud/api/v1/topics

# Kafka
kcat -b your-org.streamhouse.cloud:9092 -t events \
  -X security.protocol=SASL_PLAINTEXT \
  -X sasl.mechanism=PLAIN \
  -X sasl.username=sk_live_... \
  -X sasl.password=sk_live_...
```

### BYOC Architecture

```
┌─────────────────────────────────┐    ┌──────────────────────────┐
│  StreamHouse Control Plane       │    │  Your AWS Account         │
│  (our infrastructure)            │    │                           │
│                                  │    │  ┌─────────────────────┐ │
│  ┌───────────────────────────┐  │    │  │ S3 Bucket            │ │
│  │ Server + Agents           │──┼────┼─▶│ (your data)          │ │
│  │ (API, compute, routing)   │  │    │  └─────────────────────┘ │
│  └───────────────────────────┘  │    │                           │
│                                  │    │  ┌─────────────────────┐ │
│  ┌───────────────────────────┐  │    │  │ IAM Role             │ │
│  │ PostgreSQL (metadata)     │  │    │  │ (cross-account)      │ │
│  └───────────────────────────┘  │    │  └─────────────────────┘ │
└─────────────────────────────────┘    └──────────────────────────┘
         STS AssumeRole ──────────────────────▲
```

### BYOC Security Model

- StreamHouse **never** stores your AWS credentials
- Access uses STS `AssumeRole` with short-lived tokens (~1 hour, auto-refreshed)
- The `ExternalId` condition prevents confused-deputy attacks
- You can revoke access at any time by deleting the IAM role
- Your S3 bucket can have additional controls (VPC endpoints, bucket policies, encryption)

---

## Option 3: Fully Managed

StreamHouse runs everything — infrastructure, storage, compute, monitoring. You just use the API.

### Step 1: Sign Up

Go to the StreamHouse Cloud dashboard and select **Fully Managed** during onboarding.

### Step 2: Create Your Organization

Your organization is created automatically with `deployment_mode: managed`. You get:
- A dedicated endpoint (e.g., `your-org.streamhouse.cloud`)
- REST API on port 443 (HTTPS)
- Kafka protocol on port 9092
- gRPC on port 50051

### Step 3: Create an API Key

Go to **Settings > API Keys** in the dashboard and create a key with the permissions you need.

### Step 4: Start Producing

```bash
# Create a topic
curl -X POST https://your-org.streamhouse.cloud/api/v1/topics \
  -H "Authorization: Bearer sk_live_..." \
  -H "Content-Type: application/json" \
  -d '{"name": "events", "partition_count": 4}'

# Produce messages
curl -X POST https://your-org.streamhouse.cloud/api/v1/produce \
  -H "Authorization: Bearer sk_live_..." \
  -H "Content-Type: application/json" \
  -d '{"topic": "events", "key": "user-1", "value": "{\"action\":\"signup\"}"}'

# Consume
curl "https://your-org.streamhouse.cloud/api/v1/consume?topic=events&partition=0&offset=0" \
  -H "Authorization: Bearer sk_live_..."
```

### Kafka Clients

```python
from confluent_kafka import Producer

producer = Producer({
    'bootstrap.servers': 'your-org.streamhouse.cloud:9092',
    'security.protocol': 'SASL_SSL',
    'sasl.mechanism': 'PLAIN',
    'sasl.username': 'sk_live_...',
    'sasl.password': 'sk_live_...',
})
producer.produce('events', key='user-1', value=b'{"action":"signup"}')
producer.flush()
```

### What You Get

- Auto-scaling agents based on partition count and throughput
- Managed PostgreSQL for metadata
- S3 storage with automated lifecycle policies
- Built-in monitoring (accessible via the dashboard)
- Automatic backups and disaster recovery
- Zero operational overhead

---

## Migrating Between Modes

### Self-Hosted to BYOC/Managed

StreamHouse stores all data as segments in S3 with a consistent path format: `org-{id}/data/{topic}/{partition}/{offset}.seg`. To migrate:

1. Create an organization on StreamHouse Cloud (BYOC or Managed)
2. Copy your S3 segments to the new location using `aws s3 sync`
3. Run the reconcile-from-S3 command on the cloud org to rebuild metadata from the segments

### BYOC to Managed (or vice versa)

Contact support — we can re-point the storage layer without data movement if you're willing to grant temporary access, or help you `s3 sync` between buckets.

---

## Environment Variable Reference

| Variable | Used In | Description |
|---|---|---|
| `STREAMHOUSE_AUTH_ENABLED` | Self-Hosted | Enable API key auth (`true`/`false`) |
| `STREAMHOUSE_ADMIN_KEY` | Self-Hosted | Bootstrap admin key |
| `OIDC_ISSUER_URL` | Self-Hosted | OIDC provider URL for SSO |
| `CLERK_ISSUER_URL` | Self-Hosted | Legacy alias for `OIDC_ISSUER_URL` |
| `BYOC_ENABLED` | Cloud (BYOC) | Enable BYOC S3 client pool |
| `BYOC_CONTROL_PLANE_ROLE_ARN` | Cloud (BYOC) | Control plane role for STS AssumeRole |
| `DATABASE_URL` | All | PostgreSQL connection string |
| `STREAMHOUSE_BUCKET` | All | S3 bucket name |
| `USE_LOCAL_STORAGE` | Self-Hosted (dev) | Use local filesystem instead of S3 |

See [Configuration](configuration.md) for the complete list.
