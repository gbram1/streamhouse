//! Compliance Reports Generation
//!
//! Generates compliance reports from audit data for various regulatory frameworks.
//!
//! ## Supported Frameworks
//!
//! - **SOC 2 Type II**: Access controls, system monitoring, data integrity
//! - **GDPR**: Data access logging, consent tracking, retention compliance
//! - **HIPAA**: PHI access audit, security incident tracking
//! - **PCI DSS**: Cardholder data access, security monitoring
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::compliance::{ComplianceReporter, ReportConfig, Framework};
//!
//! let reporter = ComplianceReporter::new(audit_store);
//!
//! // Generate SOC 2 report
//! let report = reporter.generate(
//!     Framework::Soc2,
//!     ReportConfig::default()
//!         .date_range(start, end)
//!         .format(ReportFormat::Json),
//! ).await?;
//!
//! // Export to file
//! report.export_to_file("soc2_report.json").await?;
//! ```
//!
//! ## Report Contents
//!
//! Each report includes:
//! - Executive summary
//! - Time period covered
//! - Access control events
//! - Security incidents
//! - Data access patterns
//! - Anomaly detection results
//! - Recommendations

use crate::audit_store::{AuditQuery, AuditStore, StoredAuditRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

/// Compliance report errors
#[derive(Debug, Error)]
pub enum ComplianceError {
    #[error("Audit store error: {0}")]
    AuditStore(#[from] crate::audit_store::AuditStoreError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid date range")]
    InvalidDateRange,

    #[error("No data available for report")]
    NoData,
}

pub type Result<T> = std::result::Result<T, ComplianceError>;

/// Compliance frameworks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Framework {
    /// SOC 2 Type II
    Soc2,
    /// General Data Protection Regulation
    Gdpr,
    /// Health Insurance Portability and Accountability Act
    Hipaa,
    /// Payment Card Industry Data Security Standard
    PciDss,
    /// Custom/General compliance report
    Custom,
}

impl Framework {
    /// Get framework name
    pub fn name(&self) -> &'static str {
        match self {
            Framework::Soc2 => "SOC 2 Type II",
            Framework::Gdpr => "GDPR",
            Framework::Hipaa => "HIPAA",
            Framework::PciDss => "PCI DSS",
            Framework::Custom => "Custom",
        }
    }

    /// Get framework description
    pub fn description(&self) -> &'static str {
        match self {
            Framework::Soc2 => "Service Organization Control 2 Type II - Security, Availability, Processing Integrity, Confidentiality, Privacy",
            Framework::Gdpr => "General Data Protection Regulation - EU data protection and privacy",
            Framework::Hipaa => "Health Insurance Portability and Accountability Act - Protected health information",
            Framework::PciDss => "Payment Card Industry Data Security Standard - Cardholder data protection",
            Framework::Custom => "Custom compliance report",
        }
    }
}

/// Report output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportFormat {
    #[default]
    Json,
    Csv,
    Html,
}

/// Report configuration
#[derive(Debug, Clone)]
pub struct ReportConfig {
    /// Start of reporting period (Unix timestamp ms)
    pub start_time: Option<i64>,
    /// End of reporting period (Unix timestamp ms)
    pub end_time: Option<i64>,
    /// Output format
    pub format: ReportFormat,
    /// Include raw audit records
    pub include_raw_records: bool,
    /// Maximum records to include (for raw records section)
    pub max_records: usize,
    /// Filter by organization
    pub organization_id: Option<String>,
    /// Filter by user/API key
    pub api_key_id: Option<String>,
    /// Include recommendations
    pub include_recommendations: bool,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            format: ReportFormat::Json,
            include_raw_records: false,
            max_records: 1000,
            organization_id: None,
            api_key_id: None,
            include_recommendations: true,
        }
    }
}

impl ReportConfig {
    /// Set date range
    pub fn date_range(mut self, start: i64, end: i64) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// Set output format
    pub fn format(mut self, format: ReportFormat) -> Self {
        self.format = format;
        self
    }

    /// Include raw audit records
    pub fn with_raw_records(mut self, include: bool) -> Self {
        self.include_raw_records = include;
        self
    }

    /// Filter by organization
    pub fn organization(mut self, org_id: impl Into<String>) -> Self {
        self.organization_id = Some(org_id.into());
        self
    }

    /// Filter by API key
    pub fn api_key(mut self, key_id: impl Into<String>) -> Self {
        self.api_key_id = Some(key_id.into());
        self
    }
}

/// Access control event summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlSummary {
    /// Total authentication attempts
    pub total_auth_attempts: u64,
    /// Successful authentications
    pub successful_auths: u64,
    /// Failed authentications
    pub failed_auths: u64,
    /// Authentication failure rate
    pub failure_rate: f64,
    /// Unique users/keys
    pub unique_principals: u64,
    /// Breakdown by method
    pub by_method: HashMap<String, u64>,
}

/// Data access summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessSummary {
    /// Total data access events
    pub total_accesses: u64,
    /// Read operations
    pub read_operations: u64,
    /// Write operations
    pub write_operations: u64,
    /// Delete operations
    pub delete_operations: u64,
    /// Unique resources accessed
    pub unique_resources: u64,
    /// Top accessed resources
    pub top_resources: Vec<(String, u64)>,
}

/// Security incident summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIncidentSummary {
    /// Total security events
    pub total_events: u64,
    /// Authorization failures
    pub auth_failures: u64,
    /// Rate limit events
    pub rate_limit_events: u64,
    /// Potential anomalies detected
    pub anomalies: u64,
    /// Events by severity
    pub by_severity: HashMap<String, u64>,
}

/// System availability summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilitySummary {
    /// Total requests processed
    pub total_requests: u64,
    /// Successful requests (2xx/3xx)
    pub successful_requests: u64,
    /// Failed requests (4xx/5xx)
    pub failed_requests: u64,
    /// Uptime percentage
    pub uptime_percentage: f64,
    /// Average response time (ms)
    pub avg_response_time_ms: f64,
    /// 95th percentile response time (ms)
    pub p95_response_time_ms: f64,
    /// Error breakdown by status code
    pub errors_by_status: HashMap<u16, u64>,
}

/// Compliance finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Finding ID
    pub id: String,
    /// Severity level
    pub severity: FindingSeverity,
    /// Finding category
    pub category: String,
    /// Description
    pub description: String,
    /// Affected control (e.g., "CC6.1" for SOC 2)
    pub control: Option<String>,
    /// Evidence references
    pub evidence: Vec<String>,
    /// Recommendation
    pub recommendation: Option<String>,
}

/// Finding severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Generated compliance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// Report metadata
    pub metadata: ReportMetadata,
    /// Executive summary
    pub executive_summary: String,
    /// Access control summary
    pub access_control: AccessControlSummary,
    /// Data access summary
    pub data_access: DataAccessSummary,
    /// Security incidents
    pub security_incidents: SecurityIncidentSummary,
    /// System availability
    pub availability: AvailabilitySummary,
    /// Compliance findings
    pub findings: Vec<Finding>,
    /// Recommendations
    pub recommendations: Vec<String>,
    /// Raw audit records (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_records: Option<Vec<StoredAuditRecord>>,
}

/// Report metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetadata {
    /// Report ID
    pub report_id: String,
    /// Framework
    pub framework: Framework,
    /// Report title
    pub title: String,
    /// Generation timestamp
    pub generated_at: String,
    /// Reporting period start
    pub period_start: String,
    /// Reporting period end
    pub period_end: String,
    /// Total records analyzed
    pub total_records_analyzed: u64,
    /// Report version
    pub version: String,
}

impl ComplianceReport {
    /// Export report to file
    pub async fn export_to_file(&self, path: &str) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        let mut file = File::create(path).await?;
        file.write_all(content.as_bytes()).await?;
        Ok(())
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Convert to CSV format (simplified)
    pub fn to_csv(&self) -> String {
        let mut csv = String::new();

        // Header
        csv.push_str("Report ID,Framework,Generated At,Period Start,Period End\n");
        csv.push_str(&format!(
            "{},{},{},{},{}\n\n",
            self.metadata.report_id,
            self.metadata.framework.name(),
            self.metadata.generated_at,
            self.metadata.period_start,
            self.metadata.period_end,
        ));

        // Summary metrics
        csv.push_str("Metric,Value\n");
        csv.push_str(&format!("Total Auth Attempts,{}\n", self.access_control.total_auth_attempts));
        csv.push_str(&format!("Auth Failure Rate,{:.2}%\n", self.access_control.failure_rate * 100.0));
        csv.push_str(&format!("Total Data Accesses,{}\n", self.data_access.total_accesses));
        csv.push_str(&format!("Security Events,{}\n", self.security_incidents.total_events));
        csv.push_str(&format!("System Uptime,{:.2}%\n", self.availability.uptime_percentage));
        csv.push_str(&format!("Avg Response Time,{:.2}ms\n\n", self.availability.avg_response_time_ms));

        // Findings
        csv.push_str("Finding ID,Severity,Category,Description\n");
        for finding in &self.findings {
            csv.push_str(&format!(
                "{},{:?},{},{}\n",
                finding.id,
                finding.severity,
                finding.category,
                finding.description.replace(',', ";"),
            ));
        }

        csv
    }
}

/// Compliance report generator
pub struct ComplianceReporter {
    audit_store: Arc<AuditStore>,
}

impl ComplianceReporter {
    /// Create a new compliance reporter
    pub fn new(audit_store: Arc<AuditStore>) -> Self {
        Self { audit_store }
    }

    /// Generate a compliance report
    pub async fn generate(
        &self,
        framework: Framework,
        config: ReportConfig,
    ) -> Result<ComplianceReport> {
        // Build query
        let mut query = AuditQuery::default();

        if let Some(start) = config.start_time {
            query = query.since(start);
        }
        if let Some(end) = config.end_time {
            query = query.until(end);
        }
        if let Some(ref org_id) = config.organization_id {
            query = query.organization(org_id);
        }
        if let Some(ref key_id) = config.api_key_id {
            query = query.api_key(key_id);
        }

        // Fetch audit records
        let records = self.audit_store.query(query).await?;

        if records.is_empty() {
            return Err(ComplianceError::NoData);
        }

        // Calculate time range
        let (period_start, period_end) = self.calculate_period(&records, &config);

        // Generate summaries
        let access_control = self.analyze_access_control(&records);
        let data_access = self.analyze_data_access(&records);
        let security_incidents = self.analyze_security_incidents(&records);
        let availability = self.analyze_availability(&records);

        // Generate findings based on framework
        let findings = self.generate_findings(
            framework,
            &access_control,
            &data_access,
            &security_incidents,
            &availability,
        );

        // Generate recommendations
        let recommendations = if config.include_recommendations {
            self.generate_recommendations(framework, &findings)
        } else {
            vec![]
        };

        // Generate executive summary
        let executive_summary = self.generate_executive_summary(
            framework,
            &access_control,
            &security_incidents,
            &availability,
            &findings,
        );

        // Include raw records if requested
        let raw_records = if config.include_raw_records {
            Some(records.into_iter().take(config.max_records).collect())
        } else {
            None
        };

        // Generate report ID
        let report_id = format!(
            "{}-{}-{}",
            framework.name().to_lowercase().replace(' ', "-"),
            Utc::now().format("%Y%m%d"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        Ok(ComplianceReport {
            metadata: ReportMetadata {
                report_id,
                framework,
                title: format!("{} Compliance Report", framework.name()),
                generated_at: Utc::now().to_rfc3339(),
                period_start,
                period_end,
                total_records_analyzed: self.audit_store.current_sequence(),
                version: "1.0".to_string(),
            },
            executive_summary,
            access_control,
            data_access,
            security_incidents,
            availability,
            findings,
            recommendations,
            raw_records,
        })
    }

    fn calculate_period(
        &self,
        records: &[StoredAuditRecord],
        config: &ReportConfig,
    ) -> (String, String) {
        let start = config
            .start_time
            .or_else(|| records.first().map(|r| r.entry.timestamp))
            .unwrap_or_else(|| Utc::now().timestamp_millis());

        let end = config
            .end_time
            .or_else(|| records.last().map(|r| r.entry.timestamp))
            .unwrap_or_else(|| Utc::now().timestamp_millis());

        let start_dt = DateTime::from_timestamp_millis(start)
            .unwrap_or_else(Utc::now);
        let end_dt = DateTime::from_timestamp_millis(end)
            .unwrap_or_else(Utc::now);

        (start_dt.to_rfc3339(), end_dt.to_rfc3339())
    }

    fn analyze_access_control(&self, records: &[StoredAuditRecord]) -> AccessControlSummary {
        let mut by_method: HashMap<String, u64> = HashMap::new();
        let mut unique_principals: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut total_auth = 0u64;
        let mut successful = 0u64;
        let mut failed = 0u64;

        for record in records {
            // Count by HTTP method
            *by_method.entry(record.entry.method.clone()).or_insert(0) += 1;

            // Track unique principals
            if let Some(ref key) = record.entry.api_key_id {
                unique_principals.insert(key.clone());
            }

            // Count auth events (401/403 are auth failures)
            if record.entry.path.contains("/api/") {
                total_auth += 1;
                if record.entry.status_code == 401 || record.entry.status_code == 403 {
                    failed += 1;
                } else {
                    successful += 1;
                }
            }
        }

        let failure_rate = if total_auth > 0 {
            failed as f64 / total_auth as f64
        } else {
            0.0
        };

        AccessControlSummary {
            total_auth_attempts: total_auth,
            successful_auths: successful,
            failed_auths: failed,
            failure_rate,
            unique_principals: unique_principals.len() as u64,
            by_method,
        }
    }

    fn analyze_data_access(&self, records: &[StoredAuditRecord]) -> DataAccessSummary {
        let mut total = 0u64;
        let mut reads = 0u64;
        let mut writes = 0u64;
        let mut deletes = 0u64;
        let mut resources: HashMap<String, u64> = HashMap::new();

        for record in records {
            total += 1;

            // Categorize by HTTP method
            match record.entry.method.as_str() {
                "GET" | "HEAD" => reads += 1,
                "POST" | "PUT" | "PATCH" => writes += 1,
                "DELETE" => deletes += 1,
                _ => {}
            }

            // Extract resource from path
            let resource = record.entry.path.split('/').take(4).collect::<Vec<_>>().join("/");
            *resources.entry(resource).or_insert(0) += 1;
        }

        // Top resources
        let mut top_resources: Vec<_> = resources.into_iter().collect();
        top_resources.sort_by(|a, b| b.1.cmp(&a.1));
        top_resources.truncate(10);

        DataAccessSummary {
            total_accesses: total,
            read_operations: reads,
            write_operations: writes,
            delete_operations: deletes,
            unique_resources: top_resources.len() as u64,
            top_resources,
        }
    }

    fn analyze_security_incidents(&self, records: &[StoredAuditRecord]) -> SecurityIncidentSummary {
        let mut total = 0u64;
        let mut auth_failures = 0u64;
        let mut rate_limits = 0u64;
        let mut by_severity: HashMap<String, u64> = HashMap::new();

        for record in records {
            // Count security-relevant events
            match record.entry.status_code {
                401 => {
                    auth_failures += 1;
                    total += 1;
                    *by_severity.entry("medium".to_string()).or_insert(0) += 1;
                }
                403 => {
                    auth_failures += 1;
                    total += 1;
                    *by_severity.entry("high".to_string()).or_insert(0) += 1;
                }
                429 => {
                    rate_limits += 1;
                    total += 1;
                    *by_severity.entry("low".to_string()).or_insert(0) += 1;
                }
                500..=599 => {
                    total += 1;
                    *by_severity.entry("high".to_string()).or_insert(0) += 1;
                }
                _ => {}
            }
        }

        // Simple anomaly detection: unusual activity patterns
        let anomalies = if auth_failures > records.len() as u64 / 10 {
            1 // High failure rate is an anomaly
        } else {
            0
        };

        SecurityIncidentSummary {
            total_events: total,
            auth_failures,
            rate_limit_events: rate_limits,
            anomalies,
            by_severity,
        }
    }

    fn analyze_availability(&self, records: &[StoredAuditRecord]) -> AvailabilitySummary {
        let mut total = 0u64;
        let mut successful = 0u64;
        let mut failed = 0u64;
        let mut response_times: Vec<u64> = Vec::new();
        let mut errors_by_status: HashMap<u16, u64> = HashMap::new();

        for record in records {
            total += 1;
            response_times.push(record.entry.duration_ms);

            if record.entry.status_code < 400 {
                successful += 1;
            } else {
                failed += 1;
                *errors_by_status.entry(record.entry.status_code).or_insert(0) += 1;
            }
        }

        let uptime = if total > 0 {
            (successful as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let avg_response = if !response_times.is_empty() {
            response_times.iter().sum::<u64>() as f64 / response_times.len() as f64
        } else {
            0.0
        };

        // Calculate p95
        let p95_response = if !response_times.is_empty() {
            let mut sorted = response_times.clone();
            sorted.sort();
            let idx = (sorted.len() as f64 * 0.95) as usize;
            sorted.get(idx.min(sorted.len() - 1)).copied().unwrap_or(0) as f64
        } else {
            0.0
        };

        AvailabilitySummary {
            total_requests: total,
            successful_requests: successful,
            failed_requests: failed,
            uptime_percentage: uptime,
            avg_response_time_ms: avg_response,
            p95_response_time_ms: p95_response,
            errors_by_status,
        }
    }

    fn generate_findings(
        &self,
        framework: Framework,
        access_control: &AccessControlSummary,
        _data_access: &DataAccessSummary,
        security_incidents: &SecurityIncidentSummary,
        availability: &AvailabilitySummary,
    ) -> Vec<Finding> {
        let mut findings = Vec::new();
        let mut finding_id = 1;

        // High authentication failure rate
        if access_control.failure_rate > 0.1 {
            findings.push(Finding {
                id: format!("F-{:03}", finding_id),
                severity: FindingSeverity::High,
                category: "Access Control".to_string(),
                description: format!(
                    "Authentication failure rate ({:.1}%) exceeds threshold (10%)",
                    access_control.failure_rate * 100.0
                ),
                control: match framework {
                    Framework::Soc2 => Some("CC6.1".to_string()),
                    Framework::PciDss => Some("8.2".to_string()),
                    _ => None,
                },
                evidence: vec!["Audit log analysis".to_string()],
                recommendation: Some("Review failed authentication attempts and implement additional security controls".to_string()),
            });
            finding_id += 1;
        }

        // Low uptime
        if availability.uptime_percentage < 99.9 {
            findings.push(Finding {
                id: format!("F-{:03}", finding_id),
                severity: if availability.uptime_percentage < 99.0 {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                },
                category: "Availability".to_string(),
                description: format!(
                    "System availability ({:.2}%) below target (99.9%)",
                    availability.uptime_percentage
                ),
                control: match framework {
                    Framework::Soc2 => Some("A1.1".to_string()),
                    _ => None,
                },
                evidence: vec!["Request success rate analysis".to_string()],
                recommendation: Some("Investigate error patterns and improve system reliability".to_string()),
            });
            finding_id += 1;
        }

        // Security anomalies
        if security_incidents.anomalies > 0 {
            findings.push(Finding {
                id: format!("F-{:03}", finding_id),
                severity: FindingSeverity::Medium,
                category: "Security Monitoring".to_string(),
                description: format!(
                    "{} potential security anomalies detected",
                    security_incidents.anomalies
                ),
                control: match framework {
                    Framework::Soc2 => Some("CC7.2".to_string()),
                    Framework::Hipaa => Some("164.312(b)".to_string()),
                    _ => None,
                },
                evidence: vec!["Anomaly detection analysis".to_string()],
                recommendation: Some("Review detected anomalies and implement additional monitoring".to_string()),
            });
            finding_id += 1;
        }

        // High response times
        if availability.p95_response_time_ms > 1000.0 {
            findings.push(Finding {
                id: format!("F-{:03}", finding_id),
                severity: FindingSeverity::Low,
                category: "Performance".to_string(),
                description: format!(
                    "95th percentile response time ({:.0}ms) exceeds 1 second",
                    availability.p95_response_time_ms
                ),
                control: None,
                evidence: vec!["Response time analysis".to_string()],
                recommendation: Some("Optimize slow endpoints and consider caching strategies".to_string()),
            });
        }

        findings
    }

    fn generate_recommendations(
        &self,
        framework: Framework,
        findings: &[Finding],
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Framework-specific recommendations
        match framework {
            Framework::Soc2 => {
                recommendations.push("Maintain continuous audit logging for all system access".to_string());
                recommendations.push("Implement multi-factor authentication for administrative access".to_string());
                recommendations.push("Conduct regular access reviews and revoke unused credentials".to_string());
            }
            Framework::Gdpr => {
                recommendations.push("Ensure data subject access requests can be fulfilled within 30 days".to_string());
                recommendations.push("Implement data retention policies and automated deletion".to_string());
                recommendations.push("Maintain records of processing activities".to_string());
            }
            Framework::Hipaa => {
                recommendations.push("Encrypt all PHI at rest and in transit".to_string());
                recommendations.push("Implement automatic logoff for inactive sessions".to_string());
                recommendations.push("Conduct regular risk assessments".to_string());
            }
            Framework::PciDss => {
                recommendations.push("Implement strong access control measures".to_string());
                recommendations.push("Regularly test security systems and processes".to_string());
                recommendations.push("Maintain an information security policy".to_string());
            }
            Framework::Custom => {}
        }

        // Finding-based recommendations
        for finding in findings {
            if let Some(ref rec) = finding.recommendation {
                if !recommendations.contains(rec) {
                    recommendations.push(rec.clone());
                }
            }
        }

        recommendations
    }

    fn generate_executive_summary(
        &self,
        framework: Framework,
        access_control: &AccessControlSummary,
        security_incidents: &SecurityIncidentSummary,
        availability: &AvailabilitySummary,
        findings: &[Finding],
    ) -> String {
        let critical_findings = findings.iter().filter(|f| f.severity == FindingSeverity::Critical).count();
        let high_findings = findings.iter().filter(|f| f.severity == FindingSeverity::High).count();

        let overall_status = if critical_findings > 0 {
            "requires immediate attention"
        } else if high_findings > 0 {
            "requires attention"
        } else {
            "meets compliance requirements"
        };

        format!(
            "This {} compliance report covers the audit period and {} overall. \
            During the reporting period, the system processed {} authentication attempts \
            with a {:.1}% failure rate. System availability was {:.2}%. \
            {} security events were recorded, including {} authorization failures. \
            The assessment identified {} findings requiring attention ({} critical, {} high severity).",
            framework.name(),
            overall_status,
            access_control.total_auth_attempts,
            access_control.failure_rate * 100.0,
            availability.uptime_percentage,
            security_incidents.total_events,
            security_incidents.auth_failures,
            findings.len(),
            critical_findings,
            high_findings,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditEntry;
    use crate::audit_store::AuditStoreConfig;

    fn mock_audit_entry(request_id: u64, status_code: u16, method: &str) -> AuditEntry {
        AuditEntry {
            request_id,
            method: method.to_string(),
            path: "/api/v1/topics".to_string(),
            query: None,
            api_key_id: Some("key_123".to_string()),
            organization_id: Some("org_456".to_string()),
            status_code,
            duration_ms: 45,
            operation: "list_topics".to_string(),
            client_ip: Some("127.0.0.1".to_string()),
            user_agent: Some("test-client/1.0".to_string()),
            success: status_code < 400,
            error: if status_code >= 400 { Some("Error".to_string()) } else { None },
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[tokio::test]
    async fn test_generate_soc2_report() {
        let config = AuditStoreConfig::memory().skip_verification();
        let store = AuditStore::new(config).await.unwrap();

        // Add some audit entries
        for i in 0..10 {
            let status = if i % 5 == 0 { 401 } else { 200 };
            let method = if i % 3 == 0 { "POST" } else { "GET" };
            store.append(&mock_audit_entry(i, status, method)).await.unwrap();
        }

        let reporter = ComplianceReporter::new(store);
        let report = reporter
            .generate(Framework::Soc2, ReportConfig::default())
            .await
            .unwrap();

        assert_eq!(report.metadata.framework, Framework::Soc2);
        assert!(!report.executive_summary.is_empty());
        assert!(report.access_control.total_auth_attempts > 0);
    }

    #[tokio::test]
    async fn test_generate_gdpr_report() {
        let config = AuditStoreConfig::memory().skip_verification();
        let store = AuditStore::new(config).await.unwrap();

        for i in 0..5 {
            store.append(&mock_audit_entry(i, 200, "GET")).await.unwrap();
        }

        let reporter = ComplianceReporter::new(store);
        let report = reporter
            .generate(Framework::Gdpr, ReportConfig::default())
            .await
            .unwrap();

        assert_eq!(report.metadata.framework, Framework::Gdpr);
        assert!(report.recommendations.iter().any(|r| r.contains("data subject")));
    }

    #[tokio::test]
    async fn test_report_with_findings() {
        let config = AuditStoreConfig::memory().skip_verification();
        let store = AuditStore::new(config).await.unwrap();

        // Create many failed auth attempts to trigger findings
        for i in 0..20 {
            let status = if i < 15 { 401 } else { 200 }; // 75% failure rate
            store.append(&mock_audit_entry(i, status, "GET")).await.unwrap();
        }

        let reporter = ComplianceReporter::new(store);
        let report = reporter
            .generate(Framework::Soc2, ReportConfig::default())
            .await
            .unwrap();

        // Should have findings for high failure rate
        assert!(!report.findings.is_empty());
        assert!(report.findings.iter().any(|f| f.category == "Access Control"));
    }

    #[tokio::test]
    async fn test_report_to_csv() {
        let config = AuditStoreConfig::memory().skip_verification();
        let store = AuditStore::new(config).await.unwrap();

        for i in 0..5 {
            store.append(&mock_audit_entry(i, 200, "GET")).await.unwrap();
        }

        let reporter = ComplianceReporter::new(store);
        let report = reporter
            .generate(Framework::Custom, ReportConfig::default())
            .await
            .unwrap();

        let csv = report.to_csv();
        assert!(csv.contains("Report ID"));
        assert!(csv.contains("Framework"));
        assert!(csv.contains("Metric,Value"));
    }

    #[test]
    fn test_framework_info() {
        assert_eq!(Framework::Soc2.name(), "SOC 2 Type II");
        assert!(Framework::Gdpr.description().contains("EU"));
        assert!(Framework::Hipaa.description().contains("health"));
    }

    #[test]
    fn test_report_config_builder() {
        let config = ReportConfig::default()
            .date_range(1000, 2000)
            .format(ReportFormat::Csv)
            .organization("org_123")
            .with_raw_records(true);

        assert_eq!(config.start_time, Some(1000));
        assert_eq!(config.end_time, Some(2000));
        assert_eq!(config.format, ReportFormat::Csv);
        assert_eq!(config.organization_id, Some("org_123".to_string()));
        assert!(config.include_raw_records);
    }
}
