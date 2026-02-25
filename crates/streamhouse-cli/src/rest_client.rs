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

    #[test]
    fn test_client_creation_with_trailing_slash() {
        let client = RestClient::new("http://localhost:8081/");
        assert_eq!(client.base_url, "http://localhost:8081/");
    }

    #[test]
    fn test_client_creation_with_path_prefix() {
        let client = RestClient::new("http://localhost:8081/api/v1");
        assert_eq!(client.base_url, "http://localhost:8081/api/v1");
    }

    #[test]
    fn test_client_creation_from_string_type() {
        let url = String::from("https://schema-registry.example.com:443");
        let client = RestClient::new(url);
        assert_eq!(
            client.base_url,
            "https://schema-registry.example.com:443"
        );
    }

    #[test]
    fn test_url_building_concatenation() {
        // Verify the URL building pattern used by get/post/put/delete
        let client = RestClient::new("http://localhost:8081");
        let url = format!("{}{}", client.base_url, "/subjects");
        assert_eq!(url, "http://localhost:8081/subjects");
    }

    #[test]
    fn test_url_building_with_subject_and_version() {
        let client = RestClient::new("http://localhost:8081");
        let subject = "my-topic-value";
        let version = "latest";
        let url = format!(
            "{}/subjects/{}/versions/{}",
            client.base_url, subject, version
        );
        assert_eq!(
            url,
            "http://localhost:8081/subjects/my-topic-value/versions/latest"
        );
    }

    #[test]
    fn test_url_building_schema_by_id() {
        let client = RestClient::new("http://localhost:8081");
        let id = 42;
        let url = format!("{}/schemas/ids/{}", client.base_url, id);
        assert_eq!(url, "http://localhost:8081/schemas/ids/42");
    }

    #[test]
    fn test_schema_registry_client_from_string() {
        let url = String::from("https://registry.prod.example.com");
        let client = SchemaRegistryClient::new(url);
        assert_eq!(
            client.client.base_url,
            "https://registry.prod.example.com"
        );
    }

    #[test]
    fn test_register_schema_request_serialization_minimal() {
        let request = RegisterSchemaRequest {
            schema: r#"{"type":"string"}"#.to_string(),
            schema_type: None,
            references: None,
            metadata: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["schema"], r#"{"type":"string"}"#);
        // Optional fields with skip_serializing_if should be absent
        assert!(parsed.get("schemaType").is_none());
        assert!(parsed.get("references").is_none());
        assert!(parsed.get("metadata").is_none());
    }

    #[test]
    fn test_register_schema_request_serialization_full() {
        let request = RegisterSchemaRequest {
            schema: r#"{"type":"record","name":"Test","fields":[]}"#.to_string(),
            schema_type: Some("AVRO".to_string()),
            references: Some(vec![SchemaReference {
                name: "other.avsc".to_string(),
                subject: "other-value".to_string(),
                version: 1,
            }]),
            metadata: Some(serde_json::json!({"owner": "team-a"})),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["schemaType"], "AVRO");
        assert_eq!(parsed["references"][0]["name"], "other.avsc");
        assert_eq!(parsed["references"][0]["subject"], "other-value");
        assert_eq!(parsed["references"][0]["version"], 1);
        assert_eq!(parsed["metadata"]["owner"], "team-a");
    }

    #[test]
    fn test_register_schema_response_deserialization() {
        let json = r#"{"id": 7}"#;
        let response: RegisterSchemaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, 7);
    }

    #[test]
    fn test_schema_reference_roundtrip() {
        let reference = SchemaReference {
            name: "com.example.User".to_string(),
            subject: "user-value".to_string(),
            version: 3,
        };
        let json = serde_json::to_string(&reference).unwrap();
        let deserialized: SchemaReference = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "com.example.User");
        assert_eq!(deserialized.subject, "user-value");
        assert_eq!(deserialized.version, 3);
    }

    #[test]
    fn test_schema_response_deserialization() {
        let json = r#"{
            "subject": "orders-value",
            "version": 2,
            "id": 10,
            "schema": "{\"type\":\"string\"}",
            "schemaType": "AVRO",
            "references": [
                {"name": "ref1", "subject": "ref-subject", "version": 1}
            ]
        }"#;
        let response: SchemaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.subject, "orders-value");
        assert_eq!(response.version, 2);
        assert_eq!(response.id, 10);
        assert_eq!(response.schema, r#"{"type":"string"}"#);
        assert_eq!(response.schema_type, "AVRO");
        assert_eq!(response.references.len(), 1);
        assert_eq!(response.references[0].name, "ref1");
    }

    #[test]
    fn test_schema_response_deserialization_no_references() {
        let json = r#"{
            "subject": "events-value",
            "version": 1,
            "id": 5,
            "schema": "{}",
            "schemaType": "JSON"
        }"#;
        let response: SchemaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.subject, "events-value");
        assert!(response.references.is_empty());
    }

    #[test]
    fn test_compatibility_config_deserialization() {
        let json = r#"{"compatibilityLevel": "BACKWARD"}"#;
        let config: CompatibilityConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.compatibility_level, "BACKWARD");
    }

    #[test]
    fn test_compatibility_config_roundtrip() {
        let config = CompatibilityConfig {
            compatibility_level: "FULL_TRANSITIVE".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CompatibilityConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.compatibility_level,
            "FULL_TRANSITIVE"
        );
        // Verify the rename is applied
        assert!(json.contains("compatibilityLevel"));
        assert!(!json.contains("compatibility_level"));
    }

    #[test]
    fn test_set_compatibility_request_serialization() {
        let request = SetCompatibilityRequest {
            compatibility: "FORWARD".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["compatibility"], "FORWARD");
    }

    #[test]
    fn test_compatibility_levels_serialize() {
        // Test all standard Schema Registry compatibility levels
        let levels = [
            "NONE",
            "BACKWARD",
            "BACKWARD_TRANSITIVE",
            "FORWARD",
            "FORWARD_TRANSITIVE",
            "FULL",
            "FULL_TRANSITIVE",
        ];
        for level in levels {
            let config = CompatibilityConfig {
                compatibility_level: level.to_string(),
            };
            let json = serde_json::to_string(&config).unwrap();
            let deserialized: CompatibilityConfig =
                serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.compatibility_level, level);
        }
    }
}
