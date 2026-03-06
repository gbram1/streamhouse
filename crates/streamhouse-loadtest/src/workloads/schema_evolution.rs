//! Periodic schema evolution workload.

use crate::http_client::HttpClient;
use crate::metrics;
use serde_json::json;
use std::time::Duration;
use tokio::sync::watch;

pub async fn run_schema_evolution(
    client: HttpClient,
    interval_secs: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    // Wait before first evolution to let producers warm up without schema validation
    tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    let mut version = 2;

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => break,
        }

        if *shutdown.borrow() {
            break;
        }

        // Evolve ft-user-events-value (Avro) with a new optional field
        let field_name = format!("field_v{version}");
        let avro_schema = json!({
            "type": "record",
            "name": "UserEvent",
            "namespace": "com.streamhouse.loadtest",
            "fields": [
                {"name": "user_id", "type": "string"},
                {"name": "event_type", "type": "string"},
                {"name": "timestamp", "type": "long"},
                {"name": "seq", "type": "long"},
                {"name": "metadata", "type": {"type": "map", "values": "string"}},
                {"name": field_name, "type": ["null", "string"], "default": null}
            ]
        });

        let body = json!({
            "schema": avro_schema.to_string(),
            "schemaType": "AVRO"
        });

        match client
            .post_json::<serde_json::Value>("/schemas/subjects/ft-user-events-value/versions", &body)
            .await
        {
            Ok(resp) => {
                let id = resp.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
                metrics::SCHEMA_EVOLUTIONS.inc();
                tracing::info!(
                    subject = "ft-user-events-value",
                    version,
                    schema_id = id,
                    "Schema evolved"
                );
            }
            Err(e) => {
                tracing::debug!(version, err = %e, "Schema evolution failed");
            }
        }

        version += 1;
    }
}
