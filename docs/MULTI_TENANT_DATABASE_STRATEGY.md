# StreamHouse Multi-Tenant Database Strategy

**Created**: 2026-01-27
**Status**: Approved - Option 1 for MVP
**Goal**: Define database isolation strategy for managed service multi-tenancy

---

## Overview

When users sign up for StreamHouse managed service, we need to isolate their data. This document outlines three architecture options and our chosen approach.

---

## Architecture Options

### Option 1: **Shared Database with Organization ID** ⭐ CHOSEN FOR MVP

```
Single PostgreSQL Instance
└── Database: streamhouse_metadata
    ├── Table: organizations
    ├── Table: topics (with organization_id column)
    ├── Table: partitions (with organization_id column)
    ├── Table: consumer_offsets (with organization_id column)
    └── Table: partition_leases (with organization_id column)
```

**Implementation**:
```sql
-- All tables include organization_id for isolation
CREATE TABLE topics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name VARCHAR(255) NOT NULL,
    partition_count INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- Namespace: org_id ensures no collisions
    UNIQUE (organization_id, name)
);

-- Row-Level Security (RLS) policy
ALTER TABLE topics ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON topics
    USING (organization_id = current_setting('app.current_org_id')::UUID);

-- Set org_id at connection level
SET app.current_org_id = '00000000-0000-0000-0000-000000000001';
```

**Pros**:
- ✅ **Most cost-effective**: Single RDS instance for all customers
- ✅ **Simple to implement**: Standard SQL with foreign keys
- ✅ **Easy operations**: One database to backup, monitor, upgrade
- ✅ **PostgreSQL RLS**: Built-in row-level security for extra safety
- ✅ **Scalable**: Good for 100s-1000s of tenants
- ✅ **Fast queries**: All data in one DB, no cross-database joins

**Cons**:
- ❌ **Shared resources**: All tenants compete for same CPU/memory
- ❌ **Noisy neighbor**: Heavy tenant can impact others
- ❌ **Compliance concerns**: Some enterprises require physical isolation

**Cost**:
```
1 RDS instance (db.r6g.xlarge): $600/month
Supporting 500 customers = $1.20/customer/month
```

**Use For**: Free, Starter, Pro tiers (95% of customers)

---

### Option 2: **Separate PostgreSQL Schemas per Tenant**

```
Single PostgreSQL Instance
├── Schema: public (for global tables)
├── Schema: tenant_acme
│   ├── topics
│   ├── partitions
│   ├── consumer_offsets
│   └── partition_leases
├── Schema: tenant_beta
│   ├── topics
│   ├── partitions
│   └── ...
└── Schema: tenant_gamma
    └── ...
```

**Implementation**:
```sql
-- Create schema per organization
CREATE SCHEMA tenant_acme;
CREATE SCHEMA tenant_beta;

-- Set search_path per connection
SET search_path TO tenant_acme;

-- Tables in isolated schema (no org_id needed)
CREATE TABLE tenant_acme.topics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    partition_count INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (name)
);
```

**Pros**:
- ✅ **Better isolation**: Logical separation at schema level
- ✅ **Simpler queries**: No organization_id in WHERE clauses
- ✅ **Per-tenant permissions**: Can grant different users access to specific schemas
- ✅ **Easy migration path**: Can move schema to dedicated DB later

**Cons**:
- ❌ **Connection overhead**: Need to set search_path per connection
- ❌ **More complex migrations**: Must run migrations for each schema
- ❌ **Still shared resources**: Same CPU/memory pool
- ❌ **Schema count limits**: PostgreSQL recommends < 1000 schemas

**Cost**:
```
1 RDS instance (db.r6g.2xlarge): $1,200/month
Supporting 500 customers = $2.40/customer/month
```

**Use For**: Pro+ tier customers who want stronger isolation

---

### Option 3: **Dedicated RDS Instance per Tenant**

```
AWS Account
├── RDS Instance: tenant-acme-metadata (db.t3.small)
│   └── Database: streamhouse_metadata
├── RDS Instance: tenant-beta-metadata (db.t3.small)
│   └── Database: streamhouse_metadata
└── RDS Instance: tenant-gamma-metadata (db.r6g.xlarge)
    └── Database: streamhouse_metadata
```

**Implementation**:
```rust
// Connection routing based on tier
pub struct MetadataStoreRouter {
    shared_pool: PgPool,
    dedicated_pools: HashMap<Uuid, PgPool>,
}

impl MetadataStoreRouter {
    pub async fn get_store_for_org(
        &self,
        org_id: Uuid,
    ) -> Arc<dyn MetadataStore> {
        let org = self.get_organization(org_id).await?;

        match org.tier {
            Tier::Free | Tier::Starter | Tier::Pro => {
                // Shared database
                Arc::new(PostgresMetadataStore::new(self.shared_pool.clone()))
            }
            Tier::Enterprise => {
                // Dedicated RDS instance
                let pool = self.dedicated_pools
                    .entry(org_id)
                    .or_insert_with(|| {
                        PgPool::connect(&org.dedicated_db_url).await?
                    });
                Arc::new(PostgresMetadataStore::new(pool.clone()))
            }
        }
    }
}
```

**Pros**:
- ✅ **Complete physical isolation**: Dedicated CPU, memory, disk
- ✅ **No noisy neighbors**: Performance is predictable
- ✅ **Compliance ready**: Meets strict HIPAA, SOC 2 requirements
- ✅ **Custom sizing**: Can scale per customer needs
- ✅ **Security**: Network-level isolation possible

**Cons**:
- ❌ **Very expensive**: $200-600/month per tenant
- ❌ **Complex operations**: Must manage hundreds of RDS instances
- ❌ **Slow provisioning**: Takes 5-10 minutes to spin up new instance
- ❌ **Backup complexity**: Must backup each instance separately

**Cost**:
```
500 customers × db.t3.small ($50/month) = $25,000/month
= $50/customer/month infrastructure cost alone
```

**Use For**: Enterprise tier only (charge $500-1000/month premium)

---

## S3 Storage Isolation (All Tiers)

**Chosen Approach**: Prefix-based isolation in single bucket

```
s3://streamhouse-prod-data/
├── org-00000001/                    # Acme Corp
│   ├── data/orders/0/00000000.seg
│   ├── data/orders/1/00000000.seg
│   └── data/users/0/00000000.seg
├── org-00000002/                    # Beta Inc
│   ├── data/orders/0/00000000.seg
│   └── data/events/0/00000000.seg
└── org-00000003/                    # Gamma LLC
    └── data/metrics/0/00000000.seg
```

**IAM Policy** (restrict access per prefix):
```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
      "Resource": "arn:aws:s3:::streamhouse-prod-data/org-${org_id}/*"
    }
  ]
}
```

**Enterprise Option**: Dedicated S3 bucket
```
s3://streamhouse-enterprise-acme/
s3://streamhouse-enterprise-beta/
```

**Why this works**:
- S3 is already multi-tenant by design
- Prefix provides logical isolation
- IAM policies enforce access control
- Cost-effective (single bucket)
- Easy to migrate to dedicated bucket later

---

## MVP Implementation Plan (Option 1)

### Phase 1: Database Schema (Week 5)

#### 1.1: Create Organization Tables

```sql
-- migrations-postgres/008_multi_tenancy.sql

-- Organizations (tenants)
CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    plan VARCHAR(50) NOT NULL DEFAULT 'free', -- free, starter, pro, enterprise
    status VARCHAR(50) NOT NULL DEFAULT 'active' -- active, suspended, deleted
);

-- Users
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Organization membership
CREATE TABLE organization_members (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL DEFAULT 'member', -- owner, admin, member, viewer
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (organization_id, user_id)
);

-- API keys (for CLI/SDK authentication)
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(255) NOT NULL UNIQUE,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ
);

CREATE INDEX idx_api_keys_org ON api_keys(organization_id);
```

#### 1.2: Add organization_id to Existing Tables

```sql
-- Add organization_id to topics
ALTER TABLE topics
    ADD COLUMN organization_id UUID REFERENCES organizations(id) ON DELETE CASCADE;

-- Add organization_id to consumer_offsets
ALTER TABLE consumer_offsets
    ADD COLUMN organization_id UUID REFERENCES organizations(id) ON DELETE CASCADE;

-- Update unique constraints
ALTER TABLE topics DROP CONSTRAINT topics_name_key;
ALTER TABLE topics ADD CONSTRAINT topics_org_name_unique UNIQUE (organization_id, name);

-- Add indexes for performance
CREATE INDEX idx_topics_org ON topics(organization_id);
CREATE INDEX idx_consumer_offsets_org ON consumer_offsets(organization_id);
```

#### 1.3: Row-Level Security (Optional but Recommended)

```sql
-- Enable RLS on all multi-tenant tables
ALTER TABLE topics ENABLE ROW LEVEL SECURITY;
ALTER TABLE partitions ENABLE ROW LEVEL SECURITY;
ALTER TABLE consumer_offsets ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their org's data
CREATE POLICY tenant_isolation_topics ON topics
    USING (organization_id = current_setting('app.current_org_id')::UUID);

CREATE POLICY tenant_isolation_partitions ON partitions
    USING (topic IN (
        SELECT name FROM topics WHERE organization_id = current_setting('app.current_org_id')::UUID
    ));

CREATE POLICY tenant_isolation_consumer_offsets ON consumer_offsets
    USING (organization_id = current_setting('app.current_org_id')::UUID);
```

### Phase 2: Code Changes (Week 5-6)

#### 2.1: Update MetadataStore Trait

```rust
// crates/streamhouse-metadata/src/lib.rs

#[async_trait]
pub trait MetadataStore: Send + Sync {
    // Add org_id parameter to all methods
    async fn create_topic(
        &self,
        org_id: Uuid,
        config: TopicConfig,
    ) -> Result<Topic>;

    async fn list_topics(&self, org_id: Uuid) -> Result<Vec<Topic>>;

    async fn get_topic(&self, org_id: Uuid, name: &str) -> Result<Option<Topic>>;

    async fn delete_topic(&self, org_id: Uuid, name: &str) -> Result<()>;

    // Consumer offset methods with org_id
    async fn commit_offset(
        &self,
        org_id: Uuid,
        group_id: &str,
        topic: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<()>;

    // ... update all other methods similarly
}
```

#### 2.2: Update PostgreSQL Implementation

```rust
// crates/streamhouse-metadata/src/postgres_metadata.rs

impl MetadataStore for PostgresMetadataStore {
    async fn create_topic(
        &self,
        org_id: Uuid,
        config: TopicConfig,
    ) -> Result<Topic> {
        // Check quota first
        let quota = self.get_organization_quota(org_id).await?;
        let current_count = self.list_topics(org_id).await?.len();

        if current_count >= quota.max_topics as usize {
            return Err(MetadataError::QuotaExceeded(
                format!("Max topics ({}) reached", quota.max_topics)
            ));
        }

        // Insert with org_id
        let topic = sqlx::query_as::<_, Topic>(
            r#"
            INSERT INTO topics (organization_id, name, partition_count, created_at)
            VALUES ($1, $2, $3, NOW())
            RETURNING id, organization_id, name, partition_count, created_at
            "#,
        )
        .bind(org_id)
        .bind(&config.name)
        .bind(config.partition_count as i32)
        .fetch_one(&self.pool)
        .await?;

        Ok(topic)
    }

    async fn list_topics(&self, org_id: Uuid) -> Result<Vec<Topic>> {
        let topics = sqlx::query_as::<_, Topic>(
            r#"
            SELECT id, organization_id, name, partition_count, created_at
            FROM topics
            WHERE organization_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(topics)
    }
}
```

#### 2.3: Add Quota Management

```sql
-- migrations-postgres/009_quotas.sql

CREATE TABLE organization_quotas (
    organization_id UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
    max_topics INTEGER NOT NULL DEFAULT 10,
    max_partitions_per_topic INTEGER NOT NULL DEFAULT 4,
    max_messages_per_month BIGINT NOT NULL DEFAULT 1000000, -- 1M for free tier
    max_storage_gb INTEGER NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Default quotas by plan
CREATE TABLE plan_quotas (
    plan VARCHAR(50) PRIMARY KEY,
    max_topics INTEGER NOT NULL,
    max_partitions_per_topic INTEGER NOT NULL,
    max_messages_per_month BIGINT NOT NULL,
    max_storage_gb INTEGER NOT NULL
);

INSERT INTO plan_quotas (plan, max_topics, max_partitions_per_topic, max_messages_per_month, max_storage_gb) VALUES
    ('free', 3, 2, 1000000, 1),
    ('starter', 10, 4, 10000000, 10),
    ('pro', 100, 12, 100000000, 100),
    ('enterprise', 999999, 999, 999999999999, 999999);
```

```rust
// Quota enforcement
impl PostgresMetadataStore {
    pub async fn get_organization_quota(&self, org_id: Uuid) -> Result<Quota> {
        let quota = sqlx::query_as::<_, Quota>(
            r#"
            SELECT oq.*
            FROM organization_quotas oq
            WHERE oq.organization_id = $1
            "#,
        )
        .bind(org_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(quota)
    }

    pub async fn check_quota_before_create(&self, org_id: Uuid, operation: &str) -> Result<()> {
        let quota = self.get_organization_quota(org_id).await?;

        match operation {
            "topic" => {
                let count = self.list_topics(org_id).await?.len();
                if count >= quota.max_topics as usize {
                    return Err(MetadataError::QuotaExceeded(
                        format!("Max topics ({}) reached. Upgrade your plan.", quota.max_topics)
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }
}
```

### Phase 3: S3 Prefix Isolation (Week 6)

#### 3.1: Update WriterPool

```rust
// crates/streamhouse-storage/src/writer_pool.rs

impl WriterPool {
    pub fn new(
        object_store: Arc<dyn ObjectStore>,
        org_id: Uuid, // NEW: Organization ID for prefix
        config: WriteConfig,
    ) -> Self {
        Self {
            object_store,
            org_id,
            config,
            writers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_writer(&self, topic: &str, partition_id: u32) -> Result<Arc<PartitionWriter>> {
        // Create path with org_id prefix: org-{uuid}/data/{topic}/{partition}/
        let base_path = format!("org-{}/data/{}/{}", self.org_id, topic, partition_id);

        let writer = PartitionWriter::new(
            Arc::clone(&self.object_store),
            topic.to_string(),
            partition_id,
            base_path, // S3 prefix includes org_id
            self.config.clone(),
        )
        .await?;

        Ok(Arc::new(writer))
    }
}
```

#### 3.2: Update Agent Initialization

```rust
// crates/streamhouse-agent/src/agent.rs

impl Agent {
    pub async fn new(
        org_id: Uuid, // NEW: Organization context
        agent_id: String,
        address: String,
        metadata_store: Arc<dyn MetadataStore>,
        object_store: Arc<dyn ObjectStore>,
    ) -> Result<Self> {
        let writer_pool = Arc::new(WriterPool::new(
            Arc::clone(&object_store),
            org_id, // Pass org_id for S3 prefix isolation
            WriteConfig::default(),
        ));

        Ok(Self {
            agent_id,
            address,
            org_id,
            metadata_store,
            object_store,
            writer_pool,
            // ...
        })
    }
}
```

---

## Migration Path to Option 2 or 3

### Upgrade to Schema-per-Tenant (Pro Tier)

```rust
// When customer upgrades to Pro tier
pub async fn migrate_to_dedicated_schema(
    org_id: Uuid,
    pool: &PgPool,
) -> Result<()> {
    let org = get_organization(org_id, pool).await?;
    let schema_name = format!("tenant_{}", org.slug);

    // 1. Create schema
    sqlx::query(&format!("CREATE SCHEMA {}", schema_name))
        .execute(pool)
        .await?;

    // 2. Copy data from shared tables to new schema
    sqlx::query(&format!(
        r#"
        CREATE TABLE {}.topics AS
        SELECT id, name, partition_count, created_at
        FROM public.topics
        WHERE organization_id = $1
        "#,
        schema_name
    ))
    .bind(org_id)
    .execute(pool)
    .await?;

    // 3. Update organization record
    sqlx::query(
        "UPDATE organizations SET dedicated_schema = $1 WHERE id = $2"
    )
    .bind(&schema_name)
    .bind(org_id)
    .execute(pool)
    .await?;

    // 4. Update connection to use schema
    // Future connections set: SET search_path TO tenant_acme;

    Ok(())
}
```

### Upgrade to Dedicated RDS (Enterprise Tier)

```rust
// Provision dedicated RDS instance via Terraform/AWS API
pub async fn provision_dedicated_rds(
    org_id: Uuid,
) -> Result<String> {
    let org = get_organization(org_id).await?;

    // 1. Create RDS instance via AWS SDK
    let db_url = aws_create_rds_instance(
        &format!("streamhouse-{}", org.slug),
        "db.r6g.xlarge",
    ).await?;

    // 2. Run migrations on new instance
    run_migrations(&db_url).await?;

    // 3. Copy data from shared DB
    migrate_data_to_dedicated_db(org_id, &db_url).await?;

    // 4. Update organization record
    sqlx::query(
        "UPDATE organizations SET dedicated_db_url = $1 WHERE id = $2"
    )
    .bind(&db_url)
    .bind(org_id)
    .execute(pool)
    .await?;

    Ok(db_url)
}
```

---

## Testing Multi-Tenancy

### Test 1: Data Isolation

```rust
#[tokio::test]
async fn test_tenant_isolation() {
    let metadata = PostgresMetadataStore::new(db_url).await?;

    let org1 = create_test_org("acme").await?;
    let org2 = create_test_org("beta").await?;

    // Create topic for org1
    metadata.create_topic(org1.id, TopicConfig {
        name: "orders".to_string(),
        partition_count: 4,
        ..Default::default()
    }).await?;

    // Org2 should NOT see org1's topic
    let org2_topics = metadata.list_topics(org2.id).await?;
    assert_eq!(org2_topics.len(), 0);

    // Org1 should see its own topic
    let org1_topics = metadata.list_topics(org1.id).await?;
    assert_eq!(org1_topics.len(), 1);
}
```

### Test 2: Quota Enforcement

```rust
#[tokio::test]
async fn test_quota_enforcement() {
    let metadata = PostgresMetadataStore::new(db_url).await?;
    let org = create_test_org_with_plan("acme", "free").await?; // free = 3 topics max

    // Create 3 topics (should succeed)
    for i in 0..3 {
        metadata.create_topic(org.id, TopicConfig {
            name: format!("topic-{}", i),
            partition_count: 2,
            ..Default::default()
        }).await?;
    }

    // 4th topic should fail
    let result = metadata.create_topic(org.id, TopicConfig {
        name: "topic-4".to_string(),
        partition_count: 2,
        ..Default::default()
    }).await;

    assert!(matches!(result, Err(MetadataError::QuotaExceeded(_))));
}
```

### Test 3: S3 Prefix Isolation

```rust
#[tokio::test]
async fn test_s3_prefix_isolation() {
    let org1 = Uuid::new_v4();
    let org2 = Uuid::new_v4();

    let writer_pool_1 = WriterPool::new(object_store.clone(), org1, config);
    let writer_pool_2 = WriterPool::new(object_store.clone(), org2, config);

    // Write to org1
    let writer1 = writer_pool_1.get_writer("orders", 0).await?;
    writer1.append(&[record1]).await?;

    // Write to org2
    let writer2 = writer_pool_2.get_writer("orders", 0).await?;
    writer2.append(&[record2]).await?;

    // Verify S3 paths are different
    let segments1 = list_s3_objects(&format!("org-{}/", org1)).await?;
    let segments2 = list_s3_objects(&format!("org-{}/", org2)).await?;

    assert_eq!(segments1.len(), 1);
    assert_eq!(segments2.len(), 1);
    assert_ne!(segments1[0], segments2[0]); // Different paths
}
```

---

## Summary

### MVP Choice: Option 1 (Shared Database with Organization ID)

**Implementation Timeline**:
- Week 5: Database migrations and schema changes
- Week 6: Code updates (MetadataStore trait, PostgreSQL impl)
- Week 6: S3 prefix isolation
- Week 6: Quota enforcement
- Week 7: Testing and validation

**Cost for 500 Customers**:
- PostgreSQL (db.r6g.xlarge): $600/month = $1.20/customer
- S3 (prefix-based): $30/month = $0.06/customer
- **Total**: ~$1.26/customer/month

**Upgrade Path**:
- Pro customers can request schema isolation (Option 2)
- Enterprise customers get dedicated RDS (Option 3)

**Security**:
- Row-level security policies (PostgreSQL RLS)
- Application-level org_id enforcement
- S3 IAM policies per prefix
- API key scoping to organizations

---

**Next Steps**:
1. Review and approve this document
2. Implement Phase 1 (database migrations) in Week 5
3. Update production roadmap with multi-tenancy timeline
4. Begin implementation following MVP plan

**Document Owner**: Engineering Team
**Last Updated**: 2026-01-27
**Status**: Ready for Implementation
