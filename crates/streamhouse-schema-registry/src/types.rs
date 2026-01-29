//! Core Schema Registry Types

use serde::{Deserialize, Serialize};

/// Schema format (Avro, Protobuf, JSON Schema)
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

/// Compatibility mode for schema evolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompatibilityMode {
    /// New schema can read data written with old schema
    Backward,

    /// Old schema can read data written with new schema
    Forward,

    /// Both backward and forward compatible
    Full,

    /// Backward compatible with all previous versions
    BackwardTransitive,

    /// Forward compatible with all previous versions
    ForwardTransitive,

    /// Full compatibility with all previous versions
    FullTransitive,

    /// No compatibility checking
    None,
}

impl Default for CompatibilityMode {
    fn default() -> Self {
        CompatibilityMode::Backward
    }
}

/// Schema metadata stored in registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Unique schema ID
    pub id: i32,

    /// Subject (topic-key or topic-value)
    pub subject: String,

    /// Version number (starts at 1)
    pub version: i32,

    /// Schema format
    pub schema_type: SchemaFormat,

    /// Schema definition (JSON string)
    pub schema: String,

    /// References to other schemas (for nested types)
    #[serde(default)]
    pub references: Vec<SchemaReference>,

    /// Metadata
    #[serde(default)]
    pub metadata: SchemaMetadata,
}

/// Reference to another schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaReference {
    pub name: String,
    pub subject: String,
    pub version: i32,
}

/// Schema metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaMetadata {
    /// Properties map
    #[serde(default)]
    pub properties: std::collections::HashMap<String, String>,

    /// Tags for organization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Ruleset for validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ruleset: Option<String>,
}

/// Subject configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectConfig {
    pub subject: String,
    pub compatibility: CompatibilityMode,
}

/// Schema registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSchemaRequest {
    pub schema: String,

    #[serde(rename = "schemaType")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<SchemaFormat>,

    #[serde(default)]
    pub references: Vec<SchemaReference>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SchemaMetadata>,
}

/// Schema registration response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSchemaResponse {
    pub id: i32,
}

/// Schema lookup response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaResponse {
    pub subject: String,
    pub version: i32,
    pub id: i32,
    pub schema: String,

    #[serde(rename = "schemaType")]
    pub schema_type: SchemaFormat,

    #[serde(default)]
    pub references: Vec<SchemaReference>,
}

/// Version list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionsResponse {
    pub versions: Vec<i32>,
}
