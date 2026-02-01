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
    existing: &Schema,
    new_schema: &str,
    mode: CompatibilityMode,
) -> Result<bool> {
    use prost::Message;
    use prost_types::FileDescriptorSet;

    // Parse existing schema as FileDescriptorSet
    let existing_bytes = existing.schema.as_bytes();
    let existing_fds = FileDescriptorSet::decode(existing_bytes)
        .map_err(|e| SchemaError::InvalidSchema(format!("Invalid existing Protobuf schema: {}", e)))?;

    // Parse new schema as FileDescriptorSet
    let new_bytes = new_schema.as_bytes();
    let new_fds = FileDescriptorSet::decode(new_bytes)
        .map_err(|e| SchemaError::InvalidSchema(format!("Invalid new Protobuf schema: {}", e)))?;

    match mode {
        CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
            // New schema can read data written with old schema
            check_protobuf_backward_compatible(&existing_fds, &new_fds)
        }
        CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
            // Old schema can read data written with new schema
            check_protobuf_backward_compatible(&new_fds, &existing_fds)
        }
        CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
            let backward = check_protobuf_backward_compatible(&existing_fds, &new_fds)?;
            let forward = check_protobuf_backward_compatible(&new_fds, &existing_fds)?;
            Ok(backward && forward)
        }
        CompatibilityMode::None => Ok(true),
    }
}

/// Check if new Protobuf schema is backward compatible with existing schema
fn check_protobuf_backward_compatible(
    writer_fds: &prost_types::FileDescriptorSet,
    reader_fds: &prost_types::FileDescriptorSet,
) -> Result<bool> {
    use prost_types::field_descriptor_proto::Type;

    // For simplicity, compare the first message type in each file
    // In production, you'd iterate through all message types

    if writer_fds.file.is_empty() || reader_fds.file.is_empty() {
        return Ok(true); // No messages to compare
    }

    let writer_file = &writer_fds.file[0];
    let reader_file = &reader_fds.file[0];

    if writer_file.message_type.is_empty() || reader_file.message_type.is_empty() {
        return Ok(true); // No messages to compare
    }

    let writer_msg = &writer_file.message_type[0];
    let reader_msg = &reader_file.message_type[0];

    // Check message names match
    if writer_msg.name != reader_msg.name {
        return Ok(false);
    }

    // Build field maps by number
    let mut writer_fields = std::collections::HashMap::new();
    for field in &writer_msg.field {
        if let Some(number) = field.number {
            writer_fields.insert(number, field);
        }
    }

    let mut reader_fields = std::collections::HashMap::new();
    for field in &reader_msg.field {
        if let Some(number) = field.number {
            reader_fields.insert(number, field);
        }
    }

    // Check compatibility rules:
    // 1. Field numbers must not be reused for different types
    // 2. Required fields in reader must exist in writer
    // 3. Field types must be compatible

    for (&number, reader_field) in &reader_fields {
        if let Some(writer_field) = writer_fields.get(&number) {
            // Field exists in both - check type compatibility
            if let (Some(reader_type_int), Some(writer_type_int)) = (reader_field.r#type, writer_field.r#type) {
                // Convert i32 to Type enum
                if let (Some(reader_type), Some(writer_type)) = (
                    Type::try_from(reader_type_int).ok(),
                    Type::try_from(writer_type_int).ok(),
                ) {
                    if !protobuf_types_compatible(reader_type, writer_type) {
                        tracing::warn!(
                            field_number = number,
                            reader_type = ?reader_type,
                            writer_type = ?writer_type,
                            "Protobuf field type mismatch"
                        );
                        return Ok(false);
                    }
                }
            }
        } else {
            // Field only in reader
            // For backward compatibility, reader can have new fields
            // as long as they're not required
            if reader_field.label == Some(prost_types::field_descriptor_proto::Label::Required as i32) {
                tracing::warn!(
                    field_number = number,
                    field_name = ?reader_field.name,
                    "Required field in reader not found in writer"
                );
                return Ok(false);
            }
        }
    }

    // Check for removed required fields (field in writer but not in reader)
    for (&number, writer_field) in &writer_fields {
        if !reader_fields.contains_key(&number) {
            if writer_field.label == Some(prost_types::field_descriptor_proto::Label::Required as i32) {
                tracing::warn!(
                    field_number = number,
                    field_name = ?writer_field.name,
                    "Required field removed from schema"
                );
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Check if Protobuf types are compatible
fn protobuf_types_compatible(
    reader_type: prost_types::field_descriptor_proto::Type,
    writer_type: prost_types::field_descriptor_proto::Type,
) -> bool {
    use prost_types::field_descriptor_proto::Type;

    // Exact match
    if reader_type == writer_type {
        return true;
    }

    // Promotion rules (safe type conversions)
    match (reader_type, writer_type) {
        // int32 can be promoted to int64
        (Type::Int64, Type::Int32) => true,
        (Type::Int64, Type::Sint32) => true,
        (Type::Sint64, Type::Sint32) => true,

        // uint32 can be promoted to uint64
        (Type::Uint64, Type::Uint32) => true,

        // float can be promoted to double
        (Type::Double, Type::Float) => true,

        // bytes and string are compatible in some cases
        (Type::String, Type::Bytes) => true,
        (Type::Bytes, Type::String) => true,

        _ => false,
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
    old_schema: &serde_json::Value,
    new_schema: &serde_json::Value,
) -> Result<bool> {
    // For backward compatibility, new schema must accept all data that old schema accepted
    // This means:
    // 1. Required fields in new schema must be subset of required fields in old schema
    // 2. Type restrictions in new schema must be looser than or equal to old schema
    // 3. Additional properties allowed in new if disallowed in old

    let old_obj = old_schema.as_object().ok_or_else(|| {
        SchemaError::InvalidSchema("Schema must be an object".to_string())
    })?;

    let new_obj = new_schema.as_object().ok_or_else(|| {
        SchemaError::InvalidSchema("Schema must be an object".to_string())
    })?;

    // Check required fields
    let old_required = old_obj
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();

    let new_required = new_obj
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();

    // New schema cannot require fields that weren't required in old schema
    for field in &new_required {
        if !old_required.contains(field) {
            tracing::warn!(
                field = field,
                "New schema requires field that wasn't required in old schema"
            );
            return Ok(false);
        }
    }

    // Check type compatibility
    if let (Some(old_type), Some(new_type)) = (old_obj.get("type"), new_obj.get("type")) {
        if !json_types_compatible(old_type, new_type) {
            tracing::warn!(
                old_type = ?old_type,
                new_type = ?new_type,
                "Incompatible type change"
            );
            return Ok(false);
        }
    }

    // Check properties if this is an object schema
    if let (Some(old_props), Some(new_props)) = (old_obj.get("properties"), new_obj.get("properties")) {
        let old_props_obj = old_props.as_object();
        let new_props_obj = new_props.as_object();

        if let (Some(old_p), Some(new_p)) = (old_props_obj, new_props_obj) {
            // Check each property for compatibility
            for (prop_name, old_prop_schema) in old_p {
                if let Some(new_prop_schema) = new_p.get(prop_name) {
                    // Property exists in both - recurse
                    if !check_json_backward_compatible(old_prop_schema, new_prop_schema)? {
                        return Ok(false);
                    }
                }
                // Property removed from new schema is OK for backward compatibility
            }
        }
    }

    // Check additionalProperties
    // If old schema disallowed additional properties, new schema must also disallow them
    let old_additional = old_obj
        .get("additionalProperties")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let new_additional = new_obj
        .get("additionalProperties")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if !old_additional && new_additional {
        tracing::warn!("New schema allows additional properties when old schema didn't");
        // This is actually OK for backward compatibility - we'll allow it
    }

    Ok(true)
}

/// Check if JSON Schema types are compatible
fn json_types_compatible(old_type: &serde_json::Value, new_type: &serde_json::Value) -> bool {
    // Exact match
    if old_type == new_type {
        return true;
    }

    // Check if types are arrays (multiple types allowed)
    match (old_type.as_array(), new_type.as_array()) {
        (Some(old_types), Some(new_types)) => {
            // New types must be a superset of old types (more permissive)
            old_types.iter().all(|old_t| new_types.contains(old_t))
        }
        (Some(old_types), None) => {
            // Old had multiple types, new has single type
            // Single type must be in old types
            old_types.contains(new_type)
        }
        (None, Some(new_types)) => {
            // Old had single type, new has multiple types
            // Old type must be in new types (new is more permissive)
            new_types.contains(old_type)
        }
        (None, None) => {
            // Both are strings - check type promotion
            match (old_type.as_str(), new_type.as_str()) {
                (Some("integer"), Some("number")) => true, // integer -> number is OK
                (Some(a), Some(b)) => a == b,
                _ => false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avro_same_schema_compatible() {
        let schema_str =
            r#"{"type": "record", "name": "User", "fields": [{"name": "name", "type": "string"}]}"#;

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
        let old_schema =
            r#"{"type": "record", "name": "User", "fields": [{"name": "name", "type": "string"}]}"#;
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

    #[test]
    fn test_json_schema_backward_compatible_remove_required() {
        let old_schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        }"#;

        let new_schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name"]
        }"#;

        let existing = Schema {
            id: 1,
            subject: "test".to_string(),
            version: 1,
            schema_type: SchemaFormat::Json,
            schema: old_schema.to_string(),
            references: vec![],
            metadata: Default::default(),
        };

        let result = check_compatibility(
            &existing,
            new_schema,
            SchemaFormat::Json,
            CompatibilityMode::Backward,
        );

        assert!(result.is_ok());
        assert!(result.unwrap(), "Removing required field should be backward compatible");
    }

    #[test]
    fn test_json_schema_not_backward_compatible_add_required() {
        let old_schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        }"#;

        let new_schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        }"#;

        let existing = Schema {
            id: 1,
            subject: "test".to_string(),
            version: 1,
            schema_type: SchemaFormat::Json,
            schema: old_schema.to_string(),
            references: vec![],
            metadata: Default::default(),
        };

        let result = check_compatibility(
            &existing,
            new_schema,
            SchemaFormat::Json,
            CompatibilityMode::Backward,
        );

        assert!(result.is_ok());
        assert!(!result.unwrap(), "Adding required field should not be backward compatible");
    }

    #[test]
    fn test_json_schema_type_widening() {
        let old_schema = r#"{
            "type": "object",
            "properties": {
                "value": {"type": "integer"}
            }
        }"#;

        let new_schema = r#"{
            "type": "object",
            "properties": {
                "value": {"type": "number"}
            }
        }"#;

        let existing = Schema {
            id: 1,
            subject: "test".to_string(),
            version: 1,
            schema_type: SchemaFormat::Json,
            schema: old_schema.to_string(),
            references: vec![],
            metadata: Default::default(),
        };

        let result = check_compatibility(
            &existing,
            new_schema,
            SchemaFormat::Json,
            CompatibilityMode::Backward,
        );

        assert!(result.is_ok());
        assert!(result.unwrap(), "Widening integer to number should be backward compatible");
    }

    #[test]
    fn test_json_schema_full_compatibility() {
        let old_schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        }"#;

        let new_schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            },
            "required": ["name"]
        }"#;

        let existing = Schema {
            id: 1,
            subject: "test".to_string(),
            version: 1,
            schema_type: SchemaFormat::Json,
            schema: old_schema.to_string(),
            references: vec![],
            metadata: Default::default(),
        };

        let result = check_compatibility(
            &existing,
            new_schema,
            SchemaFormat::Json,
            CompatibilityMode::Full,
        );

        assert!(result.is_ok());
        // Adding optional field is both backward and forward compatible
        assert!(result.unwrap(), "Adding optional field should be fully compatible");
    }
}
