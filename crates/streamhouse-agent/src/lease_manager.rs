//! Lease Manager - Partition Leadership Coordination
//!
//! The LeaseManager handles partition leadership using time-based leases with epoch fencing.
//! This prevents split-brain scenarios where two agents think they lead the same partition.
//!
//! ## How It Works
//!
//! 1. **Acquire Lease**: Before writing, check if we have valid lease
//!    - If no lease exists or expired → acquire it (epoch++)
//!    - If held by other agent → fail
//! 2. **Renew Lease**: Background task extends lease every 20s
//! 3. **Epoch Fencing**: Include epoch in all writes, reject stale epochs
//!
//! ## Lease Structure
//!
//! ```sql
//! CREATE TABLE partition_leases (
//!     topic VARCHAR(255),
//!     partition_id INT,
//!     agent_id VARCHAR(255),
//!     lease_epoch BIGINT,
//!     lease_expires_at BIGINT,  -- Absolute timestamp
//!     PRIMARY KEY (topic, partition_id)
//! );
//! ```
//!
//! ## Example
//!
//! ```rust,no_run
//! use streamhouse_agent::LeaseManager;
//!
//! # async fn example(manager: LeaseManager) -> Result<(), Box<dyn std::error::Error>> {
//! // Try to acquire lease before writing
//! let epoch = manager.ensure_lease("orders", 0).await?;
//!
//! // Write data with epoch (fencing)
//! // writer.append_with_epoch(record, epoch).await?;
//!
//! // Lease automatically renewed in background
//! # Ok(())
//! # }
//! ```
//!
//! ## Phase 4.2 Implementation
//!
//! Lease acquisition and renewal logic with epoch fencing.

use crate::error::{AgentError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use streamhouse_metadata::MetadataStore;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Default lease duration (30 seconds)
const DEFAULT_LEASE_DURATION_MS: i64 = 30_000;

/// Lease renewal interval (10 seconds = renew at 1/3 of lease duration)
const LEASE_RENEWAL_INTERVAL: Duration = Duration::from_secs(10);

/// Manages partition leases for this agent
pub struct LeaseManager {
    agent_id: String,
    metadata_store: Arc<dyn MetadataStore>,
    leases: Arc<RwLock<HashMap<(String, u32), CachedLease>>>,
    renewal_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

/// Cached lease information for a partition
#[derive(Debug, Clone)]
struct CachedLease {
    topic: String,
    partition_id: u32,
    epoch: u64,
    expires_at: i64,
}

impl LeaseManager {
    /// Create a new lease manager
    pub fn new(agent_id: String, metadata_store: Arc<dyn MetadataStore>) -> Self {
        Self {
            agent_id,
            metadata_store,
            leases: Arc::new(RwLock::new(HashMap::new())),
            renewal_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the lease renewal background task
    pub async fn start_renewal_task(&self) -> Result<()> {
        let agent_id = self.agent_id.clone();
        let metadata_store = Arc::clone(&self.metadata_store);
        let leases = Arc::clone(&self.leases);

        let task = LeaseRenewalTask {
            agent_id,
            metadata_store,
            leases,
            interval: LEASE_RENEWAL_INTERVAL,
        };

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        *self.renewal_handle.write().await = Some(handle);

        info!(
            agent_id = %self.agent_id,
            interval_seconds = LEASE_RENEWAL_INTERVAL.as_secs(),
            "Lease renewal task started"
        );

        Ok(())
    }

    /// Stop the lease renewal background task
    pub async fn stop_renewal_task(&self) -> Result<()> {
        let mut handle_guard = self.renewal_handle.write().await;

        if let Some(handle) = handle_guard.take() {
            handle.abort();
            let _ = handle.await;

            info!(
                agent_id = %self.agent_id,
                "Lease renewal task stopped"
            );
        }

        Ok(())
    }

    /// Ensure this agent has a valid lease for the partition
    ///
    /// Returns the current epoch for fencing.
    ///
    /// This will:
    /// 1. Check in-memory cache for valid lease
    /// 2. If missing/expired, acquire from metadata store
    /// 3. Return epoch for fencing
    pub async fn ensure_lease(&self, topic: &str, partition_id: u32) -> Result<u64> {
        // Check cache first
        let cached_epoch = {
            let leases = self.leases.read().await;
            leases
                .get(&(topic.to_string(), partition_id))
                .filter(|lease| !is_expired(lease.expires_at))
                .map(|lease| lease.epoch)
        };

        if let Some(epoch) = cached_epoch {
            debug!(
                agent_id = %self.agent_id,
                topic = %topic,
                partition_id = partition_id,
                epoch = epoch,
                "Using cached lease"
            );
            return Ok(epoch);
        }

        // Cache miss or expired - acquire new lease
        self.acquire_lease(topic, partition_id).await
    }

    /// Acquire a new lease for a partition
    async fn acquire_lease(&self, topic: &str, partition_id: u32) -> Result<u64> {
        debug!(
            agent_id = %self.agent_id,
            topic = %topic,
            partition_id = partition_id,
            "Acquiring partition lease"
        );

        let lease = self
            .metadata_store
            .acquire_partition_lease(
                topic,
                partition_id,
                &self.agent_id,
                DEFAULT_LEASE_DURATION_MS,
            )
            .await?;

        // Update cache
        {
            let mut leases = self.leases.write().await;
            leases.insert(
                (topic.to_string(), partition_id),
                CachedLease {
                    topic: topic.to_string(),
                    partition_id,
                    epoch: lease.epoch,
                    expires_at: lease.lease_expires_at,
                },
            );
        }

        info!(
            agent_id = %self.agent_id,
            topic = %topic,
            partition_id = partition_id,
            epoch = lease.epoch,
            expires_at = lease.lease_expires_at,
            "Acquired partition lease"
        );

        Ok(lease.epoch)
    }

    /// Release lease for a partition (called on shutdown)
    pub async fn release_lease(&self, topic: &str, partition_id: u32) -> Result<()> {
        // Remove from cache
        {
            let mut leases = self.leases.write().await;
            leases.remove(&(topic.to_string(), partition_id));
        }

        // Release in metadata store
        self.metadata_store
            .release_partition_lease(topic, partition_id, &self.agent_id)
            .await?;

        info!(
            agent_id = %self.agent_id,
            topic = %topic,
            partition_id = partition_id,
            "Released partition lease"
        );

        Ok(())
    }

    /// Release all leases (called on shutdown)
    pub async fn release_all_leases(&self) -> Result<()> {
        let leases_to_release: Vec<(String, u32)> = {
            let leases = self.leases.read().await;
            leases.keys().cloned().collect()
        };

        for (topic, partition_id) in leases_to_release {
            if let Err(e) = self.release_lease(&topic, partition_id).await {
                warn!(
                    agent_id = %self.agent_id,
                    topic = %topic,
                    partition_id = partition_id,
                    error = %e,
                    "Failed to release lease during shutdown"
                );
            }
        }

        Ok(())
    }

    /// Get current epoch for a partition (if we hold lease)
    pub async fn get_epoch(&self, topic: &str, partition_id: u32) -> Option<u64> {
        let leases = self.leases.read().await;
        leases
            .get(&(topic.to_string(), partition_id))
            .filter(|lease| !is_expired(lease.expires_at))
            .map(|lease| lease.epoch)
    }

    /// Get all active leases held by this agent
    pub async fn get_active_leases(&self) -> Vec<(String, u32, u64)> {
        let leases = self.leases.read().await;
        leases
            .values()
            .filter(|lease| !is_expired(lease.expires_at))
            .map(|lease| (lease.topic.clone(), lease.partition_id, lease.epoch))
            .collect()
    }

    /// Initiate a graceful lease transfer to another agent.
    ///
    /// This starts the handoff protocol:
    /// 1. Creates a pending transfer in metadata store
    /// 2. Waits for target agent to accept
    /// 3. Caller should flush pending writes before completing
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition_id` - Partition ID
    /// * `to_agent_id` - Target agent ID
    /// * `reason` - Reason for transfer
    /// * `timeout_ms` - Transfer timeout
    ///
    /// # Returns
    ///
    /// The transfer ID for tracking, or error if transfer cannot be initiated.
    pub async fn initiate_transfer(
        &self,
        topic: &str,
        partition_id: u32,
        to_agent_id: &str,
        reason: streamhouse_metadata::LeaderChangeReason,
        timeout_ms: u32,
    ) -> Result<String> {
        // Verify we hold the lease
        let _epoch = self.ensure_lease(topic, partition_id).await?;

        info!(
            agent_id = %self.agent_id,
            topic = %topic,
            partition_id = partition_id,
            to_agent_id = %to_agent_id,
            reason = ?reason,
            "Initiating lease transfer"
        );

        let transfer = self
            .metadata_store
            .initiate_lease_transfer(
                topic,
                partition_id,
                &self.agent_id,
                to_agent_id,
                reason,
                timeout_ms,
            )
            .await
            .map_err(AgentError::Metadata)?;

        info!(
            agent_id = %self.agent_id,
            transfer_id = %transfer.transfer_id,
            topic = %topic,
            partition_id = partition_id,
            to_agent_id = %to_agent_id,
            "Lease transfer initiated"
        );

        Ok(transfer.transfer_id)
    }

    /// Complete a graceful lease transfer after flushing data.
    ///
    /// This atomically transfers the lease to the target agent.
    /// Should only be called after:
    /// 1. Transfer has been accepted by target agent
    /// 2. All pending writes have been flushed to S3
    ///
    /// # Arguments
    ///
    /// * `transfer_id` - Transfer ID
    /// * `last_flushed_offset` - Last offset flushed to S3
    /// * `high_watermark` - Current high watermark
    pub async fn complete_transfer(
        &self,
        transfer_id: &str,
        last_flushed_offset: u64,
        high_watermark: u64,
    ) -> Result<()> {
        info!(
            agent_id = %self.agent_id,
            transfer_id = %transfer_id,
            last_flushed_offset = last_flushed_offset,
            high_watermark = high_watermark,
            "Completing lease transfer"
        );

        let lease = self
            .metadata_store
            .complete_lease_transfer(transfer_id, last_flushed_offset, high_watermark)
            .await
            .map_err(AgentError::Metadata)?;

        // Remove from our cache since we no longer hold it
        {
            let mut leases = self.leases.write().await;
            leases.remove(&(lease.topic.clone(), lease.partition_id));
        }

        info!(
            agent_id = %self.agent_id,
            transfer_id = %transfer_id,
            new_leader = %lease.leader_agent_id,
            new_epoch = lease.epoch,
            "Lease transfer completed"
        );

        Ok(())
    }

    /// Accept a pending lease transfer from another agent.
    ///
    /// Called when this agent is the target of a transfer.
    ///
    /// # Arguments
    ///
    /// * `transfer_id` - Transfer ID
    pub async fn accept_transfer(&self, transfer_id: &str) -> Result<()> {
        info!(
            agent_id = %self.agent_id,
            transfer_id = %transfer_id,
            "Accepting lease transfer"
        );

        self.metadata_store
            .accept_lease_transfer(transfer_id, &self.agent_id)
            .await
            .map_err(AgentError::Metadata)?;

        info!(
            agent_id = %self.agent_id,
            transfer_id = %transfer_id,
            "Lease transfer accepted"
        );

        Ok(())
    }

    /// Reject a pending lease transfer.
    ///
    /// # Arguments
    ///
    /// * `transfer_id` - Transfer ID
    /// * `reason` - Reason for rejection
    pub async fn reject_transfer(&self, transfer_id: &str, reason: &str) -> Result<()> {
        warn!(
            agent_id = %self.agent_id,
            transfer_id = %transfer_id,
            reason = %reason,
            "Rejecting lease transfer"
        );

        self.metadata_store
            .reject_lease_transfer(transfer_id, &self.agent_id, reason)
            .await
            .map_err(AgentError::Metadata)?;

        Ok(())
    }

    /// Get pending transfers where this agent is the target.
    ///
    /// Used to check for incoming transfer requests.
    pub async fn get_incoming_transfers(&self) -> Result<Vec<streamhouse_metadata::LeaseTransfer>> {
        let transfers = self
            .metadata_store
            .get_pending_transfers_for_agent(&self.agent_id)
            .await
            .map_err(AgentError::Metadata)?;

        Ok(transfers
            .into_iter()
            .filter(|t| t.to_agent_id == self.agent_id)
            .collect())
    }

    /// Gracefully transfer all leases before shutdown.
    ///
    /// Attempts to transfer each lease to an available agent.
    /// Falls back to simple release if no agents are available.
    ///
    /// # Arguments
    ///
    /// * `available_agents` - List of agent IDs that can accept transfers
    /// * `timeout_ms` - Timeout for each transfer
    pub async fn transfer_all_leases(
        &self,
        available_agents: &[String],
        timeout_ms: u32,
    ) -> Result<()> {
        let leases: Vec<(String, u32)> = {
            let leases = self.leases.read().await;
            leases.keys().cloned().collect()
        };

        if leases.is_empty() {
            debug!(
                agent_id = %self.agent_id,
                "No leases to transfer"
            );
            return Ok(());
        }

        info!(
            agent_id = %self.agent_id,
            lease_count = leases.len(),
            available_agents = ?available_agents,
            "Initiating graceful transfer of all leases"
        );

        // If no available agents, fall back to release
        if available_agents.is_empty() {
            warn!(
                agent_id = %self.agent_id,
                "No available agents for transfer, falling back to lease release"
            );
            return self.release_all_leases().await;
        }

        // Round-robin distribute leases among available agents
        let mut agent_idx = 0;
        let mut transfers_initiated = 0;
        let mut transfers_failed = 0;

        for (topic, partition_id) in leases {
            let target_agent = &available_agents[agent_idx % available_agents.len()];
            agent_idx += 1;

            match self
                .initiate_transfer(
                    &topic,
                    partition_id,
                    target_agent,
                    streamhouse_metadata::LeaderChangeReason::GracefulHandoff,
                    timeout_ms,
                )
                .await
            {
                Ok(_transfer_id) => {
                    transfers_initiated += 1;
                    // Note: In a real implementation, we would wait for acceptance
                    // and then complete the transfer after flushing. For now,
                    // this just initiates the transfer.
                }
                Err(e) => {
                    warn!(
                        agent_id = %self.agent_id,
                        topic = %topic,
                        partition_id = partition_id,
                        error = %e,
                        "Failed to initiate transfer, falling back to release"
                    );
                    transfers_failed += 1;

                    // Fall back to simple release
                    if let Err(e) = self.release_lease(&topic, partition_id).await {
                        error!(
                            agent_id = %self.agent_id,
                            topic = %topic,
                            partition_id = partition_id,
                            error = %e,
                            "Failed to release lease during graceful shutdown"
                        );
                    }
                }
            }
        }

        info!(
            agent_id = %self.agent_id,
            transfers_initiated = transfers_initiated,
            transfers_failed = transfers_failed,
            "Completed lease transfer initiation"
        );

        Ok(())
    }
}

/// Background task that renews leases
struct LeaseRenewalTask {
    agent_id: String,
    metadata_store: Arc<dyn MetadataStore>,
    leases: Arc<RwLock<HashMap<(String, u32), CachedLease>>>,
    interval: Duration,
}

impl LeaseRenewalTask {
    async fn run(self) {
        info!(
            agent_id = %self.agent_id,
            interval_seconds = self.interval.as_secs(),
            "Lease renewal task started"
        );

        let mut renewal_count: u64 = 0;
        let mut failure_count: u64 = 0;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.interval) => {}
                _ = tokio::signal::ctrl_c() => {
                    info!(
                        agent_id = %self.agent_id,
                        "Lease renewal task received shutdown signal"
                    );
                    break;
                }
            }

            // Get list of leases to renew
            let leases_to_renew: Vec<(String, u32)> = {
                let leases = self.leases.read().await;
                leases
                    .iter()
                    .filter(|(_, lease)| !is_expired(lease.expires_at))
                    .map(|(key, _)| key.clone())
                    .collect()
            };

            if leases_to_renew.is_empty() {
                debug!(
                    agent_id = %self.agent_id,
                    "No active leases to renew"
                );
                continue;
            }

            debug!(
                agent_id = %self.agent_id,
                lease_count = leases_to_renew.len(),
                "Renewing partition leases"
            );

            // Renew each lease
            for (topic, partition_id) in leases_to_renew {
                match self.renew_lease(&topic, partition_id).await {
                    Ok(_) => {
                        renewal_count += 1;
                        debug!(
                            agent_id = %self.agent_id,
                            topic = %topic,
                            partition_id = partition_id,
                            total_renewals = renewal_count,
                            "Lease renewed"
                        );
                    }
                    Err(e) => {
                        failure_count += 1;
                        error!(
                            agent_id = %self.agent_id,
                            topic = %topic,
                            partition_id = partition_id,
                            error = %e,
                            failure_count,
                            "Lease renewal failed"
                        );

                        // Remove from cache if renewal failed
                        let mut leases = self.leases.write().await;
                        leases.remove(&(topic, partition_id));
                    }
                }
            }
        }

        info!(
            agent_id = %self.agent_id,
            total_renewals = renewal_count,
            total_failures = failure_count,
            "Lease renewal task stopped"
        );
    }

    async fn renew_lease(&self, topic: &str, partition_id: u32) -> Result<()> {
        let lease = self
            .metadata_store
            .acquire_partition_lease(
                topic,
                partition_id,
                &self.agent_id,
                DEFAULT_LEASE_DURATION_MS,
            )
            .await?;

        // Update cache with new expiration
        let mut leases = self.leases.write().await;
        leases.insert(
            (topic.to_string(), partition_id),
            CachedLease {
                topic: topic.to_string(),
                partition_id,
                epoch: lease.epoch,
                expires_at: lease.lease_expires_at,
            },
        );

        Ok(())
    }
}

/// Check if a lease has expired
fn is_expired(expires_at: i64) -> bool {
    let now = current_timestamp_ms();
    now >= expires_at
}

/// Get current timestamp in milliseconds since epoch
fn current_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_millis() as i64
}

/// Validate epoch for fencing (prevents split-brain writes)
///
/// This should be called before any write operation to ensure the epoch
/// hasn't changed (i.e., we still hold the lease).
///
/// # Example
///
/// ```ignore
/// let epoch = lease_manager.ensure_lease("orders", 0).await?;
///
/// // ... prepare write ...
///
/// // Before writing, validate epoch hasn't changed
/// validate_epoch(&lease_manager, "orders", 0, epoch).await?;
///
/// // Safe to write - we still hold the lease
/// writer.append(record).await?;
/// ```
pub async fn validate_epoch(
    manager: &LeaseManager,
    topic: &str,
    partition_id: u32,
    expected_epoch: u64,
) -> Result<()> {
    match manager.get_epoch(topic, partition_id).await {
        Some(current_epoch) if current_epoch == expected_epoch => Ok(()),
        Some(current_epoch) => Err(AgentError::StaleEpoch {
            expected: expected_epoch,
            actual: current_epoch,
        }),
        None => Err(AgentError::LeaseExpired {
            topic: topic.to_string(),
            partition: partition_id,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use streamhouse_metadata::SqliteMetadataStore;

    #[tokio::test]
    async fn test_lease_manager_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_lease_mgr.db");

        let metadata = SqliteMetadataStore::new(db_path.to_str().unwrap())
            .await
            .unwrap();
        let metadata = Arc::new(metadata) as Arc<dyn MetadataStore>;

        let manager = LeaseManager::new("test-agent".to_string(), metadata);
        assert_eq!(manager.agent_id, "test-agent");
    }
}
