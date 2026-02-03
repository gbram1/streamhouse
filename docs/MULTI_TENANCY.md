# Multi-Tenancy Architecture (Phase 21.5)

StreamHouse supports multi-tenant deployments where multiple organizations share the same infrastructure with complete isolation.

## Overview

Multi-tenancy in StreamHouse provides:

- **Organization isolation**: Each organization has its own topics, consumer groups, and data
- **S3 prefix isolation**: Data is stored under organization-specific prefixes
- **API key authentication**: Secure access via API keys with granular permissions
- **Quota enforcement**: Per-organization limits on resources and throughput
- **Billing integration**: Usage tracking for billing and cost allocation

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        StreamHouse                               │
│                                                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │ Org: Acme   │  │ Org: Beta   │  │ Org: Gamma  │              │
│  │             │  │             │  │             │              │
│  │ Topics:     │  │ Topics:     │  │ Topics:     │              │
│  │  - orders   │  │  - events   │  │  - logs     │              │
│  │  - users    │  │  - metrics  │  │  - traces   │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
│         │                │                │                       │
│         ▼                ▼                ▼                       │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                      S3 Bucket                               ││
│  │                                                               ││
│  │  org-{acme-id}/     org-{beta-id}/     org-{gamma-id}/       ││
│  │   └── data/          └── data/          └── data/            ││
│  │       └── orders/        └── events/        └── logs/        ││
│  │           └── 0/             └── 0/             └── 0/       ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## Key Components

### 1. Organizations

Organizations are the root of the tenant hierarchy:

```rust
pub struct Organization {
    pub id: String,           // UUID
    pub name: String,         // "Acme Corporation"
    pub slug: String,         // "acme-corp"
    pub plan: OrganizationPlan,  // Free, Pro, Enterprise
    pub status: OrganizationStatus,  // Active, Suspended, Deleted
    pub created_at: i64,
    pub settings: HashMap<String, String>,
}
```

### 2. API Keys

API keys authenticate requests and determine tenant context:

```rust
pub struct ApiKey {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub key_prefix: String,       // First 12 chars (e.g., "sk_live_abc1")
    pub permissions: Vec<String>, // ["read", "write", "admin"]
    pub scopes: Vec<String>,      // Topic patterns (empty = all)
    pub expires_at: Option<i64>,
    pub created_at: i64,
}
```

### 3. Quotas

Per-organization resource limits:

```rust
pub struct OrganizationQuota {
    pub max_topics: i32,
    pub max_partitions_per_topic: i32,
    pub max_storage_bytes: i64,
    pub max_produce_bytes_per_sec: i64,
    pub max_consume_bytes_per_sec: i64,
    pub max_requests_per_sec: i32,
    pub max_consumer_groups: i32,
    // ...
}
```

## Plan Limits

| Resource              | Free    | Pro      | Enterprise |
|-----------------------|---------|----------|------------|
| Topics                | 10      | 100      | Unlimited  |
| Partitions/topic      | 12      | 48       | Unlimited  |
| Storage               | 10 GB   | 1 TB     | Custom     |
| Produce throughput    | 10 MB/s | 100 MB/s | Custom     |
| Consume throughput    | 50 MB/s | 500 MB/s | Custom     |
| Requests/sec          | 1,000   | 10,000   | Custom     |
| Consumer groups       | 50      | 500      | Unlimited  |

## Authentication

### REST API

Use the `Authorization` header:

```bash
curl -H "Authorization: Bearer sk_live_abc123..." \
     https://api.streamhouse.io/v1/topics
```

Or the `X-API-Key` header:

```bash
curl -H "X-API-Key: sk_live_abc123..." \
     https://api.streamhouse.io/v1/topics
```

### Kafka Protocol

Use SASL/PLAIN authentication:

```python
from kafka import KafkaProducer

producer = KafkaProducer(
    bootstrap_servers=['localhost:9092'],
    security_protocol='SASL_PLAINTEXT',
    sasl_mechanism='PLAIN',
    sasl_plain_username='sk_live_abc123...',
    sasl_plain_password='sk_live_abc123...',
)
```

## S3 Path Isolation

All data is stored with organization-specific prefixes:

```
s3://streamhouse-data/
├── org-{uuid-1}/
│   └── data/
│       ├── orders/
│       │   ├── 0/
│       │   │   ├── 00000000000000000000.seg
│       │   │   └── 00000000000000010000.seg
│       │   └── 1/
│       └── users/
└── org-{uuid-2}/
    └── data/
        └── events/
```

This is implemented via `TenantObjectStore` which wraps the underlying S3 client:

```rust
// All paths are automatically prefixed
let tenant_store = TenantObjectStore::from_organization_id(
    s3_store,
    "11111111-1111-1111-1111-111111111111",
);

// Request: data/orders/0/00000000000000000000.seg
// Actual:  org-11111111-1111-1111-1111-111111111111/data/orders/0/00000000000000000000.seg
```

## Quota Enforcement

The `QuotaEnforcer` checks and enforces limits:

```rust
let enforcer = QuotaEnforcer::new(metadata_store);

// Check before creating topic
match enforcer.check_topic_creation(&tenant_ctx, partition_count).await? {
    QuotaCheck::Allowed => create_topic(),
    QuotaCheck::Denied(reason) => return Err(QuotaExceeded(reason)),
    QuotaCheck::Warning(reason) => {
        log::warn!("{}", reason);
        create_topic()
    }
}

// Check produce throughput
match enforcer.check_produce(&tenant_ctx, bytes).await? {
    QuotaCheck::Allowed => produce(),
    QuotaCheck::Denied(reason) => return Err(RateLimited(reason)),
    // ...
}
```

## Usage Tracking

Usage is tracked for billing and monitoring:

```rust
// After successful produce
enforcer.track_produce_bytes(&tenant_ctx, bytes_written).await?;

// Get quota summary
let summary = enforcer.get_quota_summary(&tenant_ctx).await?;
println!("Storage: {} / {} bytes", summary.storage_used_bytes, summary.storage_limit_bytes);
```

## API Endpoints

### Organizations

```
POST   /api/v1/organizations              Create organization
GET    /api/v1/organizations              List organizations
GET    /api/v1/organizations/:id          Get organization
PATCH  /api/v1/organizations/:id          Update organization
DELETE /api/v1/organizations/:id          Delete organization
```

### API Keys

```
POST   /api/v1/api-keys                   Create API key
GET    /api/v1/api-keys                   List API keys
DELETE /api/v1/api-keys/:id               Revoke API key
```

### Quotas

```
GET    /api/v1/organizations/:id/quota    Get quotas
PUT    /api/v1/organizations/:id/quota    Set quotas
GET    /api/v1/organizations/:id/usage    Get usage
```

## Database Schema

### Organizations Table

```sql
CREATE TABLE organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    plan TEXT NOT NULL DEFAULT 'free',
    status TEXT NOT NULL DEFAULT 'active',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    settings TEXT DEFAULT '{}'
);
```

### API Keys Table

```sql
CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    key_hash TEXT UNIQUE NOT NULL,
    key_prefix TEXT NOT NULL,
    permissions TEXT NOT NULL DEFAULT '["read","write"]',
    scopes TEXT NOT NULL DEFAULT '[]',
    expires_at INTEGER,
    last_used_at INTEGER,
    created_at INTEGER NOT NULL,
    created_by TEXT
);
```

### Quotas Table

```sql
CREATE TABLE organization_quotas (
    organization_id TEXT PRIMARY KEY REFERENCES organizations(id),
    max_topics INTEGER NOT NULL DEFAULT 10,
    max_partitions_per_topic INTEGER NOT NULL DEFAULT 12,
    max_total_partitions INTEGER NOT NULL DEFAULT 100,
    max_storage_bytes INTEGER NOT NULL DEFAULT 10737418240,
    max_retention_days INTEGER NOT NULL DEFAULT 7,
    max_produce_bytes_per_sec INTEGER NOT NULL DEFAULT 10485760,
    max_consume_bytes_per_sec INTEGER NOT NULL DEFAULT 52428800,
    max_requests_per_sec INTEGER NOT NULL DEFAULT 1000,
    max_consumer_groups INTEGER NOT NULL DEFAULT 50,
    max_schemas INTEGER NOT NULL DEFAULT 100,
    max_schema_versions_per_subject INTEGER NOT NULL DEFAULT 100,
    max_connections INTEGER NOT NULL DEFAULT 100
);
```

## Migration from Single-Tenant

Existing data without organization IDs uses the default organization:

```rust
pub const DEFAULT_ORGANIZATION_ID: &str = "00000000-0000-0000-0000-000000000000";
```

To migrate:
1. Create the new organizations
2. Update existing records with organization_id
3. Move S3 data to organization prefixes
4. Update segment metadata with new S3 paths

## Security Considerations

1. **API Key Storage**: Keys are never stored; only SHA-256 hashes
2. **Path Isolation**: S3 prefixes prevent cross-tenant access
3. **Permission Scoping**: Keys can be scoped to specific topics
4. **Expiration**: Keys can have expiration dates
5. **Audit Logging**: All authentication attempts are logged

## Configuration

Enable multi-tenancy in the server configuration:

```toml
[multi_tenancy]
enabled = true
allow_anonymous = false  # Require authentication
default_plan = "free"
```

## Monitoring

Key metrics for multi-tenancy:

- `streamhouse_organizations_total` - Number of organizations
- `streamhouse_api_keys_total` - Number of active API keys
- `streamhouse_quota_usage_percent{org_id, metric}` - Quota usage percentage
- `streamhouse_auth_failures_total{reason}` - Authentication failures
