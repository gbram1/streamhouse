//! Schema-aware Serialization and Deserialization
//!
//! Utilities for encoding/decoding messages with embedded schema IDs.

use crate::{error::SchemaError, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Magic byte indicating schema ID is present
const MAGIC_BYTE: u8 = 0x00;

/// Serialize data with schema ID
///
/// Format: [magic_byte(1)][schema_id(4)][data(N)]
pub fn serialize_with_schema_id(schema_id: i32, data: &[u8]) -> Bytes {
    let mut buf = BytesMut::with_capacity(1 + 4 + data.len());

    // Magic byte
    buf.put_u8(MAGIC_BYTE);

    // Schema ID (big-endian for compatibility with Confluent)
    buf.put_i32(schema_id);

    // Actual data
    buf.put_slice(data);

    buf.freeze()
}

/// Deserialize and extract schema ID from data
///
/// Returns (schema_id, data_without_header)
pub fn deserialize_with_schema_id(data: &[u8]) -> Result<(i32, &[u8])> {
    if data.len() < 5 {
        return Err(SchemaError::DeserializationError(
            "Data too short to contain schema ID".to_string(),
        ));
    }

    // Check magic byte
    if data[0] != MAGIC_BYTE {
        return Err(SchemaError::DeserializationError(format!(
            "Invalid magic byte: expected 0x00, got 0x{:02x}",
            data[0]
        )));
    }

    // Extract schema ID (big-endian)
    let mut id_bytes = &data[1..5];
    let schema_id = id_bytes.get_i32();

    // Return schema ID and remaining data
    Ok((schema_id, &data[5..]))
}

/// Serialize Avro data with schema
pub fn serialize_avro(
    schema: &apache_avro::Schema,
    value: &apache_avro::types::Value,
) -> Result<Vec<u8>> {
    apache_avro::to_avro_datum(schema, value.clone())
        .map_err(|e| SchemaError::SerializationError(e.to_string()))
}

/// Deserialize Avro data with schema
pub fn deserialize_avro(
    schema: &apache_avro::Schema,
    data: &[u8],
) -> Result<apache_avro::types::Value> {
    apache_avro::from_avro_datum(schema, &mut &data[..], None)
        .map_err(|e| SchemaError::DeserializationError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_schema_id() {
        let schema_id = 123;
        let data = b"hello world";

        // Serialize
        let serialized = serialize_with_schema_id(schema_id, data);

        // Check format
        assert_eq!(serialized[0], MAGIC_BYTE);
        assert_eq!(serialized.len(), 1 + 4 + data.len());

        // Deserialize
        let (extracted_id, extracted_data) = deserialize_with_schema_id(&serialized).unwrap();
        assert_eq!(extracted_id, schema_id);
        assert_eq!(extracted_data, data);
    }

    #[test]
    fn test_deserialize_invalid_magic_byte() {
        let data = vec![0xFF, 0x00, 0x00, 0x00, 0x01, 0x42];
        let result = deserialize_with_schema_id(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_too_short() {
        let data = vec![0x00, 0x01];
        let result = deserialize_with_schema_id(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_avro_serialization() {
        use apache_avro::types::Value;
        use apache_avro::Schema;

        let schema_str = r#"{"type": "string"}"#;
        let schema = Schema::parse_str(schema_str).unwrap();
        let value = Value::String("test".to_string());

        // Serialize
        let serialized = serialize_avro(&schema, &value).unwrap();
        assert!(!serialized.is_empty());

        // Deserialize
        let deserialized = deserialize_avro(&schema, &serialized).unwrap();
        assert_eq!(deserialized, value);
    }
}
