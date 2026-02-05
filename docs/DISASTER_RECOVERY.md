# StreamHouse Disaster Recovery Guide

This guide explains all disaster recovery features in StreamHouse and how they work together to ensure data durability, high availability, and rapid recovery from failures.

## Overview

StreamHouse provides comprehensive disaster recovery capabilities through several integrated systems:

| Component | Purpose | RPO | RTO |
|-----------|---------|-----|-----|
| Point-in-Time Recovery (PITR) | Recover to any timestamp | Near-zero | Minutes |
| Leader Election | Coordinate cluster leadership | N/A | Seconds |
| Automatic Failover | Handle node failures | Zero | < 30 seconds |
| Audit Trail | Immutable change history | Zero | Instant |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    StreamHouse Cluster                           │
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │   Node 1     │    │   Node 2     │    │   Node 3     │       │
│  │  (Leader)    │←──→│  (Follower)  │←──→│  (Follower)  │       │
│  └──────┬───────┘    └──────────────┘    └──────────────┘       │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Write-Ahead Log (WAL)                   │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐      │   │
│  │  │ Segment │→ │ Segment │→ │ Segment │→ │ Active  │      │   │
│  │  │   001   │  │   002   │  │   003   │  │ Segment │      │   │
│  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘      │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Base Backups (S3)                       │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐                │   │
│  │  │ Backup 1 │  │ Backup 2 │  │ Backup 3 │  ...           │   │
│  │  │ T-24h    │  │ T-12h    │  │ T-1h     │                │   │
│  │  └──────────┘  └──────────┘  └──────────┘                │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Point-in-Time Recovery (PITR)

PITR enables recovery to any specific timestamp by combining continuous WAL archiving with periodic base backups.

### How It Works

1. **Write-Ahead Logging (WAL)**: Every metadata change is recorded to a tamper-proof WAL before being applied
2. **Base Backups**: Full snapshots of system state are taken at configurable intervals
3. **Recovery**: To recover, we restore from the nearest base backup and replay WAL entries

### WAL Operations

The WAL captures all metadata operations:

```rust
pub enum WalOperation {
    TopicCreated { topic_id, topic_name, partitions, replication_factor, config },
    TopicDeleted { topic_id, topic_name },
    TopicConfigUpdated { topic_id, old_config, new_config },
    ConsumerGroupCreated { group_id, group_name },
    ConsumerGroupOffsetCommitted { group_id, topic, partition, offset },
    ConsumerGroupDeleted { group_id },
    AclCreated { resource_type, resource_name, principal, permission },
    AclDeleted { resource_type, resource_name, principal },
    UserCreated { user_id, username },
    UserDeleted { user_id },
    SchemaRegistered { subject, version, schema_id },
    Checkpoint { backup_id },
    Custom { operation_type, data },
}
```

### WAL Chain Verification

Each WAL entry includes a SHA-256 hash of its contents and the previous entry's hash, creating a cryptographic chain that detects tampering:

```rust
pub struct WalEntry {
    pub lsn: u64,           // Log Sequence Number
    pub timestamp: DateTime<Utc>,
    pub operation: WalOperation,
    pub prev_hash: String,  // Hash of previous entry
    pub hash: String,       // Hash of this entry
}
```

### Configuration

```rust
let config = PitrConfig::builder()
    .wal_directory("./pitr/wal")
    .backup_directory("./pitr/backups")
    .base_backup_interval(Duration::from_secs(3600))  // Hourly backups
    .wal_retention(Duration::from_secs(86400 * 7))    // 7-day retention
    .wal_segment_size(16 * 1024 * 1024)               // 16MB segments
    .backup_retention_count(24)                        // Keep 24 backups
    .compress_backups(true)
    .verify_wal(true)
    .build()?;

let pitr = PitrManager::new(config).await?;
```

### Usage Examples

**Logging Changes:**

```rust
// Log a topic creation
pitr.log_change(WalOperation::TopicCreated {
    topic_id: "topic-123".to_string(),
    topic_name: "orders".to_string(),
    partitions: 6,
    replication_factor: 3,
    config: serde_json::json!({ "retention.ms": 604800000 }),
}).await?;
```

**Creating a Base Backup:**

```rust
let snapshot = get_current_system_state().await;
let backup = pitr.create_base_backup(snapshot).await?;
println!("Backup {} created at LSN {}", backup.id, backup.start_lsn);
```

**Recovery to a Specific Time:**

```rust
// Recover to 1 hour ago
let target_time = Utc::now() - Duration::from_secs(3600);
let result = pitr.recover(RecoveryTarget::Timestamp(target_time)).await?;

println!("Recovered to {} with {} WAL entries replayed",
    result.snapshot.timestamp,
    result.wal_entries_replayed);
```

**Recovery to a Specific LSN:**

```rust
let result = pitr.recover(RecoveryTarget::Lsn(12345)).await?;
```

**Recovery to Latest:**

```rust
let result = pitr.recover(RecoveryTarget::Latest).await?;
```

### Monitoring

```rust
// Get WAL statistics
let stats = pitr.wal_stats().await;
println!("Current LSN: {}", stats.current_lsn);
println!("Active entries: {}", stats.active_segment_entries);
println!("Archived segments: {}", stats.archived_segment_count);

// Verify WAL integrity
let verification = pitr.verify_wal_integrity(0).await?;
if verification.is_valid {
    println!("WAL chain verified: {} entries", verification.verified_entries);
} else {
    eprintln!("WAL corruption detected: {:?}", verification.errors);
}

// List available backups
let backups = pitr.list_backups().await;
for backup in backups {
    println!("Backup {} at {}: {} bytes",
        backup.id, backup.created_at, backup.size_bytes);
}
```

## Leader Election

Leader election ensures only one node coordinates writes at any time, preventing split-brain scenarios.

### How It Works

1. Nodes compete for leadership by acquiring a lease
2. The leader must renew the lease periodically
3. If renewal fails, other nodes can become leader
4. Fencing tokens prevent stale leaders from causing issues

### Fencing Tokens

Each leadership term has a monotonically increasing fencing token. Requests with stale tokens are rejected:

```
Time ─────────────────────────────────────────────────►

Node A: ████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░
        Token: 1         ^
                         │ Loses lease
                         │
Node B: ░░░░░░░░░░░░░░░░████████████████████████████
                         Token: 2

Any request from Node A with Token 1 after this point
is rejected because Token 2 is now active.
```

### Configuration

```rust
// Memory backend (for testing)
let config = LeaderConfig::memory()
    .namespace("my-cluster")
    .node_id("node-1")
    .lease_duration(Duration::from_secs(30))
    .renew_interval(Duration::from_secs(10));

// PostgreSQL backend (for production)
let config = LeaderConfig::postgres("postgres://localhost/streamhouse")
    .namespace("my-cluster")
    .node_id("node-1")
    .lease_duration(Duration::from_secs(30))
    .renew_interval(Duration::from_secs(10));

let election = LeaderElection::new(config).await?;
```

### Usage Examples

**Acquiring Leadership:**

```rust
// Start leadership campaign (non-blocking)
election.start().await;

// Check if we're the leader
if election.is_leader().await {
    // Perform leader-only operations
    perform_cluster_coordination().await;
}
```

**With Leadership Guard:**

```rust
// Acquire leadership and hold it
if let Ok(guard) = election.acquire_leadership().await {
    // Leadership is held while guard is in scope
    let token = guard.fencing_token();

    // Use token in distributed operations
    distributed_write(data, token).await;

    // Leadership released when guard is dropped
}
```

**Subscribing to Events:**

```rust
let mut events = election.subscribe().await;
while let Ok(event) = events.recv().await {
    match event {
        LeaderEvent::Acquired { node_id, fencing_token } => {
            println!("Leadership acquired by {} with token {}", node_id, fencing_token);
        }
        LeaderEvent::Lost { reason } => {
            println!("Leadership lost: {:?}", reason);
        }
        LeaderEvent::Renewed { fencing_token } => {
            // Leadership renewed
        }
    }
}
```

## Automatic Failover

Automatic failover detects node failures and promotes a healthy follower to leader.

### How It Works

1. **Health Monitoring**: Continuous health checks on all nodes
2. **Failure Detection**: Consecutive failures trigger unhealthy status
3. **Failover Decision**: Based on configurable policies
4. **Leader Promotion**: Best candidate is promoted to leader

### Failover Policies

| Policy | Behavior | Use Case |
|--------|----------|----------|
| `Immediate` | Failover as soon as unhealthy detected | Maximum availability |
| `Graceful { grace_period }` | Wait for grace period before failover | Avoid flapping |
| `Manual` | Never auto-failover, require operator | Critical workloads |

### Configuration

```rust
let config = FailoverConfig::builder()
    .health_check_interval(Duration::from_secs(5))
    .health_check_timeout(Duration::from_secs(10))
    .unhealthy_threshold(3)      // 3 consecutive failures = unhealthy
    .healthy_threshold(2)        // 2 consecutive successes = healthy
    .policy(FailoverPolicy::Graceful {
        grace_period: Duration::from_secs(30),
    })
    .build()?;

let failover = FailoverManager::new(config, leader_election).await;
```

### Health Check Implementation

```rust
// HTTP health checker
let checker = HttpHealthChecker::new("http://node:8080/health");

// Custom health checker
struct MyHealthChecker;

#[async_trait]
impl HealthChecker for MyHealthChecker {
    async fn check(&self) -> HealthCheckResult {
        // Custom health logic
        if is_healthy() {
            HealthCheckResult::healthy()
        } else {
            HealthCheckResult::unhealthy("Database connection failed")
        }
    }
}
```

### Subscribing to Failover Events

```rust
let mut events = failover.subscribe().await;
while let Ok(event) = events.recv().await {
    match event {
        FailoverEvent::NodeUnhealthy { node_id, reason } => {
            alert_ops_team(&format!("Node {} unhealthy: {}", node_id, reason));
        }
        FailoverEvent::FailoverStarted { from, to } => {
            log::warn!("Failover started: {} -> {}", from, to);
        }
        FailoverEvent::FailoverCompleted { new_leader } => {
            log::info!("Failover completed: new leader is {}", new_leader);
        }
        FailoverEvent::FailoverFailed { reason } => {
            alert_ops_team(&format!("Failover failed: {}", reason));
        }
    }
}
```

### Monitoring

```rust
let stats = failover.stats().await;
println!("Total failovers: {}", stats.total_failovers);
println!("Successful: {}", stats.successful_failovers);
println!("Failed: {}", stats.failed_failovers);
println!("Last failover: {:?}", stats.last_failover_at);
```

## Immutable Audit Trail

The audit trail provides a tamper-proof record of all system changes for compliance and forensics.

### How It Works

1. Every significant event is logged with context
2. Entries are hashed into a chain (like blockchain)
3. Chain integrity can be verified at any time
4. Supports SOC2, GDPR, HIPAA, PCI-DSS compliance reports

### Audit Entry Structure

```rust
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub actor: String,           // Who performed the action
    pub resource: String,        // What was affected
    pub action: String,          // What was done
    pub details: serde_json::Value,  // Additional context
    pub source_ip: Option<String>,
    pub request_id: Option<String>,
}
```

### Event Types

```rust
pub enum AuditEventType {
    // Authentication events
    AuthSuccess,
    AuthFailure,
    TokenIssued,
    TokenRevoked,

    // Resource events
    ResourceCreated,
    ResourceModified,
    ResourceDeleted,
    ResourceAccessed,

    // Permission events
    PermissionGranted,
    PermissionRevoked,

    // System events
    ConfigChanged,
    SystemStarted,
    SystemStopped,
}
```

### Usage

```rust
let audit_store = AuditStore::new(AuditStoreConfig::memory()).await?;

// Log an event
log_audit_event(
    &audit_store,
    AuditEventType::ResourceCreated,
    "user:alice",
    "topic:orders",
    "create",
    serde_json::json!({
        "partitions": 6,
        "replication_factor": 3
    }),
)?;

// Query audit history
let query = AuditQuery::builder()
    .actor("user:alice")
    .start_time(Utc::now() - Duration::from_secs(86400))
    .build();

let entries = audit_store.query(query).await?;
```

### Chain Verification

```rust
let result = audit_store.verify_chain().await?;
match result {
    VerificationResult::Valid { entries_verified } => {
        println!("Audit chain valid: {} entries verified", entries_verified);
    }
    VerificationResult::Invalid { entry_index, reason } => {
        eprintln!("Audit chain corrupted at entry {}: {}", entry_index, reason);
    }
}
```

## Compliance Reporting

Generate compliance reports for various regulatory frameworks.

### Supported Frameworks

- **SOC2**: Service Organization Control 2
- **GDPR**: General Data Protection Regulation
- **HIPAA**: Health Insurance Portability and Accountability Act
- **PCI-DSS**: Payment Card Industry Data Security Standard

### Generating Reports

```rust
let reporter = ComplianceReporter::new(audit_store);

let config = ReportConfig::builder()
    .framework(Framework::SOC2)
    .start_date(Utc::now() - Duration::from_secs(86400 * 30))  // Last 30 days
    .format(ReportFormat::Json)
    .build();

let report = reporter.generate_report(config).await?;

// Access findings
for finding in &report.findings {
    match finding.severity {
        FindingSeverity::Critical | FindingSeverity::High => {
            alert_security_team(&finding);
        }
        _ => {}
    }
}

// Export report
let json = reporter.export(&report, ReportFormat::Json)?;
let csv = reporter.export(&report, ReportFormat::Csv)?;
let html = reporter.export(&report, ReportFormat::Html)?;
```

## Recovery Procedures

### Scenario 1: Single Node Failure

**Symptoms:** One node becomes unresponsive

**Automatic Response:**
1. Health checker detects failure (after `unhealthy_threshold` checks)
2. Node marked as unhealthy
3. If leader, failover triggered based on policy
4. Follower promoted to leader

**Manual Verification:**
```bash
# Check cluster status
curl http://cluster:8080/api/v1/health

# View failover events
curl http://cluster:8080/api/v1/admin/failover/events
```

### Scenario 2: Data Corruption

**Symptoms:** Inconsistent reads, verification failures

**Recovery Steps:**

1. **Identify Corruption Point:**
```rust
let verification = pitr.verify_wal_integrity(0).await?;
if !verification.is_valid {
    let corrupt_lsn = verification.errors[0].parse::<u64>()?;
}
```

2. **Recover to Last Good State:**
```rust
let last_good_lsn = corrupt_lsn - 1;
let result = pitr.recover(RecoveryTarget::Lsn(last_good_lsn)).await?;
apply_snapshot(result.snapshot).await?;
```

3. **Notify Operations:**
```rust
alert_ops_team("Data recovered to LSN {}, {} entries lost",
    last_good_lsn, current_lsn - last_good_lsn);
```

### Scenario 3: Complete Cluster Loss

**Symptoms:** All nodes unavailable

**Recovery Steps:**

1. **Provision New Infrastructure:**
```bash
# Deploy new cluster
terraform apply -var="cluster_size=3"
```

2. **Restore from Latest Backup:**
```rust
// List available backups (from S3)
let backups = list_s3_backups("s3://streamhouse-backups/").await?;
let latest = backups.iter().max_by_key(|b| b.created_at)?;

// Download and restore
let snapshot = download_backup(&latest.id).await?;
let pitr = PitrManager::new(config).await?;
let result = pitr.recover(RecoveryTarget::Latest).await?;
apply_snapshot(result.snapshot).await?;
```

3. **Verify Recovery:**
```rust
// Verify data integrity
let verification = pitr.verify_wal_integrity(0).await?;
assert!(verification.is_valid);

// Verify topics exist
let topics = metadata.list_topics().await?;
println!("Recovered {} topics", topics.len());
```

### Scenario 4: Point-in-Time Recovery

**Symptoms:** Need to recover to specific timestamp (e.g., before bad deployment)

**Recovery Steps:**

```rust
// 1. Identify target timestamp (e.g., before deployment at 2024-01-15 14:00 UTC)
let target = DateTime::parse_from_rfc3339("2024-01-15T13:59:00Z")?.with_timezone(&Utc);

// 2. Find the backup to use
let backups = pitr.list_backups().await;
let backup = backups.iter()
    .filter(|b| b.created_at < target)
    .max_by_key(|b| b.created_at)?;

println!("Using backup {} from {}", backup.id, backup.created_at);

// 3. Perform recovery
let result = pitr.recover(RecoveryTarget::Timestamp(target)).await?;

println!("Recovered to {} (LSN {})",
    result.snapshot.timestamp,
    result.snapshot.lsn);
println!("Replayed {} WAL entries", result.wal_entries_replayed);

// 4. Apply recovered state
apply_snapshot(result.snapshot).await?;
```

## Best Practices

### Backup Strategy

1. **Backup Frequency**: Set `base_backup_interval` based on your RPO
   - Critical data: Every 15-60 minutes
   - Standard data: Every 1-6 hours

2. **Retention Policy**: Balance storage costs with recovery needs
   - Keep at least 24 hourly backups
   - Keep at least 7 daily backups
   - Keep at least 4 weekly backups

3. **Cross-Region Replication**: Store backups in multiple regions
   ```rust
   let config = PitrConfig::builder()
       .backup_directory("s3://primary-region/backups")
       .build()?;

   // Set up cross-region replication in S3
   ```

### Monitoring Recommendations

1. **WAL Lag**: Alert if WAL backup is more than 5 minutes behind
2. **Backup Age**: Alert if no successful backup in last 2 hours
3. **Chain Validity**: Run integrity check every hour
4. **Failover Events**: Alert on any failover occurrence

### Testing Recovery

1. **Regular Drills**: Test recovery procedures monthly
2. **Automated Verification**:
   ```rust
   // Daily recovery test
   let result = pitr.recover(RecoveryTarget::Latest).await?;
   assert!(result.snapshot.topics.len() > 0);
   ```
3. **Document Results**: Keep records of recovery test outcomes

### Security Considerations

1. **Encrypt Backups**: Use S3 server-side encryption
2. **Access Control**: Restrict backup access to operators
3. **Audit Access**: Log all backup/restore operations
4. **Verify Integrity**: Check chain integrity before critical operations

## Troubleshooting

### WAL Verification Fails

```
Error: WAL entry 12345 chain broken
```

**Cause**: Gap in WAL sequence or corruption

**Resolution**:
1. Check for missing WAL segments
2. Restore from last known good backup
3. Replay available WAL entries

### Backup Creation Fails

```
Error: Backup error: Failed to serialize snapshot
```

**Cause**: Snapshot too large or serialization issue

**Resolution**:
1. Check available memory
2. Verify all data types are serializable
3. Consider incremental backups for large datasets

### Failover Not Triggering

**Cause**: Policy may be set to `Manual`, or threshold not reached

**Resolution**:
1. Check failover policy configuration
2. Verify health check is running
3. Check `unhealthy_threshold` setting
4. Review health check logs

### Leader Election Stuck

**Cause**: All nodes may be trying to acquire simultaneously

**Resolution**:
1. Check network connectivity between nodes
2. Verify lease backend (PostgreSQL) is accessible
3. Check for clock skew between nodes
4. Review leader election logs

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PITR_WAL_DIR` | WAL storage directory | `./pitr/wal` |
| `PITR_BACKUP_DIR` | Backup storage directory | `./pitr/backups` |
| `PITR_BACKUP_INTERVAL_SECS` | Backup frequency | `3600` |
| `PITR_WAL_RETENTION_SECS` | WAL retention period | `604800` |
| `LEADER_LEASE_DURATION_SECS` | Leadership lease duration | `30` |
| `LEADER_RENEW_INTERVAL_SECS` | Lease renewal interval | `10` |
| `FAILOVER_HEALTH_CHECK_INTERVAL_SECS` | Health check frequency | `5` |
| `FAILOVER_UNHEALTHY_THRESHOLD` | Failures before unhealthy | `3` |
| `FAILOVER_POLICY` | Failover policy (`immediate`, `graceful`, `manual`) | `graceful` |

## Related Documentation

- [Authentication Guide](./AUTHENTICATION.md) - Security configuration
- [API Reference](./API_REFERENCE.md) - REST API documentation
- [Operations Guide](./OPERATIONS.md) - Day-to-day operations
- [Architecture Overview](./ARCHITECTURE.md) - System design
