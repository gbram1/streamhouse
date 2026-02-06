# Deploy StreamHouse on DigitalOcean

This guide walks you through deploying StreamHouse on DigitalOcean with managed PostgreSQL and Spaces (S3-compatible storage).

**Estimated time:** 20-30 minutes  
**Estimated cost:** ~$30-50/month (Droplet + DB + Spaces)

## Prerequisites

- DigitalOcean account
- Domain name (optional, for HTTPS)

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   DigitalOcean                       │
│                                                     │
│  ┌─────────────┐     ┌──────────────────────────┐  │
│  │   Droplet   │     │   Managed PostgreSQL     │  │
│  │  (4GB RAM)  │────▶│   (db-s-1vcpu-1gb)      │  │
│  │             │     └──────────────────────────┘  │
│  │ StreamHouse │                                   │
│  │   Server    │     ┌──────────────────────────┐  │
│  │    + UI     │────▶│       Spaces             │  │
│  └─────────────┘     │   (S3-compatible)        │  │
│                      └──────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## Step 1: Create DigitalOcean Resources

### 1.1 Create a Managed PostgreSQL Database

1. Go to **Databases** → **Create Database Cluster**
2. Choose:
   - **Engine:** PostgreSQL 16
   - **Plan:** Basic ($15/mo - 1 vCPU, 1 GB RAM, 10 GB SSD)
   - **Datacenter:** Choose your region (e.g., NYC3)
   - **Name:** `streamhouse-db`
3. Click **Create Database Cluster**
4. Wait for it to be ready (~5 minutes)
5. Go to the database → **Connection Details** and note:
   - Host
   - Port
   - Username
   - Password
   - Database name (create one called `streamhouse`)

### 1.2 Create a Spaces Bucket

1. Go to **Spaces Object Storage** → **Create a Space**
2. Choose:
   - **Datacenter:** Same region as your database
   - **Name:** `streamhouse-data` (or your preferred name)
   - **File Listing:** Restricted
3. Click **Create a Space**
4. Go to **API** → **Spaces Keys** → **Generate New Key**
5. Note your Access Key and Secret Key

### 1.3 Create a Droplet

1. Go to **Droplets** → **Create Droplet**
2. Choose:
   - **Image:** Ubuntu 24.04 LTS
   - **Plan:** Basic - $24/mo (4 GB RAM, 2 vCPUs, 80 GB SSD)
   - **Datacenter:** Same region as your database
   - **Authentication:** SSH Key (recommended)
3. Click **Create Droplet**
4. Note the public IP address

## Step 2: Configure the Droplet

SSH into your droplet:

```bash
ssh root@your-droplet-ip
```

### 2.1 Create a non-root user (recommended)

```bash
# Create user
adduser streamhouse
usermod -aG sudo streamhouse

# Copy SSH keys
mkdir -p /home/streamhouse/.ssh
cp ~/.ssh/authorized_keys /home/streamhouse/.ssh/
chown -R streamhouse:streamhouse /home/streamhouse/.ssh

# Switch to new user
su - streamhouse
```

### 2.2 Install Docker

```bash
# Install Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# Log out and back in for group changes
exit
ssh streamhouse@your-droplet-ip

# Verify Docker works
docker --version
```

## Step 3: Deploy StreamHouse

### 3.1 Clone the repository

```bash
git clone https://github.com/yourusername/streamhouse.git
cd streamhouse
```

### 3.2 Configure environment

```bash
# Copy the example file
cp .env.production.example .env

# Edit with your values
nano .env
```

Fill in your values:

```bash
# Database - from Step 1.1
DATABASE_URL=postgres://doadmin:YOUR_PASSWORD@your-db-cluster.db.ondigitalocean.com:25060/streamhouse?sslmode=require

# Spaces - from Step 1.2
S3_ENDPOINT=https://nyc3.digitaloceanspaces.com
S3_BUCKET=streamhouse-data
S3_REGION=nyc3
AWS_ACCESS_KEY_ID=your-spaces-access-key
AWS_SECRET_ACCESS_KEY=your-spaces-secret-key
S3_PATH_STYLE=false

# Public URLs - use your droplet IP
PUBLIC_API_URL=http://YOUR_DROPLET_IP:8080
PUBLIC_WS_URL=ws://YOUR_DROPLET_IP:8080
```

### 3.3 Run the deployment script

```bash
./scripts/deploy-digitalocean.sh
```

This will:
1. Validate your environment
2. Build Docker images (~10-15 min first time)
3. Start StreamHouse

### 3.4 Verify deployment

```bash
# Check service health
curl http://localhost:8080/health

# View logs
docker compose -f docker-compose.prod.yml logs -f
```

## Step 4: Access StreamHouse

- **Web UI:** `http://YOUR_DROPLET_IP:3000`
- **REST API:** `http://YOUR_DROPLET_IP:8080`
- **gRPC:** `YOUR_DROPLET_IP:9090`

### Quick Test

```bash
# Create a topic
curl -X POST http://YOUR_DROPLET_IP:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "test-topic", "partitions": 3}'

# Produce a message
curl -X POST http://YOUR_DROPLET_IP:8080/api/v1/topics/test-topic/messages \
  -H "Content-Type: application/json" \
  -d '{"key": "test-key", "value": "{\"hello\": \"world\"}"}'

# List topics
curl http://YOUR_DROPLET_IP:8080/api/v1/topics
```

## Step 5: (Optional) Enable Monitoring

```bash
# Start Prometheus + Grafana
docker compose -f docker-compose.prod.yml --profile monitoring up -d

# Access Grafana at http://YOUR_DROPLET_IP:3001
# Default login: admin / admin (change in .env)
```

## Step 6: (Optional) Set Up a Domain + HTTPS

### Using Caddy (recommended - automatic HTTPS)

```bash
# Install Caddy
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install caddy

# Configure Caddy
sudo tee /etc/caddy/Caddyfile << 'CADDYFILE'
your-domain.com {
    # Web UI
    handle / {
        reverse_proxy localhost:3000
    }
    
    # API
    handle /api/* {
        reverse_proxy localhost:8080
    }
    
    # WebSocket
    handle /ws/* {
        reverse_proxy localhost:8080
    }
}
CADDYFILE

# Restart Caddy
sudo systemctl restart caddy
```

Update your `.env`:
```bash
PUBLIC_API_URL=https://your-domain.com
PUBLIC_WS_URL=wss://your-domain.com
```

## Troubleshooting

### Check logs

```bash
# All services
docker compose -f docker-compose.prod.yml logs

# Just StreamHouse server
docker compose -f docker-compose.prod.yml logs streamhouse

# Follow logs in real-time
docker compose -f docker-compose.prod.yml logs -f
```

### Database connection issues

```bash
# Test connection from droplet
psql "postgres://doadmin:PASSWORD@host:25060/streamhouse?sslmode=require"

# Make sure your droplet IP is in the database's trusted sources
# Go to Database → Settings → Trusted Sources → Add your droplet IP
```

### Spaces connection issues

```bash
# Test Spaces access
curl -I https://YOUR_BUCKET.nyc3.digitaloceanspaces.com

# Verify keys are correct
# Go to API → Spaces Keys and regenerate if needed
```

### Restart services

```bash
docker compose -f docker-compose.prod.yml restart
```

### Full rebuild

```bash
docker compose -f docker-compose.prod.yml down
docker compose -f docker-compose.prod.yml build --no-cache
docker compose -f docker-compose.prod.yml up -d
```

## Cost Breakdown

| Resource | Plan | Monthly Cost |
|----------|------|-------------|
| Droplet | Basic 4GB | $24 |
| Managed PostgreSQL | Basic 1GB | $15 |
| Spaces | 250GB included | $5 |
| **Total** | | **$44/month** |

For higher throughput, consider:
- Droplet: 8GB RAM ($48/mo)
- PostgreSQL: 2GB RAM ($30/mo)

## Next Steps

1. **Set up backups:** Enable automated backups for PostgreSQL in the DO dashboard
2. **Add monitoring:** Enable the monitoring profile for Prometheus/Grafana
3. **Configure alerts:** Set up alerting in Grafana or use DO's built-in monitoring
4. **Scale:** Add more Droplets and load balancing if needed
