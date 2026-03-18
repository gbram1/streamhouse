//! Pipeline log tailing command
//!
//! `stm logs <pipeline>` polls pipeline state and shows record progress.

use anyhow::{Context, Result};
use chrono::Local;
use serde::Deserialize;

use crate::rest_client::RestClient;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineResponse {
    name: String,
    state: String,
    #[serde(default)]
    records_processed: i64,
    #[serde(default)]
    error_message: Option<String>,
}

pub async fn handle_logs(
    pipeline_name: &str,
    api_url: &str,
    api_key: Option<&str>,
    org_id: Option<&str>,
) -> Result<()> {
    let client = RestClient::with_org(api_url, api_key.map(String::from), org_id.map(String::from));

    let path = format!("/api/v1/pipelines/{}", pipeline_name);

    // Initial fetch
    let initial: PipelineResponse = client
        .get(&path)
        .await
        .context(format!("Pipeline '{}' not found", pipeline_name))?;

    println!(
        "Tailing logs for pipeline '{}' (Ctrl+C to stop)",
        pipeline_name
    );
    println!();

    let mut last_records = initial.records_processed;
    let mut last_state = initial.state.clone();

    let now = Local::now().format("%H:%M:%S");
    println!(
        "[{}] {}: {}, {} records",
        now, initial.name, initial.state, initial.records_processed
    );

    if let Some(ref err) = initial.error_message {
        println!("  error: {}", err);
    }

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!();
                println!("Stopped.");
                break;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                match client.get::<PipelineResponse>(&path).await {
                    Ok(resp) => {
                        let delta = resp.records_processed - last_records;
                        let now = Local::now().format("%H:%M:%S");

                        if delta > 0 || resp.state != last_state {
                            let delta_str = if delta > 0 {
                                format!(", +{} records (total: {})", delta, resp.records_processed)
                            } else {
                                format!(", {} records", resp.records_processed)
                            };

                            println!("[{}] {}: {}{}", now, resp.name, resp.state, delta_str);

                            if let Some(ref err) = resp.error_message {
                                if resp.state != last_state {
                                    println!("  error: {}", err);
                                }
                            }

                            last_records = resp.records_processed;
                            last_state = resp.state;
                        }
                    }
                    Err(e) => {
                        let now = Local::now().format("%H:%M:%S");
                        eprintln!("[{}] Error polling pipeline: {}", now, e);
                    }
                }
            }
        }
    }

    Ok(())
}
