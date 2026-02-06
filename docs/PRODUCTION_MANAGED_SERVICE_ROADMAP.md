# StreamHouse - Production Managed Service Roadmap

**Created**: 2026-01-27
**Status**: 🚀 Ready to Execute
**Goal**: Transform StreamHouse from a working distributed system into a production-ready managed streaming service
**Duration**: 6 months (24 weeks)

---

## Executive Summary

StreamHouse has completed Phases 1-7 with a **fully functional distributed streaming platform**. This roadmap details the work required to launch as a **production-grade managed service** with:

- ✅ **Multi-tenant SaaS platform** with web console and billing
- ✅ **Enterprise features** (schema registry, exactly-once, multi-region)
- ✅ **Production operations** (monitoring, auto-scaling, backups, compliance)
- ✅ **Kubernetes deployment** on AWS (EKS + RDS + S3)
- ✅ **Self-service onboarding** with 5-minute quickstart

### Current State (Phases 1-7 Complete)

```
✅ Core Streaming: Producer, Consumer, Topics, Partitions
✅ Distributed Agents: Multi-agent coordination, partition leases
✅ Storage: MinIO/S3 with LZ4 compression (162 segments verified)
✅ Metadata: SQLite/PostgreSQL for topics, partitions, offsets
✅ gRPC API: ProducerService, topic management, streamctl CLI
✅ Observability: Structured logging, health checks, monitoring queries
```

### Target State (Month 6)

```
🎯 Managed Service Platform:
   - Web console for topic management
   - Multi-tenant isolation with billing
   - Schema registry for type safety
   - Exactly-once semantics for transactions
   - Auto-scaling agents on Kubernetes
   - Multi-region replication (optional)
   - Compliance-ready (SOC 2, GDPR)

🎯 Production Infrastructure:
   - Kubernetes on AWS EKS
   - PostgreSQL on AWS RDS
   - S3 for segment storage
   - CloudWatch + Prometheus + Grafana
   - Automated backups and DR

🎯 User Experience:
   - Sign up → Create topic → Produce/Consume in 5 minutes
   - Self-service dashboard
   - Usage-based billing ($50-$200-$Enterprise)
   - Documentation and API reference
```

---

## Phase 8: Production Infrastructure (Weeks 1-4)

**Goal**: Deploy StreamHouse on AWS with Kubernetes, RDS PostgreSQL, and S3

### 8.1: Kubernetes Deployment (Week 1)

#### Deliverables

1. **Helm Chart** (`deploy/kubernetes/streamhouse/`)
   ```yaml
   # values.yaml
   replicaCount: 3

   image:
     repository: streamhouse/agent
     tag: "v1.0.0"
     pullPolicy: IfNotPresent

   service:
     type: LoadBalancer
     grpcPort: 50051
     metricsPort: 8080

   metadata:
     postgresUrl: "postgresql://streamhouse:password@postgres:5432/streamhouse_metadata"

   storage:
     s3:
       bucket: "streamhouse-prod-data"
       region: "us-east-1"

   autoscaling:
     enabled: true
     minReplicas: 3
     maxReplicas: 10
     targetCPUUtilizationPercentage: 70
   ```

2. **Agent StatefulSet** (`deploy/kubernetes/streamhouse/templates/agent-statefulset.yaml`)
   ```yaml
   apiVersion: apps/v1
   kind: StatefulSet
   metadata:
     name: streamhouse-agent
   spec:
     replicas: 3
     selector:
       matchLabels:
         app: streamhouse-agent
     template:
       spec:
         containers:
         - name: agent
           image: streamhouse/agent:v1.0.0
           ports:
           - containerPort: 50051
             name: grpc
           - containerPort: 8080
             name: metrics
           env:
           - name: AGENT_ID
             valueFrom:
               fieldRef:
                 fieldPath: metadata.name
           - name: METADATA_DB
             valueFrom:
               secretKeyRef:
                 name: streamhouse-secrets
                 key: postgres-url
           - name: AWS_REGION
             value: "us-east-1"
           - name: STREAMHOUSE_BUCKET
             value: "streamhouse-prod-data"
           livenessProbe:
             httpGet:
               path: /health
               port: 8080
             initialDelaySeconds: 30
             periodSeconds: 10
           readinessProbe:
             httpGet:
               path: /ready
               port: 8080
             initialDelaySeconds: 10
             periodSeconds: 5
   ```

3. **Service** (`deploy/kubernetes/streamhouse/templates/service.yaml`)
   ```yaml
   apiVersion: v1
   kind: Service
   metadata:
     name: streamhouse-grpc
     annotations:
       service.beta.kubernetes.io/aws-load-balancer-type: "nlb"
   spec:
     type: LoadBalancer
     ports:
     - port: 50051
       targetPort: 50051
       protocol: TCP
       name: grpc
     selector:
       app: streamhouse-agent
   ```

4. **IRSA (IAM Roles for Service Accounts)** for S3 access
   ```bash
   # Create IAM role for S3 access
   eksctl create iamserviceaccount \
     --name streamhouse-agent \
     --namespace default \
     --cluster streamhouse-prod \
     --attach-policy-arn arn:aws:iam::aws:policy/AmazonS3FullAccess \
     --approve
   ```

#### Testing

```bash
# Deploy to EKS
helm install streamhouse ./deploy/kubernetes/streamhouse \
  --set metadata.postgresUrl="postgresql://..." \
  --set storage.s3.bucket="streamhouse-prod-data"

# Verify pods
kubectl get pods -l app=streamhouse-agent
# Expected: 3 pods running

# Check logs
kubectl logs -f streamhouse-agent-0

# Test gRPC endpoint
export STREAMHOUSE_ADDR=$(kubectl get svc streamhouse-grpc -o jsonpath='{.status.loadBalancer.ingress[0].hostname}'):50051
cargo run --bin streamctl -- topic list
```

#### Success Criteria

- ✅ 3 agent pods running on EKS
- ✅ Network Load Balancer exposes gRPC port
- ✅ Agents connect to RDS PostgreSQL
- ✅ Agents write segments to S3
- ✅ Health checks pass
- ✅ Auto-scaling based on CPU

---

### 8.2: AWS RDS PostgreSQL (Week 1)

#### Deliverables

1. **RDS Instance** (Terraform)
   ```hcl
   # deploy/terraform/rds.tf
   resource "aws_db_instance" "streamhouse_metadata" {
     identifier           = "streamhouse-prod-metadata"
     engine              = "postgres"
     engine_version      = "15.4"
     instance_class      = "db.r6g.xlarge"
     allocated_storage   = 100
     storage_type        = "gp3"
     storage_encrypted   = true

     db_name  = "streamhouse_metadata"
     username = "streamhouse"
     password = var.db_password

     multi_az               = true
     backup_retention_period = 7
     backup_window          = "03:00-04:00"
     maintenance_window     = "mon:04:00-mon:05:00"

     vpc_security_group_ids = [aws_security_group.streamhouse_db.id]
     db_subnet_group_name   = aws_db_subnet_group.streamhouse.name

     enabled_cloudwatch_logs_exports = ["postgresql"]

     tags = {
       Name        = "streamhouse-prod-metadata"
       Environment = "production"
     }
   }
   ```

2. **Database Migrations** (Run on startup)
   ```rust
   // crates/streamhouse-metadata/src/postgres_migrations.rs
   pub async fn run_migrations(pool: &PgPool) -> Result<()> {
       sqlx::migrate!("./migrations-postgres")
           .run(pool)
           .await?;
       Ok(())
   }
   ```

3. **Connection Pooling** (Already exists in codebase)
   ```rust
   // Increase pool size for production
   let pool = PgPoolOptions::new()
       .max_connections(50)
       .acquire_timeout(Duration::from_secs(3))
       .connect(&database_url)
       .await?;
   ```

#### Testing

```bash
# Apply Terraform
cd deploy/terraform
terraform init
terraform apply

# Get RDS endpoint
export RDS_ENDPOINT=$(terraform output -raw rds_endpoint)

# Test connection
psql -h $RDS_ENDPOINT -U streamhouse -d streamhouse_metadata -c "SELECT version();"

# Run migrations
cargo run --bin streamhouse-migrate -- \
  --database-url "postgresql://streamhouse:password@$RDS_ENDPOINT:5432/streamhouse_metadata"
```

#### Success Criteria

- ✅ RDS instance running in Multi-AZ
- ✅ Automated backups enabled (7-day retention)
- ✅ Encryption at rest enabled
- ✅ Connection pooling configured
- ✅ Migrations applied successfully
- ✅ CloudWatch logs enabled

---

### 8.3: S3 Storage (Week 2)

#### Deliverables

1. **S3 Bucket** (Terraform)
   ```hcl
   # deploy/terraform/s3.tf
   resource "aws_s3_bucket" "streamhouse_data" {
     bucket = "streamhouse-prod-data"

     tags = {
       Name        = "streamhouse-prod-data"
       Environment = "production"
     }
   }

   resource "aws_s3_bucket_versioning" "streamhouse_data" {
     bucket = aws_s3_bucket.streamhouse_data.id

     versioning_configuration {
       status = "Enabled"
     }
   }

   resource "aws_s3_bucket_lifecycle_configuration" "streamhouse_data" {
     bucket = aws_s3_bucket.streamhouse_data.id

     rule {
       id     = "archive-old-segments"
       status = "Enabled"

       transition {
         days          = 30
         storage_class = "STANDARD_IA"
       }

       transition {
         days          = 90
         storage_class = "GLACIER"
       }

       expiration {
         days = 365
       }
     }
   }

   resource "aws_s3_bucket_server_side_encryption_configuration" "streamhouse_data" {
     bucket = aws_s3_bucket.streamhouse_data.id

     rule {
       apply_server_side_encryption_by_default {
         sse_algorithm = "AES256"
       }
     }
   }
   ```

2. **IAM Policy** for agent access
   ```hcl
   resource "aws_iam_policy" "streamhouse_s3_access" {
     name = "streamhouse-s3-access"

     policy = jsonencode({
       Version = "2012-10-17"
       Statement = [
         {
           Effect = "Allow"
           Action = [
             "s3:GetObject",
             "s3:PutObject",
             "s3:DeleteObject",
             "s3:ListBucket"
           ]
           Resource = [
             "${aws_s3_bucket.streamhouse_data.arn}",
             "${aws_s3_bucket.streamhouse_data.arn}/*"
           ]
         }
       ]
     })
   }
   ```

#### Testing

```bash
# Apply Terraform
terraform apply

# Test S3 access from agent pod
kubectl exec -it streamhouse-agent-0 -- sh
aws s3 ls s3://streamhouse-prod-data/
aws s3 cp test.txt s3://streamhouse-prod-data/test.txt

# Verify lifecycle policy
aws s3api get-bucket-lifecycle-configuration --bucket streamhouse-prod-data
```

#### Success Criteria

- ✅ S3 bucket created with versioning
- ✅ Lifecycle policies configured (30d → IA, 90d → Glacier)
- ✅ Encryption at rest enabled (AES256)
- ✅ IAM policy grants agent access
- ✅ Agents can write/read segments
- ✅ Cross-region replication configured (optional)

---

### 8.4: Monitoring & Observability (Week 3-4)

#### Deliverables

1. **Prometheus Operator** (Helm)
   ```bash
   helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
   helm install prometheus prometheus-community/kube-prometheus-stack \
     --namespace monitoring \
     --create-namespace
   ```

2. **ServiceMonitor** for agent metrics
   ```yaml
   # deploy/kubernetes/streamhouse/templates/servicemonitor.yaml
   apiVersion: monitoring.coreos.com/v1
   kind: ServiceMonitor
   metadata:
     name: streamhouse-agent
   spec:
     selector:
       matchLabels:
         app: streamhouse-agent
     endpoints:
     - port: metrics
       interval: 15s
       path: /metrics
   ```

3. **Grafana Dashboard** (Import existing)
   ```bash
   # Import grafana/dashboards/streamhouse-overview.json
   kubectl port-forward -n monitoring svc/prometheus-grafana 3000:80
   open http://localhost:3000
   # Login: admin / prom-operator
   # Import dashboard from file
   ```

4. **CloudWatch Integration**
   ```yaml
   # deploy/kubernetes/streamhouse/templates/fluentbit-config.yaml
   apiVersion: v1
   kind: ConfigMap
   metadata:
     name: fluent-bit-config
   data:
     fluent-bit.conf: |
       [OUTPUT]
           Name cloudwatch_logs
           Match *
           region us-east-1
           log_group_name /aws/eks/streamhouse-prod
           log_stream_prefix agent-
           auto_create_group true
   ```

5. **Alerts** (PrometheusRule)
   ```yaml
   apiVersion: monitoring.coreos.com/v1
   kind: PrometheusRule
   metadata:
     name: streamhouse-alerts
   spec:
     groups:
     - name: streamhouse
       interval: 30s
       rules:
       - alert: AgentDown
         expr: up{job="streamhouse-agent"} == 0
         for: 2m
         annotations:
           summary: "StreamHouse agent is down"

       - alert: HighConsumerLag
         expr: streamhouse_consumer_lag_records > 10000
         for: 5m
         annotations:
           summary: "Consumer lag > 10,000 messages"

       - alert: OrphanedPartitions
         expr: streamhouse_orphaned_partitions > 0
         for: 1m
         annotations:
           summary: "Partitions without assigned owner"
   ```

#### Testing

```bash
# Check Prometheus targets
kubectl port-forward -n monitoring svc/prometheus-operated 9090:9090
open http://localhost:9090/targets

# Query metrics
curl http://localhost:9090/api/v1/query?query=streamhouse_records_sent_total

# Test alerts
kubectl get prometheusrules -n monitoring
```

#### Success Criteria

- ✅ Prometheus scraping agent metrics
- ✅ Grafana dashboard showing throughput, latency, lag
- ✅ CloudWatch logs receiving agent logs
- ✅ Alerts configured for critical failures
- ✅ PagerDuty integration (optional)

---

## Phase 9: Multi-Tenancy & Platform (Weeks 5-10)

**Goal**: Build SaaS platform with web console, authentication, and billing

### 9.1: Multi-Tenant Data Model (Week 5)

#### Deliverables

1. **Organization Schema** (New migration)
   ```sql
   -- migrations-postgres/008_multi_tenancy.sql
   CREATE TABLE organizations (
       id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
       name VARCHAR(255) NOT NULL,
       slug VARCHAR(255) NOT NULL UNIQUE,
       created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
       plan VARCHAR(50) NOT NULL DEFAULT 'free', -- free, starter, pro, enterprise
       status VARCHAR(50) NOT NULL DEFAULT 'active' -- active, suspended, deleted
   );

   CREATE TABLE users (
       id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
       email VARCHAR(255) NOT NULL UNIQUE,
       password_hash VARCHAR(255) NOT NULL,
       name VARCHAR(255),
       created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
   );

   CREATE TABLE organization_members (
       organization_id UUID NOT NULL REFERENCES organizations(id),
       user_id UUID NOT NULL REFERENCES users(id),
       role VARCHAR(50) NOT NULL DEFAULT 'member', -- owner, admin, member, viewer
       created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
       PRIMARY KEY (organization_id, user_id)
   );

   CREATE TABLE api_keys (
       id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
       organization_id UUID NOT NULL REFERENCES organizations(id),
       name VARCHAR(255) NOT NULL,
       key_hash VARCHAR(255) NOT NULL UNIQUE,
       last_used_at TIMESTAMPTZ,
       created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
       expires_at TIMESTAMPTZ
   );

   -- Add organization_id to existing tables
   ALTER TABLE topics ADD COLUMN organization_id UUID REFERENCES organizations(id);
   ALTER TABLE consumer_offsets ADD COLUMN organization_id UUID REFERENCES organizations(id);

   -- Namespace isolation: topics are org.topic_name internally
   CREATE INDEX idx_topics_org_name ON topics(organization_id, name);
   ```

2. **Quota Management**
   ```sql
   CREATE TABLE organization_quotas (
       organization_id UUID PRIMARY KEY REFERENCES organizations(id),
       max_topics INTEGER NOT NULL DEFAULT 10,
       max_partitions_per_topic INTEGER NOT NULL DEFAULT 4,
       max_messages_per_month BIGINT NOT NULL DEFAULT 1000000, -- 1M for free tier
       max_storage_gb INTEGER NOT NULL DEFAULT 1,
       updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
   );

   CREATE TABLE organization_usage (
       organization_id UUID NOT NULL REFERENCES organizations(id),
       month DATE NOT NULL,
       messages_produced BIGINT NOT NULL DEFAULT 0,
       messages_consumed BIGINT NOT NULL DEFAULT 0,
       storage_gb_hours NUMERIC(10,2) NOT NULL DEFAULT 0,
       PRIMARY KEY (organization_id, month)
   );
   ```

3. **MetadataStore Updates** (Add org_id to all methods)
   ```rust
   // crates/streamhouse-metadata/src/lib.rs
   #[async_trait]
   pub trait MetadataStore: Send + Sync {
       async fn create_topic(
           &self,
           org_id: Uuid,
           name: &str,
           partition_count: u32,
       ) -> Result<Topic>;

       async fn list_topics(&self, org_id: Uuid) -> Result<Vec<Topic>>;

       // ... update all methods with org_id parameter
   }
   ```

#### Testing

```bash
# Run migrations
cargo run --bin streamhouse-migrate

# Test tenant isolation
psql -h $RDS_ENDPOINT -U streamhouse -d streamhouse_metadata <<SQL
-- Create test organizations
INSERT INTO organizations (id, name, slug) VALUES
  ('00000000-0000-0000-0000-000000000001', 'Acme Corp', 'acme'),
  ('00000000-0000-0000-0000-000000000002', 'Beta Inc', 'beta');

-- Create topics for each org
INSERT INTO topics (organization_id, name, partition_count) VALUES
  ('00000000-0000-0000-0000-000000000001', 'orders', 4),
  ('00000000-0000-0000-0000-000000000002', 'orders', 2);

-- Verify isolation
SELECT organization_id, name FROM topics;
SQL
```

#### Success Criteria

- ✅ Organizations can be created
- ✅ Topics isolated by organization_id
- ✅ Quotas enforced (max topics, partitions, messages)
- ✅ No cross-organization data access
- ✅ API keys scoped to organizations

---

### 9.2: Authentication Service (Week 6)

#### Deliverables

1. **Auth Service** (New crate: `crates/streamhouse-auth/`)
   ```rust
   // crates/streamhouse-auth/src/lib.rs
   use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
   use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
   use uuid::Uuid;

   #[derive(Debug, Serialize, Deserialize)]
   pub struct Claims {
       pub sub: Uuid,           // user_id
       pub org: Uuid,           // organization_id
       pub role: String,        // owner, admin, member, viewer
       pub exp: usize,          // expiration
   }

   pub struct AuthService {
       jwt_secret: Vec<u8>,
       metadata_store: Arc<dyn MetadataStore>,
   }

   impl AuthService {
       pub async fn signup(
           &self,
           email: &str,
           password: &str,
           org_name: &str,
       ) -> Result<(Uuid, String)> {
           // Hash password with Argon2
           let salt = SaltString::generate(&mut OsRng);
           let argon2 = Argon2::default();
           let password_hash = argon2
               .hash_password(password.as_bytes(), &salt)?
               .to_string();

           // Create organization and user
           let org_id = self.metadata_store
               .create_organization(org_name)
               .await?;

           let user_id = self.metadata_store
               .create_user(email, &password_hash)
               .await?;

           self.metadata_store
               .add_organization_member(org_id, user_id, "owner")
               .await?;

           // Generate JWT
           let token = self.generate_jwt(user_id, org_id, "owner")?;

           Ok((user_id, token))
       }

       pub async fn login(&self, email: &str, password: &str) -> Result<String> {
           let user = self.metadata_store
               .get_user_by_email(email)
               .await?
               .ok_or(AuthError::InvalidCredentials)?;

           // Verify password
           let parsed_hash = PasswordHash::new(&user.password_hash)?;
           Argon2::default()
               .verify_password(password.as_bytes(), &parsed_hash)?;

           // Get user's organization (use first if multiple)
           let membership = self.metadata_store
               .get_user_organizations(user.id)
               .await?
               .first()
               .ok_or(AuthError::NoOrganization)?;

           // Generate JWT
           let token = self.generate_jwt(
               user.id,
               membership.organization_id,
               &membership.role,
           )?;

           Ok(token)
       }

       pub fn verify_jwt(&self, token: &str) -> Result<Claims> {
           let token_data = decode::<Claims>(
               token,
               &DecodingKey::from_secret(&self.jwt_secret),
               &Validation::default(),
           )?;

           Ok(token_data.claims)
       }

       fn generate_jwt(&self, user_id: Uuid, org_id: Uuid, role: &str) -> Result<String> {
           let expiration = chrono::Utc::now()
               .checked_add_signed(chrono::Duration::hours(24))
               .unwrap()
               .timestamp() as usize;

           let claims = Claims {
               sub: user_id,
               org: org_id,
               role: role.to_string(),
               exp: expiration,
           };

           let token = encode(
               &Header::default(),
               &claims,
               &EncodingKey::from_secret(&self.jwt_secret),
           )?;

           Ok(token)
       }
   }
   ```

2. **gRPC Interceptor** (Add JWT validation)
   ```rust
   // crates/streamhouse-agent/src/auth_interceptor.rs
   use tonic::{Request, Status};

   pub struct AuthInterceptor {
       auth_service: Arc<AuthService>,
   }

   impl tonic::service::Interceptor for AuthInterceptor {
       fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
           let token = request
               .metadata()
               .get("authorization")
               .and_then(|v| v.to_str().ok())
               .and_then(|v| v.strip_prefix("Bearer "))
               .ok_or_else(|| Status::unauthenticated("Missing token"))?;

           let claims = self.auth_service
               .verify_jwt(token)
               .map_err(|_| Status::unauthenticated("Invalid token"))?;

           // Inject claims into request extensions
           request.extensions_mut().insert(claims);

           Ok(request)
       }
   }
   ```

3. **API Key Authentication** (Alternative to JWT for CLI)
   ```rust
   // API keys for CLI tools
   pub async fn create_api_key(
       &self,
       org_id: Uuid,
       name: &str,
   ) -> Result<String> {
       let key = format!("sk_live_{}", Alphanumeric.sample_string(&mut rand::thread_rng(), 32));
       let key_hash = argon2::hash_encoded(key.as_bytes(), b"somesalt", &Default::default())?;

       self.metadata_store
           .create_api_key(org_id, name, &key_hash)
           .await?;

       Ok(key) // Return once, never stored in plaintext
   }
   ```

#### Testing

```bash
# Test signup
curl -X POST http://localhost:8080/api/v1/auth/signup \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "secure_password_123",
    "org_name": "My Company"
  }'
# Response: {"user_id": "...", "token": "eyJ..."}

# Test login
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "secure_password_123"
  }'
# Response: {"token": "eyJ..."}

# Test authenticated gRPC call
cargo run --bin streamctl -- \
  --token "eyJ..." \
  topic list
```

#### Success Criteria

- ✅ Users can sign up and create organizations
- ✅ Password hashing with Argon2
- ✅ JWT tokens issued on login
- ✅ gRPC calls require valid JWT or API key
- ✅ Claims injected into request context
- ✅ Token expiration enforced

---

### 9.3: Web Console (Weeks 7-9)

#### Deliverables

1. **Frontend Stack** (Next.js + React)
   ```bash
   # Create Next.js app
   cd web
   npx create-next-app@latest streamhouse-console --typescript --tailwind --app

   # Install dependencies
   npm install @tanstack/react-query axios recharts @headlessui/react heroicons
   ```

2. **Project Structure**
   ```
   web/streamhouse-console/
   ├── src/
   │   ├── app/
   │   │   ├── (auth)/
   │   │   │   ├── login/page.tsx
   │   │   │   └── signup/page.tsx
   │   │   ├── (dashboard)/
   │   │   │   ├── layout.tsx
   │   │   │   ├── page.tsx              # Dashboard home
   │   │   │   ├── topics/
   │   │   │   │   ├── page.tsx          # List topics
   │   │   │   │   ├── [id]/page.tsx     # Topic details
   │   │   │   │   └── new/page.tsx      # Create topic
   │   │   │   ├── consumers/page.tsx     # Consumer groups
   │   │   │   ├── monitoring/page.tsx    # Metrics & alerts
   │   │   │   ├── settings/page.tsx      # Org settings
   │   │   │   └── billing/page.tsx       # Usage & billing
   │   │   └── layout.tsx
   │   ├── components/
   │   │   ├── TopicList.tsx
   │   │   ├── TopicCreate.tsx
   │   │   ├── MetricsChart.tsx
   │   │   ├── ConsumerLagTable.tsx
   │   │   └── Sidebar.tsx
   │   ├── lib/
   │   │   ├── api.ts                     # API client
   │   │   ├── auth.ts                    # Auth helpers
   │   │   └── types.ts                   # TypeScript types
   │   └── hooks/
   │       ├── useTopics.ts
   │       ├── useMetrics.ts
   │       └── useAuth.ts
   ```

3. **Key Pages**

   **Dashboard Home** (`src/app/(dashboard)/page.tsx`)
   ```tsx
   'use client';

   import { useQuery } from '@tanstack/react-query';
   import { api } from '@/lib/api';
   import { MetricsChart } from '@/components/MetricsChart';

   export default function DashboardPage() {
     const { data: stats } = useQuery({
       queryKey: ['dashboard-stats'],
       queryFn: () => api.getDashboardStats(),
       refetchInterval: 5000, // Refresh every 5s
     });

     return (
       <div className="space-y-6">
         <h1 className="text-2xl font-bold">Dashboard</h1>

         {/* Summary Cards */}
         <div className="grid grid-cols-4 gap-4">
           <StatCard
             title="Topics"
             value={stats?.topicCount ?? 0}
             icon={FolderIcon}
           />
           <StatCard
             title="Messages/sec"
             value={stats?.throughput ?? 0}
             icon={ArrowUpIcon}
           />
           <StatCard
             title="Consumer Lag"
             value={stats?.totalLag ?? 0}
             icon={ClockIcon}
             alert={stats?.totalLag > 1000}
           />
           <StatCard
             title="Storage"
             value={`${stats?.storageGB ?? 0} GB`}
             icon={DatabaseIcon}
           />
         </div>

         {/* Throughput Chart */}
         <div className="bg-white p-6 rounded-lg shadow">
           <h2 className="text-lg font-semibold mb-4">Throughput (last 1 hour)</h2>
           <MetricsChart
             metrics={stats?.throughputHistory ?? []}
             xKey="timestamp"
             yKey="messagesPerSecond"
           />
         </div>

         {/* Recent Topics */}
         <div className="bg-white p-6 rounded-lg shadow">
           <h2 className="text-lg font-semibold mb-4">Recent Topics</h2>
           <TopicList topics={stats?.recentTopics ?? []} limit={5} />
         </div>
       </div>
     );
   }
   ```

   **Topic List** (`src/app/(dashboard)/topics/page.tsx`)
   ```tsx
   'use client';

   import { useQuery } from '@tanstack/react-query';
   import { api } from '@/lib/api';
   import { PlusIcon } from '@heroicons/react/24/outline';
   import Link from 'next/link';

   export default function TopicsPage() {
     const { data: topics, isLoading } = useQuery({
       queryKey: ['topics'],
       queryFn: () => api.listTopics(),
     });

     return (
       <div className="space-y-6">
         <div className="flex justify-between items-center">
           <h1 className="text-2xl font-bold">Topics</h1>
           <Link
             href="/topics/new"
             className="btn btn-primary flex items-center"
           >
             <PlusIcon className="h-5 w-5 mr-2" />
             Create Topic
           </Link>
         </div>

         {isLoading ? (
           <div>Loading...</div>
         ) : (
           <div className="bg-white shadow overflow-hidden sm:rounded-lg">
             <table className="min-w-full divide-y divide-gray-200">
               <thead className="bg-gray-50">
                 <tr>
                   <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                     Name
                   </th>
                   <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                     Partitions
                   </th>
                   <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                     Messages
                   </th>
                   <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                     Created
                   </th>
                   <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">
                     Actions
                   </th>
                 </tr>
               </thead>
               <tbody className="bg-white divide-y divide-gray-200">
                 {topics?.map((topic) => (
                   <tr key={topic.id} className="hover:bg-gray-50">
                     <td className="px-6 py-4">
                       <Link
                         href={`/topics/${topic.id}`}
                         className="text-indigo-600 hover:text-indigo-900"
                       >
                         {topic.name}
                       </Link>
                     </td>
                     <td className="px-6 py-4">{topic.partitionCount}</td>
                     <td className="px-6 py-4">
                       {topic.totalMessages.toLocaleString()}
                     </td>
                     <td className="px-6 py-4 text-sm text-gray-500">
                       {new Date(topic.createdAt).toLocaleDateString()}
                     </td>
                     <td className="px-6 py-4 text-right">
                       <button className="text-red-600 hover:text-red-900">
                         Delete
                       </button>
                     </td>
                   </tr>
                 ))}
               </tbody>
             </table>
           </div>
         )}
       </div>
     );
   }
   ```

   **Create Topic** (`src/app/(dashboard)/topics/new/page.tsx`)
   ```tsx
   'use client';

   import { useMutation, useQueryClient } from '@tanstack/react-query';
   import { api } from '@/lib/api';
   import { useRouter } from 'next/navigation';
   import { useState } from 'react';

   export default function NewTopicPage() {
     const router = useRouter();
     const queryClient = useQueryClient();
     const [name, setName] = useState('');
     const [partitions, setPartitions] = useState(4);

     const createMutation = useMutation({
       mutationFn: (data: { name: string; partitionCount: number }) =>
         api.createTopic(data.name, data.partitionCount),
       onSuccess: () => {
         queryClient.invalidateQueries({ queryKey: ['topics'] });
         router.push('/topics');
       },
     });

     const handleSubmit = (e: React.FormEvent) => {
       e.preventDefault();
       createMutation.mutate({ name, partitionCount: partitions });
     };

     return (
       <div className="max-w-2xl mx-auto">
         <h1 className="text-2xl font-bold mb-6">Create Topic</h1>

         <form onSubmit={handleSubmit} className="space-y-6 bg-white p-6 rounded-lg shadow">
           <div>
             <label className="block text-sm font-medium text-gray-700">
               Topic Name
             </label>
             <input
               type="text"
               value={name}
               onChange={(e) => setName(e.target.value)}
               className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
               placeholder="orders"
               required
             />
             <p className="mt-2 text-sm text-gray-500">
               Lowercase letters, numbers, hyphens, and underscores only
             </p>
           </div>

           <div>
             <label className="block text-sm font-medium text-gray-700">
               Partitions
             </label>
             <input
               type="number"
               value={partitions}
               onChange={(e) => setPartitions(parseInt(e.target.value))}
               className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
               min={1}
               max={100}
               required
             />
             <p className="mt-2 text-sm text-gray-500">
               More partitions = higher parallelism. Cannot be changed later.
             </p>
           </div>

           <div className="flex justify-end space-x-4">
             <button
               type="button"
               onClick={() => router.back()}
               className="btn btn-secondary"
             >
               Cancel
             </button>
             <button
               type="submit"
               disabled={createMutation.isPending}
               className="btn btn-primary"
             >
               {createMutation.isPending ? 'Creating...' : 'Create Topic'}
             </button>
           </div>
         </form>
       </div>
     );
   }
   ```

4. **API Client** (`src/lib/api.ts`)
   ```typescript
   import axios from 'axios';

   const client = axios.create({
     baseURL: process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080/api/v1',
   });

   // Add JWT token to all requests
   client.interceptors.request.use((config) => {
     const token = localStorage.getItem('token');
     if (token) {
       config.headers.Authorization = `Bearer ${token}`;
     }
     return config;
   });

   export const api = {
     // Auth
     login: (email: string, password: string) =>
       client.post('/auth/login', { email, password }).then((r) => r.data),

     signup: (email: string, password: string, orgName: string) =>
       client.post('/auth/signup', { email, password, org_name: orgName }).then((r) => r.data),

     // Topics
     listTopics: () =>
       client.get('/topics').then((r) => r.data),

     getTopic: (id: string) =>
       client.get(`/topics/${id}`).then((r) => r.data),

     createTopic: (name: string, partitionCount: number) =>
       client.post('/topics', { name, partition_count: partitionCount }).then((r) => r.data),

     deleteTopic: (id: string) =>
       client.delete(`/topics/${id}`).then((r) => r.data),

     // Metrics
     getDashboardStats: () =>
       client.get('/metrics/dashboard').then((r) => r.data),

     getTopicMetrics: (topicId: string, timeRange: string) =>
       client.get(`/metrics/topics/${topicId}`, { params: { range: timeRange } }).then((r) => r.data),

     // Consumer groups
     listConsumerGroups: () =>
       client.get('/consumers').then((r) => r.data),

     getConsumerLag: (groupId: string) =>
       client.get(`/consumers/${groupId}/lag`).then((r) => r.data),
   };
   ```

#### Testing

```bash
# Start development server
cd web/streamhouse-console
npm run dev

# Open browser
open http://localhost:3000

# Test flows:
# 1. Sign up → Create organization
# 2. Login → View dashboard
# 3. Create topic → View topic details
# 4. View monitoring → Check metrics
```

#### Success Criteria

- ✅ User can sign up and login
- ✅ Dashboard shows real-time stats
- ✅ Topics can be created/deleted via UI
- ✅ Metrics visualized in charts
- ✅ Consumer lag displayed
- ✅ Responsive design (mobile-friendly)
- ✅ Real-time updates via WebSocket or polling

---

### 9.4: REST API Gateway (Week 10)

#### Deliverables

1. **API Gateway** (New crate: `crates/streamhouse-api/`)
   ```rust
   // crates/streamhouse-api/src/main.rs
   use axum::{
       routing::{get, post, delete},
       Router,
       extract::{State, Path, Query},
       Json,
   };
   use tower_http::cors::{CorsLayer, Any};

   #[tokio::main]
   async fn main() -> Result<()> {
       let state = AppState {
           metadata_store: create_postgres_store().await?,
           auth_service: Arc::new(AuthService::new()),
       };

       let app = Router::new()
           // Auth routes
           .route("/api/v1/auth/signup", post(handlers::auth::signup))
           .route("/api/v1/auth/login", post(handlers::auth::login))

           // Topic routes
           .route("/api/v1/topics", get(handlers::topics::list))
           .route("/api/v1/topics", post(handlers::topics::create))
           .route("/api/v1/topics/:id", get(handlers::topics::get))
           .route("/api/v1/topics/:id", delete(handlers::topics::delete))

           // Consumer routes
           .route("/api/v1/consumers", get(handlers::consumers::list))
           .route("/api/v1/consumers/:id/lag", get(handlers::consumers::lag))

           // Metrics routes
           .route("/api/v1/metrics/dashboard", get(handlers::metrics::dashboard))
           .route("/api/v1/metrics/topics/:id", get(handlers::metrics::topic))

           // Organization routes
           .route("/api/v1/organizations", get(handlers::orgs::list))
           .route("/api/v1/organizations/:id/members", get(handlers::orgs::members))
           .route("/api/v1/organizations/:id/usage", get(handlers::orgs::usage))

           .layer(CorsLayer::new().allow_origin(Any))
           .with_state(state);

       let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
       axum::serve(listener, app).await?;

       Ok(())
   }
   ```

2. **Topic Handlers** (`crates/streamhouse-api/src/handlers/topics.rs`)
   ```rust
   use axum::{extract::{State, Path}, Json, http::StatusCode};
   use serde::{Deserialize, Serialize};

   #[derive(Deserialize)]
   pub struct CreateTopicRequest {
       name: String,
       partition_count: u32,
   }

   #[derive(Serialize)]
   pub struct TopicResponse {
       id: String,
       name: String,
       partition_count: u32,
       total_messages: u64,
       created_at: String,
   }

   pub async fn list(
       State(state): State<AppState>,
       claims: Claims, // Injected by auth middleware
   ) -> Result<Json<Vec<TopicResponse>>, ApiError> {
       let topics = state.metadata_store
           .list_topics(claims.org)
           .await?;

       let responses = topics.into_iter()
           .map(|t| TopicResponse {
               id: t.name.clone(),
               name: t.name,
               partition_count: t.partition_count,
               total_messages: t.total_messages,
               created_at: t.created_at.to_rfc3339(),
           })
           .collect();

       Ok(Json(responses))
   }

   pub async fn create(
       State(state): State<AppState>,
       claims: Claims,
       Json(req): Json<CreateTopicRequest>,
   ) -> Result<(StatusCode, Json<TopicResponse>), ApiError> {
       // Check quota
       let quota = state.metadata_store
           .get_organization_quota(claims.org)
           .await?;

       let current_topics = state.metadata_store
           .list_topics(claims.org)
           .await?
           .len() as i32;

       if current_topics >= quota.max_topics {
           return Err(ApiError::QuotaExceeded("Max topics reached"));
       }

       // Create topic
       let topic = state.metadata_store
           .create_topic(claims.org, &req.name, req.partition_count)
           .await?;

       Ok((
           StatusCode::CREATED,
           Json(TopicResponse {
               id: topic.name.clone(),
               name: topic.name,
               partition_count: topic.partition_count,
               total_messages: 0,
               created_at: topic.created_at.to_rfc3339(),
           })
       ))
   }
   ```

#### Testing

```bash
# Start API server
cargo run --bin streamhouse-api

# Test endpoints
curl http://localhost:8080/api/v1/auth/signup \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"pass123","org_name":"Test Org"}'

TOKEN=$(curl -s http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"pass123"}' \
  | jq -r '.token')

curl http://localhost:8080/api/v1/topics \
  -H "Authorization: Bearer $TOKEN"
```

#### Success Criteria

- ✅ REST API serves all web console needs
- ✅ JWT authentication on all protected routes
- ✅ CORS enabled for web console
- ✅ Quota enforcement on topic creation
- ✅ Proper error responses (400, 401, 403, 500)
- ✅ API documentation (OpenAPI/Swagger)

---

## Phase 10: Enterprise Features (Weeks 11-16)

**Goal**: Add schema registry, exactly-once semantics, and stream processing

### 10.1: Schema Registry (Weeks 11-12)

#### Deliverables

1. **Schema Storage** (New migration)
   ```sql
   CREATE TABLE schemas (
       id SERIAL PRIMARY KEY,
       organization_id UUID NOT NULL REFERENCES organizations(id),
       subject VARCHAR(255) NOT NULL, -- e.g., "orders-value"
       version INTEGER NOT NULL,
       schema_type VARCHAR(50) NOT NULL, -- avro, protobuf, json
       schema_text TEXT NOT NULL,
       compatibility VARCHAR(50) NOT NULL DEFAULT 'backward', -- backward, forward, full, none
       created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
       UNIQUE (organization_id, subject, version)
   );

   CREATE INDEX idx_schemas_subject ON schemas(organization_id, subject);
   ```

2. **Schema Registry Service** (`crates/streamhouse-schema-registry/`)
   ```rust
   // crates/streamhouse-schema-registry/src/lib.rs
   use apache_avro::Schema as AvroSchema;

   pub struct SchemaRegistry {
       metadata_store: Arc<dyn MetadataStore>,
       cache: Arc<RwLock<HashMap<(Uuid, String, i32), Arc<Schema>>>>,
   }

   impl SchemaRegistry {
       pub async fn register_schema(
           &self,
           org_id: Uuid,
           subject: &str,
           schema_text: &str,
           schema_type: SchemaType,
       ) -> Result<SchemaId> {
           // Parse schema
           let schema = match schema_type {
               SchemaType::Avro => {
                   AvroSchema::parse_str(schema_text)?;
               },
               SchemaType::Protobuf => {
                   // Parse protobuf
               },
               SchemaType::Json => {
                   // Parse JSON schema
               },
           };

           // Check compatibility with latest version
           if let Some(latest) = self.get_latest_schema(org_id, subject).await? {
               self.check_compatibility(&latest, &schema)?;
           }

           // Store new version
           let version = self.metadata_store
               .register_schema(org_id, subject, schema_text, schema_type)
               .await?;

           Ok(SchemaId { subject: subject.to_string(), version })
       }

       pub async fn get_schema(
           &self,
           org_id: Uuid,
           subject: &str,
           version: Option<i32>,
       ) -> Result<Arc<Schema>> {
           // Check cache
           let cache_key = (org_id, subject.to_string(), version.unwrap_or(-1));
           if let Some(schema) = self.cache.read().await.get(&cache_key) {
               return Ok(Arc::clone(schema));
           }

           // Fetch from database
           let schema = self.metadata_store
               .get_schema(org_id, subject, version)
               .await?;

           // Cache it
           self.cache.write().await.insert(cache_key, Arc::clone(&schema));

           Ok(schema)
       }

       fn check_compatibility(
           &self,
           old_schema: &Schema,
           new_schema: &Schema,
       ) -> Result<()> {
           // Implement Avro schema evolution rules
           // - Can add fields with defaults (backward compatible)
           // - Can remove fields (forward compatible)
           // - Can't change field types
           // - Can't rename fields without aliases

           match (old_schema, new_schema) {
               (Schema::Avro(old), Schema::Avro(new)) => {
                   avro::compatibility::can_read(old, new)?;
               },
               _ => {
                   // Protobuf/JSON compatibility checks
               }
           }

           Ok(())
       }
   }
   ```

3. **Producer Integration** (Update Producer)
   ```rust
   // crates/streamhouse-client/src/producer.rs
   impl Producer {
       pub async fn send_with_schema<T: Serialize>(
           &self,
           topic: &str,
           key: Option<&[u8]>,
           value: &T,
           schema_id: SchemaId,
       ) -> Result<SendResult> {
           // Serialize value with Avro
           let mut encoded = Vec::new();

           // Write magic byte (0) + schema ID (4 bytes)
           encoded.push(0);
           encoded.extend_from_slice(&schema_id.id.to_be_bytes());

           // Write Avro-encoded value
           let schema = self.schema_registry
               .get_schema(&schema_id)
               .await?;
           let avro_value = to_avro_value(value, &schema)?;
           avro_value.serialize(&mut Encoder::new(&mut encoded))?;

           // Send to partition
           self.send(topic, key, &encoded, None).await
       }
   }
   ```

#### Testing

```bash
# Register schema
curl -X POST http://localhost:8081/subjects/orders-value/versions \
  -H "Content-Type: application/json" \
  -d '{
    "schema": "{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"id\",\"type\":\"long\"},{\"name\":\"amount\",\"type\":\"double\"}]}"
  }'
# Response: {"id": 1}

# Produce with schema
cargo run --example produce_with_schema

# Get schema
curl http://localhost:8081/subjects/orders-value/versions/1
```

#### Success Criteria

- ✅ Schemas registered and versioned
- ✅ Compatibility checking enforced
- ✅ Producer embeds schema ID in messages
- ✅ Consumer validates messages against schema
- ✅ Schema caching for performance
- ✅ REST API for schema management

---

### 10.2: Exactly-Once Semantics (Weeks 13-14)

#### Deliverables

1. **Producer ID Registry** (New migration)
   ```sql
   CREATE TABLE producer_ids (
       producer_id VARCHAR(255) PRIMARY KEY,
       organization_id UUID NOT NULL REFERENCES organizations(id),
       last_sequence BIGINT NOT NULL DEFAULT 0,
       created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
       updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
   );

   CREATE TABLE producer_sequence_numbers (
       producer_id VARCHAR(255) NOT NULL REFERENCES producer_ids(producer_id),
       topic VARCHAR(255) NOT NULL,
       partition_id INTEGER NOT NULL,
       sequence_number BIGINT NOT NULL,
       offset BIGINT NOT NULL,
       PRIMARY KEY (producer_id, topic, partition_id, sequence_number)
   );
   ```

2. **Idempotent Producer** (Update Producer)
   ```rust
   // crates/streamhouse-client/src/producer.rs
   impl ProducerBuilder {
       pub fn enable_idempotence(mut self, enable: bool) -> Self {
           self.idempotence_enabled = enable;
           self
       }
   }

   impl Producer {
       async fn send_idempotent(
           &self,
           topic: &str,
           key: Option<&[u8]>,
           value: &[u8],
           partition: Option<u32>,
       ) -> Result<SendResult> {
           // Get or create producer ID
           if self.producer_id.read().await.is_none() {
               let pid = self.metadata_store
                   .register_producer_id(self.client_id.clone())
                   .await?;
               *self.producer_id.write().await = Some(pid);
           }

           let producer_id = self.producer_id.read().await.as_ref().unwrap().clone();

           // Get next sequence number for this partition
           let partition = partition.unwrap_or_else(|| self.partition_for(key));
           let sequence = self.next_sequence(topic, partition).await;

           // Send with producer ID + sequence
           let request = ProduceRequest {
               topic: topic.to_string(),
               partition,
               producer_id: Some(producer_id),
               sequence_number: Some(sequence),
               records: vec![Record {
                   key: key.map(|k| k.to_vec()),
                   value: value.to_vec(),
                   headers: HashMap::new(),
               }],
           };

           let response = self.agent_client.produce(request).await?;

           // On success, increment sequence
           self.increment_sequence(topic, partition).await;

           Ok(SendResult {
               offset: response.base_offset,
               partition,
           })
       }
   }
   ```

3. **Agent Deduplication** (Update Agent)
   ```rust
   // crates/streamhouse-agent/src/grpc_service.rs
   impl ProducerServiceImpl {
       async fn produce_idempotent(
           &self,
           request: ProduceRequest,
       ) -> Result<ProduceResponse> {
           let producer_id = request.producer_id.as_ref().unwrap();
           let sequence = request.sequence_number.unwrap();

           // Check if already written
           if let Some(existing_offset) = self.metadata_store
               .get_offset_for_sequence(
                   producer_id,
                   &request.topic,
                   request.partition,
                   sequence,
               )
               .await?
           {
               // Duplicate - return existing offset
               tracing::info!(
                   producer_id,
                   sequence,
                   existing_offset,
                   "Duplicate request, returning existing offset"
               );
               return Ok(ProduceResponse {
                   base_offset: existing_offset,
                   partition: request.partition,
               });
           }

           // Write records
           let base_offset = self.writer_pool
               .get_writer(&request.topic, request.partition)
               .await?
               .append(&request.records)
               .await?;

           // Store sequence → offset mapping
           self.metadata_store
               .store_producer_sequence(
                   producer_id,
                   &request.topic,
                   request.partition,
                   sequence,
                   base_offset,
               )
               .await?;

           Ok(ProduceResponse {
               base_offset,
               partition: request.partition,
           })
       }
   }
   ```

#### Testing

```bash
# Test idempotent producer
cargo run --example test_idempotence

# Simulate network failure + retry
cargo test --test idempotence_tests test_duplicate_on_retry
# Expected: Only one message written, same offset returned

# Test sequence number gaps
cargo test --test idempotence_tests test_out_of_order_sequence
# Expected: Error returned for out-of-order sequence
```

#### Success Criteria

- ✅ Producer ID assigned automatically
- ✅ Sequence numbers incremented per partition
- ✅ Duplicates detected and deduplicated
- ✅ Retries don't create duplicates
- ✅ Out-of-order sequences rejected
- ✅ Performance overhead < 5%

---

### 10.3: Stream Processing (Weeks 15-16)

#### Deliverables

1. **Stream Processor Framework** (`crates/streamhouse-streams/`)
   ```rust
   // crates/streamhouse-streams/src/lib.rs
   pub struct StreamProcessor {
       source_topic: String,
       sink_topic: String,
       consumer: Consumer,
       producer: Producer,
       operator: Box<dyn Operator>,
   }

   #[async_trait]
   pub trait Operator: Send + Sync {
       async fn process(&self, record: ConsumedRecord) -> Result<Vec<Record>>;
   }

   impl StreamProcessor {
       pub async fn run(&mut self) -> Result<()> {
           loop {
               // Poll source topic
               let records = self.consumer
                   .poll(Duration::from_secs(1))
                   .await?;

               for record in records {
                   // Process record
                   let outputs = self.operator.process(record).await?;

                   // Write to sink topic
                   for output in outputs {
                       self.producer
                           .send(&self.sink_topic, output.key.as_deref(), &output.value, None)
                           .await?;
                   }
               }

               // Commit offsets
               self.consumer.commit().await?;
           }
       }
   }
   ```

2. **Windowing Operators**
   ```rust
   pub struct TumblingWindow {
       window_size: Duration,
       aggregator: Arc<dyn Aggregator>,
       windows: HashMap<i64, WindowState>,
   }

   #[async_trait]
   impl Operator for TumblingWindow {
       async fn process(&self, record: ConsumedRecord) -> Result<Vec<Record>> {
           let timestamp = record.timestamp.unwrap_or_else(|| Utc::now());
           let window_start = self.window_for_timestamp(timestamp);

           // Get or create window
           let window = self.windows
               .entry(window_start)
               .or_insert_with(|| WindowState::new(window_start));

           // Add record to window
           window.add(record);

           // Check if window closed
           if self.is_window_closed(window_start) {
               let result = self.aggregator.aggregate(&window.records)?;
               self.windows.remove(&window_start);

               return Ok(vec![Record {
                   key: Some(window_start.to_be_bytes().to_vec()),
                   value: serde_json::to_vec(&result)?,
                   headers: HashMap::new(),
               }]);
           }

           Ok(vec![])
       }
   }
   ```

3. **Example: Order Totals**
   ```rust
   // examples/stream_processing/order_totals.rs
   use streamhouse_streams::*;

   #[tokio::main]
   async fn main() -> Result<()> {
       let processor = StreamProcessor::builder()
           .source_topic("orders")
           .sink_topic("order_totals")
           .operator(TumblingWindow::new(
               Duration::from_secs(300), // 5-minute windows
               SumAggregator::new("amount"),
           ))
           .build()
           .await?;

       processor.run().await?;

       Ok(())
   }
   ```

#### Testing

```bash
# Run stream processor
cargo run --example order_totals

# Produce orders
for i in {1..1000}; do
  cargo run --bin streamctl -- produce orders \
    --value "{\"order_id\": $i, \"amount\": $((RANDOM % 100))}"
done

# Consume aggregated results
cargo run --bin streamctl -- consume order_totals --limit 10
# Expected: 5-minute windowed totals
```

#### Success Criteria

- ✅ Tumbling windows working
- ✅ Sliding windows working
- ✅ Aggregations (sum, count, avg, min, max)
- ✅ State persistence (RocksDB-backed)
- ✅ Exactly-once processing
- ✅ Late-arriving data handling

---

## Phase 11: Billing & Business (Weeks 17-18)

**Goal**: Implement usage tracking, billing, and monetization

### Deliverables

1. **Usage Tracking** (Background service)
   ```rust
   // crates/streamhouse-billing/src/usage_tracker.rs
   pub struct UsageTracker {
       metadata_store: Arc<dyn MetadataStore>,
   }

   impl UsageTracker {
       pub async fn track_hourly_usage(&self) -> Result<()> {
           let orgs = self.metadata_store.list_organizations().await?;

           for org in orgs {
               // Count messages produced
               let messages_produced = self.metadata_store
                   .count_messages_produced_last_hour(org.id)
                   .await?;

               // Calculate storage GB-hours
               let storage_bytes = self.metadata_store
                   .calculate_storage_bytes(org.id)
                   .await?;
               let storage_gb_hours = storage_bytes as f64 / 1_073_741_824.0; // bytes to GB

               // Store usage
               self.metadata_store
                   .record_usage(org.id, messages_produced, storage_gb_hours)
                   .await?;
           }

           Ok(())
       }
   }
   ```

2. **Stripe Integration** (`crates/streamhouse-billing/src/stripe_client.rs`)
   ```rust
   use stripe::{Client, Customer, Subscription, Invoice};

   pub struct StripeClient {
       client: Client,
   }

   impl StripeClient {
       pub async fn create_customer(
           &self,
           org_id: Uuid,
           email: &str,
           org_name: &str,
       ) -> Result<String> {
           let customer = Customer::create(
               &self.client,
               CreateCustomer {
                   email: Some(email),
                   name: Some(org_name),
                   metadata: Some([("org_id".to_string(), org_id.to_string())].into()),
                   ..Default::default()
               },
           ).await?;

           Ok(customer.id.to_string())
       }

       pub async fn create_usage_record(
           &self,
           subscription_item_id: &str,
           quantity: i64,
           timestamp: i64,
       ) -> Result<()> {
           SubscriptionItem::create_usage_record(
               &self.client,
               subscription_item_id,
               CreateUsageRecord {
                   quantity,
                   timestamp: Some(timestamp),
                   action: Some(UsageRecordAction::Set),
               },
           ).await?;

           Ok(())
       }
   }
   ```

3. **Pricing Configuration** (`config/pricing.yaml`)
   ```yaml
   plans:
     free:
       max_topics: 3
       max_partitions_per_topic: 2
       max_messages_per_month: 1000000     # 1M
       max_storage_gb: 1
       price_usd: 0

     starter:
       max_topics: 10
       max_partitions_per_topic: 4
       max_messages_per_month: 10000000    # 10M
       max_storage_gb: 10
       base_price_usd: 50
       overage:
         messages_per_million: 5           # $5 per 1M messages
         storage_per_gb: 0.10              # $0.10 per GB

     pro:
       max_topics: 100
       max_partitions_per_topic: 12
       max_messages_per_month: 100000000   # 100M
       max_storage_gb: 100
       base_price_usd: 200
       overage:
         messages_per_million: 4
         storage_per_gb: 0.08

     enterprise:
       unlimited: true
       custom_pricing: true
   ```

4. **Billing Dashboard** (Add to web console)
   ```tsx
   // src/app/(dashboard)/billing/page.tsx
   export default function BillingPage() {
     const { data: usage } = useQuery({
       queryKey: ['billing-usage'],
       queryFn: () => api.getBillingUsage(),
     });

     return (
       <div className="space-y-6">
         <h1 className="text-2xl font-bold">Billing & Usage</h1>

         {/* Current Plan */}
         <div className="bg-white p-6 rounded-lg shadow">
           <h2 className="text-lg font-semibold mb-4">Current Plan: {usage?.plan}</h2>
           <p className="text-3xl font-bold">${usage?.currentMonthCost}</p>
           <p className="text-sm text-gray-500">Estimated cost for this month</p>
         </div>

         {/* Usage Meters */}
         <div className="grid grid-cols-2 gap-4">
           <UsageMeter
             title="Messages"
             current={usage?.messagesThisMonth}
             limit={usage?.messageLimit}
             unit="messages"
           />
           <UsageMeter
             title="Storage"
             current={usage?.storageGB}
             limit={usage?.storageLimit}
             unit="GB"
           />
         </div>

         {/* Upgrade CTA */}
         {usage?.plan === 'free' && (
           <div className="bg-indigo-50 p-6 rounded-lg">
             <h3 className="font-semibold mb-2">Upgrade to Starter</h3>
             <p className="text-sm mb-4">Get 10M messages/month and priority support</p>
             <button className="btn btn-primary">Upgrade Now</button>
           </div>
         )}
       </div>
     );
   }
   ```

#### Testing

```bash
# Track usage
cargo run --bin streamhouse-billing-tracker

# Sync to Stripe
cargo run --bin streamhouse-billing-sync

# View invoices
curl http://localhost:8080/api/v1/billing/invoices \
  -H "Authorization: Bearer $TOKEN"
```

#### Success Criteria

- ✅ Usage tracked hourly
- ✅ Stripe customers created on signup
- ✅ Subscriptions managed via Stripe
- ✅ Usage-based billing working
- ✅ Invoices generated monthly
- ✅ Quota enforcement
- ✅ Upgrade/downgrade flows

---

## Phase 12: Production Operations (Weeks 19-22)

**Goal**: Auto-scaling, backups, disaster recovery, compliance

### 12.1: Auto-Scaling (Week 19)

#### Deliverables

1. **HorizontalPodAutoscaler** (Update Helm chart)
   ```yaml
   # deploy/kubernetes/streamhouse/templates/hpa.yaml
   apiVersion: autoscaling/v2
   kind: HorizontalPodAutoscaler
   metadata:
     name: streamhouse-agent
   spec:
     scaleTargetRef:
       apiVersion: apps/v1
       kind: StatefulSet
       name: streamhouse-agent
     minReplicas: 3
     maxReplicas: 20
     metrics:
     - type: Resource
       resource:
         name: cpu
         target:
           type: Utilization
           averageUtilization: 70
     - type: Resource
       resource:
         name: memory
         target:
           type: Utilization
           averageUtilization: 80
     - type: Pods
       pods:
         metric:
           name: streamhouse_active_partitions
         target:
           type: AverageValue
           averageValue: "50"
     behavior:
       scaleDown:
         stabilizationWindowSeconds: 300
         policies:
         - type: Percent
           value: 50
           periodSeconds: 60
       scaleUp:
         stabilizationWindowSeconds: 60
         policies:
         - type: Percent
           value: 100
           periodSeconds: 30
   ```

2. **Custom Metrics** (Prometheus Adapter)
   ```yaml
   # deploy/kubernetes/monitoring/prometheus-adapter-config.yaml
   rules:
   - seriesQuery: 'streamhouse_active_partitions'
     resources:
       overrides:
         pod: {resource: "pod"}
     name:
       as: "streamhouse_active_partitions"
     metricsQuery: 'avg_over_time(streamhouse_active_partitions[5m])'
   ```

#### Testing

```bash
# Apply HPA
kubectl apply -f deploy/kubernetes/streamhouse/templates/hpa.yaml

# Check status
kubectl get hpa streamhouse-agent

# Simulate load
for i in {1..10000}; do
  cargo run --bin streamctl -- produce orders --value "{\"id\": $i}"
done

# Watch scaling
kubectl get pods -l app=streamhouse-agent -w
# Expected: Pods scale from 3 to 10+
```

#### Success Criteria

- ✅ CPU-based auto-scaling working
- ✅ Custom metrics (active partitions) working
- ✅ Scale-up responsive (< 2 minutes)
- ✅ Scale-down gradual (5-minute stabilization)
- ✅ No disruption during scaling

---

### 12.2: Automated Backups (Week 20)

#### Deliverables

1. **RDS Automated Backups** (Already configured in Terraform)
   - Daily snapshots (7-day retention)
   - Point-in-time recovery (PITR)
   - Multi-AZ replication

2. **S3 Versioning & Replication** (Terraform)
   ```hcl
   resource "aws_s3_bucket_versioning" "streamhouse_data" {
     bucket = aws_s3_bucket.streamhouse_data.id

     versioning_configuration {
       status = "Enabled"
     }
   }

   resource "aws_s3_bucket_replication_configuration" "streamhouse_data" {
     bucket = aws_s3_bucket.streamhouse_data.id
     role   = aws_iam_role.replication.arn

     rule {
       id     = "replicate-all"
       status = "Enabled"

       destination {
         bucket        = aws_s3_bucket.streamhouse_data_backup.arn
         storage_class = "GLACIER"
       }
     }
   }
   ```

3. **Metadata Backup Script** (`scripts/backup-metadata.sh`)
   ```bash
   #!/bin/bash

   # Backup PostgreSQL to S3
   TIMESTAMP=$(date +%Y%m%d_%H%M%S)
   BACKUP_FILE="streamhouse_metadata_${TIMESTAMP}.sql.gz"

   pg_dump -h $RDS_ENDPOINT -U streamhouse streamhouse_metadata \
     | gzip > /tmp/$BACKUP_FILE

   aws s3 cp /tmp/$BACKUP_FILE s3://streamhouse-backups/metadata/$BACKUP_FILE

   rm /tmp/$BACKUP_FILE

   echo "Backup complete: s3://streamhouse-backups/metadata/$BACKUP_FILE"
   ```

4. **CronJob for Backups** (Kubernetes)
   ```yaml
   apiVersion: batch/v1
   kind: CronJob
   metadata:
     name: metadata-backup
   spec:
     schedule: "0 2 * * *"  # Daily at 2 AM
     jobTemplate:
       spec:
         template:
           spec:
             containers:
             - name: backup
               image: postgres:15
               command:
               - /bin/bash
               - -c
               - |
                 pg_dump -h $RDS_ENDPOINT -U streamhouse streamhouse_metadata \
                   | gzip | aws s3 cp - s3://streamhouse-backups/metadata/backup_$(date +%Y%m%d).sql.gz
             restartPolicy: OnFailure
   ```

#### Testing

```bash
# Trigger manual backup
kubectl create job --from=cronjob/metadata-backup manual-backup-$(date +%s)

# Verify backup in S3
aws s3 ls s3://streamhouse-backups/metadata/

# Test restore
aws s3 cp s3://streamhouse-backups/metadata/backup_20260127.sql.gz - \
  | gunzip | psql -h $RDS_ENDPOINT -U streamhouse streamhouse_metadata
```

#### Success Criteria

- ✅ Daily RDS snapshots
- ✅ S3 versioning enabled
- ✅ Cross-region replication
- ✅ Metadata backups to S3
- ✅ Restore procedure documented
- ✅ Backup monitoring/alerts

---

### 12.3: Disaster Recovery (Week 21)

#### Deliverables

1. **DR Runbook** (`docs/disaster-recovery.md`)
   ```markdown
   # Disaster Recovery Runbook

   ## RTO/RPO Targets
   - RTO (Recovery Time Objective): 1 hour
   - RPO (Recovery Point Objective): 5 minutes

   ## Scenarios

   ### 1. Complete Region Failure

   **Detection**: All health checks failing in primary region

   **Steps**:
   1. Verify secondary region is healthy
   2. Update DNS to point to secondary region
   3. Promote RDS read replica to primary
   4. Restart agents in secondary region
   5. Verify data integrity

   **Commands**:
   ```bash
   # Promote RDS replica
   aws rds promote-read-replica \
     --db-instance-identifier streamhouse-prod-metadata-replica

   # Update DNS
   aws route53 change-resource-record-sets \
     --hosted-zone-id Z123 \
     --change-batch file://dns-update.json

   # Restart agents
   kubectl rollout restart statefulset/streamhouse-agent -n production
   ```

   ### 2. Data Corruption

   **Detection**: Integrity checks failing, data inconsistencies

   **Steps**:
   1. Stop all writes (set agents to read-only)
   2. Identify corruption timestamp
   3. Restore from backup before corruption
   4. Replay S3 segments from corruption point
   5. Verify data integrity
   6. Resume writes

   **Commands**:
   ```bash
   # Restore RDS from snapshot
   aws rds restore-db-instance-from-db-snapshot \
     --db-instance-identifier streamhouse-restored \
     --db-snapshot-identifier streamhouse-20260127-02-00

   # Replay segments
   cargo run --bin streamhouse-replay -- \
     --from-timestamp 2026-01-27T02:00:00Z \
     --to-timestamp 2026-01-27T03:00:00Z
   ```
   ```

2. **Multi-Region Setup** (Optional - Terraform)
   ```hcl
   # deploy/terraform/multi-region.tf

   # Primary region: us-east-1
   module "primary_region" {
     source = "./modules/streamhouse"
     region = "us-east-1"
     role   = "primary"
   }

   # Secondary region: us-west-2
   module "secondary_region" {
     source = "./modules/streamhouse"
     region = "us-west-2"
     role   = "secondary"
   }

   # RDS cross-region read replica
   resource "aws_db_instance" "replica" {
     provider             = aws.us-west-2
     identifier           = "streamhouse-prod-metadata-replica"
     replicate_source_db  = module.primary_region.rds_arn
     instance_class       = "db.r6g.xlarge"
     publicly_accessible  = false
   }

   # S3 cross-region replication (already configured)
   ```

3. **Chaos Engineering** (Test DR procedures)
   ```bash
   # Use Chaos Mesh to test failures
   kubectl apply -f chaos/pod-failure.yaml
   kubectl apply -f chaos/network-partition.yaml
   kubectl apply -f chaos/disk-failure.yaml

   # Verify system recovers
   ./scripts/check-system-health.sh
   ```

#### Testing

```bash
# Test region failover
./scripts/dr-test-region-failover.sh

# Test data restore
./scripts/dr-test-data-restore.sh

# Test chaos scenarios
./scripts/dr-test-chaos.sh
```

#### Success Criteria

- ✅ DR runbook complete and tested
- ✅ Multi-region setup (optional)
- ✅ RTO < 1 hour verified
- ✅ RPO < 5 minutes verified
- ✅ Chaos tests passing
- ✅ Team trained on DR procedures

---

### 12.4: Compliance & Security (Week 22)

#### Deliverables

1. **Encryption** (Already configured)
   - ✅ S3 encryption at rest (AES-256)
   - ✅ RDS encryption at rest
   - ✅ TLS 1.3 for gRPC
   - ✅ TLS 1.3 for HTTPS (web console)

2. **Audit Logging** (New feature)
   ```rust
   // crates/streamhouse-audit/src/lib.rs
   pub struct AuditLog {
       metadata_store: Arc<dyn MetadataStore>,
   }

   impl AuditLog {
       pub async fn log_event(&self, event: AuditEvent) -> Result<()> {
           self.metadata_store
               .insert_audit_log(
                   event.organization_id,
                   event.user_id,
                   &event.action,
                   &event.resource_type,
                   event.resource_id.as_deref(),
                   &event.metadata,
               )
               .await
       }
   }

   // Example events:
   // - topic.created
   // - topic.deleted
   // - user.login
   // - user.logout
   // - api_key.created
   // - api_key.revoked
   ```

3. **GDPR Compliance** (Data deletion)
   ```rust
   // crates/streamhouse-api/src/handlers/gdpr.rs
   pub async fn delete_user_data(
       State(state): State<AppState>,
       claims: Claims,
       Path(user_id): Path<Uuid>,
   ) -> Result<StatusCode, ApiError> {
       // Verify authorization
       if claims.sub != user_id && claims.role != "owner" {
           return Err(ApiError::Forbidden);
       }

       // Delete user data
       state.metadata_store
           .delete_user(user_id)
           .await?;

       // Delete S3 segments for user's topics
       state.s3_client
           .delete_user_segments(user_id)
           .await?;

       // Log deletion
       state.audit_log
           .log_event(AuditEvent {
               organization_id: claims.org,
               user_id: Some(user_id),
               action: "user.data_deleted".to_string(),
               resource_type: "user".to_string(),
               resource_id: Some(user_id.to_string()),
               metadata: serde_json::json!({"reason": "GDPR request"}),
           })
           .await?;

       Ok(StatusCode::NO_CONTENT)
   }
   ```

4. **SOC 2 Readiness** (`docs/compliance/soc2-checklist.md`)
   ```markdown
   # SOC 2 Type II Readiness Checklist

   ## Security
   - [x] Encryption at rest (S3, RDS)
   - [x] Encryption in transit (TLS 1.3)
   - [x] Multi-factor authentication
   - [x] Role-based access control
   - [x] Audit logging
   - [x] Security incident response plan

   ## Availability
   - [x] Multi-AZ deployment
   - [x] Automated backups (7-day retention)
   - [x] Disaster recovery plan
   - [x] 99.9% uptime SLA
   - [x] Monitoring and alerting

   ## Confidentiality
   - [x] Data isolation (multi-tenancy)
   - [x] VPC network isolation
   - [x] Secrets management (AWS Secrets Manager)
   - [x] API key rotation

   ## Processing Integrity
   - [x] Data validation (schema registry)
   - [x] Integrity checks (checksums)
   - [x] Exactly-once semantics
   - [x] Idempotent operations

   ## Privacy
   - [x] GDPR compliance (data deletion)
   - [x] Privacy policy
   - [x] Data retention policies
   - [x] User consent management
   ```

#### Testing

```bash
# Test audit logging
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"name":"test","partition_count":4}'

# Verify audit log
psql -h $RDS_ENDPOINT -U streamhouse -d streamhouse_metadata \
  -c "SELECT * FROM audit_logs WHERE action = 'topic.created' ORDER BY created_at DESC LIMIT 10;"

# Test GDPR deletion
curl -X DELETE http://localhost:8080/api/v1/users/$USER_ID/data \
  -H "Authorization: Bearer $TOKEN"
```

#### Success Criteria

- ✅ All data encrypted
- ✅ Audit logs for all actions
- ✅ GDPR data deletion working
- ✅ SOC 2 controls implemented
- ✅ Security incident response plan
- ✅ Compliance documentation complete

---

## Phase 13: Launch Preparation (Weeks 23-24)

**Goal**: Documentation, testing, launch checklist

### Deliverables

1. **Public Documentation** (Docusaurus site)
   ```
   docs-site/
   ├── docs/
   │   ├── getting-started/
   │   │   ├── quickstart.md
   │   │   ├── concepts.md
   │   │   └── first-topic.md
   │   ├── guides/
   │   │   ├── producer.md
   │   │   ├── consumer.md
   │   │   ├── schema-registry.md
   │   │   └── stream-processing.md
   │   ├── api-reference/
   │   │   ├── rest-api.md
   │   │   ├── grpc-api.md
   │   │   └── cli.md
   │   └── operations/
   │       ├── monitoring.md
   │       ├── scaling.md
   │       └── troubleshooting.md
   ```

2. **Load Testing** (k6 scripts)
   ```javascript
   // scripts/load-test/produce-load.js
   import grpc from 'k6/net/grpc';
   import { check } from 'k6';

   const client = new grpc.Client();
   client.load(['../proto'], 'streamhouse.proto');

   export const options = {
     stages: [
       { duration: '2m', target: 100 },   // Ramp up to 100 producers
       { duration: '5m', target: 100 },   // Stay at 100
       { duration: '2m', target: 1000 },  // Ramp up to 1000
       { duration: '10m', target: 1000 }, // Stay at 1000
       { duration: '2m', target: 0 },     // Ramp down
     ],
   };

   export default function () {
     client.connect('streamhouse-lb:50051', { plaintext: true });

     const response = client.invoke('streamhouse.ProducerService/Produce', {
       topic: 'orders',
       partition: Math.floor(Math.random() * 12),
       records: [{
         value: Buffer.from(JSON.stringify({
           order_id: Math.floor(Math.random() * 1000000),
           amount: Math.random() * 1000,
         })),
       }],
     });

     check(response, {
       'status is OK': (r) => r && r.status === grpc.StatusOK,
     });

     client.close();
   }
   ```

3. **Launch Checklist** (`docs/LAUNCH_CHECKLIST.md`)
   ```markdown
   # Production Launch Checklist

   ## Infrastructure (Week before launch)
   - [ ] EKS cluster running in production VPC
   - [ ] RDS PostgreSQL with Multi-AZ
   - [ ] S3 bucket with lifecycle policies
   - [ ] CloudWatch logs and alarms
   - [ ] Prometheus and Grafana
   - [ ] DNS configured (streamhouse.com)
   - [ ] SSL certificates issued

   ## Application (3 days before launch)
   - [ ] Latest code deployed to production
   - [ ] Database migrations applied
   - [ ] Feature flags configured
   - [ ] Secrets rotated
   - [ ] API documentation published

   ## Testing (2 days before launch)
   - [ ] Load test passed (10K msg/sec sustained)
   - [ ] Chaos engineering tests passed
   - [ ] DR procedures tested
   - [ ] Security scan passed
   - [ ] Performance benchmarks met

   ## Monitoring (1 day before launch)
   - [ ] All alerts configured
   - [ ] PagerDuty rotation set
   - [ ] Runbooks complete
   - [ ] Health checks passing
   - [ ] Metrics dashboards ready

   ## Business (Launch day)
   - [ ] Pricing plans finalized
   - [ ] Stripe webhooks configured
   - [ ] Legal terms published
   - [ ] Privacy policy published
   - [ ] Support email configured
   - [ ] Marketing site live

   ## Go Live! 🚀
   - [ ] Open signups
   - [ ] Send launch announcement
   - [ ] Monitor for 24 hours
   - [ ] Post-launch retrospective
   ```

4. **Beta Testing Program**
   - Invite 10 early users
   - Free Pro plan for beta testers
   - Weekly feedback sessions
   - Bug bounty program

#### Testing

```bash
# Run load tests
k6 run scripts/load-test/produce-load.js

# Expected results:
# - 10,000 msg/sec sustained
# - p99 latency < 50ms
# - 0 errors

# Smoke tests
./scripts/smoke-test-production.sh
```

#### Success Criteria

- ✅ Documentation complete and published
- ✅ Load tests passed (10K msg/sec)
- ✅ Beta testers onboarded
- ✅ Launch checklist 100% complete
- ✅ Team trained and ready
- ✅ Monitoring verified

---

## Success Metrics (Month 6)

### Technical Metrics

| Metric | Target | How to Measure |
|--------|--------|----------------|
| **Uptime** | 99.9% | CloudWatch uptime monitoring |
| **Throughput** | 10,000 msg/sec | Prometheus `streamhouse_records_sent_total` |
| **Latency (p99)** | < 50ms | Prometheus `streamhouse_write_latency_seconds` |
| **Failover Time** | < 60 seconds | Chaos engineering tests |
| **Data Loss** | 0 events | `streamhouse_data_loss_events_total` |
| **Agent Auto-scaling** | 2-minute scale-up | Kubernetes HPA metrics |

### Business Metrics

| Metric | Target (Month 1) | Target (Month 6) |
|--------|------------------|------------------|
| **Signups** | 50 | 500 |
| **Paying Customers** | 5 | 50 |
| **MRR** | $250 | $5,000 |
| **Churn Rate** | < 10% | < 5% |
| **NPS Score** | > 40 | > 60 |

### User Experience Metrics

| Metric | Target |
|--------|--------|
| **Time to First Topic** | < 5 minutes |
| **Time to First Message** | < 10 minutes |
| **Dashboard Load Time** | < 2 seconds |
| **API Documentation Score** | > 90/100 |

---

## Resource Requirements

### Team

| Role | FTE | Duration |
|------|-----|----------|
| **Backend Engineers** | 3 | 6 months |
| **Frontend Engineer** | 1 | 3 months (Weeks 7-16) |
| **DevOps/SRE** | 1 | 6 months |
| **Product Manager** | 0.5 | 6 months |
| **Designer** | 0.25 | 2 months (Weeks 7-14) |
| **QA Engineer** | 0.5 | 3 months (Weeks 16-24) |

**Total**: ~6 FTE-months

### Infrastructure Costs (Monthly)

| Service | Specification | Cost |
|---------|---------------|------|
| **EKS** | 1 cluster + 5-10 nodes (t3.xlarge) | $500-1,000 |
| **RDS PostgreSQL** | db.r6g.xlarge Multi-AZ | $600 |
| **S3** | 1 TB storage + requests | $30 |
| **CloudWatch** | Logs + metrics | $100 |
| **Load Balancers** | 2 NLBs | $40 |
| **Data Transfer** | 500 GB/month | $50 |
| **Stripe** | 2.9% + $0.30 per transaction | Variable |
| **Total** | | **~$1,500-2,000/month** |

**Year 1 Infrastructure**: ~$20,000

### Software Costs (Annual)

| Service | Cost |
|---------|------|
| **Stripe** | Included (pay-as-you-go) |
| **PagerDuty** | $300 |
| **StatusPage** | $300 |
| **Sentry** | $300 |
| **Total** | **~$900/year** |

---

## Risk Mitigation

### Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Performance bottlenecks** | Medium | High | Load testing at each phase |
| **Data loss bugs** | Low | Critical | Extensive testing, chaos engineering |
| **Security vulnerabilities** | Medium | Critical | Security audits, penetration testing |
| **Scaling issues** | Medium | High | Auto-scaling, load testing |
| **Multi-tenancy bugs** | Low | High | Integration tests, beta testing |

### Business Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Low user adoption** | Medium | Critical | Beta program, marketing, docs |
| **High churn** | Medium | High | Customer success, support |
| **Pricing too high** | Medium | Medium | Market research, competitor analysis |
| **Competitors** | High | Medium | Feature differentiation, speed to market |

---

## Timeline Summary

```
Month 1 (Weeks 1-4):   Phase 8 - Production Infrastructure
Month 2 (Weeks 5-8):   Phase 9.1-9.2 - Multi-Tenancy + Auth
Month 3 (Weeks 9-12):  Phase 9.3-9.4 - Web Console + API Gateway
Month 4 (Weeks 13-16): Phase 10 - Enterprise Features
Month 5 (Weeks 17-22): Phase 11-12 - Billing + Operations
Month 6 (Weeks 23-24): Phase 13 - Launch Prep
```

---

## Next Steps

### Week 1 (Start Immediately)

1. **Infrastructure Setup**
   ```bash
   # Set up AWS account
   aws configure

   # Create Terraform workspace
   cd deploy/terraform
   terraform init
   terraform plan

   # Apply infrastructure
   terraform apply
   ```

2. **Kubernetes Deployment**
   ```bash
   # Create EKS cluster
   eksctl create cluster -f deploy/eks/cluster.yaml

   # Deploy Helm chart
   helm install streamhouse ./deploy/kubernetes/streamhouse
   ```

3. **Verify Production**
   ```bash
   # Run health checks
   ./scripts/check-system-health.sh

   # Test end-to-end
   ./scripts/demo-complete.sh
   ```

### Communication

- **Weekly Standups**: Monday, Wednesday, Friday
- **Sprint Planning**: Every 2 weeks
- **Demo Days**: End of each phase
- **Retrospectives**: End of each month

---

**Document Owner**: Engineering Team
**Last Updated**: 2026-01-27
**Status**: Ready for Execution 🚀
