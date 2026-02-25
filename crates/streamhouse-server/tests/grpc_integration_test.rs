//! Integration tests for StreamHouse server configuration types.
//!
//! These tests verify that the public config structs exported from
//! `streamhouse_server` have correct defaults and can be customized.

use std::time::Duration;

use streamhouse_server::CompactionConfig;
use streamhouse_server::MaintenanceConfig;

#[test]
fn test_compaction_config_defaults_via_public_api() {
    let config = CompactionConfig::default();
    // 5-minute default check interval
    assert_eq!(config.check_interval, Duration::from_secs(300));
    // 1 million records per run
    assert_eq!(config.max_records_per_run, 1_000_000);
    // 1-hour minimum segment age
    assert_eq!(config.min_segment_age, Duration::from_secs(3600));
    // 24-hour tombstone retention
    assert_eq!(config.tombstone_retention, Duration::from_secs(86400));
}

#[test]
fn test_maintenance_config_defaults_via_public_api() {
    let config = MaintenanceConfig::default();
    // 5-second tick interval
    assert_eq!(config.tick_interval, Duration::from_secs(5));
    // 10,000 records per tick per partition
    assert_eq!(config.max_records_per_tick, 10_000);
    // 60-second view timeout
    assert_eq!(config.view_timeout, Duration::from_secs(60));
}

#[test]
fn test_compaction_config_custom_construction() {
    let config = CompactionConfig {
        check_interval: Duration::from_secs(10),
        max_records_per_run: 100,
        min_segment_age: Duration::from_secs(0),
        tombstone_retention: Duration::from_secs(60),
    };
    assert_eq!(config.check_interval, Duration::from_secs(10));
    assert_eq!(config.max_records_per_run, 100);
    assert_eq!(config.min_segment_age, Duration::ZERO);
    assert_eq!(config.tombstone_retention, Duration::from_secs(60));
}
