# StreamHouse Managed Service - Deployment Strategy

**Created**: 2026-01-23
**Status**: Planning (for post-Phase 6)
**Goal**: Production-ready managed service deployment

---

## Executive Summary

StreamHouse managed service will be deployed on **Kubernetes (EKS/GKE/AKS)** using a modern cloud-native stack. The architecture leverages managed services where possible to minimize operational overhead.

### Key Design Principles

1. **Cloud-Native First** - Kubernetes for orchestration, managed services for data stores
2. **Multi-Tenant by Default** - Agent groups + namespaces for customer isolation
3. **Zero-Trust Security** - mTLS, network policies, pod security standards
4. **Observable** - Metrics, logs, traces from day one
5. **Cost-Optimized** - Spot instances for agents, reserved for metadata store

---

## Architecture Overview

```
┌────────────────────────────────────────────────────────────────┐
│                   StreamHouse Managed Service                  │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────  CONTROL PLANE  ──────────────────────┐   │
│  │                                                          │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │   │
│  │  │   Web UI     │  │   API GW     │  │    Admin     │  │   │
│  │  │  (Next.js)   │  │   (Kong)     │  │   Service    │  │   │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  │   │
│  │                                                          │   │
│  │  Features:                                               │   │
│  │  • Customer dashboard                                    │   │
│  │  • Topic management                                      │   │
│  │  • Usage analytics                                       │   │
│  │  • Billing integration (Stripe)                         │   │
│  │                                                          │   │
│  └──────────────────────────┬───────────────────────────────┘   │
│                             │                                   │
│  ┌─────────────────  DATA PLANE  ──────────────────────────┐   │
│  │                                                          │   │
│  │  Kubernetes Cluster (EKS/GKE/AKS)                       │   │
│  │                                                          │   │
│  │  ┌─────────────────────────────────────────────────┐    │   │
│  │  │  Agent Pods (StatefulSet)                       │    │   │
│  │  │                                                  │    │   │
│  │  │  ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐  │    │   │
│  │  │  │Agt-1│  │Agt-2│  │Agt-3│  │Agt-N│  │SQL E│  │    │   │
│  │  │  │us-1a│  │us-1b│  │us-1c│  │  …  │  │Eng. │  │    │   │
│  │  │  └─────┘  └─────┘  └─────┘  └─────┘  └─────┘  │    │   │
│  │  │                                                  │    │   │
│  │  │  Auto-scaling: HPA (CPU/network) + Karpenter    │    │   │
│  │  │  Instance types: c6i.xlarge (spot 80%)          │    │   │
│  │  └─────────────────────────────────────────────────┘    │   │
│  │                                                          │   │
│  │  Service Mesh: Istio/Linkerd                            │   │
│  │  • mTLS between pods                                    │   │
│  │  • Circuit breaking                                     │   │
│  │  • Rate limiting                                        │   │
│  │                                                          │   │
│  └──────────────────────────┬───────────────────────────────┘   │
│                             │                                   │
│  ┌─────────────────  METADATA LAYER  ──────────────────────┐   │
│  │                                                          │   │
│  │  ┌──────────────────────────────────────────────────┐   │   │
│  │  │  Amazon RDS for PostgreSQL (Multi-AZ)           │   │   │
│  │  │  • db.r6g.xlarge (reserved)                     │   │   │
│  │  │  • Read replicas (2x) for scaling               │   │   │
│  │  │  • Automatic backups (30 days)                  │   │   │
│  │  │  • Point-in-time recovery                       │   │   │
│  │  └──────────────────────────────────────────────────┘   │   │
│  │                                                          │   │
│  │  Alternative: Amazon Aurora Serverless v2               │   │
│  │  • Auto-scaling capacity                                │   │
│  │  • Better cost optimization                             │   │
│  │                                                          │   │
│  └──────────────────────────┬───────────────────────────────┘   │
│                             │                                   │
│  ┌─────────────────  STORAGE LAYER  ───────────────────────┐   │
│  │                                                          │   │
│  │  ┌──────────────────────────────────────────────────┐   │   │
│  │  │  Amazon S3                                       │   │   │
│  │  │  • Standard tier for recent data (30 days)      │   │   │
│  │  │  • Intelligent-Tiering for older data           │   │   │
│  │  │  • Glacier for archival (optional)              │   │   │
│  │  │  • Versioning enabled                            │   │   │
│  │  │  • Lifecycle policies                            │   │   │
│  │  └──────────────────────────────────────────────────┘   │   │
│  │                                                          │   │
│  │  Alternative: S3 Express One Zone                       │   │
│  │  • 4x lower latency                                     │   │
│  │  • Higher throughput                                    │   │
│  │  • Cost: $0.16/GB vs $0.023/GB (standard)              │   │
│  │                                                          │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────  OBSERVABILITY  ───────────────────────┐   │
│  │                                                          │   │
│  │  Metrics:    Prometheus + Grafana Cloud                 │   │
│  │  Logs:       Loki + Grafana Cloud                       │   │
│  │  Traces:     Tempo + Grafana Cloud                      │   │
│  │  Alerting:   PagerDuty + Opsgenie                       │   │
│  │                                                          │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
└────────────────────────────────────────────────────────────────┘
```

---

## Deployment Model

### Option 1: Single Region (MVP) ⭐ RECOMMENDED

**Target**: First 100 customers, < 1TB/day

```yaml
Region: us-east-1
Availability Zones: us-east-1a, us-east-1b, us-east-1c

EKS Cluster:
  Node Groups:
    - agents: 6-20 nodes (c6i.xlarge, 80% spot)
    - system: 3 nodes (t3.medium, on-demand)

RDS PostgreSQL:
  - Primary: us-east-1a (db.r6g.xlarge)
  - Replica: us-east-1b (db.r6g.xlarge)
  - Automatic failover < 60s

S3:
  - Bucket: streamhouse-prod-us-east-1
  - Replication: Enabled (cross-region backup)

Cost Estimate:
  - EKS Control Plane: $72/month
  - Agent Nodes: $600/month (10 nodes avg, spot)
  - RDS: $400/month (reserved)
  - S3: $23/GB/month (1TB = $23)
  - NAT Gateway: $45/month
  - Data Transfer: $90/month (1TB out)

  Total: ~$1,230/month + $113/TB stored
```

### Option 2: Multi-Region (Scale)

**Target**: 1000+ customers, > 10TB/day

```yaml
Regions: us-east-1, eu-west-1, ap-southeast-1

Global Load Balancer:
  - AWS Global Accelerator
  - Route 53 latency-based routing
  - Health checks per region

Per-Region Stack:
  - Same as Single Region above
  - Regional S3 buckets
  - Regional RDS (no cross-region replication)

Cross-Region:
  - Control plane shared (single region)
  - Data plane isolated (per region)
  - Customer routing: Based on location

Cost: ~$3,600/month (3 regions) + storage
```

### Option 3: Multi-Tenant SaaS (Future)

**Target**: 10,000+ customers

- Virtual clusters per customer (namespaces)
- Resource quotas and limits
- Network policies for isolation
- Separate S3 prefixes per customer
- Shared metadata store with row-level security

---

## Infrastructure as Code

### Stack

```yaml
IaC Tool: Terraform + Terragrunt
GitOps: ArgoCD for K8s resources
Secrets: AWS Secrets Manager + External Secrets Operator
```

### Repository Structure

```
streamhouse-infra/
├── terraform/
│   ├── modules/
│   │   ├── eks/
│   │   ├── rds/
│   │   ├── s3/
│   │   ├── vpc/
│   │   └── monitoring/
│   ├── environments/
│   │   ├── dev/
│   │   ├── staging/
│   │   └── prod/
│   └── global/
│       └── route53/
├── kubernetes/
│   ├── base/
│   │   ├── streamhouse-agent/
│   │   ├── control-plane/
│   │   └── monitoring/
│   ├── overlays/
│   │   ├── dev/
│   │   ├── staging/
│   │   └── prod/
│   └── argocd/
│       └── applications/
└── scripts/
    ├── deploy.sh
    ├── rollback.sh
    └── disaster-recovery.sh
```

---

## Kubernetes Configuration

### Agent Deployment

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: streamhouse-agent
  namespace: streamhouse
spec:
  serviceName: streamhouse-agent
  replicas: 6  # Auto-scaled by HPA
  selector:
    matchLabels:
      app: streamhouse-agent
  template:
    metadata:
      labels:
        app: streamhouse-agent
        version: v1.0.0
    spec:
      serviceAccountName: streamhouse-agent

      # Multi-AZ spreading
      topologySpreadConstraints:
      - maxSkew: 1
        topologyKey: topology.kubernetes.io/zone
        whenUnsatisfiable: DoNotSchedule
        labelSelector:
          matchLabels:
            app: streamhouse-agent

      # Prefer spot instances
      nodeSelector:
        node.kubernetes.io/instance-type: c6i.xlarge
      tolerations:
      - key: "spot"
        operator: "Equal"
        value: "true"
        effect: "NoSchedule"

      containers:
      - name: agent
        image: streamhouse/agent:v1.0.0
        ports:
        - containerPort: 9090
          name: grpc
          protocol: TCP
        - containerPort: 9091
          name: metrics
          protocol: TCP

        env:
        - name: AGENT_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: AVAILABILITY_ZONE
          valueFrom:
            fieldRef:
              fieldPath: metadata.labels['topology.kubernetes.io/zone']
        - name: AGENT_GROUP
          value: "prod"
        - name: POSTGRES_URL
          valueFrom:
            secretKeyRef:
              name: streamhouse-secrets
              key: postgres-url
        - name: S3_BUCKET
          value: "streamhouse-prod-us-east-1"

        resources:
          requests:
            cpu: 2000m
            memory: 4Gi
          limits:
            cpu: 4000m
            memory: 8Gi

        livenessProbe:
          grpc:
            port: 9090
          initialDelaySeconds: 30
          periodSeconds: 10

        readinessProbe:
          grpc:
            port: 9090
          initialDelaySeconds: 5
          periodSeconds: 5

        lifecycle:
          preStop:
            exec:
              command: ["/bin/sh", "-c", "sleep 30"]  # Graceful shutdown
---
apiVersion: v1
kind: Service
metadata:
  name: streamhouse-agent
  namespace: streamhouse
spec:
  type: LoadBalancer
  selector:
    app: streamhouse-agent
  ports:
  - port: 9090
    targetPort: 9090
    name: grpc
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-type: nlb
    service.beta.kubernetes.io/aws-load-balancer-cross-zone-load-balancing-enabled: "true"
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: streamhouse-agent-hpa
  namespace: streamhouse
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: StatefulSet
    name: streamhouse-agent
  minReplicas: 6
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
  behavior:
    scaleUp:
      stabilizationWindowSeconds: 60
      policies:
      - type: Percent
        value: 50
        periodSeconds: 60
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Pods
        value: 1
        periodSeconds: 120
```

### Control Plane Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: streamhouse-control-plane
  namespace: streamhouse
spec:
  replicas: 3
  selector:
    matchLabels:
      app: control-plane
  template:
    metadata:
      labels:
        app: control-plane
    spec:
      containers:
      - name: api
        image: streamhouse/control-plane:v1.0.0
        ports:
        - containerPort: 8080
        env:
        - name: POSTGRES_URL
          valueFrom:
            secretKeyRef:
              name: streamhouse-secrets
              key: postgres-url
        - name: STRIPE_KEY
          valueFrom:
            secretKeyRef:
              name: streamhouse-secrets
              key: stripe-key
        resources:
          requests:
            cpu: 500m
            memory: 1Gi
          limits:
            cpu: 1000m
            memory: 2Gi
```

---

## Auto-Scaling Strategy

### Horizontal Pod Autoscaler (HPA)

**Scale based on**:
- CPU utilization > 70%
- Memory utilization > 80%
- Custom metric: Network throughput (Mbps)

### Karpenter (Node Autoscaler)

```yaml
apiVersion: karpenter.sh/v1alpha5
kind: Provisioner
metadata:
  name: default
spec:
  requirements:
  - key: karpenter.sh/capacity-type
    operator: In
    values: ["spot", "on-demand"]
  - key: node.kubernetes.io/instance-type
    operator: In
    values: ["c6i.xlarge", "c6i.2xlarge"]
  - key: topology.kubernetes.io/zone
    operator: In
    values: ["us-east-1a", "us-east-1b", "us-east-1c"]

  limits:
    resources:
      cpu: 1000
      memory: 1000Gi

  providerRef:
    name: default

  # Spot instance strategy
  weight: 100
```

**Benefits**:
- Provision nodes in seconds (vs minutes with cluster autoscaler)
- Bin-packing optimization
- 80% spot instances for cost savings
- Automatic instance type selection

---

## Database Strategy

### Amazon RDS PostgreSQL

**Configuration**:
```yaml
Instance: db.r6g.xlarge (reserved, 1-year)
  - 4 vCPUs (ARM Graviton2)
  - 32 GB RAM
  - 500 GB GP3 storage (10,000 IOPS)

Multi-AZ: Enabled
  - Primary: us-east-1a
  - Standby: us-east-1b
  - Automatic failover < 60s

Backups:
  - Automated daily snapshots
  - Retention: 30 days
  - Point-in-time recovery enabled

Read Replicas: 2
  - us-east-1a (for read traffic)
  - us-east-1c (disaster recovery)

Cost: $400/month (reserved)
```

### Connection Pooling

Use **PgBouncer** in transaction mode:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: pgbouncer
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: pgbouncer
        image: edoburu/pgbouncer:1.19.0
        env:
        - name: DATABASES_HOST
          value: streamhouse-prod.abc123.us-east-1.rds.amazonaws.com
        - name: DATABASES_PORT
          value: "5432"
        - name: POOL_MODE
          value: "transaction"
        - name: MAX_CLIENT_CONN
          value: "1000"
        - name: DEFAULT_POOL_SIZE
          value: "25"
        resources:
          requests:
            cpu: 100m
            memory: 256Mi
```

**Benefits**:
- Reduced connections to RDS (25 vs 1000)
- Better connection reuse
- Lower RDS costs

---

## Storage Strategy

### S3 Bucket Configuration

```hcl
resource "aws_s3_bucket" "streamhouse_prod" {
  bucket = "streamhouse-prod-us-east-1"

  versioning {
    enabled = true
  }

  lifecycle_rule {
    id      = "tier-to-intelligent"
    enabled = true

    transition {
      days          = 30
      storage_class = "INTELLIGENT_TIERING"
    }

    transition {
      days          = 90
      storage_class = "GLACIER_IR"
    }

    expiration {
      days = 730  # 2 years
    }
  }

  server_side_encryption_configuration {
    rule {
      apply_server_side_encryption_by_default {
        sse_algorithm = "AES256"
      }
    }
  }
}

resource "aws_s3_bucket_public_access_block" "streamhouse_prod" {
  bucket = aws_s3_bucket.streamhouse_prod.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}
```

### Cost Optimization

**Storage Tiers**:
- Standard (0-30 days): $0.023/GB
- Intelligent-Tiering (30-90 days): Auto-optimized
- Glacier Instant Retrieval (90+ days): $0.004/GB

**Example**:
- 10 TB total storage
- 1 TB recent (Standard): $23/month
- 3 TB moderate (Intelligent): $45/month
- 6 TB old (Glacier IR): $24/month
- **Total**: $92/month (vs $230 all-standard)

---

## Monitoring & Observability

### Metrics Stack

**Prometheus + Grafana Cloud**

```yaml
# Prometheus scrape config
scrape_configs:
- job_name: 'streamhouse-agent'
  kubernetes_sd_configs:
  - role: pod
  relabel_configs:
  - source_labels: [__meta_kubernetes_pod_label_app]
    regex: streamhouse-agent
    action: keep
  metric_relabel_configs:
  - source_labels: [__name__]
    regex: (streamhouse_.*|go_.*|process_.*)
    action: keep

# Key metrics
- streamhouse_agent_write_latency_seconds
- streamhouse_agent_read_latency_seconds
- streamhouse_partition_lease_count
- streamhouse_partition_failover_total
- streamhouse_segment_cache_hit_ratio
- streamhouse_metadata_query_duration_seconds
```

**Grafana Dashboards**:
1. System Overview (agents, partitions, throughput)
2. Performance (latency p50/p95/p99)
3. Agent Health (heartbeats, leases, failures)
4. Customer Metrics (per-tenant usage)
5. Cost Analysis (S3, RDS, compute)

### Logging Stack

**Loki + Grafana Cloud**

```yaml
# Promtail config
clients:
- url: https://logs-prod-us-east-1.grafana.net/loki/api/v1/push
  basic_auth:
    username: ${GRAFANA_USER}
    password: ${GRAFANA_API_KEY}

scrape_configs:
- job_name: kubernetes-pods
  kubernetes_sd_configs:
  - role: pod
  pipeline_stages:
  - json:
      expressions:
        level: level
        msg: msg
        agent_id: agent_id
        topic: topic
        partition: partition
  - labels:
      level:
      agent_id:
      topic:
```

### Tracing Stack

**Tempo + Grafana Cloud**

```yaml
# OpenTelemetry collector
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

exporters:
  otlp:
    endpoint: tempo-us-east-1.grafana.net:443
    headers:
      authorization: Basic ${GRAFANA_TEMPO_KEY}

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [otlp]
```

**Traced Operations**:
- gRPC produce/consume requests
- Partition lease acquisition
- S3 upload/download
- PostgreSQL queries

---

## Security

### Network Security

**VPC Configuration**:
```
VPC: 10.0.0.0/16
  - Public Subnets: 10.0.1.0/24, 10.0.2.0/24, 10.0.3.0/24
  - Private Subnets: 10.0.11.0/24, 10.0.12.0/24, 10.0.13.0/24
  - Database Subnets: 10.0.21.0/24, 10.0.22.0/24, 10.0.23.0/24

Security Groups:
  - Agent: Allow 9090 from NLB, 9091 from Prometheus
  - RDS: Allow 5432 from agents only
  - Control Plane: Allow 8080 from API Gateway
```

### Pod Security Standards

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: streamhouse
  labels:
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/audit: restricted
    pod-security.kubernetes.io/warn: restricted
```

### Network Policies

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: agent-network-policy
  namespace: streamhouse
spec:
  podSelector:
    matchLabels:
      app: streamhouse-agent
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - podSelector:
        matchLabels:
          app: streamhouse-agent
    ports:
    - protocol: TCP
      port: 9090
  egress:
  - to:
    - namespaceSelector: {}
    ports:
    - protocol: TCP
      port: 5432  # PostgreSQL
  - to:
    - namespaceSelector: {}
    ports:
    - protocol: TCP
      port: 443  # S3 HTTPS
```

### Service Mesh (Optional)

**Istio** for advanced features:

```yaml
apiVersion: security.istio.io/v1beta1
kind: PeerAuthentication
metadata:
  name: default
  namespace: streamhouse
spec:
  mtls:
    mode: STRICT  # Enforce mTLS between all pods
```

---

## Disaster Recovery

### RTO/RPO Goals

| Tier | RTO | RPO | Cost |
|------|-----|-----|------|
| Standard | 1 hour | 15 min | Base |
| Premium | 15 min | 5 min | +30% |
| Enterprise | 5 min | 0 min | +60% |

### Backup Strategy

**PostgreSQL**:
- Automated daily snapshots (30 days retention)
- Transaction logs backed up every 5 minutes
- Point-in-time recovery enabled

**S3**:
- Versioning enabled
- Cross-region replication to backup region
- Lifecycle policies prevent accidental deletion

### DR Procedures

**1. Region Failure**
```bash
# Automated failover to backup region
aws route53 change-resource-record-sets \
  --hosted-zone-id Z123 \
  --change-batch file://failover.json

# Promote backup region RDS replica
aws rds promote-read-replica \
  --db-instance-identifier streamhouse-prod-us-west-2
```

**2. Data Corruption**
```bash
# Point-in-time recovery
aws rds restore-db-instance-to-point-in-time \
  --source-db-instance-identifier streamhouse-prod \
  --target-db-instance-identifier streamhouse-prod-recovery \
  --restore-time 2026-01-23T12:00:00Z
```

---

## Cost Analysis

### Monthly Costs (1TB workload)

| Component | Cost | Notes |
|-----------|------|-------|
| **Compute** | | |
| EKS Control Plane | $72 | Fixed |
| Agent Nodes (10×) | $600 | c6i.xlarge spot |
| System Nodes (3×) | $90 | t3.medium on-demand |
| **Database** | | |
| RDS Primary | $280 | db.r6g.xlarge reserved |
| Read Replicas (2×) | $120 | db.r6g.large |
| **Storage** | | |
| S3 Standard (1TB) | $23 | Recent data |
| S3 Intelligent (3TB) | $45 | Older data |
| RDS Storage (500GB) | $75 | GP3 |
| **Networking** | | |
| NAT Gateway | $45 | 3 AZs × $0.045/hour |
| Data Transfer Out | $90 | 1TB × $0.09/GB |
| NLB | $20 | 3 AZs |
| **Observability** | | |
| Grafana Cloud | $100 | Metrics + logs + traces |
| **Total** | **$1,560** | **+ $113/TB stored** |

### Revenue Model

**Pricing Tiers**:
```
Starter:  $49/month  - 10 GB ingress, 100 GB storage
Growth:   $199/month - 100 GB ingress, 1 TB storage
Business: $499/month - 500 GB ingress, 5 TB storage
Enterprise: Custom   - Unlimited, dedicated cluster
```

**Unit Economics** (Growth tier example):
- Revenue: $199/month
- Cost: $50/month (20 customers share $1,000 infra)
- Margin: $149/month (75%)

---

## Operations

### Deployment Process

**CI/CD Pipeline** (GitHub Actions):

```yaml
name: Deploy to Production

on:
  push:
    branches: [main]
    tags: ['v*']

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3

    - name: Build Docker images
      run: |
        docker build -t streamhouse/agent:${{ github.sha }} .
        docker push streamhouse/agent:${{ github.sha }}

    - name: Update K8s manifests
      run: |
        kubectl set image statefulset/streamhouse-agent \
          agent=streamhouse/agent:${{ github.sha }}

    - name: Wait for rollout
      run: |
        kubectl rollout status statefulset/streamhouse-agent

    - name: Run smoke tests
      run: ./scripts/smoke-test.sh
```

### On-Call Rotation

**PagerDuty Integration**:

```yaml
# Alert rules
groups:
- name: streamhouse
  rules:
  - alert: AgentDown
    expr: up{job="streamhouse-agent"} == 0
    for: 5m
    annotations:
      summary: Agent {{ $labels.pod }} is down

  - alert: HighWriteLatency
    expr: histogram_quantile(0.99, streamhouse_write_latency_seconds) > 1
    for: 10m
    annotations:
      summary: Write p99 latency > 1s

  - alert: DatabaseConnections
    expr: pg_stat_database_numbackends > 80
    for: 5m
    annotations:
      summary: PostgreSQL connections approaching limit
```

---

## Timeline

### Phase 1: Foundation (Month 1-2)
- ✅ Set up AWS accounts, VPCs
- ✅ Deploy EKS cluster with Terraform
- ✅ Set up RDS PostgreSQL
- ✅ Configure S3 buckets
- ✅ Deploy monitoring stack

### Phase 2: Core Service (Month 3-4)
- ✅ Deploy StreamHouse agents
- ✅ Set up control plane (web UI, API)
- ✅ Configure auto-scaling
- ✅ Implement billing integration

### Phase 3: Production Hardening (Month 5-6)
- ✅ Security audit
- ✅ Load testing
- ✅ DR procedures
- ✅ Documentation

### Phase 4: Launch (Month 7)
- ✅ Beta customers
- ✅ Public launch
- ✅ Marketing

---

## Summary

**Stack**:
- ✅ Kubernetes (EKS) for orchestration
- ✅ RDS PostgreSQL for metadata
- ✅ S3 for segment storage
- ✅ Grafana Cloud for observability
- ✅ Terraform for IaC
- ✅ ArgoCD for GitOps

**Cost**: ~$1,560/month for 1TB workload

**SLA Targets**:
- 99.9% uptime (8.76 hours downtime/year)
- Write latency: p99 < 100ms
- Read latency: p99 < 50ms

**This is the plan to run StreamHouse as a production managed service!** 🚀
