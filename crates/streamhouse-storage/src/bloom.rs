//! Bloom Filter for Segment Keys (Phase 8.4c)
//!
//! Bloom filters provide fast probabilistic key existence checks, avoiding
//! unnecessary S3 reads when a key definitely doesn't exist in a segment.
//!
//! ## Benefits
//!
//! - **Fast negative lookups**: O(1) to determine key is NOT in segment
//! - **Memory efficient**: ~10 bits per key for 1% false positive rate
//! - **Disk efficient**: Stored alongside segment index metadata
//!
//! ## False Positive Rate
//!
//! The bloom filter has a configurable false positive rate (default 1%).
//! This means:
//! - If `might_contain(key)` returns `false`, the key is definitely NOT in the segment
//! - If `might_contain(key)` returns `true`, the key MIGHT be in the segment (1% false positive)
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::bloom::SegmentBloomFilter;
//!
//! // Create a bloom filter during segment write
//! let mut bloom = SegmentBloomFilter::new(10000); // Expected 10k records
//!
//! // Add keys as records are written
//! bloom.add(b"user:123");
//! bloom.add(b"user:456");
//!
//! // Serialize for storage
//! let bytes = bloom.to_bytes();
//!
//! // Later, deserialize and check
//! let bloom = SegmentBloomFilter::from_bytes(&bytes)?;
//! if bloom.might_contain(b"user:789") {
//!     // Key might be in segment, need to read from S3
//! } else {
//!     // Key definitely NOT in segment, skip S3 read
//! }
//! ```
//!
//! ## Storage
//!
//! Bloom filter bytes are stored in segment metadata or as a sidecar file:
//! - `segments/{topic}/{partition}/{segment_id}.bloom`

use crate::error::{Error, Result};
use bloomfilter::Bloom;
use serde::{Deserialize, Serialize};

/// Default false positive rate (1%)
const DEFAULT_FALSE_POSITIVE_RATE: f64 = 0.01;

/// Bloom filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomFilterConfig {
    /// Expected number of items to store
    pub expected_items: usize,

    /// Target false positive rate (default: 0.01 = 1%)
    #[serde(default = "default_fp_rate")]
    pub false_positive_rate: f64,
}

fn default_fp_rate() -> f64 {
    DEFAULT_FALSE_POSITIVE_RATE
}

impl Default for BloomFilterConfig {
    fn default() -> Self {
        Self {
            expected_items: 10000,
            false_positive_rate: DEFAULT_FALSE_POSITIVE_RATE,
        }
    }
}

/// Segment bloom filter for fast key existence checks
pub struct SegmentBloomFilter {
    bloom: Bloom<[u8]>,
    config: BloomFilterConfig,
    item_count: usize,
}

impl SegmentBloomFilter {
    /// Create a new bloom filter with expected item count
    pub fn new(expected_items: usize) -> Self {
        Self::with_config(BloomFilterConfig {
            expected_items,
            ..Default::default()
        })
    }

    /// Create a new bloom filter with custom configuration
    pub fn with_config(config: BloomFilterConfig) -> Self {
        let bloom = Bloom::new_for_fp_rate(config.expected_items, config.false_positive_rate);

        tracing::debug!(
            expected_items = config.expected_items,
            fp_rate = config.false_positive_rate,
            bitmap_bits = bloom.number_of_bits(),
            num_hashes = bloom.number_of_hash_functions(),
            "Created segment bloom filter (Phase 8.4c)"
        );

        Self {
            bloom,
            config,
            item_count: 0,
        }
    }

    /// Add a key to the bloom filter
    pub fn add(&mut self, key: &[u8]) {
        self.bloom.set(key);
        self.item_count += 1;
    }

    /// Check if a key might be in the segment
    ///
    /// Returns:
    /// - `false`: Key is definitely NOT in the segment
    /// - `true`: Key MIGHT be in the segment (false positive possible)
    pub fn might_contain(&self, key: &[u8]) -> bool {
        self.bloom.check(key)
    }

    /// Get the number of items added to the filter
    pub fn item_count(&self) -> usize {
        self.item_count
    }

    /// Get the size of the bloom filter in bits
    pub fn size_bits(&self) -> u64 {
        self.bloom.number_of_bits()
    }

    /// Get the size of the bloom filter in bytes
    pub fn size_bytes(&self) -> usize {
        (self.bloom.number_of_bits() as usize + 7) / 8
    }

    /// Get the number of hash functions used
    pub fn num_hashes(&self) -> u32 {
        self.bloom.number_of_hash_functions()
    }

    /// Serialize the bloom filter to bytes for storage
    pub fn to_bytes(&self) -> Vec<u8> {
        // Format: [version(1)][config_len(4)][config_json][item_count(8)][num_bits(8)][num_hashes(4)][sip_keys(32)][bloom_bytes]
        let config_json = serde_json::to_vec(&self.config).unwrap_or_default();
        let bloom_bytes = self.bloom.bitmap();
        let sip_keys = self.bloom.sip_keys();
        let num_bits = self.bloom.number_of_bits();
        let num_hashes = self.bloom.number_of_hash_functions();

        let mut result = Vec::with_capacity(1 + 4 + config_json.len() + 8 + 8 + 4 + 32 + bloom_bytes.len());

        // Version byte (v2 with SIP keys)
        result.push(2);

        // Config length and data
        result.extend_from_slice(&(config_json.len() as u32).to_le_bytes());
        result.extend_from_slice(&config_json);

        // Item count
        result.extend_from_slice(&(self.item_count as u64).to_le_bytes());

        // Bloom filter params: num_bits, num_hashes
        result.extend_from_slice(&num_bits.to_le_bytes());
        result.extend_from_slice(&num_hashes.to_le_bytes());

        // SIP keys (2 pairs of u64)
        result.extend_from_slice(&sip_keys[0].0.to_le_bytes());
        result.extend_from_slice(&sip_keys[0].1.to_le_bytes());
        result.extend_from_slice(&sip_keys[1].0.to_le_bytes());
        result.extend_from_slice(&sip_keys[1].1.to_le_bytes());

        // Bloom filter bitmap
        result.extend_from_slice(&bloom_bytes);

        result
    }

    /// Deserialize a bloom filter from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(Error::SegmentError("Empty bloom filter data".to_string()));
        }

        let mut cursor = 0;

        // Version
        let version = data[cursor];
        cursor += 1;

        if version != 2 {
            return Err(Error::SegmentError(format!(
                "Unsupported bloom filter version: {} (expected 2)",
                version
            )));
        }

        // Config length
        if cursor + 4 > data.len() {
            return Err(Error::SegmentError(
                "Invalid bloom filter: truncated config length".to_string(),
            ));
        }
        let config_len =
            u32::from_le_bytes([data[cursor], data[cursor + 1], data[cursor + 2], data[cursor + 3]])
                as usize;
        cursor += 4;

        // Config data
        if cursor + config_len > data.len() {
            return Err(Error::SegmentError(
                "Invalid bloom filter: truncated config data".to_string(),
            ));
        }
        let config: BloomFilterConfig =
            serde_json::from_slice(&data[cursor..cursor + config_len]).map_err(|e| {
                Error::SegmentError(format!("Invalid bloom filter config: {}", e))
            })?;
        cursor += config_len;

        // Item count
        if cursor + 8 > data.len() {
            return Err(Error::SegmentError(
                "Invalid bloom filter: truncated item count".to_string(),
            ));
        }
        let item_count = u64::from_le_bytes([
            data[cursor],
            data[cursor + 1],
            data[cursor + 2],
            data[cursor + 3],
            data[cursor + 4],
            data[cursor + 5],
            data[cursor + 6],
            data[cursor + 7],
        ]) as usize;
        cursor += 8;

        // Num bits
        if cursor + 8 > data.len() {
            return Err(Error::SegmentError(
                "Invalid bloom filter: truncated num_bits".to_string(),
            ));
        }
        let num_bits = u64::from_le_bytes([
            data[cursor],
            data[cursor + 1],
            data[cursor + 2],
            data[cursor + 3],
            data[cursor + 4],
            data[cursor + 5],
            data[cursor + 6],
            data[cursor + 7],
        ]);
        cursor += 8;

        // Num hashes
        if cursor + 4 > data.len() {
            return Err(Error::SegmentError(
                "Invalid bloom filter: truncated num_hashes".to_string(),
            ));
        }
        let num_hashes =
            u32::from_le_bytes([data[cursor], data[cursor + 1], data[cursor + 2], data[cursor + 3]]);
        cursor += 4;

        // SIP keys (32 bytes = 4 x u64)
        if cursor + 32 > data.len() {
            return Err(Error::SegmentError(
                "Invalid bloom filter: truncated SIP keys".to_string(),
            ));
        }
        let sip_key_0_0 = u64::from_le_bytes([
            data[cursor], data[cursor + 1], data[cursor + 2], data[cursor + 3],
            data[cursor + 4], data[cursor + 5], data[cursor + 6], data[cursor + 7],
        ]);
        cursor += 8;
        let sip_key_0_1 = u64::from_le_bytes([
            data[cursor], data[cursor + 1], data[cursor + 2], data[cursor + 3],
            data[cursor + 4], data[cursor + 5], data[cursor + 6], data[cursor + 7],
        ]);
        cursor += 8;
        let sip_key_1_0 = u64::from_le_bytes([
            data[cursor], data[cursor + 1], data[cursor + 2], data[cursor + 3],
            data[cursor + 4], data[cursor + 5], data[cursor + 6], data[cursor + 7],
        ]);
        cursor += 8;
        let sip_key_1_1 = u64::from_le_bytes([
            data[cursor], data[cursor + 1], data[cursor + 2], data[cursor + 3],
            data[cursor + 4], data[cursor + 5], data[cursor + 6], data[cursor + 7],
        ]);
        cursor += 8;

        let sip_keys = [(sip_key_0_0, sip_key_0_1), (sip_key_1_0, sip_key_1_1)];

        // Bloom filter bitmap
        let bloom_bytes = &data[cursor..];

        // Recreate bloom filter with exact parameters
        let bloom = Bloom::from_existing(bloom_bytes, num_bits, num_hashes, sip_keys);

        Ok(Self {
            bloom,
            config,
            item_count,
        })
    }

    /// Clear all items from the bloom filter
    pub fn clear(&mut self) {
        self.bloom.clear();
        self.item_count = 0;
    }
}

/// Bloom filter statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct BloomFilterStats {
    /// Number of bloom filter lookups
    pub lookups: u64,

    /// Number of true negatives (definitely not in segment)
    pub true_negatives: u64,

    /// Number of potential positives (might be in segment)
    pub potential_positives: u64,

    /// Estimated false positive count (potential positives that were actually negative)
    pub estimated_false_positives: u64,
}

impl BloomFilterStats {
    /// Record a lookup result
    pub fn record_lookup(&mut self, might_contain: bool, actually_contains: bool) {
        self.lookups += 1;

        if might_contain {
            self.potential_positives += 1;
            if !actually_contains {
                self.estimated_false_positives += 1;
            }
        } else {
            self.true_negatives += 1;
        }
    }

    /// Get the observed false positive rate
    pub fn observed_fp_rate(&self) -> f64 {
        if self.potential_positives == 0 {
            0.0
        } else {
            self.estimated_false_positives as f64 / self.potential_positives as f64
        }
    }

    /// Get the skip rate (percentage of lookups that avoided S3 reads)
    pub fn skip_rate(&self) -> f64 {
        if self.lookups == 0 {
            0.0
        } else {
            self.true_negatives as f64 / self.lookups as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter_basic() {
        let mut bloom = SegmentBloomFilter::new(100);

        // Add some keys
        bloom.add(b"key1");
        bloom.add(b"key2");
        bloom.add(b"key3");

        assert_eq!(bloom.item_count(), 3);

        // Keys we added should be found
        assert!(bloom.might_contain(b"key1"));
        assert!(bloom.might_contain(b"key2"));
        assert!(bloom.might_contain(b"key3"));

        // Keys we didn't add should (very likely) not be found
        // Note: There's a small chance of false positives
        let mut found_false_positives = 0;
        for i in 100..200 {
            if bloom.might_contain(format!("nonexistent_{}", i).as_bytes()) {
                found_false_positives += 1;
            }
        }

        // With 1% false positive rate and 100 checks, we expect ~1 false positive
        assert!(
            found_false_positives < 10,
            "Too many false positives: {}",
            found_false_positives
        );
    }

    #[test]
    fn test_bloom_filter_serialization() {
        let mut bloom = SegmentBloomFilter::new(100);

        bloom.add(b"key1");
        bloom.add(b"key2");
        bloom.add(b"key3");

        // Serialize
        let bytes = bloom.to_bytes();
        assert!(!bytes.is_empty());

        // Deserialize
        let restored = SegmentBloomFilter::from_bytes(&bytes).unwrap();

        assert_eq!(restored.item_count(), 3);
        assert!(restored.might_contain(b"key1"));
        assert!(restored.might_contain(b"key2"));
        assert!(restored.might_contain(b"key3"));
    }

    #[test]
    fn test_bloom_filter_custom_config() {
        let config = BloomFilterConfig {
            expected_items: 1000,
            false_positive_rate: 0.001, // 0.1% FP rate
        };

        let bloom = SegmentBloomFilter::with_config(config);

        // More bits needed for lower FP rate
        assert!(bloom.size_bits() > 0);
        assert!(bloom.num_hashes() > 0);
    }

    #[test]
    fn test_bloom_filter_stats() {
        let mut stats = BloomFilterStats::default();

        // Simulate lookups
        stats.record_lookup(false, false); // True negative
        stats.record_lookup(false, false); // True negative
        stats.record_lookup(true, true); // True positive
        stats.record_lookup(true, false); // False positive

        assert_eq!(stats.lookups, 4);
        assert_eq!(stats.true_negatives, 2);
        assert_eq!(stats.potential_positives, 2);
        assert_eq!(stats.estimated_false_positives, 1);
        assert_eq!(stats.skip_rate(), 0.5);
        assert_eq!(stats.observed_fp_rate(), 0.5);
    }

    #[test]
    fn test_bloom_filter_clear() {
        let mut bloom = SegmentBloomFilter::new(100);

        bloom.add(b"key1");
        assert!(bloom.might_contain(b"key1"));
        assert_eq!(bloom.item_count(), 1);

        bloom.clear();
        assert_eq!(bloom.item_count(), 0);
        // After clear, key might still match due to how bloom filters work,
        // but item_count is reset
    }

    #[test]
    fn test_bloom_filter_empty_data_error() {
        let result = SegmentBloomFilter::from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_bloom_filter_invalid_version_error() {
        let result = SegmentBloomFilter::from_bytes(&[99]); // Invalid version
        assert!(result.is_err());
    }
}
