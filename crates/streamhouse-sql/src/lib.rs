//! StreamHouse SQL Query Engine
//!
//! Provides SQL-based message querying for StreamHouse topics.
//! This is a simplified SQL engine focused on point-in-time queries,
//! not continuous stream processing.
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
//! -- Show topics
//! SHOW TOPICS;
//!
//! -- Describe topic
//! DESCRIBE orders;
//! ```
//!
//! ## Limitations
//!
//! - No GROUP BY (no aggregations across messages)
//! - No JOINs
//! - No INSERT/UPDATE/DELETE (read-only)
//! - No subqueries
//! - Max 10,000 rows per query

mod error;
mod executor;
mod parser;
mod types;

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
