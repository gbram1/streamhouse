use lazy_static::lazy_static;
use prometheus::{
    HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
};
use std::sync::Once;

static INIT: Once = Once::new();

lazy_static! {
    /// Global Prometheus metrics registry
    pub static ref REGISTRY: Registry = Registry::new();

    // ============================================================================
    // Producer Metrics
    // ============================================================================

    /// Total number of records produced
    pub static ref PRODUCER_RECORDS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_producer_records_total", "Total records produced"),
        &["topic"]
    ).expect("metric can be created");

    /// Total bytes produced
    pub static ref PRODUCER_BYTES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_producer_bytes_total", "Total bytes produced"),
        &["topic"]
    ).expect("metric can be created");

    /// Producer request latency
    pub static ref PRODUCER_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_producer_latency_seconds", "Producer latency in seconds")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
        &["topic"]
    ).expect("metric can be created");

    /// Producer batch size
    pub static ref PRODUCER_BATCH_SIZE: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_producer_batch_size", "Producer batch size in records")
            .buckets(vec![1.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0]),
        &["topic"]
    ).expect("metric can be created");

    /// Producer errors
    pub static ref PRODUCER_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_producer_errors_total", "Total producer errors"),
        &["topic", "error_type"]
    ).expect("metric can be created");

    // ============================================================================
    // Consumer Metrics
    // ============================================================================

    /// Total number of records consumed
    pub static ref CONSUMER_RECORDS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_consumer_records_total", "Total records consumed"),
        &["topic", "consumer_group"]
    ).expect("metric can be created");

    /// Consumer lag (difference between latest offset and committed offset)
    pub static ref CONSUMER_LAG: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_consumer_lag", "Consumer lag in number of messages"),
        &["topic", "partition", "consumer_group"]
    ).expect("metric can be created");

    /// Consumer rebalances
    pub static ref CONSUMER_REBALANCES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_consumer_rebalances_total", "Total consumer rebalances"),
        &["consumer_group"]
    ).expect("metric can be created");

    /// Consumer errors
    pub static ref CONSUMER_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_consumer_errors_total", "Total consumer errors"),
        &["topic", "consumer_group", "error_type"]
    ).expect("metric can be created");

    // ============================================================================
    // Storage Metrics
    // ============================================================================

    /// Total segment writes
    pub static ref SEGMENT_WRITES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_segment_writes_total", "Total segments written"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// Total segment flushes to S3
    pub static ref SEGMENT_FLUSHES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_segment_flushes_total", "Total segment flushes to S3"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// S3 requests by operation type
    pub static ref S3_REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_s3_requests_total", "Total S3 requests"),
        &["operation"] // GET, PUT, DELETE, LIST, HEAD
    ).expect("metric can be created");

    /// S3 errors by type
    pub static ref S3_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_s3_errors_total", "Total S3 errors"),
        &["operation", "error_type"] // throttling, not_found, access_denied, etc.
    ).expect("metric can be created");

    /// S3 request latency
    pub static ref S3_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_s3_latency_seconds", "S3 request latency in seconds")
            .buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
        &["operation"]
    ).expect("metric can be created");

    /// Cache hits
    pub static ref CACHE_HITS_TOTAL: IntCounter = IntCounter::new(
        "streamhouse_cache_hits_total",
        "Total cache hits"
    ).expect("metric can be created");

    /// Cache misses
    pub static ref CACHE_MISSES_TOTAL: IntCounter = IntCounter::new(
        "streamhouse_cache_misses_total",
        "Total cache misses"
    ).expect("metric can be created");

    /// Cache size in bytes
    pub static ref CACHE_SIZE_BYTES: IntGauge = IntGauge::new(
        "streamhouse_cache_size_bytes",
        "Current cache size in bytes"
    ).expect("metric can be created");

    // ============================================================================
    // System Metrics
    // ============================================================================

    /// Active connections
    pub static ref CONNECTIONS_ACTIVE: IntGauge = IntGauge::new(
        "streamhouse_connections_active",
        "Number of active connections"
    ).expect("metric can be created");

    /// Total partitions
    pub static ref PARTITIONS_TOTAL: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_partitions_total", "Total partitions"),
        &["topic"]
    ).expect("metric can be created");

    /// Total topics
    pub static ref TOPICS_TOTAL: IntGauge = IntGauge::new(
        "streamhouse_topics_total",
        "Total topics"
    ).expect("metric can be created");

    /// Active agents
    pub static ref AGENTS_ACTIVE: IntGauge = IntGauge::new(
        "streamhouse_agents_active",
        "Number of active agents"
    ).expect("metric can be created");

    /// Server uptime in seconds
    pub static ref UPTIME_SECONDS: IntGauge = IntGauge::new(
        "streamhouse_uptime_seconds",
        "Server uptime in seconds"
    ).expect("metric can be created");

    /// Database errors (connection failures, query errors)
    pub static ref DATABASE_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_database_errors_total", "Total database errors"),
        &["operation", "error_type"] // operation=query/connect, error_type=timeout/connection_refused/etc.
    ).expect("metric can be created");

    // ============================================================================
    // Throttle & Circuit Breaker Metrics (Phase 12.4.2)
    // ============================================================================

    /// Throttle decisions by type (allow, rate_limited, circuit_open)
    pub static ref THROTTLE_DECISIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_throttle_decisions_total", "Total throttle decisions"),
        &["operation", "decision"] // operation=put/get/delete, decision=allow/rate_limited/circuit_open
    ).expect("metric can be created");

    /// Current rate limit for S3 operations (ops/sec)
    pub static ref THROTTLE_RATE_CURRENT: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_throttle_rate_current", "Current rate limit in operations per second"),
        &["operation"] // put, get, delete
    ).expect("metric can be created");

    /// Circuit breaker state (0=Closed, 1=Open, 2=HalfOpen)
    pub static ref CIRCUIT_BREAKER_STATE: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_circuit_breaker_state", "Circuit breaker state (0=Closed, 1=Open, 2=HalfOpen)"),
        &["operation"] // put, get, delete
    ).expect("metric can be created");

    /// Circuit breaker state transitions
    pub static ref CIRCUIT_BREAKER_TRANSITIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_circuit_breaker_transitions_total", "Total circuit breaker state transitions"),
        &["operation", "from_state", "to_state"] // from_state/to_state: closed/open/half_open
    ).expect("metric can be created");

    /// Circuit breaker failure count
    pub static ref CIRCUIT_BREAKER_FAILURES: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_circuit_breaker_failures", "Current consecutive failure count"),
        &["operation"]
    ).expect("metric can be created");

    /// Circuit breaker success count (in half-open state)
    pub static ref CIRCUIT_BREAKER_SUCCESSES: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_circuit_breaker_successes", "Current consecutive success count in half-open"),
        &["operation"]
    ).expect("metric can be created");

    // ============================================================================
    // Lease Manager Metrics (Phase 4.2)
    // ============================================================================

    /// Partition lease acquisitions
    pub static ref LEASE_ACQUISITIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_lease_acquisitions_total", "Total partition lease acquisitions"),
        &["topic", "partition", "result"] // result=success/conflict/error
    ).expect("metric can be created");

    /// Partition lease renewals
    pub static ref LEASE_RENEWALS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_lease_renewals_total", "Total partition lease renewals"),
        &["topic", "partition", "result"] // result=success/failure
    ).expect("metric can be created");

    /// Lease conflicts (stale epoch rejections)
    pub static ref LEASE_CONFLICTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_lease_conflicts_total", "Total lease conflicts"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// Current lease epoch
    pub static ref LEASE_EPOCH_CURRENT: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_lease_epoch_current", "Current lease epoch for partition"),
        &["topic", "partition", "agent_id"]
    ).expect("metric can be created");

    /// Lease expiration time (Unix timestamp)
    pub static ref LEASE_EXPIRES_AT: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_lease_expires_at", "Lease expiration timestamp"),
        &["topic", "partition", "agent_id"]
    ).expect("metric can be created");

    // ============================================================================
    // Leader Change Tracking Metrics (Fast Leader Handoff)
    // ============================================================================

    /// Total leader changes by reason
    /// Reasons: lease_expired, graceful_handoff, agent_crash, rebalance
    pub static ref LEADER_CHANGES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_leader_changes_total", "Total leadership changes by reason"),
        &["topic", "partition", "reason"] // reason=lease_expired/graceful_handoff/agent_crash/rebalance
    ).expect("metric can be created");

    /// Leader handoff latency (time from initiation to completion)
    pub static ref LEADER_HANDOFF_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_leader_handoff_latency_seconds", "Leader handoff latency in seconds")
            .buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// Leader gap time (time partition was leaderless)
    pub static ref LEADER_GAP_SECONDS: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_leader_gap_seconds", "Time partition was without a leader")
            .buckets(vec![0.0, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// Pending lease transfers (in-progress graceful handoffs)
    pub static ref LEADER_TRANSFERS_PENDING: IntGauge = IntGauge::new(
        "streamhouse_leader_transfers_pending",
        "Number of pending leader transfers"
    ).expect("metric can be created");

    /// Lease transfer results
    pub static ref LEADER_TRANSFERS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_leader_transfers_total", "Total leader transfer operations"),
        &["result"] // result=success/timeout/rejected/error
    ).expect("metric can be created");

    // ============================================================================
    // WAL Metrics (Phase 10)
    // ============================================================================

    /// WAL append operations
    pub static ref WAL_APPENDS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_wal_appends_total", "Total WAL append operations"),
        &["topic", "partition", "result"] // result=success/error
    ).expect("metric can be created");

    /// WAL recovery operations
    pub static ref WAL_RECOVERIES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_wal_recoveries_total", "Total WAL recovery operations"),
        &["topic", "partition", "result"] // result=success/error
    ).expect("metric can be created");

    /// WAL records recovered
    pub static ref WAL_RECORDS_RECOVERED: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_wal_records_recovered", "Number of records recovered from WAL"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// WAL records skipped (corruption)
    pub static ref WAL_RECORDS_SKIPPED: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_wal_records_skipped", "Number of corrupted WAL records skipped"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// WAL file size in bytes
    pub static ref WAL_SIZE_BYTES: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_wal_size_bytes", "WAL file size in bytes"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// WAL truncate operations
    pub static ref WAL_TRUNCATES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_wal_truncates_total", "Total WAL truncate operations"),
        &["topic", "partition", "result"] // result=success/error
    ).expect("metric can be created");

    // ============================================================================
    // Schema Registry Metrics (Phase 9)
    // ============================================================================

    /// Schema registration operations
    pub static ref SCHEMA_REGISTRATIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_schema_registrations_total", "Total schema registrations"),
        &["subject", "result"] // result=success/error/incompatible
    ).expect("metric can be created");

    /// Schema registry errors by type
    pub static ref SCHEMA_REGISTRY_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_schema_registry_errors_total", "Total schema registry errors"),
        &["type"] // not_found, incompatible, invalid, storage_error, etc.
    ).expect("metric can be created");

    /// Schema compatibility checks
    pub static ref SCHEMA_COMPATIBILITY_CHECKS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_schema_compatibility_checks_total", "Total schema compatibility checks"),
        &["subject", "result"] // result=compatible/incompatible/error
    ).expect("metric can be created");

    /// Schema cache entries
    pub static ref SCHEMA_CACHE_ENTRIES: IntGauge = IntGauge::new(
        "streamhouse_schema_cache_entries",
        "Number of entries in schema cache"
    ).expect("metric can be created");

    /// Schema lookups by ID
    pub static ref SCHEMA_LOOKUPS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_schema_lookups_total", "Total schema lookups by ID"),
        &["result"] // result=hit/miss
    ).expect("metric can be created");

    /// Total schemas registered
    pub static ref SCHEMAS_TOTAL: IntGauge = IntGauge::new(
        "streamhouse_schemas_total",
        "Total number of registered schemas"
    ).expect("metric can be created");

    /// Total subjects
    pub static ref SUBJECTS_TOTAL: IntGauge = IntGauge::new(
        "streamhouse_subjects_total",
        "Total number of schema subjects"
    ).expect("metric can be created");

    // ============================================================================
    // Per-Producer Debugging Metrics (Phase 10.7: Idempotent Producers)
    // ============================================================================

    /// Producer dedup results (accepted, duplicate, gap)
    pub static ref PRODUCER_DEDUP_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_producer_dedup_total", "Total producer dedup checks by result"),
        &["producer_id", "result"] // result=accepted/duplicate/gap
    ).expect("metric can be created");

    /// Current producer sequence number per (producer, topic, partition)
    pub static ref PRODUCER_SEQUENCE_CURRENT: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_producer_sequence_current", "Current producer sequence number"),
        &["producer_id", "topic", "partition"]
    ).expect("metric can be created");

    /// Producer fence events (epoch bumps / fencing of old instances)
    pub static ref PRODUCER_FENCE_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_producer_fence_total", "Total producer fence events"),
        &["producer_id"]
    ).expect("metric can be created");

    // ============================================================================
    // Partition Imbalance Metrics (Phase 10.8: Hot-Partition Detection)
    // ============================================================================

    /// Per-partition write rate in records/sec
    pub static ref PARTITION_WRITE_RATE: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_partition_write_rate", "Partition write rate in records per second"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// Per-partition write rate in bytes/sec
    pub static ref PARTITION_BYTES_RATE: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_partition_bytes_rate", "Partition write rate in bytes per second"),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// Partition imbalance ratio (stdev/mean of write rates across partitions for a topic)
    pub static ref PARTITION_IMBALANCE_RATIO: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_partition_imbalance_ratio", "Partition write rate imbalance ratio (stdev/mean * 1000)"),
        &["topic"]
    ).expect("metric can be created");

    // ============================================================================
    // Backpressure Visibility Metrics (Phase 10.9: Credit-Based Backpressure)
    // ============================================================================

    /// Total backpressure throttle events per producer
    pub static ref BACKPRESSURE_THROTTLE_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("streamhouse_backpressure_throttle_total", "Total backpressure throttle events"),
        &["producer_id"]
    ).expect("metric can be created");

    /// Available credits per producer
    pub static ref BACKPRESSURE_CREDITS_AVAILABLE: IntGaugeVec = IntGaugeVec::new(
        Opts::new("streamhouse_backpressure_credits_available", "Available backpressure credits per producer"),
        &["producer_id"]
    ).expect("metric can be created");

    /// Backpressure wait time distribution in seconds
    pub static ref BACKPRESSURE_WAIT_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_backpressure_wait_time_seconds", "Backpressure wait time in seconds")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
        &["producer_id"]
    ).expect("metric can be created");

    // ============================================================================
    // E2E Latency Breakdown Metrics (Phase 10.10: Enhanced Ops)
    // ============================================================================

    /// End-to-end produce latency (from request arrival to ack)
    pub static ref E2E_PRODUCE_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_e2e_produce_latency_seconds", "End-to-end produce latency in seconds")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
        &["topic"]
    ).expect("metric can be created");

    /// WAL append latency (time to write to the write-ahead log)
    pub static ref WAL_APPEND_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_wal_append_latency_seconds", "WAL append latency in seconds")
            .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25]),
        &["topic", "partition"]
    ).expect("metric can be created");

    /// S3 flush latency (time to upload a segment to S3)
    pub static ref S3_FLUSH_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new("streamhouse_s3_flush_latency_seconds", "S3 flush latency in seconds")
            .buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]),
        &["topic", "partition"]
    ).expect("metric can be created");
}

/// Initialize metrics registry
/// Can be called multiple times safely (idempotent)
pub fn init() {
    INIT.call_once(|| {
        // Producer metrics
        REGISTRY
            .register(Box::new(PRODUCER_RECORDS_TOTAL.clone()))
            .expect("producer_records_total can be registered");
        REGISTRY
            .register(Box::new(PRODUCER_BYTES_TOTAL.clone()))
            .expect("producer_bytes_total can be registered");
        REGISTRY
            .register(Box::new(PRODUCER_LATENCY.clone()))
            .expect("producer_latency can be registered");
        REGISTRY
            .register(Box::new(PRODUCER_BATCH_SIZE.clone()))
            .expect("producer_batch_size can be registered");
        REGISTRY
            .register(Box::new(PRODUCER_ERRORS_TOTAL.clone()))
            .expect("producer_errors_total can be registered");

        // Consumer metrics
        REGISTRY
            .register(Box::new(CONSUMER_RECORDS_TOTAL.clone()))
            .expect("consumer_records_total can be registered");
        REGISTRY
            .register(Box::new(CONSUMER_LAG.clone()))
            .expect("consumer_lag can be registered");
        REGISTRY
            .register(Box::new(CONSUMER_REBALANCES_TOTAL.clone()))
            .expect("consumer_rebalances_total can be registered");
        REGISTRY
            .register(Box::new(CONSUMER_ERRORS_TOTAL.clone()))
            .expect("consumer_errors_total can be registered");

        // Storage metrics
        REGISTRY
            .register(Box::new(SEGMENT_WRITES_TOTAL.clone()))
            .expect("segment_writes_total can be registered");
        REGISTRY
            .register(Box::new(SEGMENT_FLUSHES_TOTAL.clone()))
            .expect("segment_flushes_total can be registered");
        REGISTRY
            .register(Box::new(S3_REQUESTS_TOTAL.clone()))
            .expect("s3_requests_total can be registered");
        REGISTRY
            .register(Box::new(S3_ERRORS_TOTAL.clone()))
            .expect("s3_errors_total can be registered");
        REGISTRY
            .register(Box::new(S3_LATENCY.clone()))
            .expect("s3_latency can be registered");
        REGISTRY
            .register(Box::new(CACHE_HITS_TOTAL.clone()))
            .expect("cache_hits_total can be registered");
        REGISTRY
            .register(Box::new(CACHE_MISSES_TOTAL.clone()))
            .expect("cache_misses_total can be registered");
        REGISTRY
            .register(Box::new(CACHE_SIZE_BYTES.clone()))
            .expect("cache_size_bytes can be registered");

        // System metrics
        REGISTRY
            .register(Box::new(CONNECTIONS_ACTIVE.clone()))
            .expect("connections_active can be registered");
        REGISTRY
            .register(Box::new(PARTITIONS_TOTAL.clone()))
            .expect("partitions_total can be registered");
        REGISTRY
            .register(Box::new(TOPICS_TOTAL.clone()))
            .expect("topics_total can be registered");
        REGISTRY
            .register(Box::new(AGENTS_ACTIVE.clone()))
            .expect("agents_active can be registered");
        REGISTRY
            .register(Box::new(UPTIME_SECONDS.clone()))
            .expect("uptime_seconds can be registered");
        REGISTRY
            .register(Box::new(DATABASE_ERRORS_TOTAL.clone()))
            .expect("database_errors_total can be registered");

        // Throttle & Circuit Breaker metrics
        REGISTRY
            .register(Box::new(THROTTLE_DECISIONS_TOTAL.clone()))
            .expect("throttle_decisions_total can be registered");
        REGISTRY
            .register(Box::new(THROTTLE_RATE_CURRENT.clone()))
            .expect("throttle_rate_current can be registered");
        REGISTRY
            .register(Box::new(CIRCUIT_BREAKER_STATE.clone()))
            .expect("circuit_breaker_state can be registered");
        REGISTRY
            .register(Box::new(CIRCUIT_BREAKER_TRANSITIONS_TOTAL.clone()))
            .expect("circuit_breaker_transitions_total can be registered");
        REGISTRY
            .register(Box::new(CIRCUIT_BREAKER_FAILURES.clone()))
            .expect("circuit_breaker_failures can be registered");
        REGISTRY
            .register(Box::new(CIRCUIT_BREAKER_SUCCESSES.clone()))
            .expect("circuit_breaker_successes can be registered");

        // Lease Manager metrics
        REGISTRY
            .register(Box::new(LEASE_ACQUISITIONS_TOTAL.clone()))
            .expect("lease_acquisitions_total can be registered");
        REGISTRY
            .register(Box::new(LEASE_RENEWALS_TOTAL.clone()))
            .expect("lease_renewals_total can be registered");
        REGISTRY
            .register(Box::new(LEASE_CONFLICTS_TOTAL.clone()))
            .expect("lease_conflicts_total can be registered");
        REGISTRY
            .register(Box::new(LEASE_EPOCH_CURRENT.clone()))
            .expect("lease_epoch_current can be registered");
        REGISTRY
            .register(Box::new(LEASE_EXPIRES_AT.clone()))
            .expect("lease_expires_at can be registered");

        // Leader Change Tracking metrics
        REGISTRY
            .register(Box::new(LEADER_CHANGES_TOTAL.clone()))
            .expect("leader_changes_total can be registered");
        REGISTRY
            .register(Box::new(LEADER_HANDOFF_LATENCY.clone()))
            .expect("leader_handoff_latency can be registered");
        REGISTRY
            .register(Box::new(LEADER_GAP_SECONDS.clone()))
            .expect("leader_gap_seconds can be registered");
        REGISTRY
            .register(Box::new(LEADER_TRANSFERS_PENDING.clone()))
            .expect("leader_transfers_pending can be registered");
        REGISTRY
            .register(Box::new(LEADER_TRANSFERS_TOTAL.clone()))
            .expect("leader_transfers_total can be registered");

        // WAL metrics
        REGISTRY
            .register(Box::new(WAL_APPENDS_TOTAL.clone()))
            .expect("wal_appends_total can be registered");
        REGISTRY
            .register(Box::new(WAL_RECOVERIES_TOTAL.clone()))
            .expect("wal_recoveries_total can be registered");
        REGISTRY
            .register(Box::new(WAL_RECORDS_RECOVERED.clone()))
            .expect("wal_records_recovered can be registered");
        REGISTRY
            .register(Box::new(WAL_RECORDS_SKIPPED.clone()))
            .expect("wal_records_skipped can be registered");
        REGISTRY
            .register(Box::new(WAL_SIZE_BYTES.clone()))
            .expect("wal_size_bytes can be registered");
        REGISTRY
            .register(Box::new(WAL_TRUNCATES_TOTAL.clone()))
            .expect("wal_truncates_total can be registered");

        // Schema Registry metrics
        REGISTRY
            .register(Box::new(SCHEMA_REGISTRATIONS_TOTAL.clone()))
            .expect("schema_registrations_total can be registered");
        REGISTRY
            .register(Box::new(SCHEMA_REGISTRY_ERRORS_TOTAL.clone()))
            .expect("schema_registry_errors_total can be registered");
        REGISTRY
            .register(Box::new(SCHEMA_COMPATIBILITY_CHECKS_TOTAL.clone()))
            .expect("schema_compatibility_checks_total can be registered");
        REGISTRY
            .register(Box::new(SCHEMA_CACHE_ENTRIES.clone()))
            .expect("schema_cache_entries can be registered");
        REGISTRY
            .register(Box::new(SCHEMA_LOOKUPS_TOTAL.clone()))
            .expect("schema_lookups_total can be registered");
        REGISTRY
            .register(Box::new(SCHEMAS_TOTAL.clone()))
            .expect("schemas_total can be registered");
        REGISTRY
            .register(Box::new(SUBJECTS_TOTAL.clone()))
            .expect("subjects_total can be registered");

        // Per-Producer Debugging metrics (Phase 10.7)
        REGISTRY
            .register(Box::new(PRODUCER_DEDUP_TOTAL.clone()))
            .expect("producer_dedup_total can be registered");
        REGISTRY
            .register(Box::new(PRODUCER_SEQUENCE_CURRENT.clone()))
            .expect("producer_sequence_current can be registered");
        REGISTRY
            .register(Box::new(PRODUCER_FENCE_TOTAL.clone()))
            .expect("producer_fence_total can be registered");

        // Partition Imbalance metrics (Phase 10.8)
        REGISTRY
            .register(Box::new(PARTITION_WRITE_RATE.clone()))
            .expect("partition_write_rate can be registered");
        REGISTRY
            .register(Box::new(PARTITION_BYTES_RATE.clone()))
            .expect("partition_bytes_rate can be registered");
        REGISTRY
            .register(Box::new(PARTITION_IMBALANCE_RATIO.clone()))
            .expect("partition_imbalance_ratio can be registered");

        // Backpressure Visibility metrics (Phase 10.9)
        REGISTRY
            .register(Box::new(BACKPRESSURE_THROTTLE_TOTAL.clone()))
            .expect("backpressure_throttle_total can be registered");
        REGISTRY
            .register(Box::new(BACKPRESSURE_CREDITS_AVAILABLE.clone()))
            .expect("backpressure_credits_available can be registered");
        REGISTRY
            .register(Box::new(BACKPRESSURE_WAIT_TIME.clone()))
            .expect("backpressure_wait_time can be registered");

        // E2E Latency Breakdown metrics (Phase 10.10)
        REGISTRY
            .register(Box::new(E2E_PRODUCE_LATENCY.clone()))
            .expect("e2e_produce_latency can be registered");
        REGISTRY
            .register(Box::new(WAL_APPEND_LATENCY.clone()))
            .expect("wal_append_latency can be registered");
        REGISTRY
            .register(Box::new(S3_FLUSH_LATENCY.clone()))
            .expect("s3_flush_latency can be registered");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registration() {
        init();
        // If no panic, registration succeeded
    }

    #[test]
    fn test_producer_metrics() {
        PRODUCER_RECORDS_TOTAL
            .with_label_values(&["test-topic"])
            .inc();
        PRODUCER_BYTES_TOTAL
            .with_label_values(&["test-topic"])
            .inc_by(1024);

        assert_eq!(
            PRODUCER_RECORDS_TOTAL
                .with_label_values(&["test-topic"])
                .get(),
            1
        );
        assert_eq!(
            PRODUCER_BYTES_TOTAL
                .with_label_values(&["test-topic"])
                .get(),
            1024
        );
    }

    #[test]
    fn test_consumer_lag() {
        CONSUMER_LAG
            .with_label_values(&["test-topic", "0", "test-group"])
            .set(1000);

        assert_eq!(
            CONSUMER_LAG
                .with_label_values(&["test-topic", "0", "test-group"])
                .get(),
            1000
        );
    }

    #[test]
    fn test_cache_metrics() {
        CACHE_HITS_TOTAL.inc();
        CACHE_MISSES_TOTAL.inc_by(5);

        assert_eq!(CACHE_HITS_TOTAL.get(), 1);
        assert_eq!(CACHE_MISSES_TOTAL.get(), 5);
    }
}
