//! StreamHouse SQL Query Engine
//!
//! Provides SQL-based message querying for StreamHouse topics.
//! This engine supports both point-in-time queries and streaming window aggregations.
//!
//! ## Performance
//!
//! Uses Apache Arrow and DataFusion for high-performance query execution:
//! - Columnar data format for cache-efficient processing
//! - Vectorized operations for filters and aggregations
//! - SIMD-optimized computations where available
//!
//! ## Supported SQL
//!
//! ```sql
//! -- Basic select
//! SELECT * FROM orders LIMIT 100;
//!
//! -- Filter by key
//! SELECT * FROM orders WHERE key = 'customer-123';
//!
//! -- Filter by offset range
//! SELECT * FROM orders
//! WHERE partition = 0
//!   AND offset >= 1000 AND offset < 2000;
//!
//! -- Filter by timestamp
//! SELECT * FROM orders
//! WHERE timestamp >= '2026-01-15T00:00:00Z';
//!
//! -- JSON field extraction
//! SELECT
//!     key,
//!     offset,
//!     json_extract(value, '$.customer_id') as customer_id
//! FROM orders
//! WHERE json_extract(value, '$.amount') > 100
//! LIMIT 50;
//!
//! -- Count messages
//! SELECT COUNT(*) FROM orders WHERE partition = 0;
//!
//! -- Window aggregations (tumbling windows)
//! SELECT
//!     window_start, window_end,
//!     COUNT(*) as event_count,
//!     SUM(json_extract(value, '$.amount')) as total
//! FROM orders
//! GROUP BY TUMBLE(timestamp, INTERVAL '1' HOUR);
//!
//! -- Show topics
//! SHOW TOPICS;
//!
//! -- Describe topic
//! DESCRIBE orders;
//! ```
//!
//! ## Limitations
//!
//! - Max 10,000 rows per query (configurable)
//! - 30 second default timeout
//! - No INSERT/UPDATE/DELETE (read-only)
//! - No subqueries

mod arrow_executor;
pub mod cdc;
mod error;
mod executor;
mod parser;
pub mod streaming;
mod types;
pub mod watermark;

pub mod checkpoint_manager;
pub mod operators;
pub mod rocksdb_state;
pub mod scheduler;

pub use arrow_executor::ArrowExecutor;
pub use error::SqlError;
pub use executor::SqlExecutor;
pub use parser::parse_query;
pub use types::*;

// Re-export window aggregation functions for materialized view maintenance
pub use executor::{
    compute_aggregation, extract_group_key, group_into_hop_windows, group_into_session_windows,
    group_into_tumble_windows, WindowGroups, WindowKey,
};

/// Result type for SQL operations
pub type Result<T> = std::result::Result<T, SqlError>;
