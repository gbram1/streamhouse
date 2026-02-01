//! HTTP client for Schema Registry
//!
//! Provides a simple interface to register and retrieve schemas from the
//! StreamHouse Schema Registry REST API.

use crate::error::{ClientError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Schema format enum for registration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SchemaFormat {
    Avro,
    Protobuf,
    Json,
}

impl SchemaFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            SchemaFormat::Avro => "AVRO",
            SchemaFormat::Protobuf => "PROTOBUF",
            SchemaFormat::Json => "JSON",
        }
    }
}

/// Schema object returned from registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub id: i32,
    pub subject: String,
    pub version: i32,
    pub schema: String,

    #[serde(rename = "schemaType")]
    pub schema_type: SchemaFormat,
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

/// HTTP client for Schema Registry operations
pub struct SchemaRegistryClient {
    base_url: String,
    http_client: reqwest::Client,
}

impl SchemaRegistryClient {
    /// Create a new Schema Registry client
    ///
    /// # Arguments
    /// * `base_url` - Base URL of schema registry (e.g., "http://localhost:8080")
    pub fn new(base_url: String) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url,
            http_client,
        }
    }

    /// Register a schema and get its ID
    ///
    /// If the schema already exists, returns the existing ID.
    /// If it's a new schema, registers it and returns the new ID.
    ///
    /// # Arguments
    /// * `subject` - Subject name (e.g., "orders-value")
    /// * `schema` - Schema definition as JSON string
    /// * `format` - Schema format (Avro, Protobuf, or JSON)
    ///
    /// # Returns
    /// Schema ID that can be used to encode messages
    pub async fn register_schema(
        &self,
        subject: &str,
        schema: &str,
        format: SchemaFormat,
    ) -> Result<i32> {
        let url = format!(
            "{}/schemas/subjects/{}/versions",
            self.base_url, subject
        );

        let request = RegisterSchemaRequest {
            schema: schema.to_string(),
            schema_type: format.as_str().to_string(),
        };

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                ClientError::SchemaRegistryError(format!(
                    "Failed to register schema: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::SchemaRegistryError(format!(
                "Schema registration failed with status {}: {}",
                status, body
            )));
        }

        let register_response: RegisterSchemaResponse = response.json().await.map_err(|e| {
            ClientError::SchemaRegistryError(format!(
                "Failed to parse registration response: {}",
                e
            ))
        })?;

        tracing::debug!(
            schema_id = register_response.id,
            subject = subject,
            format = ?format,
            "Schema registered successfully"
        );

        Ok(register_response.id)
    }

    /// Get schema by ID
    ///
    /// # Arguments
    /// * `id` - Schema ID
    ///
    /// # Returns
    /// Schema object with definition and metadata
    pub async fn get_schema_by_id(&self, id: i32) -> Result<Schema> {
        let url = format!("{}/schemas/schemas/{}", self.base_url, id);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                ClientError::SchemaRegistryError(format!("Failed to fetch schema: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::SchemaRegistryError(format!(
                "Schema lookup failed with status {}: {}",
                status, body
            )));
        }

        let schema: Schema = response.json().await.map_err(|e| {
            ClientError::SchemaRegistryError(format!("Failed to parse schema response: {}", e))
        })?;

        tracing::debug!(
            schema_id = id,
            subject = %schema.subject,
            version = schema.version,
            "Schema retrieved successfully"
        );

        Ok(schema)
    }

    /// Test compatibility of a schema with existing versions
    ///
    /// # Arguments
    /// * `subject` - Subject name
    /// * `schema` - Schema definition to test
    /// * `version` - Version to test against ("latest" or version number)
    ///
    /// # Returns
    /// true if compatible, false otherwise
    pub async fn test_compatibility(
        &self,
        subject: &str,
        schema: &str,
        version: &str,
    ) -> Result<bool> {
        let url = format!(
            "{}/schemas/compatibility/subjects/{}/versions/{}",
            self.base_url, subject, version
        );

        let request = serde_json::json!({
            "schema": schema,
        });

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                ClientError::SchemaRegistryError(format!(
                    "Failed to test compatibility: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::SchemaRegistryError(format!(
                "Compatibility test failed with status {}: {}",
                status, body
            )));
        }

        let result: serde_json::Value = response.json().await.map_err(|e| {
            ClientError::SchemaRegistryError(format!(
                "Failed to parse compatibility response: {}",
                e
            ))
        })?;

        let is_compatible = result
            .get("is_compatible")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        tracing::debug!(
            subject = subject,
            version = version,
            is_compatible = is_compatible,
            "Compatibility test completed"
        );

        Ok(is_compatible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_format_as_str() {
        assert_eq!(SchemaFormat::Avro.as_str(), "AVRO");
        assert_eq!(SchemaFormat::Protobuf.as_str(), "PROTOBUF");
        assert_eq!(SchemaFormat::Json.as_str(), "JSON");
    }
}
