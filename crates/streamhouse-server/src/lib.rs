//! StreamHouse gRPC Server
//!
//! Provides gRPC API for:
//! - Admin operations (create/list/delete topics)
//! - Producer operations (produce records)
//! - Consumer operations (consume records, commit offsets)

pub mod compaction;
pub mod materialized_view_maintenance;
pub mod services;

// Include generated protobuf code
pub mod pb {
    tonic::include_proto!("streamhouse");
}

pub use compaction::{CompactionConfig, CompactionTask};
pub use materialized_view_maintenance::{MaintenanceConfig, MaterializedViewMaintenance};
pub use services::StreamHouseService;
