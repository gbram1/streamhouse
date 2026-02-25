//! Credit-Based Backpressure System
//!
//! This module implements an end-to-end credit-based backpressure mechanism for
//! controlling producer write rates. When the storage layer (WAL, S3 flushing)
//! falls behind, credits are exhausted and producers are throttled.
//!
//! ## Why Backpressure?
//!
//! Without backpressure, a fast producer can overwhelm the system:
//! - **WAL grows unbounded**: Memory pressure and slow recovery
//! - **S3 flush queue backs up**: Data loss window increases
//! - **Agent OOM kill**: Catastrophic failure
//!
//! With credit-based backpressure:
//! - Producers are throttled before memory is exhausted
//! - Steady-state: credits refill at the rate the system can flush
//! - Burst: initial credits absorb transient spikes
//! - Fairness: per-producer tracking prevents one producer from starving others
//!
//! ## Algorithm
//!
//! Each producer gets `initial_credits` on first request. Each produce request
//! consumes credits equal to the number of records (or a minimum of
//! `min_credits_per_request`). Credits refill over time at `refill_rate_per_sec`,
//! capped at `max_credits`.
//!
//! When a producer runs out of credits, the controller returns the number of
//! milliseconds the producer should wait before retrying.
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::backpressure::{BackpressureController, BackpressureConfig};
//!
//! let controller = BackpressureController::new(BackpressureConfig::default());
//!
//! // Before accepting a produce request
//! match controller.acquire_credits("producer-1", 100).await {
//!     Ok(()) => { /* proceed with write */ }
//!     Err(wait_ms) => {
//!         // Tell producer to retry after wait_ms
//!     }
//! }
//!
//! // After successful S3 flush, release credits back
//! controller.release_credits("producer-1", 100).await;
//! ```

use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::time::Instant;

/// Configuration for the backpressure controller.
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// Credits granted to a new producer on first request.
    pub initial_credits: u64,
    /// Maximum credits a producer can accumulate.
    pub max_credits: u64,
    /// Rate at which credits refill per second.
    pub refill_rate_per_sec: u64,
    /// Minimum credits consumed per request (even for a single record).
    pub min_credits_per_request: u64,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            initial_credits: 10_000,
            max_credits: 100_000,
            refill_rate_per_sec: 5_000,
            min_credits_per_request: 1,
        }
    }
}

/// Per-producer credit tracking.
#[derive(Debug, Clone)]
pub struct ProducerCredits {
    /// Currently available credits.
    pub available: u64,
    /// Total credits ever granted (initial + refills).
    pub total_granted: u64,
    /// Total credits ever consumed.
    pub total_consumed: u64,
    /// Last time credits were refilled.
    pub last_refill: Instant,
    /// Number of times this producer has been throttled (insufficient credits).
    pub throttle_count: u64,
}

impl ProducerCredits {
    fn new(initial_credits: u64) -> Self {
        Self {
            available: initial_credits,
            total_granted: initial_credits,
            total_consumed: 0,
            last_refill: Instant::now(),
            throttle_count: 0,
        }
    }

    #[cfg(test)]
    fn new_with_instant(initial_credits: u64, now: Instant) -> Self {
        Self {
            available: initial_credits,
            total_granted: initial_credits,
            total_consumed: 0,
            last_refill: now,
            throttle_count: 0,
        }
    }
}

/// Credit-based backpressure controller for producer write throttling.
pub struct BackpressureController {
    /// Per-producer credit tracking.
    credits: RwLock<HashMap<String, ProducerCredits>>,
    /// Controller configuration.
    config: BackpressureConfig,
}

impl BackpressureController {
    /// Create a new backpressure controller with the given configuration.
    pub fn new(config: BackpressureConfig) -> Self {
        Self {
            credits: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Try to acquire credits for a produce request.
    ///
    /// If sufficient credits are available, they are consumed and `Ok(())` is returned.
    /// If insufficient credits are available, `Err(wait_ms)` is returned indicating
    /// how long the producer should wait before retrying.
    ///
    /// New producers are automatically initialized with `initial_credits`.
    /// Credits are refilled based on elapsed time since last refill.
    ///
    /// # Arguments
    ///
    /// * `producer_id` - Unique producer identifier
    /// * `record_count` - Number of records (credits) to acquire
    ///
    /// # Returns
    ///
    /// - `Ok(())` if credits were successfully acquired
    /// - `Err(wait_ms)` if the producer should wait `wait_ms` milliseconds
    pub async fn acquire_credits(
        &self,
        producer_id: &str,
        record_count: u64,
    ) -> std::result::Result<(), u64> {
        let cost = record_count.max(self.config.min_credits_per_request);
        let mut credits = self.credits.write().await;

        let entry = credits
            .entry(producer_id.to_string())
            .or_insert_with(|| ProducerCredits::new(self.config.initial_credits));

        // Refill credits based on elapsed time
        self.refill_entry(entry);

        if entry.available >= cost {
            entry.available -= cost;
            entry.total_consumed += cost;
            Ok(())
        } else {
            entry.throttle_count += 1;
            let deficit = cost - entry.available;
            let wait_ms = self.get_throttle_time_ms_for_deficit(deficit);
            Err(wait_ms)
        }
    }

    /// Release credits back to a producer.
    ///
    /// Called after a successful S3 flush or WAL truncation to restore credits.
    /// The available credits are capped at `max_credits`.
    ///
    /// # Arguments
    ///
    /// * `producer_id` - Unique producer identifier
    /// * `amount` - Number of credits to release
    pub async fn release_credits(&self, producer_id: &str, amount: u64) {
        let mut credits = self.credits.write().await;
        if let Some(entry) = credits.get_mut(producer_id) {
            entry.available = (entry.available + amount).min(self.config.max_credits);
            entry.total_granted += amount;
        }
    }

    /// Refill credits for all producers based on elapsed time.
    ///
    /// This should be called periodically (e.g., every second) by a background task
    /// to ensure producers get credits even when not actively producing.
    pub async fn refill(&self) {
        let mut credits = self.credits.write().await;
        for entry in credits.values_mut() {
            self.refill_entry(entry);
        }
    }

    /// Calculate the throttle wait time in milliseconds for a given credit deficit.
    ///
    /// # Arguments
    ///
    /// * `deficit` - Number of credits needed beyond what is available
    ///
    /// # Returns
    ///
    /// Milliseconds to wait for enough credits to refill.
    pub fn get_throttle_time_ms(&self, deficit: u64) -> u64 {
        self.get_throttle_time_ms_for_deficit(deficit)
    }

    /// Get the credit state for a specific producer.
    ///
    /// Returns `None` if the producer has not yet been tracked.
    pub async fn get_producer_credits(&self, producer_id: &str) -> Option<ProducerCredits> {
        let credits = self.credits.read().await;
        credits.get(producer_id).cloned()
    }

    /// Get the number of tracked producers.
    pub async fn producer_count(&self) -> usize {
        self.credits.read().await.len()
    }

    /// Remove a producer's credit tracking (e.g., on disconnect or fencing).
    pub async fn remove_producer(&self, producer_id: &str) {
        let mut credits = self.credits.write().await;
        credits.remove(producer_id);
    }

    /// Get the current configuration.
    pub fn config(&self) -> &BackpressureConfig {
        &self.config
    }

    // ---- Internal helpers ----

    fn refill_entry(&self, entry: &mut ProducerCredits) {
        let elapsed_secs = entry.last_refill.elapsed().as_secs_f64();
        if elapsed_secs < 0.001 {
            return;
        }

        let refill_amount = (self.config.refill_rate_per_sec as f64 * elapsed_secs) as u64;
        if refill_amount > 0 {
            let old = entry.available;
            entry.available = (entry.available + refill_amount).min(self.config.max_credits);
            entry.total_granted += entry.available - old;
            entry.last_refill = Instant::now();
        }
    }

    fn get_throttle_time_ms_for_deficit(&self, deficit: u64) -> u64 {
        if self.config.refill_rate_per_sec == 0 {
            return u64::MAX;
        }
        // How many milliseconds to wait for `deficit` credits to refill
        let wait_secs = deficit as f64 / self.config.refill_rate_per_sec as f64;
        (wait_secs * 1000.0).ceil() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_config() -> BackpressureConfig {
        BackpressureConfig {
            initial_credits: 100,
            max_credits: 500,
            refill_rate_per_sec: 50,
            min_credits_per_request: 1,
        }
    }

    #[tokio::test]
    async fn test_new_producer_gets_initial_credits() {
        let controller = BackpressureController::new(test_config());

        // First acquire should succeed (100 initial credits, requesting 10)
        let result = controller.acquire_credits("p1", 10).await;
        assert!(result.is_ok());

        let credits = controller.get_producer_credits("p1").await.unwrap();
        assert_eq!(credits.available, 90); // 100 - 10
    }

    #[tokio::test]
    async fn test_acquire_exact_credits() {
        let controller = BackpressureController::new(test_config());

        // Acquire exactly all initial credits
        let result = controller.acquire_credits("p1", 100).await;
        assert!(result.is_ok());

        let credits = controller.get_producer_credits("p1").await.unwrap();
        assert_eq!(credits.available, 0);
    }

    #[tokio::test]
    async fn test_throttle_when_insufficient() {
        let controller = BackpressureController::new(test_config());

        // Use up all credits
        controller.acquire_credits("p1", 100).await.unwrap();

        // Next request should be throttled
        let result = controller.acquire_credits("p1", 10).await;
        assert!(result.is_err());

        let wait_ms = result.unwrap_err();
        assert!(wait_ms > 0, "Should return positive wait time");
    }

    #[tokio::test]
    async fn test_throttle_count_increments() {
        let controller = BackpressureController::new(test_config());

        // Use up all credits
        controller.acquire_credits("p1", 100).await.unwrap();

        // Get throttled twice
        let _ = controller.acquire_credits("p1", 10).await;
        let _ = controller.acquire_credits("p1", 10).await;

        let credits = controller.get_producer_credits("p1").await.unwrap();
        assert_eq!(credits.throttle_count, 2);
    }

    #[tokio::test]
    async fn test_release_credits() {
        let controller = BackpressureController::new(test_config());

        // Use up all credits
        controller.acquire_credits("p1", 100).await.unwrap();

        // Release some credits back
        controller.release_credits("p1", 50).await;

        let credits = controller.get_producer_credits("p1").await.unwrap();
        assert_eq!(credits.available, 50);

        // Should be able to acquire again
        let result = controller.acquire_credits("p1", 30).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_release_credits_capped_at_max() {
        let controller = BackpressureController::new(test_config());

        // Release more than max_credits
        controller.acquire_credits("p1", 1).await.unwrap();
        controller.release_credits("p1", 1_000_000).await;

        let credits = controller.get_producer_credits("p1").await.unwrap();
        assert_eq!(credits.available, 500); // capped at max_credits
    }

    #[tokio::test]
    async fn test_credits_refill_over_time() {
        let config = BackpressureConfig {
            initial_credits: 10,
            max_credits: 100,
            refill_rate_per_sec: 1000, // 1000 credits/sec = 1 credit per ms
            min_credits_per_request: 1,
        };
        let controller = BackpressureController::new(config);

        // Use up all credits
        controller.acquire_credits("p1", 10).await.unwrap();

        // Wait for some refill
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should have some credits now
        let result = controller.acquire_credits("p1", 1).await;
        assert!(result.is_ok(), "Should have refilled credits after 50ms");
    }

    #[tokio::test]
    async fn test_min_credits_per_request() {
        let config = BackpressureConfig {
            initial_credits: 100,
            max_credits: 500,
            refill_rate_per_sec: 50,
            min_credits_per_request: 10,
        };
        let controller = BackpressureController::new(config);

        // Request 1 record, but minimum cost is 10
        controller.acquire_credits("p1", 1).await.unwrap();

        let credits = controller.get_producer_credits("p1").await.unwrap();
        assert_eq!(credits.available, 90); // 100 - 10 (min)
        assert_eq!(credits.total_consumed, 10);
    }

    #[tokio::test]
    async fn test_multiple_producers_independent() {
        let controller = BackpressureController::new(test_config());

        controller.acquire_credits("p1", 100).await.unwrap();
        controller.acquire_credits("p2", 50).await.unwrap();

        // p1 is exhausted
        assert!(controller.acquire_credits("p1", 1).await.is_err());

        // p2 still has credits
        assert!(controller.acquire_credits("p2", 1).await.is_ok());
    }

    #[tokio::test]
    async fn test_remove_producer() {
        let controller = BackpressureController::new(test_config());

        controller.acquire_credits("p1", 10).await.unwrap();
        assert_eq!(controller.producer_count().await, 1);

        controller.remove_producer("p1").await;
        assert_eq!(controller.producer_count().await, 0);
        assert!(controller.get_producer_credits("p1").await.is_none());
    }

    #[tokio::test]
    async fn test_get_throttle_time_ms() {
        let controller = BackpressureController::new(test_config());

        // With refill_rate_per_sec=50, to get 50 credits should take ~1000ms
        let wait = controller.get_throttle_time_ms(50);
        assert_eq!(wait, 1000);

        // To get 25 credits should take ~500ms
        let wait = controller.get_throttle_time_ms(25);
        assert_eq!(wait, 500);

        // To get 1 credit should take 20ms
        let wait = controller.get_throttle_time_ms(1);
        assert_eq!(wait, 20);
    }

    #[tokio::test]
    async fn test_refill_all_producers() {
        let config = BackpressureConfig {
            initial_credits: 10,
            max_credits: 100,
            refill_rate_per_sec: 10_000, // high rate for test
            min_credits_per_request: 1,
        };
        let controller = BackpressureController::new(config);

        // Exhaust credits for two producers
        controller.acquire_credits("p1", 10).await.unwrap();
        controller.acquire_credits("p2", 10).await.unwrap();

        // Wait a bit for refill
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Call refill explicitly
        controller.refill().await;

        // Both should have refilled credits
        let c1 = controller.get_producer_credits("p1").await.unwrap();
        let c2 = controller.get_producer_credits("p2").await.unwrap();
        assert!(c1.available > 0, "p1 should have refilled");
        assert!(c2.available > 0, "p2 should have refilled");
    }

    #[tokio::test]
    async fn test_total_consumed_tracking() {
        let controller = BackpressureController::new(test_config());

        controller.acquire_credits("p1", 30).await.unwrap();
        controller.acquire_credits("p1", 20).await.unwrap();

        let credits = controller.get_producer_credits("p1").await.unwrap();
        assert_eq!(credits.total_consumed, 50);
    }

    #[tokio::test]
    async fn test_config_accessible() {
        let config = test_config();
        let controller = BackpressureController::new(config.clone());
        assert_eq!(controller.config().initial_credits, 100);
        assert_eq!(controller.config().max_credits, 500);
    }
}
