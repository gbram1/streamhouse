use axum::{http::StatusCode, routing::get, Router};
use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabel, EncodeLabelSet, LabelSetEncoder};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::Registry;
use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};

// =============================================================================
// Label types (manual EncodeLabelSet impl for dynamic labels)
// =============================================================================

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

// =============================================================================
// Wrapper types (same API as old prometheus crate)
// =============================================================================

#[derive(Clone)]
pub struct IntCounterVec {
    family: Family<DynLabels, Counter<u64>>,
    keys: Vec<String>,
}

pub struct IntCounterRef<'a> {
    family: &'a Family<DynLabels, Counter<u64>>,
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

#[derive(Clone, Debug)]
struct BucketConstructor {
    buckets: Arc<Vec<f64>>,
}

impl prometheus_client::metrics::family::MetricConstructor<Histogram> for BucketConstructor {
    fn new_metric(&self) -> Histogram {
        Histogram::new(self.buckets.iter().copied())
    }
}

#[derive(Clone)]
pub struct HistogramVec {
    family: Family<DynLabels, Histogram, BucketConstructor>,
    keys: Vec<String>,
}

pub struct HistogramRef<'a> {
    family: &'a Family<DynLabels, Histogram, BucketConstructor>,
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

/// Wrapper for a standalone Histogram (no labels).
#[derive(Clone)]
pub struct SimpleHistogram {
    inner: Histogram,
}

impl SimpleHistogram {
    pub fn observe(&self, v: f64) {
        self.inner.observe(v);
    }
}

/// Wrapper for a standalone IntCounter (no labels).
#[derive(Clone)]
pub struct IntCounter {
    inner: Counter<u64>,
}

impl IntCounter {
    pub fn inc(&self) {
        self.inner.inc();
    }
    pub fn inc_by(&self, v: u64) {
        self.inner.inc_by(v);
    }
}

/// Wrapper for a standalone IntGauge (no labels).
#[derive(Clone)]
pub struct IntGauge {
    inner: Gauge<i64>,
}

impl IntGauge {
    pub fn set(&self, v: i64) {
        self.inner.set(v);
    }
    pub fn inc(&self) {
        self.inner.inc();
    }
    pub fn dec(&self) {
        self.inner.dec();
    }
    pub fn get(&self) -> i64 {
        self.inner.get()
    }
}

// =============================================================================
// Global registry
// =============================================================================

pub static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

// =============================================================================
// Metric statics
// =============================================================================

// ---- Production counters ----------------------------------------------------

pub static PRODUCED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org", "topic", "protocol"]);
    REGISTRY.lock().unwrap().register(
        "loadtest_produced_total",
        "Total records produced",
        m.family.clone(),
    );
    m
});

pub static CONSUMED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org", "topic", "group"]);
    REGISTRY.lock().unwrap().register(
        "loadtest_consumed_total",
        "Total records consumed",
        m.family.clone(),
    );
    m
});

pub static PRODUCE_ERRORS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org", "topic", "protocol"]);
    REGISTRY.lock().unwrap().register(
        "loadtest_produce_errors_total",
        "Total produce errors",
        m.family.clone(),
    );
    m
});

pub static CONSUME_ERRORS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org", "topic"]);
    REGISTRY.lock().unwrap().register(
        "loadtest_consume_errors_total",
        "Total consume errors",
        m.family.clone(),
    );
    m
});

// ---- Latency histograms -----------------------------------------------------

pub static PRODUCE_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["protocol"],
        vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ],
    );
    REGISTRY.lock().unwrap().register(
        "loadtest_produce_latency_seconds",
        "Produce latency",
        m.family.clone(),
    );
    m
});

pub static CONSUME_LATENCY: LazyLock<SimpleHistogram> = LazyLock::new(|| {
    let inner = Histogram::new(
        vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ]
        .into_iter(),
    );
    REGISTRY.lock().unwrap().register(
        "loadtest_consume_latency_seconds",
        "Consume latency",
        inner.clone(),
    );
    SimpleHistogram { inner }
});

pub static SQL_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let m = HistogramVec::new(
        &["query_type"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
    );
    REGISTRY.lock().unwrap().register(
        "loadtest_sql_latency_seconds",
        "SQL query latency",
        m.family.clone(),
    );
    m
});

// ---- Integrity --------------------------------------------------------------

pub static DATA_LOSS_EVENTS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["org", "topic"]);
    REGISTRY.lock().unwrap().register(
        "loadtest_data_loss_events_total",
        "Data loss events detected",
        m.family.clone(),
    );
    m
});

pub static INTEGRITY_CHECKS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["result"]);
    REGISTRY.lock().unwrap().register(
        "loadtest_integrity_checks_total",
        "Integrity check results",
        m.family.clone(),
    );
    m
});

// ---- Schema / SQL / Storage -------------------------------------------------

pub static SCHEMA_EVOLUTIONS: LazyLock<IntCounter> = LazyLock::new(|| {
    let inner = Counter::<u64>::default();
    REGISTRY.lock().unwrap().register(
        "loadtest_schema_evolutions_total",
        "Schema evolutions",
        inner.clone(),
    );
    IntCounter { inner }
});

pub static SQL_QUERIES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let m = IntCounterVec::new(&["query_type", "result"]);
    REGISTRY.lock().unwrap().register(
        "loadtest_sql_queries_total",
        "SQL queries executed",
        m.family.clone(),
    );
    m
});

pub static S3_SEGMENTS_VERIFIED: LazyLock<IntCounter> = LazyLock::new(|| {
    let inner = Counter::<u64>::default();
    REGISTRY.lock().unwrap().register(
        "loadtest_s3_segments_verified_total",
        "S3 segments verified",
        inner.clone(),
    );
    IntCounter { inner }
});

// ---- Gauges -----------------------------------------------------------------

pub static ACTIVE_PRODUCERS: LazyLock<IntGauge> = LazyLock::new(|| {
    let inner = Gauge::<i64>::default();
    REGISTRY.lock().unwrap().register(
        "loadtest_active_producers",
        "Active producer tasks",
        inner.clone(),
    );
    IntGauge { inner }
});

pub static ACTIVE_CONSUMERS: LazyLock<IntGauge> = LazyLock::new(|| {
    let inner = Gauge::<i64>::default();
    REGISTRY.lock().unwrap().register(
        "loadtest_active_consumers",
        "Active consumer tasks",
        inner.clone(),
    );
    IntGauge { inner }
});

pub static RUNNING_SECONDS: LazyLock<IntGauge> = LazyLock::new(|| {
    let inner = Gauge::<i64>::default();
    REGISTRY.lock().unwrap().register(
        "loadtest_running_seconds",
        "Load test uptime",
        inner.clone(),
    );
    IntGauge { inner }
});

// ---- Initialize all metrics (touch the lazy statics) ------------------------

pub fn init() {
    let _ = &*PRODUCED_TOTAL;
    let _ = &*CONSUMED_TOTAL;
    let _ = &*PRODUCE_ERRORS;
    let _ = &*CONSUME_ERRORS;
    let _ = &*PRODUCE_LATENCY;
    let _ = &*CONSUME_LATENCY;
    let _ = &*SQL_LATENCY;
    let _ = &*DATA_LOSS_EVENTS;
    let _ = &*INTEGRITY_CHECKS;
    let _ = &*SCHEMA_EVOLUTIONS;
    let _ = &*SQL_QUERIES_TOTAL;
    let _ = &*S3_SEGMENTS_VERIFIED;
    let _ = &*ACTIVE_PRODUCERS;
    let _ = &*ACTIVE_CONSUMERS;
    let _ = &*RUNNING_SECONDS;
}

// ---- HTTP server ------------------------------------------------------------

async fn metrics_handler() -> Result<String, StatusCode> {
    let registry = REGISTRY.lock().unwrap();
    let mut buffer = String::new();
    encode(&mut buffer, &registry).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(buffer)
}

pub async fn start_metrics_server(port: u16) -> anyhow::Result<()> {
    let app = Router::new().route("/metrics", get(metrics_handler));
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Metrics server on http://{addr}/metrics");
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    Ok(())
}
