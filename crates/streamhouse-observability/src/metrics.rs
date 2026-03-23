use prometheus_client::encoding::{EncodeLabel, EncodeLabelSet, LabelSetEncoder};
use prometheus_client::metrics::counter::Counter as PCounter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge as PGauge;
use prometheus_client::metrics::histogram::Histogram as PHist;
use prometheus_client::registry::Registry;
use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};

// =============================================================================
// Wrapper types that preserve the old `prometheus` crate API
// =============================================================================

/// Label set backed by a vector of key-value pairs.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct DynLabels(Vec<(String, String)>);

impl EncodeLabelSet for DynLabels {
    fn encode(&self, mut encoder: LabelSetEncoder) -> Result<(), fmt::Error> {
        for (key, val) in &self.0 {
            let pair = (key.as_str(), val.as_str());
            EncodeLabel::encode(&pair, encoder.encode_label())?;
        }
        Ok(())
    }
}

fn make_labels(keys: &[&str], values: &[&str]) -> DynLabels {
    DynLabels(
        keys.iter()
            .zip(values.iter())
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

// ---- IntCounterVec ----------------------------------------------------------

/// Drop-in replacement for `prometheus::IntCounterVec`.
#[derive(Clone)]
pub struct IntCounterVec {
    family: Family<DynLabels, PCounter<u64>>,
    keys: Vec<String>,
}

/// Handle returned by `IntCounterVec::with_label_values`.
pub struct IntCounterRef<'a> {
    family: &'a Family<DynLabels, PCounter<u64>>,
    labels: DynLabels,
}

impl IntCounterVec {
    fn new(keys: &[&str]) -> Self {
        Self {
            family: Family::default(),
            keys: keys.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_label_values<'a>(&'a self, values: &[&str]) -> IntCounterRef<'a> {
        let labels = make_labels(
            &self.keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            values,
        );
        IntCounterRef {
            family: &self.family,
            labels,
        }
    }
}

impl IntCounterRef<'_> {
    pub fn inc(&self) {
        self.family.get_or_create(&self.labels).inc();
    }
    pub fn inc_by(&self, v: u64) {
        self.family.get_or_create(&self.labels).inc_by(v);
    }
    pub fn get(&self) -> u64 {
        self.family.get_or_create(&self.labels).get()
    }
}

// ---- IntGaugeVec ------------------------------------------------------------

/// Drop-in replacement for `prometheus::IntGaugeVec`.
#[derive(Clone)]
pub struct IntGaugeVec {
    family: Family<DynLabels, PGauge<i64>>,
    keys: Vec<String>,
}

pub struct IntGaugeRef<'a> {
    family: &'a Family<DynLabels, PGauge<i64>>,
    labels: DynLabels,
}

impl IntGaugeVec {
    fn new(keys: &[&str]) -> Self {
        Self {
            family: Family::default(),
            keys: keys.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_label_values<'a>(&'a self, values: &[&str]) -> IntGaugeRef<'a> {
        let labels = make_labels(
            &self.keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            values,
        );
        IntGaugeRef {
            family: &self.family,
            labels,
        }
    }
}

impl IntGaugeRef<'_> {
    pub fn set(&self, v: i64) {
        self.family.get_or_create(&self.labels).set(v);
    }
    pub fn inc(&self) {
        self.family.get_or_create(&self.labels).inc();
    }
    pub fn dec(&self) {
        self.family.get_or_create(&self.labels).dec();
    }
    pub fn get(&self) -> i64 {
        self.family.get_or_create(&self.labels).get()
    }
}

// ---- HistogramVec -----------------------------------------------------------

/// Drop-in replacement for `prometheus::HistogramVec`.
///
/// Uses `Family<DynLabels, PHist>` with `MetricConstructor` to provide custom
/// buckets to each new histogram created on first access.
#[derive(Clone)]
pub struct HistogramVec {
    family: Family<DynLabels, PHist, BucketConstructor>,
    keys: Vec<String>,
}

/// Constructor that stores the bucket boundaries so `Family` can create
/// histograms with the right buckets.
#[derive(Clone, Debug)]
struct BucketConstructor {
    buckets: Arc<Vec<f64>>,
}

impl prometheus_client::metrics::family::MetricConstructor<PHist> for BucketConstructor {
    fn new_metric(&self) -> PHist {
        PHist::new(self.buckets.iter().copied())
    }
}

pub struct HistogramRef<'a> {
    family: &'a Family<DynLabels, PHist, BucketConstructor>,
    labels: DynLabels,
}

impl HistogramVec {
    fn new(keys: &[&str], buckets: Vec<f64>) -> Self {
        Self {
            family: Family::new_with_constructor(BucketConstructor {
                buckets: Arc::new(buckets),
            }),
            keys: keys.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_label_values<'a>(&'a self, values: &[&str]) -> HistogramRef<'a> {
        let labels = make_labels(
            &self.keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            values,
        );
        HistogramRef {
            family: &self.family,
            labels,
        }
    }
}

impl HistogramRef<'_> {
    pub fn observe(&self, v: f64) {
        self.family.get_or_create(&self.labels).observe(v);
    }
}

// ---- IntGauge (no labels) ---------------------------------------------------

/// Drop-in replacement for `prometheus::IntGauge`.
#[derive(Clone)]
pub struct IntGauge {
    gauge: PGauge<i64>,
}

impl IntGauge {
    fn new() -> Self {
        Self {
            gauge: PGauge::default(),
        }
    }
    pub fn set(&self, v: i64) {
        self.gauge.set(v);
    }
    pub fn inc(&self) {
        self.gauge.inc();
    }
    pub fn dec(&self) {
        self.gauge.dec();
    }
    pub fn get(&self) -> i64 {
        self.gauge.get()
    }
}

// =============================================================================
// Global registry
// =============================================================================

pub static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

// =============================================================================
// Metric statics — same names / public visibility as before
// =============================================================================

// ---- Producer Metrics -------------------------------------------------------

pub static PRODUCER_RECORDS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_records",
        "Total records produced",
        m.family.clone(),
    );
    m
});

pub static PRODUCER_BYTES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_bytes",
        "Total bytes produced",
        m.family.clone(),
    );
    m
});

pub static PRODUCER_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "topic"],
        vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_latency_seconds",
        "Producer latency in seconds",
        m.family.clone(),
    );
    m
});

pub static PRODUCER_BATCH_SIZE: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "topic"],
        vec![1.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_batch_size",
        "Producer batch size in records",
        m.family.clone(),
    );
    m
});

pub static PRODUCER_ERRORS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "error_type"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_errors",
        "Total producer errors",
        m.family.clone(),
    );
    m
});

// ---- Consumer Metrics -------------------------------------------------------

pub static CONSUMER_RECORDS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "consumer_group"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_consumer_records",
        "Total records consumed",
        m.family.clone(),
    );
    m
});

pub static CONSUMER_LAG: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic", "partition", "consumer_group"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_consumer_lag",
        "Consumer lag in number of messages",
        m.family.clone(),
    );
    m
});

pub static CONSUMER_REBALANCES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "consumer_group"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_consumer_rebalances",
        "Total consumer rebalances",
        m.family.clone(),
    );
    m
});

pub static CONSUMER_ERRORS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "consumer_group", "error_type"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_consumer_errors",
        "Total consumer errors",
        m.family.clone(),
    );
    m
});

// ---- Storage Metrics --------------------------------------------------------

pub static SEGMENT_WRITES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_segment_writes",
        "Total segments written",
        m.family.clone(),
    );
    m
});

pub static SEGMENT_FLUSHES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_segment_flushes",
        "Total segment flushes to S3",
        m.family.clone(),
    );
    m
});

pub static SEGMENT_BUFFER_RECORDS: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_segment_buffer_records",
        "Records currently in segment buffer per partition",
        m.family.clone(),
    );
    m
});

pub static S3_REQUESTS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "operation"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_s3_requests",
        "Total S3 requests",
        m.family.clone(),
    );
    m
});

pub static S3_ERRORS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "operation", "error_type"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_s3_errors",
        "Total S3 errors",
        m.family.clone(),
    );
    m
});

pub static S3_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "operation"],
        vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_s3_latency_seconds",
        "S3 request latency in seconds",
        m.family.clone(),
    );
    m
});

pub static CACHE_HITS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_cache_hits",
        "Total cache hits",
        m.family.clone(),
    );
    m
});

pub static CACHE_MISSES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_cache_misses",
        "Total cache misses",
        m.family.clone(),
    );
    m
});

pub static CACHE_SIZE_BYTES: LazyLock<IntGauge> = LazyLock::new(|| {
    let m = IntGauge::new();
    REGISTRY.lock().unwrap().register(
        "streamhouse_cache_size_bytes",
        "Current cache size in bytes",
        m.gauge.clone(),
    );
    m
});

// ---- System Metrics ---------------------------------------------------------

pub static CONNECTIONS_ACTIVE: LazyLock<IntGauge> = LazyLock::new(|| {
    let m = IntGauge::new();
    REGISTRY.lock().unwrap().register(
        "streamhouse_connections_active",
        "Number of active connections",
        m.gauge.clone(),
    );
    m
});

pub static PARTITIONS_TOTAL: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_partitions",
        "Total partitions",
        m.family.clone(),
    );
    m
});

pub static TOPICS_TOTAL: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id"]);
    REGISTRY
        .lock()
        .unwrap()
        .register("streamhouse_topics", "Total topics", m.family.clone());
    m
});

pub static AGENTS_ACTIVE: LazyLock<IntGauge> = LazyLock::new(|| {
    let m = IntGauge::new();
    REGISTRY.lock().unwrap().register(
        "streamhouse_agents_active",
        "Number of active agents",
        m.gauge.clone(),
    );
    m
});

pub static UPTIME_SECONDS: LazyLock<IntGauge> = LazyLock::new(|| {
    let m = IntGauge::new();
    REGISTRY.lock().unwrap().register(
        "streamhouse_uptime_seconds",
        "Server uptime in seconds",
        m.gauge.clone(),
    );
    m
});

pub static DATABASE_ERRORS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "operation", "error_type"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_database_errors",
        "Total database errors",
        m.family.clone(),
    );
    m
});

// ---- Throttle & Circuit Breaker Metrics -------------------------------------

pub static THROTTLE_DECISIONS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "operation", "decision"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_throttle_decisions",
        "Total throttle decisions",
        m.family.clone(),
    );
    m
});

pub static THROTTLE_RATE_CURRENT: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "operation"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_throttle_rate_current",
        "Current rate limit in operations per second",
        m.family.clone(),
    );
    m
});

pub static CIRCUIT_BREAKER_STATE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "operation"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_circuit_breaker_state",
        "Circuit breaker state (0=Closed, 1=Open, 2=HalfOpen)",
        m.family.clone(),
    );
    m
});

pub static CIRCUIT_BREAKER_TRANSITIONS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "operation", "from_state", "to_state"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_circuit_breaker_transitions",
        "Total circuit breaker state transitions",
        m.family.clone(),
    );
    m
});

pub static CIRCUIT_BREAKER_FAILURES: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "operation"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_circuit_breaker_failures",
        "Current consecutive failure count",
        m.family.clone(),
    );
    m
});

pub static CIRCUIT_BREAKER_SUCCESSES: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "operation"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_circuit_breaker_successes",
        "Current consecutive success count in half-open",
        m.family.clone(),
    );
    m
});

// ---- Lease Manager Metrics --------------------------------------------------

pub static LEASE_ACQUISITIONS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_lease_acquisitions",
        "Total partition lease acquisitions",
        m.family.clone(),
    );
    m
});

pub static LEASE_RENEWALS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_lease_renewals",
        "Total partition lease renewals",
        m.family.clone(),
    );
    m
});

pub static LEASE_CONFLICTS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_lease_conflicts",
        "Total lease conflicts",
        m.family.clone(),
    );
    m
});

pub static LEASE_EPOCH_CURRENT: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic", "partition", "agent_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_lease_epoch_current",
        "Current lease epoch for partition",
        m.family.clone(),
    );
    m
});

pub static LEASE_EXPIRES_AT: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic", "partition", "agent_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_lease_expires_at",
        "Lease expiration timestamp",
        m.family.clone(),
    );
    m
});

// ---- Leader Change Tracking Metrics -----------------------------------------

pub static LEADER_CHANGES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition", "reason"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_leader_changes",
        "Total leadership changes by reason",
        m.family.clone(),
    );
    m
});

pub static LEADER_HANDOFF_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "topic", "partition"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_leader_handoff_latency_seconds",
        "Leader handoff latency in seconds",
        m.family.clone(),
    );
    m
});

pub static LEADER_GAP_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "topic", "partition"],
        vec![0.0, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_leader_gap_seconds",
        "Time partition was without a leader",
        m.family.clone(),
    );
    m
});

pub static LEADER_TRANSFERS_PENDING: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_leader_transfers_pending",
        "Number of pending leader transfers",
        m.family.clone(),
    );
    m
});

pub static LEADER_TRANSFERS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_leader_transfers",
        "Total leader transfer operations",
        m.family.clone(),
    );
    m
});

// ---- WAL Metrics ------------------------------------------------------------

pub static WAL_APPENDS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_wal_appends",
        "Total WAL append operations",
        m.family.clone(),
    );
    m
});

pub static WAL_RECOVERIES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_wal_recoveries",
        "Total WAL recovery operations",
        m.family.clone(),
    );
    m
});

pub static WAL_RECORDS_RECOVERED: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_wal_records_recovered",
        "Number of records recovered from WAL",
        m.family.clone(),
    );
    m
});

pub static WAL_RECORDS_SKIPPED: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_wal_records_skipped",
        "Number of corrupted WAL records skipped",
        m.family.clone(),
    );
    m
});

pub static WAL_SIZE_BYTES: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_wal_size_bytes",
        "WAL file size in bytes",
        m.family.clone(),
    );
    m
});

pub static WAL_TRUNCATES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "topic", "partition", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_wal_truncates",
        "Total WAL truncate operations",
        m.family.clone(),
    );
    m
});

// ---- Schema Registry Metrics ------------------------------------------------

pub static SCHEMA_REGISTRATIONS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "subject", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_schema_registrations",
        "Total schema registrations",
        m.family.clone(),
    );
    m
});

pub static SCHEMA_REGISTRY_ERRORS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "type"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_schema_registry_errors",
        "Total schema registry errors",
        m.family.clone(),
    );
    m
});

pub static SCHEMA_COMPATIBILITY_CHECKS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "subject", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_schema_compatibility_checks",
        "Total schema compatibility checks",
        m.family.clone(),
    );
    m
});

pub static SCHEMA_CACHE_ENTRIES: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_schema_cache_entries",
        "Number of entries in schema cache",
        m.family.clone(),
    );
    m
});

pub static SCHEMA_LOOKUPS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_schema_lookups",
        "Total schema lookups by ID",
        m.family.clone(),
    );
    m
});

pub static SCHEMAS_TOTAL: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_schemas",
        "Total number of registered schemas",
        m.family.clone(),
    );
    m
});

pub static SUBJECTS_TOTAL: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_subjects",
        "Total number of schema subjects",
        m.family.clone(),
    );
    m
});

// ---- Per-Producer Debugging Metrics -----------------------------------------

pub static PRODUCER_DEDUP_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "producer_id", "result"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_dedup",
        "Total producer dedup checks by result",
        m.family.clone(),
    );
    m
});

pub static PRODUCER_SEQUENCE_CURRENT: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "producer_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_sequence_current",
        "Current producer sequence number",
        m.family.clone(),
    );
    m
});

pub static PRODUCER_FENCE_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "producer_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_producer_fence",
        "Total producer fence events",
        m.family.clone(),
    );
    m
});

// ---- Partition Imbalance Metrics --------------------------------------------

pub static PARTITION_WRITE_RATE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_partition_write_rate",
        "Partition write rate in records per second",
        m.family.clone(),
    );
    m
});

pub static PARTITION_BYTES_RATE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic", "partition"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_partition_bytes_rate",
        "Partition write rate in bytes per second",
        m.family.clone(),
    );
    m
});

pub static PARTITION_IMBALANCE_RATIO: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "topic"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_partition_imbalance_ratio",
        "Partition write rate imbalance ratio (stdev/mean * 1000)",
        m.family.clone(),
    );
    m
});

// ---- Backpressure Visibility Metrics ----------------------------------------

pub static BACKPRESSURE_THROTTLE_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "producer_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_backpressure_throttle",
        "Total backpressure throttle events",
        m.family.clone(),
    );
    m
});

pub static BACKPRESSURE_CREDITS_AVAILABLE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let m = IntGaugeVec::new(&["org_id", "producer_id"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_backpressure_credits_available",
        "Available backpressure credits per producer",
        m.family.clone(),
    );
    m
});

pub static BACKPRESSURE_WAIT_TIME: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "producer_id"],
        vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_backpressure_wait_time_seconds",
        "Backpressure wait time in seconds",
        m.family.clone(),
    );
    m
});

// ---- E2E Latency Breakdown Metrics ------------------------------------------

pub static E2E_PRODUCE_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "topic"],
        vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_e2e_produce_latency_seconds",
        "End-to-end produce latency in seconds",
        m.family.clone(),
    );
    m
});

pub static WAL_APPEND_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "topic", "partition"],
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_wal_append_latency_seconds",
        "WAL append latency in seconds",
        m.family.clone(),
    );
    m
});

pub static S3_FLUSH_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "topic", "partition"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_s3_flush_latency_seconds",
        "S3 flush latency in seconds",
        m.family.clone(),
    );
    m
});

// ---- Rate Limiting Metrics --------------------------------------------------

pub static RATE_LIMIT_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org_id", "decision", "protocol"]);
    REGISTRY.lock().unwrap().register(
        "streamhouse_rate_limit",
        "Total rate limit decisions",
        m.family.clone(),
    );
    m
});

pub static THROTTLE_TIME_MS: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["org_id", "protocol"],
        vec![0.0, 10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0],
    );
    REGISTRY.lock().unwrap().register(
        "streamhouse_throttle_time_ms",
        "Kafka throttle time in milliseconds",
        m.family.clone(),
    );
    m
});

// =============================================================================
// Initialization
// =============================================================================

/// Initialize metrics registry.
/// Can be called multiple times safely (idempotent via LazyLock).
pub fn init() {
    // Touch all lazy statics to ensure they are registered.
    let _ = &*PRODUCER_RECORDS_TOTAL;
    let _ = &*PRODUCER_BYTES_TOTAL;
    let _ = &*PRODUCER_LATENCY;
    let _ = &*PRODUCER_BATCH_SIZE;
    let _ = &*PRODUCER_ERRORS_TOTAL;
    let _ = &*CONSUMER_RECORDS_TOTAL;
    let _ = &*CONSUMER_LAG;
    let _ = &*CONSUMER_REBALANCES_TOTAL;
    let _ = &*CONSUMER_ERRORS_TOTAL;
    let _ = &*SEGMENT_WRITES_TOTAL;
    let _ = &*SEGMENT_FLUSHES_TOTAL;
    let _ = &*SEGMENT_BUFFER_RECORDS;
    let _ = &*S3_REQUESTS_TOTAL;
    let _ = &*S3_ERRORS_TOTAL;
    let _ = &*S3_LATENCY;
    let _ = &*CACHE_HITS_TOTAL;
    let _ = &*CACHE_MISSES_TOTAL;
    let _ = &*CACHE_SIZE_BYTES;
    let _ = &*CONNECTIONS_ACTIVE;
    let _ = &*PARTITIONS_TOTAL;
    let _ = &*TOPICS_TOTAL;
    let _ = &*AGENTS_ACTIVE;
    let _ = &*UPTIME_SECONDS;
    let _ = &*DATABASE_ERRORS_TOTAL;
    let _ = &*THROTTLE_DECISIONS_TOTAL;
    let _ = &*THROTTLE_RATE_CURRENT;
    let _ = &*CIRCUIT_BREAKER_STATE;
    let _ = &*CIRCUIT_BREAKER_TRANSITIONS_TOTAL;
    let _ = &*CIRCUIT_BREAKER_FAILURES;
    let _ = &*CIRCUIT_BREAKER_SUCCESSES;
    let _ = &*LEASE_ACQUISITIONS_TOTAL;
    let _ = &*LEASE_RENEWALS_TOTAL;
    let _ = &*LEASE_CONFLICTS_TOTAL;
    let _ = &*LEASE_EPOCH_CURRENT;
    let _ = &*LEASE_EXPIRES_AT;
    let _ = &*LEADER_CHANGES_TOTAL;
    let _ = &*LEADER_HANDOFF_LATENCY;
    let _ = &*LEADER_GAP_SECONDS;
    let _ = &*LEADER_TRANSFERS_PENDING;
    let _ = &*LEADER_TRANSFERS_TOTAL;
    let _ = &*WAL_APPENDS_TOTAL;
    let _ = &*WAL_RECOVERIES_TOTAL;
    let _ = &*WAL_RECORDS_RECOVERED;
    let _ = &*WAL_RECORDS_SKIPPED;
    let _ = &*WAL_SIZE_BYTES;
    let _ = &*WAL_TRUNCATES_TOTAL;
    let _ = &*SCHEMA_REGISTRATIONS_TOTAL;
    let _ = &*SCHEMA_REGISTRY_ERRORS_TOTAL;
    let _ = &*SCHEMA_COMPATIBILITY_CHECKS_TOTAL;
    let _ = &*SCHEMA_CACHE_ENTRIES;
    let _ = &*SCHEMA_LOOKUPS_TOTAL;
    let _ = &*SCHEMAS_TOTAL;
    let _ = &*SUBJECTS_TOTAL;
    let _ = &*PRODUCER_DEDUP_TOTAL;
    let _ = &*PRODUCER_SEQUENCE_CURRENT;
    let _ = &*PRODUCER_FENCE_TOTAL;
    let _ = &*PARTITION_WRITE_RATE;
    let _ = &*PARTITION_BYTES_RATE;
    let _ = &*PARTITION_IMBALANCE_RATIO;
    let _ = &*BACKPRESSURE_THROTTLE_TOTAL;
    let _ = &*BACKPRESSURE_CREDITS_AVAILABLE;
    let _ = &*BACKPRESSURE_WAIT_TIME;
    let _ = &*E2E_PRODUCE_LATENCY;
    let _ = &*WAL_APPEND_LATENCY;
    let _ = &*S3_FLUSH_LATENCY;
    let _ = &*RATE_LIMIT_TOTAL;
    let _ = &*THROTTLE_TIME_MS;
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
            .with_label_values(&["test-org", "test-topic"])
            .inc();
        PRODUCER_BYTES_TOTAL
            .with_label_values(&["test-org", "test-topic"])
            .inc_by(1024);

        assert_eq!(
            PRODUCER_RECORDS_TOTAL
                .with_label_values(&["test-org", "test-topic"])
                .get(),
            1
        );
        assert_eq!(
            PRODUCER_BYTES_TOTAL
                .with_label_values(&["test-org", "test-topic"])
                .get(),
            1024
        );
    }

    #[test]
    fn test_consumer_lag() {
        CONSUMER_LAG
            .with_label_values(&["test-org", "test-topic", "0", "test-group"])
            .set(1000);

        assert_eq!(
            CONSUMER_LAG
                .with_label_values(&["test-org", "test-topic", "0", "test-group"])
                .get(),
            1000
        );
    }

    #[test]
    fn test_cache_metrics() {
        CACHE_HITS_TOTAL.with_label_values(&["test-org"]).inc();
        CACHE_MISSES_TOTAL
            .with_label_values(&["test-org"])
            .inc_by(5);

        assert_eq!(CACHE_HITS_TOTAL.with_label_values(&["test-org"]).get(), 1);
        assert_eq!(CACHE_MISSES_TOTAL.with_label_values(&["test-org"]).get(), 5);
    }
}
