//! Schema Compatibility Checking
//!
//! Validates that schema evolution follows compatibility rules.

use crate::{
    error::{Result, SchemaError},
    types::{CompatibilityMode, Schema, SchemaFormat},
};

/// Check if a new schema is compatible with an existing schema
pub fn check_compatibility(
    existing: &Schema,
    new_schema: &str,
    new_format: SchemaFormat,
    mode: CompatibilityMode,
) -> Result<bool> {
    if existing.schema_type != new_format {
        return Err(SchemaError::IncompatibleSchema(format!(
            "Schema format mismatch: expected {:?}, got {:?}",
            existing.schema_type, new_format
        )));
    }

    match existing.schema_type {
        SchemaFormat::Avro => check_avro_compatibility(existing, new_schema, mode),
        SchemaFormat::Protobuf => check_protobuf_compatibility(existing, new_schema, mode),
        SchemaFormat::Json => check_json_compatibility(existing, new_schema, mode),
    }
}

/// Check Avro schema compatibility
fn check_avro_compatibility(
    existing: &Schema,
    new_schema: &str,
    mode: CompatibilityMode,
) -> Result<bool> {
    use apache_avro::Schema as AvroSchema;

    let existing_schema = AvroSchema::parse_str(&existing.schema)
        .map_err(|e| SchemaError::InvalidSchema(format!("Invalid existing Avro schema: {}", e)))?;

    let new_schema = AvroSchema::parse_str(new_schema)
        .map_err(|e| SchemaError::InvalidSchema(format!("Invalid new Avro schema: {}", e)))?;

    match mode {
        CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
            // New schema can read data written with old schema
            // This means: new schema >= old schema (new schema has all fields from old)
            check_avro_backward_compatible(&existing_schema, &new_schema)
        }
        CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
            // Old schema can read data written with new schema
            // This means: old schema >= new schema (old schema has all fields from new)
            check_avro_backward_compatible(&new_schema, &existing_schema)
        }
        CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
            // Both backward and forward compatible
            let backward = check_avro_backward_compatible(&existing_schema, &new_schema)?;
            let forward = check_avro_backward_compatible(&new_schema, &existing_schema)?;
            Ok(backward && forward)
        }
        CompatibilityMode::None => Ok(true),
    }
}

/// Check if new Avro schema is backward compatible with existing schema
fn check_avro_backward_compatible(
    reader_schema: &apache_avro::Schema,
    writer_schema: &apache_avro::Schema,
) -> Result<bool> {
    // Simplified compatibility check
    // In production, use apache_avro::Schema::compatible or similar

    match (reader_schema, writer_schema) {
        // Same schema is always compatible
        (r, w) if r == w => Ok(true),

        // Record compatibility
        (apache_avro::Schema::Record(r), apache_avro::Schema::Record(w)) => {
            if r.name != w.name {
                return Ok(false);
            }

            // Check that all fields in reader exist in writer or have defaults
            for reader_field in &r.fields {
                let writer_field = w.fields.iter().find(|f| f.name == reader_field.name);

                match writer_field {
                    Some(wf) => {
                        // Field exists in both - check type compatibility
                        if !schemas_compatible(&reader_field.schema, &wf.schema) {
                            return Ok(false);
                        }
                    }
                    None => {
                        // Field only in reader - needs default value
                        if reader_field.default.is_none() {
                            return Ok(false);
                        }
                    }
                }
            }
            Ok(true)
        }

        // Primitive type compatibility
        _ => Ok(schemas_compatible(reader_schema, writer_schema)),
    }
}

fn schemas_compatible(schema1: &apache_avro::Schema, schema2: &apache_avro::Schema) -> bool {
    // Simplified type compatibility check
    match (schema1, schema2) {
        (apache_avro::Schema::String, apache_avro::Schema::String) => true,
        (apache_avro::Schema::Int, apache_avro::Schema::Int) => true,
        (apache_avro::Schema::Long, apache_avro::Schema::Long) => true,
        (apache_avro::Schema::Float, apache_avro::Schema::Float) => true,
        (apache_avro::Schema::Double, apache_avro::Schema::Double) => true,
        (apache_avro::Schema::Boolean, apache_avro::Schema::Boolean) => true,
        (apache_avro::Schema::Bytes, apache_avro::Schema::Bytes) => true,

        // Promotion rules
        (apache_avro::Schema::Long, apache_avro::Schema::Int) => true,
        (apache_avro::Schema::Double, apache_avro::Schema::Float) => true,

        _ => false,
    }
}

/// Check Protobuf schema compatibility
fn check_protobuf_compatibility(
    _existing: &Schema,
    _new_schema: &str,
    mode: CompatibilityMode,
) -> Result<bool> {
    // Placeholder: Protobuf compatibility checking
    // In production, use protobuf descriptor parsing and field number checking

    match mode {
        CompatibilityMode::None => Ok(true),
        _ => {
            tracing::warn!("Protobuf compatibility checking not yet implemented");
            Ok(true) // Allow for now
        }
    }
}

/// Check JSON Schema compatibility
fn check_json_compatibility(
    existing: &Schema,
    new_schema: &str,
    mode: CompatibilityMode,
) -> Result<bool> {
    // Parse both schemas as JSON
    let existing_json: serde_json::Value = serde_json::from_str(&existing.schema)
        .map_err(|e| SchemaError::InvalidSchema(format!("Invalid existing JSON schema: {}", e)))?;

    let new_json: serde_json::Value = serde_json::from_str(new_schema)
        .map_err(|e| SchemaError::InvalidSchema(format!("Invalid new JSON schema: {}", e)))?;

    match mode {
        CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
            // New schema can validate data that old schema validated
            check_json_backward_compatible(&existing_json, &new_json)
        }
        CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
            // Old schema can validate data that new schema validates
            check_json_backward_compatible(&new_json, &existing_json)
        }
        CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
            let backward = check_json_backward_compatible(&existing_json, &new_json)?;
            let forward = check_json_backward_compatible(&new_json, &existing_json)?;
            Ok(backward && forward)
        }
        CompatibilityMode::None => Ok(true),
    }
}

fn check_json_backward_compatible(
    _old_schema: &serde_json::Value,
    _new_schema: &serde_json::Value,
) -> Result<bool> {
    // Simplified JSON Schema compatibility
    // In production, use jsonschema crate for proper validation

    tracing::warn!("JSON Schema compatibility checking simplified");
    Ok(true) // Allow for now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avro_same_schema_compatible() {
        let schema_str = r#"{"type": "record", "name": "User", "fields": [{"name": "name", "type": "string"}]}"#;

        let existing = Schema {
            id: 1,
            subject: "test".to_string(),
            version: 1,
            schema_type: SchemaFormat::Avro,
            schema: schema_str.to_string(),
            references: vec![],
            metadata: Default::default(),
        };

        let result = check_compatibility(
            &existing,
            schema_str,
            SchemaFormat::Avro,
            CompatibilityMode::Backward,
        );

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_avro_backward_compatible_new_optional_field() {
        let old_schema = r#"{"type": "record", "name": "User", "fields": [{"name": "name", "type": "string"}]}"#;
        let new_schema = r#"{"type": "record", "name": "User", "fields": [{"name": "name", "type": "string"}, {"name": "age", "type": "int", "default": 0}]}"#;

        let existing = Schema {
            id: 1,
            subject: "test".to_string(),
            version: 1,
            schema_type: SchemaFormat::Avro,
            schema: old_schema.to_string(),
            references: vec![],
            metadata: Default::default(),
        };

        let result = check_compatibility(
            &existing,
            new_schema,
            SchemaFormat::Avro,
            CompatibilityMode::Backward,
        );

        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
