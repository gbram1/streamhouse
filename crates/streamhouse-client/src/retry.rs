//! Retry Logic with Exponential Backoff
//!
//! This module implements retry logic for handling transient failures when sending
//! records to StreamHouse agents via gRPC.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐
//! │  send(...)   │ Producer API
//! └──────┬───────┘
//!        │
//!        ▼
//! ┌──────────────────────────────┐
//! │  RetryPolicy                 │
//! │  - max_retries: 5            │
//! │  - initial_backoff: 100ms    │
//! │  - max_backoff: 30s          │
//! │  - backoff_multiplier: 2.0   │
//! └──────┬───────────────────────┘
//!        │
//!        ├─→ Attempt 1: Immediate
//!        ├─→ Attempt 2: Wait 100ms (backoff)
//!        ├─→ Attempt 3: Wait 200ms (backoff * 2)
//!        ├─→ Attempt 4: Wait 400ms (backoff * 4)
//!        ├─→ Attempt 5: Wait 800ms (backoff * 8)
//!        └─→ Attempt 6: Wait 1.6s  (backoff * 16)
//! ```
//!
//! ## Retryable vs Non-Retryable Errors
//!
//! **Retryable** (transient failures):
//! - `UNAVAILABLE`: Agent temporarily down, restarting, or overloaded
//! - `DEADLINE_EXCEEDED`: Request timeout (network congestion)
//! - `RESOURCE_EXHAUSTED`: Agent at capacity (backpressure)
//! - `INTERNAL`: Temporary internal error
//!
//! **Non-Retryable** (permanent failures):
//! - `NOT_FOUND`: Agent doesn't hold lease for partition (route to new agent)
//! - `FAILED_PRECONDITION`: Lease expired (route to new agent)
//! - `INVALID_ARGUMENT`: Bad request (won't succeed on retry)
//! - `UNAUTHENTICATED`: Invalid credentials (won't succeed on retry)
//! - `PERMISSION_DENIED`: Authorization failure (won't succeed on retry)
//!
//! ## Performance
//!
//! Retry logic ensures high availability without impacting performance:
//! - Most requests succeed on first attempt (no retry overhead)
//! - Exponential backoff prevents thundering herd on agent recovery
//! - Configurable limits prevent infinite retry loops
//!
//! ## Examples
//!
//! ```ignore
//! use streamhouse_client::retry::{RetryPolicy, retry_with_backoff};
//!
//! let policy = RetryPolicy::default(); // 5 retries, 100ms-30s backoff
//!
//! let result = retry_with_backoff(&policy, || async {
//!     send_to_agent(records).await
//! }).await?;
//! ```

use std::time::Duration;
use tokio::time::sleep;
use tonic::Status;
use tracing::{debug, warn};

/// Retry policy configuration for exponential backoff.
///
/// # Fields
///
/// * `max_retries` - Maximum number of retry attempts (default: 5)
/// * `initial_backoff` - Initial backoff duration (default: 100ms)
/// * `max_backoff` - Maximum backoff duration (default: 30s)
/// * `backoff_multiplier` - Backoff multiplier for exponential growth (default: 2.0)
///
/// # Backoff Calculation
///
/// ```text
/// backoff = min(initial_backoff * multiplier^attempt, max_backoff)
///
/// Example with defaults (100ms initial, 2x multiplier, 30s max):
/// - Attempt 1: 100ms
/// - Attempt 2: 200ms
/// - Attempt 3: 400ms
/// - Attempt 4: 800ms
/// - Attempt 5: 1.6s
/// - Attempt 6+: capped at 30s
/// ```
///
/// # Performance Tuning
///
/// - **Low latency**: Decrease `initial_backoff` to 10ms (faster retries)
/// - **High availability**: Increase `max_retries` to 10+ (more attempts)
/// - **Rate limiting**: Increase `backoff_multiplier` to 3.0+ (longer waits between attempts)
///
/// # Examples
///
/// ```ignore
/// // Default policy (balanced)
/// let policy = RetryPolicy::default();
///
/// // Aggressive retries (low latency)
/// let policy = RetryPolicy {
///     max_retries: 10,
///     initial_backoff: Duration::from_millis(10),
///     max_backoff: Duration::from_secs(10),
///     backoff_multiplier: 1.5,
/// };
///
/// // Conservative retries (prevent overload)
/// let policy = RetryPolicy {
///     max_retries: 3,
///     initial_backoff: Duration::from_millis(500),
///     max_backoff: Duration::from_secs(60),
///     backoff_multiplier: 3.0,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_retries: usize,

    /// Initial backoff duration
    pub initial_backoff: Duration,

    /// Maximum backoff duration
    pub max_backoff: Duration,

    /// Backoff multiplier for exponential growth
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    /// Create a default retry policy.
    ///
    /// # Returns
    ///
    /// RetryPolicy with:
    /// - max_retries: 5
    /// - initial_backoff: 100ms
    /// - max_backoff: 30s
    /// - backoff_multiplier: 2.0
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy with custom settings.
    ///
    /// # Arguments
    ///
    /// * `max_retries` - Maximum number of retry attempts
    /// * `initial_backoff` - Initial backoff duration
    /// * `max_backoff` - Maximum backoff duration
    /// * `backoff_multiplier` - Backoff multiplier for exponential growth
    ///
    /// # Returns
    ///
    /// A new `RetryPolicy` with the specified configuration.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let policy = RetryPolicy::new(
    ///     10,
    ///     Duration::from_millis(50),
    ///     Duration::from_secs(60),
    ///     2.5,
    /// );
    /// ```
    pub fn new(
        max_retries: usize,
        initial_backoff: Duration,
        max_backoff: Duration,
        backoff_multiplier: f64,
    ) -> Self {
        Self {
            max_retries,
            initial_backoff,
            max_backoff,
            backoff_multiplier,
        }
    }

    /// Calculate backoff duration for a given attempt number.
    ///
    /// # Arguments
    ///
    /// * `attempt` - Attempt number (0-indexed)
    ///
    /// # Returns
    ///
    /// Backoff duration = min(initial_backoff * multiplier^attempt, max_backoff)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let policy = RetryPolicy::default();
    /// assert_eq!(policy.backoff(0), Duration::from_millis(100));
    /// assert_eq!(policy.backoff(1), Duration::from_millis(200));
    /// assert_eq!(policy.backoff(2), Duration::from_millis(400));
    /// ```
    pub fn backoff(&self, attempt: usize) -> Duration {
        let backoff_ms =
            self.initial_backoff.as_millis() as f64 * self.backoff_multiplier.powi(attempt as i32);
        let backoff = Duration::from_millis(backoff_ms as u64);
        backoff.min(self.max_backoff)
    }

    /// Check if an error is retryable.
    ///
    /// # Arguments
    ///
    /// * `status` - gRPC Status error
    ///
    /// # Returns
    ///
    /// `true` if the error is retryable (transient), `false` if permanent.
    ///
    /// # Retryable Errors
    ///
    /// - `UNAVAILABLE`: Agent temporarily down
    /// - `DEADLINE_EXCEEDED`: Request timeout
    /// - `RESOURCE_EXHAUSTED`: Agent at capacity
    /// - `INTERNAL`: Temporary internal error
    ///
    /// # Non-Retryable Errors
    ///
    /// - `NOT_FOUND`: Agent doesn't hold lease
    /// - `FAILED_PRECONDITION`: Lease expired
    /// - `INVALID_ARGUMENT`: Bad request
    /// - `UNAUTHENTICATED`: Invalid credentials
    /// - `PERMISSION_DENIED`: Authorization failure
    /// - All other error codes
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let policy = RetryPolicy::default();
    ///
    /// let unavailable = Status::unavailable("agent down");
    /// assert!(policy.is_retryable(&unavailable));
    ///
    /// let not_found = Status::not_found("no lease");
    /// assert!(!policy.is_retryable(&not_found));
    /// ```
    pub fn is_retryable(&self, status: &Status) -> bool {
        use tonic::Code;

        match status.code() {
            // Retryable: transient failures
            Code::Unavailable => true,       // Agent down/restarting
            Code::DeadlineExceeded => true,  // Timeout (network congestion)
            Code::ResourceExhausted => true, // Agent at capacity
            Code::Internal => true,          // Temporary internal error

            // Non-retryable: permanent failures
            Code::NotFound => false,           // Agent doesn't hold lease
            Code::FailedPrecondition => false, // Lease expired
            Code::InvalidArgument => false,    // Bad request
            Code::Unauthenticated => false,    // Invalid credentials
            Code::PermissionDenied => false,   // Authorization failure

            // All other codes: non-retryable
            _ => false,
        }
    }
}

/// Retry a gRPC operation with exponential backoff.
///
/// # Arguments
///
/// * `policy` - Retry policy configuration
/// * `operation` - Async operation to retry
///
/// # Returns
///
/// - `Ok(T)` if operation succeeds within max_retries
/// - `Err(Status)` if all retries exhausted or non-retryable error
///
/// # Behavior
///
/// 1. Try operation
/// 2. If success, return result
/// 3. If error is non-retryable, return error immediately
/// 4. If error is retryable and retries remaining:
///    - Calculate backoff duration
///    - Sleep for backoff duration
///    - Retry operation
/// 5. If all retries exhausted, return last error
///
/// # Examples
///
/// ```ignore
/// use streamhouse_client::retry::{RetryPolicy, retry_with_backoff};
///
/// let policy = RetryPolicy::default();
///
/// let result = retry_with_backoff(&policy, || async {
///     send_to_agent(records).await
/// }).await?;
/// ```
pub async fn retry_with_backoff<F, Fut, T>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T, Status>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, Status>>,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(attempt = attempt + 1, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(status) => {
                // Check if error is retryable
                if !policy.is_retryable(&status) {
                    warn!(
                        code = ?status.code(),
                        message = %status.message(),
                        "Non-retryable error, giving up"
                    );
                    return Err(status);
                }

                // Check if we've exhausted retries
                if attempt >= policy.max_retries {
                    warn!(
                        attempt = attempt + 1,
                        max_retries = policy.max_retries,
                        code = ?status.code(),
                        message = %status.message(),
                        "Max retries exhausted, giving up"
                    );
                    return Err(status);
                }

                // Calculate backoff and retry
                let backoff = policy.backoff(attempt);
                warn!(
                    attempt = attempt + 1,
                    max_retries = policy.max_retries,
                    backoff_ms = backoff.as_millis(),
                    code = ?status.code(),
                    message = %status.message(),
                    "Retryable error, backing off"
                );

                sleep(backoff).await;
                attempt += 1;
            }
        }
    }
}

/// Retry a gRPC operation with jittered exponential backoff.
///
/// # Arguments
///
/// * `policy` - Retry policy configuration
/// * `operation` - Async operation to retry
///
/// # Returns
///
/// - `Ok(T)` if operation succeeds within max_retries
/// - `Err(Status)` if all retries exhausted or non-retryable error
///
/// # Jitter
///
/// Adds random jitter (±25%) to backoff duration to prevent thundering herd.
/// This is useful when many clients retry simultaneously after an agent failure.
///
/// ```text
/// jittered_backoff = backoff * (0.75 + random(0.0, 0.5))
/// ```
///
/// # Examples
///
/// ```ignore
/// use streamhouse_client::retry::{RetryPolicy, retry_with_jittered_backoff};
///
/// let policy = RetryPolicy::default();
///
/// let result = retry_with_jittered_backoff(&policy, || async {
///     send_to_agent(records).await
/// }).await?;
/// ```
pub async fn retry_with_jittered_backoff<F, Fut, T>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T, Status>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, Status>>,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(attempt = attempt + 1, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(status) => {
                // Check if error is retryable
                if !policy.is_retryable(&status) {
                    warn!(
                        code = ?status.code(),
                        message = %status.message(),
                        "Non-retryable error, giving up"
                    );
                    return Err(status);
                }

                // Check if we've exhausted retries
                if attempt >= policy.max_retries {
                    warn!(
                        attempt = attempt + 1,
                        max_retries = policy.max_retries,
                        code = ?status.code(),
                        message = %status.message(),
                        "Max retries exhausted, giving up"
                    );
                    return Err(status);
                }

                // Calculate backoff with jitter
                let base_backoff = policy.backoff(attempt);
                let jitter = 0.75 + (rand::random::<f64>() * 0.5); // 0.75-1.25x
                let jittered_backoff =
                    Duration::from_millis((base_backoff.as_millis() as f64 * jitter) as u64);

                warn!(
                    attempt = attempt + 1,
                    max_retries = policy.max_retries,
                    backoff_ms = jittered_backoff.as_millis(),
                    code = ?status.code(),
                    message = %status.message(),
                    "Retryable error, backing off with jitter"
                );

                sleep(jittered_backoff).await;
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ========================================================================
    // RetryPolicy - default values
    // ========================================================================

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_backoff, Duration::from_millis(100));
        assert_eq!(policy.max_backoff, Duration::from_secs(30));
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_retry_policy_new_custom() {
        let policy = RetryPolicy::new(
            10,
            Duration::from_millis(50),
            Duration::from_secs(60),
            3.0,
        );
        assert_eq!(policy.max_retries, 10);
        assert_eq!(policy.initial_backoff, Duration::from_millis(50));
        assert_eq!(policy.max_backoff, Duration::from_secs(60));
        assert_eq!(policy.backoff_multiplier, 3.0);
    }

    #[test]
    fn test_retry_policy_clone() {
        let policy = RetryPolicy::new(
            7,
            Duration::from_millis(200),
            Duration::from_secs(45),
            2.5,
        );
        let cloned = policy.clone();
        assert_eq!(cloned.max_retries, 7);
        assert_eq!(cloned.initial_backoff, Duration::from_millis(200));
        assert_eq!(cloned.max_backoff, Duration::from_secs(45));
        assert_eq!(cloned.backoff_multiplier, 2.5);
    }

    #[test]
    fn test_retry_policy_zero_retries() {
        let policy = RetryPolicy::new(
            0,
            Duration::from_millis(100),
            Duration::from_secs(30),
            2.0,
        );
        assert_eq!(policy.max_retries, 0);
    }

    // ========================================================================
    // RetryPolicy - backoff calculation
    // ========================================================================

    #[test]
    fn test_backoff_exponential_growth_default() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.backoff(0), Duration::from_millis(100));
        assert_eq!(policy.backoff(1), Duration::from_millis(200));
        assert_eq!(policy.backoff(2), Duration::from_millis(400));
        assert_eq!(policy.backoff(3), Duration::from_millis(800));
        assert_eq!(policy.backoff(4), Duration::from_millis(1600));
    }

    #[test]
    fn test_backoff_max_cap() {
        let policy = RetryPolicy {
            max_retries: 10,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        };

        assert_eq!(policy.backoff(0), Duration::from_secs(1));
        assert_eq!(policy.backoff(1), Duration::from_secs(2));
        assert_eq!(policy.backoff(2), Duration::from_secs(4));
        assert_eq!(policy.backoff(3), Duration::from_secs(8));
        assert_eq!(policy.backoff(4), Duration::from_secs(10)); // Capped
        assert_eq!(policy.backoff(5), Duration::from_secs(10)); // Capped
        assert_eq!(policy.backoff(100), Duration::from_secs(10)); // Still capped
    }

    #[test]
    fn test_backoff_with_multiplier_3() {
        let policy = RetryPolicy::new(
            10,
            Duration::from_millis(100),
            Duration::from_secs(60),
            3.0,
        );

        assert_eq!(policy.backoff(0), Duration::from_millis(100));   // 100 * 3^0
        assert_eq!(policy.backoff(1), Duration::from_millis(300));   // 100 * 3^1
        assert_eq!(policy.backoff(2), Duration::from_millis(900));   // 100 * 3^2
        assert_eq!(policy.backoff(3), Duration::from_millis(2700));  // 100 * 3^3
    }

    #[test]
    fn test_backoff_with_multiplier_1_no_growth() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(500),
            Duration::from_secs(60),
            1.0,
        );

        // Multiplier of 1.0 means constant backoff
        assert_eq!(policy.backoff(0), Duration::from_millis(500));
        assert_eq!(policy.backoff(1), Duration::from_millis(500));
        assert_eq!(policy.backoff(2), Duration::from_millis(500));
        assert_eq!(policy.backoff(3), Duration::from_millis(500));
    }

    #[test]
    fn test_backoff_with_fractional_multiplier() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(100),
            Duration::from_secs(60),
            1.5,
        );

        assert_eq!(policy.backoff(0), Duration::from_millis(100));  // 100 * 1.5^0 = 100
        assert_eq!(policy.backoff(1), Duration::from_millis(150));  // 100 * 1.5^1 = 150
        assert_eq!(policy.backoff(2), Duration::from_millis(225));  // 100 * 1.5^2 = 225
    }

    #[test]
    fn test_backoff_attempt_zero() {
        let policy = RetryPolicy::default();
        // Attempt 0 should return initial_backoff (multiplier^0 = 1)
        assert_eq!(policy.backoff(0), policy.initial_backoff);
    }

    #[test]
    fn test_backoff_very_large_attempt_stays_capped() {
        let policy = RetryPolicy::default();
        // Even with a very large attempt number, should cap at max_backoff
        let backoff = policy.backoff(1000);
        assert!(backoff <= policy.max_backoff);
    }

    #[test]
    fn test_backoff_never_exceeds_max() {
        let policy = RetryPolicy::new(
            20,
            Duration::from_millis(10),
            Duration::from_millis(500),
            2.0,
        );

        for attempt in 0..20 {
            assert!(policy.backoff(attempt) <= Duration::from_millis(500));
        }
    }

    #[test]
    fn test_backoff_small_initial_and_max() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(1),
            Duration::from_millis(5),
            2.0,
        );

        assert_eq!(policy.backoff(0), Duration::from_millis(1));
        assert_eq!(policy.backoff(1), Duration::from_millis(2));
        assert_eq!(policy.backoff(2), Duration::from_millis(4));
        assert_eq!(policy.backoff(3), Duration::from_millis(5)); // capped
    }

    // ========================================================================
    // RetryPolicy - is_retryable
    // ========================================================================

    #[test]
    fn test_is_retryable_unavailable() {
        let policy = RetryPolicy::default();
        assert!(policy.is_retryable(&Status::unavailable("agent down")));
    }

    #[test]
    fn test_is_retryable_deadline_exceeded() {
        let policy = RetryPolicy::default();
        assert!(policy.is_retryable(&Status::deadline_exceeded("timeout")));
    }

    #[test]
    fn test_is_retryable_resource_exhausted() {
        let policy = RetryPolicy::default();
        assert!(policy.is_retryable(&Status::resource_exhausted("at capacity")));
    }

    #[test]
    fn test_is_retryable_internal() {
        let policy = RetryPolicy::default();
        assert!(policy.is_retryable(&Status::internal("internal error")));
    }

    #[test]
    fn test_not_retryable_not_found() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::not_found("no lease")));
    }

    #[test]
    fn test_not_retryable_failed_precondition() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::failed_precondition("lease expired")));
    }

    #[test]
    fn test_not_retryable_invalid_argument() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::invalid_argument("bad request")));
    }

    #[test]
    fn test_not_retryable_unauthenticated() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::unauthenticated("invalid creds")));
    }

    #[test]
    fn test_not_retryable_permission_denied() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::permission_denied("no access")));
    }

    #[test]
    fn test_not_retryable_ok() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::ok("ok")));
    }

    #[test]
    fn test_not_retryable_cancelled() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::cancelled("cancelled")));
    }

    #[test]
    fn test_not_retryable_already_exists() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::already_exists("already exists")));
    }

    #[test]
    fn test_not_retryable_unimplemented() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::unimplemented("not implemented")));
    }

    #[test]
    fn test_not_retryable_data_loss() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::data_loss("data lost")));
    }

    #[test]
    fn test_not_retryable_out_of_range() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::out_of_range("out of range")));
    }

    #[test]
    fn test_not_retryable_aborted() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::aborted("aborted")));
    }

    #[test]
    fn test_not_retryable_unknown() {
        let policy = RetryPolicy::default();
        assert!(!policy.is_retryable(&Status::unknown("unknown")));
    }

    #[test]
    fn test_is_retryable_preserves_message() {
        let policy = RetryPolicy::default();
        let status = Status::unavailable("custom agent down message");
        assert!(policy.is_retryable(&status));
        assert_eq!(status.message(), "custom agent down message");
    }

    #[test]
    fn test_is_retryable_independent_of_policy_config() {
        // is_retryable should not depend on retry count or backoff settings
        let aggressive = RetryPolicy::new(100, Duration::from_nanos(1), Duration::from_nanos(1), 1.0);
        let conservative = RetryPolicy::new(0, Duration::from_secs(60), Duration::from_secs(3600), 10.0);

        let unavailable = Status::unavailable("down");
        let not_found = Status::not_found("missing");

        assert!(aggressive.is_retryable(&unavailable));
        assert!(conservative.is_retryable(&unavailable));
        assert!(!aggressive.is_retryable(&not_found));
        assert!(!conservative.is_retryable(&not_found));
    }

    // ========================================================================
    // retry_with_backoff - async tests
    // ========================================================================

    #[tokio::test]
    async fn test_retry_with_backoff_immediate_success() {
        let policy = RetryPolicy::default();
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, Status>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_eventual_success() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(Status::unavailable("agent down"))
                } else {
                    Ok::<i32, Status>(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_non_retryable_immediate_fail() {
        let policy = RetryPolicy::default();
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, Status>(Status::not_found("no lease"))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_exhausted() {
        let policy = RetryPolicy {
            max_retries: 2,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            backoff_multiplier: 2.0,
        };
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, Status>(Status::unavailable("agent down"))
            }
        })
        .await;

        assert!(result.is_err());
        // Initial attempt + 2 retries = 3 total
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_zero_retries() {
        let policy = RetryPolicy::new(
            0,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, Status>(Status::unavailable("agent down"))
            }
        })
        .await;

        assert!(result.is_err());
        // Zero retries means only 1 attempt
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_preserves_error_code() {
        let policy = RetryPolicy::new(
            1,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );

        let result = retry_with_backoff(&policy, || async {
            Err::<i32, Status>(Status::unavailable("still down"))
        })
        .await;

        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unavailable);
        assert_eq!(err.message(), "still down");
    }

    #[tokio::test]
    async fn test_retry_with_backoff_preserves_error_message_on_non_retryable() {
        let policy = RetryPolicy::default();

        let result = retry_with_backoff(&policy, || async {
            Err::<i32, Status>(Status::invalid_argument("field 'topic' is required"))
        })
        .await;

        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert_eq!(err.message(), "field 'topic' is required");
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success_on_last_attempt() {
        let policy = RetryPolicy::new(
            3,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 3 {
                    Err(Status::unavailable("down"))
                } else {
                    Ok::<&str, Status>("recovered")
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(attempts.load(Ordering::SeqCst), 4); // 1 initial + 3 retries
    }

    #[tokio::test]
    async fn test_retry_with_backoff_different_retryable_errors() {
        let policy = RetryPolicy::new(
            4,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        // Different retryable errors on each attempt
        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                match count {
                    0 => Err(Status::unavailable("down")),
                    1 => Err(Status::deadline_exceeded("timeout")),
                    2 => Err(Status::resource_exhausted("full")),
                    _ => Ok::<&str, Status>("success"),
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_non_retryable_after_retries() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        // First retryable, then non-retryable
        let result = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(Status::unavailable("down"))
                } else {
                    Err(Status::permission_denied("no access"))
                }
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::PermissionDenied);
        // Stops immediately on non-retryable, doesn't exhaust max_retries
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_returns_different_types() {
        let policy = RetryPolicy::new(
            1,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );

        // Test with Vec
        let result = retry_with_backoff(&policy, || async {
            Ok::<Vec<u8>, Status>(vec![1, 2, 3])
        })
        .await;
        assert_eq!(result.unwrap(), vec![1, 2, 3]);

        // Test with String
        let result = retry_with_backoff(&policy, || async {
            Ok::<String, Status>("hello".to_string())
        })
        .await;
        assert_eq!(result.unwrap(), "hello");

        // Test with unit
        let result = retry_with_backoff(&policy, || async {
            Ok::<(), Status>(())
        })
        .await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // retry_with_jittered_backoff - async tests
    // ========================================================================

    #[tokio::test]
    async fn test_retry_with_jitter_immediate_success() {
        let policy = RetryPolicy::default();
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, Status>(99)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 99);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_jitter_eventual_success() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 3 {
                    Err(Status::internal("error"))
                } else {
                    Ok::<&str, Status>("ok")
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_retry_with_jitter_non_retryable_immediate_fail() {
        let policy = RetryPolicy::default();
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, Status>(Status::invalid_argument("bad field"))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_jitter_exhausted() {
        let policy = RetryPolicy::new(
            2,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, Status>(Status::unavailable("agent down"))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // 1 initial + 2 retries
    }

    #[tokio::test]
    async fn test_retry_with_jitter_zero_retries() {
        let policy = RetryPolicy::new(
            0,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), Status>(Status::unavailable("down"))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_jitter_preserves_last_error() {
        let policy = RetryPolicy::new(
            2,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, Status>(Status::unavailable(format!("attempt {}", count)))
            }
        })
        .await;

        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unavailable);
        assert_eq!(err.message(), "attempt 2"); // last attempt's message
    }

    #[tokio::test]
    async fn test_retry_with_jitter_switches_to_non_retryable_stops() {
        let policy = RetryPolicy::new(
            10,
            Duration::from_millis(1),
            Duration::from_millis(10),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Err(Status::unavailable("transient"))
                } else {
                    Err(Status::not_found("permanent"))
                }
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
        assert_eq!(attempts.load(Ordering::SeqCst), 2); // Did not exhaust 10 retries
    }

    // ========================================================================
    // Timing validation (coarse checks -- we verify retries actually wait)
    // ========================================================================

    #[tokio::test]
    async fn test_retry_backoff_actually_waits() {
        let policy = RetryPolicy::new(
            1,
            Duration::from_millis(50),
            Duration::from_millis(200),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let start = tokio::time::Instant::now();
        let _ = retry_with_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), Status>(Status::unavailable("down"))
            }
        })
        .await;

        let elapsed = start.elapsed();
        // With 1 retry and 50ms initial backoff, should wait at least ~50ms
        assert!(
            elapsed >= Duration::from_millis(40),
            "Expected at least ~50ms delay, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_retry_jitter_actually_waits() {
        let policy = RetryPolicy::new(
            1,
            Duration::from_millis(50),
            Duration::from_millis(200),
            2.0,
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let start = tokio::time::Instant::now();
        let _ = retry_with_jittered_backoff(&policy, || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), Status>(Status::unavailable("down"))
            }
        })
        .await;

        let elapsed = start.elapsed();
        // Jitter range is 0.75-1.25x of 50ms = 37.5ms - 62.5ms, allow some slack
        assert!(
            elapsed >= Duration::from_millis(25),
            "Expected at least ~37ms delay with jitter, got {:?}",
            elapsed
        );
    }
}
