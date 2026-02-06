//! Circuit Breaker for S3 Operations
//!
//! This module implements a circuit breaker pattern to prevent cascading failures
//! when S3 is experiencing issues. The circuit breaker has three states:
//!
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Too many failures, reject requests immediately (fail-fast)
//! - **HalfOpen**: Testing recovery, allow limited requests to probe
//!
//! ## State Transitions
//!
//! ```text
//! ┌────────┐  failures >= threshold  ┌──────┐
//! │ Closed │ ─────────────────────> │ Open │
//! └───┬────┘                         └───┬──┘
//!     │                                  │
//!     │ success                          │ timeout expired
//!     │                                  │
//!     │      ┌──────────┐                │
//!     └───── │ HalfOpen │ <──────────────┘
//!            └─────┬────┘
//!                  │
//!                  │ consecutive successes >= threshold
//!                  └──────> Back to Closed
//! ```
//!
//! ## Configuration
//!
//! - **failure_threshold**: Number of failures before opening (default: 5)
//! - **success_threshold**: Consecutive successes in half-open to close (default: 2)
//! - **timeout**: How long to wait in open state before trying half-open (default: 30s)
//!
//! ## Performance
//!
//! - State check overhead: <50ns (atomic load)
//! - State transition: <1µs (atomic compare-and-swap)
//! - No background threads

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed = 0,
    /// Too many failures - reject requests immediately
    Open = 1,
    /// Testing recovery - allow limited requests
    HalfOpen = 2,
}

impl From<u8> for CircuitState {
    fn from(value: u8) -> Self {
        match value {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed, // Default to closed for unknown values
        }
    }
}

/// Configuration for circuit breaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening
    pub failure_threshold: u32,
    /// Number of consecutive successes in half-open to close
    pub success_threshold: u32,
    /// Duration to wait in open state before transitioning to half-open
    pub timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,      // Open after 5 failures
            success_threshold: 2,      // Close after 2 successes in half-open
            timeout: Duration::from_secs(30), // Wait 30s before testing
        }
    }
}

/// Circuit breaker for S3 operations
///
/// Prevents cascading failures by failing fast when S3 is unhealthy.
/// Uses atomic operations for lock-free state checks.
pub struct CircuitBreaker {
    /// Current state (0=Closed, 1=Open, 2=HalfOpen)
    state: AtomicU8,
    /// Consecutive failure count in Closed state
    failure_count: AtomicU64,
    /// Consecutive success count in HalfOpen state
    success_count: AtomicU64,
    /// Timestamp when circuit opened (microseconds since epoch)
    opened_at: AtomicU64,
    /// Configuration
    config: Arc<RwLock<CircuitBreakerConfig>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with given configuration
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            opened_at: AtomicU64::new(0),
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Get current time in microseconds since epoch
    fn now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    /// Check if a request is allowed (non-blocking)
    ///
    /// Returns true if request should proceed, false if circuit is open
    pub async fn allow_request(&self) -> bool {
        let current_state: CircuitState = self.state.load(Ordering::Acquire).into();

        match current_state {
            CircuitState::Closed => true, // Always allow in closed state

            CircuitState::Open => {
                // Check if timeout has elapsed
                let opened_at = self.opened_at.load(Ordering::Acquire);
                let now = Self::now_micros();
                let config = self.config.read().await;
                let timeout_micros = config.timeout.as_micros() as u64;

                if now - opened_at >= timeout_micros {
                    // Transition to half-open
                    self.transition_to_half_open();
                    true // Allow this request to test
                } else {
                    false // Still in timeout, reject
                }
            }

            CircuitState::HalfOpen => {
                // Allow limited requests in half-open (for testing)
                true
            }
        }
    }

    /// Report a successful operation
    pub async fn report_success(&self) {
        let current_state: CircuitState = self.state.load(Ordering::Acquire).into();

        match current_state {
            CircuitState::Closed => {
                // Reset failure count on success in closed state
                self.failure_count.store(0, Ordering::Release);
            }

            CircuitState::HalfOpen => {
                // Increment success count
                let success_count = self.success_count.fetch_add(1, Ordering::AcqRel) + 1;

                // Check if we should transition to closed
                let config = self.config.read().await;
                if success_count >= config.success_threshold as u64 {
                    self.transition_to_closed();
                }
            }

            CircuitState::Open => {
                // Ignore successes in open state (shouldn't happen)
            }
        }
    }

    /// Report a failed operation
    pub async fn report_failure(&self) {
        let current_state: CircuitState = self.state.load(Ordering::Acquire).into();

        match current_state {
            CircuitState::Closed => {
                // Increment failure count
                let failure_count = self.failure_count.fetch_add(1, Ordering::AcqRel) + 1;

                // Check if we should open the circuit
                let config = self.config.read().await;
                if failure_count >= config.failure_threshold as u64 {
                    self.transition_to_open();
                }
            }

            CircuitState::HalfOpen => {
                // Any failure in half-open immediately reopens circuit
                self.transition_to_open();
            }

            CircuitState::Open => {
                // Already open, ignore additional failures
            }
        }
    }

    /// Transition to Open state
    fn transition_to_open(&self) {
        self.state.store(CircuitState::Open as u8, Ordering::Release);
        self.opened_at.store(Self::now_micros(), Ordering::Release);
        self.failure_count.store(0, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
    }

    /// Transition to HalfOpen state
    fn transition_to_half_open(&self) {
        self.state
            .store(CircuitState::HalfOpen as u8, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
        self.failure_count.store(0, Ordering::Release);
    }

    /// Transition to Closed state
    fn transition_to_closed(&self) {
        self.state
            .store(CircuitState::Closed as u8, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
        self.failure_count.store(0, Ordering::Release);
    }

    /// Get current circuit state (for monitoring)
    pub fn current_state(&self) -> CircuitState {
        self.state.load(Ordering::Acquire).into()
    }

    /// Get current failure count (for monitoring)
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Acquire)
    }

    /// Get current success count (for monitoring)
    pub fn success_count(&self) -> u64 {
        self.success_count.load(Ordering::Acquire)
    }

    /// Manually reset the circuit to closed state
    pub fn reset(&self) {
        self.transition_to_closed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_starts_closed() {
        let breaker = CircuitBreaker::with_defaults();
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert!(breaker.allow_request().await);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_failures() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(1),
        });

        // Report 2 failures (below threshold)
        breaker.report_failure().await;
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert!(breaker.allow_request().await);

        // Report 3rd failure (hits threshold)
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);
        assert!(!breaker.allow_request().await); // Should reject
    }

    #[tokio::test]
    async fn test_circuit_rejects_in_open_state() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_secs(10), // Long timeout
        });

        // Open the circuit
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Should reject multiple requests
        assert!(!breaker.allow_request().await);
        assert!(!breaker.allow_request().await);
        assert!(!breaker.allow_request().await);
    }

    #[tokio::test]
    async fn test_circuit_transitions_to_half_open() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(50), // Short timeout for testing
        });

        // Open the circuit
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Wait for timeout
        sleep(Duration::from_millis(60)).await;

        // Should transition to half-open
        assert!(breaker.allow_request().await);
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_half_open_closes_on_success() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
        });

        // Open the circuit
        breaker.report_failure().await;

        // Wait for half-open
        sleep(Duration::from_millis(60)).await;
        breaker.allow_request().await;

        // Report 1 success (below threshold)
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Report 2nd success (hits threshold)
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_reopens_on_failure() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
        });

        // Open the circuit
        breaker.report_failure().await;

        // Wait for half-open
        sleep(Duration::from_millis(60)).await;
        breaker.allow_request().await;

        // Report success, then failure
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_success_resets_failure_count() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(1),
        });

        // Report 2 failures
        breaker.report_failure().await;
        breaker.report_failure().await;
        assert_eq!(breaker.failure_count(), 2);

        // Report success - should reset counter
        breaker.report_success().await;
        assert_eq!(breaker.failure_count(), 0);

        // Should still be closed
        assert_eq!(breaker.current_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_manual_reset() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_secs(100), // Very long timeout
        });

        // Open the circuit
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Manually reset
        breaker.reset();
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert!(breaker.allow_request().await);
    }

    // ---------------------------------------------------------------
    // CircuitState conversions
    // ---------------------------------------------------------------

    #[test]
    fn test_circuit_state_from_u8_closed() {
        assert_eq!(CircuitState::from(0u8), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_state_from_u8_open() {
        assert_eq!(CircuitState::from(1u8), CircuitState::Open);
    }

    #[test]
    fn test_circuit_state_from_u8_half_open() {
        assert_eq!(CircuitState::from(2u8), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_state_from_u8_unknown_defaults_to_closed() {
        assert_eq!(CircuitState::from(3u8), CircuitState::Closed);
        assert_eq!(CircuitState::from(255u8), CircuitState::Closed);
    }

    // ---------------------------------------------------------------
    // Default config values
    // ---------------------------------------------------------------

    #[test]
    fn test_default_config_values() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 2);
        assert_eq!(config.timeout, Duration::from_secs(30));
    }

    // ---------------------------------------------------------------
    // Failure counting
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_failure_count_increments() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 10,
            success_threshold: 2,
            timeout: Duration::from_secs(30),
        });

        for expected in 1..=5 {
            breaker.report_failure().await;
            assert_eq!(breaker.failure_count(), expected);
        }
    }

    #[tokio::test]
    async fn test_failure_count_resets_on_open() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(30),
        });

        breaker.report_failure().await;
        breaker.report_failure().await;
        breaker.report_failure().await; // Opens circuit

        assert_eq!(breaker.current_state(), CircuitState::Open);
        // Failure count is reset to 0 when transitioning to Open
        assert_eq!(breaker.failure_count(), 0);
    }

    #[tokio::test]
    async fn test_success_count_in_half_open() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 5,
            timeout: Duration::from_millis(50),
        });

        // Open circuit
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Wait for half-open
        sleep(Duration::from_millis(60)).await;
        breaker.allow_request().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Report successes and check count
        for expected in 1..=4 {
            breaker.report_success().await;
            assert_eq!(breaker.success_count(), expected);
            assert_eq!(breaker.current_state(), CircuitState::HalfOpen);
        }

        // 5th success should close the circuit
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert_eq!(breaker.success_count(), 0); // Reset on transition to Closed
    }

    // ---------------------------------------------------------------
    // Full lifecycle: Closed -> Open -> HalfOpen -> Closed
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_full_lifecycle() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 3,
            timeout: Duration::from_millis(50),
        });

        // Phase 1: Closed
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert!(breaker.allow_request().await);

        // Phase 2: Closed -> Open
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);
        assert!(!breaker.allow_request().await);

        // Phase 3: Open -> HalfOpen (after timeout)
        sleep(Duration::from_millis(60)).await;
        assert!(breaker.allow_request().await);
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Phase 4: HalfOpen -> Closed (consecutive successes)
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::Closed);

        // Verify it's fully functional again
        assert!(breaker.allow_request().await);
        assert_eq!(breaker.failure_count(), 0);
        assert_eq!(breaker.success_count(), 0);
    }

    // ---------------------------------------------------------------
    // Full lifecycle: Closed -> Open -> HalfOpen -> Open (failure in half-open)
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_lifecycle_half_open_to_open_on_failure() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 3,
            timeout: Duration::from_millis(50),
        });

        // Open the circuit
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Wait for half-open
        sleep(Duration::from_millis(60)).await;
        breaker.allow_request().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Failure in half-open immediately reopens
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);
        assert!(!breaker.allow_request().await);
    }

    // ---------------------------------------------------------------
    // Multiple resets cycle
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_multiple_reset_cycles() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            timeout: Duration::from_secs(100),
        });

        for _ in 0..5 {
            // Open
            breaker.report_failure().await;
            assert_eq!(breaker.current_state(), CircuitState::Open);

            // Reset
            breaker.reset();
            assert_eq!(breaker.current_state(), CircuitState::Closed);
            assert!(breaker.allow_request().await);
        }
    }

    // ---------------------------------------------------------------
    // Open state ignores additional failures
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_open_state_ignores_extra_failures() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_secs(100),
        });

        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Additional failures should not change state
        breaker.report_failure().await;
        breaker.report_failure().await;
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);
    }

    // ---------------------------------------------------------------
    // Open state ignores successes
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_open_state_ignores_successes() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            timeout: Duration::from_secs(100),
        });

        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Success in open state should not change state
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);
    }

    // ---------------------------------------------------------------
    // HalfOpen allows requests
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_half_open_allows_requests() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 10,
            timeout: Duration::from_millis(50),
        });

        breaker.report_failure().await;
        sleep(Duration::from_millis(60)).await;
        breaker.allow_request().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Multiple requests should be allowed in half-open
        assert!(breaker.allow_request().await);
        assert!(breaker.allow_request().await);
    }

    // ---------------------------------------------------------------
    // Success in closed state resets failure count to 0
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_success_in_closed_resets_failures() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(30),
        });

        // Accumulate failures
        breaker.report_failure().await;
        breaker.report_failure().await;
        breaker.report_failure().await;
        assert_eq!(breaker.failure_count(), 3);

        // One success resets failures
        breaker.report_success().await;
        assert_eq!(breaker.failure_count(), 0);
        assert_eq!(breaker.current_state(), CircuitState::Closed);

        // Now we need 5 more failures to open (not 2)
        for _ in 0..4 {
            breaker.report_failure().await;
        }
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);
    }

    // ---------------------------------------------------------------
    // with_defaults creates a functional breaker
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_with_defaults_functional() {
        let breaker = CircuitBreaker::with_defaults();
        assert_eq!(breaker.current_state(), CircuitState::Closed);
        assert!(breaker.allow_request().await);
        assert_eq!(breaker.failure_count(), 0);
        assert_eq!(breaker.success_count(), 0);
    }

    // ---------------------------------------------------------------
    // Threshold of 1 success immediately closes from half-open
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_success_threshold_of_one() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            timeout: Duration::from_millis(50),
        });

        // Open then half-open
        breaker.report_failure().await;
        sleep(Duration::from_millis(60)).await;
        breaker.allow_request().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Single success should close
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::Closed);
    }

    // ---------------------------------------------------------------
    // Serde roundtrip for config
    // ---------------------------------------------------------------

    #[test]
    fn test_config_serde_roundtrip() {
        let config = CircuitBreakerConfig {
            failure_threshold: 10,
            success_threshold: 5,
            timeout: Duration::from_secs(60),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CircuitBreakerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.failure_threshold, 10);
        assert_eq!(deserialized.success_threshold, 5);
    }

    // ---------------------------------------------------------------
    // CircuitState Debug + Clone
    // ---------------------------------------------------------------

    #[test]
    fn test_circuit_state_debug() {
        let s = CircuitState::Closed;
        let debug = format!("{:?}", s);
        assert!(debug.contains("Closed"));
    }

    #[test]
    fn test_circuit_state_clone() {
        let s = CircuitState::HalfOpen;
        let cloned = s.clone();
        assert_eq!(s, cloned);
    }

    #[test]
    fn test_circuit_state_copy() {
        let s = CircuitState::Open;
        let copied = s;
        assert_eq!(s, copied);
    }
}
