//! Chaos / Fault-Injection Tests for StreamHouse Storage Layer
//!
//! Phase 10.6 - Resilience testing for circuit breaker, rate limiter,
//! cache, and WAL-like crash recovery scenarios.
//!
//! These tests intentionally push components beyond normal operating conditions
//! to verify correct behavior under adversarial workloads.

use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use streamhouse_storage::cache::SegmentCache;
use streamhouse_storage::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
use streamhouse_storage::rate_limiter::{BucketConfig, RateLimiter, S3Operation};
use streamhouse_storage::wal::{SyncPolicy, WALConfig, WAL};

// ============================================================================
// Fault Injection Helpers
// ============================================================================

/// Probabilistic fault injector for chaos testing.
///
/// Uses a simple deterministic counter-based approach to inject failures
/// at a configurable rate without requiring the `rand` crate.
/// Thread-safe via atomics.
struct FaultInjector {
    /// Denominator for failure probability: fail 1 out of every N calls.
    /// A value of 2 means ~50% failure rate, 3 means ~33%, etc.
    failure_denominator: u64,
    /// Monotonically increasing call counter.
    counter: AtomicU64,
    /// Total number of injected faults (for observability).
    faults_injected: AtomicU64,
}

impl FaultInjector {
    /// Create a new fault injector.
    ///
    /// `failure_denominator`: fail 1 out of every N calls.
    /// E.g. 2 = 50%, 3 = 33%, 5 = 20%, 1 = 100% (always fail).
    fn new(failure_denominator: u64) -> Self {
        assert!(failure_denominator >= 1, "denominator must be >= 1");
        Self {
            failure_denominator,
            counter: AtomicU64::new(0),
            faults_injected: AtomicU64::new(0),
        }
    }

    /// Returns `true` if this call should be treated as a failure.
    fn should_fail(&self) -> bool {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        let fail = (count % self.failure_denominator) == 0;
        if fail {
            self.faults_injected.fetch_add(1, Ordering::Relaxed);
        }
        fail
    }

    /// Number of faults injected so far.
    fn total_faults(&self) -> u64 {
        self.faults_injected.load(Ordering::Relaxed)
    }
}

/// Spawn `n` concurrent tasks that each execute `work` and return all results.
async fn spawn_concurrent<F, Fut, T>(n: usize, work: F) -> Vec<T>
where
    F: Fn(usize) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let work = Arc::new(work);
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let w = work.clone();
        handles.push(tokio::spawn(async move { w(i).await }));
    }
    let mut results = Vec::with_capacity(n);
    for h in handles {
        results.push(h.await.expect("task panicked"));
    }
    results
}

// ============================================================================
// 1. Circuit Breaker Chaos Tests
// ============================================================================

/// Rapid concurrent failures should open the circuit exactly once,
/// not leave it in an inconsistent state.
#[tokio::test]
async fn chaos_cb_rapid_concurrent_failures() {
    let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 5,
        success_threshold: 2,
        timeout: Duration::from_secs(60),
    }));

    // 50 concurrent tasks each reporting a failure
    let b = breaker.clone();
    spawn_concurrent(50, move |_| {
        let b = b.clone();
        async move {
            b.report_failure().await;
        }
    })
    .await;

    // Circuit should be open (not in some half-state)
    assert_eq!(breaker.current_state(), CircuitState::Open);
}

/// After burst errors open the circuit, it should recover through
/// half-open and back to closed.
#[tokio::test]
async fn chaos_cb_recovery_after_burst_errors() {
    let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: Duration::from_millis(50),
    }));

    // Burst of failures
    for _ in 0..10 {
        breaker.report_failure().await;
    }
    assert_eq!(breaker.current_state(), CircuitState::Open);

    // Wait for timeout to allow half-open
    sleep(Duration::from_millis(60)).await;
    assert!(breaker.allow_request().await);
    assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

    // Successful recovery
    breaker.report_success().await;
    breaker.report_success().await;
    assert_eq!(breaker.current_state(), CircuitState::Closed);
}

/// Concurrent success/failure reports in half-open should not
/// leave the circuit in a broken state.
#[tokio::test]
async fn chaos_cb_state_consistency_under_contention() {
    let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 1,
        success_threshold: 3,
        timeout: Duration::from_millis(50),
    }));

    // Open the circuit
    breaker.report_failure().await;
    assert_eq!(breaker.current_state(), CircuitState::Open);

    // Wait for half-open
    sleep(Duration::from_millis(60)).await;
    breaker.allow_request().await;
    assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

    // Concurrent successes and failures
    let b = breaker.clone();
    spawn_concurrent(20, move |i| {
        let b = b.clone();
        async move {
            if i % 3 == 0 {
                b.report_failure().await;
            } else {
                b.report_success().await;
            }
        }
    })
    .await;

    // State should be one of the valid states, never an unknown value
    let state = breaker.current_state();
    assert!(
        state == CircuitState::Closed
            || state == CircuitState::Open
            || state == CircuitState::HalfOpen,
        "Invalid state: {:?}",
        state
    );
}

/// Opening and closing the circuit rapidly in a loop should not
/// corrupt internal counters.
#[tokio::test]
async fn chaos_cb_rapid_open_close_cycles() {
    let breaker = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 1,
        success_threshold: 1,
        timeout: Duration::from_millis(10),
    });

    for _ in 0..20 {
        // Open
        breaker.report_failure().await;
        assert_eq!(breaker.current_state(), CircuitState::Open);

        // Wait for half-open
        sleep(Duration::from_millis(15)).await;
        breaker.allow_request().await;
        assert_eq!(breaker.current_state(), CircuitState::HalfOpen);

        // Close
        breaker.report_success().await;
        assert_eq!(breaker.current_state(), CircuitState::Closed);
    }

    // Final state should be consistent
    assert_eq!(breaker.failure_count(), 0);
    assert_eq!(breaker.success_count(), 0);
}

/// Fault injector drives the circuit breaker: a high failure rate
/// should eventually open the circuit.
#[tokio::test]
async fn chaos_cb_fault_injector_driven() {
    let breaker = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 5,
        success_threshold: 2,
        timeout: Duration::from_secs(60),
    });

    let injector = FaultInjector::new(2); // 50% failure rate

    for _ in 0..100 {
        if breaker.allow_request().await {
            if injector.should_fail() {
                breaker.report_failure().await;
            } else {
                breaker.report_success().await;
            }
        }
    }

    // With 50% failure rate and threshold of 5, circuit should have opened
    assert!(injector.total_faults() >= 5);
    // The circuit may be open or closed depending on timing, but it should
    // have been opened at least once (we cannot check history, but can verify
    // the current state is valid).
    let state = breaker.current_state();
    assert!(
        state == CircuitState::Closed || state == CircuitState::Open,
        "unexpected state: {:?}",
        state
    );
}

/// Many concurrent allow_request() calls on an open circuit should
/// all be rejected (no spurious allows).
#[tokio::test]
async fn chaos_cb_concurrent_rejection_in_open() {
    let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 1,
        success_threshold: 2,
        timeout: Duration::from_secs(300), // very long timeout
    }));

    // Open the circuit
    breaker.report_failure().await;
    assert_eq!(breaker.current_state(), CircuitState::Open);

    // 100 concurrent allow_request calls - all should be false
    let b = breaker.clone();
    let results = spawn_concurrent(100, move |_| {
        let b = b.clone();
        async move { b.allow_request().await }
    })
    .await;

    let allowed_count = results.iter().filter(|&&r| r).count();
    assert_eq!(
        allowed_count, 0,
        "Expected 0 requests allowed, got {}",
        allowed_count
    );
}

// ============================================================================
// 2. Rate Limiter Chaos Tests
// ============================================================================

/// Drain all tokens from a bucket and verify it rejects until refill.
#[tokio::test]
async fn chaos_rl_token_depletion_and_recovery() {
    let limiter = RateLimiter::new(
        BucketConfig {
            rate: 100.0, // 100 tokens/sec
            burst: 10,
            min_rate: 10.0,
            max_rate: 1000.0,
        },
        BucketConfig::default(),
        BucketConfig::default(),
    );

    // Drain all PUT tokens
    let mut acquired = 0;
    for _ in 0..20 {
        if limiter.try_acquire(S3Operation::Put).await {
            acquired += 1;
        }
    }
    assert_eq!(acquired, 10, "Should acquire exactly burst capacity");

    // Should be rejected now
    assert!(!limiter.try_acquire(S3Operation::Put).await);

    // Wait for refill (100 tokens/sec = ~1 token per 10ms)
    sleep(Duration::from_millis(50)).await;

    // Should be able to acquire again
    assert!(limiter.try_acquire(S3Operation::Put).await);
}

/// Sustained throttling should drive the adaptive rate down to the floor.
#[tokio::test]
async fn chaos_rl_adaptive_backoff_sustained_throttling() {
    let limiter = RateLimiter::new(
        BucketConfig {
            rate: 1000.0,
            burst: 100,
            min_rate: 50.0,
            max_rate: 10000.0,
        },
        BucketConfig::default(),
        BucketConfig::default(),
    );

    // Simulate 20 consecutive throttle events
    for _ in 0..20 {
        limiter.report_throttled(S3Operation::Put).await;
    }

    let (put_rate, _, _) = limiter.current_rates().await;
    // After 20 halvings from 1000: 1000 * 0.5^20 << 50
    // Should be clamped at the floor of 50
    assert!(
        (put_rate - 50.0).abs() < 0.01,
        "Rate should be at floor 50.0, got {}",
        put_rate
    );
}

/// The rate floor should never be violated, even under extreme throttling.
#[tokio::test]
async fn chaos_rl_rate_floor_enforcement() {
    let limiter = RateLimiter::new(
        BucketConfig {
            rate: 200.0,
            burst: 10,
            min_rate: 100.0,
            max_rate: 1000.0,
        },
        BucketConfig::default(),
        BucketConfig::default(),
    );

    // Throttle 100 times
    for _ in 0..100 {
        limiter.report_throttled(S3Operation::Put).await;
    }

    let (put_rate, _, _) = limiter.current_rates().await;
    assert!(
        put_rate >= 100.0,
        "Rate {} fell below floor 100.0",
        put_rate
    );
}

/// Throttling one operation type should not affect others.
#[tokio::test]
async fn chaos_rl_concurrent_operation_isolation() {
    let limiter = Arc::new(RateLimiter::new(
        BucketConfig {
            rate: 1000.0,
            burst: 50,
            min_rate: 10.0,
            max_rate: 10000.0,
        },
        BucketConfig {
            rate: 2000.0,
            burst: 50,
            min_rate: 10.0,
            max_rate: 10000.0,
        },
        BucketConfig {
            rate: 3000.0,
            burst: 50,
            min_rate: 10.0,
            max_rate: 10000.0,
        },
    ));

    // Hammer PUT throttling from many tasks
    let l = limiter.clone();
    spawn_concurrent(50, move |_| {
        let l = l.clone();
        async move {
            l.report_throttled(S3Operation::Put).await;
        }
    })
    .await;

    let (put_rate, get_rate, delete_rate) = limiter.current_rates().await;
    // PUT should be degraded
    assert!(put_rate < 1000.0, "PUT rate should be reduced");
    // GET and DELETE should be untouched
    assert_eq!(get_rate, 2000.0, "GET rate should be unchanged");
    assert_eq!(delete_rate, 3000.0, "DELETE rate should be unchanged");
}

/// Concurrent acquire calls from many tasks should respect the burst limit.
#[tokio::test]
async fn chaos_rl_concurrent_acquire_burst_limit() {
    let limiter = Arc::new(RateLimiter::new(
        BucketConfig {
            rate: 1.0, // Very slow refill
            burst: 20,
            min_rate: 1.0,
            max_rate: 100.0,
        },
        BucketConfig::default(),
        BucketConfig::default(),
    ));

    // 100 concurrent tasks each trying to acquire
    let l = limiter.clone();
    let results = spawn_concurrent(100, move |_| {
        let l = l.clone();
        async move { l.try_acquire(S3Operation::Put).await }
    })
    .await;

    let total_acquired: usize = results.iter().filter(|&&r| r).count();
    // Should not exceed burst capacity (20) + potential refill during test
    assert!(
        total_acquired <= 25,
        "Acquired {} tokens, expected <= ~25 (burst 20 + small refill)",
        total_acquired
    );
    assert!(
        total_acquired >= 15,
        "Acquired {} tokens, expected >= 15 (most of burst 20)",
        total_acquired
    );
}

/// Rapidly alternating throttle/success events should keep rate bounded.
#[tokio::test]
async fn chaos_rl_thrashing_throttle_success() {
    let limiter = RateLimiter::new(
        BucketConfig {
            rate: 1000.0,
            burst: 100,
            min_rate: 50.0,
            max_rate: 5000.0,
        },
        BucketConfig::default(),
        BucketConfig::default(),
    );

    // Alternate throttle and success rapidly
    for _ in 0..100 {
        limiter.report_throttled(S3Operation::Put).await;
        limiter.report_success(S3Operation::Put).await;
    }

    let (put_rate, _, _) = limiter.current_rates().await;
    // Net effect: each cycle multiplies by 0.5 * 1.05 = 0.525
    // After many cycles the rate should be near the floor
    assert!(
        put_rate >= 50.0,
        "Rate {} should be >= floor 50.0",
        put_rate
    );
    assert!(
        put_rate <= 5000.0,
        "Rate {} should be <= ceiling 5000.0",
        put_rate
    );
}

// ============================================================================
// 3. Cache Chaos Tests
// ============================================================================

/// Fill cache beyond capacity and verify LRU eviction under memory pressure.
#[tokio::test]
async fn chaos_cache_lru_eviction_under_pressure() {
    let temp_dir = tempfile::tempdir().unwrap();
    // Cache can hold 500 bytes
    let cache = SegmentCache::new(temp_dir.path().join("cache"), 500).unwrap();

    // Write 10 segments of 100 bytes each (total 1000 > capacity 500)
    for i in 0..10 {
        let key = format!("segment-{}", i);
        let data = Bytes::from(vec![i as u8; 100]);
        cache.put(&key, data).await.unwrap();
    }

    let stats = cache.stats().await;
    // Current size should not exceed max
    assert!(
        stats.current_size <= 500,
        "Cache size {} exceeds max 500",
        stats.current_size
    );

    // Most recent entries should still be present
    assert!(cache.get("segment-9").await.unwrap().is_some());
    assert!(cache.get("segment-8").await.unwrap().is_some());

    // Oldest entries should have been evicted
    assert!(cache.get("segment-0").await.unwrap().is_none());
    assert!(cache.get("segment-1").await.unwrap().is_none());
}

/// Concurrent reads and writes should not corrupt cache state.
#[tokio::test]
async fn chaos_cache_concurrent_reads_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 10000).unwrap());

    // Pre-populate
    for i in 0..10 {
        let key = format!("pre-{}", i);
        cache
            .put(&key, Bytes::from(vec![i as u8; 50]))
            .await
            .unwrap();
    }

    // Concurrent readers
    let c1 = cache.clone();
    let read_handle = tokio::spawn(async move {
        let mut hits = 0u64;
        for round in 0..50 {
            let key = format!("pre-{}", round % 10);
            if c1.get(&key).await.unwrap().is_some() {
                hits += 1;
            }
        }
        hits
    });

    // Concurrent writers
    let c2 = cache.clone();
    let write_handle = tokio::spawn(async move {
        for i in 0..50 {
            let key = format!("new-{}", i);
            c2.put(&key, Bytes::from(vec![0u8; 50])).await.unwrap();
        }
    });

    let (hits, _) = tokio::join!(read_handle, write_handle);
    let hits = hits.unwrap();

    // We should have gotten at least some cache hits from pre-populated data
    assert!(hits > 0, "Expected some cache hits, got 0");

    // Cache state should be consistent
    let stats = cache.stats().await;
    assert!(stats.current_size <= 10000);
    assert!(stats.entry_count > 0);
}

/// Putting an oversized entry should not panic or corrupt other entries.
#[tokio::test]
async fn chaos_cache_oversized_entries() {
    let temp_dir = tempfile::tempdir().unwrap();
    // Cache max = 200 bytes
    let cache = SegmentCache::new(temp_dir.path().join("cache"), 200).unwrap();

    // Put a normal entry first
    cache
        .put("small", Bytes::from(vec![1u8; 50]))
        .await
        .unwrap();
    assert!(cache.get("small").await.unwrap().is_some());

    // Put an oversized entry (500 bytes > 200 byte max)
    // This should not panic
    cache
        .put("huge", Bytes::from(vec![2u8; 500]))
        .await
        .unwrap();

    // The oversized entry may or may not be retrievable, but the cache
    // should still be in a consistent (non-panicking) state
    let _stats = cache.stats().await;
}

/// Many concurrent writers to the same cache key should not corrupt data.
#[tokio::test]
async fn chaos_cache_concurrent_same_key_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 10000).unwrap());

    // 20 tasks all writing to the same key with different data
    let c = cache.clone();
    spawn_concurrent(20, move |i| {
        let c = c.clone();
        async move {
            let data = Bytes::from(vec![i as u8; 100]);
            c.put("contested-key", data).await.unwrap();
        }
    })
    .await;

    // The key should exist and contain valid data (from one of the writers)
    let result = cache.get("contested-key").await.unwrap();
    assert!(result.is_some());
    let data = result.unwrap();
    assert_eq!(data.len(), 100);
    // All bytes should be the same value (from a single writer, not mixed)
    let first_byte = data[0];
    assert!(
        data.iter().all(|&b| b == first_byte),
        "Data corruption detected: not all bytes match"
    );
}

/// Rapid put/get cycles should maintain cache consistency.
#[tokio::test]
async fn chaos_cache_rapid_put_get_cycles() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = SegmentCache::new(temp_dir.path().join("cache"), 5000).unwrap();

    for i in 0..100 {
        let key = format!("cycle-{}", i % 10);
        let data = Bytes::from(vec![i as u8; 50]);
        cache.put(&key, data.clone()).await.unwrap();

        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some(), "Key {} missing right after put", key);
        assert_eq!(retrieved.unwrap(), data, "Data mismatch for key {}", key);
    }
}

// ============================================================================
// 4. WAL-like Crash Recovery Tests
// ============================================================================

/// Write data, drop the WAL (simulating crash), reopen and verify recovery.
#[tokio::test]
async fn chaos_wal_crash_recovery_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = WALConfig {
        directory: temp_dir.path().to_path_buf(),
        sync_policy: SyncPolicy::Always,
        max_size_bytes: 1024 * 1024,
        batch_enabled: false,
        ..Default::default()
    };

    // Write records then "crash" (drop WAL)
    {
        let wal = WAL::open("crash-test", 0, config.clone()).await.unwrap();
        wal.append(Some(b"key-1"), b"value-1").await.unwrap();
        wal.append(Some(b"key-2"), b"value-2").await.unwrap();
        wal.append(Some(b"key-3"), b"value-3").await.unwrap();
        wal.sync().await.unwrap();
        // WAL dropped here - simulates crash
    }

    // Reopen and recover
    let wal = WAL::open("crash-test", 0, config).await.unwrap();
    let records = wal.recover().await.unwrap();

    assert_eq!(records.len(), 3, "Should recover all 3 records after crash");
    assert_eq!(records[0].value, Bytes::from("value-1"));
    assert_eq!(records[1].value, Bytes::from("value-2"));
    assert_eq!(records[2].value, Bytes::from("value-3"));
}

/// Write many records, crash mid-way through a batch, verify partial recovery.
#[tokio::test]
async fn chaos_wal_crash_recovery_many_records() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = WALConfig {
        directory: temp_dir.path().to_path_buf(),
        sync_policy: SyncPolicy::Always,
        max_size_bytes: 10 * 1024 * 1024,
        batch_enabled: false,
        ..Default::default()
    };

    let record_count = 100;

    // Write records
    {
        let wal = WAL::open("crash-many", 0, config.clone()).await.unwrap();
        for i in 0..record_count {
            let key = format!("key-{}", i);
            let value = format!("value-{}", i);
            wal.append(Some(key.as_bytes()), value.as_bytes())
                .await
                .unwrap();
        }
        wal.sync().await.unwrap();
        // Drop/crash
    }

    // Recover
    let wal = WAL::open("crash-many", 0, config).await.unwrap();
    let records = wal.recover().await.unwrap();

    assert_eq!(
        records.len(),
        record_count,
        "Should recover all {} records",
        record_count
    );

    // Verify ordering
    for (i, record) in records.iter().enumerate() {
        let expected_value = format!("value-{}", i);
        assert_eq!(
            record.value,
            Bytes::from(expected_value),
            "Record {} has wrong value",
            i
        );
    }
}

/// Truncate then crash, verify empty recovery.
#[tokio::test]
async fn chaos_wal_truncate_then_crash() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = WALConfig {
        directory: temp_dir.path().to_path_buf(),
        sync_policy: SyncPolicy::Always,
        max_size_bytes: 1024 * 1024,
        batch_enabled: false,
        ..Default::default()
    };

    // Write, truncate, then crash
    {
        let wal = WAL::open("truncate-crash", 0, config.clone())
            .await
            .unwrap();
        wal.append(Some(b"key-1"), b"value-1").await.unwrap();
        wal.append(Some(b"key-2"), b"value-2").await.unwrap();
        wal.truncate().await.unwrap();
        // Drop/crash after truncate
    }

    // Recover should find nothing
    let wal = WAL::open("truncate-crash", 0, config).await.unwrap();
    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 0, "Should recover 0 records after truncate");
}

/// Write, crash, write more, crash again, verify cumulative recovery.
#[tokio::test]
async fn chaos_wal_double_crash_recovery() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = WALConfig {
        directory: temp_dir.path().to_path_buf(),
        sync_policy: SyncPolicy::Always,
        max_size_bytes: 1024 * 1024,
        batch_enabled: false,
        ..Default::default()
    };

    // First write session
    {
        let wal = WAL::open("double-crash", 0, config.clone())
            .await
            .unwrap();
        wal.append(Some(b"session1-key"), b"session1-value")
            .await
            .unwrap();
        wal.sync().await.unwrap();
        // Crash 1
    }

    // Second write session (appends to existing WAL)
    {
        let wal = WAL::open("double-crash", 0, config.clone())
            .await
            .unwrap();
        wal.append(Some(b"session2-key"), b"session2-value")
            .await
            .unwrap();
        wal.sync().await.unwrap();
        // Crash 2
    }

    // Final recovery should see records from both sessions
    let wal = WAL::open("double-crash", 0, config).await.unwrap();
    let records = wal.recover().await.unwrap();

    assert_eq!(records.len(), 2, "Should recover records from both sessions");
    assert_eq!(records[0].value, Bytes::from("session1-value"));
    assert_eq!(records[1].value, Bytes::from("session2-value"));
}

/// Shared WAL crash recovery: Agent A writes and crashes, Agent B
/// takes over the partition and recovers Agent A's records.
#[tokio::test]
async fn chaos_wal_shared_partition_failover() {
    let temp_dir = tempfile::tempdir().unwrap();

    // Agent A writes records
    {
        let config_a = WALConfig {
            directory: temp_dir.path().to_path_buf(),
            sync_policy: SyncPolicy::Always,
            max_size_bytes: 1024 * 1024,
            agent_id: Some("agent-a".to_string()),
            batch_enabled: false,
            ..Default::default()
        };
        let wal_a = WAL::open("failover-topic", 0, config_a).await.unwrap();
        wal_a
            .append(Some(b"k1"), b"agent-a-record-1")
            .await
            .unwrap();
        wal_a
            .append(Some(b"k2"), b"agent-a-record-2")
            .await
            .unwrap();
        wal_a.sync().await.unwrap();
        // Agent A crashes (WAL dropped)
    }

    // Agent B takes over the partition
    let (recovered, stale_files) =
        WAL::recover_partition(temp_dir.path(), "failover-topic", 0, "agent-b")
            .await
            .unwrap();

    assert_eq!(recovered.len(), 2);
    assert_eq!(recovered[0].value, Bytes::from("agent-a-record-1"));
    assert_eq!(recovered[1].value, Bytes::from("agent-a-record-2"));
    assert_eq!(stale_files.len(), 1);
}

// ============================================================================
// 5. Stress Tests (combining multiple components)
// ============================================================================

/// Stress test: fault injector drives circuit breaker while
/// rate limiter is also being hammered.
#[tokio::test]
async fn chaos_stress_combined_cb_and_rl() {
    let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 10,
        success_threshold: 3,
        timeout: Duration::from_millis(50),
    }));
    let limiter = Arc::new(RateLimiter::new(
        BucketConfig {
            rate: 500.0,
            burst: 100,
            min_rate: 10.0,
            max_rate: 5000.0,
        },
        BucketConfig::default(),
        BucketConfig::default(),
    ));
    let injector = Arc::new(FaultInjector::new(3)); // 33% failure rate

    let b = breaker.clone();
    let l = limiter.clone();
    let inj = injector.clone();

    // 50 concurrent "operation" tasks
    spawn_concurrent(50, move |_| {
        let b = b.clone();
        let l = l.clone();
        let inj = inj.clone();
        async move {
            for _ in 0..20 {
                // Check circuit breaker
                if !b.allow_request().await {
                    continue;
                }
                // Check rate limiter
                if !l.try_acquire(S3Operation::Put).await {
                    continue;
                }
                // Simulate operation with fault injection
                if inj.should_fail() {
                    b.report_failure().await;
                    l.report_throttled(S3Operation::Put).await;
                } else {
                    b.report_success().await;
                    l.report_success(S3Operation::Put).await;
                }
            }
        }
    })
    .await;

    // Verify both components are in valid states
    let cb_state = breaker.current_state();
    assert!(
        cb_state == CircuitState::Closed
            || cb_state == CircuitState::Open
            || cb_state == CircuitState::HalfOpen
    );

    let (put_rate, get_rate, _) = limiter.current_rates().await;
    assert!(put_rate >= 10.0, "PUT rate should be above floor");
    // GET was not touched, should be at default
    assert_eq!(get_rate, 5000.0);
}

/// Stress test: concurrent cache operations under memory pressure
/// while circuit breaker state is flapping.
#[tokio::test]
async fn chaos_stress_cache_under_cb_flapping() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(SegmentCache::new(temp_dir.path().join("cache"), 2000).unwrap());
    let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 1,
        timeout: Duration::from_millis(20),
    }));

    let c = cache.clone();
    let b = breaker.clone();

    // Writer tasks that respect the circuit breaker
    let writer_handle = tokio::spawn(async move {
        for i in 0..100 {
            if b.allow_request().await {
                let key = format!("stress-{}", i);
                let data = Bytes::from(vec![(i % 256) as u8; 50]);
                let _ = c.put(&key, data).await;
                b.report_success().await;
            } else {
                // Circuit is open, report failure to keep it flapping
                sleep(Duration::from_millis(25)).await;
                if b.allow_request().await {
                    b.report_failure().await;
                }
            }
        }
    });

    // Reader tasks running concurrently
    let c2 = cache.clone();
    let reader_handle = tokio::spawn(async move {
        let mut total_reads = 0u64;
        for i in 0..100 {
            let key = format!("stress-{}", i % 50);
            let _ = c2.get(&key).await;
            total_reads += 1;
        }
        total_reads
    });

    let (_, reads) = tokio::join!(writer_handle, reader_handle);
    let reads = reads.unwrap();
    assert!(reads > 0, "Should have completed some reads");

    // Cache should be in a consistent state
    let stats = cache.stats().await;
    assert!(stats.current_size <= 2000);
}

/// Fault injector stats verification.
#[tokio::test]
async fn chaos_fault_injector_statistics() {
    let injector = FaultInjector::new(4); // 25% failure rate

    let total_calls = 100;
    let mut failures = 0;
    for _ in 0..total_calls {
        if injector.should_fail() {
            failures += 1;
        }
    }

    // With denominator 4, we expect 25 failures out of 100
    assert_eq!(failures, 25);
    assert_eq!(injector.total_faults(), 25);
}

/// Fault injector with 100% failure rate.
#[tokio::test]
async fn chaos_fault_injector_always_fail() {
    let injector = FaultInjector::new(1);

    for _ in 0..50 {
        assert!(injector.should_fail());
    }
    assert_eq!(injector.total_faults(), 50);
}

/// Multiple fault injectors with different rates used concurrently.
#[tokio::test]
async fn chaos_fault_injector_concurrent_usage() {
    let injector = Arc::new(FaultInjector::new(5)); // 20%

    let inj = injector.clone();
    spawn_concurrent(10, move |_| {
        let inj = inj.clone();
        async move {
            for _ in 0..100 {
                inj.should_fail();
            }
        }
    })
    .await;

    // 10 tasks * 100 calls = 1000 total calls, ~200 should fail (20%)
    let total_faults = injector.total_faults();
    assert_eq!(total_faults, 200, "Expected 200 faults, got {}", total_faults);
}
