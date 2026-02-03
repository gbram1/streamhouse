//! Kafka API handlers
//!
//! Each handler processes a specific Kafka API request and returns the response.

mod api_versions;
mod consumer_groups;
mod fetch;
mod metadata;
mod offsets;
mod produce;
mod topics;

pub use api_versions::handle_api_versions;
pub use consumer_groups::{
    handle_describe_groups, handle_find_coordinator, handle_heartbeat, handle_join_group,
    handle_leave_group, handle_list_groups, handle_sync_group,
};
pub use fetch::handle_fetch;
pub use metadata::handle_metadata;
pub use offsets::{handle_list_offsets, handle_offset_commit, handle_offset_fetch};
pub use produce::handle_produce;
pub use topics::{handle_create_topics, handle_delete_topics};
