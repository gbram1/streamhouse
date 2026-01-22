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
