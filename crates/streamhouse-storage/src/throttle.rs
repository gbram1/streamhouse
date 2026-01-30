//! S3 Throttle Coordinator
//!
//! This module coordinates rate limiting and circuit breaking for S3 operations,
//! providing a unified interface for throttling protection.
//!
//! ## Architecture
//!
//! ```text
//! PartitionWriter
//!       │
//!       ▼
//! ┌─────────────────┐
//! │ ThrottleCoord   │
//! └────┬────────┬───┘
//!      │        │
//!      ▼        ▼
//! ┌─────────┐ ┌──────────┐
//! │  Rate   │ │ Circuit  │
//! │ Limiter │ │ Breaker  │
//! └─────────┘ └──────────┘
//! ```
//!
//! ## Workflow
//!
//! 1. **Before S3 operation**: Call `acquire(op)` to check both:
//!    - Rate limiter: Do we have capacity?
//!    - Circuit breaker: Is S3 healthy?
//!
//! 2. **After S3 operation**: Call `report_result(op, result)` to:
//!    - Update rate limiter (adaptive adjustment)
//!    - Update circuit breaker (state transitions)
//!
//! ## Error Handling
//!
//! - **503 SlowDown**: Report throttled, reduce rate, increment failures
//! - **5xx errors**: Increment failures (may open circuit)
//! - **Success**: Reset failure count, increase rate gradually
//!
//! ## Configuration
//!
//! All configuration is optional - uses sensible defaults based on S3 limits.

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
use crate::rate_limiter::{BucketConfig, RateLimiter, S3Operation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Result of a throttle check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleDecision {
    /// Request is allowed to proceed
    Allow,
    /// Request is rate limited (no tokens available)
    RateLimited,
    /// Request is rejected by circuit breaker (S3 unhealthy)
    CircuitOpen,
}

/// Configuration for throttle coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottleConfig {
    /// Rate limiter configuration for PUT operations
    pub put_rate: BucketConfig,
    /// Rate limiter configuration for GET operations
    pub get_rate: BucketConfig,
    /// Rate limiter configuration for DELETE operations
    pub delete_rate: BucketConfig,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            // PUT: 3000/sec (S3 limit: 3500/sec, keep 15% headroom)
            put_rate: BucketConfig {
                rate: 3000.0,
                burst: 5000,
                min_rate: 100.0,
                max_rate: 10000.0,
            },
            // GET: 5000/sec (S3 limit: 5500/sec, keep 10% headroom)
            get_rate: BucketConfig {
                rate: 5000.0,
                burst: 8000,
                min_rate: 100.0,
                max_rate: 15000.0,
            },
            // DELETE: 3000/sec (same as PUT)
            delete_rate: BucketConfig {
                rate: 3000.0,
                burst: 5000,
                min_rate: 100.0,
                max_rate: 10000.0,
            },
            // Circuit breaker: Open after 5 failures, test after 30s
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 2,
                timeout: Duration::from_secs(30),
            },
        }
    }
}

/// Throttle coordinator for S3 operations
///
/// Combines rate limiting and circuit breaking into a unified API.
/// Thread-safe and designed for concurrent access from multiple partitions.
pub struct ThrottleCoordinator {
    rate_limiter: Arc<RateLimiter>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl ThrottleCoordinator {
    /// Create a new throttle coordinator with custom configuration
    pub fn new(config: ThrottleConfig) -> Self {
        Self {
            rate_limiter: Arc::new(RateLimiter::new(
                config.put_rate,
                config.get_rate,
                config.delete_rate,
            )),
            circuit_breaker: Arc::new(CircuitBreaker::new(config.circuit_breaker)),
        }
    }

    /// Create with default S3 limits
    pub fn with_defaults() -> Self {
        Self::new(ThrottleConfig::default())
    }

    /// Try to acquire permission for an S3 operation
    ///
    /// Checks both rate limiter and circuit breaker.
    /// Returns immediately (non-blocking).
    pub async fn acquire(&self, op: S3Operation) -> ThrottleDecision {
        // Check circuit breaker first (fail-fast if open)
        if !self.circuit_breaker.allow_request().await {
            return ThrottleDecision::CircuitOpen;
        }

        // Check rate limiter
        if !self.rate_limiter.try_acquire(op).await {
            return ThrottleDecision::RateLimited;
        }

        ThrottleDecision::Allow
    }

    /// Report the result of an S3 operation
    ///
    /// Updates both rate limiter and circuit breaker based on result.
    ///
    /// # Arguments
    /// * `op` - The S3 operation type
    /// * `success` - True if operation succeeded, false if failed
    /// * `is_throttle_error` - True if failure was a 503 SlowDown error
    pub async fn report_result(&self, op: S3Operation, success: bool, is_throttle_error: bool) {
        if success {
            // Report success to both components
            self.rate_limiter.report_success(op).await;
            self.circuit_breaker.report_success().await;
        } else {
            // Report failure to circuit breaker
            self.circuit_breaker.report_failure().await;

            // If it's a throttle error, reduce rate
            if is_throttle_error {
                self.rate_limiter.report_throttled(op).await;
            }
        }
    }

    /// Get current circuit breaker state (for monitoring)
    pub fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.current_state()
    }

    /// Get current rate limits (for monitoring)
    pub async fn current_rates(&self) -> (f64, f64, f64) {
        self.rate_limiter.current_rates().await
    }

    /// Get available tokens (for monitoring)
    pub fn available_tokens(&self) -> (f64, f64, f64) {
        self.rate_limiter.available_tokens()
    }

    /// Manually reset circuit breaker (for admin operations)
    pub fn reset_circuit(&self) {
        self.circuit_breaker.reset();
    }
}

impl Clone for ThrottleCoordinator {
    fn clone(&self) -> Self {
        Self {
            rate_limiter: Arc::clone(&self.rate_limiter),
            circuit_breaker: Arc::clone(&self.circuit_breaker),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_throttle_allows_request() {
        let coordinator = ThrottleCoordinator::with_defaults();

        // Should allow requests initially
        let decision = coordinator.acquire(S3Operation::Put).await;
        assert_eq!(decision, ThrottleDecision::Allow);
    }

    #[tokio::test]
    async fn test_throttle_rate_limited() {
        let coordinator = ThrottleCoordinator::new(ThrottleConfig {
            put_rate: BucketConfig {
                rate: 100.0,
                burst: 5, // Very small burst
                min_rate: 10.0,
                max_rate: 1000.0,
            },
            ..Default::default()
        });

        // Drain the bucket
        for _ in 0..5 {
            assert_eq!(
                coordinator.acquire(S3Operation::Put).await,
                ThrottleDecision::Allow
            );
        }

        // Should be rate limited now
        let decision = coordinator.acquire(S3Operation::Put).await;
        assert_eq!(decision, ThrottleDecision::RateLimited);
    }

    #[tokio::test]
    async fn test_throttle_circuit_open() {
        let coordinator = ThrottleCoordinator::new(ThrottleConfig {
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 2,
                success_threshold: 2,
                timeout: Duration::from_secs(10),
            },
            ..Default::default()
        });

        // Report failures to open circuit
        coordinator
            .report_result(S3Operation::Put, false, false)
            .await;
        coordinator
            .report_result(S3Operation::Put, false, false)
            .await;

        // Should be rejected by circuit breaker
        let decision = coordinator.acquire(S3Operation::Put).await;
        assert_eq!(decision, ThrottleDecision::CircuitOpen);
    }

    #[tokio::test]
    async fn test_throttle_success_flow() {
        let coordinator = ThrottleCoordinator::new(ThrottleConfig {
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout: Duration::from_secs(1),
            },
            ..Default::default()
        });

        // Report some failures (below threshold)
        coordinator
            .report_result(S3Operation::Put, false, false)
            .await;
        coordinator
            .report_result(S3Operation::Put, false, false)
            .await;

        // Should still allow
        assert_eq!(
            coordinator.acquire(S3Operation::Put).await,
            ThrottleDecision::Allow
        );

        // Report success - should reset failure count
        coordinator
            .report_result(S3Operation::Put, true, false)
            .await;

        // Should still allow
        assert_eq!(
            coordinator.acquire(S3Operation::Put).await,
            ThrottleDecision::Allow
        );
    }

    #[tokio::test]
    async fn test_throttle_error_reduces_rate() {
        let coordinator = ThrottleCoordinator::with_defaults();

        let (initial_put, _, _) = coordinator.current_rates().await;

        // Report throttle error
        coordinator
            .report_result(S3Operation::Put, false, true)
            .await;

        let (reduced_put, _, _) = coordinator.current_rates().await;

        // Rate should be reduced
        assert!(reduced_put < initial_put);
    }

    #[tokio::test]
    async fn test_throttle_recovery_flow() {
        let coordinator = ThrottleCoordinator::new(ThrottleConfig {
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 1,
                success_threshold: 2,
                timeout: Duration::from_millis(50),
            },
            ..Default::default()
        });

        // Open circuit
        coordinator
            .report_result(S3Operation::Put, false, false)
            .await;
        assert_eq!(coordinator.circuit_state(), CircuitState::Open);

        // Wait for half-open
        sleep(Duration::from_millis(60)).await;

        // Should transition to half-open on next request
        assert_eq!(
            coordinator.acquire(S3Operation::Put).await,
            ThrottleDecision::Allow
        );
        assert_eq!(coordinator.circuit_state(), CircuitState::HalfOpen);

        // Report successes to close circuit
        coordinator
            .report_result(S3Operation::Put, true, false)
            .await;
        coordinator
            .report_result(S3Operation::Put, true, false)
            .await;

        assert_eq!(coordinator.circuit_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_throttle_clone() {
        let coordinator = ThrottleCoordinator::with_defaults();
        let cloned = coordinator.clone();

        // Both should share the same state
        coordinator
            .report_result(S3Operation::Put, false, false)
            .await;

        // State should be visible in clone
        assert_eq!(
            coordinator.circuit_breaker.failure_count(),
            cloned.circuit_breaker.failure_count()
        );
    }

    #[tokio::test]
    async fn test_independent_operation_types() {
        let coordinator = ThrottleCoordinator::new(ThrottleConfig {
            put_rate: BucketConfig {
                rate: 100.0,
                burst: 5,
                min_rate: 10.0,
                max_rate: 1000.0,
            },
            get_rate: BucketConfig {
                rate: 100.0,
                burst: 10, // Different burst
                min_rate: 10.0,
                max_rate: 1000.0,
            },
            ..Default::default()
        });

        // Drain PUT bucket
        for _ in 0..5 {
            assert_eq!(
                coordinator.acquire(S3Operation::Put).await,
                ThrottleDecision::Allow
            );
        }

        // PUT should be rate limited
        assert_eq!(
            coordinator.acquire(S3Operation::Put).await,
            ThrottleDecision::RateLimited
        );

        // GET should still work (independent bucket)
        assert_eq!(
            coordinator.acquire(S3Operation::Get).await,
            ThrottleDecision::Allow
        );
    }
}
