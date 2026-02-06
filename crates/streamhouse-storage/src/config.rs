//! Storage Configuration
//!
//! This module defines configuration for the write path.
//!
//! ## WriteConfig
//!
//! Controls how segments are created, rolled, and uploaded to S3:
//!
//! - **segment_max_size**: Roll segment when it reaches this size (default: 64MB)
//! - **segment_max_age_ms**: Roll segment after this time even if not full (default: 10 min)
//! - **s3_bucket**: S3 bucket name for storing segments
//! - **s3_region**: AWS region or MinIO region
//! - **s3_endpoint**: Optional custom S3 endpoint (for MinIO/localstack)
//! - **block_size_target**: Target size for compressed blocks within segments (default: 1MB)
//! - **s3_upload_retries**: Number of retries for S3 uploads with exponential backoff (default: 3)
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::WriteConfig;
//!
//! // Production config (AWS S3)
//! let config = WriteConfig {
//!     s3_bucket: "my-streamhouse-bucket".to_string(),
//!     s3_region: "us-east-1".to_string(),
//!     s3_endpoint: None,
//!     ..Default::default()
//! };
//!
//! // Development config (MinIO)
//! let config = WriteConfig {
//!     s3_bucket: "streamhouse".to_string(),
//!     s3_region: "us-east-1".to_string(),
//!     s3_endpoint: Some("http://localhost:9000".to_string()),
//!     segment_max_size: 1024 * 1024, // 1MB for faster testing
//!     ..Default::default()
//! };
//! ```

use crate::throttle::ThrottleConfig;
use crate::wal::WALConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteConfig {
    /// Maximum segment size in bytes before rolling (default: 64MB)
    #[serde(default = "default_segment_max_size")]
    pub segment_max_size: usize,

    /// Maximum segment age in milliseconds before rolling (default: 10 minutes)
    #[serde(default = "default_segment_max_age_ms")]
    pub segment_max_age_ms: u64,

    /// S3 bucket name
    pub s3_bucket: String,

    /// S3 region
    pub s3_region: String,

    /// Optional S3 endpoint (for MinIO/localstack)
    pub s3_endpoint: Option<String>,

    /// Block size target for compression in bytes (default: 1MB)
    #[serde(default = "default_block_size")]
    pub block_size_target: usize,

    /// Number of S3 upload retries with exponential backoff (default: 3)
    #[serde(default = "default_retries")]
    pub s3_upload_retries: u32,

    /// WAL configuration (optional - if None, WAL is disabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wal_config: Option<WALConfig>,

    /// Throttle configuration (optional - if None, throttling is disabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle_config: Option<ThrottleConfig>,

    // === Phase 8.4 Performance Optimizations ===
    /// Minimum size for multipart upload (default: 8MB)
    /// Files smaller than this use simple PUT
    #[serde(default = "default_multipart_threshold")]
    pub multipart_threshold: usize,

    /// Part size for multipart uploads (default: 8MB)
    /// Must be >= 5MB per S3 requirements
    #[serde(default = "default_multipart_part_size")]
    pub multipart_part_size: usize,

    /// Maximum concurrent upload parts (default: 4)
    #[serde(default = "default_parallel_upload_parts")]
    pub parallel_upload_parts: usize,
}

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            segment_max_size: default_segment_max_size(),
            segment_max_age_ms: default_segment_max_age_ms(),
            s3_bucket: "streamhouse".to_string(),
            s3_region: "us-east-1".to_string(),
            s3_endpoint: None,
            block_size_target: default_block_size(),
            s3_upload_retries: default_retries(),
            wal_config: None,      // WAL disabled by default
            throttle_config: None, // Throttling disabled by default
            multipart_threshold: default_multipart_threshold(),
            multipart_part_size: default_multipart_part_size(),
            parallel_upload_parts: default_parallel_upload_parts(),
        }
    }
}

fn default_segment_max_size() -> usize {
    64 * 1024 * 1024 // 64MB
}

fn default_segment_max_age_ms() -> u64 {
    10 * 60 * 1000 // 10 minutes
}

fn default_block_size() -> usize {
    1024 * 1024 // 1MB
}

fn default_retries() -> u32 {
    3
}

fn default_multipart_threshold() -> usize {
    8 * 1024 * 1024 // 8MB - use multipart for segments larger than this
}

fn default_multipart_part_size() -> usize {
    8 * 1024 * 1024 // 8MB per part (S3 minimum is 5MB)
}

fn default_parallel_upload_parts() -> usize {
    4 // Upload 4 parts concurrently
}
