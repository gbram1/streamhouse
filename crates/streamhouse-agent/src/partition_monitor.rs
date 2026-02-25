//! Hot-Partition Detection
//!
//! This module implements per-partition write rate tracking to detect hot partitions
//! that are receiving disproportionately high write traffic. Hot partitions can cause:
//!
//! - **Imbalanced storage**: One partition's segments grow much faster than others
//! - **Increased latency**: The hot partition's agent becomes a bottleneck
//! - **S3 throttling**: Concentrated writes to one prefix can trigger rate limits
//!
//! ## Detection Algorithm
//!
//! For each `(topic, partition)`, the monitor tracks:
//! - Total records written in the current window
//! - Total bytes written in the current window
//! - Write rates (records/sec and bytes/sec)
//!
//! A partition is flagged as "hot" when either:
//! - Record write rate exceeds `hot_threshold_records_per_sec` (default: 50,000/s)
//! - Byte write rate exceeds `hot_threshold_bytes_per_sec` (default: 50 MB/s)
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_agent::partition_monitor::{PartitionMonitor, PartitionMonitorConfig};
//!
//! let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());
//!
//! // Record writes as they happen
//! monitor.record_write("orders", 0, 100, 51200).await;
//!
//! // Periodically check for hot partitions
//! let hot = monitor.check_hot_partitions().await;
//! for ((topic, partition), stats) in &hot {
//!     tracing::warn!("Hot partition: {}/{} at {} records/sec", topic, partition, stats.write_rate_records_per_sec);
//! }
//! ```

use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::time::Instant;

/// Configuration for the partition monitor.
#[derive(Debug, Clone)]
pub struct PartitionMonitorConfig {
    /// Record rate threshold (records/sec) above which a partition is considered hot.
    pub hot_threshold_records_per_sec: f64,
    /// Byte rate threshold (bytes/sec) above which a partition is considered hot.
    pub hot_threshold_bytes_per_sec: f64,
    /// Duration of the sliding window in seconds for rate calculation.
    pub window_seconds: u64,
    /// How often (in seconds) to run the hot-partition check.
    pub check_interval_seconds: u64,
}

impl Default for PartitionMonitorConfig {
    fn default() -> Self {
        Self {
            hot_threshold_records_per_sec: 50_000.0,
            hot_threshold_bytes_per_sec: 50_000_000.0, // 50 MB/s
            window_seconds: 60,
            check_interval_seconds: 30,
        }
    }
}

/// Per-partition write statistics.
#[derive(Debug, Clone)]
pub struct PartitionStats {
    /// Total records written in the current window.
    pub records_total: u64,
    /// Total bytes written in the current window.
    pub bytes_total: u64,
    /// When the current window started.
    pub last_reset: Instant,
    /// Whether this partition is currently flagged as hot.
    pub is_hot: bool,
    /// Calculated write rate in records per second.
    pub write_rate_records_per_sec: f64,
    /// Calculated write rate in bytes per second.
    pub write_rate_bytes_per_sec: f64,
}

impl PartitionStats {
    fn new() -> Self {
        Self {
            records_total: 0,
            bytes_total: 0,
            last_reset: Instant::now(),
            is_hot: false,
            write_rate_records_per_sec: 0.0,
            write_rate_bytes_per_sec: 0.0,
        }
    }

    #[cfg(test)]
    fn new_with_instant(now: Instant) -> Self {
        Self {
            records_total: 0,
            bytes_total: 0,
            last_reset: now,
            is_hot: false,
            write_rate_records_per_sec: 0.0,
            write_rate_bytes_per_sec: 0.0,
        }
    }
}

/// Tracks per-partition write rates and detects hot partitions.
pub struct PartitionMonitor {
    /// Write rates per (topic, partition).
    rates: RwLock<HashMap<(String, u32), PartitionStats>>,
    /// Monitor configuration.
    config: PartitionMonitorConfig,
}

impl PartitionMonitor {
    /// Create a new partition monitor with the given configuration.
    pub fn new(config: PartitionMonitorConfig) -> Self {
        Self {
            rates: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Record a write to a partition.
    ///
    /// Called by the write path after each successful append to track the
    /// partition's write rate.
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name
    /// * `partition` - Partition ID
    /// * `record_count` - Number of records in the write batch
    /// * `byte_count` - Total bytes in the write batch
    pub async fn record_write(
        &self,
        topic: &str,
        partition: u32,
        record_count: u64,
        byte_count: u64,
    ) {
        let key = (topic.to_string(), partition);
        let mut rates = self.rates.write().await;
        let stats = rates.entry(key).or_insert_with(PartitionStats::new);

        let elapsed = stats.last_reset.elapsed().as_secs();
        if elapsed >= self.config.window_seconds {
            // Window expired, reset counters
            stats.records_total = 0;
            stats.bytes_total = 0;
            stats.last_reset = Instant::now();
        }

        stats.records_total += record_count;
        stats.bytes_total += byte_count;
    }

    /// Check all tracked partitions and flag hot ones.
    ///
    /// Calculates write rates for all partitions and flags those exceeding
    /// the configured thresholds. Returns only the hot partitions.
    ///
    /// # Returns
    ///
    /// A vector of `((topic, partition), PartitionStats)` for all hot partitions.
    pub async fn check_hot_partitions(&self) -> Vec<((String, u32), PartitionStats)> {
        let mut rates = self.rates.write().await;
        let mut hot = Vec::new();

        for (key, stats) in rates.iter_mut() {
            let elapsed_secs = stats.last_reset.elapsed().as_secs_f64();
            if elapsed_secs < 0.001 {
                // Avoid division by near-zero; not enough time has passed.
                continue;
            }

            stats.write_rate_records_per_sec = stats.records_total as f64 / elapsed_secs;
            stats.write_rate_bytes_per_sec = stats.bytes_total as f64 / elapsed_secs;

            stats.is_hot = stats.write_rate_records_per_sec
                > self.config.hot_threshold_records_per_sec
                || stats.write_rate_bytes_per_sec > self.config.hot_threshold_bytes_per_sec;

            if stats.is_hot {
                hot.push((key.clone(), stats.clone()));
            }
        }

        hot
    }

    /// Get statistics for a specific partition.
    ///
    /// Returns `None` if the partition has not been seen yet.
    pub async fn get_stats(&self, topic: &str, partition: u32) -> Option<PartitionStats> {
        let key = (topic.to_string(), partition);
        let mut rates = self.rates.write().await;

        rates.get_mut(&key).map(|stats| {
            let elapsed_secs = stats.last_reset.elapsed().as_secs_f64();
            if elapsed_secs > 0.001 {
                stats.write_rate_records_per_sec = stats.records_total as f64 / elapsed_secs;
                stats.write_rate_bytes_per_sec = stats.bytes_total as f64 / elapsed_secs;
            }
            stats.clone()
        })
    }

    /// Get statistics for all tracked partitions.
    ///
    /// Returns a snapshot of all partition stats with up-to-date rate calculations.
    pub async fn get_all_stats(&self) -> HashMap<(String, u32), PartitionStats> {
        let mut rates = self.rates.write().await;
        let mut result = HashMap::new();

        for (key, stats) in rates.iter_mut() {
            let elapsed_secs = stats.last_reset.elapsed().as_secs_f64();
            if elapsed_secs > 0.001 {
                stats.write_rate_records_per_sec = stats.records_total as f64 / elapsed_secs;
                stats.write_rate_bytes_per_sec = stats.bytes_total as f64 / elapsed_secs;
            }
            result.insert(key.clone(), stats.clone());
        }

        result
    }

    /// Reset all tracked partition statistics.
    ///
    /// Clears all entries from the monitor. Useful when agents are rebalanced
    /// or during testing.
    pub async fn reset(&self) {
        let mut rates = self.rates.write().await;
        rates.clear();
    }

    /// Get the current configuration.
    pub fn config(&self) -> &PartitionMonitorConfig {
        &self.config
    }

    /// Get the number of tracked partitions.
    pub async fn tracked_count(&self) -> usize {
        self.rates.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn low_threshold_config() -> PartitionMonitorConfig {
        PartitionMonitorConfig {
            hot_threshold_records_per_sec: 100.0,
            hot_threshold_bytes_per_sec: 1_000.0,
            window_seconds: 60,
            check_interval_seconds: 10,
        }
    }

    #[tokio::test]
    async fn test_record_write_creates_entry() {
        let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());
        assert_eq!(monitor.tracked_count().await, 0);

        monitor.record_write("orders", 0, 100, 5120).await;
        assert_eq!(monitor.tracked_count().await, 1);
    }

    #[tokio::test]
    async fn test_record_write_accumulates() {
        let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());

        monitor.record_write("orders", 0, 100, 5120).await;
        monitor.record_write("orders", 0, 200, 10240).await;

        let stats = monitor.get_stats("orders", 0).await.unwrap();
        assert_eq!(stats.records_total, 300);
        assert_eq!(stats.bytes_total, 15360);
    }

    #[tokio::test]
    async fn test_different_partitions_tracked_independently() {
        let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());

        monitor.record_write("orders", 0, 100, 5120).await;
        monitor.record_write("orders", 1, 200, 10240).await;

        assert_eq!(monitor.tracked_count().await, 2);

        let s0 = monitor.get_stats("orders", 0).await.unwrap();
        assert_eq!(s0.records_total, 100);

        let s1 = monitor.get_stats("orders", 1).await.unwrap();
        assert_eq!(s1.records_total, 200);
    }

    #[tokio::test]
    async fn test_different_topics_tracked_independently() {
        let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());

        monitor.record_write("orders", 0, 100, 5120).await;
        monitor.record_write("events", 0, 200, 10240).await;

        assert_eq!(monitor.tracked_count().await, 2);

        let orders = monitor.get_stats("orders", 0).await.unwrap();
        assert_eq!(orders.records_total, 100);

        let events = monitor.get_stats("events", 0).await.unwrap();
        assert_eq!(events.records_total, 200);
    }

    #[tokio::test]
    async fn test_hot_partition_detection_by_records() {
        let config = low_threshold_config();
        let monitor = PartitionMonitor::new(config);

        // Write enough records to exceed threshold (100 rec/s)
        // We'll write 1000 records in a very short time
        monitor.record_write("orders", 0, 1000, 100).await;

        // Small delay to ensure some time has passed for rate calculation
        tokio::time::sleep(Duration::from_millis(10)).await;

        let hot = monitor.check_hot_partitions().await;
        assert!(!hot.is_empty(), "Partition should be hot by record rate");
        assert_eq!(hot[0].0, ("orders".to_string(), 0));
        assert!(hot[0].1.is_hot);
    }

    #[tokio::test]
    async fn test_hot_partition_detection_by_bytes() {
        let config = low_threshold_config();
        let monitor = PartitionMonitor::new(config);

        // Write enough bytes to exceed threshold (1000 bytes/s)
        monitor.record_write("orders", 0, 1, 100_000).await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        let hot = monitor.check_hot_partitions().await;
        assert!(!hot.is_empty(), "Partition should be hot by byte rate");
        assert!(hot[0].1.write_rate_bytes_per_sec > 1_000.0);
    }

    #[tokio::test]
    async fn test_non_hot_partition_not_flagged() {
        let config = PartitionMonitorConfig {
            hot_threshold_records_per_sec: 1_000_000.0,
            hot_threshold_bytes_per_sec: 1_000_000_000.0,
            window_seconds: 60,
            check_interval_seconds: 10,
        };
        let monitor = PartitionMonitor::new(config);

        monitor.record_write("orders", 0, 10, 512).await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        let hot = monitor.check_hot_partitions().await;
        assert!(hot.is_empty(), "Partition should not be hot");

        let stats = monitor.get_stats("orders", 0).await.unwrap();
        assert!(!stats.is_hot);
    }

    #[tokio::test]
    async fn test_reset_clears_all() {
        let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());

        monitor.record_write("orders", 0, 100, 5120).await;
        monitor.record_write("events", 0, 200, 10240).await;
        assert_eq!(monitor.tracked_count().await, 2);

        monitor.reset().await;
        assert_eq!(monitor.tracked_count().await, 0);
        assert!(monitor.get_stats("orders", 0).await.is_none());
    }

    #[tokio::test]
    async fn test_get_all_stats() {
        let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());

        monitor.record_write("orders", 0, 100, 5120).await;
        monitor.record_write("orders", 1, 200, 10240).await;
        monitor.record_write("events", 0, 50, 2048).await;

        let all = monitor.get_all_stats().await;
        assert_eq!(all.len(), 3);
        assert!(all.contains_key(&("orders".to_string(), 0)));
        assert!(all.contains_key(&("orders".to_string(), 1)));
        assert!(all.contains_key(&("events".to_string(), 0)));
    }

    #[tokio::test]
    async fn test_window_reset() {
        let config = PartitionMonitorConfig {
            window_seconds: 0, // Immediate window expiration
            ..PartitionMonitorConfig::default()
        };
        let monitor = PartitionMonitor::new(config);

        monitor.record_write("orders", 0, 100, 5120).await;

        // Sleep briefly so the window expires (window_seconds = 0)
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Next write should reset counters due to expired window
        monitor.record_write("orders", 0, 50, 2048).await;

        let stats = monitor.get_stats("orders", 0).await.unwrap();
        // After reset, only the second write should be counted
        assert_eq!(stats.records_total, 50);
        assert_eq!(stats.bytes_total, 2048);
    }

    #[tokio::test]
    async fn test_config_accessible() {
        let config = PartitionMonitorConfig {
            hot_threshold_records_per_sec: 42.0,
            ..PartitionMonitorConfig::default()
        };
        let monitor = PartitionMonitor::new(config);
        assert_eq!(monitor.config().hot_threshold_records_per_sec, 42.0);
    }

    #[tokio::test]
    async fn test_unknown_partition_returns_none() {
        let monitor = PartitionMonitor::new(PartitionMonitorConfig::default());
        assert!(monitor.get_stats("nonexistent", 99).await.is_none());
    }
}
