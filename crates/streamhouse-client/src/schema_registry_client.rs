//! Schema Registry HTTP Client
//!
//! Simple HTTP client for interacting with the StreamHouse Schema Registry.

use crate::error::{ClientError, Result};
use serde::{Deserialize, Serialize};

/// Schema format
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SchemaFormat {
    Avro,
    Protobuf,
    Json,
}

/// Schema object returned from registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub id: i32,
    pub subject: String,
    pub version: i32,
    #[serde(rename = "schema")]
    pub definition: String,
    #[serde(rename = "schemaType")]
    pub format: SchemaFormat,
}

/// Schema registration request
#[derive(Debug, Serialize)]
struct RegisterSchemaRequest {
    schema: String,
    #[serde(rename = "schemaType")]
    schema_type: String,
}

/// Schema registration response
#[derive(Debug, Deserialize)]
struct RegisterSchemaResponse {
    id: i32,
}

/// Simple HTTP client for schema registry
pub struct SchemaRegistryClient {
    base_url: String,
    http_client: reqwest::Client,
}

impl SchemaRegistryClient {
    /// Create a new schema registry client
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Register a schema and get its ID
    ///
    /// If the schema already exists, returns the existing ID.
    pub async fn register_schema(
        &self,
        subject: &str,
        schema: &str,
        format: SchemaFormat,
    ) -> Result<i32> {
        let url = format!("{}/subjects/{}/versions", self.base_url, subject);

        let format_str = match format {
            SchemaFormat::Avro => "AVRO",
            SchemaFormat::Protobuf => "PROTOBUF",
            SchemaFormat::Json => "JSON",
        };

        let request_body = RegisterSchemaRequest {
            schema: schema.to_string(),
            schema_type: format_str.to_string(),
        };

        let response = self
            .http_client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ClientError::SchemaRegistryError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ClientError::SchemaRegistryError(format!(
                "Failed to register schema: {} - {}",
                status, error_text
            )));
        }

        let response_body: RegisterSchemaResponse = response.json().await.map_err(|e| {
            ClientError::SchemaRegistryError(format!("Failed to parse response: {}", e))
        })?;

        tracing::debug!(
            subject = %subject,
            schema_id = response_body.id,
            "Schema registered successfully"
        );

        Ok(response_body.id)
    }

    /// Get schema by ID
    pub async fn get_schema_by_id(&self, id: i32) -> Result<Schema> {
        let url = format!("{}/schemas/ids/{}", self.base_url, id);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ClientError::SchemaRegistryError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ClientError::SchemaRegistryError(format!(
                "Failed to get schema by ID {}: {} - {}",
                id, status, error_text
            )));
        }

        let schema: Schema = response.json().await.map_err(|e| {
            ClientError::SchemaRegistryError(format!("Failed to parse schema: {}", e))
        })?;

        Ok(schema)
    }

    /// Check if a schema is compatible with the subject's evolution policy
    pub async fn test_compatibility(&self, subject: &str, schema: &str) -> Result<bool> {
        let url = format!(
            "{}/compatibility/subjects/{}/versions/latest",
            self.base_url, subject
        );

        let request_body = serde_json::json!({
            "schema": schema,
        });

        let response = self
            .http_client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ClientError::SchemaRegistryError(e.to_string()))?;

        if !response.status().is_success() {
            // If subject doesn't exist, treat as compatible (first schema)
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(true);
            }

            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ClientError::SchemaRegistryError(format!(
                "Failed to test compatibility: {} - {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct CompatibilityResponse {
            is_compatible: bool,
        }

        let response_body: CompatibilityResponse = response.json().await.map_err(|e| {
            ClientError::SchemaRegistryError(format!("Failed to parse response: {}", e))
        })?;

        Ok(response_body.is_compatible)
    }
}
