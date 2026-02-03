# StreamHouse Production Deployment Guide

A comprehensive guide for deploying StreamHouse in production environments.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Hardware Requirements](#hardware-requirements)
3. [Deployment Options](#deployment-options)
4. [Configuration](#configuration)
5. [Security](#security)
6. [Monitoring & Alerting](#monitoring--alerting)
7. [Backup & Recovery](#backup--recovery)
8. [Scaling](#scaling)
9. [Troubleshooting](#troubleshooting)

## Architecture Overview

```
                    ┌─────────────────┐
                    │   Load Balancer │
                    │  (nginx/HAProxy)│
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
        ┌─────▼─────┐  ┌─────▼─────┐  ┌─────▼─────┐
        │StreamHouse│  │StreamHouse│  │StreamHouse│
        │  Agent 1  │  │  Agent 2  │  │  Agent 3  │
        └─────┬─────┘  └─────┬─────┘  └─────┬─────┘
              │              │              │
              └──────────────┼──────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
  ┌─────▼─────┐        ┌─────▼─────┐        ┌─────▼─────┐
  │ PostgreSQL│        │    S3     │        │   MinIO   │
  │  (Primary)│        │  (AWS)    │        │  (Self-   │
  │           │        │           │        │  Hosted)  │
  └───────────┘        └───────────┘        └───────────┘
```

## Hardware Requirements

### Minimum (Development/Testing)

| Resource | Specification |
|----------|---------------|
| CPU | 2 cores |
| RAM | 4 GB |
| Disk | 20 GB SSD |
| Network | 100 Mbps |

### Recommended (Production)

| Resource | Specification |
|----------|---------------|
| CPU | 8+ cores |
| RAM | 16-32 GB |
| Disk | 100+ GB NVMe SSD |
| Network | 1+ Gbps |

### High-Performance

| Resource | Specification |
|----------|---------------|
| CPU | 16-32 cores |
| RAM | 64-128 GB |
| Disk | 500+ GB NVMe SSD (for WAL) |
| Network | 10+ Gbps |

### Sizing Guidelines

| Throughput | Agents | PostgreSQL | S3 Storage |
|------------|--------|------------|------------|
| 10K msg/s | 1 | db.t3.medium | Standard |
| 100K msg/s | 2-3 | db.r5.large | Standard |
| 500K msg/s | 5-10 | db.r5.xlarge | Standard |
| 1M+ msg/s | 10+ | db.r5.2xlarge+ | Intelligent Tiering |

## Deployment Options

### Option 1: Docker Compose (Single Server)

Best for: Small deployments, < 50K msg/s

```bash
# Clone repository
git clone https://github.com/streamhouse/streamhouse
cd streamhouse

# Configure environment
cp .env.example .env
# Edit .env with production values

# Start services
docker compose -f docker-compose.prod.yml up -d
```

### Option 2: DigitalOcean Droplet

Best for: Medium deployments, cost-effective

```bash
# On your DigitalOcean droplet
# 1. Install Docker
curl -fsSL https://get.docker.com | sh

# 2. Clone and configure
git clone https://github.com/streamhouse/streamhouse
cd streamhouse

# 3. Configure for production
export DATABASE_URL="postgres://user:pass@your-managed-postgres:5432/streamhouse"
export S3_ENDPOINT="https://nyc3.digitaloceanspaces.com"
export S3_BUCKET="your-bucket"
export AWS_ACCESS_KEY_ID="your-spaces-key"
export AWS_SECRET_ACCESS_KEY="your-spaces-secret"

# 4. Start
docker compose up -d
```

### Option 3: AWS with Managed Services

Best for: Large deployments, high availability

```bash
# Use AWS RDS for PostgreSQL
DATABASE_URL=postgres://user:pass@your-rds-instance.region.rds.amazonaws.com:5432/streamhouse

# Use native S3
S3_BUCKET=your-streamhouse-bucket
AWS_REGION=us-east-1

# Deploy via ECS, EKS, or EC2
```

## Configuration

### Environment Variables

```bash
# Required
DATABASE_URL=postgres://user:password@host:5432/streamhouse
S3_ENDPOINT=https://s3.us-east-1.amazonaws.com  # or MinIO URL
S3_BUCKET=streamhouse-data
AWS_ACCESS_KEY_ID=your-access-key
AWS_SECRET_ACCESS_KEY=your-secret-key

# Server
HTTP_PORT=8080
GRPC_PORT=9090
RUST_LOG=info

# Performance
WAL_ENABLED=true
WAL_DIR=/data/wal
WAL_SYNC_MS=100
SEGMENT_CACHE_SIZE=1073741824  # 1GB
BATCH_SIZE=1000
BATCH_TIMEOUT_MS=10

# S3 Settings
S3_REGION=us-east-1
S3_PATH_STYLE=false  # true for MinIO

# TLS (optional)
TLS_CERT_PATH=/etc/ssl/certs/streamhouse.crt
TLS_KEY_PATH=/etc/ssl/private/streamhouse.key
```

### Production docker-compose.yml

```yaml
services:
  streamhouse:
    image: streamhouse/server:latest
    restart: always
    ports:
      - "8080:8080"
      - "9090:9090"
    environment:
      DATABASE_URL: ${DATABASE_URL}
      S3_ENDPOINT: ${S3_ENDPOINT}
      S3_BUCKET: ${S3_BUCKET}
      AWS_ACCESS_KEY_ID: ${AWS_ACCESS_KEY_ID}
      AWS_SECRET_ACCESS_KEY: ${AWS_SECRET_ACCESS_KEY}
      WAL_ENABLED: "true"
      WAL_DIR: /data/wal
      RUST_LOG: info
    volumes:
      - wal_data:/data/wal
      - cache_data:/data/cache
    deploy:
      resources:
        limits:
          cpus: '4'
          memory: 8G
        reservations:
          cpus: '2'
          memory: 4G
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 10s
      timeout: 5s
      retries: 3

volumes:
  wal_data:
  cache_data:
```

## Security

### TLS/SSL Configuration

```bash
# Generate certificates (production: use Let's Encrypt or your CA)
openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout streamhouse.key \
  -out streamhouse.crt \
  -subj "/CN=streamhouse.example.com"

# Configure StreamHouse
export TLS_CERT_PATH=/etc/ssl/certs/streamhouse.crt
export TLS_KEY_PATH=/etc/ssl/private/streamhouse.key
```

### Network Security

```yaml
# docker-compose.yml - Internal network only
services:
  streamhouse:
    networks:
      - internal
    # Don't expose ports directly in production
    # Use a reverse proxy instead

  nginx:
    image: nginx:alpine
    ports:
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    networks:
      - internal
      - external

networks:
  internal:
    internal: true
  external:
```

### Nginx Reverse Proxy

```nginx
# nginx.conf
upstream streamhouse {
    server streamhouse:8080;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name streamhouse.example.com;

    ssl_certificate /etc/ssl/certs/streamhouse.crt;
    ssl_certificate_key /etc/ssl/private/streamhouse.key;

    location / {
        proxy_pass http://streamhouse;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### S3 Bucket Policy

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "AWS": "arn:aws:iam::ACCOUNT_ID:role/streamhouse-role"
      },
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::your-bucket",
        "arn:aws:s3:::your-bucket/*"
      ]
    }
  ]
}
```

## Monitoring & Alerting

### Prometheus Metrics

StreamHouse exposes metrics at `/metrics`:

```bash
curl http://localhost:8080/metrics
```

Key metrics to monitor:

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `streamhouse_messages_produced_total` | Total messages produced | - |
| `streamhouse_messages_consumed_total` | Total messages consumed | - |
| `streamhouse_segment_writes_total` | Segments written to S3 | - |
| `streamhouse_s3_request_duration_seconds` | S3 latency | p99 > 500ms |
| `streamhouse_wal_entries` | WAL queue size | > 10000 |
| `streamhouse_active_connections` | Active connections | > 1000 |

### Grafana Dashboard

Import the StreamHouse dashboard from `grafana/dashboards/streamhouse-overview.json`.

### Alerting Rules

```yaml
# prometheus/alerts.yaml
groups:
  - name: streamhouse
    rules:
      - alert: HighS3Latency
        expr: histogram_quantile(0.99, streamhouse_s3_request_duration_seconds_bucket) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High S3 latency detected"

      - alert: WALBacklog
        expr: streamhouse_wal_entries > 10000
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "WAL backlog growing"

      - alert: HighErrorRate
        expr: rate(streamhouse_errors_total[5m]) > 10
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High error rate"
```

## Backup & Recovery

### PostgreSQL Backup

```bash
# Automated daily backup
pg_dump -h localhost -U streamhouse streamhouse | gzip > backup_$(date +%Y%m%d).sql.gz

# Upload to S3
aws s3 cp backup_$(date +%Y%m%d).sql.gz s3://backups/streamhouse/
```

### S3 Data Protection

Enable S3 versioning and lifecycle rules:

```bash
# Enable versioning
aws s3api put-bucket-versioning \
  --bucket your-bucket \
  --versioning-configuration Status=Enabled

# Lifecycle rule for old versions
aws s3api put-bucket-lifecycle-configuration \
  --bucket your-bucket \
  --lifecycle-configuration file://lifecycle.json
```

### Disaster Recovery

1. **RPO (Recovery Point Objective):** Minutes (S3 durability + WAL)
2. **RTO (Recovery Time Objective):** < 30 minutes

Recovery steps:
```bash
# 1. Restore PostgreSQL from backup
psql -h new-host -U streamhouse < backup.sql

# 2. Start StreamHouse with same S3 bucket
# Data is automatically available from S3

# 3. Verify
curl http://localhost:8080/health
curl http://localhost:8080/api/v1/topics
```

## Scaling

### Horizontal Scaling

Run multiple StreamHouse agents behind a load balancer:

```yaml
# docker-compose.scale.yml
services:
  streamhouse:
    image: streamhouse/server:latest
    deploy:
      replicas: 3
    environment:
      # All agents share same DATABASE_URL and S3
      DATABASE_URL: ${DATABASE_URL}
      S3_BUCKET: ${S3_BUCKET}
```

### Vertical Scaling

Increase resources for single agent:

```yaml
deploy:
  resources:
    limits:
      cpus: '8'
      memory: 32G
```

### Topic Partitioning

More partitions = more parallelism:

```bash
# Create topic with many partitions
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "high-throughput", "partitions": 32}'
```

## Troubleshooting

### Common Issues

**1. Connection refused to PostgreSQL**
```bash
# Check PostgreSQL is running
docker compose logs postgres

# Verify connection string
psql $DATABASE_URL -c "SELECT 1"
```

**2. S3 permission errors**
```bash
# Test S3 access
aws s3 ls s3://your-bucket/

# Check IAM credentials
aws sts get-caller-identity
```

**3. High memory usage**
```bash
# Reduce cache size
export SEGMENT_CACHE_SIZE=536870912  # 512MB

# Restart
docker compose restart streamhouse
```

**4. Slow writes**
```bash
# Enable WAL for durability without waiting for S3
export WAL_ENABLED=true
export WAL_SYNC_MS=100

# Increase batch size
export BATCH_SIZE=2000
```

### Logs

```bash
# View logs
docker compose logs -f streamhouse

# Filter for errors
docker compose logs streamhouse 2>&1 | grep -i error

# Increase log level
export RUST_LOG=debug
```

### Health Checks

```bash
# Basic health
curl http://localhost:8080/health

# Readiness (can serve traffic)
curl http://localhost:8080/ready

# Liveness (is running)
curl http://localhost:8080/live

# Metrics
curl http://localhost:8080/metrics | head -50
```

## Cost Estimation

### Monthly Cost Calculator

| Component | Small | Medium | Large |
|-----------|-------|--------|-------|
| **Compute** (StreamHouse agents) | $50 | $200 | $800 |
| **PostgreSQL** (managed) | $30 | $100 | $400 |
| **S3 Storage** (per TB) | $23 | $23 | $23 |
| **S3 Requests** (per million) | $0.40 | $4 | $40 |
| **Data Transfer** (per GB) | $0.09 | $0.09 | $0.09 |
| **Total (1TB data)** | ~$110 | ~$350 | ~$1,300 |

### Cost Optimization Tips

1. **Use S3 Intelligent Tiering** for automatic cost optimization
2. **Enable compression** to reduce storage costs
3. **Set retention policies** to delete old data
4. **Use reserved instances** for compute savings

## Checklist

### Pre-Deployment

- [ ] PostgreSQL provisioned and accessible
- [ ] S3 bucket created with proper IAM permissions
- [ ] TLS certificates obtained
- [ ] Network security groups configured
- [ ] Monitoring and alerting set up
- [ ] Backup strategy defined

### Post-Deployment

- [ ] Health checks passing
- [ ] Metrics flowing to Prometheus
- [ ] Test message produce/consume working
- [ ] Alerting rules firing correctly
- [ ] Runbook documentation complete

## Support

- GitHub Issues: https://github.com/streamhouse/streamhouse/issues
- Documentation: https://streamhouse.dev/docs
- Community Discord: https://discord.gg/streamhouse
