use axum::{http::StatusCode, routing::get, Router};
use prometheus::{
    Encoder, Histogram, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts,
    Registry, TextEncoder,
};
use std::sync::LazyLock;

pub static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

// ─── Production counters ────────────────────────────────────────────────────

pub static PRODUCED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let opts = Opts::new("loadtest_produced_total", "Total records produced");
    let counter = IntCounterVec::new(opts, &["org", "topic", "protocol"]).unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

pub static CONSUMED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let opts = Opts::new("loadtest_consumed_total", "Total records consumed");
    let counter = IntCounterVec::new(opts, &["org", "topic", "group"]).unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

pub static PRODUCE_ERRORS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let opts = Opts::new("loadtest_produce_errors_total", "Total produce errors");
    let counter = IntCounterVec::new(opts, &["org", "topic", "protocol"]).unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

pub static CONSUME_ERRORS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let opts = Opts::new("loadtest_consume_errors_total", "Total consume errors");
    let counter = IntCounterVec::new(opts, &["org", "topic"]).unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

// ─── Latency histograms ─────────────────────────────────────────────────────

pub static PRODUCE_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let opts = HistogramOpts::new("loadtest_produce_latency_seconds", "Produce latency")
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]);
    let hist = HistogramVec::new(opts, &["protocol"]).unwrap();
    REGISTRY.register(Box::new(hist.clone())).unwrap();
    hist
});

pub static CONSUME_LATENCY: LazyLock<Histogram> = LazyLock::new(|| {
    let opts = HistogramOpts::new("loadtest_consume_latency_seconds", "Consume latency")
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]);
    let hist = Histogram::with_opts(opts).unwrap();
    REGISTRY.register(Box::new(hist.clone())).unwrap();
    hist
});

pub static SQL_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    let opts = HistogramOpts::new("loadtest_sql_latency_seconds", "SQL query latency")
        .buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]);
    let hist = HistogramVec::new(opts, &["query_type"]).unwrap();
    REGISTRY.register(Box::new(hist.clone())).unwrap();
    hist
});

// ─── Integrity ──────────────────────────────────────────────────────────────

pub static DATA_LOSS_EVENTS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let opts = Opts::new("loadtest_data_loss_events_total", "Data loss events detected");
    let counter = IntCounterVec::new(opts, &["org", "topic"]).unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

pub static INTEGRITY_CHECKS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let opts = Opts::new("loadtest_integrity_checks_total", "Integrity check results");
    let counter = IntCounterVec::new(opts, &["result"]).unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

// ─── Schema / SQL / Storage ─────────────────────────────────────────────────

pub static SCHEMA_EVOLUTIONS: LazyLock<IntCounter> = LazyLock::new(|| {
    let counter = IntCounter::new("loadtest_schema_evolutions_total", "Schema evolutions").unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

pub static SQL_QUERIES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let opts = Opts::new("loadtest_sql_queries_total", "SQL queries executed");
    let counter = IntCounterVec::new(opts, &["query_type", "result"]).unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

pub static S3_SEGMENTS_VERIFIED: LazyLock<IntCounter> = LazyLock::new(|| {
    let counter =
        IntCounter::new("loadtest_s3_segments_verified_total", "S3 segments verified").unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

// ─── Gauges ─────────────────────────────────────────────────────────────────

pub static ACTIVE_PRODUCERS: LazyLock<IntGauge> = LazyLock::new(|| {
    let gauge = IntGauge::new("loadtest_active_producers", "Active producer tasks").unwrap();
    REGISTRY.register(Box::new(gauge.clone())).unwrap();
    gauge
});

pub static ACTIVE_CONSUMERS: LazyLock<IntGauge> = LazyLock::new(|| {
    let gauge = IntGauge::new("loadtest_active_consumers", "Active consumer tasks").unwrap();
    REGISTRY.register(Box::new(gauge.clone())).unwrap();
    gauge
});

pub static RUNNING_SECONDS: LazyLock<IntGauge> = LazyLock::new(|| {
    let gauge = IntGauge::new("loadtest_running_seconds", "Load test uptime").unwrap();
    REGISTRY.register(Box::new(gauge.clone())).unwrap();
    gauge
});

// ─── Initialize all metrics (touch the lazy statics) ────────────────────────

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

// ─── HTTP server ────────────────────────────────────────────────────────────

async fn metrics_handler() -> Result<String, StatusCode> {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    encoder
        .encode(&metric_families, &mut buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    String::from_utf8(buffer).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
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
