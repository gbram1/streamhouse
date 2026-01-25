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
        let backoff_ms = self.initial_backoff.as_millis() as f64
            * self.backoff_multiplier.powi(attempt as i32);
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
                    debug!(
                        attempt = attempt + 1,
                        "Operation succeeded after retry"
                    );
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
                    debug!(
                        attempt = attempt + 1,
                        "Operation succeeded after retry"
                    );
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
                let jittered_backoff = Duration::from_millis(
                    (base_backoff.as_millis() as f64 * jitter) as u64,
                );

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

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_backoff, Duration::from_millis(100));
        assert_eq!(policy.max_backoff, Duration::from_secs(30));
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_backoff_calculation() {
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

        // Should cap at max_backoff (10s)
        assert_eq!(policy.backoff(0), Duration::from_secs(1));
        assert_eq!(policy.backoff(1), Duration::from_secs(2));
        assert_eq!(policy.backoff(2), Duration::from_secs(4));
        assert_eq!(policy.backoff(3), Duration::from_secs(8));
        assert_eq!(policy.backoff(4), Duration::from_secs(10)); // Capped
        assert_eq!(policy.backoff(5), Duration::from_secs(10)); // Capped
    }

    #[test]
    fn test_is_retryable() {
        let policy = RetryPolicy::default();

        // Retryable
        assert!(policy.is_retryable(&Status::unavailable("agent down")));
        assert!(policy.is_retryable(&Status::deadline_exceeded("timeout")));
        assert!(policy.is_retryable(&Status::resource_exhausted("at capacity")));
        assert!(policy.is_retryable(&Status::internal("internal error")));

        // Non-retryable
        assert!(!policy.is_retryable(&Status::not_found("no lease")));
        assert!(!policy.is_retryable(&Status::failed_precondition("lease expired")));
        assert!(!policy.is_retryable(&Status::invalid_argument("bad request")));
        assert!(!policy.is_retryable(&Status::unauthenticated("invalid creds")));
        assert!(!policy.is_retryable(&Status::permission_denied("no access")));
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success() {
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
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // Should succeed first try
    }

    #[tokio::test]
    async fn test_retry_with_backoff_eventual_success() {
        let policy = RetryPolicy::default();
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
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // Fails twice, succeeds third time
    }

    #[tokio::test]
    async fn test_retry_with_backoff_non_retryable() {
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
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // Should not retry
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
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // Initial + 2 retries
    }
}
