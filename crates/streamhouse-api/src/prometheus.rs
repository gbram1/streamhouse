//! Prometheus client for querying metrics
//!
//! Queries the Prometheus server to fetch real metrics data for the Web UI.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Prometheus query client
#[derive(Clone)]
pub struct PrometheusClient {
    client: reqwest::Client,
    base_url: String,
}

/// Prometheus query response
#[derive(Debug, Deserialize)]
pub struct PrometheusResponse {
    pub status: String,
    pub data: PrometheusData,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "resultType", content = "result")]
pub enum PrometheusData {
    #[serde(rename = "vector")]
    Vector(Vec<VectorResult>),
    #[serde(rename = "matrix")]
    Matrix(Vec<MatrixResult>),
    #[serde(rename = "scalar")]
    Scalar((f64, String)),
}

#[derive(Debug, Deserialize)]
pub struct VectorResult {
    pub metric: serde_json::Value,
    pub value: (f64, String),
}

#[derive(Debug, Deserialize)]
pub struct MatrixResult {
    pub metric: serde_json::Value,
    pub values: Vec<(f64, String)>,
}

/// Time series data point for UI
#[derive(Debug, Clone, Serialize)]
pub struct TimeSeriesPoint {
    pub timestamp: i64,
    pub value: f64,
}

impl PrometheusClient {
    /// Create a new Prometheus client
    pub fn new(prometheus_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: prometheus_url.trim_end_matches('/').to_string(),
        }
    }

    /// Execute an instant query
    pub async fn query(&self, promql: &str) -> Result<PrometheusResponse, PrometheusError> {
        let url = format!("{}/api/v1/query", self.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("query", promql)])
            .send()
            .await
            .map_err(|e| PrometheusError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(PrometheusError::RequestFailed(format!(
                "Prometheus returned status {}",
                response.status()
            )));
        }

        response
            .json()
            .await
            .map_err(|e| PrometheusError::ParseFailed(e.to_string()))
    }

    /// Execute a range query
    pub async fn query_range(
        &self,
        promql: &str,
        start: i64,
        end: i64,
        step: &str,
    ) -> Result<PrometheusResponse, PrometheusError> {
        let url = format!("{}/api/v1/query_range", self.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[
                ("query", promql),
                ("start", &start.to_string()),
                ("end", &end.to_string()),
                ("step", step),
            ])
            .send()
            .await
            .map_err(|e| PrometheusError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(PrometheusError::RequestFailed(format!(
                "Prometheus returned status {}",
                response.status()
            )));
        }

        response
            .json()
            .await
            .map_err(|e| PrometheusError::ParseFailed(e.to_string()))
    }

    /// Get throughput metrics (messages per second)
    pub async fn get_throughput(&self, time_range: &str) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
        let (start, end, step) = self.parse_time_range(time_range);

        // Query rate of producer records
        let query = "sum(rate(streamhouse_producer_records_total[1m]))";

        let response = self.query_range(query, start, end, &step).await?;

        self.extract_time_series(&response)
    }

    /// Get bytes throughput
    pub async fn get_bytes_throughput(&self, time_range: &str) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
        let (start, end, step) = self.parse_time_range(time_range);

        let query = "sum(rate(streamhouse_producer_bytes_total[1m]))";

        let response = self.query_range(query, start, end, &step).await?;

        self.extract_time_series(&response)
    }

    /// Get latency percentiles
    pub async fn get_latency_percentiles(&self, time_range: &str) -> Result<LatencyPercentiles, PrometheusError> {
        let (start, end, step) = self.parse_time_range(time_range);

        // Query p50, p95, p99 latencies
        let p50_query = "histogram_quantile(0.50, sum(rate(streamhouse_producer_latency_seconds_bucket[1m])) by (le))";
        let p95_query = "histogram_quantile(0.95, sum(rate(streamhouse_producer_latency_seconds_bucket[1m])) by (le))";
        let p99_query = "histogram_quantile(0.99, sum(rate(streamhouse_producer_latency_seconds_bucket[1m])) by (le))";

        let p50 = self.query_range(p50_query, start, end, &step).await?;
        let p95 = self.query_range(p95_query, start, end, &step).await?;
        let p99 = self.query_range(p99_query, start, end, &step).await?;

        Ok(LatencyPercentiles {
            p50: self.extract_time_series(&p50).unwrap_or_default(),
            p95: self.extract_time_series(&p95).unwrap_or_default(),
            p99: self.extract_time_series(&p99).unwrap_or_default(),
        })
    }

    /// Get error rate
    pub async fn get_error_rate(&self, time_range: &str) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
        let (start, end, step) = self.parse_time_range(time_range);

        // Query error rate as percentage
        let query = r#"
            sum(rate(streamhouse_producer_errors_total[1m]))
            /
            (sum(rate(streamhouse_producer_records_total[1m])) + 0.0001)
            * 100
        "#;

        let response = self.query_range(query.trim(), start, end, &step).await?;

        self.extract_time_series(&response)
    }

    /// Get error count
    pub async fn get_error_count(&self, time_range: &str) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
        let (start, end, step) = self.parse_time_range(time_range);

        let query = "sum(increase(streamhouse_producer_errors_total[1m]))";

        let response = self.query_range(query, start, end, &step).await?;

        self.extract_time_series(&response)
    }

    /// Get current circuit breaker state
    pub async fn get_circuit_breaker_state(&self) -> Result<i64, PrometheusError> {
        let query = "streamhouse_circuit_breaker_state";
        let response = self.query(query).await?;

        if let PrometheusData::Vector(results) = response.data {
            if let Some(result) = results.first() {
                return result.value.1.parse().map_err(|_| PrometheusError::ParseFailed("Invalid value".to_string()));
            }
        }

        Ok(0) // Default to Closed
    }

    /// Get S3 throttling rate
    pub async fn get_throttling_rate(&self) -> Result<f64, PrometheusError> {
        let query = r#"
            sum(rate(streamhouse_throttle_decisions_total{decision="rate_limited"}[5m]))
            /
            (sum(rate(streamhouse_throttle_decisions_total[5m])) + 0.0001)
            * 100
        "#;

        let response = self.query(query.trim()).await?;

        if let PrometheusData::Vector(results) = response.data {
            if let Some(result) = results.first() {
                return result.value.1.parse().map_err(|_| PrometheusError::ParseFailed("Invalid value".to_string()));
            }
        }

        Ok(0.0)
    }

    /// Check if Prometheus is available
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/v1/status/runtimeinfo", self.base_url);
        self.client.get(&url).send().await.is_ok()
    }

    /// Parse time range string to (start, end, step)
    fn parse_time_range(&self, time_range: &str) -> (i64, i64, String) {
        let now = chrono::Utc::now().timestamp();

        match time_range {
            "5m" => (now - 300, now, "10s".to_string()),
            "1h" => (now - 3600, now, "60s".to_string()),
            "24h" => (now - 86400, now, "900s".to_string()),
            "7d" => (now - 604800, now, "3600s".to_string()),
            _ => (now - 3600, now, "60s".to_string()),
        }
    }

    /// Extract time series from Prometheus response
    fn extract_time_series(&self, response: &PrometheusResponse) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
        match &response.data {
            PrometheusData::Matrix(results) => {
                if let Some(result) = results.first() {
                    Ok(result
                        .values
                        .iter()
                        .map(|(ts, val)| TimeSeriesPoint {
                            timestamp: *ts as i64,
                            value: val.parse().unwrap_or(0.0),
                        })
                        .collect())
                } else {
                    Ok(vec![])
                }
            }
            PrometheusData::Vector(results) => {
                Ok(results
                    .iter()
                    .map(|r| TimeSeriesPoint {
                        timestamp: r.value.0 as i64,
                        value: r.value.1.parse().unwrap_or(0.0),
                    })
                    .collect())
            }
            _ => Ok(vec![]),
        }
    }
}

/// Latency percentiles structure
#[derive(Debug, Clone)]
pub struct LatencyPercentiles {
    pub p50: Vec<TimeSeriesPoint>,
    pub p95: Vec<TimeSeriesPoint>,
    pub p99: Vec<TimeSeriesPoint>,
}

/// Prometheus query errors
#[derive(Debug, thiserror::Error)]
pub enum PrometheusError {
    #[error("Prometheus request failed: {0}")]
    RequestFailed(String),

    #[error("Failed to parse Prometheus response: {0}")]
    ParseFailed(String),

    #[error("Prometheus unavailable")]
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_range() {
        let client = PrometheusClient::new("http://localhost:9090");

        let (start, end, step) = client.parse_time_range("5m");
        assert!(end - start == 300);
        assert_eq!(step, "10s");

        let (start, end, step) = client.parse_time_range("1h");
        assert!(end - start == 3600);
        assert_eq!(step, "60s");
    }
}
