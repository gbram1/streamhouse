//! Record Data Structure
//!
//! This module defines the core `Record` type - the fundamental unit of data in StreamHouse.
//!
//! ## What is a Record?
//! A record is a single message/event in a stream, similar to:
//! - A Kafka message
//! - A log entry
//! - An event in an event stream
//!
//! ## Structure
//! Each record contains:
//! - **offset**: Unique, monotonically increasing ID within a partition (like Kafka)
//! - **timestamp**: When the record was created (milliseconds since epoch)
//! - **key**: Optional identifier for partitioning/grouping (e.g., user_id)
//! - **value**: The actual payload/data (arbitrary bytes)
//!
//! ## Design Decisions
//! - Uses `bytes::Bytes` for zero-copy operations (no allocations when slicing)
//! - Implements `Serialize`/`Deserialize` for metadata storage
//! - Key is optional because not all use cases need keys
//! - Offset is u64 to support very large streams (18 quintillion records)
//!
//! ## Example
//! ```ignore
//! let record = Record::new(
//!     100,                              // offset
//!     1234567890000,                    // timestamp
//!     Some(Bytes::from("user123")),     // key
//!     Bytes::from(r#"{"action": "click"}"#),  // value (JSON)
//! );
//! ```

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// A single record in the stream
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Offset of this record in the partition
    pub offset: u64,

    /// Timestamp in milliseconds since epoch
    pub timestamp: u64,

    /// Optional key
    pub key: Option<Bytes>,

    /// Value (payload)
    pub value: Bytes,
}

impl Record {
    pub fn new(offset: u64, timestamp: u64, key: Option<Bytes>, value: Bytes) -> Self {
        Self {
            offset,
            timestamp,
            key,
            value,
        }
    }

    /// Estimate the size of this record in bytes
    pub fn estimated_size(&self) -> usize {
        8 + // offset
        8 + // timestamp
        self.key.as_ref().map(|k| k.len()).unwrap_or(0) +
        self.value.len()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Construction
    // ---------------------------------------------------------------

    #[test]
    fn test_new_with_key() {
        let rec = Record::new(
            42,
            1_700_000_000_000,
            Some(Bytes::from("my-key")),
            Bytes::from("hello world"),
        );
        assert_eq!(rec.offset, 42);
        assert_eq!(rec.timestamp, 1_700_000_000_000);
        assert_eq!(rec.key, Some(Bytes::from("my-key")));
        assert_eq!(rec.value, Bytes::from("hello world"));
    }

    #[test]
    fn test_new_without_key() {
        let rec = Record::new(0, 0, None, Bytes::from("payload"));
        assert_eq!(rec.offset, 0);
        assert_eq!(rec.timestamp, 0);
        assert!(rec.key.is_none());
        assert_eq!(rec.value, Bytes::from("payload"));
    }

    #[test]
    fn test_new_with_empty_value() {
        let rec = Record::new(1, 1, None, Bytes::new());
        assert!(rec.value.is_empty());
    }

    #[test]
    fn test_new_with_empty_key() {
        let rec = Record::new(1, 1, Some(Bytes::new()), Bytes::from("v"));
        assert_eq!(rec.key, Some(Bytes::new()));
    }

    #[test]
    fn test_new_max_offset_and_timestamp() {
        let rec = Record::new(u64::MAX, u64::MAX, None, Bytes::from("x"));
        assert_eq!(rec.offset, u64::MAX);
        assert_eq!(rec.timestamp, u64::MAX);
    }

    // ---------------------------------------------------------------
    // estimated_size
    // ---------------------------------------------------------------

    #[test]
    fn test_estimated_size_no_key() {
        let rec = Record::new(0, 0, None, Bytes::from("12345"));
        // 8 (offset) + 8 (timestamp) + 0 (no key) + 5 (value) = 21
        assert_eq!(rec.estimated_size(), 21);
    }

    #[test]
    fn test_estimated_size_with_key() {
        let rec = Record::new(0, 0, Some(Bytes::from("abc")), Bytes::from("12345"));
        // 8 + 8 + 3 + 5 = 24
        assert_eq!(rec.estimated_size(), 24);
    }

    #[test]
    fn test_estimated_size_empty_value_no_key() {
        let rec = Record::new(0, 0, None, Bytes::new());
        // 8 + 8 + 0 + 0 = 16
        assert_eq!(rec.estimated_size(), 16);
    }

    #[test]
    fn test_estimated_size_empty_key_and_value() {
        let rec = Record::new(0, 0, Some(Bytes::new()), Bytes::new());
        // 8 + 8 + 0 + 0 = 16
        assert_eq!(rec.estimated_size(), 16);
    }

    #[test]
    fn test_estimated_size_large_payload() {
        let big = Bytes::from(vec![0u8; 1_000_000]);
        let rec = Record::new(0, 0, None, big);
        assert_eq!(rec.estimated_size(), 16 + 1_000_000);
    }

    #[test]
    fn test_estimated_size_large_key_and_value() {
        let key = Bytes::from(vec![0u8; 500]);
        let val = Bytes::from(vec![0u8; 1000]);
        let rec = Record::new(0, 0, Some(key), val);
        assert_eq!(rec.estimated_size(), 16 + 500 + 1000);
    }

    // ---------------------------------------------------------------
    // Clone
    // ---------------------------------------------------------------

    #[test]
    fn test_clone() {
        let rec = Record::new(7, 99, Some(Bytes::from("k")), Bytes::from("v"));
        let cloned = rec.clone();
        assert_eq!(rec, cloned);
    }

    #[test]
    fn test_clone_independence() {
        let rec = Record::new(1, 2, Some(Bytes::from("key")), Bytes::from("val"));
        let cloned = rec.clone();
        assert_eq!(rec.offset, cloned.offset);
        assert_eq!(rec.timestamp, cloned.timestamp);
        assert_eq!(rec.key, cloned.key);
        assert_eq!(rec.value, cloned.value);
    }

    // ---------------------------------------------------------------
    // PartialEq / Eq
    // ---------------------------------------------------------------

    #[test]
    fn test_eq_identical() {
        let a = Record::new(1, 2, Some(Bytes::from("k")), Bytes::from("v"));
        let b = Record::new(1, 2, Some(Bytes::from("k")), Bytes::from("v"));
        assert_eq!(a, b);
    }

    #[test]
    fn test_ne_different_offset() {
        let a = Record::new(1, 2, None, Bytes::from("v"));
        let b = Record::new(2, 2, None, Bytes::from("v"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_ne_different_timestamp() {
        let a = Record::new(1, 1, None, Bytes::from("v"));
        let b = Record::new(1, 2, None, Bytes::from("v"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_ne_different_key() {
        let a = Record::new(1, 1, Some(Bytes::from("a")), Bytes::from("v"));
        let b = Record::new(1, 1, Some(Bytes::from("b")), Bytes::from("v"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_ne_key_some_vs_none() {
        let a = Record::new(1, 1, Some(Bytes::from("a")), Bytes::from("v"));
        let b = Record::new(1, 1, None, Bytes::from("v"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_ne_different_value() {
        let a = Record::new(1, 1, None, Bytes::from("x"));
        let b = Record::new(1, 1, None, Bytes::from("y"));
        assert_ne!(a, b);
    }

    // ---------------------------------------------------------------
    // Serde round-trip (JSON)
    // ---------------------------------------------------------------

    #[test]
    fn test_serde_roundtrip_with_key() {
        let rec = Record::new(
            100,
            1_700_000_000_000,
            Some(Bytes::from("user-123")),
            Bytes::from(r#"{"action":"click"}"#),
        );
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_without_key() {
        let rec = Record::new(0, 0, None, Bytes::from("data"));
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_empty_value() {
        let rec = Record::new(1, 1, None, Bytes::new());
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_binary_value() {
        let binary_data = Bytes::from(vec![0u8, 1, 2, 255, 254, 253]);
        let rec = Record::new(10, 20, None, binary_data);
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_max_values() {
        let rec = Record::new(u64::MAX, u64::MAX, Some(Bytes::from("k")), Bytes::from("v"));
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_json_structure() {
        let rec = Record::new(42, 100, None, Bytes::from("hi"));
        let val: serde_json::Value = serde_json::to_value(&rec).expect("to_value");
        assert_eq!(val["offset"], 42);
        assert_eq!(val["timestamp"], 100);
        assert!(val["key"].is_null());
    }

    #[test]
    fn test_serde_json_key_present_in_output() {
        let rec = Record::new(1, 2, Some(Bytes::from("mykey")), Bytes::from("v"));
        let val: serde_json::Value = serde_json::to_value(&rec).expect("to_value");
        assert!(!val["key"].is_null());
    }

    // ---------------------------------------------------------------
    // Debug
    // ---------------------------------------------------------------

    #[test]
    fn test_debug_impl() {
        let rec = Record::new(1, 2, None, Bytes::from("v"));
        let debug = format!("{:?}", rec);
        assert!(debug.contains("Record"));
        assert!(debug.contains("offset"));
        assert!(debug.contains("timestamp"));
    }

    // ---------------------------------------------------------------
    // Construction - additional edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_new_with_large_binary_key() {
        let key = Bytes::from(vec![0xFFu8; 4096]);
        let rec = Record::new(1, 1, Some(key.clone()), Bytes::from("v"));
        assert_eq!(rec.key.unwrap().len(), 4096);
    }

    #[test]
    fn test_new_preserves_binary_value() {
        let value = Bytes::from(vec![0u8, 1, 2, 127, 128, 255]);
        let rec = Record::new(0, 0, None, value.clone());
        assert_eq!(rec.value, value);
    }

    #[test]
    fn test_new_with_zero_offset_nonzero_timestamp() {
        let rec = Record::new(0, 1_700_000_000_000, None, Bytes::from("v"));
        assert_eq!(rec.offset, 0);
        assert_eq!(rec.timestamp, 1_700_000_000_000);
    }

    #[test]
    fn test_new_unicode_key_and_value() {
        let rec = Record::new(
            1,
            2,
            Some(Bytes::from("cle-unicode-\u{1F600}")),
            Bytes::from("\u{00E9}\u{00E8}\u{00EA}"),
        );
        assert!(rec.key.is_some());
        assert!(!rec.value.is_empty());
    }

    // ---------------------------------------------------------------
    // estimated_size - additional edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_estimated_size_with_one_byte_key_and_value() {
        let rec = Record::new(0, 0, Some(Bytes::from("a")), Bytes::from("b"));
        // 8 + 8 + 1 + 1 = 18
        assert_eq!(rec.estimated_size(), 18);
    }

    #[test]
    fn test_estimated_size_consistency_across_clones() {
        let rec = Record::new(10, 20, Some(Bytes::from("key")), Bytes::from("value"));
        let cloned = rec.clone();
        assert_eq!(rec.estimated_size(), cloned.estimated_size());
    }

    #[test]
    fn test_estimated_size_does_not_count_metadata_overhead() {
        // estimated_size only counts offset (8) + timestamp (8) + key len + value len
        // There is no overhead for the Option wrapper or Bytes metadata
        let rec_with_none = Record::new(0, 0, None, Bytes::from("abc"));
        let rec_with_empty = Record::new(0, 0, Some(Bytes::new()), Bytes::from("abc"));
        // Both should be 8 + 8 + 0 + 3 = 19
        assert_eq!(rec_with_none.estimated_size(), 19);
        assert_eq!(rec_with_empty.estimated_size(), 19);
    }

    // ---------------------------------------------------------------
    // Clone - deeper checks
    // ---------------------------------------------------------------

    #[test]
    fn test_clone_with_none_key() {
        let rec = Record::new(0, 0, None, Bytes::from("data"));
        let cloned = rec.clone();
        assert_eq!(rec, cloned);
        assert!(cloned.key.is_none());
    }

    #[test]
    fn test_clone_preserves_large_value() {
        let value = Bytes::from(vec![42u8; 100_000]);
        let rec = Record::new(0, 0, None, value.clone());
        let cloned = rec.clone();
        assert_eq!(cloned.value.len(), 100_000);
        assert_eq!(rec.value, cloned.value);
    }

    // ---------------------------------------------------------------
    // PartialEq / Eq - additional edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_eq_both_empty() {
        let a = Record::new(0, 0, None, Bytes::new());
        let b = Record::new(0, 0, None, Bytes::new());
        assert_eq!(a, b);
    }

    #[test]
    fn test_eq_both_with_empty_key() {
        let a = Record::new(1, 1, Some(Bytes::new()), Bytes::from("v"));
        let b = Record::new(1, 1, Some(Bytes::new()), Bytes::from("v"));
        assert_eq!(a, b);
    }

    #[test]
    fn test_ne_empty_key_vs_none_key() {
        // Some(empty) != None
        let a = Record::new(1, 1, Some(Bytes::new()), Bytes::from("v"));
        let b = Record::new(1, 1, None, Bytes::from("v"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_eq_reflexive() {
        let rec = Record::new(42, 99, Some(Bytes::from("k")), Bytes::from("v"));
        assert_eq!(rec, rec);
    }

    #[test]
    fn test_eq_symmetric() {
        let a = Record::new(10, 20, Some(Bytes::from("k")), Bytes::from("v"));
        let b = Record::new(10, 20, Some(Bytes::from("k")), Bytes::from("v"));
        assert_eq!(a, b);
        assert_eq!(b, a);
    }

    // ---------------------------------------------------------------
    // Serde round-trip (JSON) - additional edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_serde_roundtrip_with_empty_key() {
        let rec = Record::new(1, 2, Some(Bytes::new()), Bytes::from("val"));
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_large_payload() {
        let large_value = Bytes::from(vec![0xABu8; 10_000]);
        let rec = Record::new(999, 888, None, large_value);
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_zero_fields() {
        let rec = Record::new(0, 0, None, Bytes::new());
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_json_key_null_when_none() {
        let rec = Record::new(1, 2, None, Bytes::from("v"));
        let val: serde_json::Value = serde_json::to_value(&rec).expect("to_value");
        assert!(val["key"].is_null());
    }

    #[test]
    fn test_serde_json_key_not_null_when_some_empty() {
        let rec = Record::new(1, 2, Some(Bytes::new()), Bytes::from("v"));
        let val: serde_json::Value = serde_json::to_value(&rec).expect("to_value");
        assert!(!val["key"].is_null());
    }

    #[test]
    fn test_serde_pretty_print_roundtrip() {
        let rec = Record::new(42, 100, Some(Bytes::from("key")), Bytes::from("value"));
        let pretty = serde_json::to_string_pretty(&rec).expect("pretty");
        let deserialized: Record = serde_json::from_str(&pretty).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    #[test]
    fn test_serde_deserialize_from_manual_json() {
        // Bytes serializes as a byte array in serde_json
        let rec = Record::new(5, 10, None, Bytes::from("hi"));
        let json = serde_json::to_string(&rec).expect("serialize");
        // Deserialize the same JSON string and verify it round-trips
        let deserialized: Record = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.offset, 5);
        assert_eq!(deserialized.timestamp, 10);
        assert!(deserialized.key.is_none());
        assert_eq!(deserialized.value, Bytes::from("hi"));
    }

    #[test]
    fn test_serde_multiple_records_in_vec() {
        let records = vec![
            Record::new(0, 100, None, Bytes::from("first")),
            Record::new(1, 200, Some(Bytes::from("k")), Bytes::from("second")),
            Record::new(2, 300, None, Bytes::new()),
        ];
        let json = serde_json::to_string(&records).expect("serialize vec");
        let deserialized: Vec<Record> = serde_json::from_str(&json).expect("deserialize vec");
        assert_eq!(records, deserialized);
    }

    // ---------------------------------------------------------------
    // Debug - additional checks
    // ---------------------------------------------------------------

    #[test]
    fn test_debug_contains_key_value() {
        let rec = Record::new(1, 2, Some(Bytes::from("mykey")), Bytes::from("myval"));
        let debug = format!("{:?}", rec);
        assert!(debug.contains("key"));
        assert!(debug.contains("value"));
    }

    #[test]
    fn test_debug_none_key_shows_none() {
        let rec = Record::new(1, 2, None, Bytes::from("v"));
        let debug = format!("{:?}", rec);
        assert!(debug.contains("None"));
    }
}
