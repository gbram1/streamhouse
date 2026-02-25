//! Connection Pool with Circuit Breakers
//!
//! Manages pooled connections to cluster nodes with circuit breaker protection.
//! When a node becomes unhealthy, its circuit breaker opens to prevent
//! cascading failures.
//!
//! ## Circuit Breaker States
//!
//! - **Closed**: Normal operation, requests flow through
//! - **Open**: Node is unhealthy, requests are immediately rejected
//! - **HalfOpen**: After a timeout, a single request is allowed through to test recovery
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_agent::connection_pool::{ConnectionPool, PoolConfig};
//! use std::time::Duration;
//!
//! let config = PoolConfig {
//!     max_connections_per_node: 10,
//!     connection_timeout: Duration::from_secs(5),
//!     ..Default::default()
//! };
//! let pool = ConnectionPool::new(config);
//!
//! let conn = pool.get_connection("10.0.1.1:9090").await?;
//! // Use connection...
//! pool.return_connection("10.0.1.1:9090", conn).await;
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};

/// Configuration for the connection pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Maximum number of connections per node.
    pub max_connections_per_node: usize,
    /// Timeout for establishing a new connection.
    pub connection_timeout: Duration,
    /// How long an idle connection can live before being closed.
    pub idle_timeout: Duration,
    /// Maximum number of retries for failed connections.
    pub max_retries: u32,
    /// Number of consecutive failures before the circuit breaker opens.
    pub circuit_breaker_threshold: u32,
    /// How long the circuit breaker stays open before transitioning to half-open.
    pub circuit_breaker_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_node: 10,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
            max_retries: 3,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout: Duration::from_secs(30),
        }
    }
}

/// State of a circuit breaker.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation, requests flow through.
    Closed,
    /// Node is unhealthy, requests are rejected.
    Open,
    /// Testing if node has recovered, allowing a single request through.
    HalfOpen,
}

/// Circuit breaker for a single node.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Current state of the circuit breaker.
    pub state: CircuitState,
    /// Number of consecutive failures.
    pub failure_count: u32,
    /// Threshold at which the circuit opens.
    pub threshold: u32,
    /// How long the circuit stays open before going half-open.
    pub timeout: Duration,
    /// When the last failure occurred.
    pub last_failure: Option<Instant>,
    /// When the last success occurred.
    pub last_success: Option<Instant>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given threshold and timeout.
    pub fn new(threshold: u32, timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            threshold,
            timeout,
            last_failure: None,
            last_success: None,
        }
    }

    /// Record a failure. If failures exceed the threshold, open the circuit.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());
        if self.failure_count >= self.threshold {
            self.state = CircuitState::Open;
            debug!(
                failures = self.failure_count,
                threshold = self.threshold,
                "Circuit breaker OPENED"
            );
        }
    }

    /// Record a success. Reset the circuit breaker to closed.
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
        self.last_success = Some(Instant::now());
    }

    /// Check if a request should be allowed through.
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if enough time has passed to try half-open
                if let Some(last_failure) = self.last_failure {
                    if last_failure.elapsed() >= self.timeout {
                        self.state = CircuitState::HalfOpen;
                        debug!("Circuit breaker transitioning to HALF-OPEN");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow one request through in half-open state
                true
            }
        }
    }
}

/// A pooled connection to a remote node.
#[derive(Debug, Clone)]
pub struct PooledConnection {
    /// Unique identifier for this connection.
    pub id: u64,
    /// Address of the remote node.
    pub address: String,
    /// When this connection was created.
    pub created_at: Instant,
    /// When this connection was last used.
    pub last_used: Instant,
    /// Whether the connection is currently checked out.
    pub checked_out: bool,
    /// Whether the connection is considered healthy.
    pub healthy: bool,
}

impl PooledConnection {
    /// Check if this connection is idle (last_used elapsed > timeout).
    pub fn is_idle(&self, timeout: Duration) -> bool {
        self.last_used.elapsed() > timeout
    }
}

/// Errors that can occur in the connection pool.
#[derive(Debug, Error)]
pub enum PoolError {
    /// The circuit breaker for this node is open.
    #[error("Circuit breaker is open for {0}")]
    CircuitOpen(String),

    /// No connections available and pool is at capacity.
    #[error("Connection pool exhausted for {0}")]
    PoolExhausted(String),

    /// Connection timed out.
    #[error("Connection timeout for {0}")]
    Timeout(String),

    /// Generic connection error.
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

/// Pool statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Total connections created.
    pub total_connections_created: u64,
    /// Total connections returned.
    pub total_connections_returned: u64,
    /// Total failed connections.
    pub total_connections_failed: u64,
    /// Total successful connections.
    pub total_connections_success: u64,
}

/// Connection pool with circuit breaker protection.
#[derive(Debug)]
pub struct ConnectionPool {
    /// Pool configuration.
    pub config: PoolConfig,
    /// Per-node connection lists.
    connections: Arc<RwLock<HashMap<String, Vec<PooledConnection>>>>,
    /// Per-node circuit breakers.
    circuit_breakers: Mutex<HashMap<String, CircuitBreaker>>,
    /// Counter for generating connection IDs.
    next_conn_id: AtomicU64,
    /// Stats counters.
    total_created: AtomicU64,
    total_returned: AtomicU64,
    total_failed: AtomicU64,
    total_success: AtomicU64,
}

impl ConnectionPool {
    /// Create a new connection pool with the given configuration.
    pub fn new(config: PoolConfig) -> Self {
        info!(
            max_per_node = config.max_connections_per_node,
            "Creating connection pool"
        );
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            circuit_breakers: Mutex::new(HashMap::new()),
            next_conn_id: AtomicU64::new(1),
            total_created: AtomicU64::new(0),
            total_returned: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
            total_success: AtomicU64::new(0),
        }
    }

    /// Get a connection to a node. Returns a pooled connection if available,
    /// otherwise creates a new one. Returns an error if the circuit breaker is open.
    pub async fn get_connection(&self, address: &str) -> Result<PooledConnection, PoolError> {
        // Check circuit breaker
        {
            let mut breakers = self.circuit_breakers.lock().await;
            let breaker = breakers
                .entry(address.to_string())
                .or_insert_with(|| {
                    CircuitBreaker::new(
                        self.config.circuit_breaker_threshold,
                        self.config.circuit_breaker_timeout,
                    )
                });
            if !breaker.allow_request() {
                return Err(PoolError::CircuitOpen(address.to_string()));
            }
        }

        // Try to get an existing idle connection
        {
            let mut conns = self.connections.write().await;
            if let Some(pool) = conns.get_mut(address) {
                if let Some(conn) = pool.iter_mut().find(|c| !c.checked_out && c.healthy) {
                    conn.checked_out = true;
                    conn.last_used = Instant::now();
                    debug!(address = address, conn_id = conn.id, "Reusing pooled connection");
                    return Ok(conn.clone());
                }

                // Check if we can create a new one
                if pool.len() >= self.config.max_connections_per_node {
                    return Err(PoolError::PoolExhausted(address.to_string()));
                }
            }
        }

        // Create a new connection
        let id = self.next_conn_id.fetch_add(1, Ordering::Relaxed);
        let conn = PooledConnection {
            id,
            address: address.to_string(),
            created_at: Instant::now(),
            last_used: Instant::now(),
            checked_out: true,
            healthy: true,
        };

        self.total_created.fetch_add(1, Ordering::Relaxed);

        let mut conns = self.connections.write().await;
        conns
            .entry(address.to_string())
            .or_default()
            .push(conn.clone());

        debug!(
            address = address,
            conn_id = id,
            "Created new pooled connection"
        );

        Ok(conn)
    }

    /// Return a connection to the pool.
    pub async fn return_connection(&self, address: &str, conn: PooledConnection) {
        let mut conns = self.connections.write().await;
        if let Some(pool) = conns.get_mut(address) {
            if let Some(existing) = pool.iter_mut().find(|c| c.id == conn.id) {
                existing.checked_out = false;
                existing.last_used = Instant::now();
                self.total_returned.fetch_add(1, Ordering::Relaxed);
                debug!(
                    address = address,
                    conn_id = conn.id,
                    "Returned connection to pool"
                );
            }
        }
    }

    /// Mark a connection as failed. Records the failure in the circuit breaker.
    pub async fn mark_connection_failed(&self, address: &str) {
        self.total_failed.fetch_add(1, Ordering::Relaxed);

        let mut breakers = self.circuit_breakers.lock().await;
        let breaker = breakers
            .entry(address.to_string())
            .or_insert_with(|| {
                CircuitBreaker::new(
                    self.config.circuit_breaker_threshold,
                    self.config.circuit_breaker_timeout,
                )
            });
        breaker.record_failure();

        info!(
            address = address,
            failure_count = breaker.failure_count,
            "Connection failure recorded"
        );
    }

    /// Mark a connection as successful. Records the success in the circuit breaker.
    pub async fn mark_connection_success(&self, address: &str) {
        self.total_success.fetch_add(1, Ordering::Relaxed);

        let mut breakers = self.circuit_breakers.lock().await;
        if let Some(breaker) = breakers.get_mut(address) {
            breaker.record_success();
        }
    }

    /// Get the circuit breaker state for a node.
    pub async fn circuit_state(&self, address: &str) -> Option<CircuitState> {
        let breakers = self.circuit_breakers.lock().await;
        breakers.get(address).map(|b| b.state)
    }

    /// Get pool statistics.
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            total_connections_created: self.total_created.load(Ordering::Relaxed),
            total_connections_returned: self.total_returned.load(Ordering::Relaxed),
            total_connections_failed: self.total_failed.load(Ordering::Relaxed),
            total_connections_success: self.total_success.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> PoolConfig {
        PoolConfig {
            max_connections_per_node: 5,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
            max_retries: 3,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout: Duration::from_secs(30),
        }
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.max_connections_per_node, 10);
        assert_eq!(config.circuit_breaker_threshold, 5);
    }

    #[test]
    fn test_circuit_breaker_new() {
        let cb = CircuitBreaker::new(5, Duration::from_secs(30));
        assert_eq!(cb.state, CircuitState::Closed);
        assert_eq!(cb.failure_count, 0);
    }

    #[test]
    fn test_circuit_breaker_record_failure() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.record_failure();
        assert_eq!(cb.failure_count, 1);
        assert_eq!(cb.state, CircuitState::Closed);

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_record_success() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.failure_count, 0);
        assert_eq!(cb.state, CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_allow_request_closed() {
        let mut cb = CircuitBreaker::new(5, Duration::from_secs(30));
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_deny_request_open() {
        let mut cb = CircuitBreaker::new(2, Duration::from_secs(300));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let mut cb = CircuitBreaker::new(1, Duration::from_millis(0));
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);
        // With 0ms timeout, should immediately transition to half-open
        assert!(cb.allow_request());
        assert_eq!(cb.state, CircuitState::HalfOpen);
    }

    #[test]
    fn test_pooled_connection_is_idle() {
        let mut conn = PooledConnection {
            id: 1,
            address: "localhost:9090".to_string(),
            created_at: Instant::now(),
            last_used: Instant::now(),
            checked_out: false,
            healthy: true,
        };
        // Just created, should not be idle with 60s timeout
        assert!(!conn.is_idle(Duration::from_secs(60)));

        // Sleep briefly then check with 0s timeout
        std::thread::sleep(Duration::from_millis(1));
        conn.last_used = Instant::now() - Duration::from_millis(1);
        assert!(conn.is_idle(Duration::from_secs(0)));
    }

    #[tokio::test]
    async fn test_get_connection() {
        let pool = ConnectionPool::new(make_config());
        let conn = pool.get_connection("localhost:9090").await.unwrap();
        assert_eq!(conn.address, "localhost:9090");
        assert!(conn.checked_out);
        assert!(conn.healthy);
    }

    #[tokio::test]
    async fn test_return_connection() {
        let pool = ConnectionPool::new(make_config());
        let conn = pool.get_connection("localhost:9090").await.unwrap();
        pool.return_connection("localhost:9090", conn).await;

        let stats = pool.stats();
        assert_eq!(stats.total_connections_returned, 1);
    }

    #[tokio::test]
    async fn test_pool_exhausted() {
        let config = PoolConfig {
            max_connections_per_node: 1,
            ..make_config()
        };
        let pool = ConnectionPool::new(config);

        let _conn = pool.get_connection("localhost:9090").await.unwrap();
        let result = pool.get_connection("localhost:9090").await;
        assert!(matches!(result, Err(PoolError::PoolExhausted(_))));
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens() {
        let config = PoolConfig {
            circuit_breaker_threshold: 2,
            circuit_breaker_timeout: Duration::from_secs(300),
            ..make_config()
        };
        let pool = ConnectionPool::new(config);

        pool.mark_connection_failed("localhost:9090").await;
        pool.mark_connection_failed("localhost:9090").await;

        let state = pool.circuit_state("localhost:9090").await;
        assert_eq!(state, Some(CircuitState::Open));

        let result = pool.get_connection("localhost:9090").await;
        assert!(matches!(result, Err(PoolError::CircuitOpen(_))));
    }

    #[tokio::test]
    async fn test_mark_connection_success_resets_circuit() {
        let config = PoolConfig {
            circuit_breaker_threshold: 2,
            ..make_config()
        };
        let pool = ConnectionPool::new(config);

        pool.mark_connection_failed("localhost:9090").await;
        pool.mark_connection_success("localhost:9090").await;

        let state = pool.circuit_state("localhost:9090").await;
        assert_eq!(state, Some(CircuitState::Closed));
    }

    #[tokio::test]
    async fn test_stats() {
        let pool = ConnectionPool::new(make_config());

        let conn = pool.get_connection("localhost:9090").await.unwrap();
        pool.return_connection("localhost:9090", conn).await;
        pool.mark_connection_failed("localhost:9091").await;
        pool.mark_connection_success("localhost:9090").await;

        let stats = pool.stats();
        assert_eq!(stats.total_connections_created, 1);
        assert_eq!(stats.total_connections_returned, 1);
        assert_eq!(stats.total_connections_failed, 1);
        assert_eq!(stats.total_connections_success, 1);
    }

    #[tokio::test]
    async fn test_reuse_returned_connection() {
        let pool = ConnectionPool::new(make_config());

        let conn = pool.get_connection("localhost:9090").await.unwrap();
        let id = conn.id;
        pool.return_connection("localhost:9090", conn).await;

        // Should reuse the returned connection
        let conn2 = pool.get_connection("localhost:9090").await.unwrap();
        assert_eq!(conn2.id, id);
    }
}
