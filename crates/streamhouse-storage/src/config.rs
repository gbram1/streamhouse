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
