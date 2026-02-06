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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompatibilityMode {
    /// New schema can read data written with old schema
    #[default]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ========================================================================
    // SchemaFormat tests
    // ========================================================================

    #[test]
    fn test_schema_format_as_str_avro() {
        assert_eq!(SchemaFormat::Avro.as_str(), "AVRO");
    }

    #[test]
    fn test_schema_format_as_str_protobuf() {
        assert_eq!(SchemaFormat::Protobuf.as_str(), "PROTOBUF");
    }

    #[test]
    fn test_schema_format_as_str_json() {
        assert_eq!(SchemaFormat::Json.as_str(), "JSON");
    }

    #[test]
    fn test_schema_format_equality() {
        assert_eq!(SchemaFormat::Avro, SchemaFormat::Avro);
        assert_eq!(SchemaFormat::Protobuf, SchemaFormat::Protobuf);
        assert_eq!(SchemaFormat::Json, SchemaFormat::Json);
        assert_ne!(SchemaFormat::Avro, SchemaFormat::Protobuf);
        assert_ne!(SchemaFormat::Avro, SchemaFormat::Json);
        assert_ne!(SchemaFormat::Protobuf, SchemaFormat::Json);
    }

    #[test]
    fn test_schema_format_clone() {
        let format = SchemaFormat::Avro;
        let cloned = format.clone();
        assert_eq!(format, cloned);
    }

    #[test]
    fn test_schema_format_copy() {
        let format = SchemaFormat::Protobuf;
        let copied = format;
        // Both are still usable because SchemaFormat is Copy
        assert_eq!(format, copied);
    }

    #[test]
    fn test_schema_format_debug() {
        assert_eq!(format!("{:?}", SchemaFormat::Avro), "Avro");
        assert_eq!(format!("{:?}", SchemaFormat::Protobuf), "Protobuf");
        assert_eq!(format!("{:?}", SchemaFormat::Json), "Json");
    }

    #[test]
    fn test_schema_format_serialize_avro() {
        let json = serde_json::to_string(&SchemaFormat::Avro).unwrap();
        assert_eq!(json, r#""AVRO""#);
    }

    #[test]
    fn test_schema_format_serialize_protobuf() {
        let json = serde_json::to_string(&SchemaFormat::Protobuf).unwrap();
        assert_eq!(json, r#""PROTOBUF""#);
    }

    #[test]
    fn test_schema_format_serialize_json() {
        let json = serde_json::to_string(&SchemaFormat::Json).unwrap();
        assert_eq!(json, r#""JSON""#);
    }

    #[test]
    fn test_schema_format_deserialize_avro() {
        let format: SchemaFormat = serde_json::from_str(r#""AVRO""#).unwrap();
        assert_eq!(format, SchemaFormat::Avro);
    }

    #[test]
    fn test_schema_format_deserialize_protobuf() {
        let format: SchemaFormat = serde_json::from_str(r#""PROTOBUF""#).unwrap();
        assert_eq!(format, SchemaFormat::Protobuf);
    }

    #[test]
    fn test_schema_format_deserialize_json() {
        let format: SchemaFormat = serde_json::from_str(r#""JSON""#).unwrap();
        assert_eq!(format, SchemaFormat::Json);
    }

    #[test]
    fn test_schema_format_deserialize_invalid() {
        let result: Result<SchemaFormat, _> = serde_json::from_str(r#""YAML""#);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_format_serde_roundtrip() {
        for format in [SchemaFormat::Avro, SchemaFormat::Protobuf, SchemaFormat::Json] {
            let json = serde_json::to_string(&format).unwrap();
            let deserialized: SchemaFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(format, deserialized);
        }
    }

    // ========================================================================
    // CompatibilityMode tests
    // ========================================================================

    #[test]
    fn test_compatibility_mode_default() {
        let mode: CompatibilityMode = Default::default();
        assert_eq!(mode, CompatibilityMode::Backward);
    }

    #[test]
    fn test_compatibility_mode_equality() {
        assert_eq!(CompatibilityMode::Backward, CompatibilityMode::Backward);
        assert_eq!(CompatibilityMode::Forward, CompatibilityMode::Forward);
        assert_eq!(CompatibilityMode::Full, CompatibilityMode::Full);
        assert_eq!(CompatibilityMode::None, CompatibilityMode::None);
        assert_ne!(CompatibilityMode::Backward, CompatibilityMode::Forward);
        assert_ne!(CompatibilityMode::Full, CompatibilityMode::None);
    }

    #[test]
    fn test_compatibility_mode_clone() {
        let mode = CompatibilityMode::FullTransitive;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_compatibility_mode_serialize_all_variants() {
        let expected = vec![
            (CompatibilityMode::Backward, r#""BACKWARD""#),
            (CompatibilityMode::Forward, r#""FORWARD""#),
            (CompatibilityMode::Full, r#""FULL""#),
            (CompatibilityMode::BackwardTransitive, r#""BACKWARD_TRANSITIVE""#),
            (CompatibilityMode::ForwardTransitive, r#""FORWARD_TRANSITIVE""#),
            (CompatibilityMode::FullTransitive, r#""FULL_TRANSITIVE""#),
            (CompatibilityMode::None, r#""NONE""#),
        ];
        for (mode, expected_json) in expected {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", mode);
        }
    }

    #[test]
    fn test_compatibility_mode_deserialize_all_variants() {
        let cases = vec![
            (r#""BACKWARD""#, CompatibilityMode::Backward),
            (r#""FORWARD""#, CompatibilityMode::Forward),
            (r#""FULL""#, CompatibilityMode::Full),
            (r#""BACKWARD_TRANSITIVE""#, CompatibilityMode::BackwardTransitive),
            (r#""FORWARD_TRANSITIVE""#, CompatibilityMode::ForwardTransitive),
            (r#""FULL_TRANSITIVE""#, CompatibilityMode::FullTransitive),
            (r#""NONE""#, CompatibilityMode::None),
        ];
        for (json, expected_mode) in cases {
            let mode: CompatibilityMode = serde_json::from_str(json).unwrap();
            assert_eq!(mode, expected_mode, "Failed for {}", json);
        }
    }

    #[test]
    fn test_compatibility_mode_deserialize_invalid() {
        let result: Result<CompatibilityMode, _> = serde_json::from_str(r#""UNKNOWN""#);
        assert!(result.is_err());
    }

    #[test]
    fn test_compatibility_mode_serde_roundtrip() {
        let modes = vec![
            CompatibilityMode::Backward,
            CompatibilityMode::Forward,
            CompatibilityMode::Full,
            CompatibilityMode::BackwardTransitive,
            CompatibilityMode::ForwardTransitive,
            CompatibilityMode::FullTransitive,
            CompatibilityMode::None,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: CompatibilityMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }

    // ========================================================================
    // SchemaMetadata tests
    // ========================================================================

    #[test]
    fn test_schema_metadata_default() {
        let meta = SchemaMetadata::default();
        assert!(meta.properties.is_empty());
        assert!(meta.tags.is_empty());
        assert!(meta.ruleset.is_none());
    }

    #[test]
    fn test_schema_metadata_serde_roundtrip() {
        let mut props = HashMap::new();
        props.insert("owner".to_string(), "team-a".to_string());
        props.insert("env".to_string(), "production".to_string());

        let meta = SchemaMetadata {
            properties: props,
            tags: vec!["important".to_string(), "v2".to_string()],
            ruleset: Some("rule1".to_string()),
        };

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: SchemaMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.properties.get("owner").unwrap(), "team-a");
        assert_eq!(deserialized.properties.get("env").unwrap(), "production");
        assert_eq!(deserialized.tags.len(), 2);
        assert!(deserialized.tags.contains(&"important".to_string()));
        assert!(deserialized.tags.contains(&"v2".to_string()));
        assert_eq!(deserialized.ruleset, Some("rule1".to_string()));
    }

    #[test]
    fn test_schema_metadata_ruleset_none_omitted() {
        let meta = SchemaMetadata {
            properties: HashMap::new(),
            tags: vec![],
            ruleset: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("ruleset"), "ruleset=None should be omitted from serialization");
    }

    #[test]
    fn test_schema_metadata_deserialize_missing_optional_fields() {
        let json = r#"{"properties": {}}"#;
        let meta: SchemaMetadata = serde_json::from_str(json).unwrap();
        assert!(meta.tags.is_empty());
        assert!(meta.ruleset.is_none());
    }

    // ========================================================================
    // SchemaReference tests
    // ========================================================================

    #[test]
    fn test_schema_reference_serde_roundtrip() {
        let reference = SchemaReference {
            name: "Address".to_string(),
            subject: "address-value".to_string(),
            version: 3,
        };

        let json = serde_json::to_string(&reference).unwrap();
        let deserialized: SchemaReference = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "Address");
        assert_eq!(deserialized.subject, "address-value");
        assert_eq!(deserialized.version, 3);
    }

    // ========================================================================
    // Schema tests
    // ========================================================================

    #[test]
    fn test_schema_serde_roundtrip() {
        let schema = Schema {
            id: 42,
            subject: "user-value".to_string(),
            version: 5,
            schema_type: SchemaFormat::Avro,
            schema: r#"{"type":"record","name":"User","fields":[]}"#.to_string(),
            references: vec![
                SchemaReference {
                    name: "Address".to_string(),
                    subject: "address-value".to_string(),
                    version: 1,
                },
            ],
            metadata: SchemaMetadata {
                properties: {
                    let mut m = HashMap::new();
                    m.insert("owner".to_string(), "team-a".to_string());
                    m
                },
                tags: vec!["important".to_string()],
                ruleset: None,
            },
        };

        let json = serde_json::to_string(&schema).unwrap();
        let deserialized: Schema = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, 42);
        assert_eq!(deserialized.subject, "user-value");
        assert_eq!(deserialized.version, 5);
        assert_eq!(deserialized.schema_type, SchemaFormat::Avro);
        assert_eq!(deserialized.references.len(), 1);
        assert_eq!(deserialized.references[0].name, "Address");
    }

    #[test]
    fn test_schema_deserialize_defaults_for_references_and_metadata() {
        let json = r#"{
            "id": 1,
            "subject": "test",
            "version": 1,
            "schema_type": "AVRO",
            "schema": "{}"
        }"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        assert!(schema.references.is_empty());
        assert!(schema.metadata.properties.is_empty());
        assert!(schema.metadata.tags.is_empty());
    }

    // ========================================================================
    // SubjectConfig tests
    // ========================================================================

    #[test]
    fn test_subject_config_serde_roundtrip() {
        let config = SubjectConfig {
            subject: "orders-value".to_string(),
            compatibility: CompatibilityMode::Full,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SubjectConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.subject, "orders-value");
        assert_eq!(deserialized.compatibility, CompatibilityMode::Full);
    }

    // ========================================================================
    // RegisterSchemaRequest tests
    // ========================================================================

    #[test]
    fn test_register_schema_request_serde_roundtrip() {
        let request = RegisterSchemaRequest {
            schema: r#"{"type":"string"}"#.to_string(),
            schema_type: Some(SchemaFormat::Json),
            references: vec![],
            metadata: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RegisterSchemaRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.schema, r#"{"type":"string"}"#);
        assert_eq!(deserialized.schema_type, Some(SchemaFormat::Json));
        assert!(deserialized.references.is_empty());
        assert!(deserialized.metadata.is_none());
    }

    #[test]
    fn test_register_schema_request_schema_type_rename() {
        let json = r#"{"schema":"{}", "schemaType": "AVRO", "references": []}"#;
        let request: RegisterSchemaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.schema_type, Some(SchemaFormat::Avro));
    }

    #[test]
    fn test_register_schema_request_optional_fields_absent() {
        let json = r#"{"schema":"{}"}"#;
        let request: RegisterSchemaRequest = serde_json::from_str(json).unwrap();
        assert!(request.schema_type.is_none());
        assert!(request.references.is_empty());
        assert!(request.metadata.is_none());
    }

    #[test]
    fn test_register_schema_request_omit_none_schema_type() {
        let request = RegisterSchemaRequest {
            schema: "{}".to_string(),
            schema_type: None,
            references: vec![],
            metadata: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("schemaType"), "None schema_type should be omitted");
    }

    // ========================================================================
    // RegisterSchemaResponse tests
    // ========================================================================

    #[test]
    fn test_register_schema_response_serde() {
        let response = RegisterSchemaResponse { id: 99 };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"id":99}"#);

        let deserialized: RegisterSchemaResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, 99);
    }

    // ========================================================================
    // SchemaResponse tests
    // ========================================================================

    #[test]
    fn test_schema_response_serde_roundtrip() {
        let response = SchemaResponse {
            subject: "user-value".to_string(),
            version: 2,
            id: 10,
            schema: r#"{"type":"string"}"#.to_string(),
            schema_type: SchemaFormat::Protobuf,
            references: vec![],
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: SchemaResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.subject, "user-value");
        assert_eq!(deserialized.version, 2);
        assert_eq!(deserialized.id, 10);
        assert_eq!(deserialized.schema_type, SchemaFormat::Protobuf);
    }

    #[test]
    fn test_schema_response_schema_type_rename() {
        let json = r#"{
            "subject": "test",
            "version": 1,
            "id": 1,
            "schema": "{}",
            "schemaType": "JSON",
            "references": []
        }"#;
        let response: SchemaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.schema_type, SchemaFormat::Json);
    }

    // ========================================================================
    // VersionsResponse tests
    // ========================================================================

    #[test]
    fn test_versions_response_serde() {
        let response = VersionsResponse {
            versions: vec![1, 2, 3, 4],
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: VersionsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.versions, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_versions_response_empty() {
        let response = VersionsResponse { versions: vec![] };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: VersionsResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.versions.is_empty());
    }
}
