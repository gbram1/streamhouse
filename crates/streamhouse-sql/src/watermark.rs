//! Watermark tracking for event-time stream processing
//!
//! Watermarks represent progress in event time and are used to determine when
//! all data up to a certain timestamp has been received. This module provides:
//!
//! - Per-partition watermark tracking
//! - Out-of-order event tolerance
//! - Idle partition detection and watermark advancement
//! - Late event classification

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::Result;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the [`WatermarkTracker`].
#[derive(Debug, Clone)]
pub struct WatermarkConfig {
    /// Maximum expected out-of-orderness of events (default: 5 seconds).
    ///
    /// The watermark is held back by this amount from the maximum observed
    /// event timestamp so that late-arriving records are not immediately dropped.
    pub max_out_of_orderness: Duration,

    /// If no events arrive on a partition within this duration, the partition is
    /// considered idle and its watermark is advanced to the global watermark
    /// (default: 60 seconds).
    pub idle_timeout: Duration,

    /// How frequently watermarks are recalculated (default: 1 second).
    pub watermark_interval: Duration,
}

impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            max_out_of_orderness: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            watermark_interval: Duration::from_secs(1),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-partition watermark state
// ---------------------------------------------------------------------------

/// Watermark state for a single (topic, partition) pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionWatermark {
    /// Topic name.
    pub topic: String,
    /// Partition id.
    pub partition: u32,
    /// Current watermark value (ms since epoch).
    pub current_watermark: i64,
    /// Highest event timestamp observed on this partition (ms since epoch).
    pub max_timestamp_seen: i64,
    /// Wall-clock instant of the most recent event.
    #[serde(skip, default = "Instant::now")]
    pub last_event_time: Instant,
    /// Whether the partition is considered idle.
    pub is_idle: bool,
    /// Counter of events dropped because they arrived too late.
    pub late_events_dropped: u64,
}

impl PartitionWatermark {
    fn new(topic: &str, partition: u32) -> Self {
        Self {
            topic: topic.to_string(),
            partition,
            current_watermark: i64::MIN,
            max_timestamp_seen: i64::MIN,
            last_event_time: Instant::now(),
            is_idle: false,
            late_events_dropped: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Event-time classification
// ---------------------------------------------------------------------------

/// Result of checking an event timestamp against the current watermark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventTimeResult {
    /// The event is on time (timestamp >= current watermark).
    OnTime,
    /// The event is late (after the watermark but within allowed lateness).
    Late,
    /// The event is too late and should be dropped.
    TooLate,
}

// ---------------------------------------------------------------------------
// WatermarkTracker
// ---------------------------------------------------------------------------

/// Tracks watermarks across all partitions for event-time processing.
pub struct WatermarkTracker {
    config: WatermarkConfig,
    /// (topic, partition) -> PartitionWatermark
    watermarks: RwLock<HashMap<(String, u32), PartitionWatermark>>,
}

impl WatermarkTracker {
    /// Create a new tracker with the given configuration.
    pub fn new(config: WatermarkConfig) -> Self {
        Self {
            config,
            watermarks: RwLock::new(HashMap::new()),
        }
    }

    /// Update the watermark for a specific partition based on an observed event
    /// timestamp (milliseconds since epoch).
    ///
    /// Returns the [`EventTimeResult`] indicating whether the event is on time,
    /// late, or too late.
    pub async fn update_watermark(
        &self,
        topic: &str,
        partition: u32,
        event_timestamp_ms: i64,
    ) -> Result<EventTimeResult> {
        let mut watermarks = self.watermarks.write().await;
        let key = (topic.to_string(), partition);

        let pw = watermarks
            .entry(key)
            .or_insert_with(|| PartitionWatermark::new(topic, partition));

        // Classify the event before updating
        let result = self.classify_event(pw, event_timestamp_ms);

        // Always update max timestamp if this event is newer
        if event_timestamp_ms > pw.max_timestamp_seen {
            pw.max_timestamp_seen = event_timestamp_ms;
        }

        // Recalculate watermark:  max_timestamp - max_out_of_orderness
        let new_watermark = pw.max_timestamp_seen
            - self.config.max_out_of_orderness.as_millis() as i64;
        if new_watermark > pw.current_watermark {
            pw.current_watermark = new_watermark;
        }

        // Mark partition as active
        pw.last_event_time = Instant::now();
        pw.is_idle = false;

        if result == EventTimeResult::TooLate {
            pw.late_events_dropped += 1;
        }

        Ok(result)
    }

    /// Compute the global watermark, defined as the minimum watermark across
    /// all active (non-idle) partitions.
    ///
    /// Returns `None` if no partitions are tracked.
    pub async fn get_global_watermark(&self) -> Option<i64> {
        let watermarks = self.watermarks.read().await;
        watermarks
            .values()
            .filter(|pw| !pw.is_idle)
            .map(|pw| pw.current_watermark)
            .min()
    }

    /// Classify an event timestamp against the current watermark.
    ///
    /// This is a non-mutating check (useful for deciding whether to process an
    /// event without updating state).
    pub async fn check_event_time(
        &self,
        topic: &str,
        partition: u32,
        event_timestamp_ms: i64,
    ) -> EventTimeResult {
        let watermarks = self.watermarks.read().await;
        let key = (topic.to_string(), partition);

        match watermarks.get(&key) {
            None => EventTimeResult::OnTime, // first event is always on time
            Some(pw) => self.classify_event(pw, event_timestamp_ms),
        }
    }

    /// Advance the watermark for any partition that has been idle longer than
    /// `idle_timeout`. Idle partitions are advanced to the current global
    /// watermark so they do not hold back progress.
    pub async fn advance_idle_watermarks(&self) -> Result<u32> {
        // Compute global watermark across active partitions first
        let global_wm = {
            let watermarks = self.watermarks.read().await;
            watermarks
                .values()
                .filter(|pw| !pw.is_idle)
                .map(|pw| pw.current_watermark)
                .min()
        };

        let global_wm = match global_wm {
            Some(wm) => wm,
            None => return Ok(0), // no active partitions
        };

        let mut watermarks = self.watermarks.write().await;
        let mut advanced = 0u32;

        for pw in watermarks.values_mut() {
            if pw.is_idle {
                continue;
            }
            if pw.last_event_time.elapsed() > self.config.idle_timeout {
                pw.is_idle = true;
                if global_wm > pw.current_watermark {
                    pw.current_watermark = global_wm;
                }
                advanced += 1;
            }
        }

        Ok(advanced)
    }

    /// Retrieve a snapshot of all partition watermarks.
    pub async fn get_all_watermarks(&self) -> HashMap<(String, u32), PartitionWatermark> {
        self.watermarks.read().await.clone()
    }

    /// Return a reference to the tracker configuration.
    pub fn config(&self) -> &WatermarkConfig {
        &self.config
    }

    // -- internal helpers --

    fn classify_event(&self, pw: &PartitionWatermark, event_timestamp_ms: i64) -> EventTimeResult {
        if pw.current_watermark == i64::MIN {
            // No watermark established yet — treat as on-time.
            return EventTimeResult::OnTime;
        }

        if event_timestamp_ms >= pw.current_watermark {
            EventTimeResult::OnTime
        } else {
            // The event is behind the watermark. If it is within
            // max_out_of_orderness of the watermark we call it "late" but
            // still processable; otherwise it is too late.
            let lateness = pw.current_watermark - event_timestamp_ms;
            let allowed = self.config.max_out_of_orderness.as_millis() as i64;
            if lateness <= allowed {
                EventTimeResult::Late
            } else {
                EventTimeResult::TooLate
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tracker() -> WatermarkTracker {
        WatermarkTracker::new(WatermarkConfig::default())
    }

    #[tokio::test]
    async fn test_first_event_is_on_time() {
        let tracker = default_tracker();
        let result = tracker
            .update_watermark("topic", 0, 1_000_000)
            .await
            .unwrap();
        assert_eq!(result, EventTimeResult::OnTime);
    }

    #[tokio::test]
    async fn test_increasing_timestamps_on_time() {
        let tracker = default_tracker();
        for ts in [1_000_000, 2_000_000, 3_000_000] {
            let result = tracker.update_watermark("topic", 0, ts).await.unwrap();
            assert_eq!(result, EventTimeResult::OnTime);
        }
    }

    #[tokio::test]
    async fn test_slightly_late_event() {
        let tracker = WatermarkTracker::new(WatermarkConfig {
            max_out_of_orderness: Duration::from_secs(5),
            ..Default::default()
        });

        // Establish a watermark
        tracker.update_watermark("t", 0, 100_000).await.unwrap();
        // watermark = 100_000 - 5_000 = 95_000

        // An event at 93_000 is behind the watermark (95_000) but within
        // allowed lateness (5_000ms).
        let result = tracker.update_watermark("t", 0, 93_000).await.unwrap();
        assert_eq!(result, EventTimeResult::Late);
    }

    #[tokio::test]
    async fn test_too_late_event() {
        let tracker = WatermarkTracker::new(WatermarkConfig {
            max_out_of_orderness: Duration::from_secs(5),
            ..Default::default()
        });

        // Establish watermark: max_ts=100_000, watermark=95_000
        tracker.update_watermark("t", 0, 100_000).await.unwrap();

        // Event at 80_000 is 15_000ms behind watermark — beyond the 5_000ms
        // allowed lateness.
        let result = tracker.update_watermark("t", 0, 80_000).await.unwrap();
        assert_eq!(result, EventTimeResult::TooLate);
    }

    #[tokio::test]
    async fn test_late_events_dropped_counter() {
        let tracker = WatermarkTracker::new(WatermarkConfig {
            max_out_of_orderness: Duration::from_secs(1),
            ..Default::default()
        });

        tracker.update_watermark("t", 0, 100_000).await.unwrap();
        // watermark = 99_000

        // Send a too-late event (way behind)
        tracker.update_watermark("t", 0, 50_000).await.unwrap();

        let wms = tracker.get_all_watermarks().await;
        let pw = wms.get(&("t".to_string(), 0)).unwrap();
        assert_eq!(pw.late_events_dropped, 1);
    }

    #[tokio::test]
    async fn test_global_watermark_is_minimum() {
        let tracker = default_tracker();

        tracker
            .update_watermark("topic", 0, 200_000)
            .await
            .unwrap();
        tracker
            .update_watermark("topic", 1, 100_000)
            .await
            .unwrap();

        let global = tracker.get_global_watermark().await.unwrap();
        // partition 1 has max_ts=100_000, watermark=100_000-5_000=95_000
        assert_eq!(global, 95_000);
    }

    #[tokio::test]
    async fn test_global_watermark_none_when_empty() {
        let tracker = default_tracker();
        assert_eq!(tracker.get_global_watermark().await, None);
    }

    #[tokio::test]
    async fn test_check_event_time_without_mutation() {
        let tracker = default_tracker();

        // First check on unknown partition => OnTime
        let result = tracker.check_event_time("t", 0, 50_000).await;
        assert_eq!(result, EventTimeResult::OnTime);

        // Establish watermark
        tracker.update_watermark("t", 0, 100_000).await.unwrap();

        // Check a recent timestamp => OnTime
        let result = tracker.check_event_time("t", 0, 99_000).await;
        assert_eq!(result, EventTimeResult::OnTime);
    }

    #[tokio::test]
    async fn test_advance_idle_watermarks() {
        let tracker = WatermarkTracker::new(WatermarkConfig {
            idle_timeout: Duration::from_millis(10),
            max_out_of_orderness: Duration::from_secs(5),
            ..Default::default()
        });

        // Partition 0 gets an event
        tracker.update_watermark("t", 0, 200_000).await.unwrap();
        // Partition 1 gets an event
        tracker.update_watermark("t", 1, 100_000).await.unwrap();

        // Wait for the idle timeout to elapse
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Now advance — both should become idle.
        // But advance_idle_watermarks only advances to the global watermark of
        // *active* partitions. Since we're about to mark them all idle in one
        // pass, the first one processed sees the other as active.
        let advanced = tracker.advance_idle_watermarks().await.unwrap();
        assert!(advanced > 0);
    }

    #[tokio::test]
    async fn test_watermark_config_defaults() {
        let config = WatermarkConfig::default();
        assert_eq!(config.max_out_of_orderness, Duration::from_secs(5));
        assert_eq!(config.idle_timeout, Duration::from_secs(60));
        assert_eq!(config.watermark_interval, Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_watermark_advances_monotonically() {
        let tracker = default_tracker();

        tracker.update_watermark("t", 0, 100_000).await.unwrap();
        let wm1 = {
            let wms = tracker.get_all_watermarks().await;
            wms[&("t".to_string(), 0)].current_watermark
        };

        // A later event should advance the watermark
        tracker.update_watermark("t", 0, 200_000).await.unwrap();
        let wm2 = {
            let wms = tracker.get_all_watermarks().await;
            wms[&("t".to_string(), 0)].current_watermark
        };

        assert!(wm2 > wm1);

        // An earlier event should NOT decrease the watermark
        tracker.update_watermark("t", 0, 150_000).await.unwrap();
        let wm3 = {
            let wms = tracker.get_all_watermarks().await;
            wms[&("t".to_string(), 0)].current_watermark
        };

        assert_eq!(wm3, wm2);
    }

    #[tokio::test]
    async fn test_multiple_topics_independent() {
        let tracker = default_tracker();

        tracker
            .update_watermark("topic-a", 0, 500_000)
            .await
            .unwrap();
        tracker
            .update_watermark("topic-b", 0, 100_000)
            .await
            .unwrap();

        let wms = tracker.get_all_watermarks().await;
        let wm_a = wms[&("topic-a".to_string(), 0)].current_watermark;
        let wm_b = wms[&("topic-b".to_string(), 0)].current_watermark;

        assert!(wm_a > wm_b);
    }
}
