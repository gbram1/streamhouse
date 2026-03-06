//! Periodic SQL query workload.

use crate::http_client::OrgClient;
use crate::metrics;
use std::time::{Duration, Instant};
use tokio::sync::watch;

struct QuerySpec {
    query: &'static str,
    query_type: &'static str,
}

const QUERIES: &[QuerySpec] = &[
    QuerySpec {
        query: r#"SELECT COUNT(*) FROM "ft-analytics""#,
        query_type: "count",
    },
    QuerySpec {
        query: "SHOW TABLES",
        query_type: "introspection",
    },
    QuerySpec {
        query: r#"SELECT * FROM "an-metrics-raw" LIMIT 50"#,
        query_type: "scan",
    },
    QuerySpec {
        query: r#"SELECT COUNT(*) FROM "ec-search""#,
        query_type: "count",
    },
];

pub async fn run_sql_queries(
    clients: Vec<OrgClient>,
    interval_secs: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    let mut query_idx = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        let spec = &QUERIES[query_idx % QUERIES.len()];
        query_idx += 1;

        // Use the first org client (queries are cross-org via the SQL engine)
        let client = &clients[query_idx % clients.len()];

        let body = serde_json::json!({
            "query": spec.query
        });

        let start = Instant::now();
        match client
            .post_json::<serde_json::Value>("/api/v1/sql", &body)
            .await
        {
            Ok(_) => {
                let elapsed = start.elapsed().as_secs_f64();
                metrics::SQL_LATENCY
                    .with_label_values(&[spec.query_type])
                    .observe(elapsed);
                metrics::SQL_QUERIES_TOTAL
                    .with_label_values(&[spec.query_type, "ok"])
                    .inc();
                tracing::debug!(
                    query_type = spec.query_type,
                    elapsed_ms = (elapsed * 1000.0) as u64,
                    "SQL query OK"
                );
            }
            Err(e) => {
                metrics::SQL_QUERIES_TOTAL
                    .with_label_values(&[spec.query_type, "error"])
                    .inc();
                tracing::debug!(query_type = spec.query_type, err = %e, "SQL query failed");
            }
        }
    }
}
