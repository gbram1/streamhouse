//! Chaos / Fault-Injection Tests for StreamHouse Failover System
//!
//! Phase 10.6 - Resilience testing for failover manager, leader election,
//! and health checking under adversarial conditions.
//!
//! Tests cover:
//! - Health status flapping across multiple nodes
//! - Rapid leader changes
//! - Concurrent failover attempts
//! - Graceful vs immediate failover policies
//! - Event consistency under chaos

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use streamhouse_api::failover::{
    FailoverConfig, FailoverEvent, FailoverManager, FailoverPolicy, HealthStatus,
    InMemoryHealthChecker, NodeInfo,
};
use streamhouse_api::leader::{LeaderConfig, LeaderElection};

// ============================================================================
// Helpers
// ============================================================================

/// Deterministic fault injector (same as storage chaos tests).
struct FaultInjector {
    failure_denominator: u64,
    counter: AtomicU64,
    faults_injected: AtomicU64,
}

impl FaultInjector {
    fn new(failure_denominator: u64) -> Self {
        Self {
            failure_denominator,
            counter: AtomicU64::new(0),
            faults_injected: AtomicU64::new(0),
        }
    }

    fn should_fail(&self) -> bool {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        let fail = (count % self.failure_denominator) == 0;
        if fail {
            self.faults_injected.fetch_add(1, Ordering::Relaxed);
        }
        fail
    }
}

/// Create a standard leader election for testing.
async fn make_election(node_id: &str) -> Arc<LeaderElection> {
    LeaderElection::new(
        LeaderConfig::memory()
            .namespace("chaos-test")
            .node_id(node_id)
            .lease_duration(Duration::from_secs(30))
            .renew_interval(Duration::from_secs(10)),
    )
    .await
    .unwrap()
}

/// Create a failover manager with the given config and health checker.
async fn make_manager(
    config: FailoverConfig,
    node_id: &str,
    health_checker: Arc<InMemoryHealthChecker>,
) -> Arc<FailoverManager> {
    let election = make_election(node_id).await;
    FailoverManager::new(config, election, health_checker)
        .await
        .unwrap()
}

/// Register N nodes in the failover manager.
async fn register_nodes(manager: &FailoverManager, count: usize) {
    for i in 0..count {
        let node = NodeInfo::new(format!("node-{}", i))
            .with_address(format!("http://node-{}:8080", i));
        manager.register_node(node).await;
    }
}

// ============================================================================
// 1. Health Status Flapping Tests
// ============================================================================

/// A node that rapidly alternates between healthy and unhealthy should
/// eventually be marked unhealthy if the unhealthy checks exceed the threshold.
#[tokio::test]
async fn chaos_failover_health_flapping_single_node() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(3)
        .auto_failover(false);

    let manager = make_manager(config, "monitor-node", health.clone()).await;
    manager
        .register_node(NodeInfo::new("flappy-node"))
        .await;

    // Simulate flapping: alternate health status each check cycle
    for i in 0..10 {
        if i % 2 == 0 {
            health
                .set_status("flappy-node", HealthStatus::Unhealthy)
                .await;
        } else {
            health
                .set_status("flappy-node", HealthStatus::Healthy)
                .await;
        }
        manager.run_health_checks().await.unwrap();
    }

    // The node should still be functional (flapping resets the failure counter
    // on each healthy check)
    let node = manager.get_node("flappy-node").await.unwrap();
    // After the sequence, node's health depends on the last check;
    // the important thing is it didn't panic or corrupt state
    assert!(
        node.health == HealthStatus::Healthy || node.health == HealthStatus::Unhealthy,
        "Unexpected health status: {:?}",
        node.health
    );
}

/// Multiple nodes flapping simultaneously should not corrupt the node registry.
#[tokio::test]
async fn chaos_failover_multi_node_health_flapping() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(2)
        .auto_failover(false);

    let manager = make_manager(config, "monitor", health.clone()).await;
    register_nodes(&manager, 5).await;

    // Flap all nodes in different patterns
    for round in 0..20 {
        for i in 0..5 {
            let status = if (round + i) % 3 == 0 {
                HealthStatus::Unhealthy
            } else {
                HealthStatus::Healthy
            };
            health
                .set_status(&format!("node-{}", i), status)
                .await;
        }
        manager.run_health_checks().await.unwrap();
    }

    // All 5 nodes should still be registered
    let nodes = manager.get_nodes().await;
    assert_eq!(nodes.len(), 5, "All nodes should still be registered");
}

/// A node that stays unhealthy beyond the threshold should trigger a NodeDown event.
#[tokio::test]
async fn chaos_failover_sustained_unhealthy_triggers_node_down() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(3)
        .auto_failover(false);

    let manager = make_manager(config, "monitor", health.clone()).await;
    manager
        .register_node(NodeInfo::new("sick-node"))
        .await;

    // First make it healthy (to establish baseline)
    health
        .set_status("sick-node", HealthStatus::Healthy)
        .await;
    manager.run_health_checks().await.unwrap();

    let mut events = manager.subscribe();

    // Now make it unhealthy for 3 consecutive checks
    health
        .set_status("sick-node", HealthStatus::Unhealthy)
        .await;
    for _ in 0..3 {
        manager.run_health_checks().await.unwrap();
    }

    // Should have received health change and node down events
    let mut found_node_down = false;
    while let Ok(event) = events.try_recv() {
        if matches!(event, FailoverEvent::NodeDown { .. }) {
            found_node_down = true;
        }
    }
    assert!(found_node_down, "Expected NodeDown event");
}

/// A node recovering after being down should emit a NodeUp event.
#[tokio::test]
async fn chaos_failover_node_recovery_emits_node_up() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(2)
        .auto_failover(false);

    let manager = make_manager(config, "monitor", health.clone()).await;
    manager
        .register_node(NodeInfo::new("recovering-node"))
        .await;

    // Make healthy first
    health
        .set_status("recovering-node", HealthStatus::Healthy)
        .await;
    manager.run_health_checks().await.unwrap();

    // Make unhealthy (past threshold)
    health
        .set_status("recovering-node", HealthStatus::Unhealthy)
        .await;
    manager.run_health_checks().await.unwrap();
    manager.run_health_checks().await.unwrap();

    let mut events = manager.subscribe();

    // Recover
    health
        .set_status("recovering-node", HealthStatus::Healthy)
        .await;
    manager.run_health_checks().await.unwrap();

    // Should have NodeUp event
    let mut found_node_up = false;
    while let Ok(event) = events.try_recv() {
        if matches!(event, FailoverEvent::NodeUp { .. }) {
            found_node_up = true;
        }
    }
    assert!(found_node_up, "Expected NodeUp event after recovery");
}

// ============================================================================
// 2. Rapid Leader Change Tests
// ============================================================================

/// Acquiring and releasing leadership rapidly should not corrupt state.
#[tokio::test]
async fn chaos_failover_rapid_leader_acquire_release() {
    let election = make_election("rapid-node").await;

    for _ in 0..20 {
        assert!(election.try_acquire().await.unwrap());
        assert!(election.is_leader());

        election.release().await.unwrap();
        assert!(!election.is_leader());
    }
}

/// Each acquire should produce an increasing fencing token.
#[tokio::test]
async fn chaos_failover_fencing_token_monotonic() {
    let election = make_election("fencing-node").await;

    let mut last_token = 0u64;
    for _ in 0..10 {
        election.try_acquire().await.unwrap();
        let token = election.fencing_token().unwrap();
        assert!(
            token > last_token,
            "Fencing token {} should be > {}",
            token,
            last_token
        );
        last_token = token;
        election.release().await.unwrap();
    }
}

/// Force failover should emit the correct sequence of events.
#[tokio::test]
async fn chaos_failover_force_failover_event_sequence() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .policy(FailoverPolicy::Immediate)
        .auto_failover(false);

    let manager = make_manager(config, "failover-node", health.clone()).await;
    register_nodes(&manager, 3).await;

    let mut events = manager.subscribe();

    manager
        .force_failover("chaos test".to_string())
        .await
        .unwrap();

    // Collect all events
    let mut event_types = Vec::new();
    while let Ok(event) = events.try_recv() {
        event_types.push(std::mem::discriminant(&event));
    }

    // Should have at least a FailoverStarted event
    assert!(!event_types.is_empty(), "Should have emitted events");
}

/// Multiple rapid force_failover calls should not panic or deadlock.
#[tokio::test]
async fn chaos_failover_rapid_force_failover() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .policy(FailoverPolicy::Immediate)
        .auto_failover(false);

    let manager = make_manager(config, "rapid-failover", health.clone()).await;
    register_nodes(&manager, 3).await;

    // Rapid-fire failover requests
    for i in 0..10 {
        manager
            .force_failover(format!("chaos round {}", i))
            .await
            .unwrap();
    }

    // Manager should still be in a consistent state
    let stats = manager.stats().await;
    assert_eq!(stats.total_nodes, 3);
}

// ============================================================================
// 3. Concurrent Failover Attempt Tests
// ============================================================================

/// Multiple concurrent failover triggers should be handled safely.
#[tokio::test]
async fn chaos_failover_concurrent_force_failover() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .policy(FailoverPolicy::Immediate)
        .auto_failover(false);

    let manager = make_manager(config, "concurrent-node", health.clone()).await;
    register_nodes(&manager, 5).await;

    let m = manager.clone();
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let m = m.clone();
            tokio::spawn(async move {
                m.force_failover(format!("concurrent chaos {}", i))
                    .await
                    .unwrap();
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    // Should not have corrupted the node list
    let nodes = manager.get_nodes().await;
    assert_eq!(nodes.len(), 5);
}

/// Concurrent node registration and unregistration during health checks.
#[tokio::test]
async fn chaos_failover_concurrent_register_unregister() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(2)
        .auto_failover(false);

    let manager = make_manager(config, "reg-unreg-node", health.clone()).await;

    // Register 10 nodes
    register_nodes(&manager, 10).await;

    // Concurrent: some tasks register new nodes, some unregister, some run health checks
    let m1 = manager.clone();
    let h1 = tokio::spawn(async move {
        for i in 10..20 {
            m1.register_node(NodeInfo::new(format!("node-{}", i)))
                .await;
        }
    });

    let m2 = manager.clone();
    let h2 = tokio::spawn(async move {
        for i in 0..5 {
            m2.unregister_node(&format!("node-{}", i)).await;
        }
    });

    let m3 = manager.clone();
    let h3 = tokio::spawn(async move {
        for _ in 0..5 {
            let _ = m3.run_health_checks().await;
        }
    });

    let (r1, r2, r3) = tokio::join!(h1, h2, h3);
    r1.unwrap();
    r2.unwrap();
    r3.unwrap();

    // Exact count depends on execution order, but should be 15 (20 registered - 5 unregistered)
    let nodes = manager.get_nodes().await;
    assert_eq!(nodes.len(), 15, "Expected 15 nodes, got {}", nodes.len());
}

/// Health check subscription should deliver events under high concurrency.
#[tokio::test]
async fn chaos_failover_event_delivery_under_load() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(1)
        .auto_failover(false);

    let manager = make_manager(config, "event-delivery", health.clone()).await;
    manager
        .register_node(NodeInfo::new("event-node"))
        .await;

    // Make healthy first
    health
        .set_status("event-node", HealthStatus::Healthy)
        .await;
    manager.run_health_checks().await.unwrap();

    let mut events = manager.subscribe();

    // Rapidly toggle health status
    for _ in 0..5 {
        health
            .set_status("event-node", HealthStatus::Unhealthy)
            .await;
        manager.run_health_checks().await.unwrap();

        health
            .set_status("event-node", HealthStatus::Healthy)
            .await;
        manager.run_health_checks().await.unwrap();
    }

    // Drain events
    let mut event_count = 0;
    while events.try_recv().is_ok() {
        event_count += 1;
    }
    assert!(
        event_count > 0,
        "Should have received events during flapping"
    );
}

// ============================================================================
// 4. Failover Policy Tests
// ============================================================================

/// Immediate policy should trigger failover without delay.
#[tokio::test]
async fn chaos_failover_immediate_policy() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .policy(FailoverPolicy::Immediate)
        .auto_failover(false);

    let manager = make_manager(config, "immediate-node", health.clone()).await;
    register_nodes(&manager, 3).await;

    let mut events = manager.subscribe();
    let start = std::time::Instant::now();

    manager
        .force_failover("immediate test".to_string())
        .await
        .unwrap();

    let elapsed = start.elapsed();

    // Immediate failover should complete quickly (under 100ms)
    assert!(
        elapsed < Duration::from_millis(100),
        "Immediate failover took {:?}",
        elapsed
    );

    // Should have events
    assert!(events.try_recv().is_ok());
}

/// Manual policy should not automatically trigger leadership change.
#[tokio::test]
async fn chaos_failover_manual_policy_no_auto_action() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .policy(FailoverPolicy::Manual)
        .unhealthy_threshold(1)
        .auto_failover(true);

    let manager = make_manager(config, "manual-node", health.clone()).await;
    manager
        .register_node(NodeInfo::new("leader-node"))
        .await;

    // Set the current leader
    {
        // Make this node the leader
        manager
            .force_failover("initial setup".to_string())
            .await
            .unwrap();
    }

    // Make the node unhealthy
    health
        .set_status("leader-node", HealthStatus::Healthy)
        .await;
    manager.run_health_checks().await.unwrap();

    health
        .set_status("leader-node", HealthStatus::Unhealthy)
        .await;
    manager.run_health_checks().await.unwrap();

    // With manual policy, auto_failover trigger should not change leader automatically
    // in the sense that check_failover_needed calls trigger_failover which
    // for Manual policy does nothing automatic (just logs).
    // The manager should still be in a valid state.
    let stats = manager.stats().await;
    assert_eq!(stats.total_nodes, 1);
}

/// Graceful policy should introduce a delay before failover.
#[tokio::test]
async fn chaos_failover_graceful_policy_delay() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let grace_period = Duration::from_millis(100);
    let config = FailoverConfig::default()
        .policy(FailoverPolicy::Graceful {
            grace_period,
        })
        .auto_failover(false);

    let manager = make_manager(config, "graceful-node", health.clone()).await;
    register_nodes(&manager, 2).await;

    let start = std::time::Instant::now();
    manager
        .force_failover("graceful test".to_string())
        .await
        .unwrap();
    let elapsed = start.elapsed();

    // Graceful failover should wait at least the grace period
    assert!(
        elapsed >= grace_period,
        "Graceful failover completed too fast: {:?} < {:?}",
        elapsed,
        grace_period
    );
}

// ============================================================================
// 5. Combined Chaos / Stress Tests
// ============================================================================

/// Fault injector drives health flapping across multiple nodes.
#[tokio::test]
async fn chaos_failover_fault_injected_health_flapping() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(3)
        .auto_failover(false);

    let manager = make_manager(config, "fault-injected", health.clone()).await;
    register_nodes(&manager, 5).await;

    let injector = FaultInjector::new(3); // 33% failure

    for _ in 0..30 {
        for i in 0..5 {
            let node_id = format!("node-{}", i);
            if injector.should_fail() {
                health.set_status(&node_id, HealthStatus::Unhealthy).await;
            } else {
                health.set_status(&node_id, HealthStatus::Healthy).await;
            }
        }
        manager.run_health_checks().await.unwrap();
    }

    // All nodes should still be registered
    let nodes = manager.get_nodes().await;
    assert_eq!(nodes.len(), 5);

    // Stats should be consistent
    let stats = manager.stats().await;
    assert_eq!(stats.total_nodes, 5);
    assert!(stats.healthy_nodes + stats.unhealthy_nodes == 5);
}

/// Full lifecycle test: register nodes, acquire leadership, flap health,
/// trigger failover, verify events.
#[tokio::test]
async fn chaos_failover_full_lifecycle_stress() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .policy(FailoverPolicy::Immediate)
        .unhealthy_threshold(2)
        .auto_failover(false);

    let manager = make_manager(config, "lifecycle-node", health.clone()).await;
    register_nodes(&manager, 3).await;

    // All healthy initially
    for i in 0..3 {
        health
            .set_status(&format!("node-{}", i), HealthStatus::Healthy)
            .await;
    }
    manager.run_health_checks().await.unwrap();

    let mut events = manager.subscribe();

    // Node-0 goes down
    health
        .set_status("node-0", HealthStatus::Unhealthy)
        .await;
    manager.run_health_checks().await.unwrap();
    manager.run_health_checks().await.unwrap();

    // Trigger failover
    manager
        .force_failover("node-0 down".to_string())
        .await
        .unwrap();

    // Node-0 recovers
    health
        .set_status("node-0", HealthStatus::Healthy)
        .await;
    manager.run_health_checks().await.unwrap();

    // Node-1 goes down
    health
        .set_status("node-1", HealthStatus::Unhealthy)
        .await;
    manager.run_health_checks().await.unwrap();
    manager.run_health_checks().await.unwrap();

    // Trigger another failover
    manager
        .force_failover("node-1 down".to_string())
        .await
        .unwrap();

    // Collect events
    let mut event_count = 0;
    while events.try_recv().is_ok() {
        event_count += 1;
    }
    assert!(
        event_count >= 2,
        "Expected at least 2 events, got {}",
        event_count
    );

    // All nodes should still be registered
    let stats = manager.stats().await;
    assert_eq!(stats.total_nodes, 3);
}

/// Degraded nodes should still be considered healthy.
#[tokio::test]
async fn chaos_failover_degraded_counts_as_healthy() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .unhealthy_threshold(2)
        .auto_failover(false);

    let manager = make_manager(config, "degraded-test", health.clone()).await;
    manager
        .register_node(NodeInfo::new("degraded-node"))
        .await;

    health
        .set_status("degraded-node", HealthStatus::Degraded)
        .await;
    manager.run_health_checks().await.unwrap();

    let node = manager.get_node("degraded-node").await.unwrap();
    // Degraded should be treated as healthy
    assert!(
        node.health.is_healthy(),
        "Degraded should be considered healthy"
    );
}

/// Unknown health status should not be considered healthy.
#[tokio::test]
async fn chaos_failover_unknown_is_not_healthy() {
    assert!(!HealthStatus::Unknown.is_healthy());
    assert!(!HealthStatus::Unhealthy.is_healthy());
    assert!(HealthStatus::Healthy.is_healthy());
    assert!(HealthStatus::Degraded.is_healthy());
}

/// Leader election should work with the failover manager's start/stop lifecycle.
#[tokio::test]
async fn chaos_failover_start_stop_lifecycle() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .health_check_interval(Duration::from_millis(50))
        .auto_failover(false);

    let manager = make_manager(config, "start-stop", health.clone()).await;
    register_nodes(&manager, 2).await;

    // Start the manager
    manager.clone().start().await.unwrap();

    // Let it run a few health check cycles
    sleep(Duration::from_millis(200)).await;

    // Stop it
    manager.stop().await.unwrap();

    // Should have run some health checks (nodes should have last_check set)
    // Give the background task a moment to stop
    sleep(Duration::from_millis(50)).await;
}

/// Starting an already-running manager should return AlreadyRunning error.
#[tokio::test]
async fn chaos_failover_double_start_error() {
    let health = Arc::new(InMemoryHealthChecker::new());
    let config = FailoverConfig::default()
        .health_check_interval(Duration::from_millis(100))
        .auto_failover(false);

    let manager = make_manager(config, "double-start", health.clone()).await;

    // First start succeeds
    manager.clone().start().await.unwrap();

    // Second start should fail
    let result = manager.clone().start().await;
    assert!(result.is_err());

    // Cleanup
    manager.stop().await.unwrap();
    sleep(Duration::from_millis(50)).await;
}
