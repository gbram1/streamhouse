//! Segment Metadata and Configuration
//!
//! This module defines metadata structures for segments - the storage unit in StreamHouse.
//!
//! ## What is a Segment?
//! A segment is a immutable file stored in S3 containing a batch of records (similar to Kafka segments).
//! Think of it as a "chunk" of a partition that gets uploaded to S3 once it's full.
//!
//! ## Segment Lifecycle
//! 1. Records accumulate in memory (SegmentWriter)
//! 2. When segment reaches ~64-256MB, it's finalized
//! 3. Segment is uploaded to S3 as an immutable file
//! 4. Metadata is stored in SQLite for querying
//! 5. Readers can fetch and read segments from S3
//!
//! ## SegmentInfo
//! Contains all metadata needed to locate and understand a segment:
//! - Which topic/partition it belongs to
//! - Offset range it covers (base_offset to end_offset)
//! - Where it's stored in S3 (bucket + key)
//! - Size and record count
//! - When it was created
//!
//! ## Compression Types
//! - **None**: No compression (faster, but larger)
//! - **LZ4**: Fast compression with good ratio (~2x faster writes, 80%+ space savings)
//! - **Zstd**: Better compression but slower (not yet implemented)
//!
//! ## Example
//! ```ignore
//! let segment_info = SegmentInfo {
//!     id: "seg_123".to_string(),
//!     topic: "clickstream".to_string(),
//!     partition_id: 0,
//!     base_offset: 1000,
//!     end_offset: 1999,
//!     record_count: 1000,
//!     size_bytes: 5_242_880,  // ~5MB
//!     s3_bucket: "streamhouse-data".to_string(),
//!     s3_key: "clickstream/0/seg_123.bin".to_string(),
//!     created_at: 1234567890,
//! };
//! ```

use serde::{Deserialize, Serialize};

/// Information about a segment stored in S3
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentInfo {
    /// Unique segment ID
    pub id: String,

    /// Topic name
    pub topic: String,

    /// Partition ID
    pub partition_id: u32,

    /// First offset in this segment
    pub base_offset: u64,

    /// Last offset in this segment
    pub end_offset: u64,

    /// Number of records in segment
    pub record_count: u32,

    /// Size in bytes
    pub size_bytes: u64,

    /// S3 bucket name
    pub s3_bucket: String,

    /// S3 object key
    pub s3_key: String,

    /// Creation timestamp
    pub created_at: i64,
}

/// Compression type for segments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Compression {
    None = 0,
    Lz4 = 1,
    Zstd = 2,
}

impl TryFrom<u16> for Compression {
    type Error = crate::Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Compression::None),
            1 => Ok(Compression::Lz4),
            2 => Ok(Compression::Zstd),
            _ => Err(crate::Error::InvalidCompression(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Helper
    // ---------------------------------------------------------------

    fn sample_segment_info() -> SegmentInfo {
        SegmentInfo {
            id: "seg_001".to_string(),
            topic: "clickstream".to_string(),
            partition_id: 0,
            base_offset: 1000,
            end_offset: 1999,
            record_count: 1000,
            size_bytes: 5_242_880,
            s3_bucket: "streamhouse-data".to_string(),
            s3_key: "clickstream/0/seg_001.bin".to_string(),
            created_at: 1_700_000_000,
        }
    }

    // ---------------------------------------------------------------
    // SegmentInfo construction and field access
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_fields() {
        let seg = sample_segment_info();
        assert_eq!(seg.id, "seg_001");
        assert_eq!(seg.topic, "clickstream");
        assert_eq!(seg.partition_id, 0);
        assert_eq!(seg.base_offset, 1000);
        assert_eq!(seg.end_offset, 1999);
        assert_eq!(seg.record_count, 1000);
        assert_eq!(seg.size_bytes, 5_242_880);
        assert_eq!(seg.s3_bucket, "streamhouse-data");
        assert_eq!(seg.s3_key, "clickstream/0/seg_001.bin");
        assert_eq!(seg.created_at, 1_700_000_000);
    }

    #[test]
    fn test_segment_info_zero_values() {
        let seg = SegmentInfo {
            id: String::new(),
            topic: String::new(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: String::new(),
            s3_key: String::new(),
            created_at: 0,
        };
        assert_eq!(seg.base_offset, 0);
        assert_eq!(seg.record_count, 0);
    }

    #[test]
    fn test_segment_info_max_values() {
        let seg = SegmentInfo {
            id: "max".to_string(),
            topic: "t".to_string(),
            partition_id: u32::MAX,
            base_offset: u64::MAX,
            end_offset: u64::MAX,
            record_count: u32::MAX,
            size_bytes: u64::MAX,
            s3_bucket: "b".to_string(),
            s3_key: "k".to_string(),
            created_at: i64::MAX,
        };
        assert_eq!(seg.partition_id, u32::MAX);
        assert_eq!(seg.base_offset, u64::MAX);
        assert_eq!(seg.size_bytes, u64::MAX);
        assert_eq!(seg.created_at, i64::MAX);
    }

    #[test]
    fn test_segment_info_negative_created_at() {
        let seg = SegmentInfo {
            id: "neg".to_string(),
            topic: "t".to_string(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: "b".to_string(),
            s3_key: "k".to_string(),
            created_at: -1,
        };
        assert_eq!(seg.created_at, -1);
    }

    // ---------------------------------------------------------------
    // SegmentInfo Clone
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_clone() {
        let seg = sample_segment_info();
        let cloned = seg.clone();
        assert_eq!(seg, cloned);
    }

    // ---------------------------------------------------------------
    // SegmentInfo PartialEq
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_eq() {
        let a = sample_segment_info();
        let b = sample_segment_info();
        assert_eq!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_id() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.id = "seg_002".to_string();
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_topic() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.topic = "other-topic".to_string();
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_offset() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.base_offset = 2000;
        assert_ne!(a, b);
    }

    // ---------------------------------------------------------------
    // SegmentInfo Serde round-trip
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_serde_roundtrip() {
        let seg = sample_segment_info();
        let json = serde_json::to_string(&seg).expect("serialize");
        let deserialized: SegmentInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(seg, deserialized);
    }

    #[test]
    fn test_segment_info_serde_json_fields() {
        let seg = sample_segment_info();
        let val: serde_json::Value = serde_json::to_value(&seg).expect("to_value");
        assert_eq!(val["id"], "seg_001");
        assert_eq!(val["topic"], "clickstream");
        assert_eq!(val["partition_id"], 0);
        assert_eq!(val["base_offset"], 1000);
        assert_eq!(val["end_offset"], 1999);
        assert_eq!(val["record_count"], 1000);
        assert_eq!(val["size_bytes"], 5_242_880);
        assert_eq!(val["s3_bucket"], "streamhouse-data");
        assert_eq!(val["s3_key"], "clickstream/0/seg_001.bin");
        assert_eq!(val["created_at"], 1_700_000_000);
    }

    #[test]
    fn test_segment_info_serde_roundtrip_max_values() {
        let seg = SegmentInfo {
            id: "max".to_string(),
            topic: "t".to_string(),
            partition_id: u32::MAX,
            base_offset: u64::MAX,
            end_offset: u64::MAX,
            record_count: u32::MAX,
            size_bytes: u64::MAX,
            s3_bucket: "b".to_string(),
            s3_key: "k".to_string(),
            created_at: i64::MAX,
        };
        let json = serde_json::to_string(&seg).expect("serialize");
        let deserialized: SegmentInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(seg, deserialized);
    }

    #[test]
    fn test_segment_info_serde_pretty_print() {
        let seg = sample_segment_info();
        let pretty = serde_json::to_string_pretty(&seg).expect("pretty");
        let deserialized: SegmentInfo = serde_json::from_str(&pretty).expect("deserialize");
        assert_eq!(seg, deserialized);
    }

    // ---------------------------------------------------------------
    // SegmentInfo Debug
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_debug() {
        let seg = sample_segment_info();
        let debug = format!("{:?}", seg);
        assert!(debug.contains("SegmentInfo"));
        assert!(debug.contains("seg_001"));
        assert!(debug.contains("clickstream"));
    }

    // ---------------------------------------------------------------
    // Compression repr values
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_none_repr() {
        assert_eq!(Compression::None as u16, 0);
    }

    #[test]
    fn test_compression_lz4_repr() {
        assert_eq!(Compression::Lz4 as u16, 1);
    }

    #[test]
    fn test_compression_zstd_repr() {
        assert_eq!(Compression::Zstd as u16, 2);
    }

    // ---------------------------------------------------------------
    // Compression TryFrom<u16> valid values
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_try_from_0() {
        let c = Compression::try_from(0u16).unwrap();
        assert_eq!(c, Compression::None);
    }

    #[test]
    fn test_compression_try_from_1() {
        let c = Compression::try_from(1u16).unwrap();
        assert_eq!(c, Compression::Lz4);
    }

    #[test]
    fn test_compression_try_from_2() {
        let c = Compression::try_from(2u16).unwrap();
        assert_eq!(c, Compression::Zstd);
    }

    // ---------------------------------------------------------------
    // Compression TryFrom<u16> invalid values
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_try_from_3_invalid() {
        let result = Compression::try_from(3u16);
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_try_from_100_invalid() {
        let result = Compression::try_from(100u16);
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_try_from_u16_max_invalid() {
        let result = Compression::try_from(u16::MAX);
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_try_from_invalid_error_type() {
        let err = Compression::try_from(42u16).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("42"));
    }

    // ---------------------------------------------------------------
    // Compression Clone / Copy / PartialEq
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_clone() {
        let c = Compression::Lz4;
        let cloned = c.clone();
        assert_eq!(c, cloned);
    }

    #[test]
    fn test_compression_copy() {
        let c = Compression::Zstd;
        let copied = c; // Copy semantics
        assert_eq!(c, copied);
    }

    #[test]
    fn test_compression_ne() {
        assert_ne!(Compression::None, Compression::Lz4);
        assert_ne!(Compression::Lz4, Compression::Zstd);
        assert_ne!(Compression::None, Compression::Zstd);
    }

    #[test]
    fn test_compression_debug() {
        assert_eq!(format!("{:?}", Compression::None), "None");
        assert_eq!(format!("{:?}", Compression::Lz4), "Lz4");
        assert_eq!(format!("{:?}", Compression::Zstd), "Zstd");
    }

    // ---------------------------------------------------------------
    // Compression round-trip through u16
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_roundtrip_all_variants() {
        for val in 0u16..=2 {
            let c = Compression::try_from(val).unwrap();
            assert_eq!(c as u16, val);
        }
    }

    // ---------------------------------------------------------------
    // SegmentInfo - additional construction edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_unicode_topic() {
        let seg = SegmentInfo {
            id: "seg_unicode".to_string(),
            topic: "topic-\u{1F680}-rocket".to_string(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: "bucket".to_string(),
            s3_key: "key".to_string(),
            created_at: 0,
        };
        assert!(seg.topic.contains('\u{1F680}'));
    }

    #[test]
    fn test_segment_info_long_s3_key() {
        let long_key = "a/".repeat(500) + "segment.bin";
        let seg = SegmentInfo {
            id: "seg_long".to_string(),
            topic: "t".to_string(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: "b".to_string(),
            s3_key: long_key.clone(),
            created_at: 0,
        };
        assert_eq!(seg.s3_key, long_key);
    }

    #[test]
    fn test_segment_info_base_offset_equals_end_offset() {
        // A segment with a single record
        let seg = SegmentInfo {
            id: "seg_single".to_string(),
            topic: "t".to_string(),
            partition_id: 0,
            base_offset: 42,
            end_offset: 42,
            record_count: 1,
            size_bytes: 100,
            s3_bucket: "b".to_string(),
            s3_key: "k".to_string(),
            created_at: 0,
        };
        assert_eq!(seg.base_offset, seg.end_offset);
        assert_eq!(seg.record_count, 1);
    }

    #[test]
    fn test_segment_info_min_created_at() {
        let seg = SegmentInfo {
            id: "seg_min".to_string(),
            topic: "t".to_string(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: "b".to_string(),
            s3_key: "k".to_string(),
            created_at: i64::MIN,
        };
        assert_eq!(seg.created_at, i64::MIN);
    }

    // ---------------------------------------------------------------
    // SegmentInfo Clone - additional
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_clone_independence() {
        let seg = sample_segment_info();
        let mut cloned = seg.clone();
        cloned.id = "modified".to_string();
        cloned.base_offset = 9999;
        // Original should be unchanged
        assert_eq!(seg.id, "seg_001");
        assert_eq!(seg.base_offset, 1000);
        assert_ne!(seg, cloned);
    }

    // ---------------------------------------------------------------
    // SegmentInfo PartialEq - additional field differences
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_ne_different_partition_id() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.partition_id = 99;
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_end_offset() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.end_offset = 5000;
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_record_count() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.record_count = 500;
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_size_bytes() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.size_bytes = 999;
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_s3_bucket() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.s3_bucket = "other-bucket".to_string();
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_s3_key() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.s3_key = "other/key.bin".to_string();
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_ne_different_created_at() {
        let a = sample_segment_info();
        let mut b = sample_segment_info();
        b.created_at = 0;
        assert_ne!(a, b);
    }

    #[test]
    fn test_segment_info_eq_reflexive() {
        let seg = sample_segment_info();
        assert_eq!(seg, seg);
    }

    #[test]
    fn test_segment_info_eq_symmetric() {
        let a = sample_segment_info();
        let b = sample_segment_info();
        assert_eq!(a, b);
        assert_eq!(b, a);
    }

    // ---------------------------------------------------------------
    // SegmentInfo Serde - additional edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_serde_roundtrip_min_values() {
        let seg = SegmentInfo {
            id: String::new(),
            topic: String::new(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: String::new(),
            s3_key: String::new(),
            created_at: i64::MIN,
        };
        let json = serde_json::to_string(&seg).expect("serialize");
        let deserialized: SegmentInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(seg, deserialized);
    }

    #[test]
    fn test_segment_info_serde_roundtrip_negative_created_at() {
        let seg = SegmentInfo {
            id: "neg".to_string(),
            topic: "t".to_string(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: "b".to_string(),
            s3_key: "k".to_string(),
            created_at: -1_000_000,
        };
        let json = serde_json::to_string(&seg).expect("serialize");
        let deserialized: SegmentInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(seg, deserialized);
    }

    #[test]
    fn test_segment_info_serde_vec_roundtrip() {
        let segments = vec![sample_segment_info(), {
            let mut s = sample_segment_info();
            s.id = "seg_002".to_string();
            s.base_offset = 2000;
            s.end_offset = 2999;
            s
        }];
        let json = serde_json::to_string(&segments).expect("serialize vec");
        let deserialized: Vec<SegmentInfo> = serde_json::from_str(&json).expect("deserialize vec");
        assert_eq!(segments, deserialized);
    }

    #[test]
    fn test_segment_info_serde_unicode_topic() {
        let seg = SegmentInfo {
            id: "seg_u".to_string(),
            topic: "\u{00E9}v\u{00E9}nements".to_string(),
            partition_id: 0,
            base_offset: 0,
            end_offset: 0,
            record_count: 0,
            size_bytes: 0,
            s3_bucket: "b".to_string(),
            s3_key: "k".to_string(),
            created_at: 0,
        };
        let json = serde_json::to_string(&seg).expect("serialize");
        let deserialized: SegmentInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(seg, deserialized);
    }

    #[test]
    fn test_segment_info_deserialize_rejects_missing_field() {
        let json = r#"{"id":"seg","topic":"t","partition_id":0}"#;
        let result = serde_json::from_str::<SegmentInfo>(json);
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // Compression TryFrom<u16> - additional invalid values
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_try_from_4_invalid() {
        assert!(Compression::try_from(4u16).is_err());
    }

    #[test]
    fn test_compression_try_from_255_invalid() {
        assert!(Compression::try_from(255u16).is_err());
    }

    #[test]
    fn test_compression_try_from_1000_invalid() {
        assert!(Compression::try_from(1000u16).is_err());
    }

    #[test]
    fn test_compression_try_from_all_invalid_range() {
        // All values from 3 to 10 should be invalid
        for val in 3u16..=10 {
            assert!(
                Compression::try_from(val).is_err(),
                "Expected error for value {}",
                val
            );
        }
    }

    #[test]
    fn test_compression_try_from_invalid_error_message_contains_value() {
        for bad_val in [3u16, 50, 999, u16::MAX] {
            let err = Compression::try_from(bad_val).unwrap_err();
            let msg = format!("{}", err);
            assert!(
                msg.contains(&bad_val.to_string()),
                "Error message '{}' should contain '{}'",
                msg,
                bad_val
            );
        }
    }

    // ---------------------------------------------------------------
    // Compression equality exhaustive
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_eq_all_same_pairs() {
        assert_eq!(Compression::None, Compression::None);
        assert_eq!(Compression::Lz4, Compression::Lz4);
        assert_eq!(Compression::Zstd, Compression::Zstd);
    }

    #[test]
    fn test_compression_ne_all_different_pairs() {
        let variants = [Compression::None, Compression::Lz4, Compression::Zstd];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // SegmentInfo Debug - additional
    // ---------------------------------------------------------------

    #[test]
    fn test_segment_info_debug_contains_all_fields() {
        let seg = sample_segment_info();
        let debug = format!("{:?}", seg);
        assert!(debug.contains("seg_001"));
        assert!(debug.contains("clickstream"));
        assert!(debug.contains("1000"));
        assert!(debug.contains("1999"));
        assert!(debug.contains("5242880"));
        assert!(debug.contains("streamhouse-data"));
    }

    // ---------------------------------------------------------------
    // Compression round-trip u16 -> Compression -> u16
    // ---------------------------------------------------------------

    #[test]
    fn test_compression_roundtrip_none() {
        let val = 0u16;
        let c = Compression::try_from(val).unwrap();
        assert_eq!(c as u16, val);
        assert_eq!(c, Compression::None);
    }

    #[test]
    fn test_compression_roundtrip_lz4() {
        let val = 1u16;
        let c = Compression::try_from(val).unwrap();
        assert_eq!(c as u16, val);
        assert_eq!(c, Compression::Lz4);
    }

    #[test]
    fn test_compression_roundtrip_zstd() {
        let val = 2u16;
        let c = Compression::try_from(val).unwrap();
        assert_eq!(c as u16, val);
        assert_eq!(c, Compression::Zstd);
    }
}
