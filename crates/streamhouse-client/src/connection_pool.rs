//! gRPC Connection Pool for Producer Clients
//!
//! This module provides connection pooling for gRPC connections to StreamHouse agents.
//! Connection pooling improves performance by reusing existing connections instead of
//! creating new ones for each request.
//!
//! ## Benefits
//!
//! - **Reduced latency**: No TCP handshake or TLS negotiation for existing connections
//! - **Lower resource usage**: Fewer file descriptors and memory allocations
//! - **Better throughput**: Amortizes connection overhead across many requests
//!
//! ## Design
//!
//! The pool maintains a map of agent addresses to connection pools. Each pool has:
//! - Max connections per agent (default: 5)
//! - Idle timeout (default: 60s)
//! - Health checking on checkout
//!
//! ## Thread Safety
//!
//! ConnectionPool is Send + Sync and can be safely shared via Arc<ConnectionPool>.
//!
//! ## Examples
//!
//! ```ignore
//! use streamhouse_client::connection_pool::ConnectionPool;
//!
//! let pool = ConnectionPool::new(5, Duration::from_secs(60));
//!
//! // Get connection (creates if doesn't exist)
//! let client = pool.get_connection("http://localhost:9090").await?;
//!
//! // Use client
//! let response = client.produce(request).await?;
//!
//! // Connection automatically returned to pool when dropped
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use streamhouse_proto::producer::producer_service_client::ProducerServiceClient;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, warn};

/// Connection pool entry with health tracking.
#[derive(Clone)]
struct PooledConnection {
    /// The gRPC client
    client: ProducerServiceClient<Channel>,

    /// Last time this connection was used
    last_used: Instant,

    /// Whether this connection is currently healthy
    healthy: bool,
}

/// Pool of gRPC connections to StreamHouse agents.
///
/// Manages connection lifecycle, health checking, and idle timeout.
///
/// # Thread Safety
///
/// ConnectionPool is Send + Sync and uses RwLock for concurrent access.
///
/// # Configuration
///
/// - `max_connections_per_agent`: Maximum connections per agent (default: 5)
/// - `idle_timeout`: Close connections idle for this duration (default: 60s)
///
/// # Examples
///
/// ```ignore
/// let pool = ConnectionPool::new(5, Duration::from_secs(60));
///
/// // Get connection to an agent
/// let client = pool.get_connection("http://agent-001:9090").await?;
///
/// // Make request
/// let response = client.produce(request).await?;
/// ```
pub struct ConnectionPool {
    /// Map of agent addresses to their connection pools
    pools: Arc<RwLock<HashMap<String, Vec<PooledConnection>>>>,

    /// Maximum connections per agent
    max_connections_per_agent: usize,

    /// Idle timeout for connections
    idle_timeout: Duration,
}

impl ConnectionPool {
    /// Create a new connection pool.
    ///
    /// # Arguments
    ///
    /// * `max_connections_per_agent` - Maximum connections per agent (recommended: 5-10)
    /// * `idle_timeout` - Close connections idle for this duration (recommended: 60s)
    ///
    /// # Returns
    ///
    /// A new `ConnectionPool` ready to manage connections.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Default configuration
    /// let pool = ConnectionPool::new(5, Duration::from_secs(60));
    ///
    /// // High-throughput configuration
    /// let pool = ConnectionPool::new(10, Duration::from_secs(120));
    /// ```
    pub fn new(max_connections_per_agent: usize, idle_timeout: Duration) -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            max_connections_per_agent,
            idle_timeout,
        }
    }

    /// Get a connection to an agent, creating one if necessary.
    ///
    /// This method:
    /// 1. Checks for an existing healthy connection in the pool
    /// 2. Removes idle connections (last_used > idle_timeout)
    /// 3. Creates a new connection if pool is empty or all unhealthy
    /// 4. Returns the connection (caller owns it, returned to pool on drop)
    ///
    /// # Arguments
    ///
    /// * `address` - Agent address (e.g., "http://localhost:9090")
    ///
    /// # Returns
    ///
    /// A gRPC client connected to the agent.
    ///
    /// # Errors
    ///
    /// - `TransportError`: Failed to connect to agent
    /// - `InvalidUri`: Malformed agent address
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let client = pool.get_connection("http://agent-001:9090").await?;
    /// let response = client.produce(request).await?;
    /// ```
    pub async fn get_connection(
        &self,
        address: &str,
    ) -> Result<ProducerServiceClient<Channel>, Box<dyn std::error::Error + Send + Sync>> {
        // Try to get an existing connection
        {
            let mut pools = self.pools.write().await;
            let pool = pools.entry(address.to_string()).or_insert_with(Vec::new);

            // Remove idle connections
            let now = Instant::now();
            pool.retain(|conn| now.duration_since(conn.last_used) < self.idle_timeout);

            let pool_size = pool.len();

            // Find a healthy connection
            if let Some(conn) = pool.iter_mut().find(|c| c.healthy) {
                conn.last_used = now;
                debug!(
                    address = %address,
                    pool_size = pool_size,
                    "Reusing existing connection"
                );
                return Ok(conn.client.clone());
            }
        }

        // No healthy connection found, create a new one
        debug!(address = %address, "Creating new connection");
        let client = self.create_connection(address).await?;

        // Add to pool if not at max capacity
        {
            let mut pools = self.pools.write().await;
            let pool = pools.entry(address.to_string()).or_insert_with(Vec::new);

            if pool.len() < self.max_connections_per_agent {
                pool.push(PooledConnection {
                    client: client.clone(),
                    last_used: Instant::now(),
                    healthy: true,
                });
                debug!(
                    address = %address,
                    pool_size = pool.len(),
                    "Added connection to pool"
                );
            } else {
                debug!(
                    address = %address,
                    pool_size = pool.len(),
                    max = self.max_connections_per_agent,
                    "Pool at max capacity, connection not pooled"
                );
            }
        }

        Ok(client)
    }

    /// Create a new gRPC connection to an agent.
    ///
    /// # Arguments
    ///
    /// * `address` - Agent address (e.g., "http://localhost:9090")
    ///
    /// # Returns
    ///
    /// A new gRPC client connected to the agent.
    ///
    /// # Errors
    ///
    /// - `TransportError`: Failed to connect to agent
    /// - `InvalidUri`: Malformed agent address
    async fn create_connection(
        &self,
        address: &str,
    ) -> Result<ProducerServiceClient<Channel>, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = Endpoint::from_shared(address.to_string())?
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .tcp_keepalive(Some(Duration::from_secs(60)));

        let channel = endpoint.connect().await?;
        Ok(ProducerServiceClient::new(channel))
    }

    /// Mark a connection as unhealthy.
    ///
    /// Called when a request fails, so the connection won't be reused.
    ///
    /// # Arguments
    ///
    /// * `address` - Agent address
    ///
    /// # Examples
    ///
    /// ```ignore
    /// match client.produce(request).await {
    ///     Ok(response) => { /* success */ }
    ///     Err(e) => {
    ///         pool.mark_unhealthy(address).await;
    ///         return Err(e);
    ///     }
    /// }
    /// ```
    pub async fn mark_unhealthy(&self, address: &str) {
        let mut pools = self.pools.write().await;
        if let Some(pool) = pools.get_mut(address) {
            for conn in pool.iter_mut() {
                conn.healthy = false;
            }
            warn!(address = %address, "Marked all connections as unhealthy");
        }
    }

    /// Close all connections to a specific agent.
    ///
    /// Useful when an agent is known to be down.
    ///
    /// # Arguments
    ///
    /// * `address` - Agent address
    pub async fn close_agent(&self, address: &str) {
        let mut pools = self.pools.write().await;
        pools.remove(address);
        debug!(address = %address, "Closed all connections to agent");
    }

    /// Close all connections in the pool.
    ///
    /// Called during graceful shutdown.
    pub async fn close_all(&self) {
        let mut pools = self.pools.write().await;
        pools.clear();
        debug!("Closed all connections");
    }

    /// Get pool statistics for monitoring.
    ///
    /// # Returns
    ///
    /// Tuple of (total_connections, healthy_connections, agents_count)
    pub async fn stats(&self) -> (usize, usize, usize) {
        let pools = self.pools.read().await;
        let total = pools.values().map(|p| p.len()).sum();
        let healthy = pools
            .values()
            .flat_map(|p| p.iter())
            .filter(|c| c.healthy)
            .count();
        let agents = pools.len();
        (total, healthy, agents)
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_pool_creation() {
        let pool = ConnectionPool::new(5, Duration::from_secs(60));
        let (total, healthy, agents) = pool.stats().await;
        assert_eq!(total, 0);
        assert_eq!(healthy, 0);
        assert_eq!(agents, 0);
    }

    #[tokio::test]
    async fn test_mark_unhealthy() {
        let pool = ConnectionPool::new(5, Duration::from_secs(60));
        pool.mark_unhealthy("http://localhost:9090").await;
        // Should not panic when marking non-existent connection as unhealthy
    }

    #[tokio::test]
    async fn test_close_all() {
        let pool = ConnectionPool::new(5, Duration::from_secs(60));
        pool.close_all().await;
        let (total, healthy, agents) = pool.stats().await;
        assert_eq!(total, 0);
        assert_eq!(healthy, 0);
        assert_eq!(agents, 0);
    }
}
