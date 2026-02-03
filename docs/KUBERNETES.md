# StreamHouse Kubernetes Deployment Guide

This guide covers deploying StreamHouse on Kubernetes using Helm charts.

## Prerequisites

- Kubernetes 1.24+
- Helm 3.x
- kubectl configured with cluster access
- StorageClass for persistent volumes (optional but recommended)

---

## Local Development with Kind

[Kind](https://kind.sigs.k8s.io/) (Kubernetes in Docker) provides a simple way to run Kubernetes locally for development and testing.

### 1. Install Kind

```bash
# macOS
brew install kind

# Linux
curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
chmod +x ./kind
sudo mv ./kind /usr/local/bin/kind

# Windows (PowerShell)
choco install kind
```

### 2. Create a Cluster

```bash
# Create a cluster named "streamhouse"
kind create cluster --name streamhouse

# Verify the cluster is running
kubectl cluster-info --context kind-streamhouse
kubectl get nodes
```

### 3. Build Docker Images

```bash
# Build the StreamHouse server image
docker build -t streamhouse/agent:0.1.0 .

# Build the Web UI image
docker build -t streamhouse/ui:0.1.0 ./web

# Verify images were built
docker images | grep streamhouse
```

### 4. Load Images into Kind

Kind runs Kubernetes inside Docker containers, so images need to be loaded into the cluster:

```bash
# Load server image
kind load docker-image streamhouse/agent:0.1.0 --name streamhouse

# Load UI image
kind load docker-image streamhouse/ui:0.1.0 --name streamhouse
```

### 5. Deploy StreamHouse

```bash
# Update Helm dependencies (PostgreSQL, MinIO subcharts)
helm dependency build kubernetes/streamhouse

# Install StreamHouse with development settings
helm install streamhouse kubernetes/streamhouse \
  --namespace streamhouse \
  --create-namespace \
  --set monitoring.serviceMonitor.enabled=false \
  --set minio.auth.rootPassword=minioadmin123 \
  --set postgresql.auth.password=streamhouse123

# Watch pods come up
kubectl get pods -n streamhouse -w
```

### 6. Access Services

```bash
# Forward the API port
kubectl port-forward svc/streamhouse-agent 8080:8080 -n streamhouse &

# Forward the UI port
kubectl port-forward svc/streamhouse-ui 3000:80 -n streamhouse &

# Forward MinIO console (optional)
kubectl port-forward svc/streamhouse-minio 9001:9001 -n streamhouse &
```

Access the services:
- **API**: http://localhost:8080
- **UI**: http://localhost:3000
- **MinIO Console**: http://localhost:9001 (admin/minioadmin123)

### 7. Cleanup

```bash
# Uninstall StreamHouse
helm uninstall streamhouse -n streamhouse

# Delete the namespace
kubectl delete namespace streamhouse

# Delete the Kind cluster
kind delete cluster --name streamhouse
```

### Troubleshooting Kind

**Pods stuck in ImagePullBackOff:**
```bash
# Check if images are loaded
docker exec -it streamhouse-control-plane crictl images | grep streamhouse

# Re-load images if needed
kind load docker-image streamhouse/agent:0.1.0 --name streamhouse
```

**Insufficient resources:**
```bash
# Increase Docker Desktop resources (Settings > Resources)
# Recommended: 4 CPU, 8GB RAM

# Or use minimal dev values
helm install streamhouse kubernetes/streamhouse \
  -f kubernetes/streamhouse/values-dev.yaml \
  --namespace streamhouse --create-namespace
```

**Port conflicts:**
```bash
# Check what's using a port
lsof -i :8080

# Kill existing port-forwards
pkill -f "kubectl port-forward"
```

---

## Quick Start

### Development/Testing Deployment

```bash
# Add the StreamHouse Helm repository (when published)
helm repo add streamhouse https://charts.streamhouse.dev
helm repo update

# Or install from local charts
cd kubernetes/streamhouse

# Install with default values (includes MinIO and PostgreSQL)
helm install streamhouse . --namespace streamhouse --create-namespace
```

### Verify Installation

```bash
# Check pod status
kubectl get pods -n streamhouse

# Expected output:
# NAME                                        READY   STATUS    RESTARTS   AGE
# streamhouse-agent-0                         1/1     Running   0          2m
# streamhouse-agent-1                         1/1     Running   0          2m
# streamhouse-agent-2                         1/1     Running   0          2m
# streamhouse-schema-registry-xxx-xxx         1/1     Running   0          2m
# streamhouse-ui-xxx-xxx                      1/1     Running   0          2m
# streamhouse-postgresql-0                    1/1     Running   0          2m
# streamhouse-minio-xxx-xxx                   1/1     Running   0          2m

# Check services
kubectl get svc -n streamhouse
```

## Configuration

### values.yaml Structure

The Helm chart is configured via `values.yaml`. Key sections:

| Section | Description |
|---------|-------------|
| `agent` | StreamHouse agent StatefulSet configuration |
| `schemaRegistry` | Schema registry deployment |
| `ui` | Web UI deployment |
| `postgresql` | PostgreSQL database (Bitnami subchart) |
| `minio` | MinIO object storage (Bitnami subchart) |
| `monitoring` | Prometheus ServiceMonitor and Grafana dashboards |
| `ingress` | External access configuration |

### Common Configuration Examples

#### 1. Production with External AWS S3

```yaml
# values-production-aws.yaml
agent:
  replicaCount: 5
  resources:
    requests:
      memory: "4Gi"
      cpu: "2000m"
    limits:
      memory: "8Gi"
      cpu: "4000m"
  storage:
    bucket: "my-streamhouse-bucket"
    region: "us-west-2"
    pathStyle: false
  wal:
    enabled: true
    syncPolicy: "interval"
    syncIntervalMs: 50

# Disable MinIO, use AWS S3 with IAM roles
minio:
  enabled: false

# External PostgreSQL
postgresql:
  enabled: false
  external:
    host: "my-rds-instance.xxx.us-west-2.rds.amazonaws.com"
    port: 5432
    username: "streamhouse"
    password: ""  # Use --set postgresql.external.password=xxx
    database: "streamhouse"

# Enable ingress
ingress:
  enabled: true
  className: "nginx"
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
  hosts:
    - host: streamhouse.mycompany.com
      paths:
        - path: /api
          pathType: Prefix
          service: api
        - path: /schemas
          pathType: Prefix
          service: schema-registry
        - path: /
          pathType: Prefix
          service: ui
  tls:
    - secretName: streamhouse-tls
      hosts:
        - streamhouse.mycompany.com
```

```bash
helm install streamhouse . \
  --namespace streamhouse \
  --create-namespace \
  -f values-production-aws.yaml \
  --set postgresql.external.password="$DB_PASSWORD"
```

#### 2. On-Premise with MinIO

```yaml
# values-onprem.yaml
agent:
  replicaCount: 3
  storage:
    bucket: "streamhouse"
    region: "us-east-1"
    endpoint: ""  # Will be auto-configured for MinIO
    pathStyle: true

minio:
  enabled: true
  auth:
    rootUser: "admin"
    rootPassword: ""  # Set via --set
  persistence:
    enabled: true
    size: 500Gi
    storageClass: "fast-storage"

postgresql:
  enabled: true
  auth:
    username: "streamhouse"
    password: ""  # Set via --set
    database: "streamhouse"
  primary:
    persistence:
      enabled: true
      size: 50Gi
      storageClass: "fast-storage"
```

```bash
helm install streamhouse . \
  --namespace streamhouse \
  --create-namespace \
  -f values-onprem.yaml \
  --set minio.auth.rootPassword="$MINIO_PASSWORD" \
  --set postgresql.auth.password="$DB_PASSWORD"
```

#### 3. Development/CI with Minimal Resources

```yaml
# values-dev.yaml
agent:
  replicaCount: 1
  resources:
    requests:
      memory: "512Mi"
      cpu: "250m"
    limits:
      memory: "1Gi"
      cpu: "500m"
  persistence:
    enabled: false  # No WAL persistence for dev

schemaRegistry:
  replicaCount: 1
  resources:
    requests:
      memory: "256Mi"
      cpu: "100m"

ui:
  replicaCount: 1

minio:
  persistence:
    enabled: false

postgresql:
  primary:
    persistence:
      enabled: false
```

## Accessing Services

### Port Forwarding (Development)

```bash
# Access the API
kubectl port-forward svc/streamhouse-agent 8080:8080 -n streamhouse

# Access the UI
kubectl port-forward svc/streamhouse-ui 3000:80 -n streamhouse

# Access Schema Registry
kubectl port-forward svc/streamhouse-schema-registry 8081:8081 -n streamhouse
```

### Using Ingress (Production)

With ingress enabled, access services at:
- **API**: `https://streamhouse.mycompany.com/api/v1/topics`
- **UI**: `https://streamhouse.mycompany.com/`
- **Schema Registry**: `https://streamhouse.mycompany.com/schemas/`

## Monitoring

### Prometheus Integration

The chart creates ServiceMonitors when `monitoring.serviceMonitor.enabled: true`.

```yaml
monitoring:
  serviceMonitor:
    enabled: true
    interval: 15s
    labels:
      release: prometheus  # Match your Prometheus operator labels
```

### Grafana Dashboards

Import dashboards from `grafana/dashboards/`:
- `streamhouse-overview.json` - Cluster overview
- `streamhouse-agent.json` - Agent metrics
- `streamhouse-schema-registry.json` - Schema registry metrics
- `streamhouse-wal.json` - WAL health
- `streamhouse-s3-throttling.json` - S3 and throttling metrics

## High Availability

### Agent StatefulSet

Agents run as a StatefulSet with:
- Stable network identities (`streamhouse-agent-0`, `streamhouse-agent-1`, etc.)
- Persistent WAL storage per pod
- Ordered, graceful deployment and scaling

### Pod Disruption Budgets

```yaml
agent:
  pdb:
    enabled: true
    minAvailable: 2  # At least 2 agents must be available

schemaRegistry:
  pdb:
    enabled: true
    minAvailable: 1
```

### Anti-Affinity

For production, spread pods across nodes:

```yaml
agent:
  affinity:
    podAntiAffinity:
      preferredDuringSchedulingIgnoredDuringExecution:
        - weight: 100
          podAffinityTerm:
            labelSelector:
              matchLabels:
                app.kubernetes.io/component: agent
            topologyKey: kubernetes.io/hostname
```

## Scaling

### Manual Scaling

```bash
# Scale agents
kubectl scale statefulset streamhouse-agent --replicas=5 -n streamhouse

# Scale schema registry
kubectl scale deployment streamhouse-schema-registry --replicas=3 -n streamhouse
```

### Horizontal Pod Autoscaling (HPA)

```yaml
agent:
  autoscaling:
    enabled: true
    minReplicas: 3
    maxReplicas: 10
    targetCPUUtilizationPercentage: 70
    targetMemoryUtilizationPercentage: 80
```

## Security

### RBAC

The chart creates necessary ServiceAccount and RBAC resources:

```yaml
rbac:
  create: true

serviceAccount:
  create: true
  annotations:
    # For AWS IRSA
    eks.amazonaws.com/role-arn: "arn:aws:iam::123456789:role/streamhouse-role"
```

### Pod Security

All containers run as non-root by default:

```yaml
agent:
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    readOnlyRootFilesystem: false  # WAL needs write access
    allowPrivilegeEscalation: false
```

### Network Policies

```yaml
networkPolicy:
  enabled: true
  ingress:
    - from:
        - namespaceSelector:
            matchLabels:
              name: monitoring
      ports:
        - port: 8080
```

## Troubleshooting

### Check Pod Logs

```bash
# Agent logs
kubectl logs -f streamhouse-agent-0 -n streamhouse

# Schema registry logs
kubectl logs -f deploy/streamhouse-schema-registry -n streamhouse
```

### Health Checks

```bash
# Agent health
kubectl exec -it streamhouse-agent-0 -n streamhouse -- curl localhost:8080/health

# Agent readiness
kubectl exec -it streamhouse-agent-0 -n streamhouse -- curl localhost:8080/ready
```

### Common Issues

**Pods stuck in Pending:**
```bash
kubectl describe pod <pod-name> -n streamhouse
# Check for resource constraints or PVC issues
```

**Connection to PostgreSQL failing:**
```bash
# Verify PostgreSQL is running
kubectl get pods -l app.kubernetes.io/name=postgresql -n streamhouse

# Check connection string
kubectl get secret streamhouse-secrets -n streamhouse -o jsonpath='{.data.DATABASE_URL}' | base64 -d
```

**S3 connection issues:**
```bash
# Check MinIO (if enabled)
kubectl get pods -l app.kubernetes.io/name=minio -n streamhouse

# Verify S3 credentials
kubectl get secret streamhouse-secrets -n streamhouse -o jsonpath='{.data.S3_ACCESS_KEY}' | base64 -d
```

## Upgrading

```bash
# Update values
helm upgrade streamhouse . \
  --namespace streamhouse \
  -f values-production.yaml \
  --set image.tag="0.2.0"

# Rollback if needed
helm rollback streamhouse -n streamhouse
```

## Uninstalling

```bash
# Remove StreamHouse
helm uninstall streamhouse -n streamhouse

# Remove namespace (including PVCs)
kubectl delete namespace streamhouse

# Or keep PVCs for data preservation
kubectl delete namespace streamhouse --cascade=orphan
```

## Chart Dependencies

| Dependency | Version | Condition |
|------------|---------|-----------|
| postgresql (Bitnami) | 15.x | `postgresql.enabled` |
| minio (Bitnami) | 14.x | `minio.enabled` |

```bash
# Update dependencies
helm dependency update kubernetes/streamhouse
```

---

## Cloud Deployment

### AWS EKS

**1. Create EKS Cluster**
```bash
# Install eksctl if needed
brew install eksctl

# Create cluster
eksctl create cluster \
  --name streamhouse \
  --region us-west-2 \
  --nodegroup-name workers \
  --node-type t3.medium \
  --nodes 3 \
  --nodes-min 1 \
  --nodes-max 5

# Verify
kubectl get nodes
```

**2. Install AWS Load Balancer Controller** (for Ingress)
```bash
# Add IAM policy
eksctl utils associate-iam-oidc-provider --cluster streamhouse --approve

# Install controller
helm repo add eks https://aws.github.io/eks-charts
helm install aws-load-balancer-controller eks/aws-load-balancer-controller \
  -n kube-system \
  --set clusterName=streamhouse
```

**3. Deploy StreamHouse with RDS + S3**
```yaml
# values-eks.yaml
agent:
  replicaCount: 3
  storage:
    bucket: "your-streamhouse-bucket"
    region: "us-west-2"
    pathStyle: false

# Use AWS RDS instead of bundled PostgreSQL
postgresql:
  enabled: false
  external:
    host: "your-rds.xxxx.us-west-2.rds.amazonaws.com"
    port: 5432
    username: "streamhouse"
    password: ""  # Use --set
    database: "streamhouse"

# Use AWS S3 instead of MinIO
minio:
  enabled: false

# IAM Roles for Service Accounts (IRSA)
serviceAccount:
  annotations:
    eks.amazonaws.com/role-arn: "arn:aws:iam::ACCOUNT:role/streamhouse-role"

ingress:
  enabled: true
  className: alb
  annotations:
    alb.ingress.kubernetes.io/scheme: internet-facing
    alb.ingress.kubernetes.io/target-type: ip
```

```bash
helm install streamhouse kubernetes/streamhouse \
  -n streamhouse --create-namespace \
  -f values-eks.yaml \
  --set postgresql.external.password="$DB_PASSWORD"
```

---

### Azure AKS

**1. Create AKS Cluster**
```bash
# Create resource group
az group create --name streamhouse-rg --location eastus

# Create cluster
az aks create \
  --resource-group streamhouse-rg \
  --name streamhouse \
  --node-count 3 \
  --node-vm-size Standard_D2s_v3 \
  --enable-managed-identity \
  --generate-ssh-keys

# Get credentials
az aks get-credentials --resource-group streamhouse-rg --name streamhouse
```

**2. Deploy with Azure Database + Blob Storage**
```yaml
# values-aks.yaml
agent:
  replicaCount: 3
  storage:
    bucket: "streamhouse"
    endpoint: "https://youraccount.blob.core.windows.net"
    region: "eastus"

postgresql:
  enabled: false
  external:
    host: "your-postgres.postgres.database.azure.com"
    port: 5432
    username: "streamhouse"
    database: "streamhouse"

minio:
  enabled: false
  external:
    endpoint: "https://youraccount.blob.core.windows.net"
    accessKey: ""  # Azure Storage Account name
    secretKey: ""  # Azure Storage Account key

ingress:
  enabled: true
  className: nginx
  annotations:
    kubernetes.io/ingress.class: nginx
```

```bash
helm install streamhouse kubernetes/streamhouse \
  -n streamhouse --create-namespace \
  -f values-aks.yaml \
  --set postgresql.external.password="$DB_PASSWORD" \
  --set minio.external.accessKey="$STORAGE_ACCOUNT" \
  --set minio.external.secretKey="$STORAGE_KEY"
```

---

### Google GKE

**1. Create GKE Cluster**
```bash
# Set project
gcloud config set project YOUR_PROJECT_ID

# Create cluster
gcloud container clusters create streamhouse \
  --zone us-central1-a \
  --num-nodes 3 \
  --machine-type e2-standard-2

# Get credentials
gcloud container clusters get-credentials streamhouse --zone us-central1-a
```

**2. Deploy with Cloud SQL + GCS**
```yaml
# values-gke.yaml
agent:
  replicaCount: 3
  storage:
    bucket: "your-streamhouse-bucket"
    region: "us-central1"

postgresql:
  enabled: false
  external:
    host: "INSTANCE_IP"  # Cloud SQL private IP
    port: 5432
    username: "streamhouse"
    database: "streamhouse"

minio:
  enabled: false

# Workload Identity
serviceAccount:
  annotations:
    iam.gke.io/gcp-service-account: "streamhouse@PROJECT.iam.gserviceaccount.com"

ingress:
  enabled: true
  className: gce
```

```bash
helm install streamhouse kubernetes/streamhouse \
  -n streamhouse --create-namespace \
  -f values-gke.yaml \
  --set postgresql.external.password="$DB_PASSWORD"
```

---

### DigitalOcean DOKS

**1. Create Cluster**
```bash
doctl kubernetes cluster create streamhouse \
  --region nyc1 \
  --size s-2vcpu-4gb \
  --count 3
```

**2. Deploy with Managed Database + Spaces**
```yaml
# values-doks.yaml
agent:
  replicaCount: 2
  storage:
    bucket: "streamhouse"
    endpoint: "https://nyc3.digitaloceanspaces.com"
    region: "nyc3"

postgresql:
  enabled: false
  external:
    host: "your-db-cluster.db.ondigitalocean.com"
    port: 25060
    username: "streamhouse"
    database: "streamhouse"

minio:
  enabled: false
  external:
    endpoint: "https://nyc3.digitaloceanspaces.com"
```

---

## Cost Optimization

| Cloud | Minimum Setup | Estimated Cost/Month |
|-------|---------------|---------------------|
| AWS EKS | 2x t3.medium + RDS db.t3.micro + S3 | ~$150 |
| Azure AKS | 2x Standard_B2s + Basic DB + Blob | ~$120 |
| GKE | 2x e2-small + Cloud SQL basic + GCS | ~$100 |
| DigitalOcean | 2x s-2vcpu-4gb + Managed DB + Spaces | ~$80 |

For development/testing, use the bundled PostgreSQL and MinIO to avoid managed service costs.
