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
    pub async fn get_throughput(
        &self,
        time_range: &str,
    ) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
        let (start, end, step) = self.parse_time_range(time_range);

        // Query rate of producer records
        let query = "sum(rate(streamhouse_producer_records_total[1m]))";

        let response = self.query_range(query, start, end, &step).await?;

        self.extract_time_series(&response)
    }

    /// Get bytes throughput
    pub async fn get_bytes_throughput(
        &self,
        time_range: &str,
    ) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
        let (start, end, step) = self.parse_time_range(time_range);

        let query = "sum(rate(streamhouse_producer_bytes_total[1m]))";

        let response = self.query_range(query, start, end, &step).await?;

        self.extract_time_series(&response)
    }

    /// Get latency percentiles
    pub async fn get_latency_percentiles(
        &self,
        time_range: &str,
    ) -> Result<LatencyPercentiles, PrometheusError> {
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
    pub async fn get_error_rate(
        &self,
        time_range: &str,
    ) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
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
    pub async fn get_error_count(
        &self,
        time_range: &str,
    ) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
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
                return result
                    .value
                    .1
                    .parse()
                    .map_err(|_| PrometheusError::ParseFailed("Invalid value".to_string()));
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
                return result
                    .value
                    .1
                    .parse()
                    .map_err(|_| PrometheusError::ParseFailed("Invalid value".to_string()));
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
    fn extract_time_series(
        &self,
        response: &PrometheusResponse,
    ) -> Result<Vec<TimeSeriesPoint>, PrometheusError> {
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
            PrometheusData::Vector(results) => Ok(results
                .iter()
                .map(|r| TimeSeriesPoint {
                    timestamp: r.value.0 as i64,
                    value: r.value.1.parse().unwrap_or(0.0),
                })
                .collect()),
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

    // =========================================================================
    // PrometheusClient construction
    // =========================================================================

    #[test]
    fn test_new_client_stores_base_url() {
        let client = PrometheusClient::new("http://localhost:9090");
        assert_eq!(client.base_url, "http://localhost:9090");
    }

    #[test]
    fn test_new_client_trims_trailing_slash() {
        let client = PrometheusClient::new("http://localhost:9090/");
        assert_eq!(client.base_url, "http://localhost:9090");
    }

    #[test]
    fn test_new_client_trims_multiple_trailing_slashes() {
        let client = PrometheusClient::new("http://prom.local:9090///");
        // trim_end_matches removes all trailing '/'
        assert_eq!(client.base_url, "http://prom.local:9090");
    }

    #[test]
    fn test_new_client_preserves_path() {
        let client = PrometheusClient::new("http://prom.local:9090/prometheus");
        assert_eq!(client.base_url, "http://prom.local:9090/prometheus");
    }

    #[test]
    fn test_new_client_with_https() {
        let client = PrometheusClient::new("https://prom.example.com");
        assert_eq!(client.base_url, "https://prom.example.com");
    }

    #[test]
    fn test_new_client_with_custom_port() {
        let client = PrometheusClient::new("http://10.0.0.1:19090");
        assert_eq!(client.base_url, "http://10.0.0.1:19090");
    }

    // =========================================================================
    // parse_time_range
    // =========================================================================

    #[test]
    fn test_parse_time_range_5m() {
        let client = PrometheusClient::new("http://localhost:9090");
        let (start, end, step) = client.parse_time_range("5m");
        assert_eq!(end - start, 300);
        assert_eq!(step, "10s");
        // end should be close to now
        let now = chrono::Utc::now().timestamp();
        assert!((end - now).abs() < 2);
    }

    #[test]
    fn test_parse_time_range_1h() {
        let client = PrometheusClient::new("http://localhost:9090");
        let (start, end, step) = client.parse_time_range("1h");
        assert_eq!(end - start, 3600);
        assert_eq!(step, "60s");
    }

    #[test]
    fn test_parse_time_range_24h() {
        let client = PrometheusClient::new("http://localhost:9090");
        let (start, end, step) = client.parse_time_range("24h");
        assert_eq!(end - start, 86400);
        assert_eq!(step, "900s");
    }

    #[test]
    fn test_parse_time_range_7d() {
        let client = PrometheusClient::new("http://localhost:9090");
        let (start, end, step) = client.parse_time_range("7d");
        assert_eq!(end - start, 604800);
        assert_eq!(step, "3600s");
    }

    #[test]
    fn test_parse_time_range_unknown_defaults_to_1h() {
        let client = PrometheusClient::new("http://localhost:9090");
        let (start, end, step) = client.parse_time_range("30d");
        assert_eq!(end - start, 3600);
        assert_eq!(step, "60s");
    }

    #[test]
    fn test_parse_time_range_empty_string_defaults_to_1h() {
        let client = PrometheusClient::new("http://localhost:9090");
        let (start, end, step) = client.parse_time_range("");
        assert_eq!(end - start, 3600);
        assert_eq!(step, "60s");
    }

    #[test]
    fn test_parse_time_range_garbage_defaults_to_1h() {
        let client = PrometheusClient::new("http://localhost:9090");
        let (start, end, step) = client.parse_time_range("not_a_range");
        assert_eq!(end - start, 3600);
        assert_eq!(step, "60s");
    }

    #[test]
    fn test_parse_time_range_end_is_recent() {
        let client = PrometheusClient::new("http://localhost:9090");
        let now = chrono::Utc::now().timestamp();
        for range in &["5m", "1h", "24h", "7d"] {
            let (_, end, _) = client.parse_time_range(range);
            // end should be within 2 seconds of now
            assert!(
                (end - now).abs() < 2,
                "Time range '{}' end {} is too far from now {}",
                range,
                end,
                now
            );
        }
    }

    #[test]
    fn test_parse_time_range_start_before_end() {
        let client = PrometheusClient::new("http://localhost:9090");
        for range in &["5m", "1h", "24h", "7d", "unknown"] {
            let (start, end, _) = client.parse_time_range(range);
            assert!(
                start < end,
                "start ({}) should be before end ({}) for range '{}'",
                start,
                end,
                range
            );
        }
    }

    // =========================================================================
    // PrometheusData deserialization
    // =========================================================================

    #[test]
    fn test_prometheus_response_vector_deserialize() {
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [
                    {
                        "metric": {"__name__": "up"},
                        "value": [1700000000.0, "1"]
                    }
                ]
            }
        }"#;
        let resp: PrometheusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "success");
        match resp.data {
            PrometheusData::Vector(results) => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].value.0, 1700000000.0);
                assert_eq!(results[0].value.1, "1");
            }
            _ => panic!("Expected Vector data"),
        }
    }

    #[test]
    fn test_prometheus_response_matrix_deserialize() {
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "matrix",
                "result": [
                    {
                        "metric": {"__name__": "rate"},
                        "values": [
                            [1700000000.0, "100"],
                            [1700000060.0, "150"],
                            [1700000120.0, "200"]
                        ]
                    }
                ]
            }
        }"#;
        let resp: PrometheusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "success");
        match resp.data {
            PrometheusData::Matrix(results) => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].values.len(), 3);
                assert_eq!(results[0].values[0].1, "100");
                assert_eq!(results[0].values[2].1, "200");
            }
            _ => panic!("Expected Matrix data"),
        }
    }

    #[test]
    fn test_prometheus_response_scalar_deserialize() {
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "scalar",
                "result": [1700000000.0, "42"]
            }
        }"#;
        let resp: PrometheusResponse = serde_json::from_str(json).unwrap();
        match resp.data {
            PrometheusData::Scalar((ts, val)) => {
                assert_eq!(ts, 1700000000.0);
                assert_eq!(val, "42");
            }
            _ => panic!("Expected Scalar data"),
        }
    }

    #[test]
    fn test_prometheus_response_empty_vector() {
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": []
            }
        }"#;
        let resp: PrometheusResponse = serde_json::from_str(json).unwrap();
        match resp.data {
            PrometheusData::Vector(results) => {
                assert!(results.is_empty());
            }
            _ => panic!("Expected Vector data"),
        }
    }

    #[test]
    fn test_prometheus_response_empty_matrix() {
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "matrix",
                "result": []
            }
        }"#;
        let resp: PrometheusResponse = serde_json::from_str(json).unwrap();
        match resp.data {
            PrometheusData::Matrix(results) => {
                assert!(results.is_empty());
            }
            _ => panic!("Expected Matrix data"),
        }
    }

    #[test]
    fn test_vector_result_with_complex_metric() {
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [
                    {
                        "metric": {"__name__": "http_requests_total", "method": "GET", "code": "200"},
                        "value": [1700000000.0, "1234"]
                    }
                ]
            }
        }"#;
        let resp: PrometheusResponse = serde_json::from_str(json).unwrap();
        if let PrometheusData::Vector(results) = resp.data {
            let metric = &results[0].metric;
            assert_eq!(metric["method"], "GET");
            assert_eq!(metric["code"], "200");
        } else {
            panic!("Expected Vector");
        }
    }

    // =========================================================================
    // extract_time_series
    // =========================================================================

    #[test]
    fn test_extract_time_series_from_matrix() {
        let client = PrometheusClient::new("http://localhost:9090");
        let response = PrometheusResponse {
            status: "success".to_string(),
            data: PrometheusData::Matrix(vec![MatrixResult {
                metric: serde_json::json!({}),
                values: vec![
                    (1700000000.0, "100.5".to_string()),
                    (1700000060.0, "200.7".to_string()),
                ],
            }]),
        };
        let series = client.extract_time_series(&response).unwrap();
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].timestamp, 1700000000);
        assert!((series[0].value - 100.5).abs() < f64::EPSILON);
        assert_eq!(series[1].timestamp, 1700000060);
        assert!((series[1].value - 200.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_time_series_from_vector() {
        let client = PrometheusClient::new("http://localhost:9090");
        let response = PrometheusResponse {
            status: "success".to_string(),
            data: PrometheusData::Vector(vec![
                VectorResult {
                    metric: serde_json::json!({"instance": "a"}),
                    value: (1700000000.0, "42".to_string()),
                },
                VectorResult {
                    metric: serde_json::json!({"instance": "b"}),
                    value: (1700000000.0, "58".to_string()),
                },
            ]),
        };
        let series = client.extract_time_series(&response).unwrap();
        assert_eq!(series.len(), 2);
        assert!((series[0].value - 42.0).abs() < f64::EPSILON);
        assert!((series[1].value - 58.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_time_series_from_empty_matrix() {
        let client = PrometheusClient::new("http://localhost:9090");
        let response = PrometheusResponse {
            status: "success".to_string(),
            data: PrometheusData::Matrix(vec![]),
        };
        let series = client.extract_time_series(&response).unwrap();
        assert!(series.is_empty());
    }

    #[test]
    fn test_extract_time_series_from_scalar_returns_empty() {
        let client = PrometheusClient::new("http://localhost:9090");
        let response = PrometheusResponse {
            status: "success".to_string(),
            data: PrometheusData::Scalar((1700000000.0, "42".to_string())),
        };
        let series = client.extract_time_series(&response).unwrap();
        assert!(series.is_empty());
    }

    #[test]
    fn test_extract_time_series_invalid_value_defaults_to_zero() {
        let client = PrometheusClient::new("http://localhost:9090");
        let response = PrometheusResponse {
            status: "success".to_string(),
            data: PrometheusData::Matrix(vec![MatrixResult {
                metric: serde_json::json!({}),
                values: vec![
                    (1700000000.0, "NaN".to_string()),
                    (1700000060.0, "not_a_number".to_string()),
                ],
            }]),
        };
        let series = client.extract_time_series(&response).unwrap();
        assert_eq!(series.len(), 2);
        // "NaN" parses to f64::NAN via parse(), but unwrap_or(0.0) catches errors.
        // Actually "NaN" does parse successfully to NaN in Rust.
        // "not_a_number" will fail and default to 0.0
        assert!((series[1].value - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_time_series_multiple_matrix_results_uses_first() {
        let client = PrometheusClient::new("http://localhost:9090");
        let response = PrometheusResponse {
            status: "success".to_string(),
            data: PrometheusData::Matrix(vec![
                MatrixResult {
                    metric: serde_json::json!({"instance": "a"}),
                    values: vec![(1700000000.0, "10".to_string())],
                },
                MatrixResult {
                    metric: serde_json::json!({"instance": "b"}),
                    values: vec![(1700000000.0, "20".to_string())],
                },
            ]),
        };
        let series = client.extract_time_series(&response).unwrap();
        // Only the first MatrixResult is used
        assert_eq!(series.len(), 1);
        assert!((series[0].value - 10.0).abs() < f64::EPSILON);
    }

    // =========================================================================
    // TimeSeriesPoint
    // =========================================================================

    #[test]
    fn test_time_series_point_serialize() {
        let point = TimeSeriesPoint {
            timestamp: 1700000000,
            value: 42.5,
        };
        let json = serde_json::to_string(&point).unwrap();
        assert!(json.contains("\"timestamp\":1700000000"));
        assert!(json.contains("\"value\":42.5"));
    }

    #[test]
    fn test_time_series_point_clone() {
        let point = TimeSeriesPoint {
            timestamp: 1700000000,
            value: 99.9,
        };
        let cloned = point.clone();
        assert_eq!(cloned.timestamp, point.timestamp);
        assert!((cloned.value - point.value).abs() < f64::EPSILON);
    }

    // =========================================================================
    // LatencyPercentiles
    // =========================================================================

    #[test]
    fn test_latency_percentiles_construction() {
        let percentiles = LatencyPercentiles {
            p50: vec![TimeSeriesPoint {
                timestamp: 1,
                value: 1.0,
            }],
            p95: vec![TimeSeriesPoint {
                timestamp: 1,
                value: 5.0,
            }],
            p99: vec![TimeSeriesPoint {
                timestamp: 1,
                value: 10.0,
            }],
        };
        assert_eq!(percentiles.p50.len(), 1);
        assert_eq!(percentiles.p95.len(), 1);
        assert_eq!(percentiles.p99.len(), 1);
        assert!((percentiles.p50[0].value - 1.0).abs() < f64::EPSILON);
        assert!((percentiles.p99[0].value - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_latency_percentiles_empty() {
        let percentiles = LatencyPercentiles {
            p50: vec![],
            p95: vec![],
            p99: vec![],
        };
        assert!(percentiles.p50.is_empty());
        assert!(percentiles.p95.is_empty());
        assert!(percentiles.p99.is_empty());
    }

    #[test]
    fn test_latency_percentiles_clone() {
        let percentiles = LatencyPercentiles {
            p50: vec![TimeSeriesPoint {
                timestamp: 1,
                value: 2.0,
            }],
            p95: vec![],
            p99: vec![],
        };
        let cloned = percentiles.clone();
        assert_eq!(cloned.p50.len(), 1);
        assert!((cloned.p50[0].value - 2.0).abs() < f64::EPSILON);
    }

    // =========================================================================
    // PrometheusError
    // =========================================================================

    #[test]
    fn test_prometheus_error_request_failed_display() {
        let err = PrometheusError::RequestFailed("connection refused".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Prometheus request failed"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_prometheus_error_parse_failed_display() {
        let err = PrometheusError::ParseFailed("invalid JSON".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Failed to parse Prometheus response"));
        assert!(msg.contains("invalid JSON"));
    }

    #[test]
    fn test_prometheus_error_unavailable_display() {
        let err = PrometheusError::Unavailable;
        let msg = format!("{}", err);
        assert_eq!(msg, "Prometheus unavailable");
    }

    #[test]
    fn test_prometheus_error_is_debug() {
        let err = PrometheusError::RequestFailed("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("RequestFailed"));
    }

    // =========================================================================
    // Client clone
    // =========================================================================

    #[test]
    fn test_client_is_cloneable() {
        let client = PrometheusClient::new("http://localhost:9090");
        let cloned = client.clone();
        assert_eq!(cloned.base_url, "http://localhost:9090");
    }
}
