//! Full Two-Phase Commit (2PC) Transaction Coordinator
//!
//! Implements the complete 2PC protocol for distributed transactions in StreamHouse.
//! The coordinator manages transaction lifecycle through the states:
//! Ongoing -> Preparing -> Committed/Aborted
//!
//! ## Protocol Overview
//!
//! 1. **Begin**: Producer starts a transaction, gets a unique transaction ID
//! 2. **Add Partitions**: As records are written, partitions are enrolled
//! 3. **Prepare**: Coordinator enters the "prepare" phase, asking all participants
//! 4. **Commit/Abort**: Based on prepare results, the transaction is committed or aborted
//!
//! ## Timeout Handling
//!
//! Transactions that exceed their timeout are automatically moved to `TimedOut` state
//! during periodic `check_timeouts()` calls.
//!
//! ## Recovery
//!
//! Transactions in `Preparing` state during a crash are recovered and either
//! committed or aborted based on the recovery policy.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Errors specific to the transaction coordinator.
#[derive(Debug, Error)]
pub enum TransactionCoordinatorError {
    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Invalid state transition: {0} -> {1}")]
    InvalidStateTransition(String, String),

    #[error("Transaction timed out: {0}")]
    TransactionTimedOut(String),

    #[error("Duplicate partition: {topic}/{partition_id} already in transaction {transaction_id}")]
    DuplicatePartition {
        transaction_id: String,
        topic: String,
        partition_id: u32,
    },

    #[error("Producer mismatch: expected {expected}, got {actual}")]
    ProducerMismatch { expected: String, actual: String },

    #[error("Prepare failed: {0}")]
    PrepareFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, TransactionCoordinatorError>;

/// State of a transaction in the 2PC protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorTransactionState {
    /// Transaction is active, records are being added.
    Ongoing,
    /// Prepare phase: coordinator is asking participants to vote.
    Preparing,
    /// Transaction has been committed by all participants.
    Committed,
    /// Transaction has been aborted.
    Aborted,
    /// Transaction exceeded its timeout.
    TimedOut,
}

impl std::fmt::Display for CoordinatorTransactionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorTransactionState::Ongoing => write!(f, "ongoing"),
            CoordinatorTransactionState::Preparing => write!(f, "preparing"),
            CoordinatorTransactionState::Committed => write!(f, "committed"),
            CoordinatorTransactionState::Aborted => write!(f, "aborted"),
            CoordinatorTransactionState::TimedOut => write!(f, "timed_out"),
        }
    }
}

/// A partition participating in a transaction.
#[derive(Debug, Clone)]
pub struct TransactionPartition {
    pub topic: String,
    pub partition_id: u32,
    pub first_offset: u64,
    pub last_offset: u64,
}

/// Full record of a transaction managed by the coordinator.
#[derive(Debug, Clone)]
pub struct TransactionRecord {
    pub id: String,
    pub producer_id: String,
    pub state: CoordinatorTransactionState,
    pub partitions: Vec<TransactionPartition>,
    pub started_at: Instant,
    pub timeout: Duration,
    pub prepare_timestamp: Option<Instant>,
}

impl TransactionRecord {
    /// Check if the transaction has exceeded its timeout.
    pub fn is_timed_out(&self) -> bool {
        self.started_at.elapsed() > self.timeout
    }

    /// Check if the prepare phase has timed out (using half the transaction timeout).
    pub fn is_prepare_timed_out(&self) -> bool {
        if let Some(prepare_ts) = self.prepare_timestamp {
            prepare_ts.elapsed() > self.timeout / 2
        } else {
            false
        }
    }
}

/// Summary statistics for the transaction coordinator.
#[derive(Debug, Clone, Default)]
pub struct TransactionStats {
    pub active_transactions: usize,
    pub preparing_transactions: usize,
    pub committed_transactions: usize,
    pub aborted_transactions: usize,
    pub timed_out_transactions: usize,
    pub total_partitions_enrolled: usize,
}

/// The Two-Phase Commit Transaction Coordinator.
///
/// Manages the lifecycle of distributed transactions, ensuring atomicity
/// across multiple partitions and topics.
pub struct TransactionCoordinator {
    transactions: RwLock<HashMap<String, TransactionRecord>>,
    timeout: Duration,
}

impl TransactionCoordinator {
    /// Create a new TransactionCoordinator with the specified default timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            transactions: RwLock::new(HashMap::new()),
            timeout,
        }
    }

    /// Create a new TransactionCoordinator wrapped in an Arc for shared ownership.
    pub fn new_shared(timeout: Duration) -> Arc<Self> {
        Arc::new(Self::new(timeout))
    }

    /// Begin a new transaction for the given producer.
    ///
    /// Returns the transaction ID assigned to this transaction.
    pub async fn begin(&self, transaction_id: String, producer_id: String) -> Result<String> {
        let mut txns = self.transactions.write().await;

        if txns.contains_key(&transaction_id) {
            return Err(TransactionCoordinatorError::InvalidStateTransition(
                "new".to_string(),
                "already exists".to_string(),
            ));
        }

        let record = TransactionRecord {
            id: transaction_id.clone(),
            producer_id,
            state: CoordinatorTransactionState::Ongoing,
            partitions: Vec::new(),
            started_at: Instant::now(),
            timeout: self.timeout,
            prepare_timestamp: None,
        };

        txns.insert(transaction_id.clone(), record);
        debug!("Transaction {} started", transaction_id);
        Ok(transaction_id)
    }

    /// Add a partition to an ongoing transaction.
    ///
    /// The partition records the offset range of data written as part of this transaction.
    pub async fn add_partition(
        &self,
        transaction_id: &str,
        topic: String,
        partition_id: u32,
        first_offset: u64,
        last_offset: u64,
    ) -> Result<()> {
        let mut txns = self.transactions.write().await;

        let record = txns
            .get_mut(transaction_id)
            .ok_or_else(|| {
                TransactionCoordinatorError::TransactionNotFound(transaction_id.to_string())
            })?;

        if record.state != CoordinatorTransactionState::Ongoing {
            return Err(TransactionCoordinatorError::InvalidStateTransition(
                record.state.to_string(),
                "add_partition (requires ongoing)".to_string(),
            ));
        }

        // Check for duplicate partition enrollment
        let already_enrolled = record
            .partitions
            .iter()
            .any(|p| p.topic == topic && p.partition_id == partition_id);

        if already_enrolled {
            return Err(TransactionCoordinatorError::DuplicatePartition {
                transaction_id: transaction_id.to_string(),
                topic,
                partition_id,
            });
        }

        record.partitions.push(TransactionPartition {
            topic: topic.clone(),
            partition_id,
            first_offset,
            last_offset,
        });

        debug!(
            "Transaction {} enrolled partition {}/{}",
            transaction_id, topic, partition_id
        );
        Ok(())
    }

    /// Enter the prepare phase of the 2PC protocol.
    ///
    /// Transitions the transaction from Ongoing to Preparing.
    /// In the prepare phase, the coordinator asks all participants
    /// to vote on whether they can commit.
    pub async fn prepare(&self, transaction_id: &str) -> Result<()> {
        let mut txns = self.transactions.write().await;

        let record = txns
            .get_mut(transaction_id)
            .ok_or_else(|| {
                TransactionCoordinatorError::TransactionNotFound(transaction_id.to_string())
            })?;

        if record.state != CoordinatorTransactionState::Ongoing {
            return Err(TransactionCoordinatorError::InvalidStateTransition(
                record.state.to_string(),
                "preparing".to_string(),
            ));
        }

        if record.is_timed_out() {
            record.state = CoordinatorTransactionState::TimedOut;
            return Err(TransactionCoordinatorError::TransactionTimedOut(
                transaction_id.to_string(),
            ));
        }

        if record.partitions.is_empty() {
            return Err(TransactionCoordinatorError::PrepareFailed(
                "No partitions enrolled in transaction".to_string(),
            ));
        }

        record.state = CoordinatorTransactionState::Preparing;
        record.prepare_timestamp = Some(Instant::now());
        info!("Transaction {} entering prepare phase", transaction_id);
        Ok(())
    }

    /// Commit the transaction after a successful prepare phase.
    ///
    /// Transitions from Preparing to Committed. Returns the list of
    /// partitions that participated in the transaction.
    pub async fn commit(
        &self,
        transaction_id: &str,
    ) -> Result<Vec<TransactionPartition>> {
        let mut txns = self.transactions.write().await;

        let record = txns
            .get_mut(transaction_id)
            .ok_or_else(|| {
                TransactionCoordinatorError::TransactionNotFound(transaction_id.to_string())
            })?;

        if record.state != CoordinatorTransactionState::Preparing {
            return Err(TransactionCoordinatorError::InvalidStateTransition(
                record.state.to_string(),
                "committed".to_string(),
            ));
        }

        if record.is_timed_out() {
            record.state = CoordinatorTransactionState::TimedOut;
            return Err(TransactionCoordinatorError::TransactionTimedOut(
                transaction_id.to_string(),
            ));
        }

        record.state = CoordinatorTransactionState::Committed;
        let partitions = record.partitions.clone();
        info!(
            "Transaction {} committed with {} partitions",
            transaction_id,
            partitions.len()
        );
        Ok(partitions)
    }

    /// Abort the transaction.
    ///
    /// Can be called from Ongoing or Preparing states. Returns the list
    /// of partitions that need abort markers written.
    pub async fn abort(
        &self,
        transaction_id: &str,
    ) -> Result<Vec<TransactionPartition>> {
        let mut txns = self.transactions.write().await;

        let record = txns
            .get_mut(transaction_id)
            .ok_or_else(|| {
                TransactionCoordinatorError::TransactionNotFound(transaction_id.to_string())
            })?;

        match record.state {
            CoordinatorTransactionState::Ongoing | CoordinatorTransactionState::Preparing => {
                record.state = CoordinatorTransactionState::Aborted;
                let partitions = record.partitions.clone();
                info!(
                    "Transaction {} aborted with {} partitions",
                    transaction_id,
                    partitions.len()
                );
                Ok(partitions)
            }
            _ => Err(TransactionCoordinatorError::InvalidStateTransition(
                record.state.to_string(),
                "aborted".to_string(),
            )),
        }
    }

    /// Check for timed-out transactions and transition them to TimedOut state.
    ///
    /// Returns the IDs of transactions that were timed out.
    pub async fn check_timeouts(&self) -> Vec<String> {
        let mut txns = self.transactions.write().await;
        let mut timed_out = Vec::new();

        for (id, record) in txns.iter_mut() {
            if record.is_timed_out()
                && record.state != CoordinatorTransactionState::Committed
                && record.state != CoordinatorTransactionState::Aborted
                && record.state != CoordinatorTransactionState::TimedOut
            {
                warn!("Transaction {} timed out in state {}", id, record.state);
                record.state = CoordinatorTransactionState::TimedOut;
                timed_out.push(id.clone());
            }
        }

        timed_out
    }

    /// Recover transactions that were in the Preparing state.
    ///
    /// During crash recovery, transactions stuck in Preparing state need to be
    /// resolved. By default, they are aborted (conservative recovery).
    /// Returns the IDs of recovered (aborted) transactions.
    pub async fn recover_pending(&self) -> Vec<String> {
        let mut txns = self.transactions.write().await;
        let mut recovered = Vec::new();

        for (id, record) in txns.iter_mut() {
            if record.state == CoordinatorTransactionState::Preparing {
                info!(
                    "Recovering transaction {} from preparing state -> aborting",
                    id
                );
                record.state = CoordinatorTransactionState::Aborted;
                recovered.push(id.clone());
            }
        }

        recovered
    }

    /// Get the current state of a transaction.
    pub async fn get_transaction(&self, transaction_id: &str) -> Result<TransactionRecord> {
        let txns = self.transactions.read().await;
        txns.get(transaction_id)
            .cloned()
            .ok_or_else(|| {
                TransactionCoordinatorError::TransactionNotFound(transaction_id.to_string())
            })
    }

    /// Get aggregate statistics about all transactions.
    pub async fn get_stats(&self) -> TransactionStats {
        let txns = self.transactions.read().await;
        let mut stats = TransactionStats::default();

        for record in txns.values() {
            match record.state {
                CoordinatorTransactionState::Ongoing => stats.active_transactions += 1,
                CoordinatorTransactionState::Preparing => stats.preparing_transactions += 1,
                CoordinatorTransactionState::Committed => stats.committed_transactions += 1,
                CoordinatorTransactionState::Aborted => stats.aborted_transactions += 1,
                CoordinatorTransactionState::TimedOut => stats.timed_out_transactions += 1,
            }
            stats.total_partitions_enrolled += record.partitions.len();
        }

        stats
    }

    /// Remove completed (committed, aborted, timed_out) transactions older than
    /// the specified duration. Returns the number of transactions cleaned up.
    pub async fn cleanup_completed(&self, older_than: Duration) -> usize {
        let mut txns = self.transactions.write().await;
        let before_len = txns.len();

        txns.retain(|_, record| {
            let is_terminal = matches!(
                record.state,
                CoordinatorTransactionState::Committed
                    | CoordinatorTransactionState::Aborted
                    | CoordinatorTransactionState::TimedOut
            );
            // Keep if not terminal, or if not old enough
            !is_terminal || record.started_at.elapsed() <= older_than
        });

        before_len - txns.len()
    }

    /// List all active (non-terminal) transaction IDs.
    pub async fn list_active_transactions(&self) -> Vec<String> {
        let txns = self.transactions.read().await;
        txns.values()
            .filter(|r| {
                matches!(
                    r.state,
                    CoordinatorTransactionState::Ongoing | CoordinatorTransactionState::Preparing
                )
            })
            .map(|r| r.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn coordinator() -> TransactionCoordinator {
        TransactionCoordinator::new(Duration::from_secs(30))
    }

    fn short_timeout_coordinator() -> TransactionCoordinator {
        TransactionCoordinator::new(Duration::from_millis(50))
    }

    // Test 1: Normal commit flow (begin -> add_partition -> prepare -> commit)
    #[tokio::test]
    async fn test_normal_commit_flow() {
        let coord = coordinator();

        let txn_id = coord
            .begin("txn-1".to_string(), "producer-1".to_string())
            .await
            .unwrap();
        assert_eq!(txn_id, "txn-1");

        coord
            .add_partition(&txn_id, "topic-a".to_string(), 0, 0, 99)
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "topic-a".to_string(), 1, 100, 199)
            .await
            .unwrap();

        coord.prepare(&txn_id).await.unwrap();

        let partitions = coord.commit(&txn_id).await.unwrap();
        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions[0].topic, "topic-a");
        assert_eq!(partitions[0].partition_id, 0);
        assert_eq!(partitions[1].partition_id, 1);

        let record = coord.get_transaction(&txn_id).await.unwrap();
        assert_eq!(record.state, CoordinatorTransactionState::Committed);
    }

    // Test 2: Normal abort flow (begin -> add_partition -> abort)
    #[tokio::test]
    async fn test_normal_abort_flow() {
        let coord = coordinator();

        let txn_id = coord
            .begin("txn-2".to_string(), "producer-1".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "orders".to_string(), 0, 0, 50)
            .await
            .unwrap();

        let partitions = coord.abort(&txn_id).await.unwrap();
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions[0].topic, "orders");

        let record = coord.get_transaction(&txn_id).await.unwrap();
        assert_eq!(record.state, CoordinatorTransactionState::Aborted);
    }

    // Test 3: Abort from preparing state
    #[tokio::test]
    async fn test_abort_from_preparing() {
        let coord = coordinator();

        let txn_id = coord
            .begin("txn-3".to_string(), "producer-1".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "events".to_string(), 0, 0, 10)
            .await
            .unwrap();

        coord.prepare(&txn_id).await.unwrap();

        // Abort should work from Preparing state
        let partitions = coord.abort(&txn_id).await.unwrap();
        assert_eq!(partitions.len(), 1);

        let record = coord.get_transaction(&txn_id).await.unwrap();
        assert_eq!(record.state, CoordinatorTransactionState::Aborted);
    }

    // Test 4: Timeout handling - transaction times out during prepare
    #[tokio::test]
    async fn test_timeout_during_prepare() {
        let coord = short_timeout_coordinator();

        let txn_id = coord
            .begin("txn-4".to_string(), "producer-1".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "topic".to_string(), 0, 0, 10)
            .await
            .unwrap();

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Prepare should fail with timeout
        let result = coord.prepare(&txn_id).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionCoordinatorError::TransactionTimedOut(_) => {}
            e => panic!("Expected TransactionTimedOut, got {:?}", e),
        }
    }

    // Test 5: Timeout handling - check_timeouts catches expired transactions
    #[tokio::test]
    async fn test_check_timeouts() {
        let coord = short_timeout_coordinator();

        coord
            .begin("txn-5a".to_string(), "producer-1".to_string())
            .await
            .unwrap();
        coord
            .begin("txn-5b".to_string(), "producer-2".to_string())
            .await
            .unwrap();

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        let timed_out = coord.check_timeouts().await;
        assert_eq!(timed_out.len(), 2);
        assert!(timed_out.contains(&"txn-5a".to_string()));
        assert!(timed_out.contains(&"txn-5b".to_string()));
    }

    // Test 6: Prepare phase failure - no partitions enrolled
    #[tokio::test]
    async fn test_prepare_fails_no_partitions() {
        let coord = coordinator();

        let txn_id = coord
            .begin("txn-6".to_string(), "producer-1".to_string())
            .await
            .unwrap();

        let result = coord.prepare(&txn_id).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionCoordinatorError::PrepareFailed(msg) => {
                assert!(msg.contains("No partitions"));
            }
            e => panic!("Expected PrepareFailed, got {:?}", e),
        }
    }

    // Test 7: Invalid state transitions
    #[tokio::test]
    async fn test_invalid_state_transitions() {
        let coord = coordinator();

        let txn_id = coord
            .begin("txn-7".to_string(), "producer-1".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "topic".to_string(), 0, 0, 10)
            .await
            .unwrap();

        // Cannot commit without preparing first
        let result = coord.commit(&txn_id).await;
        assert!(result.is_err());

        // Prepare, then commit
        coord.prepare(&txn_id).await.unwrap();
        coord.commit(&txn_id).await.unwrap();

        // Cannot prepare after commit
        let result = coord.prepare(&txn_id).await;
        assert!(result.is_err());

        // Cannot abort after commit
        let result = coord.abort(&txn_id).await;
        assert!(result.is_err());
    }

    // Test 8: Concurrent transactions from different producers
    #[tokio::test]
    async fn test_concurrent_transactions() {
        let coord = coordinator();

        let txn_a = coord
            .begin("txn-8a".to_string(), "producer-1".to_string())
            .await
            .unwrap();
        let txn_b = coord
            .begin("txn-8b".to_string(), "producer-2".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_a, "orders".to_string(), 0, 0, 50)
            .await
            .unwrap();
        coord
            .add_partition(&txn_b, "orders".to_string(), 1, 0, 30)
            .await
            .unwrap();

        // Both can prepare independently
        coord.prepare(&txn_a).await.unwrap();
        coord.prepare(&txn_b).await.unwrap();

        // One commits, other aborts
        coord.commit(&txn_a).await.unwrap();
        coord.abort(&txn_b).await.unwrap();

        let record_a = coord.get_transaction(&txn_a).await.unwrap();
        let record_b = coord.get_transaction(&txn_b).await.unwrap();

        assert_eq!(record_a.state, CoordinatorTransactionState::Committed);
        assert_eq!(record_b.state, CoordinatorTransactionState::Aborted);
    }

    // Test 9: Recovery of preparing state
    #[tokio::test]
    async fn test_recover_pending_transactions() {
        let coord = coordinator();

        // Create transactions in various states
        let txn_a = coord
            .begin("txn-9a".to_string(), "producer-1".to_string())
            .await
            .unwrap();
        let txn_b = coord
            .begin("txn-9b".to_string(), "producer-2".to_string())
            .await
            .unwrap();
        let txn_c = coord
            .begin("txn-9c".to_string(), "producer-3".to_string())
            .await
            .unwrap();

        // Add partitions to all
        for txn_id in [&txn_a, &txn_b, &txn_c] {
            coord
                .add_partition(txn_id, "topic".to_string(), 0, 0, 10)
                .await
                .unwrap();
        }

        // txn_a: Preparing (should be recovered)
        coord.prepare(&txn_a).await.unwrap();

        // txn_b: Preparing then committed (should NOT be recovered)
        coord.prepare(&txn_b).await.unwrap();
        coord.commit(&txn_b).await.unwrap();

        // txn_c: Still ongoing (should NOT be recovered by recover_pending)

        let recovered = coord.recover_pending().await;
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0], "txn-9a");

        let record_a = coord.get_transaction(&txn_a).await.unwrap();
        assert_eq!(record_a.state, CoordinatorTransactionState::Aborted);
    }

    // Test 10: Duplicate transaction ID
    #[tokio::test]
    async fn test_duplicate_transaction_id() {
        let coord = coordinator();

        coord
            .begin("txn-10".to_string(), "producer-1".to_string())
            .await
            .unwrap();

        let result = coord
            .begin("txn-10".to_string(), "producer-2".to_string())
            .await;
        assert!(result.is_err());
    }

    // Test 11: Duplicate partition enrollment
    #[tokio::test]
    async fn test_duplicate_partition_enrollment() {
        let coord = coordinator();

        let txn_id = coord
            .begin("txn-11".to_string(), "producer-1".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "orders".to_string(), 0, 0, 50)
            .await
            .unwrap();

        // Same topic/partition should fail
        let result = coord
            .add_partition(&txn_id, "orders".to_string(), 0, 51, 100)
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionCoordinatorError::DuplicatePartition { .. } => {}
            e => panic!("Expected DuplicatePartition, got {:?}", e),
        }
    }

    // Test 12: Transaction stats
    #[tokio::test]
    async fn test_transaction_stats() {
        let coord = coordinator();

        // Create a few transactions
        let txn_a = coord
            .begin("txn-12a".to_string(), "p1".to_string())
            .await
            .unwrap();
        let txn_b = coord
            .begin("txn-12b".to_string(), "p2".to_string())
            .await
            .unwrap();
        coord
            .begin("txn-12c".to_string(), "p3".to_string())
            .await
            .unwrap();

        // Add partitions
        coord
            .add_partition(&txn_a, "t1".to_string(), 0, 0, 10)
            .await
            .unwrap();
        coord
            .add_partition(&txn_a, "t1".to_string(), 1, 0, 5)
            .await
            .unwrap();
        coord
            .add_partition(&txn_b, "t2".to_string(), 0, 0, 20)
            .await
            .unwrap();

        // Prepare and commit one
        coord.prepare(&txn_a).await.unwrap();
        coord.commit(&txn_a).await.unwrap();

        // Abort another
        coord
            .add_partition(&txn_b, "t2".to_string(), 1, 0, 15)
            .await
            .unwrap();
        coord.abort(&txn_b).await.unwrap();

        let stats = coord.get_stats().await;
        assert_eq!(stats.active_transactions, 1); // txn_c is ongoing
        assert_eq!(stats.committed_transactions, 1); // txn_a
        assert_eq!(stats.aborted_transactions, 1); // txn_b
        assert_eq!(stats.total_partitions_enrolled, 4); // 2 + 2 + 0
    }

    // Test 13: Cleanup completed transactions
    #[tokio::test]
    async fn test_cleanup_completed() {
        let coord = short_timeout_coordinator();

        let txn_id = coord
            .begin("txn-13".to_string(), "p1".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "t1".to_string(), 0, 0, 10)
            .await
            .unwrap();
        coord.prepare(&txn_id).await.unwrap();
        coord.commit(&txn_id).await.unwrap();

        // Sleep so transaction is old enough
        tokio::time::sleep(Duration::from_millis(100)).await;

        let cleaned = coord.cleanup_completed(Duration::from_millis(50)).await;
        assert_eq!(cleaned, 1);

        // Should be gone
        let result = coord.get_transaction(&txn_id).await;
        assert!(result.is_err());
    }

    // Test 14: List active transactions
    #[tokio::test]
    async fn test_list_active_transactions() {
        let coord = coordinator();

        let txn_a = coord
            .begin("txn-14a".to_string(), "p1".to_string())
            .await
            .unwrap();
        coord
            .begin("txn-14b".to_string(), "p2".to_string())
            .await
            .unwrap();
        let txn_c = coord
            .begin("txn-14c".to_string(), "p3".to_string())
            .await
            .unwrap();

        // Commit one
        coord
            .add_partition(&txn_a, "t1".to_string(), 0, 0, 10)
            .await
            .unwrap();
        coord.prepare(&txn_a).await.unwrap();
        coord.commit(&txn_a).await.unwrap();

        // Put one in preparing state
        coord
            .add_partition(&txn_c, "t2".to_string(), 0, 0, 10)
            .await
            .unwrap();
        coord.prepare(&txn_c).await.unwrap();

        let active = coord.list_active_transactions().await;
        // txn_14b (ongoing) and txn_14c (preparing) should be active
        assert_eq!(active.len(), 2);
        assert!(active.contains(&"txn-14b".to_string()));
        assert!(active.contains(&"txn-14c".to_string()));
    }

    // Test 15: Cannot add partition after prepare
    #[tokio::test]
    async fn test_cannot_add_partition_after_prepare() {
        let coord = coordinator();

        let txn_id = coord
            .begin("txn-15".to_string(), "p1".to_string())
            .await
            .unwrap();

        coord
            .add_partition(&txn_id, "t1".to_string(), 0, 0, 10)
            .await
            .unwrap();

        coord.prepare(&txn_id).await.unwrap();

        // Should not be able to add more partitions
        let result = coord
            .add_partition(&txn_id, "t1".to_string(), 1, 11, 20)
            .await;
        assert!(result.is_err());
    }

    // Test 16: Transaction not found
    #[tokio::test]
    async fn test_transaction_not_found() {
        let coord = coordinator();

        let result = coord.get_transaction("nonexistent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TransactionCoordinatorError::TransactionNotFound(_) => {}
            e => panic!("Expected TransactionNotFound, got {:?}", e),
        }

        let result = coord.prepare("nonexistent").await;
        assert!(result.is_err());

        let result = coord.commit("nonexistent").await;
        assert!(result.is_err());

        let result = coord.abort("nonexistent").await;
        assert!(result.is_err());
    }
}
