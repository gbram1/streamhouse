//! REST API client for StreamHouse services
//!
//! Provides HTTP client for connecting to:
//! - Schema Registry REST API
//! - Future: StreamHouse REST API (when available)

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// REST API client
pub struct RestClient {
    base_url: String,
    client: Client,
}

impl RestClient {
    /// Create a new REST client
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
        }
    }

    /// GET request
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send GET request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "HTTP {} {}: {}",
                status.as_u16(),
                status.as_str(),
                error_text
            );
        }

        response
            .json()
            .await
            .context("Failed to parse JSON response")
    }

    /// POST request
    pub async fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "HTTP {} {}: {}",
                status.as_u16(),
                status.as_str(),
                error_text
            );
        }

        response
            .json()
            .await
            .context("Failed to parse JSON response")
    }

    /// PUT request
    pub async fn put<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .put(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send PUT request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "HTTP {} {}: {}",
                status.as_u16(),
                status.as_str(),
                error_text
            );
        }

        response
            .json()
            .await
            .context("Failed to parse JSON response")
    }

    /// DELETE request
    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .context("Failed to send DELETE request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "HTTP {} {}: {}",
                status.as_u16(),
                status.as_str(),
                error_text
            );
        }

        Ok(())
    }

    /// DELETE request that returns JSON
    pub async fn delete_json<R: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .context("Failed to send DELETE request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "HTTP {} {}: {}",
                status.as_u16(),
                status.as_str(),
                error_text
            );
        }

        response
            .json()
            .await
            .context("Failed to parse JSON response")
    }
}

/// Schema Registry client (uses REST API)
pub struct SchemaRegistryClient {
    client: RestClient,
}

impl SchemaRegistryClient {
    /// Create a new Schema Registry client
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: RestClient::new(base_url),
        }
    }

    /// List all subjects
    pub async fn list_subjects(&self) -> Result<Vec<String>> {
        self.client.get("/subjects").await
    }

    /// List all versions for a subject
    pub async fn list_versions(&self, subject: &str) -> Result<Vec<i32>> {
        self.client
            .get(&format!("/subjects/{}/versions", subject))
            .await
    }

    /// Register a new schema
    pub async fn register_schema(
        &self,
        subject: &str,
        schema: &str,
        schema_type: Option<&str>,
    ) -> Result<RegisterSchemaResponse> {
        let request = RegisterSchemaRequest {
            schema: schema.to_string(),
            schema_type: schema_type.map(|s| s.to_string()),
            references: None,
            metadata: None,
        };

        self.client
            .post(&format!("/subjects/{}/versions", subject), &request)
            .await
    }

    /// Get schema by version
    pub async fn get_schema_by_version(
        &self,
        subject: &str,
        version: &str,
    ) -> Result<SchemaResponse> {
        self.client
            .get(&format!("/subjects/{}/versions/{}", subject, version))
            .await
    }

    /// Get schema by ID
    pub async fn get_schema_by_id(&self, id: i32) -> Result<SchemaResponse> {
        self.client.get(&format!("/schemas/ids/{}", id)).await
    }

    /// Delete schema version
    pub async fn delete_schema_version(&self, subject: &str, version: &str) -> Result<i32> {
        self.client
            .delete_json(&format!("/subjects/{}/versions/{}", subject, version))
            .await
    }

    /// Delete subject
    pub async fn delete_subject(&self, subject: &str) -> Result<Vec<i32>> {
        self.client
            .delete_json(&format!("/subjects/{}", subject))
            .await
    }

    /// Get global compatibility config
    pub async fn get_global_config(&self) -> Result<CompatibilityConfig> {
        self.client.get("/config").await
    }

    /// Set global compatibility config
    pub async fn set_global_config(&self, compatibility: &str) -> Result<CompatibilityConfig> {
        let request = SetCompatibilityRequest {
            compatibility: compatibility.to_string(),
        };
        self.client.put("/config", &request).await
    }

    /// Get subject-specific compatibility config
    pub async fn get_subject_config(&self, subject: &str) -> Result<CompatibilityConfig> {
        self.client.get(&format!("/config/{}", subject)).await
    }

    /// Set subject-specific compatibility config
    pub async fn set_subject_config(
        &self,
        subject: &str,
        compatibility: &str,
    ) -> Result<CompatibilityConfig> {
        let request = SetCompatibilityRequest {
            compatibility: compatibility.to_string(),
        };
        self.client
            .put(&format!("/config/{}", subject), &request)
            .await
    }
}

// Request/Response types for Schema Registry

#[derive(Debug, Serialize)]
struct RegisterSchemaRequest {
    schema: String,
    #[serde(rename = "schemaType", skip_serializing_if = "Option::is_none")]
    schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    references: Option<Vec<SchemaReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterSchemaResponse {
    pub id: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaReference {
    pub name: String,
    pub subject: String,
    pub version: i32,
}

#[derive(Debug, Deserialize)]
pub struct SchemaResponse {
    pub subject: String,
    pub version: i32,
    pub id: i32,
    pub schema: String,
    #[serde(rename = "schemaType")]
    pub schema_type: String,
    #[serde(default)]
    pub references: Vec<SchemaReference>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompatibilityConfig {
    #[serde(rename = "compatibilityLevel")]
    pub compatibility_level: String,
}

#[derive(Debug, Serialize)]
struct SetCompatibilityRequest {
    compatibility: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = RestClient::new("http://localhost:8081");
        assert_eq!(client.base_url, "http://localhost:8081");
    }

    #[test]
    fn test_schema_registry_client_creation() {
        let client = SchemaRegistryClient::new("http://localhost:8081");
        assert_eq!(client.client.base_url, "http://localhost:8081");
    }
}
